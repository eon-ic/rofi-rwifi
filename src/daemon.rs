// src/daemon.rs — 后台定时刷新缓存的守护进程

use crate::{cache, config::Config, nmcli};
use anyhow::Result;
use std::path::PathBuf;
use std::time::Duration;
use tokio::time;

pub async fn start(cfg: &Config) -> Result<()> {
    let pid_path = Config::pid_path();

    // 检查是否已在运行
    if is_running(&pid_path) {
        println!("守护进程已在运行 (PID: {})", read_pid(&pid_path).unwrap_or(0));
        return Ok(());
    }

    // 写入当前 PID
    let pid = std::process::id();
    std::fs::write(&pid_path, pid.to_string())?;
    println!("守护进程已启动 (PID: {pid})，每 {}s 刷新缓存", cfg.cache_ttl);

    // 注册退出时清理 PID 文件
    let pid_path_clone = pid_path.clone();
    ctrlc::set_handler(move || {
        let _ = std::fs::remove_file(&pid_path_clone);
        std::process::exit(0);
    })
    .ok();

    // 主循环
    let cache_path = Config::cache_path();
    let ttl = cfg.cache_ttl;
    loop {
        // 触发扫描
        nmcli::rescan().await;
        match nmcli::list_access_points().await {
            Ok(aps) => { let _ = cache::write(&cache_path, &aps); }
            Err(e)  => eprintln!("[daemon] 扫描失败: {e}"),
        }
        time::sleep(Duration::from_secs(ttl)).await;
    }
}

pub fn stop() -> Result<()> {
    let pid_path = Config::pid_path();
    if !pid_path.exists() {
        println!("守护进程未运行");
        return Ok(());
    }
    let pid = read_pid(&pid_path).ok_or_else(|| anyhow::anyhow!("无法读取 PID"))?;
    // SIGTERM
    unsafe { libc::kill(pid as i32, libc::SIGTERM); }
    std::fs::remove_file(&pid_path)?;
    println!("守护进程已停止 (PID: {pid})");
    Ok(())
}

fn is_running(pid_path: &PathBuf) -> bool {
    read_pid(pid_path)
        .map(|pid| unsafe { libc::kill(pid as i32, 0) == 0 })
        .unwrap_or(false)
}

fn read_pid(pid_path: &PathBuf) -> Option<u32> {
    std::fs::read_to_string(pid_path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
}
