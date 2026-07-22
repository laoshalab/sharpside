-- 0018: traders.trades_backfilled_at — 回填 worker 的幂等标记。
-- 对应 docs/FLOWS.md §1（ingest 只存身份，回填 worker 异步拉 trades 写 raw_trades）。
-- NULL = 从未回填；非空 = 上次回填时间。回填 worker 按 refresh 窗口重拉以增量同步新成交。

ALTER TABLE trader_hub.traders
    ADD COLUMN IF NOT EXISTS trades_backfilled_at timestamptz;

-- 回填 worker 每轮按 (trades_backfilled_at IS NULL OR < cutoff) 取批次，
-- 此索引让「从未回填」的查询走部分索引，避免全表扫。
CREATE INDEX IF NOT EXISTS idx_traders_backfill_pending
    ON trader_hub.traders (platform, updated_at DESC)
    WHERE trades_backfilled_at IS NULL;
