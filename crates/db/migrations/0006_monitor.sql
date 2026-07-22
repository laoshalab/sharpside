-- 0006: 监控层 — hot_wallets（热钥清单）+ trader_positions_snapshot（热钥浮仓最新快照）
-- 对应 docs/VENUEHUB_STORAGE.md §7 与 docs/VENUE_DESIGN.md §8

-- hot_wallets: 热钥清单与抓取配置（per Venue）
CREATE TABLE trader_hub.hot_wallets (
    platform          text        NOT NULL,
    address           text        NOT NULL,
    added_by          text        NOT NULL,
    added_at          timestamptz NOT NULL DEFAULT now(),
    priority          integer     NOT NULL DEFAULT 0,
    scan_interval_secs integer    NOT NULL DEFAULT 30,  -- 自适应基准 10–60s
    enabled           boolean     NOT NULL DEFAULT true,
    PRIMARY KEY (platform, address)
);
CREATE INDEX idx_hot_enabled ON trader_hub.hot_wallets (enabled, priority);

-- trader_positions_snapshot: 热钥当前浮仓最新快照（带 platform）
CREATE TABLE trader_hub.trader_positions_snapshot (
    platform        text        NOT NULL,
    address         text        NOT NULL,
    token_id        text        NOT NULL,
    condition_id    text,
    size            numeric     NOT NULL,
    avg_price       numeric     NOT NULL,
    current_price   numeric     NOT NULL,
    pnl             numeric     NOT NULL,
    captured_at     timestamptz NOT NULL,
    PRIMARY KEY (platform, address, token_id, captured_at)
);
CREATE INDEX idx_snapshot_trader ON trader_hub.trader_positions_snapshot (platform, address, captured_at);
-- 90 天后转对象存储（归档由外部任务处理）
