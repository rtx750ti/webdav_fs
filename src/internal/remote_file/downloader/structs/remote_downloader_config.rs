use super::download_mode::DownloadMode;

/// 默认分片大小：1MB
pub const DEFAULT_CHUNK_SIZE: u64 = 1024 * 1024;

/// 默认重试次数
pub const DEFAULT_MAX_RETRIES: usize = 3;

/// 默认重试延迟（毫秒）
pub const DEFAULT_RETRY_DELAY_MS: u64 = 1000;

#[derive(Debug, Clone)]
pub struct RemoteDownloaderConfig {
    pub download_mode: DownloadMode,
    /// 最大并发分片数
    pub max_chunks: usize,
    /// 每个分片的大小（字节）
    pub chunk_size: u64,
    /// 分片失败最大重试次数
    pub max_retries: usize,
    /// 重试延迟（毫秒）
    pub retry_delay_ms: u64,
}

impl Default for RemoteDownloaderConfig {
    fn default() -> Self {
        Self {
            download_mode: DownloadMode::OutputBytes,
            max_chunks: 1,
            chunk_size: DEFAULT_CHUNK_SIZE,
            max_retries: DEFAULT_MAX_RETRIES,
            retry_delay_ms: DEFAULT_RETRY_DELAY_MS,
        }
    }
}

