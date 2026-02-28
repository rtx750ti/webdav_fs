#[derive(Debug, Clone)]
pub enum DownloadMode {
    SaveFile(String),
    OutputBytes,
}

