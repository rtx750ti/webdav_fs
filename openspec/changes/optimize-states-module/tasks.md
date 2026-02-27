## 1. 错误类型统一

- [ ] 1.1 在 `reactive_core.rs` 的 `ReactivePropertyError` 中添加 `Destroyed` 变体
- [ ] 1.2 在 `lock_reactive.rs` 中移除 `LockReactivePropertyError` 定义
- [ ] 1.3 将 `lock_reactive.rs` 中所有 `LockReactivePropertyError` 替换为 `ReactivePropertyError`
- [ ] 1.4 在 `unlock_reactive.rs` 中更新错误类型导出（如果需要）
- [ ] 1.5 检查 `src/lib.rs` 是否需要更新公开 API 的错误类型导出

## 2. ReactiveProperty 性能优化

- [ ] 2.1 修改 `reactive_core.rs` 中 `get_current()` 返回类型为 `Option<T>`
- [ ] 2.2 移除 `get_current()` 实现中的 `Arc::new()` 包装，改为直接 `cloned()`
- [ ] 2.3 确认 `get_current_borrow()` 方法保持不变（提供零拷贝访问）

## 3. LockReactiveProperty 性能优化

- [ ] 3.1 优化 `lock_reactive.rs` 中 `wait_until()` 实现：先调用 `notified()` 再获取锁
- [ ] 3.2 确保锁在检查条件后立即释放
- [ ] 3.3 添加注释说明优化原理和并发保证

## 4. 新增便利方法 - try_update

- [ ] 4.1 在 `lock_reactive.rs` 的 `LockReactiveProperty` 中实现 `try_update()` 方法
- [ ] 4.2 使用 `try_lock()` 实现非阻塞更新逻辑
- [ ] 4.3 添加文档注释和使用示例

## 5. 新增便利方法 - get_or_default

- [ ] 5.1 在 `reactive_core.rs` 的 `ReactiveProperty` 中实现 `get_or_default()` 方法
- [ ] 5.2 在 `lock_reactive.rs` 的 `LockReactiveProperty` 中实现 `get_or_default()` 方法
- [ ] 5.3 添加 `where T: Default` trait bound
- [ ] 5.4 添加文档注释和使用示例

## 6. 新增便利方法 - map

- [ ] 6.1 在 `reactive_core.rs` 的 `ReactiveProperty` 中实现 `map()` 方法
- [ ] 6.2 在 `lock_reactive.rs` 的 `LockReactiveProperty` 中实现 `map()` 方法
- [ ] 6.3 添加文档注释和使用示例

## 7. 文档改进

- [ ] 7.1 更新 `reactive_core.rs` 模块文档，补充性能特性说明
- [ ] 7.2 更新 `lock_reactive.rs` 模块文档，补充并发保证和最佳实践
- [ ] 7.3 更新 `unlock_reactive.rs` 模块文档，说明与 LockReactiveProperty 的区别
- [ ] 7.4 为 `get_current()` 添加性能说明（堆分配次数）
- [ ] 7.5 为 `wait_until()` 添加并发保证说明（不会错过状态变化）
- [ ] 7.6 为新增方法添加完整的文档注释和示例代码

## 8. Benchmark 测试

- [ ] 8.1 在 `Cargo.toml` 中添加 `criterion` 作为 dev-dependency
- [ ] 8.2 创建 `benches/states_performance.rs` 文件
- [ ] 8.3 实现 `get_current()` 性能对比 benchmark（优化前后）
- [ ] 8.4 实现 `wait_until()` 高并发 benchmark（10/50/100 并发任务）
- [ ] 8.5 实现 `try_update()` vs `update()` 性能对比 benchmark
- [ ] 8.6 运行 benchmark 并记录结果

## 9. 并发测试

- [ ] 9.1 在 `src/tests/` 或 `tests/` 中创建并发测试文件
- [ ] 9.2 测试 `wait_until()` 在快速更新场景下不会错过通知
- [ ] 9.3 测试多个等待者同时等待时的正确性
- [ ] 9.4 测试 `try_update()` 在锁竞争场景下的行为
- [ ] 9.5 测试属性销毁时所有等待者正确收到错误

## 10. 验证和清理

- [ ] 10.1 运行 `cargo test` 确保所有测试通过
- [ ] 10.2 运行 `cargo clippy` 检查代码质量
- [ ] 10.3 运行 `cargo doc --open` 检查文档渲染效果
- [ ] 10.4 检查是否有未使用的导入或死代码
- [ ] 10.5 确认所有 breaking changes 已在文档中标注

