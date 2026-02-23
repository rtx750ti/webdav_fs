/// 下载进度：响应式状态，记录已下载字节数；总大小来自远程文件数据（创建下载器时写入）。
///
/// 调用方通过下载器的 `progress()` 读取或监听；进度比例可用 [`DownloadProgress::pct`] 获取。
#[derive(Debug, Clone, Default)]
pub struct DownloadProgress {
    /// 已下载的文件大小（字节）
    pub bytes_done: u64,
    /// 文件总大小（字节），来自远程文件数据，未知时为 `None`
    pub total: Option<u64>,
}

impl DownloadProgress {
    /// 进度百分比（0～100）；总大小为 0 或未知时返回 `f64::NAN`。
    pub fn pct(&self) -> f64 {
        self.total
            .filter(|&t| t > 0)
            .map(|t| (self.bytes_done as f64 / t as f64) * 100.0)
            .unwrap_or(f64::NAN)
    }
}
