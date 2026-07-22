-- 0003: 实体层 — traders（复合主键 platform+address）+ identities（跨 Venue 身份聚合）
-- 对应 docs/TRADERS_TABLE.md §1 与 docs/VENUE_DESIGN.md §7.1

-- identities: 跨 Venue 的同一人聚合
CREATE TABLE trader_hub.identities (
    id               uuid        PRIMARY KEY DEFAULT uuid_generate_v4(),
    alias            text,
    confidence       numeric     NOT NULL DEFAULT 0,
    manual_verified  boolean     NOT NULL DEFAULT false,
    verified_by      text,
    verified_at      timestamptz,
    created_at       timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX idx_identities_verified ON trader_hub.identities (manual_verified);

-- traders: 某 Venue 上的交易者主表。一行 = 某 Venue 上的一个交易者。
CREATE TABLE trader_hub.traders (
    platform          text        NOT NULL,
    address           text        NOT NULL,
    identity_id       uuid        REFERENCES trader_hub.identities(id) ON DELETE SET NULL,
    alias             text,
    source            text        NOT NULL,            -- leaderboard / imported / manual
    is_hot            boolean     NOT NULL DEFAULT false,
    visibility        text        NOT NULL DEFAULT 'visible',  -- visible / hidden / featured
    profile_image     text,
    x_username        text,
    verified_badge    boolean,
    user_name         text,
    first_seen        timestamptz NOT NULL DEFAULT now(),
    updated_at        timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (platform, address)
);
-- address 一律小写存储（链上地址 to_lowercase；玩钱/KYC 原值）
CREATE INDEX idx_traders_is_hot     ON trader_hub.traders (platform, is_hot) WHERE is_hot;
CREATE INDEX idx_traders_visibility ON trader_hub.traders (platform, visibility);
CREATE INDEX idx_traders_identity    ON trader_hub.traders (identity_id) WHERE identity_id IS NOT NULL;

-- updated_at 自动维护
CREATE OR REPLACE FUNCTION trader_hub.touch_updated_at()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$;

CREATE TRIGGER trg_traders_touch_updated_at
    BEFORE UPDATE ON trader_hub.traders
    FOR EACH ROW EXECUTE FUNCTION trader_hub.touch_updated_at();
