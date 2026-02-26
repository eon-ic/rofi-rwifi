// src/rofi.rs — 所有 rofi 调用封装

use crate::config::Config;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

/// 通用 rofi dmenu，返回用户选择的行，Esc 返回 None
pub async fn dmenu(
    items: &[String],
    prompt: &str,
    cfg: &Config,
    extra: &[&str], // 额外参数，如 -mesg、-a、-password
) -> Option<String> {
    let input = items.join("\n");
    let mut args = vec![
        "-dmenu".to_string(),
        "-p".to_string(),
        prompt.to_string(),
        "-font".to_string(),
        cfg.font.clone(),
        "-location".to_string(),
        cfg.position.to_string(),
        "-yoffset".to_string(),
        cfg.y_offset.to_string(),
        "-xoffset".to_string(),
        cfg.x_offset.to_string(),
    ];
    for e in extra {
        args.push(e.to_string());
    }

    let mut child = Command::new("rofi")
        .args(&args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .ok()?;

    // 异步写入候选项，写完后必须 drop/关闭 stdin
    // 否则 rofi 会一直等待更多输入而不显示界面
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(input.as_bytes()).await;
        // write_all 完成后 stdin 在此 drop，触发 EOF，rofi 才会渲染列表
    }

    let out = child.wait_with_output().await.ok()?;
    if out.status.success() {
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    } else {
        None // 用户按了 Esc
    }
}

/// 单行密码输入（显示为圆点）
pub async fn password_prompt(hint: &str, cfg: &Config) -> Option<String> {
    let prompt = format!(
        "🔒 密码{}: ",
        if hint.is_empty() {
            String::new()
        } else {
            format!(" ({hint})")
        }
    );
    dmenu(&[], &prompt, cfg, &["-password", "-lines", "0"]).await
}

/// 单行文本输入
pub async fn input_prompt(prompt: &str, cfg: &Config) -> Option<String> {
    dmenu(&[], prompt, cfg, &["-lines", "1"]).await
}

/// 二选一确认（返回 true = 确认）
pub async fn confirm(message: &str, cfg: &Config) -> bool {
    let items = vec!["是".to_string(), "否".to_string()];
    matches!(
        dmenu(&items, message, cfg, &["-lines", "2"])
            .await
            .as_deref(),
        Some("是")
    )
}

/// 在 rofi -mesg 区域显示 UTF-8 二维码
pub async fn show_qr(ssid: &str, qr_text: &str, cfg: &Config) {
    let qr_width = qr_text
        .lines()
        .next()
        .map(|l| l.chars().count())
        .unwrap_or(40);
    let rofi_width = (qr_width + 4).to_string();

    let items = vec!["── 按 Esc 或 Enter 关闭 ──".to_string()];
    let prompt = format!("📷 {ssid}");
    let rofi_width = &format!("-{rofi_width}");
    let extra = vec![
        "-mesg",
        qr_text,
        "-lines",
        "1",
        "-font",
        "Monospace 9",
        "-width",
        rofi_width,
        "-no-custom",
    ];

    // show_qr 不关心返回值
    let _ = dmenu(&items, &prompt, cfg, &extra).await;
}

/// 显示连接详情（只读，不需要选择）
pub async fn show_info(title: &str, content: &str, cfg: &Config) {
    let lines: Vec<String> = content.lines().map(str::to_string).collect();
    let extra = vec!["-no-custom", "-mesg", "按 Esc 关闭"];
    let _ = dmenu(&lines, title, cfg, &extra).await;
}

/// 构建带高亮和宽度的主菜单
pub async fn main_menu(
    items: &[String],
    prompt: &str,
    cfg: &Config,
    highlight: Option<usize>,  // 高亮行（0-indexed）
    warning_msg: Option<&str>, // 顶部警告文字
    max_lines: usize,
) -> Option<String> {
    let width = items.iter().map(|s| s.chars().count()).max().unwrap_or(40) + 4;

    let mut extra: Vec<String> = vec![
        "-lines".into(),
        max_lines.to_string(),
        "-width".into(),
        format!("-{width}"),
    ];
    if let Some(hl) = highlight {
        extra.push("-a".into());
        extra.push(hl.to_string());
    }
    if let Some(msg) = warning_msg {
        extra.push("-mesg".into());
        extra.push(msg.to_string());
    }

    let extra_refs: Vec<&str> = extra.iter().map(String::as_str).collect();
    dmenu(items, prompt, cfg, &extra_refs).await
}
