use crate::internal::entrance::remote::get_remote_files;
use crate::tests::{load_account_optional, TestVendor};

#[tokio::test]
async fn get_remote_files_test() {
    let account = match load_account_optional(TestVendor::Jianguoyun) {
        Some(a) => a,
        None => return,
    };
    let auth = match account.to_webdav_auth() {
        Ok(a) => a,
        Err(_) => return,
    };
    let _ = get_remote_files(&auth, &[""]).await;
}
