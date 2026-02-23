//! 下载器领域模块：专属于远程文件的下载器结构体，由远程文件主动创建并执行下载。
//!
//! 使用方式：`remote_file.build_downloader(auth).save_to(path).with_hook(hook).send().await`
//! 对外导出以 [`crate::remote_file`] 为准，此处仅做模块划分，不重复 pub use。

pub mod structs;
pub mod traits;