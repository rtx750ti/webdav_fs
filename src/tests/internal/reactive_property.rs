//! 响应式属性测试：功能正确性 + UnlockReactiveProperty 与 LockReactiveProperty 性能对比。
//!
//! 测试项：
//! - 基础读写、watch 监听
//! - `wait_until` 条件等待（立即满足 / 异步等待 / 销毁唤醒）
//! - 高频写 + 读吞吐量对比（Unlock vs Lock）
//! - 多任务并发写性能对比

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crate::states::lock_reactive::LockReactiveProperty;
use crate::states::unlock_reactive::UnlockReactiveProperty;

// ═══════════════════════════ 功能测试 ═══════════════════════════

#[tokio::test]
async fn unlock_basic_update_and_read() {
    let prop = UnlockReactiveProperty::new(0u64);
    prop.update(42).unwrap();
    assert_eq!(prop.get_current().unwrap(), 42);

    prop.update_field(|v| *v += 8).unwrap();
    assert_eq!(prop.get_current().unwrap(), 50);
}

#[tokio::test]
async fn lock_basic_update_and_read() {
    let prop = LockReactiveProperty::new("hello".to_string());
    prop.update("world".to_string()).await.unwrap();
    assert_eq!(prop.get_current().await.unwrap().as_str(), "world");
}

#[tokio::test]
async fn unlock_watch_receives_updates() {
    let prop = UnlockReactiveProperty::new(0i32);
    let mut watcher = prop.watch();

    prop.update(1).unwrap();
    let v = watcher.changed().await.unwrap();
    assert_eq!(v, 1);

    prop.update(2).unwrap();
    let v = watcher.changed().await.unwrap();
    assert_eq!(v, 2);
}

#[tokio::test]
async fn lock_wait_until_already_satisfied() {
    let prop = LockReactiveProperty::new(100i32);
    // 当前值已满足，应立即返回
    prop.wait_until(|v| *v == 100).await.unwrap();
}

#[tokio::test]
async fn lock_wait_until_async_satisfied() {
    let prop = LockReactiveProperty::new(0i32);
    let p = prop.clone();

    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        p.update(1).await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        p.update(42).await.unwrap();
    });

    prop.wait_until(|v| *v == 42).await.unwrap();
    assert_eq!(prop.get_current().await.unwrap(), 42);
}

#[tokio::test]
async fn lock_wait_until_blocks_when_unsatisfied() {
    let prop = LockReactiveProperty::new(0i32);
    let p = prop.clone();

    // wait_until 条件不满足时应挂起，100ms 内不会返回
    let result = tokio::time::timeout(
        Duration::from_millis(100),
        p.wait_until(|v| *v == 9999),
    )
    .await;
    assert!(result.is_err(), "条件未满足时 wait_until 应持续挂起（超时）");

    // 满足条件后应立即返回
    prop.update(9999).await.unwrap();
    let result = tokio::time::timeout(
        Duration::from_millis(100),
        p.wait_until(|v| *v == 9999),
    )
    .await;
    assert!(result.is_ok(), "条件满足后 wait_until 应立即返回");
}

// ═══════════════════════════ 性能测试 ═══════════════════════════

const ITER_COUNT: u64 = 100_000;

/// 单线程高频写性能对比
///
/// 注意：单线程场景下，LockReactiveProperty 可能更快，因为：
/// - Mutex 在无竞争时开销很小
/// - watch 通道有广播机制的开销
///
/// 这个测试主要用于确保性能在合理范围内，不做严格的快慢断言。
#[tokio::test]
async fn perf_single_thread_write() {
    println!("\n── 单线程高频写 ({ITER_COUNT} 次) ──");

    // Unlock
    let prop = UnlockReactiveProperty::new(0u64);
    let start = Instant::now();
    for i in 0..ITER_COUNT {
        let _ = prop.update(i);
    }
    let unlock_dur = start.elapsed();
    println!("  UnlockReactive : {:?}", unlock_dur);

    // Lock
    let prop = LockReactiveProperty::new(0u64);
    let start = Instant::now();
    for i in 0..ITER_COUNT {
        let _ = prop.update(i).await;
    }
    let lock_dur = start.elapsed();
    println!("  LockReactive   : {:?}", lock_dur);

    let ratio = lock_dur.as_nanos() as f64 / unlock_dur.as_nanos() as f64;
    println!("  Lock/Unlock 耗时比 : {:.2}x", ratio);
    println!("  说明：单线程场景下 Mutex 可能更快（无竞争），这是正常的");
}

/// 单线程高频读性能对比
///
/// 注意：单线程场景下，两者性能差异不大。
/// LockReactiveProperty 需要异步锁，但无竞争时开销很小。
#[tokio::test]
async fn perf_single_thread_read() {
    println!("\n── 单线程高频读 ({ITER_COUNT} 次) ──");

    let prop_u = UnlockReactiveProperty::new(42u64);
    let start = Instant::now();
    for _ in 0..ITER_COUNT {
        let _ = prop_u.get_current();
    }
    let unlock_dur = start.elapsed();
    println!("  UnlockReactive : {:?}", unlock_dur);

    let prop_l = LockReactiveProperty::new(42u64);
    let start = Instant::now();
    for _ in 0..ITER_COUNT {
        let _ = prop_l.get_current().await;
    }
    let lock_dur = start.elapsed();
    println!("  LockReactive   : {:?}", lock_dur);

    let ratio = lock_dur.as_nanos() as f64 / unlock_dur.as_nanos() as f64;
    println!("  Lock/Unlock 耗时比 : {:.2}x", ratio);
}

/// 多任务并发写性能对比
#[tokio::test]
async fn perf_concurrent_write() {
    const TASKS: u64 = 8;
    const PER_TASK: u64 = ITER_COUNT / TASKS;

    println!("\n── 多任务并发写 ({TASKS} 任务 × {PER_TASK} 次) ──");

    // Unlock
    let prop = UnlockReactiveProperty::new(0u64);
    let start = Instant::now();
    let mut handles = Vec::new();
    for t in 0..TASKS {
        let p = prop.clone();
        handles.push(tokio::spawn(async move {
            for i in 0..PER_TASK {
                let _ = p.update(t * PER_TASK + i);
            }
        }));
    }
    for h in handles { h.await.unwrap(); }
    let unlock_dur = start.elapsed();
    println!("  UnlockReactive : {:?}", unlock_dur);

    // Lock
    let prop = LockReactiveProperty::new(0u64);
    let start = Instant::now();
    let mut handles = Vec::new();
    for t in 0..TASKS {
        let p = prop.clone();
        handles.push(tokio::spawn(async move {
            for i in 0..PER_TASK {
                let _ = p.update(t * PER_TASK + i).await;
            }
        }));
    }
    for h in handles { h.await.unwrap(); }
    let lock_dur = start.elapsed();
    println!("  LockReactive   : {:?}", lock_dur);

    let ratio = lock_dur.as_nanos() as f64 / unlock_dur.as_nanos() as f64;
    println!("  Lock/Unlock 耗时比 : {:.2}x", ratio);
    // Lock 使用 Mutex，并发写性能会有损失，允许 5 倍
    assert!(ratio < 5.0, "并发写差距不应超过 5 倍，实际 {:.2}x", ratio);
}

/// wait_until 唤醒延迟测试
#[tokio::test]
async fn perf_wait_until_latency() {
    const ROUNDS: u64 = 1000;
    println!("\n── wait_until 唤醒延迟 ({ROUNDS} 轮 ping-pong) ──");

    let prop = LockReactiveProperty::new(0u64);
    let p = prop.clone();
    let total_latency = Arc::new(AtomicU64::new(0));
    let lat = total_latency.clone();

    // 写端：递增值
    let writer = tokio::spawn(async move {
        for i in 1..=ROUNDS {
            // 等读端消费上一个值后再写（奇数写入）
            tokio::task::yield_now().await;
            p.update(i).await.unwrap();
        }
    });

    // 读端：wait_until 等每个新值
    for i in 1..=ROUNDS {
        let t = Instant::now();
        prop.wait_until(|v| *v >= i).await.unwrap();
        lat.fetch_add(t.elapsed().as_nanos() as u64, Ordering::Relaxed);
    }
    writer.await.unwrap();

    let avg_ns = total_latency.load(Ordering::Relaxed) / ROUNDS;
    println!("  平均唤醒延迟 : {} ns ({} µs)", avg_ns, avg_ns / 1000);

    // watch 做同样的事情作为对照
    let prop = UnlockReactiveProperty::new(0u64);
    let p = prop.clone();
    let total_latency = Arc::new(AtomicU64::new(0));
    let lat = total_latency.clone();
    let mut watcher = prop.watch();

    let writer = tokio::spawn(async move {
        for i in 1..=ROUNDS {
            tokio::task::yield_now().await;
            p.update(i).unwrap();
        }
    });

    for _ in 1..=ROUNDS {
        let t = Instant::now();
        let _ = watcher.changed().await;
        lat.fetch_add(t.elapsed().as_nanos() as u64, Ordering::Relaxed);
    }
    writer.await.unwrap();

    let avg_ns_watch = total_latency.load(Ordering::Relaxed) / ROUNDS;
    println!("  watch 对照    : {} ns ({} µs)", avg_ns_watch, avg_ns_watch / 1000);
    println!("  wait_until / watch 延迟比 : {:.2}x", avg_ns as f64 / avg_ns_watch.max(1) as f64);
}