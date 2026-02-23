use std::path::PathBuf;

/// 本次下载的配置。
#[derive(Debug, Clone, Default)]
pub struct DownloadConfig {
    pub save_path: Option<PathBuf>,
    /// 是否输出字节数组。单线程时返回连续 `Vec<u8>`；多线程时返回 `ByteSegments`（按 offset 可寻址）。
    pub is_output_bytes: bool,
    /// 最大分片并发数：同时进行的分片下载任务上限。`None` 或 `Some(1)` 表示单线程整文件下载；`Some(n)` 且 n > 1 表示分片下载，最多 n 个分片同时请求（实际段数由文件大小与每段 4MB 决定）。
    pub max_concurrent_chunks: Option<usize>,
}
