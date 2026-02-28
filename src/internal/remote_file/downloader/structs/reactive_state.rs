use crate::internal::states::queue_reactive::QueueReactiveProperty;
use crate::states::unlock_reactive::UnlockReactiveProperty;
use std::sync::Arc;
use tokio::sync::Notify;

use super::control_command::ControlCommand;
use super::download_status::DownloadStatus;

/// 下载器响应式状态
#[derive(Debug)]
pub struct RemoteDownloaderControllerReactiveState {
    /// 命令队列（生产者端）：外部通过 send 发送控制命令
    pub(crate) command_queue: QueueReactiveProperty<ControlCommand>,
    /// 下载状态（只读）：内部根据命令更新，外部通过 watch 监听
    pub download_status: UnlockReactiveProperty<DownloadStatus>,
    /// 已下载字节数（只读）：内部更新，外部通过 watch 监听
    pub downloaded_bytes: UnlockReactiveProperty<u64>,
    /// 恢复通知器：用于精确唤醒暂停的任务
    pub(crate) resume_notifier: Arc<Notify>,
}

