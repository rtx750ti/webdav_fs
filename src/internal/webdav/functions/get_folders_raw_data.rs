use quick_xml::de::from_str;
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};

use crate::auth::structs::webdav_auth::WebdavAuth;
use crate::internal::webdav::enums::{Depth, WebDavMethod};
use crate::webdav::structs::MultiStatus;

/// 内部使用的PROPFIND请求体
const _PROPFIND_BODY: &str = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:propfind xmlns:D="DAV:">
  <D:allprop/>
</D:propfind>"#;

/// 获取原始webdav文件夹数据
pub(crate) async fn get_folders_raw_data(
    webdav_auth: &WebdavAuth,
    absolute_url: &str,
    depth: &Depth,
) -> Result<MultiStatus, String> {
    // 组装请求头
    let mut headers = HeaderMap::new();
    headers
        .insert(CONTENT_TYPE, HeaderValue::from_static("application/xml"));
    headers.insert("Depth", HeaderValue::from_static(depth.as_str()));
    headers.insert("Accept", HeaderValue::from_static("application/xml"));

    let method = WebDavMethod::PROPFIND
        .to_head_method()
        .map_err(|e| e.to_string())?;

    let http_client = &webdav_auth.client;

    // 发送 PROPFIND 到基准目录（已保证有尾部斜杠）
    let res = http_client
        .request(method, absolute_url)
        .headers(headers)
        .body(_PROPFIND_BODY)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let status = res.status();

    let xml_text = res.text().await.map_err(|e| e.to_string())?;

    if !status.is_success() && status.as_u16() != 207 {
        return Err(format!(
            "状态解析异常 {status}: {xml}",
            status = status,
            xml = xml_text
        ));
    }

    let multi_status: MultiStatus =
        from_str(&xml_text).map_err(|e| e.to_string())?;

    Ok(multi_status)
}
