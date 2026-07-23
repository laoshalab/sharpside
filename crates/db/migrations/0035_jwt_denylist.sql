-- JWT 吊销名单（denylist）。对应安全修复 1.2：JWT 吊销 + jti + 短 TTL。
--
-- 每次 JWT 校验点查 jti 是否在此表；命中即拒（登出 / 强制下线）。
-- jti 主键即索引，点查 <1ms。多实例 account/copier 共享同一 PG denylist。
--
-- 清理：exp 已过的 jti 无需保留，由运维 cron 定期执行：
--   DELETE FROM account.jwt_denylist WHERE revoked_at < now() - interval '7 days';
CREATE TABLE IF NOT EXISTS account.jwt_denylist (
    jti         text        PRIMARY KEY,
    user_id     uuid        NOT NULL REFERENCES account.users(id) ON DELETE CASCADE,
    revoked_at  timestamptz NOT NULL DEFAULT now()
);
