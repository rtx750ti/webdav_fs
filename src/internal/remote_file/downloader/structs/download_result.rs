/// 单次下载的结果。
#[derive(Debug)]
pub enum DownloadResult {
    /// 已保存到本地文件
    Saved,
    /// 单线程下载得到的完整字节（开启 output_bytes 时）
    Bytes(Vec<u8>),
}
