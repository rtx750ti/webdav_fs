//! 单阶段钩子适配器：将闭包包装成 [`DownloadHook`]，供 `with_xx_hook` 使用。

use std::future::Future;

use async_trait::async_trait;

use crate::remote_file::download::{DownloadHook, HookAbort};

/// 仅实现「开始前」的钩子适配器。
pub(crate) struct BeforeStartHookAdapter<F>(pub(crate) F);

#[async_trait]
impl<F, Fut> DownloadHook for BeforeStartHookAdapter<F>
where
    F: FnMut() -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<(), HookAbort>> + Send + 'static,
{
    async fn before_start(&mut self) -> Result<(), HookAbort> {
        (self.0)().await
    }
}

/// 仅实现「每块数据」的钩子适配器。
pub(crate) struct OnChunkHookAdapter<F>(pub(crate) F);

#[async_trait]
impl<F> DownloadHook for OnChunkHookAdapter<F>
where
    F: FnMut(&[u8]) + Send + Sync + 'static,
{
    fn on_chunk(&mut self, chunk: &[u8]) {
        (self.0)(chunk);
    }
}

/// 仅实现「进度」的钩子适配器。
pub(crate) struct OnProgressHookAdapter<F>(pub(crate) F);

#[async_trait]
impl<F> DownloadHook for OnProgressHookAdapter<F>
where
    F: FnMut(u64, Option<u64>) + Send + Sync + 'static,
{
    fn on_progress(&mut self, bytes_done: u64, total: Option<u64>) {
        (self.0)(bytes_done, total);
    }
}

/// 仅实现「完成后」的钩子适配器。
pub(crate) struct AfterCompleteHookAdapter<F>(pub(crate) F);

#[async_trait]
impl<F, Fut> DownloadHook for AfterCompleteHookAdapter<F>
where
    F: FnMut() -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    async fn after_complete(&mut self) {
        (self.0)().await
    }
}
