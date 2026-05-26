# ============================================================
# Stage 1: 构建前端静态文件
# ============================================================
FROM node:20-alpine AS frontend-builder
WORKDIR /app

# 先装依赖（利用 Docker 缓存）
COPY package.json package-lock.json ./
RUN npm ci

# 再复制源码
COPY tsconfig.json tsconfig.node.json vite.config.ts tailwind.config.js postcss.config.js index.html ./
COPY src/ ./src/

RUN npm run build

# ============================================================
# Stage 2: 构建后端二进制
# ============================================================
FROM rust:1.80-bookworm AS backend-builder
WORKDIR /app

# 先复制 Cargo.toml 构建依赖（利用 Docker 缓存）
COPY backend/Cargo.toml backend/Cargo.lock* ./backend/
RUN mkdir backend/src && echo "fn main() {}" > backend/src/main.rs
RUN cargo build --release --manifest-path backend/Cargo.toml 2>/dev/null || true

# 再复制完整源码（利用 Docker 缓存）
COPY backend/src/ ./backend/src/
RUN cargo build --release --manifest-path backend/Cargo.toml

# ============================================================
# Stage 3: 最小化 Nginx 运行镜像
# ============================================================
FROM nginx:alpine

RUN apk add --no-cache tini

# 前端静态文件
COPY --from=frontend-builder /app/dist /usr/share/nginx/html

# 后端二进制
COPY --from=backend-builder /app/backend/target/release/jeeves-server /usr/local/bin/

# Nginx 配置
COPY docker/nginx.conf /etc/nginx/conf.d/default.conf

# 启动脚本
COPY docker/entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

EXPOSE 80

ENTRYPOINT ["/sbin/tini", "--"]
CMD ["/entrypoint.sh"]
