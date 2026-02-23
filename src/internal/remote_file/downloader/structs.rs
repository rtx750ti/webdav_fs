pub mod byte_segments;
pub mod download_config;
pub mod download_hooks_container;
pub mod download_progress;
pub mod download_result;
pub(crate) mod hook_adapters;
pub mod remote_file_downloader;

pub use byte_segments::{ByteSegment, ByteSegments};
pub use download_config::DownloadConfig;
pub use download_hooks_container::DownloadHooksContainer;
pub use download_progress::DownloadProgress;
pub use download_result::DownloadResult;
pub use remote_file_downloader::RemoteFileDownloader;
