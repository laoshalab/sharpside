-- 0008: 影子层 — trader_performance_third_party + metric_audit
-- 对应 docs/VENUEHUB_STORAGE.md §9 与 docs/SHADOW_MODE.md §5
-- 与生产展示链路物理隔离：第三方指标永不进入用户界面，只写审计表 + 告警

-- trader_performance_third_party: 第三方原始快照
CREATE TABLE trader_hub.trader_performance_third_party (
    platform        text        NOT NULL,
    address         text        NOT NULL,
    source          text        NOT NULL,              -- polyedge / polynode
    period          text        NOT NULL,              -- 1H/1D/7D/30D/ALL
    roi             numeric,
    win_rate        numeric,
    realized_pnl    numeric,
    unrealized_pnl  numeric,
    wins            integer,
    losses          integer,
    markets_count   integer,
    total_volume    numeric,
    fetched_at      timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (platform, address, source, period)
);
CREATE INDEX idx_tp3_source ON trader_hub.trader_performance_third_party (source, period, fetched_at);

-- metric_audit: 对比结果
CREATE TABLE trader_hub.metric_audit (
    id                   bigserial   PRIMARY KEY,
    platform             text        NOT NULL,
    address              text        NOT NULL,
    source               text        NOT NULL,
    period               text        NOT NULL,
    metric_name          text        NOT NULL,          -- roi/win_rate/sharpe/max_drawdown/pnl
    self_value           numeric,
    third_party_value    numeric,
    diff_abs             numeric,
    diff_pct             numeric,
    status               text        NOT NULL,          -- ok / warn / alert
    audited_at           timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT audit_status CHECK (status IN ('ok', 'warn', 'alert'))
);
CREATE INDEX idx_audit_status   ON trader_hub.metric_audit (status, audited_at);
CREATE INDEX idx_audit_trader   ON trader_hub.metric_audit (platform, address, metric_name);
