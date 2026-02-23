//! 分片下载：多段 Range 请求 + 并发写文件 + 进度聚合。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use reqwest::header::RANGE;
use tokio::fs::File;
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::sync::Mutex;

use crate::internal::remote_file::downloader::structs::{
    ByteSegment, ByteSegments, DownloadConfig, DownloadHooksContainer,
    DownloadProgress, DownloadResult,
};
use crate::internal::remote_file::structs::remote_file_data::RemoteFileData;
use crate::internal::states::unlock_reactive::UnlockReactiveProperty;

use super::super::error::DownloadError;

/// 每个分片的字节数上限（4MB）；实际段数 = ceil(文件大小 / CHUNK_SIZE)，由文件大小决定，与最大分片并发数无关。
const CHUNK_SIZE: u64 = 4 * 1024 * 1024;

/// 生成单个 Range 请求头：`bytes=start-(end-1)`，end 为不含上界。
fn range_header(start: u64, end: u64) -> String {
    let end_inclusive = end.saturating_sub(1);
    format!("bytes={}-{}", start, end_inclusive)
}

/// 从配置解析最大分片并发数；大于 1 才启用分片，否则默认 2。
fn max_concurrent_from_config(config: &DownloadConfig) -> usize {
    match config.max_concurrent_chunks.filter(|n| *n > 1) {
        Some(n) => n,
        None => 2,
    }
}

/// 执行单段 Range 下载：请求、写入指定偏移、更新进度与钩子、可选写入 ByteSegment。
async fn download_one_range(
    client: &reqwest::Client,
    url: &str,
    range: &str,
    start: u64,
    total: u64,
    task_file: Option<tokio::fs::File>,
    bytes_done: Arc<AtomicU64>,
    progress: UnlockReactiveProperty<DownloadProgress>,
    hooks: Arc<Mutex<DownloadHooksContainer>>,
    segments: Option<Arc<Mutex<Vec<ByteSegment>>>>,
) -> Result<(), DownloadError> {
    if hooks.lock().await.cancel_requested() {
        return Err(DownloadError::Cancelled);
    }

    let resp = client.get(url).header(RANGE, range).send().await?;
    let body = resp.bytes().await?;
    let len = body.len() as u64;

    if let Some(mut f) = task_file {
        f.seek(std::io::SeekFrom::Start(start))
            .await
            .map_err(DownloadError::WriteFile)?;
        f.write_all(&body)
            .await
            .map_err(DownloadError::WriteFile)?;
    }

    let current = bytes_done.fetch_add(len, Ordering::Relaxed) + len;

    let _ = progress.update(DownloadProgress {
        bytes_done: current,
        total: Some(total),
    });

    let mut h = hooks.lock().await;
    h.run_on_chunk(&body);
    h.run_on_progress(current, Some(total));

    if let Some(seg) = segments {
        seg.lock().await.push(ByteSegment {
            offset: start,
            data: body.to_vec(),
        });
    }

    Ok(())
}

/// 分片下载入口：已知 size，按 CHUNK_SIZE 切段，多任务 Range 请求并写文件。
pub(crate) async fn run_chunked_download(
    client: reqwest::Client,
    file_data: RemoteFileData,
    config: DownloadConfig,
    hooks: Arc<Mutex<DownloadHooksContainer>>,
    progress: UnlockReactiveProperty<DownloadProgress>,
) -> Result<DownloadResult, DownloadError> {
    let total = match file_data.size {
        Some(t) => t,
        None => return Err(DownloadError::UnknownFileSizeForChunked),
    };

    let save_path = config.save_path.as_ref();
    let output_bytes = config.is_output_bytes;
    if save_path.is_none() && !output_bytes {
        return Err(DownloadError::NoDestination);
    }

    hooks
        .lock()
        .await
        .run_before_start()
        .await
        .map_err(DownloadError::HookAbort)?;

    let _ = progress.update(DownloadProgress {
        bytes_done: 0,
        total: Some(total),
    });

    let max_concurrent = max_concurrent_from_config(&config);
    let semaphore = Arc::new(tokio::sync::Semaphore::new(max_concurrent));
    let bytes_done = Arc::new(AtomicU64::new(0));

    let file = match save_path {
        Some(p) => {
            let f = File::create(p)
                .await
                .map_err(DownloadError::CreateFile)?;
            Some(f)
        }
        None => None,
    };

    let segments_for_bytes: Option<Arc<Mutex<Vec<ByteSegment>>>> =
        if output_bytes {
            Some(Arc::new(Mutex::new(Vec::new())))
        } else {
            None
        };

    let url = file_data.absolute_path.clone();
    let mut start: u64 = 0;
    let mut handles = Vec::new();

    while start < total {
        let end = (start + CHUNK_SIZE).min(total);
        let range = range_header(start, end);
        let task_file = match &file {
            Some(f) => {
                let cloned = f
                    .try_clone()
                    .await
                    .map_err(DownloadError::CreateFile)?;
                Some(cloned)
            }
            None => None,
        };

        let client = client.clone();
        let url = url.clone();
        let sem = Arc::clone(&semaphore);
        let progress_clone = progress.clone();
        let bytes_done_clone = Arc::clone(&bytes_done);
        let hooks_clone = Arc::clone(&hooks);
        let segments_clone = segments_for_bytes.clone();
        let start_u64 = start;

        let handle = tokio::spawn(async move {
            let _permit = sem
                .acquire_owned()
                .await
                .map_err(|_| DownloadError::ChunkedInternal("semaphore closed".into()))?;

            download_one_range(
                &client,
                &url,
                &range,
                start_u64,
                total,
                task_file,
                bytes_done_clone,
                progress_clone,
                hooks_clone,
                segments_clone,
            )
            .await
        });

        handles.push((start, handle));
        start = end;
    }

    drop(file);

    for (_start, h) in handles {
        match h.await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(e),
            Err(join_err) => return Err(DownloadError::TaskJoin(join_err)),
        }
    }

    hooks.lock().await.run_after_complete().await;

    if output_bytes {
        let segs = match &segments_for_bytes {
            Some(arc) => {
                let mut guard = arc.lock().await;
                let mut vec = Vec::new();
                std::mem::swap(&mut *guard, &mut vec);
                vec
            }
            None => {
                return Err(DownloadError::ChunkedInternal(
                    "output_bytes 时 segments 未初始化".into(),
                ));
            }
        };
        let mut segs = segs;
        segs.sort_by_key(|s| s.offset);
        Ok(DownloadResult::BytesSegments(ByteSegments::new(segs)))
    } else {
        Ok(DownloadResult::Saved)
    }
}
