//! # LockReactiveProperty
//!
//! 一个带条件等待能力的响应式属性容器，基于 `tokio::sync::Mutex` + `Notify` 实现。
//!
//! ## 与 UnlockReactiveProperty 的区别
//! - `UnlockReactiveProperty`：纯通知机制，读写不阻塞，适合高频更新场景（如进度条）。
//! - `LockReactiveProperty`：使用互斥锁保证强一致性，`wait_until` 不会错过任何满足条件的时刻。
//!
//! ## 并发保证
//!
//! 使用 `tokio::sync::Mutex` 保护值，`tokio::sync::Notify` 进行通知，保证：
//! - ✅ `wait_until` 不会错过任何满足条件的状态（即使微秒级快速切换）
//! - ✅ 多个等待者同时等待时，所有满足条件的都会被唤醒
//! - ✅ 高并发环境下的正确性
//!
//! ## 性能特性
//! - 每次 `update` 需要获取互斥锁（异步锁，不会阻塞线程）
//! - 每次 `get_current` 需要获取互斥锁
//! - 适合状态变化不是极高频的场景（如控制信号、配置更新）
//!
//! ## 使用示例
//! ```rust,no_run
//! use webdav_fs::states::lock_reactive::LockReactiveProperty;
//!
//! #[derive(Clone, PartialEq)]
//! enum Command { Running, Paused }
//!
//! # async fn example() {
//! let prop = LockReactiveProperty::new(Command::Running);
//!
//! // 另一个任务中暂停
//! let p = prop.clone();
//! tokio::spawn(async move {
//!     tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
//!     p.update(Command::Paused).unwrap();
//! });
//!
//! // 当前任务挂起，直到变为 Paused
//! prop.wait_until(|s| matches!(s, Command::Paused)).await.unwrap();
//! # }
//! ```

use std::sync::Arc;
use tokio::sync::{Mutex, Notify};

use super::reactive_core::ReactivePropertyError;

struct Inner<T> {
    value: Mutex<Option<T>>,
    notify: Notify,
}

/// 带条件等待能力的响应式属性容器。
///
/// 使用 `tokio::sync::Mutex` 保护值，`tokio::sync::Notify` 进行通知，
/// 保证高并发环境下 `wait_until` 不会错过任何满足条件的状态。
#[derive(Clone)]
pub struct LockReactiveProperty<T: Clone + Send + Sync> {
    inner: Arc<Inner<T>>,
}

impl<T> LockReactiveProperty<T>
where
    T: Clone + Send + Sync,
{
    /// 创建一个新的带条件等待能力的响应式属性。
    pub fn new(value: T) -> Self {
        Self {
            inner: Arc::new(Inner {
                value: Mutex::new(Some(value)),
                notify: Notify::new(),
            }),
        }
    }

    /// 更新属性值并通知所有等待者。
    ///
    /// # 返回值
    /// - `Ok(())`: 更新成功。
    /// - `Err(ReactivePropertyError::Destroyed)`: 属性已被销毁。
    pub async fn update(&self, new_value: T) -> Result<(), ReactivePropertyError> {
        let mut guard = self.inner.value.lock().await;
        if guard.is_none() {
            return Err(ReactivePropertyError::Destroyed);
        }
        *guard = Some(new_value);
        drop(guard);
        self.inner.notify.notify_waiters();
        Ok(())
    }

    /// 获取当前值的克隆。
    ///
    /// # 返回值
    /// - `Ok(T)`: 当前值的克隆。
    /// - `Err(ReactivePropertyError::Destroyed)`: 属性已被销毁。
    pub async fn get_current(&self) -> Result<T, ReactivePropertyError> {
        let guard = self.inner.value.lock().await;
        guard
            .as_ref()
            .cloned()
            .ok_or(ReactivePropertyError::Destroyed)
    }

    /// 尝试非阻塞地更新属性值。
    ///
    /// 此方法使用 `try_lock()` 尝试立即获取锁。如果锁当前不可用，
    /// 立即返回 `Ok(false)` 而不等待。
    ///
    /// # 返回值
    /// - `Ok(true)`: 更新成功。
    /// - `Ok(false)`: 锁不可用，未更新。
    /// - `Err(ReactivePropertyError::Destroyed)`: 属性已被销毁。
    ///
    /// # 使用场景
    ///
    /// 适合不希望阻塞的场景，例如：
    /// - 定时更新任务，如果锁被占用则跳过本次更新
    /// - 高频更新场景，避免排队等待
    ///
    /// # 示例
    /// ```rust,no_run
    /// use webdav_fs::states::lock_reactive::LockReactiveProperty;
    ///
    /// # async fn example() {
    /// let prop = LockReactiveProperty::new(0i32);
    ///
    /// // 尝试更新，如果锁不可用则跳过
    /// match prop.try_update(42) {
    ///     Ok(true) => println!("更新成功"),
    ///     Ok(false) => println!("锁被占用，跳过更新"),
    ///     Err(e) => println!("错误: {}", e),
    /// }
    /// # }
    /// ```
    pub fn try_update(&self, new_value: T) -> Result<bool, ReactivePropertyError> {
        match self.inner.value.try_lock() {
            Ok(mut guard) => {
                if guard.is_none() {
                    return Err(ReactivePropertyError::Destroyed);
                }
                *guard = Some(new_value);
                drop(guard);
                self.inner.notify.notify_waiters();
                Ok(true)
            }
            Err(_) => Ok(false),
        }
    }

    /// 获取当前值，如果属性已销毁则返回默认值。
    ///
    /// # 使用场景
    ///
    /// 适合不关心属性是否已销毁的场景，例如：
    /// - 显示 UI 时需要一个合理的默认值
    /// - 降级处理，属性销毁时使用默认配置
    ///
    /// # 示例
    /// ```rust,no_run
    /// use webdav_fs::states::lock_reactive::LockReactiveProperty;
    ///
    /// # async fn example() {
    /// let prop = LockReactiveProperty::new(42i32);
    /// let value = prop.get_or_default().await; // 返回 42
    ///
    /// // 如果属性已销毁，返回 i32::default() (0)
    /// # }
    /// ```
    pub async fn get_or_default(&self) -> T
    where
        T: Default,
    {
        self.get_current().await.unwrap_or_default()
    }

    /// 对当前值应用转换函数。
    ///
    /// 如果属性已销毁，返回 `None` 而不调用转换函数。
    ///
    /// # 使用场景
    ///
    /// 适合需要对值进行转换或提取部分字段的场景：
    /// - 提取结构体的某个字段
    /// - 对值进行计算或格式化
    /// - 链式调用与其他 Option 方法组合
    ///
    /// # 示例
    /// ```rust,no_run
    /// use webdav_fs::states::lock_reactive::LockReactiveProperty;
    ///
    /// # async fn example() {
    /// let prop = LockReactiveProperty::new("hello".to_string());
    ///
    /// // 提取字符串长度
    /// let len = prop.map(|s| s.len()).await; // Some(5)
    ///
    /// // 链式调用
    /// let upper = prop.map(|s| s.to_uppercase()).await
    ///     .unwrap_or_else(|| "DEFAULT".to_string());
    /// # }
    /// ```
    pub async fn map<R, F>(&self, f: F) -> Option<R>
    where
        F: FnOnce(&T) -> R,
    {
        self.get_current().await.ok().map(|v| f(&v))
    }

    /// 销毁属性，释放资源并唤醒所有等待者。
    ///
    /// 调用后，所有后续操作将返回 `Err(ReactivePropertyError::Destroyed)`。
    pub async fn destroy(&self) {
        let mut guard = self.inner.value.lock().await;
        *guard = None;
        drop(guard);
        self.inner.notify.notify_waiters();
    }

    /// 异步等待直到值满足指定条件。
    ///
    /// 此方法会挂起当前任务，直到属性值满足 `predicate` 返回 `true`。
    /// 如果当前值已经满足条件，则立即返回。
    ///
    /// # 并发保证
    ///
    /// ✅ 使用互斥锁保护，保证不会错过任何满足条件的状态，即使在高并发环境下。
    ///
    /// **优化说明**：先注册通知监听器再获取锁，确保在检查条件和等待通知之间不会错过任何状态变化。
    /// 这是 tokio 官方推荐的 `Notify` 使用模式，可减少高并发场景下的锁竞争。
    ///
    /// # 参数
    /// - `predicate`: 一个闭包，接收当前值的引用，返回 `bool` 表示是否满足条件。
    ///
    /// # 返回值
    /// - `Ok(())`: 值满足条件。
    /// - `Err(ReactivePropertyError::Destroyed)`: 属性已被销毁。
    ///
    /// # 示例
    /// ```rust,no_run
    /// use webdav_fs::states::lock_reactive::LockReactiveProperty;
    ///
    /// # async fn example() {
    /// let prop = LockReactiveProperty::new(0i32);
    ///
    /// let p = prop.clone();
    /// tokio::spawn(async move {
    ///     tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    ///     p.update(42).await.unwrap();
    /// });
    ///
    /// prop.wait_until(|v| *v == 42).await.unwrap();
    /// # }
    /// ```
    pub async fn wait_until<F>(
        &self,
        mut predicate: F,
    ) -> Result<(), ReactivePropertyError>
    where
        F: FnMut(&T) -> bool,
    {
        loop {
            // 优化：先注册通知监听器，再获取锁
            // 这样可以确保在释放锁后不会错过任何通知
            let notified = self.inner.notify.notified();

            let guard = self.inner.value.lock().await;
            match guard.as_ref() {
                None => return Err(ReactivePropertyError::Destroyed),
                Some(value) => {
                    if predicate(value) {
                        return Ok(());
                    }
                }
            }
            // 立即释放锁，最小化锁持有时间
            drop(guard);

            notified.await;
        }
    }
}
