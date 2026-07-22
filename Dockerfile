# syntax=docker/dockerfile:1.7
#
# Sharpside 统一多阶段 Dockerfile。
# 单文件构建任意服务：`docker build --build-arg SERVICE=sharpside-gateway -t sharpside-gateway .`
#
# 设计要点（对应 docs/TECH_STACK_RUST.md §10）：
#   - cargo-chef 三阶段：planner → cook 依赖缓存 → 增量构建单 bin，避免 9 二进制全量重编
#   - 静态二进制 + rustls，运行时无需 OpenSSL；选 debian:12-slim 以获得 curl（healthcheck）/ca-certificates
#   - 迁移经 include_str! 编译期嵌入，二进制自包含，无需挂载 migrations 目录
#   - web/admin 用相对路径 apps/{web,admin}/static serve，故 WORKDIR=/app 且保留该目录结构
#   - daemon 不在此构建（用户本地客户端，见 docs/CHANNEL_B）
#
# 可选优化：`--build-arg RUNTIME_IMAGE=gcr.io/distroless/cc-debian12` 切到 distroless（无 shell/healthcheck）。

ARG RUST_VERSION=1.91.0
ARG RUNTIME_IMAGE=debian:12-slim

# ───────────────────────── Stage 1: chef 基底 ─────────────────────────
FROM rust:${RUST_VERSION}-slim AS chef
RUN apt-get update && apt-get install -y --no-install-recommends pkg-config && rm -rf /var/lib/apt/lists/*
# cargo-chef：把 Cargo.toml/workspace 依赖图烤成可缓存的 recipe 层
RUN cargo install cargo-chef --locked --version ^0.1
WORKDIR /app

# 构建期代理（predefined ARG，无需在子 stage 重复声明即对所有 RUN 生效）。
# 默认空=直连；受限网络在 .env 设 BUILD_HTTP_PROXY=http://host.docker.internal:7890
# （compose build.extra_hosts 已加 host-gateway 映射）走宿主代理。
ARG HTTP_PROXY=""
ARG HTTPS_PROXY=""
ARG NO_PROXY="localhost,127.0.0.1"

# ───────────────────────── Stage 2: planner ─────────────────────────
# 仅读 manifest，生成 recipe.json（依赖指纹），不编译代码
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ───────────────────────── Stage 3: builder ─────────────────────────
FROM chef AS builder
# 先烤依赖：源码改动不会让此层失效
COPY --from=planner /app/recipe.json recipe.json
# [patch.crates-io] spin = vendor/spin 是 path 依赖，cook 解析 recipe 时须已存在
COPY vendor/ vendor/
RUN cargo chef cook --release --recipe-path recipe.json
# 再拷源码增量构建指定 bin
COPY . .
ARG SERVICE=sharpside-gateway
RUN cargo build --release --bin ${SERVICE}

# ───────────────────────── Stage 4: runtime ─────────────────────────
FROM ${RUNTIME_IMAGE} AS runtime
ARG SERVICE=sharpside-gateway
# curl：compose healthcheck 用；ca-certificates：reqwest/rustls 校验上游 TLS
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl \
 && rm -rf /var/lib/apt/lists/* \
 && groupadd --system --gid 65532 nonroot \
 && useradd --system --uid 65532 --gid nonroot --home-dir /app --shell /usr/sbin/nologin nonroot

WORKDIR /app
# 二进制统一拷到固定路径，ENTRYPOINT 无需按服务变化
COPY --from=builder /app/target/release/${SERVICE} /app/bin/sharpside
# web/admin 静态资源（相对 CWD 路径 apps/{web,admin}/static，见 apps/web/src/main.rs）
COPY apps/web/static  /app/apps/web/static
COPY apps/admin/static /app/apps/admin/static

# 默认监听端口（compose 用 ports 覆盖；此处仅元数据）
ARG PORT=8080
EXPOSE ${PORT}

ENV RUST_LOG=info,sharpside=info \
    RUST_BACKTRACE=1
USER nonroot
ENTRYPOINT ["/app/bin/sharpside"]
