# .cursor 目录说明

本目录存放 Cursor 对本项目的配置与“记忆”，便于 AI 在编辑时保持上下文一致。

## 内容

- **rules/** — 规则文件（`.mdc`）
  - `rust-project.mdc`：全局 Rust/项目约定（alwaysApply）
  - `project-structure.mdc`：模块与公开 API 结构（编辑 `src/**/*.rs` 时适用）
  - `code-style.mdc`：代码风格、错误与文档约定（编辑 `src/**/*.rs` 时适用）
  - `tests.mdc`：测试约定（编辑 `src/tests/**/*.rs` 时适用）

## 项目级记忆

仓库根目录的 **AGENTS.md** 是给 Agent 的项目记忆与指引，包含目录结构、技术栈、领域约定和修改注意点。编辑前可优先参考该文件。
