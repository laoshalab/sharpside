-- 钱包登录（模型 A · 身份钱包）。对应 docs/CHANNEL_A_SIGNING.md 与安全审计钱包登录方案。
--
-- 1) 用户↔钱包地址 1:N（支持多钱包 + 恢复因子）。地址统一小写存储。
--    登录钱包仅作身份凭证，交易 owner EOA 仍由 /me/deposit-wallet/provision 平台托管生成。
-- 2) auth_nonces：SIWE 一次性 nonce，防签名重放。

CREATE TABLE IF NOT EXISTS account.user_wallets (
    user_id     uuid        NOT NULL REFERENCES account.users(id) ON DELETE CASCADE,
    address     text        NOT NULL,          -- 小写 hex（0x...）
    label       text,                          -- 用户自定义别名（可空）
    is_primary  boolean     NOT NULL DEFAULT false,
    linked_at   timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (address),                      -- 一个地址只绑一个用户
    UNIQUE (user_id, label)                      -- label 用户内唯一（NULL 互不冲突）
);
CREATE INDEX IF NOT EXISTS idx_user_wallets_user ON account.user_wallets (user_id);

CREATE TABLE IF NOT EXISTS account.auth_nonces (
    address     text        NOT NULL,
    nonce       text        NOT NULL,
    issued_at   timestamptz NOT NULL DEFAULT now(),
    consumed_at timestamptz,
    PRIMARY KEY (address, nonce)
);
CREATE INDEX IF NOT EXISTS idx_auth_nonces_cleanup
    ON account.auth_nonces (issued_at)
    WHERE consumed_at IS NULL;
