use reqwest::Method;

pub enum WebDavMethod {
    PROPFIND,
}

impl WebDavMethod {
    pub fn to_string(&self) -> String {
        match self {
            WebDavMethod::PROPFIND => "PROPFIND".to_string(),
        }
    }

    pub fn to_head_method(&self) -> Result<Method, String> {
        let method =
            reqwest::Method::from_bytes(self.to_string().as_bytes())
                .map_err(|e| e.to_string())?;

        match self {
            WebDavMethod::PROPFIND => Ok(method),
        }
    }
}

pub enum Depth {
    /// 仅返回当前资源
    Zero,
    /// 返回当前资源及直接子资源
    One,
    /// 返回当前资源及所有子资源（谨慎使用）
    Infinity,
}

impl Depth {
    pub fn as_str(&self) -> &'static str {
        match self {
            Depth::Zero => "0",
            Depth::One => "1",
            Depth::Infinity => "infinity",
        }
    }
}
