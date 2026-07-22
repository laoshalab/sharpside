-- 0011: user_venue_credentials — per-Venue 加密凭证（绝不存明文私钥）。
-- 对应 docs/ARCHITECTURE.md §6.4 与 docs/VENUEHUB_STORAGE.md §8。
-- 按 (user_id, platform) 存：Polymarket session wallet 句柄 / Kalshi KYC+API key / daemon_api_key 由 users 表单独存 hash。

CREATE TABLE account.user_venue_credentials (
    user_id         uuid        NOT NULL REFERENCES account.users(id) ON DELETE CASCADE,
    platform        text        NOT NULL,
    -- 加密的凭证 blob（KMS 主钥加密），结构按 platform 不同：
    --   polymarket: { "kind": "wallet", "encrypted_handle": "..." }
    --   kalshi:     { "kind": "kyc_api_key", "encrypted_api_key": "...", "encrypted_api_secret": "..." }
    --   manifold:   { "kind": "api_key", "encrypted_key": "..." }
    encrypted_blob  jsonb       NOT NULL,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, platform),
    CONSTRAINT credential_platform CHECK (platform IN ('polymarket', 'kalshi', 'manifold', 'zeitgeist', 'azuro'))
);
CREATE INDEX idx_credentials_user ON account.user_venue_credentials (user_id);
