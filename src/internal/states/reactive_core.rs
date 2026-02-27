//! # ReactiveProperty — 响应式属性内核
//!
//! 所有响应式属性的公共基础设施。
//! [`UnlockReactiveProperty`] 和 [`LockReactiveProperty`] 均基于本模块实现。
//!
//! 本模块**不对外导出**，仅供 `states` 子模块内部复用。

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use thiserror::Error;
use tokio::sync::watch;
use tokio::sync::watch::Ref;
use tokio::sync::watch::error::RecvError;

// ──────────────────────────── Error ────────────────────────────

/// 响应式属性统一错误类型
#[derive(Debug, Error)]
pub enum ReactivePropertyError {
    /// 监听器已被销毁
    #[error("监听器已被销毁")]
    WatcherClosed,

    /// 属性已被销毁
    #[error("属性已被销毁")]
    Destroyed,

    /// watch 通道接收失败
    #[error("接收失败: {0}")]
    RecvError(#[from] RecvError),
}

// ──────────────────────────── Inner ────────────────────────────

/// 内部共享状态，包含值发送器和销毁标志。
#[derive(Debug)]
pub(crate) struct Inner<T> {
    pub(crate) sender: watch::Sender<Option<T>>,
    pub(crate) is_dropped: AtomicBool,
}

impl<T> Drop for Inner<T> {
    fn drop(&mut self) {
        self.is_dropped.store(true, Ordering::Relaxed);
        let _ = self.sender.send(None);
    }
}

// ──────────────────────────── ReactiveProperty ────────────────────────────

/// 响应式属性内核：提供 new / update / update_field / get_current / watch 等基础能力。
#[derive(Clone, Debug)]
pub struct ReactiveProperty<T: Clone + Send + Sync> {
    pub(crate) inner: Arc<Inner<T>>,
    pub(crate) cache_receiver: watch::Receiver<Option<T>>,
}

impl<T> ReactiveProperty<T>
where
    T: Clone + Send + Sync,
{
    /// 创建一个新的响应式属性。
    pub fn new(value: T) -> Self {
        let (sender, _) = watch::channel(Some(value));
        let cache_receiver = sender.subscribe();
        Self {
            inner: Arc::new(Inner {
                sender,
                is_dropped: AtomicBool::new(false),
            }),
            cache_receiver,
        }
    }

    /// 更新属性的值，所有监听者都会收到通知。
    pub fn update(
        &self,
        new_value: T,
    ) -> Result<&Self, ReactivePropertyError> {
        if self.inner.is_dropped.load(Ordering::Relaxed) {
            return Ok(self);
        }
        match self.inner.sender.send(Some(new_value)) {
            Ok(_) => Ok(self),
            Err(_) => Ok(self),
        }
    }

    /// 使用闭包更新属性的部分字段。
    pub fn update_field<F, R>(
        &self,
        updater: F,
    ) -> Result<&Self, ReactivePropertyError>
    where
        F: FnOnce(&mut T) -> R,
    {
        if self.inner.is_dropped.load(Ordering::Relaxed) {
            return Ok(self);
        }

        let mut current = match self.cache_receiver.borrow().clone() {
            Some(val) => val,
            None => return Ok(self),
        };

        updater(&mut current);
        let _ = self.inner.sender.send(Some(current));
        Ok(self)
    }

    /// 获取当前属性值的快照（会 clone）。
    ///
    /// # 性能说明
    ///
    /// 此方法直接返回 `Option<T>`，仅执行一次 clone 操作，无额外堆分配。
    /// 相比之前的 `Option<Arc<T>>` 实现，减少了一次 `Arc::new()` 的堆分配开销。
    ///
    /// 如需零拷贝访问，请使用 [`get_current_borrow()`](Self::get_current_borrow)。
    pub fn get_current(&self) -> Option<T> {
        self.cache_receiver
            .borrow()
            .as_ref()
            .cloned()
    }

    /// 获取当前属性值的只读借用（零拷贝）。
    pub fn get_current_borrow(&'_ self) -> Ref<'_, Option<T>> {
        self.cache_receiver.borrow()
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
    /// use webdav_fs::states::unlock_reactive::UnlockReactiveProperty;
    ///
    /// let prop = UnlockReactiveProperty::new(42i32);
    /// let value = prop.get_or_default(); // 返回 42
    ///
    /// // 如果属性已销毁，返回 i32::default() (0)
    /// ```
    pub fn get_or_default(&self) -> T
    where
        T: Default,
    {
        self.get_current().unwrap_or_default()
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
    /// use webdav_fs::states::unlock_reactive::UnlockReactiveProperty;
    ///
    /// let prop = UnlockReactiveProperty::new("hello".to_string());
    ///
    /// // 提取字符串长度
    /// let len = prop.map(|s| s.len()); // Some(5)
    ///
    /// // 链式调用
    /// let upper = prop.map(|s| s.to_uppercase())
    ///     .unwrap_or_else(|| "DEFAULT".to_string());
    /// ```
    pub fn map<R, F>(&self, f: F) -> Option<R>
    where
        F: FnOnce(&T) -> R,
    {
        self.get_current().as_ref().map(f)
    }

    /// 创建一个监听器，用于异步监听属性值的变化。
    pub fn watch(&self) -> PropertyWatcher<T> {
        PropertyWatcher {
            receiver: self.inner.sender.subscribe(),
            inner: Arc::clone(&self.inner),
        }
    }
}

// ──────────────────────────── PropertyWatcher ────────────────────────────

/// 属性监听器，用于异步接收属性值的变化。
pub struct PropertyWatcher<T> {
    receiver: watch::Receiver<Option<T>>,
    #[allow(dead_code)]
    inner: Arc<Inner<T>>,
}

impl<T> PropertyWatcher<T>
where
    T: Clone + Send + Sync,
{
    /// 异步等待属性值的变化，返回新值。
    pub async fn changed(&mut self) -> Result<T, ReactivePropertyError> {
        self.receiver.changed().await?;
        match self.receiver.borrow().as_ref() {
            None => Err(ReactivePropertyError::WatcherClosed),
            Some(value) => Ok(value.clone()),
        }
    }

    /// 同步获取当前值的克隆。
    pub fn borrow(&self) -> Option<T> {
        self.receiver.borrow().clone()
    }
}

