//! Worker 监督。对应 `docs/ARCHITECTURE.md` §6.1 worker 划分。
//!
//! 单进程多 worker，各自独立 tokio task + interval 循环。
//! 失败影响隔离：单个 worker 出错只记日志、等下一周期，不影响其他 worker 与 HTTP API。
//! 后续有网络环境可切到 apalis redis 队列获得持久化与重试，对外 API 不变。

use crate::state::AppState;
use tokio::task::JoinSet;

pub mod backfill;
pub mod hot;
pub mod identity;
pub mod ingest;
pub mod mapping;
pub mod official_pnl;
pub mod perf;
pub mod shadow;
pub mod signal_replay;

/// 启动全部 worker，返回 JoinSet 供 main 持有（任一 worker panic 会被观测）。
pub fn spawn_all(state: AppState) -> JoinSet<()> {
    let mut set = JoinSet::new();

    let s = state.clone();
    set.spawn(async move { ingest::run(s).await });
    let s = state.clone();
    set.spawn(async move { backfill::run(s).await });
    let s = state.clone();
    set.spawn(async move { mapping::run(s).await });
    let s = state.clone();
    set.spawn(async move { identity::run(s).await });
    let s = state.clone();
    set.spawn(async move { perf::run(s).await });
    let s = state.clone();
    set.spawn(async move { official_pnl::run(s).await });
    let s = state.clone();
    set.spawn(async move { hot::run(s).await });
    let s = state.clone();
    set.spawn(async move { signal_replay::run(s).await });
    let s = state.clone();
    set.spawn(async move { shadow::run(s).await });

    set
}
