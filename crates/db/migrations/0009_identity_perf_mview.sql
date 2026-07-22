-- 0009: 身份层 — identity_performance 物化视图（聚合某 identity 下所有 trader 的绩效）
-- 对应 docs/VENUE_DESIGN.md §7.3 与 docs/VENUEHUB_STORAGE.md §5
-- 每日定时 REFRESH MATERIALIZED VIEW

CREATE MATERIALIZED VIEW trader_hub.identity_performance AS
SELECT
    i.id            AS identity_id,
    p.period        AS period,
    SUM(p.realized_pnl)  AS realized_pnl,
    SUM(p.cost_basis)    AS cost_basis,
    CASE
        WHEN SUM(p.cost_basis) = 0 THEN 0
        ELSE SUM(p.realized_pnl) / SUM(p.cost_basis)
    END             AS roi,
    -- 胜率/Sharpe/回撤按聚合规则重算（简化：取加权平均；精确重算由 perf worker 写入独立表）
    AVG(p.win_rate)     AS win_rate,
    AVG(p.sharpe)       AS sharpe,
    MAX(p.max_drawdown) AS max_drawdown,
    COUNT(*)            AS trader_count,
    now()               AS computed_at
FROM trader_hub.identities i
JOIN trader_hub.traders t ON t.identity_id = i.id
JOIN trader_hub.trader_performance p
    ON p.platform = t.platform AND p.address = t.address
GROUP BY i.id, p.period;

CREATE UNIQUE INDEX uq_identity_perf
    ON trader_hub.identity_performance (identity_id, period);

-- 刷新函数（每日定时调用）
CREATE OR REPLACE FUNCTION trader_hub.refresh_identity_performance()
RETURNS void LANGUAGE plpgsql AS $$
BEGIN
    REFRESH MATERIALIZED VIEW CONCURRENTLY trader_hub.identity_performance;
END;
$$;
