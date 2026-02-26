// src/cache.rs — 原子写缓存，防止读到写一半的数据

use crate::types::AccessPoint;
use anyhow::Result;
use std::path::Path;
use std::time::{Duration, SystemTime};

#[derive(serde::Serialize, serde::Deserialize)]
struct CacheFile {
    timestamp: u64, // Unix timestamp（秒）
    aps: Vec<AccessPoint>,
}

/// 写缓存（原子操作：先写临时文件再 rename）
pub fn write(path: &Path, aps: &[AccessPoint]) -> Result<()> {
    let tmp = path.with_extension("tmp");
    let ts = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let data = CacheFile {
        timestamp: ts,
        aps: aps.to_vec(),
    };
    let json = serde_json::to_string(&data)?;
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, path)?; // 原子替换，读者不会看到空文件
    Ok(())
}

/// 读缓存，若文件不存在或已过期返回 None
pub fn read(path: &Path, ttl_secs: u64) -> Option<Vec<AccessPoint>> {
    let text = std::fs::read_to_string(path).ok()?;
    let data: CacheFile = serde_json::from_str(&text).ok()?;

    let age = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .saturating_sub(data.timestamp);

    if age < ttl_secs {
        Some(data.aps)
    } else {
        None // 缓存过期
    }
}

/// 强制删除缓存
pub fn invalidate(path: &Path) {
    let _ = std::fs::remove_file(path);
}

/// 返回缓存剩余有效秒数（0 表示已过期或不存在）
pub fn remaining_ttl(path: &Path, ttl_secs: u64) -> Duration {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(_) => return Duration::ZERO,
    };
    let data: CacheFile = match serde_json::from_str(&text) {
        Ok(d) => d,
        Err(_) => return Duration::ZERO,
    };
    let age = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .saturating_sub(data.timestamp);
    Duration::from_secs(ttl_secs.saturating_sub(age))
}
