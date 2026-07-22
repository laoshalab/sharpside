-- 移除邮箱认证。网站未上线、无存量用户，邮箱登录/注册功能整体删除。
--
-- 删除 account.users 的 email / password_hash 列（仅邮箱认证使用）。
-- TG 登录（tg_id）与钱包登录（user_wallets 表）不受影响。
-- daemon_api_key_hash 仍保留（独立于密码哈希，daemon 鉴权用）。

ALTER TABLE account.users
    DROP COLUMN IF EXISTS email,
    DROP COLUMN IF EXISTS password_hash;
