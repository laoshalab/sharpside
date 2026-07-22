-- 0024: trader_performance 加官方盈亏列。
-- 对应 docs/PERFORMANCE_PIPELINE.md / docs/SHADOW_MODE.md。
--
-- 用途：存 Polymarket 官方排行榜 `/v1/leaderboard?timePeriod=...` 返回的 `pnl`，
-- 与 sharpside 自算 `realized_pnl` 并存展示，让前端「盈亏」可对齐官方口径。
--
-- 列：
--   official_pnl     —— 官方该周期盈亏（USD），NULL = 未抓到/不在榜
--   official_vol     —— 官方该周期成交量（USD），NULL = 未抓到
--   official_source  —— 数据来源（如 'polymarket_leaderboard'），便于审计
--   official_pnl_at  —— 抓取时间，用于新鲜度判断与刷新调度
--
-- 仅写 (platform, address, period, category='OVERALL') 行；非 OVERALL 分类官方不给，保持 NULL。
-- 与 `realized_pnl`（NOT NULL DEFAULT 0）不同，官方列允许 NULL 以表达「无官方数据」，
-- 前端据此回落到自算值。

ALTER TABLE trader_hub.trader_performance
    ADD COLUMN IF NOT EXISTS official_pnl    numeric     NULL,
    ADD COLUMN IF NOT EXISTS official_vol    numeric     NULL,
    ADD COLUMN IF NOT EXISTS official_source text        NULL,
    ADD COLUMN IF NOT EXISTS official_pnl_at timestamptz NULL;
