#[tokio::test]
async fn get_remote_files_test() {
    let webdav_auth = WebdavAuth::new(
        "https://example.com/dav/",
        "your_username",
        "your_password",
    );
}
