-- 0039_credential_archives.sql
-- 安全修复：re-provision 前归档旧凭证密文，避免覆盖后旧 Deposit Wallet 资金不可追溯。
--
-- 每次 upsert 覆盖前，把当前行整包写入本表（含 encrypted_blob）。
-- 运维可按 user_id/platform/archived_at 找回旧 owner / DW 地址与密文（须同一 KMS）。

CREATE TABLE IF NOT EXISTS account.credential_archives (
    id                   bigserial PRIMARY KEY,
    user_id              uuid        NOT NULL REFERENCES account.users(id) ON DELETE CASCADE,
    platform             text        NOT NULL,
    kind                 text        NOT NULL DEFAULT 'unknown',
    encrypted_blob       jsonb       NOT NULL,
    proxy_address        text,
    revoked_at           timestamptz,
    revoked_by           uuid,
    original_created_at  timestamptz,
    original_updated_at  timestamptz,
    archived_at          timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_credential_archives_user_platform
    ON account.credential_archives (user_id, platform, archived_at DESC);
