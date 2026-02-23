//! 单线程下载实现：下载器方法及执行逻辑。

use std::path::Path;

use futures_util::StreamExt;
use thiserror::Error;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

use crate::internal::auth::structs::webdav_auth::WebdavAuth;
use crate::internal::remote_file::downloader::structs::hook_adapters::{
    AfterCompleteHookAdapter, BeforeStartHookAdapter, OnChunkHookAdapter,
    OnProgressHookAdapter,
};
use crate::internal::remote_file::downloader::structs::{
    DownloadConfig, DownloadHooksContainer, DownloadProgress,
    DownloadResult, RemoteFileDownloader,
};
use crate::internal::remote_file::structs::remote_file::RemoteFile;
use crate::internal::remote_file::structs::remote_file_data::RemoteFileData;
use crate::internal::states::unlock_reactive::UnlockReactiveProperty;
use crate::remote_file::download::{DownloadHook, HookAbort};

#[derive(Debug, Error)]
pub enum DownloadError {
    #[error("HTTP 请求失败: {0}")]
    Request(#[from] reqwest::Error),

    #[error("创建文件失败: {0}")]
    CreateFile(std::io::Error),

    #[error("写入文件失败: {0}")]
    WriteFile(tokio::io::Error),

    #[error("仅支持文件下载，当前为目录")]
    IsDir,

    #[error("未设置保存路径且未开启 output_bytes")]
    NoDestination,

    #[error("下载被取消")]
    Cancelled,

    /// 多线程下载尚未实现。
    #[error("多线程下载尚未实现")]
    MultiThreadUnimplemented,

    /// 钩子在 before_start 中返回错误，中止下载。
    #[error("{0}")]
    HookAbort(#[from] HookAbort),
}

impl RemoteFileDownloader {
    /// 设置保存路径；不调用则不会写入本地文件。传空路径表示不保存到文件。
    pub fn save_to(mut self, path: impl AsRef<Path>) -> Self {
        let p = path.as_ref();
        self.config.save_path = if p.as_os_str().is_empty() {
            None
        } else {
            Some(p.to_path_buf())
        };
        self
    }

    /// 设置为输出字节数组，默认不输出。
    pub fn output_bytes(mut self) -> Self {
        self.config.is_output_bytes = true;
        self
    }

    /// 注册「开始前」钩子；闭包返回 `Err(HookAbort)` 会中止本次下载。
    pub fn with_before_start_hook<F, Fut>(mut self, f: F) -> Self
    where
        F: FnMut() -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<(), HookAbort>>
            + Send
            + 'static,
    {
        self.hooks.add(BeforeStartHookAdapter(f));
        self
    }

    /// 注册「进度」钩子；参数为已下载字节数、总大小（可能未知为 `None`）。
    pub fn with_on_progress_hook<F>(mut self, f: F) -> Self
    where
        F: FnMut(u64, Option<u64>) + Send + Sync + 'static,
    {
        self.hooks.add(OnProgressHookAdapter(f));
        self
    }

    /// 注册「完成后」钩子；下载成功结束后调用。
    pub fn with_after_complete_hook<F, Fut>(mut self, f: F) -> Self
    where
        F: FnMut() -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        self.hooks.add(AfterCompleteHookAdapter(f));
        self
    }

    /// 添加完整钩子，在下载各阶段插入逻辑（通道二：钩子内可手动计算进度）。
    /// 可链式多次调用以注册多个钩子；各钩子的 before_start / on_chunk / on_progress / after_complete 会按注册顺序执行。
    pub fn with_hook(mut self, hook: impl DownloadHook + 'static) -> Self {
        self.hooks.add(hook);
        self
    }

    /// 内置的下载进度状态（通道一）。返回可共享的句柄，直接 `.watch()` 后 `changed().await` 监听进度。
    pub fn progress(&self) -> UnlockReactiveProperty<DownloadProgress> {
        self.progress_state.clone()
    }

    /// 执行下载。
    /// 根据是否开启 output_bytes 与是否多线程（concurrent_chunks > 1）分支：单线程返回 Saved 或 Bytes；多线程暂返回错误，后续实现后返回 BytesSegments。
    pub async fn send(self) -> Result<DownloadResult, DownloadError> {
        // 判断是否多线程下载，如果concurrent_chunks > 1，则返回错误，后续实现后返回 BytesSegments。
        let is_multi = self
            .config
            .concurrent_chunks
            .map(|n| n > 1)
            .unwrap_or(false);

        if is_multi {
            return Err(DownloadError::MultiThreadUnimplemented);
        }

        run_single_thread_download(
            self.client,
            self.file_data,
            self.config,
            self.hooks,
            self.progress_state,
        )
        .await
    }
}

impl RemoteFile {
    /// 创建专属于本远程文件的下载器；可链式配置后调用 [`RemoteFileDownloader::send`] 执行下载。
    pub fn build_downloader(
        &self,
        auth: &WebdavAuth,
    ) -> RemoteFileDownloader {
        build_downloader(self, auth)
    }
}

/// 由远程文件创建其专属下载器（供 [`RemoteFile::build_downloader`] 使用）。
pub fn build_downloader(
    remote_file: &RemoteFile,
    auth: &WebdavAuth,
) -> RemoteFileDownloader {
    let progress_state = UnlockReactiveProperty::new(DownloadProgress {
        bytes_done: 0,
        total: remote_file.data.size,
    });

    RemoteFileDownloader {
        client: auth.client.clone(),
        file_data: remote_file.data.clone(),
        config: DownloadConfig::default(),
        hooks: Default::default(),
        progress_state,
    }
}

async fn run_single_thread_download(
    client: reqwest::Client,
    file_data: RemoteFileData,
    config: DownloadConfig,
    mut hooks: DownloadHooksContainer,
    progress: UnlockReactiveProperty<DownloadProgress>,
) -> Result<DownloadResult, DownloadError> {
    if file_data.is_dir {
        return Err(DownloadError::IsDir);
    }

    let save_path = config.save_path.as_ref();
    let output_bytes = config.is_output_bytes;
    if save_path.is_none() && !output_bytes {
        return Err(DownloadError::NoDestination);
    }

    hooks.run_before_start().await?;

    let total = file_data.size;
    let _ = progress.update(DownloadProgress { bytes_done: 0, total });

    let resp = client.get(&file_data.absolute_path).send().await?;
    let mut stream = resp.bytes_stream();
    let mut bytes_done: u64 = 0;
    let mut out_bytes: Vec<u8> = Vec::new();
    let mut file = match (save_path, output_bytes) {
        (Some(p), false) => Some(
            File::create(p).await.map_err(DownloadError::CreateFile)?,
        ),
        _ => None,
    };

    while let Some(chunk_result) = stream.next().await {
        if hooks.cancel_requested() {
            return Err(DownloadError::Cancelled);
        }

        let chunk = chunk_result?;
        let len = chunk.len() as u64;
        bytes_done += len;

        if let Some(f) = file.as_mut() {
            f.write_all(&chunk).await.map_err(DownloadError::WriteFile)?;
        }
        if output_bytes {
            out_bytes.extend_from_slice(&chunk);
        }

        hooks.run_on_chunk(&chunk);
        hooks.run_on_progress(bytes_done, total);

        let _ = progress.update(DownloadProgress { bytes_done, total });
    }

    if let Some(mut f) = file {
        f.flush().await.map_err(DownloadError::WriteFile)?;
    }

    hooks.run_after_complete().await;

    if output_bytes {
        Ok(DownloadResult::Bytes(out_bytes))
    } else {
        Ok(DownloadResult::Saved)
    }
}
