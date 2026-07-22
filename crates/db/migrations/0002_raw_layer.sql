-- 0002: 原始层 — 保留各 Venue API 原貌，便于重算与回溯。所有 raw 表带 platform 列。
-- 对应 docs/VENUEHUB_STORAGE.md §2

-- raw_trades: 各 signal_source Venue 的 trades 端点
CREATE TABLE trader_hub.raw_trades (
    platform        text        NOT NULL,
    address         text        NOT NULL,
    token_id        text        NOT NULL,
    condition_id    text,
    side            text        NOT NULL,              -- BUY/SELL
    price           numeric     NOT NULL,
    size            numeric     NOT NULL,
    ts              timestamptz NOT NULL,
    tx_hash         text,                               -- 链上 Venue 去重键；玩钱 Venue 用 (platform, trade_id)
    trade_id        text,                               -- 玩钱/KYC Venue 的内部成交 ID
    fetched_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT raw_trades_dedup CHECK (tx_hash IS NOT NULL OR trade_id IS NOT NULL)
);
-- 去重：链上用 tx_hash，玩钱用 (platform, trade_id)
CREATE UNIQUE INDEX uq_raw_trades_tx   ON trader_hub.raw_trades (platform, tx_hash) WHERE tx_hash IS NOT NULL;
CREATE UNIQUE INDEX uq_raw_trades_id   ON trader_hub.raw_trades (platform, trade_id) WHERE trade_id IS NOT NULL;
CREATE INDEX idx_raw_trades_trader     ON trader_hub.raw_trades (platform, address, ts);
CREATE INDEX idx_raw_trades_market     ON trader_hub.raw_trades (platform, condition_id, ts);

-- raw_positions: 热钥高频快照
CREATE TABLE trader_hub.raw_positions (
    platform        text        NOT NULL,
    address         text        NOT NULL,
    token_id        text        NOT NULL,
    condition_id    text,
    size            numeric     NOT NULL,
    avg_price       numeric     NOT NULL,
    current_price   numeric     NOT NULL,
    pnl             numeric     NOT NULL,
    captured_at     timestamptz NOT NULL,
    fetched_at      timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX idx_raw_positions_trader ON trader_hub.raw_positions (platform, address, captured_at);

-- raw_closed_positions: 已平仓历史
CREATE TABLE trader_hub.raw_closed_positions (
    platform        text        NOT NULL,
    address         text        NOT NULL,
    token_id        text        NOT NULL,
    condition_id    text,
    realized_pnl    numeric     NOT NULL,
    opened_at       timestamptz,
    closed_at       timestamptz,
    outcome         text,                               -- YES/NO 或 1/0
    fetched_at      timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX idx_raw_closed_trader ON trader_hub.raw_closed_positions (platform, address, closed_at);

-- raw_prices: 各 Venue 价格历史（Polymarket CLOB /prices-history、Kalshi bars、Manifold market-probs）
CREATE TABLE trader_hub.raw_prices (
    platform        text        NOT NULL,
    token_id        text        NOT NULL,
    ts              timestamptz NOT NULL,
    price           numeric     NOT NULL,
    interval        text        NOT NULL DEFAULT '1d',  -- 1h/1d/1w
    fetched_at      timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX uq_raw_prices ON trader_hub.raw_prices (platform, token_id, ts, interval);
CREATE INDEX idx_raw_prices_token ON trader_hub.raw_prices (platform, token_id, ts);

-- raw_markets: 各 Venue 市场元数据（Polymarket Gamma、Kalshi markets、Manifold markets）
CREATE TABLE trader_hub.raw_markets (
    platform        text        NOT NULL,
    venue_market_id text        NOT NULL,
    title           text        NOT NULL,
    slug            text,
    tags            text[]      NOT NULL DEFAULT '{}',
    category        text,
    end_date        timestamptz,
    outcome_yes     numeric,
    outcome_no      numeric,
    raw_json        jsonb,                              -- 保留官方原貌，便于回溯
    fetched_at      timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX uq_raw_markets ON trader_hub.raw_markets (platform, venue_market_id);
CREATE INDEX idx_raw_markets_end  ON trader_hub.raw_markets (end_date);
