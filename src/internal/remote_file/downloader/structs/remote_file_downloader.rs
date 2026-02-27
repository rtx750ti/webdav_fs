//! 远程文件下载器
//!
//! 本模块实现了 WebDAV 远程文件的下载功能，支持单线程和分片并发两种下载模式。
//!
//! ## 功能特性
//!
//! - **单线程下载**：适用于小文件或不支持 Range 请求的服务器，整文件流式下载
//! - **分片并发下载**：适用于大文件，通过 HTTP Range 请求将文件分段并发下载，提升速度
//! - **断点续传**：分片下载支持从已下载位置继续（需文件已存在且部分完成）
//! - **响应式进度**：通过 `progress()` 获取可监听的下载进度状态
//! - **钩子机制**：支持在下载各阶段插入自定义逻辑（开始前、进度更新、完成后等）
//! - **灵活输出**：可保存到本地文件、输出字节数组，或两者同时进行
//!
//! ## 使用示例
//!
//! ```rust,no_run
//! # use webdav_fs::remote_file::RemoteFile;
//! # use webdav_fs::auth::WebdavAuth;
//! # async fn example(remote_file: RemoteFile, auth: WebdavAuth) -> Result<(), Box<dyn std::error::Error>> {
//! // 单线程下载到本地
//! let result = remote_file
//!     .build_downloader(&auth)
//!     .save_to("local_file.txt")
//!     .send()
//!     .await?;
//!
//! // 分片并发下载（最多 4 个分片同时请求）
//! let result = remote_file
//!     .build_downloader(&auth)
//!     .save_to("large_file.zip")
//!     .max_concurrent_chunks(4)
//!     .send()
//!     .await?;
//!
//! // 下载到内存（返回字节数组）
//! let result = remote_file
//!     .build_downloader(&auth)
//!     .output_bytes()
//!     .send()
//!     .await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## 下载模式选择
//!
//! - **单线程**：`max_concurrent_chunks` 未设置或为 `Some(1)` 时，执行整文件 GET 请求
//! - **分片下载**：`max_concurrent_chunks` 设置为 `Some(n)` 且 `n > 1` 时启用
//!   - 要求远程文件必须有已知的 `size`（否则返回 `UnknownFileSizeForChunked` 错误）
//!   - 每个分片固定为 4MB（见 `CHUNK_SIZE`），实际分片数由文件大小决定
//!   - `max_concurrent_chunks` 控制同时进行的分片请求数上限，而非总分片数
//!
//! ## 内部实现说明
//!
//! - `chunked_download` 子模块：分片下载的完整实现，包括续传、任务调度、进度聚合
//! - `single_download` 子模块：单线程流式下载实现
//! - `error` 子模块：下载相关错误类型定义

mod chunked_download;
mod error;
mod single_download;

use std::path::Path;
use std::sync::Arc;

use reqwest::Client;
use tokio::sync::Mutex;

use crate::internal::auth::structs::webdav_auth::WebdavAuth;
use crate::internal::remote_file::structs::remote_file_data::RemoteFileData;
use crate::internal::remote_file::structs::remote_file::RemoteFile;
use crate::internal::states::unlock_reactive::UnlockReactiveProperty;
use crate::remote_file::remote_file_downloader::chunked_download::{
    run_chunked_download, RunChunkedDownloadParams,
};

use super::download_config::DownloadConfig;
use super::download_hooks_container::DownloadHooksContainer;
use super::download_progress::DownloadProgress;
use single_download::run_single_thread_download;

pub use error::DownloadError;

/// 远程文件下载器
///
/// 拥有响应式属性（通过 `progress()` 获取）：记录已下载大小（`bytes_done`），总大小（`total`）来自远程文件数据。
pub struct RemoteFileDownloader {
    pub(crate) client: Client,
    pub(crate) file_data: RemoteFileData,
    pub(crate) config: DownloadConfig,
    pub(crate) hooks: DownloadHooksContainer,
    pub(crate) progress_state: UnlockReactiveProperty<DownloadProgress>,
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

impl RemoteFile {
    /// 创建专属于本远程文件的下载器；可链式配置后调用 [`RemoteFileDownloader::send`] 执行下载。
    pub fn build_downloader(
        &self,
        auth: &WebdavAuth,
    ) -> RemoteFileDownloader {
        build_downloader(self, auth)
    }
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

    /// 设置最大分片并发数；大于 1 时启用分片下载（需远程文件有 size），最多同时请求 n 个分片。
    pub fn max_concurrent_chunks(mut self, n: usize) -> Self {
        self.config.max_concurrent_chunks = Some(n);
        self
    }

    /// 注册「开始前」钩子；闭包返回 `Err(HookAbort)` 会中止本次下载。
    pub fn with_before_start_hook<F, Fut>(mut self, f: F) -> Self
    where
        F: FnMut() -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<(), crate::internal::remote_file::downloader::traits::download::HookAbort>> + Send + 'static,
    {
        self.hooks.add(super::hook_adapters::BeforeStartHookAdapter(f));
        self
    }

    /// 注册「进度」钩子；参数为已下载字节数、总大小（可能未知为 `None`）。
    pub fn with_on_progress_hook<F>(mut self, f: F) -> Self
    where
        F: FnMut(u64, Option<u64>) + Send + Sync + 'static,
    {
        self.hooks.add(super::hook_adapters::OnProgressHookAdapter(f));
        self
    }

    /// 注册「完成后」钩子；下载成功结束后调用。
    pub fn with_after_complete_hook<F, Fut>(mut self, f: F) -> Self
    where
        F: FnMut() -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        self.hooks.add(super::hook_adapters::AfterCompleteHookAdapter(f));
        self
    }

    /// 添加完整钩子，在下载各阶段插入逻辑。
    pub fn with_hook(
        mut self,
        hook: impl crate::internal::remote_file::downloader::traits::download::DownloadHook + 'static,
    ) -> Self {
        self.hooks.add(hook);
        self
    }

    /// 内置的下载进度状态；返回可共享句柄，`.watch()` 后 `changed().await` 监听进度。
    pub fn progress(&self) -> UnlockReactiveProperty<DownloadProgress> {
        self.progress_state.clone()
    }

    /// 执行下载。单线程返回 Saved 或 Bytes；分片（max_concurrent_chunks > 1）需已知 size，返回 Saved 或 BytesSegments。
    pub async fn send(self) -> Result<super::download_result::DownloadResult, DownloadError> {
        let is_multi = match self.config.max_concurrent_chunks {
            Some(n) => n > 1,
            None => false,
        };

        if is_multi {
            if self.file_data.size.is_none() {
                return Err(DownloadError::UnknownFileSizeForChunked);
            }
            let hooks = Arc::new(Mutex::new(self.hooks));
            return run_chunked_download(RunChunkedDownloadParams {
                client: self.client,
                file_data: self.file_data,
                config: self.config,
                hooks,
                progress: self.progress_state,
            })
            .await;
        }

        run_single_thread_download(
            &self.client,
            self.file_data,
            self.config,
            self.hooks,
            self.progress_state,
        )
        .await
    }
}
