-- 0017: user_venue_credentials.kind — 列级凭证类型（公开可读，不进 encrypted_blob 序列化盲区）。
-- 对应 docs/FRONTEND_DESIGN.md §6.5 / §11。
-- blob 内已有 kind（如 deposit_wallet_delegated）；列级便于列表 API 返回、按类型筛选。

ALTER TABLE account.user_venue_credentials
    ADD COLUMN IF NOT EXISTS kind text;

-- 从已有 blob 回填
UPDATE account.user_venue_credentials
SET kind = COALESCE(encrypted_blob->>'kind', 'unknown')
WHERE kind IS NULL;

ALTER TABLE account.user_venue_credentials
    ALTER COLUMN kind SET DEFAULT 'unknown';

ALTER TABLE account.user_venue_credentials
    ALTER COLUMN kind SET NOT NULL;

COMMENT ON COLUMN account.user_venue_credentials.kind IS
    '凭证类型（公开）：deposit_wallet_delegated / wallet / kyc_api_key / api_key 等；与 encrypted_blob.kind 对齐';

CREATE INDEX IF NOT EXISTS idx_credentials_kind
    ON account.user_venue_credentials (platform, kind);
