//! 分片下载：断点续传——根据本地文件是否存在及大小，决定续传起点或已完整。

use std::path::Path;

use tokio::fs;

use crate::internal::remote_file::downloader::structs::{DownloadProgress};
use crate::internal::states::unlock_reactive::UnlockReactiveProperty;

use super::super::error::DownloadError;

/// 断点续传检查结果：已完整可直接返回，或从某偏移开始下载。
pub enum ChunkedResumeOutcome {
    AlreadyComplete,
    DownloadFrom { start: u64 },
}

/// 计算续传起点时的参数（形参超过 3 个，用 struct 承载）。
pub struct ComputeResumeStartParams<'a> {
    pub save_path: Option<&'a Path>,
    pub output_bytes: bool,
    pub total: u64,
    pub progress: &'a UnlockReactiveProperty<DownloadProgress>,
}

/// 根据本地文件是否存在及大小，决定续传起点或已完整。
pub async fn compute_resume_start(
    params: ComputeResumeStartParams<'_>,
) -> Result<ChunkedResumeOutcome, DownloadError> {
    if params.save_path.is_none() || params.output_bytes {
        return Ok(ChunkedResumeOutcome::DownloadFrom { start: 0 });
    }
    let p = params.save_path.unwrap();
    let local_len = fs::metadata(p).await.map(|m| m.len()).unwrap_or(0);
    if local_len >= params.total {
        let _ = params.progress.update(DownloadProgress {
            bytes_done: params.total,
            total: Some(params.total),
        });
        return Ok(ChunkedResumeOutcome::AlreadyComplete);
    }
    if local_len > params.total {
        fs::remove_file(p)
            .await
            .map_err(|e| DownloadError::ChunkedInternal(e.to_string()))?;
        return Ok(ChunkedResumeOutcome::DownloadFrom { start: 0 });
    }
    Ok(ChunkedResumeOutcome::DownloadFrom {
        start: local_len,
    })
}
