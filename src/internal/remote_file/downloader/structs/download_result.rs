use super::byte_segments::ByteSegments;

/// 单次下载的结果。
#[derive(Debug)]
pub enum DownloadResult {
    /// 已保存到本地文件
    SavedToLocal(String),
    /// 单线程下载得到的完整字节
    Bytes(Vec<u8>),
    /// 多线程下载得到的分段字节，按 offset 可寻址
    ByteSegments(ByteSegments),
}

