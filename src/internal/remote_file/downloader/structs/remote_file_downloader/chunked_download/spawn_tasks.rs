//! 分片下载：生成并 spawn 各段 Range 任务，以及等待所有任务完成。

use std::sync::atomic::AtomicU64;
use std::sync::Arc;

use tokio::fs::File;
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;

use crate::internal::remote_file::downloader::structs::{
    ByteSegment, DownloadHooksContainer, DownloadProgress,
};
use crate::internal::states::unlock_reactive::UnlockReactiveProperty;

use super::constants::{range_header, CHUNK_SIZE};
use super::download_one_range::{download_one_range, DownloadOneRangeParams};
use super::super::error::DownloadError;

/// 单个分片任务句柄：(range_start, JoinHandle)。
pub type RangeTaskHandle = (u64, JoinHandle<Result<(), DownloadError>>);

/// 生成并 spawn 分片任务时的参数（形参超过 3 个，用 struct 承载）。
pub struct SpawnRangeTasksParams<'a> {
    pub client: &'a reqwest::Client,
    pub url: &'a str,
    pub total: u64,
    pub start: u64,
    pub file: &'a Option<File>,
    pub semaphore: Arc<Semaphore>,
    pub bytes_done: Arc<AtomicU64>,
    pub progress: UnlockReactiveProperty<DownloadProgress>,
    pub hooks: Arc<tokio::sync::Mutex<DownloadHooksContainer>>,
    pub segments_for_bytes: Option<Arc<tokio::sync::Mutex<Vec<ByteSegment>>>>,
}

/// 从 start 到 total 按 CHUNK_SIZE 生成并 spawn 所有分片任务，返回任务句柄列表。
pub async fn spawn_range_tasks(
    params: SpawnRangeTasksParams<'_>,
) -> Result<Vec<RangeTaskHandle>, DownloadError> {
    let mut range_start = params.start;
    let mut handles = Vec::new();
    while range_start < params.total {
        let end = (range_start + CHUNK_SIZE).min(params.total);
        let range = range_header(range_start, end);
        let task_file = match params.file {
            Some(f) => {
                let cloned = f
                .try_clone()
                .await
                .map_err(|e| DownloadError::CreateFile(e))?;
                Some(cloned)
            }
            None => None,
        };
        let download_params = DownloadOneRangeParams {
            client: params.client.clone(),
            url: params.url.to_string(),
            range,
            start: range_start,
            total: params.total,
            task_file,
            bytes_done: Arc::clone(&params.bytes_done),
            progress: params.progress.clone(),
            hooks: Arc::clone(&params.hooks),
            segments: params.segments_for_bytes.clone(),
        };
        let sem = Arc::clone(&params.semaphore);
        let handle = tokio::spawn(async move {
            let _permit = sem
                .acquire_owned()
                .await
                .map_err(|_| DownloadError::ChunkedInternal("semaphore closed".into()))?;
            download_one_range(download_params).await
        });
        handles.push((range_start, handle));
        range_start = end;
    }
    Ok(handles)
}

/// 等待全部分片任务完成并汇总错误。
pub async fn join_range_handles(
    handles: Vec<RangeTaskHandle>,
) -> Result<(), DownloadError> {
    for (_start, h) in handles {
        match h.await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(e),
            Err(join_err) => return Err(DownloadError::TaskJoin(join_err)),
        }
    }
    Ok(())
}
