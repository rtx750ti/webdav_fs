//! 分片下载：处理单块数据——写文件、更新进度、触发钩子、累积到 segment 缓冲。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tokio::fs::File;
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::sync::Mutex;

use crate::internal::remote_file::downloader::structs::{
    DownloadHooksContainer, DownloadProgress,
};
use crate::internal::states::unlock_reactive::UnlockReactiveProperty;

use super::super::error::DownloadError;

/// 处理单块数据时的参数（形参超过 3 个，用 struct 承载）。
pub struct HandleOneChunkParams<'a> {
    pub chunk: bytes::Bytes,
    pub file_offset: &'a mut u64,
    pub task_file: Option<&'a mut File>,
    pub bytes_done: &'a Arc<AtomicU64>,
    pub progress: &'a UnlockReactiveProperty<DownloadProgress>,
    pub total: u64,
    pub hooks: &'a Arc<Mutex<DownloadHooksContainer>>,
    pub segment_buf: Option<&'a mut Vec<u8>>,
}

/// 将一块数据写入文件、更新整体进度、触发钩子，并可选地累积到 segment 缓冲。
pub async fn handle_one_chunk(
    params: HandleOneChunkParams<'_>,
) -> Result<(), DownloadError> {
    let len = params.chunk.len() as u64;
    if len == 0 {
        return Ok(());
    }

    if let Some(f) = params.task_file {
        f.seek(std::io::SeekFrom::Start(*params.file_offset))
            .await
            .map_err(DownloadError::WriteFile)?;
        f.write_all(&params.chunk)
            .await
            .map_err(DownloadError::WriteFile)?;
    }
    *params.file_offset += len;

    let current = params.bytes_done.fetch_add(len, Ordering::Relaxed) + len;
    let _ = params.progress.update(DownloadProgress {
        bytes_done: current,
        total: Some(params.total),
    });
    {
        let mut h = params.hooks.lock().await;
        h.run_on_chunk(&params.chunk);
        h.run_on_progress(current, Some(params.total));
    }
    if let Some(buf) = params.segment_buf {
        buf.extend_from_slice(&params.chunk);
    }

    Ok(())
}
