# syntax=docker/dockerfile:1.7
#
# Sharpside devcontainer 镜像：在容器内跑 cargo build/test/clippy/fmt/deny，
# 对应"全部在 docker 跑镜像"的 dev/test 层（运行时层由 infra/docker-compose.yml 负责）。
#
# 与根 Dockerfile 的区别：
#   - 根 Dockerfile 是 release 多阶段（cargo-chef → cook → 单 bin），产出生产二进制；
#   - 本镜像只装工具链 + cargo 子命令，源码经 devcontainer workspace mount 进来，
#     cargo 缓存与 target/ 走命名卷（见 devcontainer.json），迭代编译不重头来。
#
# 工具链锚点见 rust-toolchain.toml（1.91.0 + rustfmt + clippy，profile=minimal）。

FROM rust:1.91.0-slim

# 系统依赖：pkg-config（构建期部分 crate 探测）、ca-certificates（rustls 校验上游 TLS）、
# curl/git（devcontainer 常用 + cargo 拉取）、ripgrep/fd-less（agent 检索友好）。
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      pkg-config \
      ca-certificates \
      curl \
      git \
      ripgrep \
      fd-find \
      less \
      bash-completion \
 && rm -rf /var/lib/apt/lists/*

# rustup 组件：rust-toolchain.toml 声明 rustfmt+clippy，容器首次进入会自动安装；
# 这里显式 rustup-add 确保镜像即用（不依赖 mount 后的 rustup 同步时机）。
RUN rustup component add rustfmt clippy

# cargo 子命令：与 .github/workflows/ci.yml 对齐（nextest 跑测试、deny 跑许可证/漏洞审计）。
# --locked 锁定到各自 Cargo.lock，离线/可复现。
RUN cargo install --locked cargo-nextest \
 && cargo install --locked cargo-deny

# devcontainer 默认以 vscode 用户运行（由 devcontainer.json 的 remoteUser 指定）。
# workspace 挂在 /workspaces/sharpside（devcontainer 标准路径）。
WORKDIR /workspaces/sharpside
