// src/main.rs — 主入口 & 菜单逻辑
mod cache;
mod config;
mod daemon;
mod nmcli;
mod notify;
mod qr;
mod rofi;
mod types;

use anyhow::Result;
use clap::{Parser, Subcommand};
use config::Config;
use std::os::unix::io::AsRawFd;
use types::{AccessPoint, ConnectResult, MenuAction, RadioState, Security};

// ════════════════════════════════════════════════════════════════
// CLI 参数
// ════════════════════════════════════════════════════════════════

#[derive(Parser)]
#[command(name = "rofi-wifi", about = "rofi Wi-Fi 管理器", version)]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// 启动后台守护进程（定时刷新缓存）
    Daemon,
    /// 停止守护进程
    DaemonStop,
    /// 立即执行一次扫描并更新缓存
    Scan,
}

// ════════════════════════════════════════════════════════════════
// 导航结果：区分"返回上级"和"退出程序"
// ════════════════════════════════════════════════════════════════

/// 子流程返回此枚举，告诉调用方下一步该做什么
#[derive(Debug)]
enum Nav {
    /// 操作完成或用户取消，回主菜单（使用当前缓存，不重新扫描）
    Back,
    /// 操作完成，回主菜单并强制重新扫描刷新列表
    Refresh,
    /// 彻底退出程序（只有主菜单按 Esc 触发）
    Quit,
}

// ════════════════════════════════════════════════════════════════
// 入口
// ════════════════════════════════════════════════════════════════

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let cfg = Config::load().unwrap_or_default();

    match cli.cmd {
        Some(Cmd::Daemon) => daemon::start(&cfg).await?,
        Some(Cmd::DaemonStop) => daemon::stop()?,
        Some(Cmd::Scan) => {
            do_scan().await;
            println!("扫描完成，缓存已更新");
        }
        // 主菜单循环：Refresh 强制重扫，Back 直接重显，Quit 退出
        None => {
            let mut force = false;
            loop {
                match run_menu(&cfg, force).await? {
                    Nav::Quit => break,
                    Nav::Back => {
                        force = false;
                    }
                    Nav::Refresh => {
                        force = true;
                    }
                }
            }
        }
    }

    Ok(())
}

// ════════════════════════════════════════════════════════════════
// 扫描 & 缓存
// ════════════════════════════════════════════════════════════════

async fn do_scan() {
    let cache_path = Config::cache_path();
    let lock_path = Config::lock_path();

    let lock_file = match std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&lock_path)
    {
        Ok(f) => f,
        Err(e) => {
            eprintln!("无法创建锁文件: {e}");
            return;
        }
    };

    let fd = lock_file.as_raw_fd();
    // LOCK_EX | LOCK_NB：独占锁，非阻塞；拿不到说明已有扫描在跑
    if unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return;
    }

    nmcli::rescan().await;
    match nmcli::list_access_points().await {
        Ok(aps) => {
            let _ = cache::write(&cache_path, &aps);
        }
        Err(e) => eprintln!("扫描失败: {e}"),
    }

    unsafe { libc::flock(fd, libc::LOCK_UN) };
}

/// 获取 AP 列表：缓存有效则秒返回 + 后台刷新，否则前台等待
async fn get_aps(cfg: &Config, force_refresh: bool) -> Vec<AccessPoint> {
    let cache_path = Config::cache_path();

    if force_refresh {
        cache::invalidate(&cache_path);
    }

    if let Some(aps) = cache::read(&cache_path, cfg.cache_ttl) {
        tokio::spawn(async { do_scan().await });
        return aps;
    }

    notify::low("扫描中", "正在搜索附近 Wi-Fi…");
    do_scan().await;
    cache::read(&cache_path, cfg.cache_ttl * 10).unwrap_or_default()
}

// ════════════════════════════════════════════════════════════════
// 主菜单（返回 Nav 而非 ()）
// ════════════════════════════════════════════════════════════════

async fn run_menu(cfg: &Config, force_refresh: bool) -> Result<Nav> {
    let (aps, radio, curr_ssid) = tokio::join!(
        get_aps(cfg, force_refresh),
        nmcli::radio_state(),
        nmcli::current_ssid(),
    );

    let toggle_label = match radio {
        RadioState::Enabled => "⚡ toggle off",
        RadioState::Disabled => "⚡ toggle on",
    };

    let refresh_label = {
        let remaining = cache::remaining_ttl(&Config::cache_path(), cfg.cache_ttl);
        if remaining.is_zero() {
            "🔄 refresh  (缓存已过期)".to_string()
        } else {
            format!("🔄 refresh  (缓存剩余 {}s)", remaining.as_secs())
        }
    };

    let mut menu_items: Vec<String> = vec![
        toggle_label.into(),
        refresh_label,
        "✏️  manual".into(),
        "❌ disconnect".into(),
        "🗑️  forget".into(),
        "📡 hotspot".into(),
    ];

    let has_connection = curr_ssid.is_some();
    let header_count = if has_connection {
        menu_items.push("📊 details".into());
        menu_items.push("📷 qrcode".into());
        8usize
    } else {
        6usize
    };

    let ap_start = menu_items.len();
    for ap in &aps {
        menu_items.push(ap.display_line());
    }

    let highlight = curr_ssid.as_ref().and_then(|ssid| {
        aps.iter()
            .position(|ap| &ap.ssid == ssid)
            .map(|i| ap_start + i)
    });

    let warning = if aps.iter().any(|ap| ap.security == Security::Open) {
        Some("⚠ 列表中含有开放（无加密）网络，请谨慎连接")
    } else {
        None
    };

    let max_lines = if radio == RadioState::Disabled {
        1
    } else {
        (aps.len() + header_count).min(cfg.max_lines)
    };

    let choice = rofi::main_menu(
        &menu_items,
        "📶 Wi-Fi: ",
        cfg,
        highlight,
        warning,
        max_lines,
    )
    .await;

    // 主菜单按 Esc → 退出程序
    let choice = match choice {
        Some(c) => c,
        None => return Ok(Nav::Quit),
    };

    let action = parse_action(&choice, &aps, &curr_ssid);
    handle_action(action, cfg, &curr_ssid, &aps).await
}

fn parse_action(choice: &str, aps: &[AccessPoint], curr_ssid: &Option<String>) -> MenuAction {
    match choice.trim() {
        s if s.starts_with("⚡") => MenuAction::ToggleRadio,
        s if s.starts_with("🔄") => MenuAction::Refresh,
        s if s.starts_with("✏️") => MenuAction::Manual,
        "❌ disconnect" => MenuAction::Disconnect,
        s if s.starts_with("🗑️") => MenuAction::Forget,
        "📡 hotspot" => MenuAction::Hotspot,
        "📊 details" => MenuAction::Details,
        "📷 qrcode" => MenuAction::QrCode,
        _ => {
            if let Some(ap) = aps.iter().find(|ap| choice.contains(&ap.ssid)) {
                MenuAction::Connect(ap.clone())
            } else if let Some(ssid) = curr_ssid {
                if let Some(ap) = aps.iter().find(|ap| &ap.ssid == ssid) {
                    MenuAction::Connect(ap.clone())
                } else {
                    MenuAction::Refresh
                }
            } else {
                MenuAction::Refresh
            }
        }
    }
}

// ════════════════════════════════════════════════════════════════
// 动作处理（所有子流程 Esc → Nav::Back 回主菜单）
// ════════════════════════════════════════════════════════════════

async fn handle_action(
    action: MenuAction,
    cfg: &Config,
    curr_ssid: &Option<String>,
    aps: &[AccessPoint],
) -> Result<Nav> {
    match action {
        // ── Wi-Fi 开关 ──────────────────────────────────────────
        MenuAction::ToggleRadio => {
            let enable = nmcli::radio_state().await == RadioState::Disabled;
            nmcli::set_radio(enable).await?;
            notify::normal("Wi-Fi", if enable { "已开启" } else { "已关闭" });
            if enable {
                // 开启后等 1s 让扫描结果出来，再交由 loop 强制刷新
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
            return Ok(Nav::Refresh);
        }

        // ── 刷新：清缓存后交由 loop 重新扫描 ───────────────────────
        MenuAction::Refresh => {
            return Ok(Nav::Refresh);
        }

        // ── 手动输入 ────────────────────────────────────────────
        MenuAction::Manual => {
            // Esc → 回主菜单
            let input = match rofi::input_prompt("手动连接 (SSID 或 SSID,密码)", cfg).await {
                Some(s) if !s.is_empty() => s,
                _ => return Ok(Nav::Back),
            };
            let (ssid, pass) = if let Some(idx) = input.find(',') {
                let s = input[..idx].trim().to_string();
                let p = input[idx + 1..].trim().to_string();
                (s, if p.is_empty() { None } else { Some(p) })
            } else {
                (input.trim().to_string(), None)
            };
            if ssid.is_empty() {
                notify::critical("错误", "SSID 不能为空");
                return Ok(Nav::Back);
            }
            do_connect_new(&ssid, pass.as_deref(), cfg).await;
        }

        // ── 断开 ────────────────────────────────────────────────
        MenuAction::Disconnect => {
            let ssid = match curr_ssid {
                Some(s) => s.clone(),
                None => {
                    notify::low("提示", "当前没有已连接的 Wi-Fi");
                    return Ok(Nav::Back);
                }
            };
            // 确认框按 Esc → 回主菜单
            if rofi::confirm(&format!("断开 {ssid}？"), cfg).await {
                match nmcli::disconnect(&ssid).await {
                    Ok(_) => notify::normal("已断开", &ssid),
                    Err(e) => notify::critical("断开失败", &e.to_string()),
                }
            }
        }

        // ── 忘记网络 ────────────────────────────────────────────
        MenuAction::Forget => {
            let saved = nmcli::saved_connections().await.unwrap_or_default();
            if saved.is_empty() {
                notify::low("提示", "没有已保存的 Wi-Fi 配置");
                return Ok(Nav::Back);
            }
            // 网络列表按 Esc → 回主菜单
            let name = match rofi::dmenu(&saved, "🗑 忘记哪个网络？", cfg, &["-lines", "6"]).await
            {
                Some(n) => n,
                None => return Ok(Nav::Back),
            };
            // 确认框按 Esc → 回主菜单
            if rofi::confirm(&format!("永久删除「{name}」？"), cfg).await {
                match nmcli::delete_connection(&name).await {
                    Ok(_) => notify::normal("已删除", &format!("{name} 的连接配置")),
                    Err(e) => notify::critical("删除失败", &e.to_string()),
                }
            }
        }

        // ── 热点 ────────────────────────────────────────────────
        MenuAction::Hotspot => {
            // 内部 Esc 均回主菜单
            handle_hotspot(cfg).await;
        }

        // ── 连接详情 ────────────────────────────────────────────
        MenuAction::Details => {
            let ssid = match curr_ssid {
                Some(s) => s.clone(),
                None => {
                    notify::low("提示", "未连接任何 Wi-Fi");
                    return Ok(Nav::Back);
                }
            };
            notify::low("获取中", "正在读取连接信息…");
            match nmcli::get_details(&ssid, &cfg.ping_host).await {
                Ok(d) => {
                    let ping_str = match d.ping_ms {
                        Some(ms) => format!("{:.1} ms", ms),
                        None => "超时".into(),
                    };
                    let content = format!(
                        "SSID     : {}\nIP       : {}\n网关     : {}\nDNS      : {}\n安全     : {}\n信号     : {}%\n延迟     : {}",
                        d.ssid, d.ip, d.gateway, d.dns, d.security, d.signal, ping_str
                    );
                    // 详情页按 Esc → 回主菜单
                    rofi::show_info(&format!("📊 {}", d.ssid), &content, cfg).await;
                }
                Err(e) => notify::critical("获取失败", &e.to_string()),
            }
        }

        // ── 二维码 ──────────────────────────────────────────────
        MenuAction::QrCode => {
            let ssid = match curr_ssid {
                Some(s) => s.clone(),
                None => {
                    notify::low("提示", "未连接任何 Wi-Fi");
                    return Ok(Nav::Back);
                }
            };
            let pass = nmcli::saved_password(&ssid).await.unwrap_or_default();
            let security = aps
                .iter()
                .find(|ap| ap.ssid == ssid)
                .map(|ap| ap.security.clone())
                .unwrap_or(Security::Wpa2);
            match qr::wifi_qr(&ssid, &pass, &security) {
                // 二维码页按 Esc → 回主菜单
                Ok(qr_text) => rofi::show_qr(&ssid, &qr_text, cfg).await,
                Err(e) => notify::critical("生成失败", &e.to_string()),
            }
        }

        // ── 连接具体 AP ─────────────────────────────────────────
        MenuAction::Connect(ap) => {
            if ap.security == Security::Open {
                let msg = format!("⚠ {} 是开放网络，流量不加密，确认连接？", ap.ssid);
                // 警告框按 Esc → 回主菜单
                if !rofi::confirm(&msg, cfg).await {
                    return Ok(Nav::Back);
                }
            }

            let saved = nmcli::saved_connections().await.unwrap_or_default();
            if saved.iter().any(|n| n == &ap.ssid) {
                notify::normal("连接中…", &ap.ssid);
                match nmcli::connect_saved(&ap.ssid, cfg).await {
                    Ok(_) => handle_post_connect(&ap.ssid, cfg).await,
                    Err(e) => notify::critical("连接失败", &e.to_string()),
                }
            } else {
                let pass = if ap.security.needs_password() {
                    // 密码框按 Esc → 回主菜单
                    match rofi::password_prompt("", cfg).await {
                        Some(p) if !p.is_empty() => Some(p),
                        _ => return Ok(Nav::Back),
                    }
                } else {
                    None
                };
                do_connect_new(&ap.ssid, pass.as_deref(), cfg).await;
            }
        }
    }

    Ok(Nav::Back)
}

// ════════════════════════════════════════════════════════════════
// 连接辅助函数
// ════════════════════════════════════════════════════════════════

async fn do_connect_new(ssid: &str, password: Option<&str>, cfg: &Config) {
    let mut pass = password.map(str::to_string);

    for attempt in 1..=cfg.max_retry {
        if attempt > 1 {
            notify::critical(
                "密码错误",
                &format!(
                    "第 {} 次输入有误，请重试 ({attempt}/{})",
                    attempt - 1,
                    cfg.max_retry
                ),
            );
            let hint = format!("第 {attempt} 次");
            // 重试密码框按 Esc → 放弃连接，回主菜单
            match rofi::password_prompt(&hint, cfg).await {
                Some(p) if !p.is_empty() => pass = Some(p),
                _ => {
                    notify::low("已取消", &format!("放弃连接 {ssid}"));
                    return;
                }
            }
        }

        notify::normal("连接中…", &format!("{ssid}（{attempt}/{}）", cfg.max_retry));

        match nmcli::connect_new(ssid, pass.as_deref(), cfg).await {
            ConnectResult::Success { ip } => {
                handle_post_connect_with_ip(ssid, &ip, cfg).await;
                return;
            }
            ConnectResult::WrongPassword => {
                if attempt == cfg.max_retry {
                    notify::critical(
                        "连接失败",
                        &format!("已重试 {} 次，密码始终错误", cfg.max_retry),
                    );
                }
            }
            ConnectResult::Timeout => {
                notify::critical("连接超时", &format!("{ssid} 连接超时，请检查信号强度"));
                return;
            }
            ConnectResult::Failed(msg) => {
                notify::critical("连接失败", &msg);
                return;
            }
        }
    }
}

async fn handle_post_connect(ssid: &str, cfg: &Config) {
    let ip = nmcli::get_ip().await.unwrap_or_else(|| "未知".into());
    handle_post_connect_with_ip(ssid, &ip, cfg).await;
}

async fn handle_post_connect_with_ip(ssid: &str, ip: &str, cfg: &Config) {
    let (ok, ping_ms) = nmcli::ping_check(&cfg.ping_host, cfg.ping_count).await;
    let net_status = if ok {
        ping_ms.map_or("✓ 网络畅通".into(), |ms| {
            format!("✓ 网络畅通 ({:.0}ms)", ms)
        })
    } else {
        "⚠ 已连接但无法访问互联网".into()
    };
    notify::normal("已连接 ✓", &format!("{ssid}\nIP: {ip}\n{net_status}"));
    try_auto_vpn(ssid, cfg).await;
}

async fn try_auto_vpn(ssid: &str, cfg: &Config) {
    for (vpn, trigger) in &cfg.auto_vpn {
        if trigger == ssid {
            notify::low("VPN", &format!("正在启动 {vpn}…"));
            let ok = tokio::process::Command::new("nmcli")
                .args(["connection", "up", vpn])
                .status()
                .await
                .map(|s| s.success())
                .unwrap_or(false);
            if ok {
                notify::normal("VPN 已连接", vpn)
            } else {
                notify::critical("VPN 失败", &format!("无法启动 {vpn}"))
            }
        }
    }
}

// ════════════════════════════════════════════════════════════════
// 热点（内部所有 Esc 均静默返回，由调用方回到主菜单）
// ════════════════════════════════════════════════════════════════

async fn handle_hotspot(cfg: &Config) {
    if let Some(active) = nmcli::hotspot_active().await {
        if rofi::confirm("关闭热点？", cfg).await {
            let _ = tokio::process::Command::new("nmcli")
                .args(["connection", "down", &active])
                .status()
                .await;
            notify::normal("热点已关闭", "");
        }
        return;
    }

    if let Some(profile) = nmcli::hotspot_profile().await {
        let _ = tokio::process::Command::new("nmcli")
            .args(["connection", "up", &profile])
            .status()
            .await;
        notify::normal("热点已开启", &profile);
        return;
    }

    // Esc 输入名称 → 静默返回主菜单
    let hs_ssid = match rofi::input_prompt("📡 热点名称: ", cfg).await {
        Some(s) if !s.is_empty() => s,
        _ => return,
    };
    // Esc 输入密码 → 静默返回主菜单
    let hs_pass = match rofi::password_prompt("热点密码（至少8位）", cfg).await {
        Some(p) if !p.is_empty() => p,
        _ => return,
    };
    if hs_pass.len() < 8 {
        notify::critical("错误", "密码至少需要 8 位");
        return;
    }
    match nmcli::create_hotspot(&hs_ssid, &hs_pass).await {
        Ok(_) => notify::normal("热点已开启", &format!("SSID: {hs_ssid}")),
        Err(e) => notify::critical("热点失败", &e.to_string()),
    }
}
