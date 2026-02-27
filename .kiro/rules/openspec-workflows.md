# OpenSpec 工作流规则

本文档定义 Kiro/Augment 在处理 OpenSpec 相关命令时的行为规则。

## 核心理念

OpenSpec 是**工件驱动的开发工作流**，通过结构化的 artifacts（提案、规范、设计、任务）管理变更：
- **思考先于实现**（explore → plan → implement）
- **决策可追溯**（所有 artifacts 归档保存）
- **灵活而非僵化**（可在任何阶段更新 artifacts）

## 通用护栏原则

1. **不猜测 change 名称**: 如果用户未明确提供，必须列出可用 changes 让用户选择
2. **Schema 感知**: 通过 `openspec status --json` 了解 schema，不假设固定结构
3. **Context 和 Rules 是约束**: 从 `openspec instructions` 获取的 `context` 和 `rules` 字段是给 AI 的指导，**绝不复制到 artifact 文件中**
4. **使用 template**: Artifact 文件应使用 CLI 返回的 `template` 字段作为结构
5. **读取依赖**: 创建 artifact 前始终读取 `dependencies` 中列出的已完成文件
6. **验证写入**: 写入文件后验证其存在再继续
7. **最小化代码变更**: 实现时保持变更聚焦和最小化
8. **暂停而非猜测**: 遇到不清晰的情况时暂停询问用户

## 命令触发词

用户可能使用以下任一形式触发命令：
- `/opsx:explore` 或 `/opsx-explore` 或 "opsx explore"
- `/opsx:new` 或 `/opsx-new` 或 "opsx new"
- `/opsx:ff` 或 `/opsx-ff` 或 "opsx ff" 或 "opsx fast-forward"
- `/opsx:continue` 或 `/opsx-continue` 或 "opsx continue"
- `/opsx:apply` 或 `/opsx-apply` 或 "opsx apply"
- `/opsx:verify` 或 `/opsx-verify` 或 "opsx verify"
- `/opsx:sync` 或 `/opsx-sync` 或 "opsx sync"
- `/opsx:archive` 或 `/opsx-archive` 或 "opsx archive"
- `/opsx:bulk-archive` 或 `/opsx-bulk-archive` 或 "opsx bulk archive"
- `/opsx:onboard` 或 `/opsx-onboard` 或 "opsx onboard"

## 命令详细规则

详见以下独立规则文件：
- `openspec-explore.md` - 探索模式
- `openspec-new.md` - 逐步创建 change
- `openspec-ff.md` - 快进创建所有 artifacts
- `openspec-continue.md` - 继续现有 change
- `openspec-apply.md` - 实现任务
- `openspec-verify.md` - 验证实现
- `openspec-sync.md` - 同步 delta specs
- `openspec-archive.md` - 归档 change
- `openspec-bulk-archive.md` - 批量归档
- `openspec-onboard.md` - 引导式入门

## CLI 工具使用约定

### 常用命令模式

```bash
# 列出所有 changes
openspec list --json

# 检查 change 状态
openspec status --change "<name>" --json

# 创建新 change
openspec new change "<name>" [--schema <schema-name>]

# 获取 artifact 创建指令
openspec instructions <artifact-id> --change "<name>" --json

# 获取 apply 指令
openspec instructions apply --change "<name>" --json

# 归档 change
openspec archive "<name>"
```

### JSON 输出解析

**openspec status --json** 返回：
- `schemaName`: 使用的 workflow schema（如 "spec-driven"）
- `artifacts`: artifact 数组，每个包含 `id`, `status` ("done"/"ready"/"blocked"), `dependencies`
- `isComplete`: 布尔值，所有 artifacts 是否完成
- `applyRequires`: 实现前需要的 artifact IDs

**openspec instructions <artifact-id> --json** 返回：
- `context`: 项目背景（约束，不输出到文件）
- `rules`: Artifact 规则（约束，不输出到文件）
- `template`: 文件结构模板（用于输出）
- `instruction`: Schema 特定指导
- `outputPath`: 写入路径
- `dependencies`: 依赖的已完成 artifacts

**openspec list --json** 返回：
- 数组，每个 change 包含 `name`, `schema`, `lastModified`, `status`

## Artifact 创建指导

### Spec-driven Schema 的 Artifacts

1. **proposal.md**: 
   - Why（为什么做）
   - What Changes（改变什么）
   - Capabilities（能力列表，每个需对应 spec 文件）
   - Impact（影响的文件）

2. **specs/<capability>/spec.md**:
   - 每个 capability 一个 spec（用 capability 名，非 change 名）
   - 格式：Requirements → Scenarios (WHEN/THEN/AND)

3. **design.md**:
   - Context（当前状态）
   - Goals / Non-Goals
   - Decisions（技术决策）

4. **tasks.md**:
   - 分组的复选框任务
   - `- [ ]` 未完成，`- [x]` 已完成

### Delta Spec 格式

Delta specs 位于 `openspec/changes/<name>/specs/<capability>/spec.md`，包含：
- `## ADDED Requirements` - 新增需求
- `## MODIFIED Requirements` - 修改现有需求（可部分更新，如只添加 scenario）
- `## REMOVED Requirements` - 删除需求
- `## RENAMED Requirements` - 重命名（FROM:/TO: 格式）

## 错误处理

- 如果 `openspec` 命令不存在，提示用户安装或初始化
- 如果 change 不存在，列出可用 changes 或建议创建新的
- 如果 artifact 被阻塞（dependencies 未满足），说明原因并建议下一步
- 如果文件写入失败，报告错误并等待用户指示

## 输出风格

- 使用中文与用户交流
- 简洁直接，避免冗长总结
- 使用 ASCII 图表辅助说明（特别是 explore 模式）
- 进度更新简短："✓ Created proposal"
- 完成时提供下一步建议，不强制推进

