//! 分片下载：发起单段 Range 请求，返回响应供流式读取。

use reqwest::header::RANGE;
use reqwest::{Client, Response};

use super::super::error::DownloadError;

/// 发起 Range 请求时的参数（形参超过 3 个时用 struct 承载）。
pub struct FetchRangeParams<'a> {
    pub client: &'a Client,
    pub url: &'a str,
    pub range: &'a str,
}

/// 发起单段 Range GET 请求，返回响应体供调用方做 `bytes_stream()`。
pub async fn fetch_range_response(
    params: FetchRangeParams<'_>,
) -> Result<Response, DownloadError> {
    let resp = params
        .client
        .get(params.url)
        .header(RANGE, params.range)
        .send()
        .await?;
    Ok(resp)
}
