//! 测试公共模块：env 多厂商配置与加载。
//!
//! - **只需改 toml**：在 `src/tests/vendors.toml` 的 `vendors` 数组中加入厂商 id，保存后执行 `cargo build` 或 `cargo test`，会自动生成 `TestVendor` 枚举与 `env/{id}.env` 文件。
//! - **只填 env 内容**：在自动生成的 `env/{id}.env` 中填写 `WEBDAV_URL`、`WEBDAV_USERNAME`、`WEBDAV_PASSWORD` 即可，变量名见 `env_var_names` 模块。
//! - **测试时选厂商**：使用 `load_account_optional(TestVendor::Xxx)` 等，IDE 有枚举补全；env 文件已由 `.gitignore` 忽略，勿提交含真实密码的文件。

#[cfg(test)]
include!(concat!(env!("OUT_DIR"), "/test_vendors.rs"));

#[cfg(test)]
use std::path::PathBuf;

#[cfg(test)]
use dotenvy::from_filename_override;
#[cfg(test)]
use std::env;

#[cfg(test)]
#[derive(Debug)]
pub struct WebDavAccount {
    pub url: String,
    pub username: String,
    pub password: String,
}

#[cfg(test)]
impl WebDavAccount {
    /// 转为 `WebdavAuth`，便于在测试中调用远程 API。
    pub fn to_webdav_auth(&self) -> Result<crate::internal::auth::structs::webdav_auth::WebdavAuth, String> {
        crate::internal::auth::structs::webdav_auth::WebdavAuth::new(
            &self.username,
            &self.password,
            &self.url,
        )
    }
}

/// 返回该厂商对应的 env 文件路径（`{manifest_dir}/src/tests/env/{vendor}.env`）。
#[cfg(test)]
pub fn env_path(vendor: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/tests/env")
        .join(format!("{}.env", vendor))
}

/// 按厂商加载账号；失败时返回可读错误（如文件不存在或缺少变量）。
#[cfg(test)]
#[allow(dead_code)]
pub fn load_account(v: TestVendor) -> Result<WebDavAccount, String> {
    let path = env_path(v.as_str());
    if !path.exists() {
        return Err(format!("env 文件不存在: {}", path.display()));
    }
    from_filename_override(&path).map_err(|e| format!("加载 env 失败: {}", e))?;
    let url = env::var("WEBDAV_URL").map_err(|_| "缺少 WEBDAV_URL")?;
    let username = env::var("WEBDAV_USERNAME").map_err(|_| "缺少 WEBDAV_USERNAME")?;
    let password = env::var("WEBDAV_PASSWORD").map_err(|_| "缺少 WEBDAV_PASSWORD")?;
    Ok(WebDavAccount {
        url,
        username,
        password,
    })
}

/// 按厂商加载账号；文件不存在或缺少变量时返回 `None`，便于“有则跑、无则跳过”的测试。
#[cfg(test)]
pub fn load_account_optional(v: TestVendor) -> Option<WebDavAccount> {
    let path = env_path(v.as_str());
    if !path.exists() {
        return None;
    }
    from_filename_override(&path).ok()?;
    let url = env::var("WEBDAV_URL").ok()?;
    let username = env::var("WEBDAV_USERNAME").ok()?;
    let password = env::var("WEBDAV_PASSWORD").ok()?;
    Some(WebDavAccount {
        url,
        username,
        password,
    })
}

#[cfg(test)]
#[allow(dead_code)]
pub fn assert_test_result(
    ok_count: usize,
    err_count: usize,
    expected_ok_count: usize,
    expected_err_count: usize,
    test_name: &str,
) {
    println!("统计结果：正确 {} 个，错误 {} 个", ok_count, err_count);

    if ok_count == expected_ok_count && err_count == expected_err_count {
        println!("测试结果: OK ✅");
    } else {
        panic!("❌ 测试异常[{}]：统计数量不匹配", test_name);
    }
}
