//! # QueueReactiveProperty — 微队列响应式属性
//!
//! 基于 tokio::sync::mpsc 实现的单向消息队列，用于命令传递场景。
//! 
//! ## 特性
//! - 无锁设计（基于 mpsc::unbounded_channel）
//! - 严格 FIFO 顺序
//! - 生产者可以有多个（Clone sender），消费者只有一个
//! - 仅库内部使用（`pub(crate)`）
//!
//! ## 使用场景
//! - 控制命令传递（如下载器的 pause/resume/cancel）
//! - 任务调度
//! - Actor 模式的消息传递
//!
//! ## 与其他响应式属性的区别
//! - `UnlockReactiveProperty`: 双向读写，广播模式，适合状态共享
//! - `LockReactiveProperty`: 双向读写，条件等待，适合状态同步
//! - `QueueReactiveProperty`: 单向传递，FIFO 消费，适合命令传递

use tokio::sync::mpsc;
use super::reactive_core::ReactiveProperty;

/// 微队列响应式属性（生产者端）
/// 
/// 可以 Clone，多个生产者可以同时往队列推送消息。
/// 内部基于 `mpsc::UnboundedSender`，无锁设计。
#[derive(Clone, Debug)]
pub(crate) struct QueueReactiveProperty<T: Clone + Send + Sync + 'static> {
    sender: mpsc::UnboundedSender<T>,
    // 同时维护一个响应式属性，用于外部订阅（只读）
    state: ReactiveProperty<Option<T>>,
}

/// 微队列消费者
///
/// 不可 Clone，只能有一个消费者。
/// 消费者独占接收端，按 FIFO 顺序消费消息。
#[derive(Debug)]
pub(crate) struct QueueReactiveConsumer<T: Clone + Send + Sync + 'static> {
    receiver: mpsc::UnboundedReceiver<T>,
    state: ReactiveProperty<Option<T>>,
}

impl<T> QueueReactiveProperty<T>
where
    T: Clone + Send + Sync + 'static,
{
    /// 创建一个新的微队列响应式属性
    /// 
    /// 返回 (生产者, 消费者) 元组。
    /// 生产者可以 Clone，消费者只能有一个。
    pub(crate) fn new() -> (Self, QueueReactiveConsumer<T>) {
        let (sender, receiver) = mpsc::unbounded_channel();
        let state = ReactiveProperty::new(None);
        
        let producer = Self {
            sender,
            state: state.clone(),
        };
        
        let consumer = QueueReactiveConsumer {
            receiver,
            state,
        };
        
        (producer, consumer)
    }
    
    /// 发送消息到队列
    /// 
    /// 无锁操作，立即返回。
    /// 如果接收端已关闭，返回 `Err(T)`。
    pub(crate) fn send(&self, value: T) -> Result<(), T> {
        // 更新响应式属性（用于外部订阅）
        let _ = self.state.update(Some(value.clone()));
        
        // 发送到队列
        self.sender.send(value).map_err(|e| e.0)
    }
    
    /// 获取用于订阅的响应式属性
    /// 
    /// 外部可以通过这个属性订阅消息变化（只读）。
    pub(crate) fn watch(&self) -> super::reactive_core::PropertyWatcher<Option<T>> {
        self.state.watch()
    }
}

impl<T> QueueReactiveConsumer<T>
where
    T: Clone + Send + Sync + 'static,
{
    /// 异步接收下一条消息
    /// 
    /// 如果队列为空，会挂起等待。
    /// 如果发送端全部关闭，返回 `None`。
    pub(crate) async fn recv(&mut self) -> Option<T> {
        let value = self.receiver.recv().await;
        
        // 更新响应式属性
        if let Some(ref v) = value {
            let _ = self.state.update(Some(v.clone()));
        }
        
        value
    }
    
    /// 尝试非阻塞接收消息
    /// 
    /// 如果队列为空，立即返回 `None`。
    pub(crate) fn try_recv(&mut self) -> Option<T> {
        match self.receiver.try_recv() {
            Ok(value) => {
                let _ = self.state.update(Some(value.clone()));
                Some(value)
            }
            Err(_) => None,
        }
    }
}
