-- 0004: 映射层 — market_mappings（跨 Venue 同事件合约等价关系）
-- 对应 docs/ARCHITECTURE.md §8.1 与 docs/VENUE_DESIGN.md §6.1
-- 跨 Venue 跟单只读 manual_verified=true AND resolution_verified=true AND status='active' 的映射

CREATE TABLE trader_hub.market_mappings (
    from_platform        text        NOT NULL,
    from_market_id       text        NOT NULL,
    to_platform          text        NOT NULL,
    to_market_id         text        NOT NULL,
    confidence           numeric     NOT NULL,         -- 0–1 启发式匹配置信度
    manual_verified      boolean     NOT NULL DEFAULT false,
    verified_by          text,
    verified_at          timestamptz,
    -- 方向翻转：Polymarket YES 可能对应 Kalshi No 合约，跟反方向会亏光
    direction_flip       boolean     NOT NULL DEFAULT false,
    -- resolution 规则对齐：同标题不同结算规则 = 假映射
    resolution_notes     text,
    resolution_verified  boolean     NOT NULL DEFAULT false,
    -- 流动性/深度门槛：映射对了也可能无法按尺寸成交
    min_notional         numeric,
    -- 失效与撤销：跟单中途市场下架/重映射
    status               text        NOT NULL DEFAULT 'active',  -- active / retired / rejected
    retired_at           timestamptz,
    created_at           timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (from_platform, from_market_id, to_platform, to_market_id),
    CONSTRAINT mapping_status CHECK (status IN ('active', 'retired', 'rejected')),
    CONSTRAINT mapping_no_self CHECK (from_platform <> to_platform OR from_market_id <> to_market_id)
);
CREATE INDEX idx_mappings_from      ON trader_hub.market_mappings (from_platform, from_market_id);
CREATE INDEX idx_mappings_verified  ON trader_hub.market_mappings (to_platform, manual_verified, resolution_verified, status);
