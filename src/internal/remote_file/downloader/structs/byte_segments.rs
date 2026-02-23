//! 多线程下载得到的分段字节：按 offset 排序，支持按偏移读取。

/// 单段字节：在整体中的起始偏移及其数据。
#[derive(Debug, Clone)]
pub struct ByteSegment {
    /// 该段在整体中的起始偏移（字节）
    pub offset: u64,
    /// 该段的数据
    pub data: Vec<u8>,
}

/// 多线程下载得到的分段字节。各段按 `offset` 升序，不重叠且连续覆盖 `[0, total_len)`。
#[derive(Debug, Clone)]
pub struct ByteSegments {
    /// 按 offset 升序的分段列表
    segments: Vec<ByteSegment>,
    /// 总字节数（最后一段的 offset + len，或 0）
    total_len: u64,
}

impl ByteSegments {
    /// 从已按 offset 升序且不重叠的分段列表构建。不校验连续性，由调用方保证。
    pub fn new(segments: Vec<ByteSegment>) -> Self {
        let total_len = segments
            .last()
            .map(|s| s.offset.saturating_add(s.data.len() as u64))
            .unwrap_or(0);
        Self {
            segments,
            total_len,
        }
    }

    /// 总字节数。
    pub fn total_len(&self) -> u64 {
        self.total_len
    }

    /// 按偏移读取一段：从 `offset` 起最多读 `len` 字节，返回新分配的字节。
    /// 若 `offset >= total_len` 返回空 Vec；若 `offset + len` 超出末尾则只读到末尾。
    pub fn read_at(&self, offset: u64, len: usize) -> Vec<u8> {
        if offset >= self.total_len || len == 0 {
            return Vec::new();
        }
        let mut out = Vec::with_capacity(len.min((self.total_len - offset) as usize));
        let end = offset.saturating_add(len as u64).min(self.total_len);

        for seg in &self.segments {
            let seg_end = seg.offset + seg.data.len() as u64;
            if seg_end <= offset || seg.offset >= end {
                continue;
            }
            let read_start = (offset.max(seg.offset) - seg.offset) as usize;
            let read_end = (end.min(seg_end) - seg.offset) as usize;
            out.extend_from_slice(&seg.data[read_start..read_end]);
        }
        out
    }
}
