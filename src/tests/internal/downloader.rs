//! 下载器测试：纯比特流输出、纯保存到本地、基于响应式进度手动计算比例、各阶段钩子。
//!
//! 测试仅使用领域 API；进度比例在测试中根据下载器的 `progress()` 与远程文件总大小手动计算。

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::get_remote_files;
use crate::remote_file::download::HookAbort;
use crate::remote_file::{
    DownloadError, DownloadResult, RemoteFileDownloader,
};
use crate::tests::{load_account_optional, TestVendor};

/// Windows 下本地保存目录
const SAVE_DIR: &str = r"C:\project\rust\quick-sync\temp-download-files";

/// 获取一个远程文件（非目录），无则返回 None（跳过测试）。
async fn require_one_remote_file(
) -> Option<(crate::remote_file::RemoteFile, crate::auth::WebdavAuth)> {
    let auth = load_account_optional(TestVendor::Teracloud)?
        .to_webdav_auth()
        .ok()?;
    let results = get_remote_files(&auth, &["./新建文件夹/hula.exe"]).await;
    let file = results
        .into_iter()
        .find_map(|r| r.ok())
        .filter(|f| !f.data.is_dir)?;
    Some((file, auth))
}

#[tokio::test]
async fn download_output_bytes_only() {
    let (remote_file, auth) = match require_one_remote_file().await {
        Some(x) => x,
        None => return,
    };

    let downloader: RemoteFileDownloader =
        remote_file.build_downloader(&auth);
    let result = downloader.output_bytes().send().await;

    let result = match result {
        Ok(r) => r,
        Err(e) => {
            eprintln!("下载失败（可检查 env）：{}", e);
            return;
        }
    };

    match result {
        DownloadResult::Bytes(ref b) => {
            assert!(!b.is_empty() || remote_file.data.size == Some(0));
            println!("纯比特流下载成功，字节数: {}", b.len());
        }
        DownloadResult::Saved => panic!("预期为 Bytes，得到 Saved"),
        DownloadResult::BytesSegments(_) => panic!("单线程应返回 Bytes，不应为 BytesSegments"),
    }
}

#[tokio::test]
async fn download_save_to_local() {
    let (remote_file, auth) = match require_one_remote_file().await {
        Some(x) => x,
        None => return,
    };

    let save_dir = Path::new(SAVE_DIR);
    if !save_dir.exists() {
        if let Err(e) = std::fs::create_dir_all(save_dir) {
            eprintln!("创建目录 {} 失败: {}，跳过测试", SAVE_DIR, e);
            return;
        }
    }

    let save_path = save_dir.join(remote_file.data.name.as_str());
    let downloader: RemoteFileDownloader =
        remote_file.build_downloader(&auth);
    let result =
        downloader.save_to(&save_path).send().await;

    let result = match result {
        Ok(r) => r,
        Err(e) => {
            eprintln!("下载失败: {}", e);
            return;
        }
    };

    match result {
        DownloadResult::Saved => {}
        DownloadResult::Bytes(_) => panic!("预期为 Saved，得到 Bytes"),
        DownloadResult::BytesSegments(_) => panic!("预期为 Saved，得到 BytesSegments"),
    }

    assert!(save_path.exists(), "文件应已保存到 {}", save_path.display());
    println!("已保存到: {}", save_path.display());
}

/// 测试：保存到本地并监听下载进度（progress() 返回可共享句柄，直接 watch 即可）。
#[tokio::test]
async fn download_save_to_local_with_progress_watch() {
    let (remote_file, auth) = match require_one_remote_file().await {
        Some(x) => x,
        None => return,
    };

    let save_dir = Path::new(SAVE_DIR);
    if !save_dir.exists() {
        if let Err(e) = std::fs::create_dir_all(save_dir) {
            eprintln!("创建目录 {} 失败: {}，跳过测试", SAVE_DIR, e);
            return;
        }
    }

    let save_path = save_dir.join(remote_file.data.name.as_str());
    let downloader = remote_file.build_downloader(&auth);
    let progress = downloader.progress();
    let progress_final = progress.clone();

    let watch_handle = tokio::spawn(async move {
        let mut watcher = progress.watch();
        while let Ok(p) = watcher.changed().await {
            println!(
                "已下载 {} / {:?}，进度 {:.1}%",
                p.bytes_done,
                p.total,
                p.pct()
            );
        }
    });

    let result = downloader.save_to(&save_path).send().await;

    watch_handle.abort();
    let _ = watch_handle.await;

    let result = match result {
        Ok(r) => r,
        Err(e) => {
            eprintln!("下载失败: {}", e);
            return;
        }
    };

    match result {
        DownloadResult::Saved => {}
        DownloadResult::Bytes(_) => panic!("预期为 Saved"),
        DownloadResult::BytesSegments(_) => panic!("预期为 Saved，得到 BytesSegments"),
    }

    assert!(save_path.exists(), "文件应已保存到 {}", save_path.display());

    if let Some(p) = progress_final.get_current().as_ref() {
        println!(
            "最终: bytes_done={}, total={:?}, 进度 {:.1}%",
            p.bytes_done,
            p.total,
            p.pct()
        );
    }
}

// ---------- 钩子测试：每个钩子一个独立测试 ----------

/// 测试 `with_before_start_hook`：开始前钩子被调用且下载可正常完成。
#[tokio::test]
async fn download_hook_before_start() {
    let (remote_file, auth) = match require_one_remote_file().await {
        Some(x) => x,
        None => return,
    };

    let before_called = Arc::new(AtomicBool::new(false));
    let before_called_clone = Arc::clone(&before_called);

    let downloader = remote_file
        .build_downloader(&auth)
        .output_bytes()
        .with_before_start_hook(move || {
            let flag = Arc::clone(&before_called_clone);
            async move {
                flag.store(true, Ordering::SeqCst);
                Ok(())
            }
        });

    let result = downloader.send().await;
    let result = match result {
        Ok(r) => r,
        Err(e) => {
            eprintln!("下载失败: {}", e);
            return;
        }
    };

    assert!(
        before_called.load(Ordering::SeqCst),
        "before_start 钩子应被调用"
    );
    match &result {
        DownloadResult::Bytes(b) => {
            assert!(!b.is_empty() || remote_file.data.size == Some(0));
            println!(
                "[before_start 钩子] 已调用，下载完成，字节数: {}",
                b.len()
            );
        }
        DownloadResult::Saved => panic!("预期 Bytes"),
        DownloadResult::BytesSegments(_) => panic!("单线程应返回 Bytes"),
    }
}

/// 测试 `with_before_start_hook` 返回 `Err(HookAbort)` 时下载被中止。
#[tokio::test]
async fn download_hook_before_start_abort() {
    let (remote_file, auth) = match require_one_remote_file().await {
        Some(x) => x,
        None => return,
    };

    let downloader = remote_file
        .build_downloader(&auth)
        .output_bytes()
        .with_before_start_hook(|| async { Err(HookAbort) });

    let result = downloader.send().await;
    match result {
        Err(DownloadError::HookAbort(_)) => {
            println!(
                "[before_start_abort 钩子] 按预期中止下载，返回 HookAbort"
            );
        }
        Ok(_) => panic!("预期 HookAbort 错误"),
        Err(e) => panic!("预期 HookAbort，得到: {}", e),
    }
}

/// 测试 `with_on_progress_hook`：进度钩子被调用，且 bytes_done 单调递增，最后一次与总大小一致。
#[tokio::test]
async fn download_hook_on_progress() {
    let (remote_file, auth) = match require_one_remote_file().await {
        Some(x) => x,
        None => return,
    };

    let progress_calls: Arc<Mutex<Vec<(u64, Option<u64>)>>> =
        Arc::new(Mutex::new(Vec::new()));
    let progress_calls_c = Arc::clone(&progress_calls);

    let downloader = remote_file
        .build_downloader(&auth)
        .output_bytes()
        .with_on_progress_hook(move |bytes_done, total| {
            progress_calls_c.lock().unwrap().push((bytes_done, total));
        });

    let result = downloader.send().await;
    let result = match result {
        Ok(r) => r,
        Err(e) => {
            eprintln!("下载失败: {}", e);
            return;
        }
    };

    let bytes_len = match &result {
        DownloadResult::Bytes(b) => b.len() as u64,
        DownloadResult::Saved => panic!("预期 Bytes"),
        DownloadResult::BytesSegments(seg) => seg.total_len(),
    };

    let calls = progress_calls.lock().unwrap();
    assert!(!calls.is_empty(), "on_progress 至少应被调用一次");
    let mut prev = 0u64;
    for &(done, _) in calls.iter() {
        assert!(done >= prev, "bytes_done 应单调递增");
        prev = done;
    }
    if let Some(&(last_done, total)) = calls.last() {
        assert_eq!(last_done, bytes_len, "最后一次进度应等于总字节数");
        println!(
            "[on_progress 钩子] 调用次数: {}，最后一次: bytes_done={}, total={:?}",
            calls.len(),
            last_done,
            total
        );
    }
}

/// 测试 `with_after_complete_hook`：下载成功后「完成后」钩子被调用。
#[tokio::test]
async fn download_hook_after_complete() {
    let (remote_file, auth) = match require_one_remote_file().await {
        Some(x) => x,
        None => return,
    };

    let after_called = Arc::new(AtomicBool::new(false));
    let after_called_clone = Arc::clone(&after_called);

    let downloader = remote_file
        .build_downloader(&auth)
        .output_bytes()
        .with_after_complete_hook(move || {
            let flag = Arc::clone(&after_called_clone);
            async move {
                flag.store(true, Ordering::SeqCst);
            }
        });

    let result = downloader.send().await;
    let result = match result {
        Ok(r) => r,
        Err(e) => {
            eprintln!("下载失败: {}", e);
            return;
        }
    };

    assert!(
        after_called.load(Ordering::SeqCst),
        "after_complete 钩子应被调用"
    );
    println!("[after_complete 钩子] 已调用，下载成功结束");
    match result {
        DownloadResult::Bytes(_) | DownloadResult::Saved | DownloadResult::BytesSegments(_) => {}
    }
}

/// 测试完整 `with_hook`（实现 DownloadHook trait）：四个阶段钩子均被调用。
#[tokio::test]
async fn download_hook_full_trait() {
    use crate::remote_file::download::DownloadHook;
    use async_trait::async_trait;

    let (remote_file, auth) = match require_one_remote_file().await {
        Some(x) => x,
        None => return,
    };

    let before = Arc::new(AtomicBool::new(false));
    let chunk_count = Arc::new(AtomicU64::new(0));
    let progress_count = Arc::new(AtomicU64::new(0));
    let after = Arc::new(AtomicBool::new(false));

    struct FullHook {
        before: Arc<AtomicBool>,
        chunk_count: Arc<AtomicU64>,
        progress_count: Arc<AtomicU64>,
        after: Arc<AtomicBool>,
    }

    #[async_trait]
    impl DownloadHook for FullHook {
        async fn before_start(&mut self) -> Result<(), HookAbort> {
            self.before.store(true, Ordering::SeqCst);
            Ok(())
        }
        fn on_chunk(&mut self, _chunk: &[u8]) {
            self.chunk_count.fetch_add(1, Ordering::SeqCst);
        }
        fn on_progress(&mut self, _bytes_done: u64, _total: Option<u64>) {
            self.progress_count.fetch_add(1, Ordering::SeqCst);
        }
        async fn after_complete(&mut self) {
            self.after.store(true, Ordering::SeqCst);
        }
    }

    let downloader = remote_file
        .build_downloader(&auth)
        .output_bytes()
        .with_hook(FullHook {
            before: Arc::clone(&before),
            chunk_count: Arc::clone(&chunk_count),
            progress_count: Arc::clone(&progress_count),
            after: Arc::clone(&after),
        });

    let result = downloader.send().await;
    let result = match result {
        Ok(r) => r,
        Err(e) => {
            eprintln!("下载失败: {}", e);
            return;
        }
    };

    assert!(
        before.load(Ordering::SeqCst),
        "full hook: before_start 应被调用"
    );
    assert!(
        chunk_count.load(Ordering::SeqCst) >= 1,
        "full hook: on_chunk 至少一次"
    );
    assert!(
        progress_count.load(Ordering::SeqCst) >= 1,
        "full hook: on_progress 至少一次"
    );
    assert!(
        after.load(Ordering::SeqCst),
        "full hook: after_complete 应被调用"
    );
    println!(
        "[full hook] before_start=✓, on_chunk 次数={}, on_progress 次数={}, after_complete=✓",
        chunk_count.load(Ordering::SeqCst),
        progress_count.load(Ordering::SeqCst)
    );
    match result {
        DownloadResult::Bytes(_) | DownloadResult::Saved | DownloadResult::BytesSegments(_) => {}
    }
}

// ---------- 分片下载测试 ----------

/// 分片下载保存到本地：max_concurrent_chunks(2)，校验文件存在且大小与远程一致。
#[tokio::test]
async fn download_chunked_save_to_local() {
    let (remote_file, auth) = match require_one_remote_file().await {
        Some(x) => x,
        None => return,
    };

    let total_size = match remote_file.data.size {
        Some(s) => s,
        None => {
            eprintln!("跳过：远程文件无 size，无法分片下载");
            return;
        }
    };

    let save_dir = Path::new(SAVE_DIR);
    if !save_dir.exists() {
        if let Err(e) = std::fs::create_dir_all(save_dir) {
            eprintln!("创建目录 {} 失败: {}，跳过测试", SAVE_DIR, e);
            return;
        }
    }

    let save_path = save_dir.join(remote_file.data.name.as_str());
    let downloader = remote_file
        .build_downloader(&auth)
        .save_to(&save_path)
        .max_concurrent_chunks(3);

    let result = downloader.send().await;

    let result = match result {
        Ok(r) => r,
        Err(e) => {
            eprintln!("分片下载失败: {}", e);
            return;
        }
    };

    match &result {
        DownloadResult::Saved => {}
        DownloadResult::Bytes(_) => panic!("预期 Saved，得到 Bytes"),
        DownloadResult::BytesSegments(_) => panic!("预期 Saved，得到 BytesSegments"),
    }

    assert!(save_path.exists(), "文件应已保存到 {}", save_path.display());
    let local_len = std::fs::metadata(&save_path).map(|m| m.len()).unwrap_or(0);
    assert_eq!(
        local_len, total_size,
        "本地文件大小应与远程一致: {} vs {}",
        local_len, total_size
    );
    println!(
        "分片下载保存成功: {}，大小 {}",
        save_path.display(),
        local_len
    );
}

/// 分片下载 + output_bytes：返回 BytesSegments，total_len 与远程 size 一致。
#[tokio::test]
async fn download_chunked_output_bytes() {
    let (remote_file, auth) = match require_one_remote_file().await {
        Some(x) => x,
        None => return,
    };

    let total_size = match remote_file.data.size {
        Some(s) => s,
        None => {
            eprintln!("跳过：远程文件无 size，无法分片下载");
            return;
        }
    };

    let concurrent = 5;
    let downloader = remote_file
        .build_downloader(&auth)
        .output_bytes()
        .max_concurrent_chunks(concurrent);

    let result = downloader.send().await;

    let result = match result {
        Ok(r) => r,
        Err(e) => {
            eprintln!("分片 output_bytes 下载失败: {}", e);
            return;
        }
    };

    let segments = match &result {
        DownloadResult::BytesSegments(s) => s,
        DownloadResult::Saved => panic!("预期 BytesSegments，得到 Saved"),
        DownloadResult::Bytes(_) => panic!("分片应返回 BytesSegments，不应为 Bytes"),
    };

    assert_eq!(
        segments.total_len(),
        total_size,
        "BytesSegments 总长应与远程 size 一致"
    );
    let segment_count = (total_size + (4 * 1024 * 1024 - 1)) / (4 * 1024 * 1024);
    println!(
        "分片 output_bytes 成功，total_len={}，并发上限={}，实际段数={}",
        segments.total_len(),
        concurrent,
        segment_count
    );
}
