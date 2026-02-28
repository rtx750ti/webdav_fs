use crate::get_remote_files;
use crate::remote_file::DownloadStatus;
use crate::tests::{TestVendor, load_account_optional};
use std::sync::{Arc, Mutex};
use tokio::time::{Duration, sleep};

/// Windows 下本地保存目录
const SAVE_DIR: &str = r"C:\project\rust\quick-sync\temp-download-files";

/// 获取一个远程文件（非目录），无则返回 None（跳过测试）。
async fn require_one_remote_file()
-> Option<(crate::remote_file::RemoteFile, crate::auth::WebdavAuth)> {
    let auth = load_account_optional(TestVendor::Teracloud)?
        .to_webdav_auth()
        .ok()?;
    let results =
        get_remote_files(&auth, &["./新建文件夹/hula.exe"]).await;
    let file = results
        .into_iter()
        .find_map(|r| r.ok())
        .filter(|f| !f.data.is_dir)?;
    Some((file, auth))
}

/// 测试：单线程下载到内存
#[tokio::test]
async fn test_single_thread_download_to_memory() {
    let Some((file, auth)) = require_one_remote_file().await else {
        println!("⚠️  跳过测试：未找到远程文件");
        return;
    };

    println!("📥 开始单线程下载到内存: {}", file.data.name);
    println!("   文件大小: {:?} bytes", file.data.size);

    let downloader = file.download(auth).output_bytes();

    let result = downloader.send().await;

    match result {
        Ok(crate::remote_file::DownloadResult::Bytes(bytes)) => {
            println!("✅ 下载成功！");
            println!("   实际大小: {} bytes", bytes.len());
            if let Some(expected) = file.data.size {
                assert_eq!(bytes.len() as u64, expected, "文件大小不匹配");
            }
        }
        Ok(_) => panic!("❌ 返回类型错误，应该是 Bytes"),
        Err(e) => panic!("❌ 下载失败: {}", e),
    }
}

/// 测试：单线程下载到文件
#[tokio::test]
async fn test_single_thread_download_to_file() {
    let Some((file, auth)) = require_one_remote_file().await else {
        println!("⚠️  跳过测试：未找到远程文件");
        return;
    };

    let save_path =
        format!("{}/single_thread_{}", SAVE_DIR, file.data.name);
    println!("📥 开始单线程下载到文件: {}", save_path);

    // 确保目录存在
    tokio::fs::create_dir_all(SAVE_DIR).await.ok();

    let downloader = file.download(auth).save_to(&save_path);

    let result = downloader.send().await;

    match result {
        Ok(crate::remote_file::DownloadResult::SavedToLocal(path)) => {
            println!("✅ 下载成功！保存到: {}", path);

            // 验证文件存在
            let metadata =
                tokio::fs::metadata(&path).await.expect("文件不存在");
            println!("   文件大小: {} bytes", metadata.len());

            if let Some(expected) = file.data.size {
                assert_eq!(metadata.len(), expected, "文件大小不匹配");
            }
        }
        Ok(_) => panic!("❌ 返回类型错误，应该是 SavedToLocal"),
        Err(e) => panic!("❌ 下载失败: {}", e),
    }
}

/// 测试：多线程分片下载到内存
#[tokio::test]
async fn test_chunked_download_to_memory() {
    let Some((file, auth)) = require_one_remote_file().await else {
        println!("⚠️  跳过测试：未找到远程文件");
        return;
    };

    println!("📥 开始多线程分片下载到内存: {}", file.data.name);
    println!("   文件大小: {:?} bytes", file.data.size);

    let downloader = file
        .download(auth)
        .output_bytes()
        .max_chunks(4) // 4 个并发分片
        .chunk_size(512 * 1024); // 每片 512KB

    let result = downloader.send().await;

    match result {
        Ok(crate::remote_file::DownloadResult::ByteSegments(segments)) => {
            println!("✅ 分片下载成功！");
            println!("   总大小: {} bytes", segments.total_len());

            if let Some(expected) = file.data.size {
                assert_eq!(
                    segments.total_len(),
                    expected,
                    "文件大小不匹配"
                );
            }

            // 测试合并为完整字节
            let full_bytes = segments.to_bytes();
            println!("   合并后大小: {} bytes", full_bytes.len());
            assert_eq!(full_bytes.len() as u64, segments.total_len());

            // 测试按偏移读取
            if segments.total_len() > 100 {
                let partial = segments.read_at(10, 50);
                assert_eq!(partial.len(), 50, "偏移读取长度不匹配");
                println!("   偏移读取测试通过");
            }
        }
        Ok(_) => panic!("❌ 返回类型错误，应该是 ByteSegments"),
        Err(e) => panic!("❌ 下载失败: {}", e),
    }
}

/// 测试：多线程分片下载到文件
#[tokio::test]
async fn test_chunked_download_to_file() {
    let Some((file, auth)) = require_one_remote_file().await else {
        println!("⚠️  跳过测试：未找到远程文件");
        return;
    };

    let save_path = format!("{}/chunked_{}", SAVE_DIR, file.data.name);
    println!("📥 开始多线程分片下载到文件: {}", save_path);

    tokio::fs::create_dir_all(SAVE_DIR).await.ok();

    let downloader = file
        .download(auth)
        .save_to(&save_path)
        .max_chunks(8) // 8 个并发分片
        .chunk_size(256 * 1024) // 每片 256KB
        .max_retries(3); // 失败重试 3 次

    let result = downloader.send().await;

    match result {
        Ok(crate::remote_file::DownloadResult::SavedToLocal(path)) => {
            println!("✅ 分片下载成功！保存到: {}", path);

            let metadata =
                tokio::fs::metadata(&path).await.expect("文件不存在");
            println!("   文件大小: {} bytes", metadata.len());

            if let Some(expected) = file.data.size {
                assert_eq!(metadata.len(), expected, "文件大小不匹配");
            }
        }
        Ok(_) => panic!("❌ 返回类型错误，应该是 SavedToLocal"),
        Err(e) => panic!("❌ 下载失败: {}", e),
    }
}

/// 测试：暂停和恢复下载
#[tokio::test]
async fn test_pause_resume_download() {
    let Some((file, auth)) = require_one_remote_file().await else {
        println!("⚠️  跳过测试：未找到远程文件");
        return;
    };

    println!("⏸️  测试暂停/恢复功能: {}", file.data.name);

    let downloader = file
        .download(auth)
        .output_bytes()
        .max_chunks(4)
        .chunk_size(256 * 1024);

    let controller = downloader.get_controller();

    // 启动下载任务
    let download_handle =
        tokio::spawn(async move { downloader.send().await });

    // 等待一小段时间让下载开始
    sleep(Duration::from_millis(100)).await;

    // 暂停下载
    println!("   ⏸️  暂停下载...");
    controller.pause().ok();
    sleep(Duration::from_millis(500)).await;

    // 恢复下载
    println!("   ▶️  恢复下载...");
    controller.resume().ok();

    // 等待下载完成
    let result = download_handle.await.expect("任务失败");

    match result {
        Ok(_) => println!("✅ 暂停/恢复测试通过！"),
        Err(e) => panic!("❌ 下载失败: {}", e),
    }
}

/// 测试：取消下载
#[tokio::test]
async fn test_cancel_download() {
    let Some((file, auth)) = require_one_remote_file().await else {
        println!("⚠️  跳过测试：未找到远程文件");
        return;
    };

    println!("❌ 测试取消功能: {}", file.data.name);

    let downloader = file
        .download(auth)
        .output_bytes()
        .max_chunks(4)
        .chunk_size(128 * 1024);

    let controller = downloader.get_controller();

    // 启动下载任务
    let download_handle =
        tokio::spawn(async move { downloader.send().await });

    // 等待一小段时间让下载开始
    sleep(Duration::from_millis(100)).await;

    // 取消下载
    println!("   ❌ 取消下载...");
    controller.cancel().ok();

    // 等待任务结束
    let result = download_handle.await.expect("任务失败");

    match result {
        Err(crate::remote_file::DownloadError::Cancelled) => {
            println!("✅ 取消测试通过！");
        }
        Ok(_) => panic!("❌ 应该返回 Cancelled 错误"),
        Err(e) => panic!("❌ 错误类型不对: {}", e),
    }
}

/// 测试：订阅下载状态变化
#[tokio::test]
async fn test_subscribe_download_status() {
    let Some((file, auth)) = require_one_remote_file().await else {
        println!("⚠️  跳过测试：未找到远程文件");
        return;
    };

    println!("📊 测试状态订阅: {}", file.data.name);

    let downloader = file
        .download(auth)
        .output_bytes()
        .max_chunks(2)
        .chunk_size(512 * 1024);

    let controller = downloader.get_controller();

    // 订阅状态变化
    let status_log = Arc::new(Mutex::new(Vec::new()));
    {
        let status_log = Arc::clone(&status_log);

        controller.subscribe_download_status(true, move |status| {
            let mut log = status_log.lock().unwrap();
            let status_str = format!("{:?}", status);
            log.push(status_str.clone());
            println!("   📊 状态变化: {}", status_str);
        });
    }

    // 启动下载
    let result = downloader.send().await;

    // 验证结果
    match result {
        Ok(_) => {
            // 等待一小段时间确保所有状态都被记录
            sleep(Duration::from_millis(100)).await;

            let log = status_log.lock().unwrap();
            println!("✅ 状态订阅测试通过！");
            println!("   记录到 {} 次状态变化", log.len());
            println!("   状态序列: {:?}", log);

            // 验证至少有 Running 和 Finished 状态
            let has_running = log.iter().any(|s| s.contains("Running"));
            let has_finished = log.iter().any(|s| s.contains("Finished"));
            assert!(has_running, "应该有 Running 状态");
            assert!(has_finished, "应该有 Finished 状态");
        }
        Err(e) => panic!("❌ 下载失败: {}", e),
    }
}

/// 测试：订阅下载进度
#[tokio::test]
async fn test_subscribe_download_progress() {
    let Some((file, auth)) = require_one_remote_file().await else {
        println!("⚠️  跳过测试：未找到远程文件");
        return;
    };

    println!("📈 测试进度订阅: {}", file.data.name);

    let downloader = file
        .download(auth)
        .output_bytes()
        .max_chunks(4)
        .chunk_size(256 * 1024);

    let controller = downloader.get_controller();

    // 订阅进度变化
    let progress_count = Arc::new(Mutex::new(0usize));
    let last_bytes = Arc::new(Mutex::new(0u64));
    {
        let progress_count = Arc::clone(&progress_count);
        let last_bytes = Arc::clone(&last_bytes);
        controller.subscribe_downloaded_bytes(true, move |bytes| {
            *progress_count.lock().unwrap() += 1;
            *last_bytes.lock().unwrap() = bytes;
            // 每 100KB 打印一次进度
            if bytes % (100 * 1024) < 50 * 1024 {
                println!("   📈 进度: {} KB", bytes / 1024);
            }
        });
    }

    // 启动下载
    let result = downloader.send().await;

    // 验证结果
    match result {
        Ok(_) => {
            // 等待一小段时间确保所有进度都被记录
            sleep(Duration::from_millis(100)).await;

            let count = *progress_count.lock().unwrap();
            let final_bytes = *last_bytes.lock().unwrap();
            println!("✅ 进度订阅测试通过！");
            println!("   总共 {} 次进度更新", count);
            println!("   最终大小: {} bytes", final_bytes);

            assert!(count > 0, "应该有进度更新");
            if let Some(expected) = file.data.size {
                assert_eq!(final_bytes, expected, "最终大小应该匹配");
            }
        }
        Err(e) => panic!("❌ 下载失败: {}", e),
    }
}

/// 测试：订阅命令队列
#[tokio::test]
async fn test_subscribe_commands() {
    let Some((file, auth)) = require_one_remote_file().await else {
        println!("⚠️  跳过测试：未找到远程文件");
        return;
    };

    println!("🎛️  测试命令订阅: {}", file.data.name);

    let downloader = file
        .download(auth)
        .output_bytes()
        .max_chunks(4)
        .chunk_size(256 * 1024);

    let controller = downloader.get_controller();

    // 订阅命令
    let command_log = Arc::new(Mutex::new(Vec::new()));
    {
        let command_log = Arc::clone(&command_log);
        controller.subscribe_commands(move |cmd| {
            let mut log = command_log.lock().unwrap();
            let cmd_str = format!("{:?}", cmd);
            log.push(cmd_str.clone());
            println!("   🎛️  命令: {}", cmd_str);
        });
    }

    // 启动下载任务
    let download_handle =
        tokio::spawn(async move { downloader.send().await });

    // 等待下载开始
    sleep(Duration::from_millis(100)).await;

    // 发送暂停命令
    controller.pause().ok();
    sleep(Duration::from_millis(200)).await;

    // 发送恢复命令
    controller.resume().ok();

    // 等待完成
    let result = download_handle.await.expect("任务失败");

    match result {
        Ok(_) => {
            // 等待一小段时间确保所有命令都被记录
            sleep(Duration::from_millis(100)).await;

            let log = command_log.lock().unwrap();
            println!("✅ 命令订阅测试通过！");
            println!("   记录到 {} 条命令", log.len());
            println!("   命令序列: {:?}", log);

            // 验证至少有 Pause 和 Resume 命令
            let has_pause = log.iter().any(|s| s.contains("Pause"));
            let has_resume = log.iter().any(|s| s.contains("Resume"));
            assert!(has_pause, "应该有 Pause 命令");
            assert!(has_resume, "应该有 Resume 命令");
        }
        Err(e) => panic!("❌ 下载失败: {}", e),
    }
}

/// 测试：暂停/恢复时的状态和进度订阅
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_pause_resume_with_subscriptions() {
    let Some((file, auth)) = require_one_remote_file().await else {
        println!("⚠️  跳过测试：未找到远程文件");
        return;
    };

    println!("⏸️▶️📊 测试暂停/恢复 + 完整订阅: {}", file.data.name);

    let downloader = file
        .download(auth)
        .output_bytes()
        .max_chunks(1)
        .chunk_size(64 * 1024);

    let controller = downloader.get_controller();

    // 订阅状态
    let status_log = Arc::new(Mutex::new(Vec::new()));
    {
        let status_log = Arc::clone(&status_log);
        controller.subscribe_download_status(true, move |status| {
            let mut log = status_log.lock().unwrap();
            let status_str = format!("{:?}", status);
            log.push(status_str.clone());
            println!("   📊 状态: {}", status_str);
        });
    }

    // 订阅进度
    let progress_updates = Arc::new(Mutex::new(0usize));
    {
        let progress_updates = Arc::clone(&progress_updates);
        controller.subscribe_downloaded_bytes(true, move |bytes| {
            *progress_updates.lock().unwrap() += 1;
            if bytes % (200 * 1024) < 100 * 1024 {
                // println!("   📈 进度: {} KB", bytes / 1024);
            }
        });
    }

    // 启动下载任务
    let download_handle =
        tokio::spawn(async move { downloader.send().await });

    // 等待下载进度达到一定比例后再暂停（更可靠的方式）
    let file_size = file.data.size.unwrap_or(1024 * 1024);
    let pause_threshold = file_size / 4; // 下载 25% 后暂停

    println!("   ⏳ 等待下载进度达到 {} KB...", pause_threshold / 1024);
    loop {
        let ctrl = controller.clone();
        let current_bytes = ctrl.get_downloaded_bytes();
        let status = ctrl.get_download_status();

        // 如果下载已经完成（文件太小或下载太快），直接跳过暂停测试
        if matches!(status, Some(DownloadStatus::Finished)) {
            drop(ctrl);
            println!("   ⚠️  文件太小或下载太快，已完成，跳过暂停测试");
            let result = download_handle.await.expect("任务失败");
            assert!(result.is_ok(), "下载应该成功");
            return;
        }

        // 达到阈值且未完成，可以暂停
        if current_bytes >= pause_threshold {
            drop(ctrl);
            break;
        }

        drop(ctrl);
        sleep(Duration::from_millis(10)).await;
    }

    // 暂停
    println!("   ⏸️  暂停...");
    controller.pause().ok();
    sleep(Duration::from_millis(500)).await;

    // 恢复
    println!("   ▶️  恢复...");
    controller.resume().ok();

    // 等待完成
    let result = download_handle.await.expect("任务失败");

    match result {
        Ok(_) => {
            // 等待一小段时间确保所有订阅都完成
            sleep(Duration::from_millis(100)).await;

            let log = status_log.lock().unwrap();
            let updates = *progress_updates.lock().unwrap();

            println!("✅ 暂停/恢复 + 完整订阅测试通过！");
            println!("   状态变化: {} 次", log.len());
            println!("   进度更新: {} 次", updates);
            println!("   状态序列: {:?}", log);

            // 验证状态序列包含关键状态
            let has_running = log.iter().any(|s| s.contains("Running"));
            let has_paused = log.iter().any(|s| s.contains("Paused"));
            let has_finished = log.iter().any(|s| s.contains("Finished"));

            assert!(has_running, "应该有 Running 状态");
            assert!(has_paused, "应该有 Paused 状态");
            assert!(has_finished, "应该有 Finished 状态");
            assert!(updates > 0, "应该有进度更新");
        }
        Err(e) => panic!("❌ 下载失败: {}", e),
    }
}
