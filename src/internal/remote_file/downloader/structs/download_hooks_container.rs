use std::sync::atomic::{AtomicBool, Ordering};

use crate::internal::remote_file::downloader::traits::download::{DownloadHook, HookAbort};

/// 钩子容器：多个钩子 + 请求取消标志。
#[derive(Default)]
pub struct DownloadHooksContainer {
    hooks: Vec<Box<dyn DownloadHook>>,
    cancel_requested: AtomicBool,
}

impl DownloadHooksContainer {
    /// 添加一个下载钩子；支持多次调用以注册多个钩子，按添加顺序依次执行。
    pub fn add(&mut self, hook: impl DownloadHook + 'static) {
        self.hooks.push(Box::new(hook));
    }

    pub fn request_cancel(&self) {
        self.cancel_requested.store(true, Ordering::Relaxed);
    }

    pub fn cancel_requested(&self) -> bool {
        self.cancel_requested.load(Ordering::Relaxed)
    }

    pub async fn run_before_start(
        &mut self,
    ) -> Result<(), HookAbort> {
        for h in self.hooks.iter_mut() {
            h.before_start().await?;
        }
        Ok(())
    }

    pub fn run_on_chunk(&mut self, chunk: &[u8]) {
        for h in self.hooks.iter_mut() {
            h.on_chunk(chunk);
        }
    }

    pub fn run_on_progress(
        &mut self,
        bytes_done: u64,
        total: Option<u64>,
    ) {
        for h in self.hooks.iter_mut() {
            h.on_progress(bytes_done, total);
        }
    }

    pub async fn run_after_complete(&mut self) {
        for h in self.hooks.iter_mut() {
            h.after_complete().await;
        }
    }
}
