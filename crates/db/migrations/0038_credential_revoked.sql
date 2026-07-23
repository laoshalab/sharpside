-- 0038_credential_revoked.sql
-- 安全修复 2.2：委托凭证撤销（revoke）。
--
-- `POST /me/deposit-wallet/revoke` 置 revoked_at=now()、revoked_by=user_id；不可逆。
-- copier load_credential 读到 revoked_at IS NOT NULL 即拒下单（pull-based 停派发，无需显式通知）。
-- 前端 /me/delegation stepper 据此显示「已撤销」锁。
--
-- 旧行 revoked_at/revoked_by 为 NULL（活跃），迁移后默认活跃，行为不变。
ALTER TABLE account.user_venue_credentials
    ADD COLUMN IF NOT EXISTS revoked_at timestamptz,
    ADD COLUMN IF NOT EXISTS revoked_by uuid REFERENCES account.users(id) ON DELETE SET NULL;

-- 已撤销凭证的快速过滤索引（copier load_credential 热路径）。
CREATE INDEX IF NOT EXISTS idx_credentials_revoked_at
    ON account.user_venue_credentials (revoked_at)
    WHERE revoked_at IS NOT NULL;
