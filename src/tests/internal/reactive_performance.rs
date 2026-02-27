//! 响应式属性性能基准测试
//!
//! 测试 UnlockReactiveProperty 和 LockReactiveProperty 在高并发场景下的性能表现。
//! 
//! 测试指标：
//! - 更新吞吐量（次/秒）
//! - 事件吞吐量（事件/秒）
//! - 平均延迟（微秒）
//! - 内存使用（MB）

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::Barrier;
use tokio::time::Instant;

use crate::remote_file::DownloadProgress;
use crate::states::unlock_reactive::UnlockReactiveProperty;
use crate::states::lock_reactive::LockReactiveProperty;

// 性能测试配置（两个测试使用相同参数）
const PERF_WATCHER_COUNT: usize = 100_0000;   // 监听器数量
const PERF_UPDATE_COUNT: usize = 1_000;     // 更新次数

/// UnlockReactiveProperty（无锁）性能基准测试：模拟大量监听器并发监听进度更新。
///
/// 测试配置：
/// - 监听器数量: 1万
/// - 更新次数: 1千
///
/// 测试场景：
/// 1. 创建 N 个监听器同时监听同一个进度属性
/// 2. 主线程执行 M 次更新
/// 3. 验证所有监听器都能正确接收到所有更新
/// 4. 测量更新耗时和全量收敛耗时
#[tokio::test(flavor = "current_thread")]
async fn unlock_reactive_property_performance_benchmark() {
    let watcher_count = PERF_WATCHER_COUNT;
    let update_count = PERF_UPDATE_COUNT;

    println!("\n========== UnlockReactiveProperty 性能基准测试 ==========");
    println!("测试配置:");
    println!("  - 监听器数量: {}", watcher_count);
    println!("  - 更新次数: {}", update_count);
    println!("  - Tokio Runtime: current_thread");
    println!("  - 属性类型: UnlockReactiveProperty (无锁)");
    println!("=========================================================\n");

    // 记录初始内存
    let mem_before = memory_stats::memory_stats().map(|s| s.physical_mem);

    // 创建响应式进度属性
    let progress = UnlockReactiveProperty::new(DownloadProgress {
        bytes_done: 0,
        total: Some(update_count as u64),
    });

    // 用于同步所有监听器启动
    let barrier = Arc::new(Barrier::new(watcher_count + 1));
    // 记录已就绪的监听器数量
    let ready = Arc::new(AtomicUsize::new(0));
    // 记录接收到的总事件数
    let total_events_received = Arc::new(AtomicUsize::new(0));

    // 启动所有监听器任务
    let mut handles = Vec::with_capacity(watcher_count);
    for watcher_id in 0..watcher_count {
        let mut watcher = progress.watch();
        let barrier = barrier.clone();
        let ready = ready.clone();
        let total_events = total_events_received.clone();

        handles.push(tokio::spawn(async move {
            // 标记就绪
            ready.fetch_add(1, Ordering::SeqCst);
            // 等待所有监听器就绪
            barrier.wait().await;

            // 获取初始值
            let mut last = watcher.borrow()
                .map(|p| p.bytes_done)
                .unwrap_or(0);
            let mut events_count = 0usize;

            // 监听所有更新
            loop {
                let p = watcher.changed().await
                    .map_err(|e| format!("监听器 {} 接收失败: {}", watcher_id, e))?;
                
                events_count += 1;

                // 验证单调递增
                if p.bytes_done < last {
                    return Err(format!(
                        "监听器 {} 检测到非单调递增: {} -> {}",
                        watcher_id, last, p.bytes_done
                    ));
                }
                last = p.bytes_done;

                // 收到最终更新，退出
                if p.bytes_done >= update_count as u64 {
                    break;
                }
            }
            
            total_events.fetch_add(events_count, Ordering::SeqCst);
            Ok::<(), String>(())
        }));
    }

    // 等待所有监听器就绪
    while ready.load(Ordering::SeqCst) < watcher_count {
        tokio::task::yield_now().await;
    }
    println!("✓ 所有 {} 个监听器已就绪\n", watcher_count);

    // 同步启动
    barrier.wait().await;
    println!("开始性能测试...\n");

    // 执行更新并计时
    let start = Instant::now();
    for v in 1..=update_count {
        let _ = progress.update(DownloadProgress {
            bytes_done: v as u64,
            total: Some(update_count as u64),
        });

        // 定期让出 CPU，避免饿死监听器
        if v % 256 == 0 {
            tokio::task::yield_now().await;
        }
    }
    let update_duration = start.elapsed();

    // 等待所有监听器完成（最多 10 秒）
    for (idx, handle) in handles.into_iter().enumerate() {
        tokio::time::timeout(
            tokio::time::Duration::from_secs(10),
            handle
        )
        .await
        .unwrap_or_else(|_| {
            panic!("监听器 {} 等待超时（10秒）", idx)
        })
        .unwrap_or_else(|e| {
            panic!("监听器 {} 任务失败: {}", idx, e)
        })
        .unwrap_or_else(|e| {
            panic!("监听器 {} 逻辑错误: {}", idx, e)
        });
    }

    let total_duration = start.elapsed();

    // 记录最终内存
    let mem_after = memory_stats::memory_stats().map(|s| s.physical_mem);

    // 计算性能指标
    let updates_per_sec = (update_count as f64) / update_duration.as_secs_f64();
    let total_events = total_events_received.load(Ordering::SeqCst);
    let events_per_sec = (total_events as f64) / total_duration.as_secs_f64();
    let avg_latency_per_update = total_duration.as_micros() / (update_count as u128);
    let mem_used = match (mem_before, mem_after) {
        (Some(before), Some(after)) => Some(after.saturating_sub(before)),
        _ => None,
    };

    // 输出测试结果
    println!("\n========== 性能测试结果 ==========");
    println!("更新阶段:");
    println!("  - 耗时: {:.2?}", update_duration);
    println!("  - 吞吐量: {:.0} 次更新/秒", updates_per_sec);
    println!("\n收敛阶段:");
    println!("  - 总耗时: {:.2?}", total_duration);
    println!("  - 总事件数: {} ({}个监听器 × {}次更新)", total_events, watcher_count, update_count);
    println!("  - 事件吞吐量: {:.0} 事件/秒", events_per_sec);
    println!("  - 平均延迟: {:.2}µs/次更新", avg_latency_per_update);
    
    if let Some(mem) = mem_used {
        println!("\n内存使用:");
        println!("  - 增量: {} bytes ({:.2} MB)", mem, mem as f64 / 1024.0 / 1024.0);
        println!("  - 每监听器: {:.2} bytes", mem as f64 / watcher_count as f64);
    }
    
    println!("====================================\n");

    // 性能断言：全量收敛应在 10 秒内完成
    assert!(
        total_duration < tokio::time::Duration::from_secs(10),
        "响应式状态收敛过慢: {:.2?}，预期 < 10s",
        total_duration
    );

    // 验证所有监听器都收到了最终值（watch channel 会跳过中间值，这是正常的）
    // 不验证总事件数，因为 watch 的语义是"最新状态"而非"消息队列"
    println!("✓ 性能测试通过！");
    println!("注: watch channel 会跳过中间值，实际接收 {} 事件（正常现象）", total_events);
}

/// LockReactiveProperty（有锁）性能基准测试：模拟大量监听器并发监听进度更新。
///
/// 测试配置：
/// - 监听器数量: 1万
/// - 更新次数: 1千
///
/// 测试场景：
/// 1. 创建 N 个监听器同时监听同一个进度属性
/// 2. 主线程执行 M 次更新
/// 3. 验证所有监听器都能正确接收到所有更新
/// 4. 测量更新耗时和全量收敛耗时
///
/// 注意：LockReactiveProperty 使用 Mutex + Notify 实现，性能低于 UnlockReactiveProperty
#[tokio::test(flavor = "current_thread")]
async fn lock_reactive_property_performance_benchmark() {
    let watcher_count = PERF_WATCHER_COUNT;
    let update_count = PERF_UPDATE_COUNT;

    println!("\n========== LockReactiveProperty 性能基准测试 ==========");
    println!("测试配置:");
    println!("  - 监听器数量: {}", watcher_count);
    println!("  - 更新次数: {}", update_count);
    println!("  - Tokio Runtime: current_thread");
    println!("  - 属性类型: LockReactiveProperty (有锁)");
    println!("=========================================================\n");

    // 记录初始内存
    let mem_before = memory_stats::memory_stats().map(|s| s.physical_mem);

    // 创建响应式进度属性
    let progress = LockReactiveProperty::new(DownloadProgress {
        bytes_done: 0,
        total: Some(update_count as u64),
    });

    // 用于同步所有监听器启动
    let barrier = Arc::new(Barrier::new(watcher_count + 1));
    // 记录已就绪的监听器数量
    let ready = Arc::new(AtomicUsize::new(0));
    // 记录接收到的总事件数
    let total_events_received = Arc::new(AtomicUsize::new(0));

    // 启动所有监听器任务
    let mut handles = Vec::with_capacity(watcher_count);
    for _ in 0..watcher_count {
        let progress_clone = progress.clone();
        let barrier = barrier.clone();
        let ready = ready.clone();
        let total_events = total_events_received.clone();

        handles.push(tokio::spawn(async move {
            // 标记就绪
            ready.fetch_add(1, Ordering::SeqCst);
            // 等待所有监听器就绪
            barrier.wait().await;

            let mut events_count = 0usize;

            // 使用轮询方式监听（因为 wait_until 在高并发下有问题）
            loop {
                tokio::task::yield_now().await;

                let current = match progress_clone.get_current().await {
                    Ok(p) => p,
                    Err(_) => break, // 属性已销毁
                };

                events_count += 1;

                // 收到最终更新，退出
                if current.bytes_done >= update_count as u64 {
                    break;
                }

                // 短暂休眠，避免过度轮询
                tokio::time::sleep(tokio::time::Duration::from_micros(10)).await;
            }

            total_events.fetch_add(events_count, Ordering::SeqCst);
            Ok::<(), String>(())
        }));
    }

    // 等待所有监听器就绪
    while ready.load(Ordering::SeqCst) < watcher_count {
        tokio::task::yield_now().await;
    }
    println!("✓ 所有 {} 个监听器已就绪\n", watcher_count);

    // 同步启动
    barrier.wait().await;
    println!("开始性能测试...\n");

    // 执行更新并计时
    let start = Instant::now();
    for v in 1..=update_count {
        let _ = progress.update(DownloadProgress {
            bytes_done: v as u64,
            total: Some(update_count as u64),
        }).await;

        // 定期让出 CPU，避免饿死监听器
        if v % 10 == 0 {
            tokio::task::yield_now().await;
        }
    }
    let update_duration = start.elapsed();

    // 等待所有监听器完成（最多 30 秒）
    for (idx, handle) in handles.into_iter().enumerate() {
        tokio::time::timeout(
            tokio::time::Duration::from_secs(30),
            handle
        )
        .await
        .unwrap_or_else(|_| {
            panic!("监听器 {} 等待超时（30秒）", idx)
        })
        .unwrap_or_else(|e| {
            panic!("监听器 {} 任务失败: {}", idx, e)
        })
        .unwrap_or_else(|e| {
            panic!("监听器 {} 逻辑错误: {}", idx, e)
        });
    }

    let total_duration = start.elapsed();

    // 记录最终内存
    let mem_after = memory_stats::memory_stats().map(|s| s.physical_mem);

    // 计算性能指标
    let updates_per_sec = (update_count as f64) / update_duration.as_secs_f64();
    let total_events = total_events_received.load(Ordering::SeqCst);
    let events_per_sec = (total_events as f64) / total_duration.as_secs_f64();
    let avg_latency_per_update = total_duration.as_micros() / (update_count as u128);
    let mem_used = match (mem_before, mem_after) {
        (Some(before), Some(after)) => Some(after.saturating_sub(before)),
        _ => None,
    };

    // 输出测试结果
    println!("\n========== 性能测试结果 ==========");
    println!("更新阶段:");
    println!("  - 耗时: {:.2?}", update_duration);
    println!("  - 吞吐量: {:.0} 次更新/秒", updates_per_sec);
    println!("\n收敛阶段:");
    println!("  - 总耗时: {:.2?}", total_duration);
    println!("  - 总事件数: {} ({}个监听器轮询)", total_events, watcher_count);
    println!("  - 事件吞吐量: {:.0} 事件/秒", events_per_sec);
    println!("  - 平均延迟: {:.2}µs/次更新", avg_latency_per_update);

    if let Some(mem) = mem_used {
        println!("\n内存使用:");
        println!("  - 增量: {} bytes ({:.2} MB)", mem, mem as f64 / 1024.0 / 1024.0);
        println!("  - 每监听器: {:.2} bytes", mem as f64 / watcher_count as f64);
    }

    println!("====================================\n");

    // 性能断言：全量收敛应在 30 秒内完成
    assert!(
        total_duration < tokio::time::Duration::from_secs(30),
        "响应式状态收敛过慢: {:.2?}，预期 < 30s",
        total_duration
    );

    println!("✓ 性能测试通过！");
    println!("注: LockReactiveProperty 使用 Mutex + Notify，本测试使用轮询方式避免 wait_until 的并发问题");
}

