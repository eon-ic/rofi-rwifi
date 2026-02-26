// src/config.rs — 配置加载，支持文件覆盖

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// rofi 字体
    pub font: String,
    /// rofi 窗口位置 (0–8, 同 rofi -location)
    pub position: u8,
    pub x_offset: i32,
    pub y_offset: i32,
    /// 菜单最大显示行数
    pub max_lines: usize,
    /// nmcli 连接超时（秒）
    pub connect_timeout: u64,
    /// 密码错误最大重试次数
    pub max_retry: u8,
    /// 缓存有效期（秒）
    pub cache_ttl: u64,
    /// Ping 连通性检测目标
    pub ping_host: String,
    /// Ping 次数
    pub ping_count: u8,
    /// VPN 联动: [("VPN profile 名", "触发 SSID"), ...]
    pub auto_vpn: Vec<(String, String)>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            font: "DejaVu Sans Mono 8".into(),
            position: 0,
            x_offset: 0,
            y_offset: 0,
            max_lines: 8,
            connect_timeout: 15,
            max_retry: 3,
            cache_ttl: 30,
            ping_host: "1.1.1.1".into(),
            ping_count: 2,
            auto_vpn: vec![],
        }
    }
}

impl Config {
    /// 按优先级查找并加载配置文件
    pub fn load() -> Result<Self> {
        let candidates = config_candidates();
        for path in &candidates {
            if path.exists() {
                let text = std::fs::read_to_string(path)?;
                let cfg: Config = toml::from_str(&text)?;
                return Ok(cfg);
            }
        }
        Ok(Config::default())
    }

    /// 返回运行时缓存文件路径
    pub fn cache_path() -> PathBuf {
        runtime_dir().join("rofi-wifi-cache.json")
    }

    /// 返回守护进程 PID 文件路径
    pub fn pid_path() -> PathBuf {
        runtime_dir().join("rofi-wifi-daemon.pid")
    }

    /// 返回扫描互斥锁文件路径（防止守护进程与手动刷新并发扫描）
    pub fn lock_path() -> PathBuf {
        runtime_dir().join("rofi-wifi-scan.lock")
    }
}

fn runtime_dir() -> PathBuf {
    std::env::var("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
}

fn config_candidates() -> Vec<PathBuf> {
    let mut v = vec![];
    // 同目录下的 config.toml
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            v.push(dir.join("config.toml"));
        }
    }
    // ~/.config/rofi/wifi.toml
    if let Some(home) = dirs::home_dir() {
        v.push(home.join(".config/rofi/wifi.toml"));
    }
    v
}
