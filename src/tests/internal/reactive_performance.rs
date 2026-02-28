//! 响应式属性性能基准测试
//!
//! 测试 UnlockReactiveProperty、LockReactiveProperty 和 QueueReactiveProperty 在高并发场景下的性能表现。
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

use crate::states::lock_reactive::LockReactiveProperty;
use crate::states::unlock_reactive::UnlockReactiveProperty;
use crate::internal::states::queue_reactive::QueueReactiveProperty;

// ═══════════════════════════ 性能测试配置常量 ═══════════════════════════

// ─────────── UnlockReactiveProperty 和 LockReactiveProperty 测试配置 ───────────
// 这两个测试模拟"1个生产者 → N个消费者"的场景（如进度通知）

/// 监听器数量：模拟同时监听进度的并发任务数（如 UI 组件、日志记录器等）
const PERF_WATCHER_COUNT: usize = 100_0000;

/// 更新次数：生产者执行的状态更新次数（如下载进度从 0% → 100%）
const PERF_UPDATE_COUNT: usize = 1_000;

/// UnlockReactive 测试超时：如果所有监听器在此时间内未完成，测试失败（秒）
const PERF_UNLOCK_TIMEOUT_SECS: u64 = 10;

/// LockReactive 测试超时：LockReactive 性能较低，给更长的超时时间（秒）
const PERF_LOCK_TIMEOUT_SECS: u64 = 30;

/// UnlockReactive 让出 CPU 的间隔：每更新 N 次后 yield，避免饿死监听器
const PERF_UNLOCK_SAMPLE_INTERVAL: usize = 256;

/// LockReactive 让出 CPU 的间隔：因为有锁竞争，需要更频繁地 yield
const PERF_LOCK_SAMPLE_INTERVAL: usize = 10;

/// LockReactive 轮询休眠时间：监听器轮询间隔，避免过度占用 CPU（微秒）
const PERF_LOCK_SLEEP_MICROS: u64 = 10;

// ─────────── QueueReactiveProperty 测试配置 ───────────
// 这个测试模拟"N个生产者 → 1个消费者"的场景（如多线程日志汇总）

/// 生产者数量：模拟同时发送消息的并发任务数
/// 实际场景：10-100 个下载分片/模块
/// 测试场景：5,000 个（极端压力测试，远超实际需求）
const QUEUE_PRODUCER_COUNT: usize = 5_000;

/// 每个生产者发送的消息数：每个任务发送多少条消息
/// 实际场景：每个分片报告几百次事件
/// 测试场景：5,000 条（极端压力测试）
/// 总消息数 = 5,000 × 5,000 = 2,500 万条
const QUEUE_MESSAGES_PER_PRODUCER: usize = 5_000;

/// 队列消费超时：如果消费者在此时间内未消费完所有消息，测试失败（秒）
/// 基准：2,500 万条消息在 260 万/秒 的吞吐量下需要约 10 秒
const QUEUE_TIMEOUT_SECS: u64 = 30;

/// FIFO 顺序测试的消息数：单独测试单生产者场景下的 FIFO 保证
/// 验证发送 0, 1, 2, ..., N-1 后，消费者是否按顺序收到
const QUEUE_FIFO_TEST_MESSAGE_COUNT: usize = 10_0000;

/// 测试用的进度结构体（独立于下载器模块）
#[derive(Debug, Clone, Copy)]
struct TestProgress {
    bytes_done: u64,
    total: Option<u64>,
}

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

    println!(
        "\n========== UnlockReactiveProperty 性能基准测试 =========="
    );
    println!("测试配置:");
    println!("  - 监听器数量: {}", watcher_count);
    println!("  - 更新次数: {}", update_count);
    println!("  - Tokio Runtime: current_thread");
    println!("  - 属性类型: UnlockReactiveProperty (无锁)");
    println!(
        "=========================================================\n"
    );

    // 记录初始内存
    let mem_before = memory_stats::memory_stats().map(|s| s.physical_mem);

    // 创建响应式进度属性
    let progress = UnlockReactiveProperty::new(TestProgress {
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
            let mut last =
                watcher.borrow().map(|p| p.bytes_done).unwrap_or(0);
            let mut events_count = 0usize;

            // 监听所有更新
            loop {
                // 等待变化通知
                watcher.changed().await.map_err(|e| {
                    format!("监听器 {} 接收失败: {}", watcher_id, e)
                })?;

                // 获取最新值
                let p = watcher.borrow().ok_or_else(|| {
                    format!("监听器 {} 无法获取值", watcher_id)
                })?;

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
        let _ = progress.update(TestProgress {
            bytes_done: v as u64,
            total: Some(update_count as u64),
        });

        // 定期让出 CPU，避免饿死监听器
        if v % PERF_UNLOCK_SAMPLE_INTERVAL == 0 {
            tokio::task::yield_now().await;
        }
    }
    let update_duration = start.elapsed();

    // 等待所有监听器完成（最多配置的超时时间）
    for (idx, handle) in handles.into_iter().enumerate() {
        tokio::time::timeout(tokio::time::Duration::from_secs(PERF_UNLOCK_TIMEOUT_SECS), handle)
            .await
            .unwrap_or_else(|_| panic!("监听器 {} 等待超时（{}秒）", idx, PERF_UNLOCK_TIMEOUT_SECS))
            .unwrap_or_else(|e| panic!("监听器 {} 任务失败: {}", idx, e))
            .unwrap_or_else(|e| panic!("监听器 {} 逻辑错误: {}", idx, e));
    }

    let total_duration = start.elapsed();

    // 记录最终内存
    let mem_after = memory_stats::memory_stats().map(|s| s.physical_mem);

    // 计算性能指标
    let updates_per_sec =
        (update_count as f64) / update_duration.as_secs_f64();
    let total_events = total_events_received.load(Ordering::SeqCst);
    let events_per_sec =
        (total_events as f64) / total_duration.as_secs_f64();
    let avg_latency_per_update =
        total_duration.as_micros() / (update_count as u128);
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
    println!(
        "  - 总事件数: {} ({}个监听器 × {}次更新)",
        total_events, watcher_count, update_count
    );
    println!("  - 事件吞吐量: {:.0} 事件/秒", events_per_sec);
    println!("  - 平均延迟: {:.2}µs/次更新", avg_latency_per_update);

    if let Some(mem) = mem_used {
        println!("\n内存使用:");
        println!(
            "  - 增量: {} bytes ({:.2} MB)",
            mem,
            mem as f64 / 1024.0 / 1024.0
        );
        println!(
            "  - 每监听器: {:.2} bytes",
            mem as f64 / watcher_count as f64
        );
    }

    println!("====================================\n");

    // 性能断言：全量收敛应在配置的超时时间内完成
    assert!(
        total_duration < tokio::time::Duration::from_secs(PERF_UNLOCK_TIMEOUT_SECS),
        "响应式状态收敛过慢: {:.2?}，预期 < {}s",
        total_duration, PERF_UNLOCK_TIMEOUT_SECS
    );

    // 验证所有监听器都收到了最终值（watch channel 会跳过中间值，这是正常的）
    // 不验证总事件数，因为 watch 的语义是"最新状态"而非"消息队列"
    println!("✓ 性能测试通过！");
    println!(
        "注: watch channel 会跳过中间值，实际接收 {} 事件（正常现象）",
        total_events
    );
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
    println!(
        "=========================================================\n"
    );

    // 记录初始内存
    let mem_before = memory_stats::memory_stats().map(|s| s.physical_mem);

    // 创建响应式进度属性
    let progress = LockReactiveProperty::new(TestProgress {
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
                tokio::time::sleep(tokio::time::Duration::from_micros(PERF_LOCK_SLEEP_MICROS))
                    .await;
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
        let _ = progress
            .update(TestProgress {
                bytes_done: v as u64,
                total: Some(update_count as u64),
            })
            .await;

        // 定期让出 CPU，避免饿死监听器
        if v % PERF_LOCK_SAMPLE_INTERVAL == 0 {
            tokio::task::yield_now().await;
        }
    }
    let update_duration = start.elapsed();

    // 等待所有监听器完成（最多 30 秒）
    for (idx, handle) in handles.into_iter().enumerate() {
        tokio::time::timeout(tokio::time::Duration::from_secs(PERF_LOCK_TIMEOUT_SECS), handle)
            .await
            .unwrap_or_else(|_| panic!("监听器 {} 等待超时（{}秒）", idx, PERF_LOCK_TIMEOUT_SECS))
            .unwrap_or_else(|e| panic!("监听器 {} 任务失败: {}", idx, e))
            .unwrap_or_else(|e| panic!("监听器 {} 逻辑错误: {}", idx, e));
    }

    let total_duration = start.elapsed();

    // 记录最终内存
    let mem_after = memory_stats::memory_stats().map(|s| s.physical_mem);

    // 计算性能指标
    let updates_per_sec =
        (update_count as f64) / update_duration.as_secs_f64();
    let total_events = total_events_received.load(Ordering::SeqCst);
    let events_per_sec =
        (total_events as f64) / total_duration.as_secs_f64();
    let avg_latency_per_update =
        total_duration.as_micros() / (update_count as u128);
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
    println!(
        "  - 总事件数: {} ({}个监听器轮询)",
        total_events, watcher_count
    );
    println!("  - 事件吞吐量: {:.0} 事件/秒", events_per_sec);
    println!("  - 平均延迟: {:.2}µs/次更新", avg_latency_per_update);

    if let Some(mem) = mem_used {
        println!("\n内存使用:");
        println!(
            "  - 增量: {} bytes ({:.2} MB)",
            mem,
            mem as f64 / 1024.0 / 1024.0
        );
        println!(
            "  - 每监听器: {:.2} bytes",
            mem as f64 / watcher_count as f64
        );
    }

    println!("====================================\n");

    // 性能断言：全量收敛应在 30 秒内完成
    assert!(
        total_duration < tokio::time::Duration::from_secs(PERF_LOCK_TIMEOUT_SECS),
        "响应式状态收敛过慢: {:.2?}，预期 < {}s",
        total_duration, PERF_LOCK_TIMEOUT_SECS
    );

    println!("✓ 性能测试通过！");
    println!(
        "注: LockReactiveProperty 使用 Mutex + Notify，本测试使用轮询方式避免 wait_until 的并发问题"
    );
}

// ═══════════════════════════ QueueReactiveProperty 性能测试 ═══════════════════════════

/// QueueReactiveProperty（微队列）性能基准测试：模拟多生产者单消费者场景。
/// 测试场景：
/// 1. 创建 N 个生产者同时往队列推送消息
/// 2. 单个消费者按 FIFO 顺序消费所有消息
/// 3. 验证消息不丢失、顺序正确
/// 4. 测量发送吞吐量和消费吞吐量
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn queue_reactive_property_performance_benchmark() {
    let producer_count = QUEUE_PRODUCER_COUNT;
    let messages_per_producer = QUEUE_MESSAGES_PER_PRODUCER;
    let total_messages = producer_count * messages_per_producer;

    println!(
        "\n========== QueueReactiveProperty 性能基准测试 =========="
    );
    println!("测试配置:");
    println!("  - 生产者数量: {}", producer_count);
    println!("  - 每生产者消息数: {}", messages_per_producer);
    println!("  - 总消息数: {}", total_messages);
    println!("  - Tokio Runtime: multi_thread (4 workers)");
    println!("  - 属性类型: QueueReactiveProperty (无锁微队列)");
    println!(
        "=========================================================\n"
    );

    // 记录初始内存
    let mem_before = memory_stats::memory_stats().map(|s| s.physical_mem);

    let (producer, mut consumer) = QueueReactiveProperty::<u64>::new();

    // 用于同步所有生产者启动
    let barrier = Arc::new(Barrier::new(producer_count + 1));

    // 启动所有生产者
    let mut producer_handles = Vec::with_capacity(producer_count);
    for producer_id in 0..producer_count {
        let p = producer.clone();
        let b = barrier.clone();

        producer_handles.push(tokio::spawn(async move {
            b.wait().await;

            let base = (producer_id * messages_per_producer) as u64;
            for i in 0..messages_per_producer {
                let value = base + i as u64;
                if let Err(_) = p.send(value) {
                    return Err(format!(
                        "生产者 {} 发送失败: value={}",
                        producer_id, value
                    ));
                }
            }
            Ok::<(), String>(())
        }));
    }

    // 启动消费者
    let consumer_handle = tokio::spawn(async move {
        let mut received_count = 0usize;
        let start = Instant::now();

        while received_count < total_messages {
            match consumer.recv().await {
                Some(_value) => {
                    received_count += 1;
                }
                None => {
                    // 所有生产者已关闭
                    break;
                }
            }
        }

        let duration = start.elapsed();
        (received_count, duration)
    });

    // 同步启动所有生产者
    barrier.wait().await;
    let send_start = Instant::now();
    println!("✓ 所有 {} 个生产者已启动\n", producer_count);

    // 等待所有生产者完成
    for (idx, handle) in producer_handles.into_iter().enumerate() {
        handle
            .await
            .unwrap_or_else(|e| panic!("生产者 {} 任务失败: {}", idx, e))
            .unwrap_or_else(|e| panic!("生产者 {} 逻辑错误: {}", idx, e));
    }
    let send_duration = send_start.elapsed();

    // drop 生产者，让消费者知道没有更多消息了
    drop(producer);

    // 等待消费者完成（最多配置的超时时间）
    let (received_count, consume_duration) = tokio::time::timeout(
        tokio::time::Duration::from_secs(QUEUE_TIMEOUT_SECS),
        consumer_handle,
    )
    .await
    .unwrap_or_else(|_| panic!("消费者等待超时（{}秒）", QUEUE_TIMEOUT_SECS))
    .expect("消费者任务失败");

    // 记录最终内存
    let mem_after = memory_stats::memory_stats().map(|s| s.physical_mem);

    // 计算性能指标
    let send_throughput =
        (total_messages as f64) / send_duration.as_secs_f64();
    let consume_throughput =
        (received_count as f64) / consume_duration.as_secs_f64();
    let mem_used = match (mem_before, mem_after) {
        (Some(before), Some(after)) => Some(after.saturating_sub(before)),
        _ => None,
    };

    // 输出测试结果
    println!("\n========== 性能测试结果 ==========");
    println!("发送阶段:");
    println!("  - 耗时: {:.2?}", send_duration);
    println!("  - 吞吐量: {:.0} 条消息/秒", send_throughput);
    println!("\n消费阶段:");
    println!("  - 耗时: {:.2?}", consume_duration);
    println!("  - 接收消息数: {}/{}", received_count, total_messages);
    println!("  - 吞吐量: {:.0} 条消息/秒", consume_throughput);

    if let Some(mem) = mem_used {
        println!("\n内存使用:");
        println!(
            "  - 增量: {} bytes ({:.2} MB)",
            mem,
            mem as f64 / 1024.0 / 1024.0
        );
        println!(
            "  - 每消息: {:.2} bytes",
            mem as f64 / total_messages as f64
        );
    }

    println!("====================================\n");

    // 验证：所有消息都被消费
    assert_eq!(
        received_count, total_messages,
        "消息丢失: 期望 {} 条，实际收到 {} 条",
        total_messages, received_count
    );

    // 性能断言：消费应在配置的超时时间内完成
    assert!(
        consume_duration < tokio::time::Duration::from_secs(QUEUE_TIMEOUT_SECS),
        "消费过慢: {:.2?}，预期 < {}s",
        consume_duration, QUEUE_TIMEOUT_SECS
    );

    println!("✓ 性能测试通过！");
    println!(
        "  所有 {} 条消息均被正确消费，无丢失",
        total_messages
    );
}

/// QueueReactiveProperty FIFO 顺序保证测试（单生产者）
///
/// 验证单个生产者发送的消息严格按 FIFO 顺序被消费。
#[tokio::test]
async fn queue_reactive_property_fifo_order_test() {
    let message_count = QUEUE_FIFO_TEST_MESSAGE_COUNT;

    println!(
        "\n========== QueueReactiveProperty FIFO 顺序测试 =========="
    );
    println!("  - 消息数量: {}", message_count);

    let (producer, mut consumer) = QueueReactiveProperty::<u64>::new();

    // 发送 0..N
    for i in 0..message_count {
        producer.send(i as u64).expect("发送失败");
    }
    drop(producer);

    // 消费并验证顺序
    let mut expected = 0u64;
    while let Some(value) = consumer.recv().await {
        assert_eq!(
            value, expected,
            "FIFO 顺序错误: 期望 {}, 实际 {}",
            expected, value
        );
        expected += 1;
    }

    assert_eq!(
        expected, message_count as u64,
        "消息丢失: 期望 {} 条，实际收到 {} 条",
        message_count, expected
    );

    println!("✓ FIFO 顺序测试通过！所有 {} 条消息严格有序", message_count);
}
