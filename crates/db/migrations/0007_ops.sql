-- 0007: 运营层 — tag_rules / category_mapping / fetch_state / audit_thresholds / user_venue_credentials
-- 对应 docs/VENUEHUB_STORAGE.md §8

-- tag_rules: 标签阈值，运营后台可调，零代码改动
CREATE TABLE trader_hub.tag_rules (
    rule_id     text        PRIMARY KEY,
    params      jsonb       NOT NULL DEFAULT '{}'::jsonb,
    enabled     boolean     NOT NULL DEFAULT true,
    updated_by  text,
    updated_at  timestamptz NOT NULL DEFAULT now()
);

-- category_mapping: 某 Venue 的官方 category → 站内分类映射（per platform）
CREATE TABLE trader_hub.category_mapping (
    platform           text        NOT NULL,
    official_category  text        NOT NULL,
    site_category      text        NOT NULL,
    display_name       text,
    PRIMARY KEY (platform, official_category)
);

-- fetch_state: 抓取游标与限流状态，per (platform, source, address)
CREATE TABLE trader_hub.fetch_state (
    platform      text        NOT NULL,
    source        text        NOT NULL,                -- leaderboard / positions / trades / markets / prices
    address       text,                               -- 可空（如 markets 抓取不按地址）
    last_ts       timestamptz,
    last_tx_hash  text,
    last_run_at   timestamptz,
    status        text        NOT NULL DEFAULT 'idle', -- idle / running / error
    error_msg     text,
    PRIMARY KEY (platform, source, address)
);

-- audit_thresholds: 影子校验阈值（per metric）
CREATE TABLE trader_hub.audit_thresholds (
    metric_name  text        PRIMARY KEY,
    warn_pct     numeric     NOT NULL DEFAULT 0,
    warn_abs     numeric     NOT NULL DEFAULT 0,
    alert_pct    numeric     NOT NULL DEFAULT 0,
    alert_abs    numeric     NOT NULL DEFAULT 0,
    updated_at   timestamptz NOT NULL DEFAULT now()
);

-- user_venue_credentials: 用户 per-Venue 凭证（加密存储，绝不存明文私钥）
-- 由 account 服务写入，copier 读取
CREATE TABLE trader_hub.user_venue_credentials (
    user_id          uuid        NOT NULL,
    platform         text        NOT NULL,
    credential_kind  text        NOT NULL,              -- wallet / kyc_api_key / api_key
    encrypted_handle text        NOT NULL,              -- KMS 主钥加密的授权句柄/API key/secret
    proxy_address    text,                               -- 关联的链上地址（链上 Venue）
    updated_at       timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, platform),
    CONSTRAINT cred_kind CHECK (credential_kind IN ('wallet', 'kyc_api_key', 'api_key'))
);
CREATE INDEX idx_creds_user     ON trader_hub.user_venue_credentials (user_id);
CREATE INDEX idx_creds_platform ON trader_hub.user_venue_credentials (platform);
