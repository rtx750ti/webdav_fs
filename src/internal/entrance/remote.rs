use crate::{
    auth::structs::webdav_auth::WebdavAuth,
    webdav::{
        enums::Depth,
        functions::get_folders_raw_data::get_folders_raw_data,
    },
};

/// 读取远程文件，并生成领域结构体模型
///
/// 支持文件夹和文件混合读取，不会做递归处理，所以需要递归请自行处理
///
/// - 注意：relative_urls是基于webdav_auth中的base_url的，所以不建议以"/"开头
///
/// example:
/// ```
/// use webdav_fs::auth::structs::webdav_auth::WebdavAuth;
/// use webdav_fs::internal::entrance::remote::read_remote_folders;
///
/// let webdav_auth = WebdavAuth::new(
///     "http://localhost:8080/".to_string(),
///     "account".to_string(),
///     "password".to_string(),
/// );
///
/// let urls = vec!["./t1","./t2/a1.txt","./t2/a2.txt","./t3"];
///
/// let folders = read_remote_folders(&webdav_auth, &urls).unwrap();
/// ```
pub async fn read_remote_folders(
    webdav_auth: &WebdavAuth,
    relative_urls: &[&str],
) -> Result<Vec<String>, String> {
    let a = get_folders_raw_data(&webdav_auth, "", &Depth::Zero).await?;
    Ok(Vec::new())
}

/// 获取远程WebDav服务器上的文件夹树
///
/// `relative_url`参数可选，未设置时默认读取webdav_auth中的base_url
///
/// - 注意1：relative_url是基于webdav_auth中的base_url的，所以不建议以"/"开头
/// - 注意2：只能读取一层目录
pub async fn read_remote_folders_tree(
    webdav_auth: &WebdavAuth,
    relative_url: Option<&str>,
) -> Result<Vec<String>, String> {
    if let Some(relative_url) = relative_url {
        let a = get_folders_raw_data(
            webdav_auth,
            relative_url,
            &Depth::One, // 这里只读取一级，避免出现递归问题
        )
        .await?;
    }

    Ok(Vec::new())
}
