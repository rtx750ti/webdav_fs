use crate::internal::states::queue_reactive::{
    QueueReactiveConsumer, QueueReactiveProperty,
};
use crate::{
    auth::WebdavAuth, remote_file::RemoteFileData,
    states::unlock_reactive::UnlockReactiveProperty,
};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use futures_util::StreamExt;
use reqwest::header::RANGE;
use tokio::fs::File;
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::sync::Mutex as TokioMutex;
use tokio::sync::Notify;

use super::byte_segments::{ByteSegment, ByteSegments};
use super::control_command::ControlCommand;
use super::download_error::DownloadError;
use super::download_mode::DownloadMode;
use super::download_result::DownloadResult;
use super::download_status::DownloadStatus;
use super::reactive_state::RemoteDownloaderControllerReactiveState;
use super::remote_downloader_config::RemoteDownloaderConfig;

#[derive(Debug)]
pub struct RemoteDownloaderController {
    file_data: Arc<RemoteFileData>,
    webdav_auth: WebdavAuth,
    config: RemoteDownloaderConfig,
    reactive_state: RemoteDownloaderControllerReactiveState,
}

/// 内部实现
impl RemoteDownloaderController {
    pub(crate) fn new(
        file_data: Arc<RemoteFileData>,
        webdav_auth: WebdavAuth,
        download_mode: DownloadMode,
    ) -> (Self, QueueReactiveConsumer<ControlCommand>) {
        // 创建命令队列
        let (command_queue, command_consumer) =
            QueueReactiveProperty::new();

        let controller = Self {
            file_data,
            webdav_auth,
            config: RemoteDownloaderConfig {
                download_mode,
                ..Default::default()
            },
            reactive_state: RemoteDownloaderControllerReactiveState {
                command_queue,
                download_status: UnlockReactiveProperty::new(
                    DownloadStatus::Running,
                ),
                downloaded_bytes: UnlockReactiveProperty::new(0),
                resume_notifier: Arc::new(Notify::new()),
            },
        };

        (controller, command_consumer)
    }

    pub(crate) fn set_download_mode(
        &mut self,
        download_mode: DownloadMode,
    ) {
        self.config.download_mode = download_mode;
    }

    pub(crate) fn set_max_chunks(&mut self, max_chunks: usize) {
        self.config.max_chunks = max_chunks;
    }

    pub(crate) fn set_chunk_size(&mut self, chunk_size: u64) {
        self.config.chunk_size = chunk_size;
    }

    pub(crate) fn set_max_retries(&mut self, max_retries: usize) {
        self.config.max_retries = max_retries;
    }
}

/// 外部接口：通过命令队列发送控制命令
impl RemoteDownloaderController {
    /// 暂停下载（发送 Pause 命令到队列）
    pub fn pause(&self) -> Result<(), ControlCommand> {
        self.reactive_state.command_queue.send(ControlCommand::Pause)
    }

    /// 恢复下载（发送 Resume 命令到队列）
    pub fn resume(&self) -> Result<(), ControlCommand> {
        let result = self.reactive_state.command_queue.send(ControlCommand::Resume);
        // 唤醒所有等待的任务
        self.reactive_state.resume_notifier.notify_waiters();
        result
    }

    /// 取消下载（发送 Cancel 命令到队列）
    pub fn cancel(&self) -> Result<(), ControlCommand> {
        let result = self.reactive_state.command_queue.send(ControlCommand::Cancel);
        // 唤醒所有等待的任务（让它们检查取消标志）
        self.reactive_state.resume_notifier.notify_waiters();
        result
    }

    /// 获取当前已下载字节数
    pub fn get_downloaded_bytes(&self) -> u64 {
        self.reactive_state.downloaded_bytes.get_current().unwrap_or(0)
    }

    /// 获取当前下载状态
    pub fn get_download_status(&self) -> Option<DownloadStatus> {
        self.reactive_state.download_status.get_current()
    }
}

/// 下载逻辑：内部消费命令队列，驱动状态变化
impl RemoteDownloaderController {
    /// 启动下载，consumer 由外部传入（因为 consumer 需要 &mut）
    pub(crate) async fn download(
        &self,
        consumer: &mut QueueReactiveConsumer<ControlCommand>,
    ) -> Result<DownloadResult, DownloadError> {
        let max_chunks = self.config.max_chunks;

        if max_chunks <= 1 {
            self.single_thread_download(consumer).await
        } else {
            self.chunked_download(consumer).await
        }
    }

    /// 辅助方法：等待恢复或取消
    async fn wait_for_resume_or_cancel(
        &self,
        consumer: &mut QueueReactiveConsumer<ControlCommand>,
        cancelled: &Arc<AtomicBool>,
        save_path: &Option<String>,
    ) -> Result<(), DownloadError> {
        loop {
            tokio::select! {
                _ = self.reactive_state.resume_notifier.notified() => {
                    // 被唤醒后检查命令
                    if let Some(cmd) = consumer.try_recv() {
                        match cmd {
                            ControlCommand::Resume => {
                                let _ = self.reactive_state.download_status
                                    .update(DownloadStatus::Running);
                                return Ok(());
                            }
                            ControlCommand::Cancel => {
                                cancelled.store(true, Ordering::SeqCst);
                                let _ = self.reactive_state.download_status
                                    .update(DownloadStatus::Canceled);
                                Self::cleanup_file(save_path).await;
                                return Err(DownloadError::Cancelled);
                            }
                            ControlCommand::Pause => continue,
                        }
                    }
                }
                cmd = consumer.recv() => {
                    match cmd {
                        Some(ControlCommand::Resume) => {
                            let _ = self.reactive_state.download_status
                                .update(DownloadStatus::Running);
                            return Ok(());
                        }
                        Some(ControlCommand::Cancel) | None => {
                            cancelled.store(true, Ordering::SeqCst);
                            let _ = self.reactive_state.download_status
                                .update(DownloadStatus::Canceled);
                            Self::cleanup_file(save_path).await;
                            return Err(DownloadError::Cancelled);
                        }
                        Some(ControlCommand::Pause) => continue,
                    }
                }
            }
        }
    }

    /// 辅助方法：清理临时文件
    async fn cleanup_file(save_path: &Option<String>) {
        if let Some(p) = save_path {
            let _ = tokio::fs::remove_file(p).await;
        }
    }

    pub(crate) async fn single_thread_download(
        &self,
        consumer: &mut QueueReactiveConsumer<ControlCommand>,
    ) -> Result<DownloadResult, DownloadError> {
        // 检查是否为目录
        if self.file_data.is_dir {
            return Err(DownloadError::IsDir);
        }

        // 解析下载模式
        let save_path = match &self.config.download_mode {
            DownloadMode::SaveFile(path) => Some(path.clone()),
            DownloadMode::OutputBytes => None,
        };
        let output_bytes = matches!(
            self.config.download_mode,
            DownloadMode::OutputBytes
        );

        if save_path.is_none() && !output_bytes {
            return Err(DownloadError::NoDestination);
        }

        // 初始化进度
        let _ = self.reactive_state.downloaded_bytes.update(0);
        let _ = self
            .reactive_state
            .download_status
            .update(DownloadStatus::Running);

        // 发起 HTTP GET 请求
        let resp = self
            .webdav_auth
            .client
            .get(&self.file_data.absolute_path)
            .send()
            .await?;

        let mut stream = resp.bytes_stream();
        let mut bytes_done: u64 = 0;
        let mut out_bytes: Vec<u8> = Vec::new();

        // 打开文件（如果需要保存到本地）
        let mut file: Option<File> = if let Some(ref p) = save_path {
            Some(File::create(p).await.map_err(DownloadError::CreateFile)?)
        } else {
            None
        };

        // 流式下载循环
        loop {
            // 使用 select! 同时处理命令和数据流
            tokio::select! {
                // 优先处理命令（biased 确保命令优先级）
                biased;

                cmd = consumer.recv() => {
                    match cmd {
                        Some(ControlCommand::Pause) => {
                            let _ = self
                                .reactive_state
                                .download_status
                                .update(DownloadStatus::Paused);

                            // 暂停：等待 resume_notifier 唤醒
                            loop {
                                tokio::select! {
                                    _ = self.reactive_state.resume_notifier.notified() => {
                                        // 被唤醒后检查命令
                                        if let Some(cmd) = consumer.try_recv() {
                                            match cmd {
                                                ControlCommand::Resume => {
                                                    let _ = self
                                                        .reactive_state
                                                        .download_status
                                                        .update(DownloadStatus::Running);
                                                    break;
                                                }
                                                ControlCommand::Cancel => {
                                                    let _ = self
                                                        .reactive_state
                                                        .download_status
                                                        .update(DownloadStatus::Canceled);
                                                    return Err(DownloadError::Cancelled);
                                                }
                                                ControlCommand::Pause => continue,
                                            }
                                        }
                                    }
                                    cmd = consumer.recv() => {
                                        match cmd {
                                            Some(ControlCommand::Resume) => {
                                                let _ = self
                                                    .reactive_state
                                                    .download_status
                                                    .update(DownloadStatus::Running);
                                                break;
                                            }
                                            Some(ControlCommand::Cancel) | None => {
                                                let _ = self
                                                    .reactive_state
                                                    .download_status
                                                    .update(DownloadStatus::Canceled);
                                                return Err(DownloadError::Cancelled);
                                            }
                                            Some(ControlCommand::Pause) => continue,
                                        }
                                    }
                                }
                            }
                        }
                        Some(ControlCommand::Cancel) => {
                            let _ = self
                                .reactive_state
                                .download_status
                                .update(DownloadStatus::Canceled);
                            return Err(DownloadError::Cancelled);
                        }
                        Some(ControlCommand::Resume) => {} // 已在运行中，忽略
                        None => return Err(DownloadError::Cancelled), // 队列关闭
                    }
                }

                // 读取下一块数据
                chunk_result = stream.next() => {
                    match chunk_result {
                        Some(Ok(chunk)) => {
                            let len = chunk.len() as u64;
                            bytes_done += len;

                            if let Some(f) = file.as_mut() {
                                f.write_all(&chunk)
                                    .await
                                    .map_err(DownloadError::WriteFile)?;
                            }
                            if output_bytes {
                                out_bytes.extend_from_slice(&chunk);
                            }

                            let _ = self
                                .reactive_state
                                .downloaded_bytes
                                .update(bytes_done);
                        }
                        Some(Err(e)) => {
                            return Err(DownloadError::Request(e));
                        }
                        None => break, // 流结束，下载完成
                    }
                }
            }
        }

        // 刷新文件缓冲区
        if let Some(mut f) = file {
            f.flush().await.map_err(DownloadError::FlushFile)?;
        }

        // 更新状态为完成
        let _ = self
            .reactive_state
            .download_status
            .update(DownloadStatus::Finished);

        // 返回结果
        if output_bytes {
            Ok(DownloadResult::Bytes(out_bytes))
        } else {
            Ok(DownloadResult::SavedToLocal(
                save_path.unwrap_or_default(),
            ))
        }
    }

    /// 多线程分片下载（改进版）
    ///
    /// 改进点：
    /// 1. 使用 CancellationToken 实现真正的任务取消
    /// 2. 使用 Arc<TokioMutex<File>> 保护文件写入，避免竞态条件
    /// 3. 支持分片失败重试
    /// 4. 可配置的分片大小
    /// 5. 取消时清理临时文件
    pub(crate) async fn chunked_download(
        &self,
        consumer: &mut QueueReactiveConsumer<ControlCommand>,
    ) -> Result<DownloadResult, DownloadError> {
        use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
        use tokio::sync::Semaphore;

        // 检查是否为目录
        if self.file_data.is_dir {
            return Err(DownloadError::IsDir);
        }

        // 检查文件大小是否已知
        let total = self
            .file_data
            .size
            .ok_or(DownloadError::UnknownFileSizeForChunked)?;

        // 解析下载模式
        let save_path = match &self.config.download_mode {
            DownloadMode::SaveFile(path) => Some(path.clone()),
            DownloadMode::OutputBytes => None,
        };
        let output_bytes = matches!(
            self.config.download_mode,
            DownloadMode::OutputBytes
        );

        if save_path.is_none() && !output_bytes {
            return Err(DownloadError::NoDestination);
        }

        // 初始化进度
        let _ = self.reactive_state.downloaded_bytes.update(0);
        let _ = self
            .reactive_state
            .download_status
            .update(DownloadStatus::Running);

        // 创建文件并预分配空间（如果需要保存）
        let file: Option<Arc<TokioMutex<File>>> = if let Some(ref p) = save_path {
            let f = File::create(p).await.map_err(DownloadError::CreateFile)?;
            // 预分配文件大小，避免碎片化
            f.set_len(total).await.map_err(DownloadError::PreallocateFile)?;
            Some(Arc::new(TokioMutex::new(f)))
        } else {
            None
        };

        // 分片数据存储（用于 output_bytes 模式）
        let segments: Arc<TokioMutex<Vec<(u64, Vec<u8>)>>> =
            Arc::new(TokioMutex::new(Vec::new()));

        // 并发控制
        let max_concurrent = self.config.max_chunks.max(2);
        let semaphore = Arc::new(Semaphore::new(max_concurrent));

        // 全局进度计数器
        let bytes_done = Arc::new(AtomicU64::new(0));

        // 取消标志（用于通知所有任务停止）
        let cancelled = Arc::new(AtomicBool::new(false));
        // 暂停标志
        let paused = Arc::new(AtomicBool::new(false));

        // 配置参数
        let chunk_size = self.config.chunk_size;
        let max_retries = self.config.max_retries;
        let retry_delay_ms = self.config.retry_delay_ms;

        // 生成分片任务
        let mut range_start = 0u64;
        let mut handles = Vec::new();
        let mut chunk_index = 0usize;

        while range_start < total {
            let range_end = (range_start + chunk_size).min(total);

            // 克隆需要的数据
            let client = self.webdav_auth.client.clone();
            let url = self.file_data.absolute_path.clone();
            let file_clone = file.clone();
            let sem = Arc::clone(&semaphore);
            let bytes_counter = Arc::clone(&bytes_done);
            let progress_state = self.reactive_state.downloaded_bytes.clone();
            let segments_clone = Arc::clone(&segments);
            let cancelled_clone = Arc::clone(&cancelled);
            let paused_clone = Arc::clone(&paused);
            let resume_notifier_clone = Arc::clone(&self.reactive_state.resume_notifier);
            let offset = range_start;
            let current_chunk_index = chunk_index;

            // Spawn 分片下载任务
            let handle = tokio::spawn(async move {
                Self::download_chunk(
                    client,
                    url,
                    offset,
                    range_end,
                    file_clone,
                    output_bytes,
                    segments_clone,
                    sem,
                    bytes_counter,
                    progress_state,
                    cancelled_clone,
                    paused_clone,
                    resume_notifier_clone,
                    current_chunk_index,
                    max_retries,
                    retry_delay_ms,
                ).await
            });

            handles.push((chunk_index, handle));
            range_start = range_end;
            chunk_index += 1;
        }

        // 收集错误
        let mut errors: Vec<String> = Vec::new();

        // 等待所有分片任务完成，同时监听控制命令
        for (idx, handle) in handles {
            tokio::pin!(handle);

            loop {
                tokio::select! {
                    biased;

                    cmd = consumer.recv() => {
                        match cmd {
                            Some(ControlCommand::Pause) => {
                                paused.store(true, Ordering::SeqCst);
                                let _ = self.reactive_state.download_status
                                    .update(DownloadStatus::Paused);
                                self.wait_for_resume_or_cancel(
                                    consumer, &cancelled, &save_path,
                                ).await?;
                                paused.store(false, Ordering::SeqCst);
                                // 恢复后继续 loop，等待 handle 完成
                                continue;
                            }
                            Some(ControlCommand::Cancel) => {
                                cancelled.store(true, Ordering::SeqCst);
                                let _ = self.reactive_state.download_status
                                    .update(DownloadStatus::Canceled);
                                Self::cleanup_file(&save_path).await;
                                return Err(DownloadError::Cancelled);
                            }
                            Some(ControlCommand::Resume) => continue,
                            None => return Err(DownloadError::Cancelled),
                        }
                    }

                    result = &mut handle => {
                        match result {
                            Ok(Ok(())) => {}
                            Ok(Err(DownloadError::Cancelled)) => {}
                            Ok(Err(e)) => {
                                errors.push(format!("分片 {}: {}", idx, e));
                            }
                            Err(e) => {
                                errors.push(format!("分片 {} 任务失败: {}", idx, e));
                            }
                        }
                        break; // handle 完成，进入下一个分片
                    }
                }
            }
        }

        // 检查是否有错误
        if !errors.is_empty() {
            // 清理临时文件
            if let Some(ref p) = save_path {
                let _ = tokio::fs::remove_file(p).await;
            }
            return Err(DownloadError::MultipleChunksFailed(errors));
        }

        // 刷新文件缓冲区
        if let Some(ref f) = file {
            let mut file_guard = f.lock().await;
            file_guard.flush().await.map_err(DownloadError::FlushFile)?;
        }

        // 更新状态为完成
        let _ = self
            .reactive_state
            .download_status
            .update(DownloadStatus::Finished);

        // 返回结果
        if output_bytes {
            // 按偏移量排序并构建 ByteSegments
            let mut raw_segments = segments.lock().await;
            raw_segments.sort_by_key(|(offset, _)| *offset);
            let byte_segments: Vec<ByteSegment> = raw_segments
                .drain(..)
                .map(|(offset, data)| ByteSegment { offset, data })
                .collect();
            Ok(DownloadResult::ByteSegments(ByteSegments::new(
                byte_segments,
            )))
        } else {
            Ok(DownloadResult::SavedToLocal(
                save_path.unwrap_or_default(),
            ))
        }
    }

    /// 下载单个分片（带重试和取消支持）
    async fn download_chunk(
        client: reqwest::Client,
        url: String,
        range_start: u64,
        range_end: u64,
        file: Option<Arc<TokioMutex<File>>>,
        output_bytes: bool,
        segments: Arc<TokioMutex<Vec<(u64, Vec<u8>)>>>,
        semaphore: Arc<tokio::sync::Semaphore>,
        bytes_counter: Arc<AtomicU64>,
        progress_state: crate::states::unlock_reactive::UnlockReactiveProperty<u64>,
        cancelled: Arc<AtomicBool>,
        paused: Arc<AtomicBool>,
        resume_notifier: Arc<Notify>,
        chunk_index: usize,
        max_retries: usize,
        retry_delay_ms: u64,
    ) -> Result<(), DownloadError> {
        // 获取信号量许可
        let _permit = semaphore.acquire().await.map_err(|_| {
            DownloadError::ChunkedInternal("信号量已关闭".into())
        })?;

        let range_header = format!("bytes={}-{}", range_start, range_end - 1);
        let mut retries = 0;

        loop {
            // 检查是否被取消
            if cancelled.load(Ordering::SeqCst) {
                return Err(DownloadError::Cancelled);
            }

            // 等待暂停结束（使用 Notify 精确唤醒）
            while paused.load(Ordering::SeqCst) {
                if cancelled.load(Ordering::SeqCst) {
                    return Err(DownloadError::Cancelled);
                }
                resume_notifier.notified().await;
            }

            // 尝试下载
            match Self::download_chunk_inner(
                &client,
                &url,
                &range_header,
                range_start,
                file.clone(),
                output_bytes,
                segments.clone(),
                bytes_counter.clone(),
                progress_state.clone(),
                cancelled.clone(),
                paused.clone(),
                resume_notifier.clone(),
            ).await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    retries += 1;
                    let last_error = e.to_string();

                    if retries > max_retries {
                        return Err(DownloadError::ChunkFailed {
                            chunk_index,
                            retries,
                            message: last_error,
                        });
                    }

                    // 等待后重试
                    tokio::time::sleep(tokio::time::Duration::from_millis(retry_delay_ms)).await;
                }
            }
        }
    }

    /// 分片下载内部实现（单次尝试）
    async fn download_chunk_inner(
        client: &reqwest::Client,
        url: &str,
        range_header: &str,
        offset: u64,
        file: Option<Arc<TokioMutex<File>>>,
        output_bytes: bool,
        segments: Arc<TokioMutex<Vec<(u64, Vec<u8>)>>>,
        bytes_counter: Arc<AtomicU64>,
        progress_state: crate::states::unlock_reactive::UnlockReactiveProperty<u64>,
        cancelled: Arc<AtomicBool>,
        paused: Arc<AtomicBool>,
        resume_notifier: Arc<Notify>,
    ) -> Result<(), DownloadError> {
        // 发起 Range 请求
        let resp = client
            .get(url)
            .header(RANGE, range_header)
            .send()
            .await?;

        let mut stream = resp.bytes_stream();
        let mut chunk_data = Vec::new();
        let mut file_offset = offset;

        // 流式读取分片数据
        while let Some(chunk_result) = stream.next().await {
            // 检查取消
            if cancelled.load(Ordering::SeqCst) {
                return Err(DownloadError::Cancelled);
            }

            // 等待暂停结束（使用 Notify 精确唤醒）
            while paused.load(Ordering::SeqCst) {
                if cancelled.load(Ordering::SeqCst) {
                    return Err(DownloadError::Cancelled);
                }
                resume_notifier.notified().await;
            }

            let chunk = chunk_result?;
            let len = chunk.len() as u64;

            // 写入文件（使用互斥锁保护）
            if let Some(ref f) = file {
                let mut file_guard = f.lock().await;
                file_guard.seek(std::io::SeekFrom::Start(file_offset))
                    .await
                    .map_err(DownloadError::SeekFile)?;
                file_guard.write_all(&chunk)
                    .await
                    .map_err(DownloadError::WriteFile)?;
                drop(file_guard);
            }

            // 保存到内存（如果需要）
            if output_bytes {
                chunk_data.extend_from_slice(&chunk);
            }

            // 更新全局进度
            let current = bytes_counter.fetch_add(len, Ordering::Relaxed) + len;
            let _ = progress_state.update(current);

            file_offset += len;
        }

        // 保存分片数据
        if output_bytes {
            segments.lock().await.push((offset, chunk_data));
        }

        Ok(())
    }
}

/// 响应式属性订阅：外部监听状态变化
impl RemoteDownloaderController {
    /// 订阅下载状态变化
    pub fn subscribe_download_status<F>(
        &self,
        return_current_value: bool,
        callback: F,
    ) where
        F: Fn(&DownloadStatus) + Send + 'static,
    {
        let mut watcher = self.reactive_state.download_status.watch();

        tokio::spawn(async move {
            if return_current_value {
                if let Some(current) = watcher.borrow() {
                    callback(&current);
                }
            }

            // 然后监听后续变化
            loop {
                match watcher.changed().await {
                    Ok(status) => callback(&status),
                    Err(_) => break,
                }
            }
        });
    }

    /// 订阅已下载字节数变化
    pub fn subscribe_downloaded_bytes<F>(
        &self,
        return_current_value: bool,
        callback: F,
    ) where
        F: Fn(u64) + Send + 'static,
    {
        let mut watcher = self.reactive_state.downloaded_bytes.watch();

        tokio::spawn(async move {
            if return_current_value {
                // 先发送当前值
                if let Some(current) = watcher.borrow() {
                    callback(current);
                }
            }

            loop {
                match watcher.changed().await {
                    Ok(bytes) => callback(bytes),
                    Err(_) => break,
                }
            }
        });
    }

    /// 订阅命令队列（外部可以监听最近一条命令）
    pub fn subscribe_commands<F>(&self, callback: F)
    where
        F: Fn(&ControlCommand) + Send + 'static,
    {
        let mut watcher = self.reactive_state.command_queue.watch();

        tokio::spawn(async move {
            loop {
                match watcher.changed().await {
                    Ok(Some(cmd)) => callback(&cmd),
                    Ok(None) => {}
                    Err(_) => break,
                }
            }
        });
    }
}

