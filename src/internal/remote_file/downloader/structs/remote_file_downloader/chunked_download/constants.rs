//! 分片下载常量与工具。

/// 每个分片的字节数上限（4MB）；实际段数 = ceil(文件大小 / CHUNK_SIZE)，由文件大小决定，与最大分片并发数无关。
pub(super) const CHUNK_SIZE: u64 = 4 * 1024 * 1024;

/// 生成单个 Range 请求头：`bytes=start-(end-1)`，end 为不含上界。
pub(super) fn range_header(start: u64, end: u64) -> String {
    let end_inclusive = end.saturating_sub(1);
    format!("bytes={}-{}", start, end_inclusive)
}
