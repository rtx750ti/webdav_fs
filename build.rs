//! 构建时根据 src/tests/vendors.toml 生成测试用厂商枚举与 env 变量名常量，供 tests 模块 include! 使用。

use std::env;
use std::fs;
use std::path::Path;

fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(|part| {
            let mut c = part.chars();
            match c.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().chain(c.flat_map(|c| c.to_lowercase())).collect(),
            }
        })
        .collect()
}

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let vendors_path = Path::new(&manifest_dir).join("src/tests/vendors.toml");
    println!("cargo:rerun-if-changed=src/tests/vendors.toml");
    println!("cargo:rerun-if-changed=src/tests/env/.env.example");
    // env 目录变化（含删除某厂商的 .env 文件）时也重新跑，以便从模板恢复缺失的 {id}.env
    println!("cargo:rerun-if-changed=src/tests/env");

    let vendor_ids: Vec<String> = if vendors_path.exists() {
        let content = fs::read_to_string(&vendors_path).unwrap_or_default();
        if content.trim().is_empty() {
            vec![]
        } else {
            parse_vendors_toml(&content).unwrap_or_else(|| parse_line_per_vendor(&content))
        }
    } else {
        vec![]
    };

    let out_dir = env::var("OUT_DIR").unwrap();
    let out_path = Path::new(&out_dir).join("test_vendors.rs");

    let mut enum_variants = String::new();
    let mut as_str_arms = String::new();
    let mut all_array = String::new();

    for (_i, id) in vendor_ids.iter().enumerate() {
        let id = id.trim();
        if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            continue;
        }
        let variant = to_pascal_case(id);
        if variant.is_empty() {
            continue;
        }
        enum_variants.push_str(&format!("    {},\n", variant));
        as_str_arms.push_str(&format!("            TestVendor::{} => \"{}\",\n", variant, id));
        all_array.push_str(&format!("            TestVendor::{},\n", variant));
    }

    // 若无任何厂商则生成一个占位变体，避免空枚举无法编译
    let (enum_variants, as_str_arms, all_array) = if enum_variants.is_empty() {
        (
            "    #[allow(dead_code)]\n    __None,\n".to_string(),
            "            TestVendor::__None => \"\",\n".to_string(),
            "            TestVendor::__None,\n".to_string(),
        )
    } else {
        (enum_variants, as_str_arms, all_array)
    };

    let code = format!(
        r#"// 自动生成，请勿手改。厂商列表来自 src/tests/vendors.toml

/// 测试可选的 WebDAV 厂商，用于在测试中手动选择用哪个 env 配置。
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestVendor {{
{variants}
}}

impl TestVendor {{
    /// 返回厂商 id（与 env 文件名 {{id}}.env 对应）。
    pub fn as_str(&self) -> &'static str {{
        match self {{
{as_str}
        }}
    }}

    /// 返回所有已配置的厂商，便于多厂商轮询测试。
    #[allow(dead_code)]
    pub fn all() -> &'static [TestVendor] {{
        static ALL: &[TestVendor] = &[
{array}
        ];
        ALL
    }}
}}

/// 测试 env 文件中所需的环境变量名，便于 IDE 补全与文档。
#[allow(dead_code)]
pub mod env_var_names {{
    /// WebDAV 根 URL（与 base_url 一致，建议以 / 结尾）
    pub const WEBDAV_URL: &str = "WEBDAV_URL";
    /// 用户名
    pub const WEBDAV_USERNAME: &str = "WEBDAV_USERNAME";
    /// 密码
    pub const WEBDAV_PASSWORD: &str = "WEBDAV_PASSWORD";
}}
"#,
        variants = enum_variants,
        as_str = as_str_arms,
        array = all_array,
    );

    fs::write(out_path, code).expect("write test_vendors.rs");

    // 根据 vendors 列表自动生成缺失的 env 文件（从 .env.example 复制），用户只需填写 URL/账号/密码
    let env_dir = Path::new(&manifest_dir).join("src/tests/env");
    let example_path = env_dir.join(".env.example");
    if example_path.exists() {
        let template = fs::read_to_string(&example_path).unwrap_or_default();
        for id in &vendor_ids {
            let id = id.trim();
            if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                continue;
            }
            let env_file = env_dir.join(format!("{}.env", id));
            if !env_file.exists() {
                fs::create_dir_all(&env_dir).ok();
                fs::write(&env_file, &template).expect("write env file");
            }
        }
    }
}

/// 解析 vendors.toml：支持 vendors = ["a","b"] 或多行 vendors = [ "a", "b" ]
fn parse_vendors_toml(content: &str) -> Option<Vec<String>> {
    let start = content.find("vendors")?;
    let after_key = &content[start + 7..];
    let open = after_key.find('[')?;
    let array_start = start + 7 + open;
    let mut depth = 1u32;
    let mut i = array_start + 1;
    let bytes = content.as_bytes();
    while i < content.len() && depth > 0 {
        match bytes.get(i) {
            Some(b'[') => depth += 1,
            Some(b']') => depth -= 1,
            _ => {}
        }
        i += 1;
    }
    if depth != 0 {
        return None;
    }
    let inner = content[array_start + 1..i - 1].trim();
    let ids: Vec<String> = inner
        .split(',')
        .map(|s| s.trim().trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if ids.is_empty() {
        None
    } else {
        Some(ids)
    }
}

fn parse_line_per_vendor(content: &str) -> Vec<String> {
    content
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|s| !s.is_empty() && !s.starts_with('#'))
        .collect()
}
