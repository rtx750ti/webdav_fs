use core::fmt;
use std::rc::Rc;

use base64::Engine;
use reqwest::{
    header::{HeaderMap, HeaderValue, AUTHORIZATION},
    Client,
};
use sha2::{Digest, Sha256};
use url::Url;

/// 认证结构体
/// 
/// 该结构体定位
/// - 用于存储基础WebDav认证信息
/// - 用于RemoteFile和LocalFile的网络访问功能支持
/// 
/// 默认Eq时会匹配base_url和token，如果需要单独比较token，需使用eq_only_token方法
#[derive(Clone)]
pub struct WebdavAuth {
    pub client: Client,    // 内部是Arc，不需要特殊处理
    pub base_url: Rc<Url>, // Rc避免深拷贝，不需要用Arc，一般也没人会改它
    pub(crate) encrypted_token: Rc<String>, // 对外导出时，不允许直接访问，哪怕它是被加密的
}

impl WebdavAuth {
    /// 创建新的认证结构体
    pub fn new(
        username: &str,
        password: &str,
        base_url: &str,
    ) -> Result<Self, String> {
        let http_client =
            _InternalHttpClient::_create(username, password)?;

        let base_url =
            _format_base_url(base_url).map_err(|e| e.to_string())?;

        Ok(Self {
            client: http_client.client,
            base_url: Rc::new(base_url),
            encrypted_token: Rc::new(http_client.encrypted_token),
        })
    }

    /// 仅比较token是否相等
    pub fn eq_only_token(&self, other: &Self) -> bool {
        self.encrypted_token == other.encrypted_token
    }
}

/// 用于比较认证结构体是否相等
impl PartialEq for WebdavAuth {
    fn eq(&self, other: &Self) -> bool {
        self.encrypted_token == other.encrypted_token
            && self.base_url == other.base_url
    }
}

/// 防止debug泄漏账号
impl fmt::Debug for WebdavAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WebdavAuth")
            .field("client", &"<Client with hidden authorization>")
            .finish()
    }
}

fn _format_base_url(url: &str) -> Result<Url, String> {
    if url.is_empty() {
        return Err("路径为空".to_string());
    }

    let mut base_url = Url::parse(url).map_err(|e| e.to_string())?;

    if !base_url.path().ends_with('/') {
        let new_path = format!("{}/", base_url.path());
        base_url.set_path(&new_path);
    }

    Ok(base_url)
}

/// 内部临时使用的http客户端结构体，在初始化WebdavAuth时使用
struct _InternalHttpClient {
    client: Client,
    encrypted_token: String,
}

impl _InternalHttpClient {
    fn _encrypt_str(data: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data.as_bytes());
        format!("{:x}", hasher.finalize())
    }
    /// 创建http客户端，内部使用
    fn _create(username: &str, password: &str) -> Result<Self, String> {
        let mut headers = HeaderMap::new();

        let token = base64::engine::general_purpose::STANDARD
            .encode(format!("{username}:{password}"));

        let auth_value =
            HeaderValue::from_str(&format!("Basic {}", token))
                .map_err(|e| e.to_string())?;

        headers.insert(AUTHORIZATION, auth_value);

        let http_client = Client::builder()
            .http1_only()
            .default_headers(headers)
            .build()
            .map_err(|e| e.to_string())?;

        let encrypted_token = Self::_encrypt_str(&token);

        Ok(Self { client: http_client, encrypted_token })
    }
}
