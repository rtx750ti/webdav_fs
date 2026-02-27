use crate::get_remote_files;
use crate::tests::{TestVendor, load_account_optional};

/// Windows 下本地保存目录
const SAVE_DIR: &str = r"C:\project\rust\quick-sync\temp-download-files";

/// 获取一个远程文件（非目录），无则返回 None（跳过测试）。
async fn require_one_remote_file()
-> Option<(crate::remote_file::RemoteFile, crate::auth::WebdavAuth)> {
    let auth = load_account_optional(TestVendor::Teracloud)?
        .to_webdav_auth()
        .ok()?;
    let results =
        get_remote_files(&auth, &["./新建文件夹/hula.exe"]).await;
    let file = results
        .into_iter()
        .find_map(|r| r.ok())
        .filter(|f| !f.data.is_dir)?;
    Some((file, auth))
}
