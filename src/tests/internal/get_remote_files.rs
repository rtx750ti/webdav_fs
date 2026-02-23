use crate::{
    get_remote_files,
    tests::{load_account_optional, TestVendor},
};

#[tokio::test]
async fn get_remote_files_test() {
    let auth = load_account_optional(TestVendor::Teracloud)
        .unwrap()
        .to_webdav_auth()
        .unwrap();
    let data = get_remote_files(&auth, &["./"]).await;
    for d in data {
        let remote_file = d.unwrap();
        println!("remote_file: {:?}", remote_file);
    }
}
