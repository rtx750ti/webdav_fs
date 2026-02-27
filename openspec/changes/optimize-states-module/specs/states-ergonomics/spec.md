## ADDED Requirements

### Requirement: 统一错误类型体系

系统 SHALL 统一响应式属性的错误类型，将 `LockReactivePropertyError` 合并到 `ReactivePropertyError` 中。

#### Scenario: 使用统一的错误类型

- **WHEN** 任何响应式属性操作失败
- **THEN** 系统 SHALL 返回 `ReactivePropertyError` 类型的错误
- **AND** 系统 SHALL NOT 使用 `LockReactivePropertyError` 类型

#### Scenario: 错误类型包含所有必要的变体

- **WHEN** 定义 `ReactivePropertyError`
- **THEN** 系统 SHALL 包含 `WatcherClosed` 变体表示监听器已关闭
- **AND** 系统 SHALL 包含 `Destroyed` 变体表示属性已销毁
- **AND** 系统 SHALL 包含 `RecvError` 变体表示接收失败
- **AND** 所有变体 SHALL 提供清晰的中文错误消息

#### Scenario: 向后兼容的错误处理

- **WHEN** 现有代码使用 `LockReactivePropertyError`
- **THEN** 系统 SHALL 在 `lock_reactive.rs` 中提供类型别名以保持兼容性
- **AND** 文档 SHALL 标记该别名为 deprecated
- **AND** 文档 SHALL 引导用户迁移到 `ReactivePropertyError`

### Requirement: 提供便利方法 get_or_default

系统 SHALL 为 `ReactiveProperty` 和 `LockReactiveProperty` 提供 `get_or_default()` 方法。

#### Scenario: 获取值或返回默认值

- **WHEN** 调用 `get_or_default()`
- **THEN** 如果属性有值，系统 SHALL 返回该值的克隆
- **AND** 如果属性已销毁或无值，系统 SHALL 返回类型 `T` 的默认值
- **AND** 系统 SHALL 要求类型 `T` 实现 `Default` trait

#### Scenario: 简化错误处理

- **WHEN** 调用者不关心属性是否已销毁
- **THEN** 调用者 SHALL 可以使用 `get_or_default()` 而不是 `get_current().unwrap_or_default()`
- **AND** 代码 SHALL 更简洁易读

### Requirement: 提供便利方法 map

系统 SHALL 为 `ReactiveProperty` 和 `LockReactiveProperty` 提供 `map()` 方法，支持对当前值应用转换函数。

#### Scenario: 对当前值应用转换

- **WHEN** 调用 `map(|value| transform(value))`
- **THEN** 系统 SHALL 获取当前值
- **AND** 系统 SHALL 对值应用转换函数
- **AND** 系统 SHALL 返回 `Option<R>`，其中 `R` 是转换结果类型

#### Scenario: 处理属性已销毁的情况

- **WHEN** 属性已销毁时调用 `map()`
- **THEN** 系统 SHALL 返回 `None`
- **AND** 系统 SHALL NOT 调用转换函数

#### Scenario: 支持链式调用

- **WHEN** 需要对值进行多次转换
- **THEN** 调用者 SHALL 可以使用 `map().and_then()` 等标准 Option 方法
- **AND** 代码 SHALL 保持函数式风格

### Requirement: 改进文档注释

系统 SHALL 改进所有公开 API 的文档注释，补充性能特性说明和最佳实践。

#### Scenario: 文档包含性能特性说明

- **WHEN** 查看 `ReactiveProperty` 和 `LockReactiveProperty` 的文档
- **THEN** 文档 SHALL 说明各方法的性能特性（是否阻塞、是否分配内存）
- **AND** 文档 SHALL 说明适用场景（高频更新 vs 条件等待）
- **AND** 文档 SHALL 提供性能对比表格

#### Scenario: 文档包含最佳实践

- **WHEN** 查看方法文档
- **THEN** 文档 SHALL 提供使用示例
- **AND** 文档 SHALL 说明常见陷阱和注意事项
- **AND** 文档 SHALL 推荐最佳实践（如何选择 `get_current` vs `get_current_borrow`）

#### Scenario: 文档包含并发保证说明

- **WHEN** 查看 `wait_until` 等并发相关方法的文档
- **THEN** 文档 SHALL 明确说明并发保证（是否会错过状态变化）
- **AND** 文档 SHALL 说明多等待者场景的行为
- **AND** 文档 SHALL 提供并发使用示例

