//! 响应式属性并发测试
//!
//! 测试项：
//! - wait_until() 在快速更新场景下不会错过通知
//! - 多个等待者同时等待时的正确性
//! - try_update() 在锁竞争场景下的行为
//! - 属性销毁时所有等待者正确收到错误

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;
use tokio::time::timeout;

use crate::states::lock_reactive::LockReactiveProperty;

// ═══════════════════════════ wait_until 快速更新测试 ═══════════════════════════

#[tokio::test]
async fn wait_until_does_not_miss_rapid_updates() {
    // 测试在快速更新场景下，wait_until 不会错过任何满足条件的状态
    let prop = Arc::new(LockReactiveProperty::new(0i32));
    let p = Arc::clone(&prop);

    // 启动快速更新任务
    tokio::spawn(async move {
        for i in 1..=100 {
            p.update(i).await.unwrap();
            // 不等待，尽可能快地更新
        }
    });

    // 等待值达到 100
    let result = timeout(Duration::from_secs(5), prop.wait_until(|v| *v == 100)).await;
    assert!(result.is_ok(), "wait_until 应该能捕获到快速更新的值");
    assert_eq!(prop.get_current().await.unwrap(), 100);
}

// ═══════════════════════════ 多等待者测试 ═══════════════════════════

#[tokio::test]
async fn multiple_waiters_all_notified() {
    // 测试多个等待者同时等待时，所有满足条件的都会被唤醒
    let prop = Arc::new(LockReactiveProperty::new(0i32));
    let success_count = Arc::new(AtomicU32::new(0));

    // 启动 10 个等待者
    let mut handles = Vec::new();
    for _ in 0..10 {
        let p = Arc::clone(&prop);
        let count = Arc::clone(&success_count);
        handles.push(tokio::spawn(async move {
            p.wait_until(|v| *v == 42).await.unwrap();
            count.fetch_add(1, Ordering::Relaxed);
        }));
    }

    // 等待所有等待者就绪
    tokio::time::sleep(Duration::from_millis(50)).await;

    // 更新值以唤醒所有等待者
    prop.update(42).await.unwrap();

    // 等待所有任务完成
    for h in handles {
        timeout(Duration::from_secs(1), h)
            .await
            .expect("等待者应该被唤醒")
            .unwrap();
    }

    // 验证所有等待者都被唤醒
    assert_eq!(success_count.load(Ordering::Relaxed), 10);
}

// ═══════════════════════════ try_update 锁竞争测试 ═══════════════════════════

#[tokio::test]
async fn try_update_handles_lock_contention() {
    // 测试 try_update 在锁竞争场景下的行为
    let prop = Arc::new(LockReactiveProperty::new(0i32));
    let success_count = Arc::new(AtomicU32::new(0));
    let failure_count = Arc::new(AtomicU32::new(0));

    // 启动多个任务尝试 try_update
    let mut handles = Vec::new();
    for i in 0..20 {
        let p = Arc::clone(&prop);
        let s_count = Arc::clone(&success_count);
        let f_count = Arc::clone(&failure_count);
        handles.push(tokio::spawn(async move {
            match p.try_update(i) {
                Ok(true) => s_count.fetch_add(1, Ordering::Relaxed),
                Ok(false) => f_count.fetch_add(1, Ordering::Relaxed),
                Err(_) => panic!("不应该返回错误"),
            };
        }));
    }

    // 等待所有任务完成
    for h in handles {
        h.await.unwrap();
    }

    let success = success_count.load(Ordering::Relaxed);
    let failure = failure_count.load(Ordering::Relaxed);

    // 验证：至少有一些成功，可能有一些失败（取决于锁竞争）
    assert!(success > 0, "应该有一些 try_update 成功");
    assert_eq!(success + failure, 20, "所有尝试都应该返回结果");
}

// ═══════════════════════════ 属性销毁测试 ═══════════════════════════

#[tokio::test]
async fn destroy_notifies_all_waiters() {
    // 测试属性销毁时所有等待者正确收到错误
    let prop = Arc::new(LockReactiveProperty::new(0i32));
    let error_count = Arc::new(AtomicU32::new(0));

    // 启动多个等待者
    let mut handles = Vec::new();
    for _ in 0..10 {
        let p = Arc::clone(&prop);
        let count = Arc::clone(&error_count);
        handles.push(tokio::spawn(async move {
            let result = p.wait_until(|v| *v == 9999).await;
            if result.is_err() {
                count.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }

    // 等待所有等待者就绪
    tokio::time::sleep(Duration::from_millis(50)).await;

    // 销毁属性
    prop.destroy().await;

    // 等待所有任务完成
    for h in handles {
        timeout(Duration::from_secs(1), h)
            .await
            .expect("等待者应该收到销毁通知")
            .unwrap();
    }

    // 验证所有等待者都收到错误
    assert_eq!(error_count.load(Ordering::Relaxed), 10);
}

