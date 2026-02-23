//! 下载相关错误类型。

use thiserror::Error;

use crate::internal::remote_file::downloader::traits::download::HookAbort;

#[derive(Debug, Error)]
pub enum DownloadError {
    #[error("HTTP 请求失败: {0}")]
    Request(#[from] reqwest::Error),

    #[error("创建文件失败: {0}")]
    CreateFile(std::io::Error),

    #[error("写入文件失败: {0}")]
    WriteFile(tokio::io::Error),

    #[error("仅支持文件下载，当前为目录")]
    IsDir,

    #[error("未设置保存路径且未开启 output_bytes")]
    NoDestination,

    #[error("下载被取消")]
    Cancelled,

    /// 分片下载需要已知文件大小。
    #[error("分片下载需要已知文件大小")]
    UnknownFileSizeForChunked,

    /// 分片任务 join 失败。
    #[error("分片任务失败: {0}")]
    TaskJoin(#[from] tokio::task::JoinError),

    #[error("分片下载内部错误: {0}")]
    ChunkedInternal(String),

    /// 钩子在 before_start 中返回错误，中止下载。
    #[error("{0}")]
    HookAbort(#[from] HookAbort),
}
