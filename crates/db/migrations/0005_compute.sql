-- 0005: 计算层 — position_timeline / trader_performance / trader_equity_curve / trader_tag
-- 对应 docs/VENUEHUB_STORAGE.md §6 与 docs/PERFORMANCE_PIPELINE.md §3-§5
-- 所有计算表带 platform 列，per (platform, address) 维度

-- position_timeline: 由 raw_trades 重建的仓位时间线（每 (platform, address, token_id) 一行）
CREATE TABLE trader_hub.position_timeline (
    platform           text        NOT NULL,
    address            text        NOT NULL,
    token_id           text        NOT NULL,
    condition_id       text,
    opened_at          timestamptz,
    closed_at          timestamptz,
    total_bought_size  numeric     NOT NULL DEFAULT 0,
    total_sold_size    numeric     NOT NULL DEFAULT 0,
    avg_cost           numeric     NOT NULL DEFAULT 0,
    realized_pnl       numeric     NOT NULL DEFAULT 0,
    final_open_size    numeric     NOT NULL DEFAULT 0,
    is_closed          boolean     NOT NULL DEFAULT false,
    holding_seconds    bigint,                            -- median 用于 DW:diamond
    computed_at        timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (platform, address, token_id)
);
CREATE INDEX idx_timeline_open   ON trader_hub.position_timeline (platform, address, is_closed);
CREATE INDEX idx_timeline_when   ON trader_hub.position_timeline (platform, address, opened_at);

-- trader_performance: 按周期物化的绩效（覆盖写 7d/30d/all 三行，per (platform, address)）
CREATE TABLE trader_hub.trader_performance (
    platform         text        NOT NULL,
    address          text        NOT NULL,
    period           text        NOT NULL,              -- 7d / 30d / all
    roi              numeric     NOT NULL DEFAULT 0,
    sharpe           numeric     NOT NULL DEFAULT 0,
    sortino          numeric     NOT NULL DEFAULT 0,
    win_rate         numeric     NOT NULL DEFAULT 0,
    max_drawdown     numeric     NOT NULL DEFAULT 0,
    realized_pnl     numeric     NOT NULL DEFAULT 0,
    unrealized_pnl   numeric     NOT NULL DEFAULT 0,
    gross_profit    numeric     NOT NULL DEFAULT 0,
    gross_loss       numeric     NOT NULL DEFAULT 0,
    profit_factor    numeric     NOT NULL DEFAULT 0,
    wins             integer     NOT NULL DEFAULT 0,
    losses           integer     NOT NULL DEFAULT 0,
    position_count   integer     NOT NULL DEFAULT 0,
    open_positions   integer     NOT NULL DEFAULT 0,
    total_volume     numeric     NOT NULL DEFAULT 0,
    cost_basis       numeric     NOT NULL DEFAULT 0,
    computed_at      timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (platform, address, period),
    CONSTRAINT perf_period CHECK (period IN ('7d', '30d', 'all'))
);
CREATE INDEX idx_perf_period ON trader_hub.trader_performance (period);
CREATE INDEX idx_perf_platform_period ON trader_hub.trader_performance (platform, period);

-- trader_equity_curve: 每日 mark-to-market 权益曲线
CREATE TABLE trader_hub.trader_equity_curve (
    platform        text        NOT NULL,
    address         text        NOT NULL,
    date            date        NOT NULL,
    equity          numeric     NOT NULL,
    daily_pnl       numeric     NOT NULL DEFAULT 0,
    drawdown_pct    numeric     NOT NULL DEFAULT 0,
    PRIMARY KEY (platform, address, date)
);
CREATE INDEX idx_equity_trader ON trader_hub.trader_equity_curve (platform, address, date);

-- trader_tag: DW / type-3 标签（per (platform, address)）
CREATE TABLE trader_hub.trader_tag (
    platform    text        NOT NULL,
    address     text        NOT NULL,
    tags        text[]      NOT NULL DEFAULT '{}',      -- DW:diamond, DW:win, type-3:limit_sniper...
    tag_attrs   jsonb       NOT NULL DEFAULT '{}'::jsonb,
    tagged_at   timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (platform, address)
);
