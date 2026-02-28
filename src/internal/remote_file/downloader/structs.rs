pub mod byte_segments;
pub mod control_command;
pub mod download_error;
pub mod download_mode;
pub mod download_result;
pub mod download_status;
pub mod reactive_state;
pub mod remote_downloader;
pub mod remote_downloader_config;
pub mod remote_downloader_controller;

// 重导出公共类型
pub use byte_segments::{ByteSegment, ByteSegments};
pub use control_command::ControlCommand;
pub use download_error::DownloadError;
pub use download_mode::DownloadMode;
pub use download_result::DownloadResult;
pub use download_status::DownloadStatus;
pub use remote_downloader::RemoteDownloader;
pub use remote_downloader_controller::RemoteDownloaderController;