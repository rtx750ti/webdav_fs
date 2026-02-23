use std::path::PathBuf;

/// 本次下载的配置。
#[derive(Debug, Clone, Default)]
pub struct DownloadConfig {
    pub save_path: Option<PathBuf>,
    /// 是否输出字节数组。单线程时返回连续 `Vec<u8>`；多线程时返回 `ByteSegments`（按 offset 可寻址）。
    pub is_output_bytes: bool,
    /// 并发分片数。`None` 或 `Some(1)` 表示单线程；`Some(n)` 且 n > 1 表示多线程分片下载。
    pub concurrent_chunks: Option<usize>,
}
