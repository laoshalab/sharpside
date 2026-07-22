-- 0025: 赎回功能 — 已结算市场赢仓位 CTF token → pUSD。
-- 对应 docs/CHANNEL_A_SIGNING.md §4.2（owner 签 WALLET batch 调 CTF.redeemPositions）。
--
-- 两部分：
--   1) raw_markets 加 closed / resolved_at —— 标记市场已结算，自动赎回 worker 据此扫新结算市场。
--   2) account.redemptions —— 赎回审计日志（自动 worker + 手动端点共用）。
--
-- 赎回与提现的区别：
--   - 提现：deposit wallet pUSD 转出到外部地址（高敏，金额风控）。
--   - 赎回：CTF winning token 换回 pUSD，纯收益转入 deposit wallet（无金额风控，但防重复）。
-- 链路共用 owner 签 WALLET batch → relayer gasless 提交。

-- ── raw_markets 加结算状态 ──
ALTER TABLE trader_hub.raw_markets
    ADD COLUMN IF NOT EXISTS closed      boolean     NOT NULL DEFAULT false,
    ADD COLUMN IF NOT EXISTS resolved_at timestamptz;
CREATE INDEX IF NOT EXISTS idx_raw_markets_closed_resolved
    ON trader_hub.raw_markets (platform, resolved_at)
    WHERE closed = true;

-- ── account.redemptions ──
CREATE TABLE account.redemptions (
    id              uuid        PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id         uuid        NOT NULL REFERENCES account.users(id) ON DELETE CASCADE,
    venue           text        NOT NULL,                  -- polymarket
    -- 市场 condition_id（CTF redeemPositions 入参）。
    condition_id    text        NOT NULL,
    -- 赢方 outcome：YES / NO（记录用；calldata 用 [1,2] 同时赎两边）。
    outcome         text        NOT NULL,
    -- 赢方 token 的 ERC-1155 id（链上 balanceOf 校验 + 审计）。
    token_id        text        NOT NULL,
    -- 赎回的 token 数量（人类单位，CTF token 1:1 collateral）。
    amount          numeric     NOT NULL,
    -- 链上交易哈希（relayer gasless 提交后返回；轮询确认前可能为空）。
    tx_hash         text,
    -- relayer transactionID（用于后续对账/轮询）。
    relayer_tx_id   text,
    -- pending = 已提交 relayer 待确认；mined = 链上确认；failed = relayer 拒绝/链上回退/超时。
    status          text        NOT NULL DEFAULT 'pending',
    -- auto = 自动 worker 触发；manual = 用户点【赎回】按钮触发。
    source          text        NOT NULL DEFAULT 'manual',
    -- 失败/降级原因（status != mined 时填充）。
    note            text,
    created_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT redemption_status CHECK (status IN ('pending', 'mined', 'failed')),
    CONSTRAINT redemption_source CHECK (source IN ('auto', 'manual')),
    CONSTRAINT redemption_outcome CHECK (outcome IN ('YES', 'NO')),
    CONSTRAINT redemption_venue CHECK (venue IN ('polymarket', 'kalshi', 'manifold', 'zeitgeist', 'azuro')),
    CONSTRAINT redemption_amount_pos CHECK (amount > 0)
);
-- 防重复：同一用户 + 同一市场 + 同一 outcome 只能有一笔 pending/mined（failed 不计，允许重试）。
-- 链上 balanceOf 赎完即 0 也兜底；此约束防并发/worker 与手动同时发起。
CREATE UNIQUE INDEX uq_redemptions_user_market_outcome
    ON account.redemptions (user_id, condition_id, outcome)
    WHERE status IN ('pending', 'mined');
CREATE INDEX idx_redemptions_user ON account.redemptions (user_id, created_at DESC);
CREATE INDEX idx_redemptions_status_pending ON account.redemptions (status, created_at)
    WHERE status = 'pending';
