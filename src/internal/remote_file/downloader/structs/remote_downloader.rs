use crate::internal::states::queue_reactive::QueueReactiveConsumer;
use crate::{auth::WebdavAuth, remote_file::RemoteFileData};
use std::sync::Arc;
use tokio::sync::Mutex;

use super::control_command::ControlCommand;
use super::download_error::DownloadError;
use super::download_mode::DownloadMode;
use super::download_result::DownloadResult;
use super::remote_downloader_controller::RemoteDownloaderController;

/// 远程文件下载器，不实现Clone，是因为下载器一旦开始下载，就不应该被克隆，否则会有多份下载器同时下载同一个文件，导致文件内容错误。
#[derive(Debug)]
pub struct RemoteDownloader {
    /// Controller 无锁共享引用：pause/resume/cancel/get_downloaded_bytes 等操作不需要锁
    /// 因为命令通过 mpsc 队列发送（无锁），状态通过 watch channel 读取（无锁）
    controller: Arc<RemoteDownloaderController>,
    /// 命令消费者：下载时内部消费命令队列（Mutex 包装是因为 recv 需要 &mut）
    command_consumer: Mutex<QueueReactiveConsumer<ControlCommand>>,
}

impl RemoteDownloader {
    pub fn new(
        remote_file_data: Arc<RemoteFileData>,
        webdav_auth: WebdavAuth,
    ) -> Self {
        let default_download_mode = DownloadMode::OutputBytes;

        let (controller, command_consumer) =
            RemoteDownloaderController::new(
                remote_file_data,
                webdav_auth,
                default_download_mode,
            );

        Self {
            controller: Arc::new(controller),
            command_consumer: Mutex::new(command_consumer),
        }
    }

    /// 设置保存路径
    /// 注意：必须在 send() 之前调用，send() 之后配置不可变
    pub fn save_to(mut self, save_path: &str) -> Self {
        Arc::get_mut(&mut self.controller)
            .expect("Cannot configure after controller is shared")
            .set_download_mode(DownloadMode::SaveFile(save_path.to_string()));
        self
    }

    /// 设置输出到内存
    pub fn output_bytes(mut self) -> Self {
        Arc::get_mut(&mut self.controller)
            .expect("Cannot configure after controller is shared")
            .set_download_mode(DownloadMode::OutputBytes);
        self
    }

    /// 设置最大分片数（并发数）
    pub fn max_chunks(mut self, max_chunks: usize) -> Self {
        Arc::get_mut(&mut self.controller)
            .expect("Cannot configure after controller is shared")
            .set_max_chunks(max_chunks);
        self
    }

    /// 设置分片大小（字节）
    pub fn chunk_size(mut self, chunk_size: u64) -> Self {
        Arc::get_mut(&mut self.controller)
            .expect("Cannot configure after controller is shared")
            .set_chunk_size(chunk_size);
        self
    }

    /// 设置分片失败最大重试次数
    pub fn max_retries(mut self, max_retries: usize) -> Self {
        Arc::get_mut(&mut self.controller)
            .expect("Cannot configure after controller is shared")
            .set_max_retries(max_retries);
        self
    }

    pub fn get_controller(
        &self,
    ) -> Arc<RemoteDownloaderController> {
        Arc::clone(&self.controller)
    }

    pub async fn send(&self) -> Result<DownloadResult, DownloadError> {
        let mut consumer = self.command_consumer.lock().await;
        // controller 是 Arc<RemoteDownloaderController>，不需要锁
        // download() 只需要 &self，pause/resume/cancel 通过 mpsc 队列发送（无锁）
        self.controller.download(&mut consumer).await
    }
}
