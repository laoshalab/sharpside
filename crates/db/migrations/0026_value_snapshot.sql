-- 0026: trader_value_snapshot — 周期性快照 Polymarket `/value` 组合估值，用于非榜地址的官方盈亏 delta。
-- 对应 docs/PERFORMANCE_PIPELINE.md / docs/SHADOW_MODE.md。
--
-- 背景：Polymarket `/value?user={addr}` 只返回**当前**持仓总估值（无历史时间序列），
-- 官方 per-period 盈亏仅 `/v1/leaderboard` 提供（Top N）。对非榜地址，sharpside 自行
-- 周期快照 `/value`，积累足够历史后按周期算 delta = latest.value - earliest(>=cutoff).value，
-- 作为官方口径的近似（含出入金，前端副标明示），写入 trader_performance.official_pnl
-- （source='polymarket_value_delta'）。
--
-- append-only：每 (platform, address, ts) 一行，靠主键去重。worker 每 tick 对一批候选地址
-- 拉一次 /value 并插入。delta 计算读「窗口内最早」与「最新」两点的差。

CREATE TABLE trader_hub.trader_value_snapshot (
    platform text        NOT NULL,
    address  text        NOT NULL,
    ts       timestamptz NOT NULL,
    value    numeric     NOT NULL,
    PRIMARY KEY (platform, address, ts)
);
CREATE INDEX idx_value_snapshot_trader
    ON trader_hub.trader_value_snapshot (platform, address, ts);
