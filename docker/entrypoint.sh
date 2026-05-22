#!/bin/sh
# NovaClaw 容器启动脚本
#
# Nginx（前台）↔ 代理 /api /ws → 后端（后台，127.0.0.1:3000）

set -e

# 配置目录（挂载卷映射到此处）
CONFIG_DIR="${NOVACLAW_CONFIG_DIR:-/data/novaclaw/config}"
DATA_DIR="${NOVACLAW_DATA_DIR:-/data/novaclaw/data}"

mkdir -p "$CONFIG_DIR" "$DATA_DIR"

# 通过环境变量通知后端路径
export XDG_CONFIG_HOME="$CONFIG_DIR"
export XDG_DATA_HOME="$DATA_DIR"

# 后台启动后端，监听 127.0.0.1:3000（仅 Nginx 可访问）
novaclaw-server --host 127.0.0.1 --port 3000 &

# 前台启动 Nginx
exec nginx -g 'daemon off;'
