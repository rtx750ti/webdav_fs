# WebDAV FS — Agent 项目记忆与指引

本文档供 Cursor / AI 在编辑本仓库时作为项目级记忆与约定参考。

## 项目概览

- **名称**: `webdav_fs`
- **类型**: Rust 库，提供 WebDAV 文件系统相关能力（认证、列举目录、远程文件模型等）
- **对外**: 通过 `src/lib.rs` 重导出；内部实现集中在 `crate::internal`

## 目录与模块结构

```
src/
├── lib.rs              # 对外 API：重导出 internal 的 entrance、auth、webdav、states、remote_file、local_file
├── internal.rs         # 内部模块入口
├── internal/
│   ├── auth/           # 认证（WebdavAuth 等）
│   ├── entrance/       # 入口：lcoal（本地）、remote（远程）
│   ├── local_file/     # 本地文件领域
│   ├── remote_file/    # 远程文件领域（RemoteFile, RemoteFileData）
│   ├── states/         # lock_reactive, unlock_reactive
│   └── webdav/         # 原始 WebDAV：enums、functions、raw_xml、traits
└── tests/              # 集成/内部测试
```

- **约定**: 对外只暴露 `lib.rs` 中声明的模块与项；`internal` 为库内部实现，不对外承诺稳定。
- **命名注意**: 入口模块中当前存在拼写 `lcoal`（对应 local），与 `internal::entrance::lcoal` 及文件 `lcoal.rs` 一致；如需重命名为 `local` 需同步改模块名与文件名及所有引用。

## 技术栈与风格

- **异步**: tokio（rt-multi-thread, macros, sync, time, fs）
- **HTTP**: reqwest（rustls-tls, json, stream, gzip, cookies），且使用 `http1_only`
- **错误**: 多处使用 `Result<T, String>` 表示可展示错误；部分使用 thiserror
- **序列化**: serde + quick-xml（WebDAV XML）
- **文档**: 注释与文档以**中文**为主；公开 API 建议保留中文 doc comment 与 example

## 领域与 API 约定

- **WebdavAuth**: 构造为 `WebdavAuth::new(username, password, base_url)`；`base_url` 在内部会格式化为带尾部 `/` 的 URL。
- **路径**: 相对路径基于 `WebdavAuth::base_url`，**不建议**以 `/` 开头。
- **WebDAV 深度**: 使用 `Depth::Zero` / `Depth::One` / `Depth::Infinity`；列举目录时常用 `Depth::One` 仅一层，避免深层递归。
- **入口**: 远程入口在 `internal::entrance::remote`（如 `get_remote_files`, `get_remote_files_tree`）；本地入口在 `internal::entrance::lcoal`。

## 测试

- 集成测试在 `src/tests/`；使用 `#[tokio::test]` 与 `dotenvy` 等。
- 修改行为时请跑一遍 `cargo test`，确保未破坏现有测试。

## 修改代码时的注意点

1. 改公开 API 时同步更新 `lib.rs` 的 `pub use` 与文档。
2. 新增或修改 `internal` 子模块时，在 `internal.rs` 或对应父模块中声明。
3. 保持错误信息为可读字符串（中文亦可），便于调用方直接展示。
4. 涉及 URL 与路径时，注意与 `base_url` 的拼接与安全校验（如 `format_url_path` 中的逻辑）。
