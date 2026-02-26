// src/notify.rs — 桌面通知，降级到 stderr

pub enum Urgency { Low, Normal, Critical }

pub fn send(urgency: Urgency, title: &str, body: &str) {
    let u = match urgency {
        Urgency::Low      => "low",
        Urgency::Normal   => "normal",
        Urgency::Critical => "critical",
    };
    // 优先用 notify-send
    let ok = std::process::Command::new("notify-send")
        .args(["-u", u, &format!("Wi-Fi: {title}"), body])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if !ok {
        eprintln!("[{u}] Wi-Fi: {title}{}", if body.is_empty() { String::new() } else { format!(": {body}") });
    }
}

pub fn low(title: &str, body: &str)      { send(Urgency::Low,      title, body) }
pub fn normal(title: &str, body: &str)   { send(Urgency::Normal,   title, body) }
pub fn critical(title: &str, body: &str) { send(Urgency::Critical, title, body) }
