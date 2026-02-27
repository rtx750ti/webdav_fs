//! # UnlockReactiveProperty
//!
//! 一个轻量级的响应式属性容器，基于 [`tokio::sync::watch`] 实现，
//! 支持异步监听和更新值。
//!
//! 内部直接复用 [`super::reactive_core::ReactiveProperty`]。
//!
//! ## 使用示例
//! ```rust,no_run
//! use webdav_fs::states::unlock_reactive::UnlockReactiveProperty;
//!
//! let prop = UnlockReactiveProperty::new(0);
//! prop.update(1).unwrap();
//! prop.update(2).unwrap();
//! ```

pub use super::reactive_core::{PropertyWatcher, ReactivePropertyError as UnlockReactivePropertyError};

/// 轻量级响应式属性容器（无条件等待能力）。
///
/// 纯通知机制，读写不阻塞，适合高频更新场景（如下载进度条）。
/// 如需条件等待能力（如暂停/恢复控制），请使用
/// [`LockReactiveProperty`](super::lock_reactive::LockReactiveProperty)。
pub type UnlockReactiveProperty<T> = super::reactive_core::ReactiveProperty<T>;
