//! 分片下载：多段 Range 请求 + 并发写文件 + 进度聚合；支持断点续传（仅保存到文件时）。

use std::path::Path;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

use tokio::fs::{File, OpenOptions};
use tokio::sync::Mutex;

use crate::internal::remote_file::downloader::structs::{
    ByteSegment, ByteSegments, DownloadConfig, DownloadHooksContainer,
    DownloadProgress, DownloadResult,
};
use crate::internal::states::unlock_reactive::UnlockReactiveProperty;

use super::resume::{compute_resume_start, ChunkedResumeOutcome, ComputeResumeStartParams};
use super::spawn_tasks::{join_range_handles, spawn_range_tasks, SpawnRangeTasksParams};
use super::super::error::DownloadError;

/// 从配置解析最大分片并发数；大于 1 才启用分片，否则默认 2。
fn max_concurrent_from_config(config: &DownloadConfig) -> usize {
    match config.max_concurrent_chunks.filter(|n| *n > 1) {
        Some(n) => n,
        None => 2,
    }
}

/// 打开已保存的文件，用于断点续传。
async fn open_saved_file(
    save_path: Option<&Path>,
    start: u64,
) -> Result<Option<File>, DownloadError> {
    let p = match save_path {
        Some(x) => x,
        None => return Ok(None),
    };
    let f = if start > 0 {
        OpenOptions::new()
            .create(true)
            .write(true)
            .open(p)
            .await
            .map_err(DownloadError::CreateFile)?
    } else {
        File::create(p).await.map_err(DownloadError::CreateFile)?
    };
    Ok(Some(f))
}

/// 分片下载入口函数的参数（形参超过 3 个，用 struct 承载）。
pub struct RunChunkedDownloadParams {
    pub client: reqwest::Client,
    pub file_data: crate::internal::remote_file::structs::remote_file_data::RemoteFileData,
    pub config: DownloadConfig,
    pub hooks: Arc<Mutex<DownloadHooksContainer>>,
    pub progress: UnlockReactiveProperty<DownloadProgress>,
}

/// 根据是否 output_bytes 从 segments 构建最终 DownloadResult。
async fn build_final_result(
    output_bytes: bool,
    segments_for_bytes: Option<Arc<Mutex<Vec<ByteSegment>>>>,
) -> Result<DownloadResult, DownloadError> {
    if !output_bytes {
        return Ok(DownloadResult::Saved);
    }
    let arc = segments_for_bytes.ok_or_else(|| {
        DownloadError::ChunkedInternal("output_bytes 时 segments 未初始化".into())
    })?;
    let mut segs: Vec<ByteSegment> = arc.lock().await.drain(..).collect();
    segs.sort_by_key(|s| s.offset);
    Ok(DownloadResult::BytesSegments(ByteSegments::new(segs)))
}

/// 分片下载入口：编排校验、续传、打开文件、spawn 任务、等待、收尾。
pub(crate) async fn run_chunked_download(
    params: RunChunkedDownloadParams,
) -> Result<DownloadResult, DownloadError> {
    let total = params
        .file_data
        .size
        .ok_or(DownloadError::UnknownFileSizeForChunked)?;
    let save_path = params.config.save_path.as_deref();
    let output_bytes = params.config.is_output_bytes;
    if save_path.is_none() && !output_bytes {
        return Err(DownloadError::NoDestination);
    }

    params
        .hooks
        .lock()
        .await
        .run_before_start()
        .await
        .map_err(DownloadError::HookAbort)?;

    let outcome = compute_resume_start(ComputeResumeStartParams {
        save_path,
        output_bytes,
        total,
        progress: &params.progress,
    })
    .await?;
    let start = match outcome {
        ChunkedResumeOutcome::AlreadyComplete => return Ok(DownloadResult::Saved),
        ChunkedResumeOutcome::DownloadFrom { start: s } => s,
    };

    let _ = params.progress.update(DownloadProgress {
        bytes_done: start,
        total: Some(total),
    });
    let max_concurrent = max_concurrent_from_config(&params.config);
    let semaphore = Arc::new(tokio::sync::Semaphore::new(max_concurrent));
    let bytes_done = Arc::new(AtomicU64::new(start));

    let file = open_saved_file(save_path, start).await?;
    let segments_for_bytes: Option<Arc<Mutex<Vec<ByteSegment>>>> =
        if output_bytes {
            Some(Arc::new(Mutex::new(Vec::new())))
        } else {
            None
        };

    let url = params.file_data.absolute_path.as_str();
    let handles = spawn_range_tasks(SpawnRangeTasksParams {
        client: &params.client,
        url,
        total,
        start,
        file: &file,
        semaphore,
        bytes_done,
        progress: params.progress.clone(),
        hooks: params.hooks.clone(),
        segments_for_bytes: segments_for_bytes.clone(),
    })
    .await?;
    drop(file);

    join_range_handles(handles).await?;
    params.hooks.lock().await.run_after_complete().await;

    build_final_result(output_bytes, segments_for_bytes).await
}
