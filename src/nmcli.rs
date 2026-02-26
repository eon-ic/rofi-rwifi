// src/nmcli.rs — 所有 nmcli 调用封装

use crate::config::Config;
use crate::types::{AccessPoint, ConnectResult, RadioState, Security};
use anyhow::{anyhow, Result};
use std::time::Duration;
use tokio::process::Command;

// ── 查询 ─────────────────────────────────────────────────────

/// 触发一次 Wi-Fi 重新扫描（不等待结果）
pub async fn rescan() {
    let _ = Command::new("nmcli")
        .args(["dev", "wifi", "rescan"])
        .output()
        .await;
}

/// 获取接入点列表，按信号强度降序
pub async fn list_access_points() -> Result<Vec<AccessPoint>> {
    let out = Command::new("nmcli")
        .args([
            "--fields",
            "IN-USE,SSID,SECURITY,SIGNAL,BARS",
            "--terse",
            "device",
            "wifi",
            "list",
        ])
        .output()
        .await?;

    let stdout = String::from_utf8_lossy(&out.stdout);
    let mut aps: Vec<AccessPoint> = stdout
        .lines()
        .filter(|l| !l.starts_with("--"))
        .filter_map(parse_ap_line)
        .collect();

    // 信号强度降序，当前连接的始终置顶
    aps.sort_by(|a, b| b.in_use.cmp(&a.in_use).then(b.signal.cmp(&a.signal)));

    // 去重（同一 SSID 可能出现在多个信道）
    aps.dedup_by(|a, b| a.ssid == b.ssid && !a.in_use);

    Ok(aps)
}

fn parse_ap_line(line: &str) -> Option<AccessPoint> {
    // nmcli -t 用 ':' 分隔，但 SSID 本身可能含 ':'，需谨慎处理
    // 格式: IN-USE:SSID:SECURITY:SIGNAL:BARS
    let parts: Vec<&str> = line.splitn(5, ':').collect();
    if parts.len() < 5 {
        return None;
    }

    let in_use = parts[0].trim() == "*";
    let ssid = parts[1].trim().to_string();
    let security = Security::from(parts[2].trim());
    let signal = parts[3].trim().parse::<u8>().unwrap_or(0);
    let bars = parts[4].trim().to_string();

    if ssid.is_empty() || ssid == "--" {
        return None;
    }

    Some(AccessPoint {
        ssid,
        security,
        signal,
        bars,
        in_use,
    })
}

/// 获取 Wi-Fi 无线电状态
pub async fn radio_state() -> RadioState {
    let out = Command::new("nmcli")
        .args(["-fields", "WIFI", "general"])
        .output()
        .await
        .unwrap();
    let s = String::from_utf8_lossy(&out.stdout);
    if s.contains("enabled") {
        RadioState::Enabled
    } else {
        RadioState::Disabled
    }
}

/// 当前已连接的 SSID（None 表示未连接）
pub async fn current_ssid() -> Option<String> {
    let out = Command::new("nmcli")
        .env("LANGUAGE", "C")
        .args(["-t", "-f", "active,ssid", "dev", "wifi"])
        .output()
        .await
        .ok()?;
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find(|l| l.starts_with("yes:"))
        .map(|l| l[4..].to_string())
}

/// 已保存的所有 Wi-Fi connection 名称
pub async fn saved_connections() -> Result<Vec<String>> {
    let out = Command::new("nmcli")
        .args(["-t", "-f", "NAME,TYPE", "connection", "show"])
        .output()
        .await?;
    let names = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| l.contains("wireless"))
        .filter_map(|l| l.split(':').next().map(str::to_string))
        .collect();
    Ok(names)
}

/// 查询已保存连接的密码（需要 polkit 授权）
pub async fn saved_password(ssid: &str) -> Option<String> {
    let out = Command::new("nmcli")
        .args([
            "-s",
            "-t",
            "-f",
            "802-11-wireless-security.psk",
            "connection",
            "show",
            ssid,
        ])
        .output()
        .await
        .ok()?;
    let s = String::from_utf8_lossy(&out.stdout);
    s.lines()
        .find(|l| l.contains("802-11-wireless-security.psk"))
        .and_then(|l| l.split(':').nth(1))
        .map(str::to_string)
}

// ── 连接管理 ─────────────────────────────────────────────────

/// 唤起已保存的 profile
pub async fn connect_saved(ssid: &str, cfg: &Config) -> Result<()> {
    let status = Command::new("nmcli")
        .args([
            "--wait",
            &cfg.connect_timeout.to_string(),
            "connection",
            "up",
            ssid,
        ])
        .status()
        .await?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("唤起失败"))
    }
}

/// 连接新网络，返回带语义的结果
pub async fn connect_new(ssid: &str, password: Option<&str>, cfg: &Config) -> ConnectResult {
    let mut args = vec![
        "--wait".to_string(),
        cfg.connect_timeout.to_string(),
        "dev".into(),
        "wifi".into(),
        "con".into(),
        ssid.to_string(),
    ];
    if let Some(p) = password {
        args.push("password".into());
        args.push(p.to_string());
    }

    match Command::new("nmcli").args(&args).output().await {
        Err(e) => ConnectResult::Failed(e.to_string()),
        Ok(out) => {
            if out.status.success() {
                let ip = get_ip().await.unwrap_or_else(|| "未知".into());
                ConnectResult::Success { ip }
            } else {
                let stderr = String::from_utf8_lossy(&out.stderr).to_lowercase();
                let stdout = String::from_utf8_lossy(&out.stdout).to_lowercase();
                let combined = format!("{stderr}{stdout}");
                // 清理残留 profile
                let _ = Command::new("nmcli")
                    .args(["connection", "delete", ssid])
                    .output()
                    .await;
                if combined.contains("secrets")
                    || combined.contains("password")
                    || combined.contains("authentication")
                    || combined.contains("802-11-wireless-security")
                {
                    ConnectResult::WrongPassword
                } else if combined.contains("timeout") {
                    ConnectResult::Timeout
                } else {
                    let msg = String::from_utf8_lossy(&out.stderr)
                        .lines()
                        .last()
                        .unwrap_or("未知错误")
                        .to_string();
                    ConnectResult::Failed(msg)
                }
            }
        }
    }
}

/// 断开当前活跃连接
pub async fn disconnect(ssid: &str) -> Result<()> {
    let status = Command::new("nmcli")
        .args(["connection", "down", ssid])
        .status()
        .await?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("断开失败"))
    }
}

/// 删除已保存的 connection profile
pub async fn delete_connection(name: &str) -> Result<()> {
    let status = Command::new("nmcli")
        .args(["connection", "delete", name])
        .status()
        .await?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("删除失败"))
    }
}

// ── 无线电 & 热点 ─────────────────────────────────────────────

pub async fn set_radio(enable: bool) -> Result<()> {
    let arg = if enable { "on" } else { "off" };
    Command::new("nmcli")
        .args(["radio", "wifi", arg])
        .status()
        .await?;
    Ok(())
}

pub async fn hotspot_active() -> Option<String> {
    let out = Command::new("nmcli")
        .args(["-t", "-f", "NAME,DEVICE", "connection", "show", "--active"])
        .output()
        .await
        .ok()?;
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find(|l| {
            let name = l.split(':').next().unwrap_or("").to_lowercase();
            name.contains("hotspot") || name.contains("热点")
        })
        .map(|l| l.split(':').next().unwrap_or("").to_string())
}

pub async fn hotspot_profile() -> Option<String> {
    let out = Command::new("nmcli")
        .args(["-t", "-f", "NAME,TYPE", "connection", "show"])
        .output()
        .await
        .ok()?;
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| l.contains("wireless"))
        .find(|l| {
            let name = l.split(':').next().unwrap_or("").to_lowercase();
            name.contains("hotspot") || name.contains("热点")
        })
        .map(|l| l.split(':').next().unwrap_or("").to_string())
}

pub async fn create_hotspot(ssid: &str, password: &str) -> Result<()> {
    let status = Command::new("nmcli")
        .args([
            "con",
            "add",
            "type",
            "wifi",
            "ifname",
            "*",
            "con-name",
            "Hotspot",
            "autoconnect",
            "no",
            "ssid",
            ssid,
            "802-11-wireless.mode",
            "ap",
            "802-11-wireless-security.key-mgmt",
            "wpa-psk",
            "802-11-wireless-security.psk",
            password,
            "ipv4.method",
            "shared",
        ])
        .status()
        .await?;
    if !status.success() {
        return Err(anyhow!("创建热点失败"));
    }

    Command::new("nmcli")
        .args(["con", "up", "Hotspot"])
        .status()
        .await?;
    Ok(())
}

// ── 网络信息 ─────────────────────────────────────────────────

pub async fn get_ip() -> Option<String> {
    // 稍等一下让 DHCP 完成
    tokio::time::sleep(Duration::from_millis(500)).await;
    let out = Command::new("nmcli")
        .args(["-t", "-f", "IP4.ADDRESS", "dev", "show"])
        .output()
        .await
        .ok()?;
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find(|l| l.contains("IP4.ADDRESS[1]"))
        .and_then(|l| l.split(':').nth(1))
        .map(str::to_string)
}

#[derive(Debug)]
pub struct ConnectionDetails {
    pub ssid: String,
    pub ip: String,
    pub gateway: String,
    pub dns: String,
    pub security: String,
    pub signal: String,
    pub ping_ms: Option<f64>,
}

pub async fn get_details(ssid: &str, ping_host: &str) -> Result<ConnectionDetails> {
    // 并发获取设备信息和 ping
    let (dev_info, ping_ms) = tokio::join!(get_dev_info(), ping_once(ping_host),);
    let (ip, gateway, dns) = dev_info;

    // 信号强度
    let signal_out = Command::new("nmcli")
        .args(["-t", "-f", "IN-USE,SIGNAL", "dev", "wifi"])
        .output()
        .await?;
    let signal = String::from_utf8_lossy(&signal_out.stdout)
        .lines()
        .find(|l| l.starts_with("*:"))
        .and_then(|l| l.split(':').nth(1))
        .unwrap_or("--")
        .to_string();

    // 安全类型
    let sec_out = Command::new("nmcli")
        .args(["-t", "-f", "IN-USE,SECURITY", "dev", "wifi"])
        .output()
        .await?;
    let security = String::from_utf8_lossy(&sec_out.stdout)
        .lines()
        .find(|l| l.starts_with("*:"))
        .and_then(|l| l.split(':').nth(1))
        .unwrap_or("--")
        .to_string();

    Ok(ConnectionDetails {
        ssid: ssid.to_string(),
        ip,
        gateway,
        dns,
        security,
        signal,
        ping_ms,
    })
}

async fn get_dev_info() -> (String, String, String) {
    let out = Command::new("nmcli")
        .args(["-t", "-f", "IP4.ADDRESS,IP4.GATEWAY,IP4.DNS", "dev", "show"])
        .output()
        .await
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);

    let extract = |prefix: &str| -> String {
        text.lines()
            .find(|l| l.contains(prefix))
            .and_then(|l| l.split(':').nth(1))
            .unwrap_or("N/A")
            .to_string()
    };

    let dns: String = text
        .lines()
        .filter(|l| l.contains("IP4.DNS"))
        .filter_map(|l| l.split(':').nth(1))
        .collect::<Vec<_>>()
        .join(", ");

    (
        extract("IP4.ADDRESS[1]"),
        extract("IP4.GATEWAY"),
        if dns.is_empty() { "N/A".into() } else { dns },
    )
}

/// 单次 ping，返回往返时延毫秒数
pub async fn ping_once(host: &str) -> Option<f64> {
    let out = Command::new("ping")
        .args(["-c", "1", "-W", "2", host])
        .output()
        .await
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    // "rtt min/avg/max/mdev = 1.234/1.234/1.234/0.000 ms"
    text.lines()
        .find(|l| l.contains("rtt") || l.contains("round-trip"))
        .and_then(|l| l.split('/').nth(4))
        .and_then(|s| s.parse::<f64>().ok())
}

/// 多次 ping 连通性检测，返回 (成功, 平均ms)
pub async fn ping_check(host: &str, count: u8) -> (bool, Option<f64>) {
    let out = Command::new("ping")
        .args(["-c", &count.to_string(), "-W", "2", host])
        .output()
        .await;
    match out {
        Err(_) => (false, None),
        Ok(o) if !o.status.success() => (false, None),
        Ok(o) => {
            let text = String::from_utf8_lossy(&o.stdout);
            let ms = text
                .lines()
                .find(|l| l.contains("rtt") || l.contains("round-trip"))
                .and_then(|l| l.split('/').nth(4))
                .and_then(|s| s.parse::<f64>().ok());
            (true, ms)
        }
    }
}
