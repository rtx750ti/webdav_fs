//! 单线程整文件下载。

use futures_util::StreamExt;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

use crate::internal::remote_file::downloader::structs::{
    DownloadConfig, DownloadHooksContainer, DownloadProgress,
    DownloadResult,
};
use crate::internal::remote_file::structs::remote_file_data::RemoteFileData;
use crate::internal::states::unlock_reactive::UnlockReactiveProperty;

use super::error::DownloadError;

/// 单线程下载：整文件 GET，流式写入并更新进度。
pub(super) async fn run_single_thread_download(
    client: &reqwest::Client,
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
