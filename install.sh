#!/usr/bin/env bash
# install.sh — 编译并安装 rofi-wifi
set -euo pipefail

INSTALL_DIR="${1:-$HOME/.local/bin}"
SYSTEMD_DIR="$HOME/.config/systemd/user"

echo "══════════════════════════════════════"
echo " rofi-wifi 安装脚本"
echo "══════════════════════════════════════"

# ── 依赖检查 ────────────────────────────────────────────────────
for cmd in cargo nmcli rofi; do
  if ! command -v "$cmd" &>/dev/null; then
    echo "[错误] 缺少依赖: $cmd"
    case "$cmd" in
      cargo) echo "  → 安装 Rust: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh" ;;
      nmcli) echo "  → sudo pacman -S networkmanager  或  sudo apt install network-manager" ;;
      rofi)  echo "  → sudo pacman -S rofi  或  sudo apt install rofi" ;;
    esac
    exit 1
  fi
done

echo "[1/3] 编译（release 模式）..."
cargo build --release 2>&1

echo "[2/3] 安装到 $INSTALL_DIR ..."
mkdir -p "$INSTALL_DIR"
cp target/release/rofi-wifi "$INSTALL_DIR/"
chmod +x "$INSTALL_DIR/rofi-wifi"

# ── 配置文件 ────────────────────────────────────────────────────
CONFIG_DIR="$HOME/.config/rofi"
mkdir -p "$CONFIG_DIR"
if [[ ! -f "$CONFIG_DIR/wifi.toml" ]]; then
  cp config.toml.example "$CONFIG_DIR/wifi.toml"
  echo "  已生成默认配置: $CONFIG_DIR/wifi.toml"
fi

# ── Systemd 用户服务（守护进程） ─────────────────────────────────
echo "[3/3] 安装 systemd 用户服务..."
mkdir -p "$SYSTEMD_DIR"
cat > "$SYSTEMD_DIR/rofi-wifi-daemon.service" <<EOF
[Unit]
Description=rofi-wifi 后台扫描守护进程
After=network.target

[Service]
Type=simple
ExecStart=$INSTALL_DIR/rofi-wifi daemon
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
EOF

systemctl --user daemon-reload
systemctl --user enable --now rofi-wifi-daemon.service && \
  echo "  守护进程已启用（开机自启）" || \
  echo "  [跳过] systemd 不可用，可手动运行: rofi-wifi daemon"

echo ""
echo "══════════════════════════════════════"
echo " 安装完成！"
echo ""
echo " 快捷键绑定（在 WM 配置中添加）:"
echo "   $INSTALL_DIR/rofi-wifi"
echo ""
echo " 可选：qrencode（二维码功能）"
echo "   sudo pacman -S qrencode"
echo "   sudo apt install qrencode"
echo "══════════════════════════════════════"
