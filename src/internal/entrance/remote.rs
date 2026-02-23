use futures_util::future::join_all;

use crate::{
    auth::structs::webdav_auth::WebdavAuth,
    remote_file::RemoteFile,
    webdav::{
        enums::Depth, functions::get_folders_raw_data,
        structs::MultiStatus,
    },
};

fn format_url_path(
    webdav_auth: &WebdavAuth,
    path: &str,
) -> Result<String, String> {
    let base_url = webdav_auth.base_url.clone();
    let joined_url =
        base_url.join(path).map_err(|_| "路径格式错误".to_string())?;

    if !joined_url.as_str().starts_with(base_url.as_str()) {
        return Err("路径格式错误".to_string());
    }

    if joined_url.scheme() != base_url.scheme()
        || joined_url.host_str() != base_url.host_str()
        || !joined_url.path().starts_with(base_url.path())
    {
        return Err("父目录不允许".to_string());
    }

    Ok(joined_url.to_string())
}

type WebDavTaskResult = Vec<Result<MultiStatus, String>>;

/// 读取远程文件，并转换成领域结构体模型
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
pub async fn get_remote_files(
    webdav_auth: &WebdavAuth,
    relative_urls: &[&str],
) -> Vec<Result<RemoteFile, String>> {
    let tasks = relative_urls.iter().map(|path| async move {
        let url = format_url_path(webdav_auth, path)?;
        let folders_raw_data =
            get_folders_raw_data(webdav_auth, &url, &Depth::Zero).await?;

        Ok(folders_raw_data)
    });

    // 并发获取全部的列表
    let fetched_webdav_task_results: WebDavTaskResult =
        join_all(tasks).await;

    let mut files_collection = Vec::new();

    for webdav_task_result in fetched_webdav_task_results {
        match webdav_task_result {
            Ok(multi_status) => {
                let from_multi_status_result =
                    RemoteFile::from_multi_status(
                        webdav_auth,
                        multi_status,
                    );

                match from_multi_status_result {
                    Ok(remote_files) => {
                        for remote_file in remote_files {
                            files_collection.push(Ok(remote_file));
                        }
                    }
                    Err(err) => files_collection.push(Err(err)),
                }
            }
            Err(e) => files_collection.push(Err(e)),
        }
    }

    files_collection
}

/// 获取远程WebDav服务器上的文件夹树
///
/// `relative_url`参数可选，未设置时默认读取webdav_auth中的base_url
///
/// - 注意1：relative_url是基于webdav_auth中的base_url的，所以不建议以"/"开头
/// - 注意2：只能读取一层目录
pub async fn get_remote_files_tree(
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
