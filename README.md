# rofi-rwifi

用 Rust 重写的 rofi Wi-Fi 管理器，比原 bash 脚本更快、更稳、更安全。

## 功能

| 功能 | 说明 |
|------|------|
| **⚡ 瞬开菜单** | 缓存 + 后台扫描，第二次起打开菜单零等待 |
| **🔒 安全密码输入** | rofi `-password` 模式，不落盘不回显 |
| **🔄 密码错误重试** | 区分密码错误 / 超时 / 其他故障，自动清理残留 profile |
| **📊 连接详情** | IP、网关、DNS、信号强度、延迟一览 |
| **📷 二维码分享** | 用 `qrcode` crate 生成 UTF-8 块字符，直接在 rofi 内显示 |
| **📡 热点管理** | 创建 / 开启 / 关闭软 AP |
| **❌ 断开 / 🗑 忘记** | 带二次确认的破坏性操作 |
| **⚠ 开放网络警告** | 连接无加密网络前弹出确认 |
| **🔌 VPN 联动** | 连上指定 SSID 后自动启动 VPN profile |
| **🔁 守护进程** | 后台定时刷新缓存，可用 systemd 管理 |

## 依赖

**必须：**
- `nmcli`（NetworkManager）
- `rofi`

**可选：**
- `notify-send`（桌面通知，无则降级到 stderr）
- `qrencode` 命令行工具（仅需 `qrcode` Rust crate，不依赖外部命令）

## 安装

```bash
git clone <repo>
cd rofi-rwifi
chmod +x install.sh
./install.sh
```

或手动编译：

```bash
cargo build --release
cp target/release/rofi-rwifi ~/.local/bin/
```

## 使用

```bash
rofi-rwifi              # 打开菜单
rofi-rwifi daemon       # 启动后台守护进程
rofi-rwifi daemon-stop  # 停止守护进程
rofi-rwifi scan         # 立即刷新缓存
```

## 配置

配置文件位置（按优先级）：
1. 可执行文件同目录的 `config.toml`
2. `~/.config/rofi/wifi.toml`

参考 `config.toml.example`。

