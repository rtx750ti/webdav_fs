//! 分片下载：执行单段 Range 下载的编排——请求、流式读块、写文件与进度、收尾 segment。

use std::sync::atomic::AtomicU64;
use std::sync::Arc;

use futures_util::StreamExt;
use tokio::fs::File;
use tokio::sync::Mutex;

use crate::internal::remote_file::downloader::structs::{
    ByteSegment, DownloadHooksContainer, DownloadProgress,
};
use crate::internal::states::unlock_reactive::UnlockReactiveProperty;

use super::chunk_handler::{handle_one_chunk, HandleOneChunkParams};
use super::range_request::{fetch_range_response, FetchRangeParams};
use super::super::error::DownloadError;

/// 执行单段 Range 下载时的参数（形参超过 3 个，用 struct 承载）。
pub struct DownloadOneRangeParams {
    pub client: reqwest::Client,
    pub url: String,
    pub range: String,
    pub start: u64,
    pub total: u64,
    pub task_file: Option<File>,
    pub bytes_done: Arc<AtomicU64>,
    pub progress: UnlockReactiveProperty<DownloadProgress>,
    pub hooks: Arc<Mutex<DownloadHooksContainer>>,
    pub segments: Option<Arc<Mutex<Vec<ByteSegment>>>>,
}

/// 执行单段 Range 下载：流式读取，每块写文件、更新进度、触发钩子，最后写入 segment。
pub async fn download_one_range(
    mut params: DownloadOneRangeParams,
) -> Result<(), DownloadError> {
    if params.hooks.lock().await.cancel_requested() {
        return Err(DownloadError::Cancelled);
    }

    let resp = fetch_range_response(FetchRangeParams {
        client: &params.client,
        url: &params.url,
        range: &params.range,
    })
    .await?;

    let mut stream = resp.bytes_stream();
    let mut file_offset = params.start;
    let mut segment_buf: Option<Vec<u8>> = params.segments.as_ref().map(|_| Vec::new());

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.map_err(DownloadError::Request)?;

        handle_one_chunk(HandleOneChunkParams {
            chunk,
            file_offset: &mut file_offset,
            task_file: params.task_file.as_mut(),
            bytes_done: &params.bytes_done,
            progress: &params.progress,
            total: params.total,
            hooks: &params.hooks,
            segment_buf: segment_buf.as_mut(),
        })
        .await?;
    }

    if let (Some(seg), Some(buf)) = (params.segments, segment_buf) {
        seg.lock().await.push(ByteSegment {
            offset: params.start,
            data: buf,
        });
    }

    Ok(())
}
