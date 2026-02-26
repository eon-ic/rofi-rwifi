// src/types.rs — 所有核心数据类型

use serde::{Deserialize, Serialize};

/// 单个 Wi-Fi 接入点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessPoint {
    pub ssid: String,
    pub security: Security,
    pub signal: u8, // 0–100
    pub bars: String,
    pub in_use: bool,
}

impl AccessPoint {
    /// 用于 rofi 显示的单行文本
    pub fn display_line(&self) -> String {
        let lock = match self.security {
            Security::Open => "   ",
            Security::Wep => "🔓 ",
            _ => "🔒 ",
        };
        let active = if self.in_use { "● " } else { "  " };
        format!(
            "{}{}{:<20}  {}  {:>3}%",
            active, lock, self.ssid, self.bars, self.signal
        )
    }
}

/// 加密类型
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Security {
    Open,
    Wep,
    Wpa,
    Wpa2,
    Wpa3,
    Unknown(String),
}

impl Security {
    pub fn needs_password(&self) -> bool {
        !matches!(self, Security::Open)
    }
}

impl std::fmt::Display for Security {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Security::Open => write!(f, "Open"),
            Security::Wep => write!(f, "WEP"),
            Security::Wpa => write!(f, "WPA"),
            Security::Wpa2 => write!(f, "WPA2"),
            Security::Wpa3 => write!(f, "WPA3"),
            Security::Unknown(s) => write!(f, "{s}"),
        }
    }
}

impl From<&str> for Security {
    fn from(s: &str) -> Self {
        let up = s.to_uppercase();
        if up.contains("WPA3") {
            Security::Wpa3
        } else if up.contains("WPA2") {
            Security::Wpa2
        } else if up.contains("WPA") {
            Security::Wpa
        } else if up.contains("WEP") {
            Security::Wep
        } else if up.is_empty() || up == "--" {
            Security::Open
        } else {
            Security::Unknown(s.to_string())
        }
    }
}

/// Wi-Fi 无线电状态
#[derive(Debug, Clone, PartialEq)]
pub enum RadioState {
    Enabled,
    Disabled,
}

/// 连接结果
#[derive(Debug)]
pub enum ConnectResult {
    Success { ip: String },
    WrongPassword,
    Timeout,
    Failed(String),
}

/// 菜单动作
#[derive(Debug, Clone)]
pub enum MenuAction {
    Connect(AccessPoint),
    ToggleRadio,
    Refresh,
    Manual,
    Disconnect,
    Forget,
    Hotspot,
    Details,
    QrCode,
}
