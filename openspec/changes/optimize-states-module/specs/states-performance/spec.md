## ADDED Requirements

### Requirement: 优化 ReactiveProperty 内存分配

系统 SHALL 优化 `ReactiveProperty::get_current()` 方法，移除不必要的 `Arc` 包装，直接返回 `Option<T>` 而非 `Option<Arc<T>>`。

#### Scenario: 获取当前值时减少堆分配

- **WHEN** 调用 `ReactiveProperty::get_current()`
- **THEN** 系统 SHALL 直接返回 `Option<T>`，通过 clone 获取值
- **AND** 系统 SHALL NOT 创建额外的 `Arc` 包装

#### Scenario: 保持零拷贝借用能力

- **WHEN** 需要零拷贝访问当前值
- **THEN** 系统 SHALL 保留 `get_current_borrow()` 方法返回 `Ref<'_, Option<T>>`
- **AND** 该方法 SHALL NOT 执行任何堆分配

### Requirement: 优化 LockReactiveProperty 锁竞争

系统 SHALL 改进 `LockReactiveProperty::wait_until` 的锁获取策略，减少高并发场景下的锁竞争。

#### Scenario: 减少 wait_until 中的锁持有时间

- **WHEN** 多个任务同时调用 `wait_until`
- **THEN** 系统 SHALL 在检查条件前先注册通知监听器
- **AND** 系统 SHALL 最小化锁的持有时间
- **AND** 系统 SHALL 在条件不满足时立即释放锁

#### Scenario: 避免惊群效应

- **WHEN** 值更新触发多个等待者
- **THEN** 系统 SHALL 使用 `notify_waiters()` 唤醒所有等待者
- **AND** 每个等待者 SHALL 独立检查条件是否满足
- **AND** 不满足条件的等待者 SHALL 继续等待而不产生额外开销

### Requirement: 提供非阻塞更新方法

系统 SHALL 为 `LockReactiveProperty` 提供 `try_update()` 方法，支持非阻塞的值更新操作。

#### Scenario: 尝试立即更新值

- **WHEN** 调用 `try_update(new_value)`
- **THEN** 系统 SHALL 尝试立即获取锁
- **AND** 如果锁可用，系统 SHALL 更新值并返回 `Ok(true)`
- **AND** 如果锁不可用，系统 SHALL 立即返回 `Ok(false)` 而不等待

#### Scenario: 非阻塞更新失败不影响后续操作

- **WHEN** `try_update()` 返回 `Ok(false)`
- **THEN** 属性状态 SHALL 保持不变
- **AND** 调用者 SHALL 可以选择重试或执行其他逻辑
- **AND** 系统 SHALL NOT 产生任何副作用

### Requirement: 性能基准测试

系统 SHALL 提供 benchmark 测试以验证性能优化效果。

#### Scenario: 测量 get_current 性能改进

- **WHEN** 运行 benchmark 测试
- **THEN** 系统 SHALL 测量优化前后 `get_current()` 的执行时间
- **AND** 系统 SHALL 测量堆分配次数的变化
- **AND** 优化后的版本 SHALL 减少至少 1 次堆分配

#### Scenario: 测量 wait_until 并发性能

- **WHEN** 在高并发场景下运行 benchmark
- **THEN** 系统 SHALL 测量多个任务同时调用 `wait_until` 的吞吐量
- **AND** 系统 SHALL 测量锁等待时间的分布
- **AND** 优化后的版本 SHALL 在高并发场景下减少 20-30% 的平均锁等待时间

