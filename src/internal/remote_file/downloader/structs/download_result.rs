use super::byte_segments::ByteSegments;

/// 单次下载的结果。
#[derive(Debug)]
pub enum DownloadResult {
    /// 已保存到本地文件
    Saved,
    /// 单线程下载得到的完整字节（开启 output_bytes 时）
    Bytes(Vec<u8>),
    /// 多线程下载得到的分段字节，按 offset 可寻址（开启 output_bytes 且多线程时）
    BytesSegments(ByteSegments),
}
