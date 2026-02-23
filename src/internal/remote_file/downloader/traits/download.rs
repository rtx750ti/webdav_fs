//! 下载相关 trait：钩子接口，供下载器领域模块调用。
//!
//! 下载器由远程文件创建并执行下载；对外使用入口为 [`crate::remote_file`]。

use async_trait::async_trait;

/// 钩子执行时请求中止下载时使用的错误。
#[derive(Debug, Clone)]
pub struct HookAbort;

impl std::fmt::Display for HookAbort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("下载被钩子中止")
    }
}

impl std::error::Error for HookAbort {}

/// 下载流程钩子：在「开始前 / 进度 / 每块数据 / 完成后」插入自定义逻辑。
///
/// 使用方式二选一（可混用）：
/// - **单阶段**：用 `with_before_start_hook` / `with_on_chunk_hook` / `with_on_progress_hook` / `with_after_complete_hook` 传入闭包；
/// - **完整钩子**：实现本 trait，通过下载器的 `with_hook` 注册。
#[async_trait]
pub trait DownloadHook: Send + Sync {
    /// 下载开始前调用（如：加锁、校验路径）。返回 `Err` 则中止本次下载。
    async fn before_start(&mut self) -> Result<(), HookAbort> {
        Ok(())
    }

    /// 每收到一段数据时调用（流式下载时可用）。`chunk` 为本段字节。
    fn on_chunk(&mut self, _chunk: &[u8]) {}

    /// 进度更新（累计已下载字节、总大小）。由下载器在写盘或拉流时调用。
    fn on_progress(&mut self, _bytes_done: u64, _total: Option<u64>) {}

    /// 下载成功结束后调用（清理、解锁等）。
    async fn after_complete(&mut self) {}
}
