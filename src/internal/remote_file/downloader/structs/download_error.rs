//! 下载相关错误类型。

use thiserror::Error;

#[derive(Debug, Error)]
pub enum DownloadError {
    #[error("HTTP 请求失败: {0}")]
    Request(#[from] reqwest::Error),

    #[error("创建文件失败: {0}")]
    CreateFile(std::io::Error),

    #[error("写入文件失败: {0}")]
    WriteFile(tokio::io::Error),

    #[error("刷新文件失败: {0}")]
    FlushFile(tokio::io::Error),

    #[error("文件定位失败: {0}")]
    SeekFile(tokio::io::Error),

    #[error("仅支持文件下载，当前为目录")]
    IsDir,

    #[error("未设置保存路径且未开启 output_bytes")]
    NoDestination,

    #[error("下载被取消")]
    Cancelled,

    #[error("下载被暂停")]
    Paused,

    #[error("分片下载需要已知文件大小")]
    UnknownFileSizeForChunked,

    #[error("分片任务失败: {0}")]
    TaskJoin(#[from] tokio::task::JoinError),

    #[error("分片下载内部错误: {0}")]
    ChunkedInternal(String),

    #[error("克隆文件句柄失败: {0}")]
    CloneFile(std::io::Error),

    #[error("获取控制器锁失败")]
    ControllerLockFailed,

    #[error("获取命令消费者锁失败")]
    ConsumerLockFailed,

    #[error("删除临时文件失败: {0}")]
    RemoveTempFile(std::io::Error),

    #[error("预分配文件空间失败: {0}")]
    PreallocateFile(std::io::Error),

    #[error("分片 {chunk_index} 下载失败，已重试 {retries} 次: {message}")]
    ChunkFailed {
        chunk_index: usize,
        retries: usize,
        message: String,
    },

    #[error("多个分片下载失败: {0:?}")]
    MultipleChunksFailed(Vec<String>),

    #[error("服务器不支持 Range 请求")]
    RangeNotSupported,
}

