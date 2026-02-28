/// 下载状态（由下载器内部维护，外部只读监听）
#[derive(Debug, Clone)]
pub enum DownloadStatus {
    Running,
    Paused,
    Canceled,
    Finished,
}

