//! Worker 心跳戳（安全修复 4.3）。
//!
//! 各 worker tick 结束时 [`WorkerTicks::touch`]；`/readyz` 据此判断是否停滞。

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

/// 关键 worker 的上次成功 tick（unix 秒）。0 = 尚未 tick。
#[derive(Debug, Default)]
pub struct WorkerTicks {
    pub ingest: AtomicI64,
    pub hot: AtomicI64,
    pub signal_replay: AtomicI64,
    pub trade_watch: AtomicI64,
}

impl WorkerTicks {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn touch_ingest(&self) {
        self.ingest.store(now_secs(), Ordering::Relaxed);
    }

    pub fn touch_hot(&self) {
        self.hot.store(now_secs(), Ordering::Relaxed);
    }

    pub fn touch_signal_replay(&self) {
        self.signal_replay.store(now_secs(), Ordering::Relaxed);
    }

    pub fn touch_trade_watch(&self) {
        self.trade_watch.store(now_secs(), Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> WorkerTickSnapshot {
        WorkerTickSnapshot {
            ingest_last_tick_at: self.ingest.load(Ordering::Relaxed),
            hot_last_tick_at: self.hot.load(Ordering::Relaxed),
            signal_replay_last_tick_at: self.signal_replay.load(Ordering::Relaxed),
            trade_watch_last_tick_at: self.trade_watch.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct WorkerTickSnapshot {
    pub ingest_last_tick_at: i64,
    pub hot_last_tick_at: i64,
    pub signal_replay_last_tick_at: i64,
    pub trade_watch_last_tick_at: i64,
}

fn now_secs() -> i64 {
    chrono::Utc::now().timestamp()
}
