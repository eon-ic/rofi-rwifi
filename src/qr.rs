// src/qr.rs — 用 qrcode crate 生成 UTF-8 块字符二维码

use crate::types::Security;
use anyhow::Result;
use qrcode::{QrCode, EcLevel};
use qrcode::render::unicode;

/// 生成 Wi-Fi 连接二维码字符串（UTF-8 块字符）
pub fn wifi_qr(ssid: &str, password: &str, security: &Security) -> Result<String> {
    let sec_str = match security {
        Security::Open => "nopass",
        Security::Wep  => "WEP",
        _              => "WPA",
    };

    // 转义 SSID/密码中的特殊字符（; , " \）
    let ssid_esc  = escape_wifi_field(ssid);
    let pass_esc  = escape_wifi_field(password);

    let qr_data = format!("WIFI:T:{sec_str};S:{ssid_esc};P:{pass_esc};;");

    let code = QrCode::with_error_correction_level(qr_data.as_bytes(), EcLevel::M)?;
    let image = code
        .render::<unicode::Dense1x2>()
        .quiet_zone(true)
        .build();

    // 每行加两个前导空格，rofi 显示时稍微居中
    let padded = image.lines()
        .map(|l| format!("  {l}"))
        .collect::<Vec<_>>()
        .join("\n");

    Ok(padded)
}

/// 转义 Wi-Fi QR 格式中的保留字符
fn escape_wifi_field(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for c in s.chars() {
        match c {
            '\\' | ';' | ',' | '"' => { out.push('\\'); out.push(c); }
            _ => out.push(c),
        }
    }
    out
}
