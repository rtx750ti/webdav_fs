use std::sync::Arc;

use crate::{
    auth::structs::webdav_auth::WebdavAuth,
    remote_file::RemoteFileData,
    webdav::{structs::MultiStatus, traits::ToRemoteFileData},
};

use crate::internal::remote_file::downloader::structs::RemoteDownloader;

#[derive(Debug, Clone)]
pub struct RemoteFile {
    pub data: Arc<RemoteFileData>, // 使用 Arc 以支持多线程共享
    pub webdav_auth: WebdavAuth,
}

impl RemoteFile {
    pub fn from_multi_status(
        webdav_auth: &WebdavAuth,
        multi_status: MultiStatus,
    ) -> Result<Vec<Self>, String> {
        let resources = multi_status
            .to_remote_file_data(&webdav_auth.base_url)
            .map_err(|e| e)?;

        let files = resources
            .iter()
            .map(|remote_file_data| Self {
                data: Arc::new(remote_file_data.clone()),
                webdav_auth: webdav_auth.clone(),
            })
            .collect::<Vec<Self>>();

        Ok(files)
    }

    /// 构建一个空的下载器
    pub fn build_downloader(&self) -> RemoteDownloader {
        RemoteDownloader::new(self.data.clone(), self.webdav_auth.clone())
    }

    /// 创建下载器（便捷方法）
    pub fn download(&self, auth: WebdavAuth) -> RemoteDownloader {
        RemoteDownloader::new(self.data.clone(), auth)
    }
}
