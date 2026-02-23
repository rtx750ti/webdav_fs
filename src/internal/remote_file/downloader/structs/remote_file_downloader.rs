use reqwest::Client;

use crate::internal::remote_file::structs::remote_file_data::RemoteFileData;
use crate::internal::states::unlock_reactive::UnlockReactiveProperty;

use super::download_config::DownloadConfig;
use super::download_hooks_container::DownloadHooksContainer;
use super::download_progress::DownloadProgress;

/// 远程文件专用下载器：由远程文件主动创建，配置后调用 `send` 执行下载。
///
/// 拥有响应式属性（通过 `progress()` 获取）：记录已下载大小（`bytes_done`），总大小（`total`）来自远程文件数据。
pub struct RemoteFileDownloader {
    pub(crate) client: Client,
    pub(crate) file_data: RemoteFileData,
    pub(crate) config: DownloadConfig,
    pub(crate) hooks: DownloadHooksContainer,
    pub(crate) progress_state: UnlockReactiveProperty<DownloadProgress>,
}
