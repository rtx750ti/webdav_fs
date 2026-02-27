## Why

`src/internal/states` 模块目前存在性能和易用性问题：`ReactiveProperty::get_current()` 存在不必要的双重包装开销，`LockReactiveProperty::wait_until` 在高并发场景下存在锁竞争，错误类型不统一导致使用复杂，且缺少常用的便利方法。优化这些问题可以提升性能、改善开发体验，并为未来扩展打下基础。

## What Changes

- 优化 `ReactiveProperty::get_current()` 的内存分配策略，移除不必要的 `Arc` 包装
- 改进 `LockReactiveProperty::wait_until` 的锁获取策略，减少锁竞争
- 统一错误类型体系，合并 `LockReactivePropertyError` 和 `ReactivePropertyError`
- 新增便利方法：
  - `try_update()` - 非阻塞更新（仅 LockReactiveProperty）
  - `get_or_default()` - 获取值或返回默认值
  - `map()` - 对当前值应用转换函数
- 改进文档注释，补充性能特性说明和最佳实践
- 添加 benchmark 测试以验证优化效果

## Capabilities

### New Capabilities
- `states-performance`: 响应式属性的性能优化能力，包括内存分配优化和并发控制改进
- `states-ergonomics`: 响应式属性的易用性增强，包括便利方法和统一错误处理

### Modified Capabilities
<!-- 无现有 capability 的 requirement 变更 -->

## Impact

**受影响的文件：**
- `src/internal/states/reactive_core.rs` - 优化 `get_current()` 实现，新增便利方法
- `src/internal/states/lock_reactive.rs` - 优化 `wait_until()` 锁策略，新增 `try_update()`，统一错误类型
- `src/internal/states/unlock_reactive.rs` - 更新错误类型导出
- `src/lib.rs` - 可能需要更新公开 API 的错误类型导出

**API 变更：**
- `ReactiveProperty::get_current()` 返回类型从 `Option<Arc<T>>` 改为 `Option<T>`（**BREAKING**）
- 错误类型统一为 `ReactivePropertyError`，移除 `LockReactivePropertyError`（**BREAKING**）
- 新增方法向后兼容，不影响现有代码

**依赖变更：**
- 无新增外部依赖
- 可能需要添加 `criterion` 用于 benchmark（dev-dependency）

**性能影响：**
- `get_current()` 预期减少 1 次堆分配
- `wait_until()` 在高并发场景下预期减少 20-30% 的锁等待时间

