-- 0012: Deposit Wallet (POLY_1271) 委托签名支持（FrenFlow 式，主路径）。
-- 对应 docs/CHANNEL_A_SIGNING.md §2.2。
-- 新增 proxy_address 列存 deposit wallet 地址（便于按地址索引/对账）。
-- encrypted_blob jsonb 结构支持两种 Polymarket 凭证：
--   { "kind": "deposit_wallet_delegated", "deposit_wallet_address": "...", "owner_address": "...",
--     "encrypted_owner_key": "...", "l2_api_key": "...", "encrypted_l2_secret": "...",
--     "l2_passphrase": "...", "builder_code": "..." }   -- 主路径（新 API 用户）
--   { "kind": "wallet", "encrypted_handle": "..." }     -- 旧 session 句柄（dev/兼容）

ALTER TABLE account.user_venue_credentials
    ADD COLUMN IF NOT EXISTS proxy_address text;

COMMENT ON COLUMN account.user_venue_credentials.proxy_address IS
    'Deposit wallet / 代理钱包地址（DepositWalletDelegated 时 = deposit wallet 地址；便于按地址索引/对账）';

CREATE INDEX IF NOT EXISTS idx_credentials_proxy
    ON account.user_venue_credentials (platform, proxy_address)
    WHERE proxy_address IS NOT NULL;
