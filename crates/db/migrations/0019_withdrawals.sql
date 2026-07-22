-- 0019: account.withdrawals — 用户提现审计日志。
-- 对应 docs/CHANNEL_A_SIGNING.md §4.1（平台代签 WALLET batch 转出 deposit wallet 资产）。
--
-- 提现是高敏操作（平台持 owner EOA 私钥可签 WALLET batch 转资产），故全量审计：
-- 每笔提现记录 user_id / venue / asset / amount / to_address / tx_hash / relayer_tx_id / status。
-- 风控（日上限、单笔上下限、目标地址白名单）在 copier 路由层校验，落库仅做事实记录。

CREATE TABLE account.withdrawals (
    id              uuid        PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id         uuid        NOT NULL REFERENCES account.users(id) ON DELETE CASCADE,
    venue           text        NOT NULL,                  -- polymarket
    asset           text        NOT NULL,                  -- pUSD
    -- 人类单位金额（如 7.0）。raw 整数 = amount * 1e6，链上 transfer 入参。
    amount          numeric     NOT NULL,
    -- 提现目标地址（须为用户绑定钱包之一，应用层校验）。小写 0x hex。
    to_address      text        NOT NULL,
    -- 链上交易哈希（relayer gasless 提交后返回；轮询确认前可能为空）。
    tx_hash         text,
    -- relayer transactionID（用于后续对账/轮询）。
    relayer_tx_id   text,
    -- pending = 已提交 relayer 待确认；mined = 链上确认；failed = relayer 拒绝/链上回退/超时。
    status          text        NOT NULL DEFAULT 'pending',
    -- 失败/降级原因（status != mined 时填充）。
    note            text,
    created_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT withdrawal_status CHECK (status IN ('pending', 'mined', 'failed')),
    CONSTRAINT withdrawal_venue CHECK (venue IN ('polymarket', 'kalshi', 'manifold', 'zeitgeist', 'azuro')),
    CONSTRAINT withdrawal_amount_pos CHECK (amount > 0)
);
CREATE INDEX idx_withdrawals_user ON account.withdrawals (user_id, created_at DESC);
CREATE INDEX idx_withdrawals_status_pending ON account.withdrawals (status, created_at)
    WHERE status = 'pending';
