use crate::{
    auth::structs::webdav_auth::WebdavAuth,
    remote_file::RemoteFileData,
    webdav::{structs::MultiStatus, traits::ToRemoteFileData},
};

#[derive(Debug, Clone)]
pub struct RemoteFile {
    pub data: RemoteFileData,
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
                data: remote_file_data.clone(),
            })
            .collect::<Vec<Self>>();

        Ok(files)
    }
}
