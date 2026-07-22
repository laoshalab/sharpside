-- 0010: account schema — users / follow_relation / copy_order / copy_execution
-- 对应 docs/ARCHITECTURE.md §6.2-§6.4 与 docs/FLOWS.md §4-§7
-- 与 trader_hub schema 物理隔离（用户/跟随/跟单数据）

-- users: 用户主表
CREATE TABLE account.users (
    id              uuid        PRIMARY KEY DEFAULT uuid_generate_v4(),
    tg_id           bigint      UNIQUE,                -- Telegram 用户 ID（web 与 TG 共用身份）
    email           text        UNIQUE,
    password_hash   text,                               -- argon2 hash（email 注册用户）
    jurisdiction    text        NOT NULL DEFAULT 'other', -- us / eu / other，决定可用 execution_venue 集合
    subscription_tier text      NOT NULL DEFAULT 'free', -- free / pro_plus
    subscription_until timestamptz,
    risk_overrides  jsonb       NOT NULL DEFAULT '{}'::jsonb, -- 用户级风控覆盖
    daemon_api_key_hash text,                           -- daemon_api_key 的 hash（绝不存明文）
    daemon_api_key_rotated_at timestamptz,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT user_tier CHECK (subscription_tier IN ('free', 'pro_plus')),
    CONSTRAINT user_jurisdiction CHECK (jurisdiction IN ('us', 'eu', 'other'))
);
CREATE INDEX idx_users_tg ON account.users (tg_id) WHERE tg_id IS NOT NULL;

-- follow_relation: 跟随关系（用户 → Trader 或 Identity）
CREATE TABLE account.follow_relation (
    id              uuid        PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id         uuid        NOT NULL REFERENCES account.users(id) ON DELETE CASCADE,
    -- 跟随对象：单 Venue trader 或跨 Venue identity（二选一）
    follow_platform text,                               -- 跟随 trader 时的 platform
    follow_address  text,                               -- 跟随 trader 时的 address
    follow_identity_id uuid,                           -- 跟随 identity 时的 identity_id
    execute_venue   text        NOT NULL,              -- 用户偏好的执行 Venue（受 jurisdiction 约束）
    channel         text        NOT NULL,              -- tg / daemon
    config          jsonb       NOT NULL DEFAULT '{}'::jsonb, -- FollowConfig（sizing/上限/过滤）
    same_venue_only boolean     NOT NULL DEFAULT false,
    active          boolean     NOT NULL DEFAULT true,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT follow_target CHECK (
        (follow_platform IS NOT NULL AND follow_address IS NOT NULL AND follow_identity_id IS NULL)
        OR (follow_platform IS NULL AND follow_address IS NULL AND follow_identity_id IS NOT NULL)
    ),
    CONSTRAINT follow_channel CHECK (channel IN ('tg', 'daemon')),
    CONSTRAINT follow_execute_venue CHECK (execute_venue IN ('polymarket', 'kalshi', 'manifold', 'zeitgeist', 'azuro'))
);
CREATE INDEX idx_follow_user    ON account.follow_relation (user_id) WHERE active;
CREATE INDEX idx_follow_trader  ON account.follow_relation (follow_platform, follow_address) WHERE active;
CREATE INDEX idx_follow_identity ON account.follow_relation (follow_identity_id) WHERE active;

-- copy_order: 跟单指令（Follow 派生 → Copier 消费）
CREATE TABLE account.copy_order (
    id                  uuid        PRIMARY KEY DEFAULT uuid_generate_v4(),
    follow_relation_id  uuid        NOT NULL REFERENCES account.follow_relation(id) ON DELETE CASCADE,
    user_id             uuid        NOT NULL REFERENCES account.users(id) ON DELETE CASCADE,
    source_venue        text        NOT NULL,
    execute_venue       text        NOT NULL,
    source_market_id    text        NOT NULL,
    source_token_id     text        NOT NULL,
    execute_market_id   text,
    execute_token_id    text,
    side                text        NOT NULL,          -- buy / sell
    price               numeric     NOT NULL,
    size                numeric     NOT NULL,
    channel             text        NOT NULL,          -- tg / daemon
    signal_at           timestamptz NOT NULL,
    enqueued_at         timestamptz NOT NULL DEFAULT now(),
    status              text        NOT NULL DEFAULT 'pending', -- pending/dispatched/filled/skipped/failed/cancelled
    skip_reason         text,                               -- skipped 时的原因
    CONSTRAINT copy_status CHECK (status IN ('pending', 'dispatched', 'filled', 'skipped', 'failed', 'cancelled')),
    CONSTRAINT copy_channel CHECK (channel IN ('tg', 'daemon')),
    CONSTRAINT copy_side CHECK (side IN ('buy', 'sell'))
);
CREATE INDEX idx_copy_queue   ON account.copy_order (status, channel, enqueued_at);
CREATE INDEX idx_copy_user    ON account.copy_order (user_id, enqueued_at);
CREATE INDEX idx_copy_since   ON account.copy_order (user_id, channel, status, enqueued_at);

-- copy_execution: 跟单成交记录（Copier 写入）
CREATE TABLE account.copy_execution (
    id              uuid        PRIMARY KEY DEFAULT uuid_generate_v4(),
    copy_order_id   uuid        NOT NULL REFERENCES account.copy_order(id) ON DELETE CASCADE,
    user_id         uuid        NOT NULL REFERENCES account.users(id) ON DELETE CASCADE,
    venue           text        NOT NULL,              -- 实际执行 Venue
    market_id       text        NOT NULL,
    token_id        text        NOT NULL,
    venue_order_id  text,                               -- Venue 返回的订单 ID
    side            text        NOT NULL,
    filled_size      numeric     NOT NULL,
    filled_price    numeric     NOT NULL,
    fee              numeric     NOT NULL DEFAULT 0,
    tx_hash         text,
    executed_at     timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT exec_side CHECK (side IN ('buy', 'sell'))
);
CREATE INDEX idx_exec_order ON account.copy_execution (copy_order_id);
CREATE INDEX idx_exec_user  ON account.copy_execution (user_id, executed_at);
