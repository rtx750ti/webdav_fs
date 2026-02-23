use std::path::PathBuf;

/// 本次下载的配置。
#[derive(Debug, Clone, Default)]
pub struct DownloadConfig {
    pub save_path: Option<PathBuf>,
    /// 是否输出字节数组；仅单线程下载时有效。
    pub is_output_bytes: bool,
}
