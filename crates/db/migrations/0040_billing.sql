-- 0040: account billing — Pro+ USDC（Polygon）订阅账本。
-- 对应 docs/ARCHITECTURE.md §6.4 Pro+ 商业化；权益仍落 users.subscription_*。
--
-- 设计：
-- - billing_invoices：应付单（一用户至多一条 pending）
-- - billing_payments：链上入账（UNIQUE chain_id+tx_hash+log_index 幂等）
-- - 确认后同事务写 users.subscription_tier/until；不碰 Deposit Wallet / pUSD

CREATE TABLE account.billing_invoices (
    id                uuid        PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id           uuid        NOT NULL REFERENCES account.users(id) ON DELETE CASCADE,
    plan              text        NOT NULL DEFAULT 'pro_plus',
    period_days       int         NOT NULL,
    -- 人类单位（如 30.00 USDC）
    amount_usdc       numeric(18, 6) NOT NULL,
    -- 链上整数（USDC 6 位：amount_usdc * 1e6）
    amount_raw        numeric     NOT NULL,
    chain_id          int         NOT NULL DEFAULT 137,
    token_address     text        NOT NULL,              -- 小写 0x；native USDC，≠ pUSD
    treasury_address  text        NOT NULL,              -- 小写 0x 平台收款地址
    status            text        NOT NULL DEFAULT 'pending',
    expires_at        timestamptz NOT NULL,
    paid_at           timestamptz,
    created_at        timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT billing_invoice_plan CHECK (plan IN ('pro_plus')),
    CONSTRAINT billing_invoice_period CHECK (period_days IN (30, 90)),
    CONSTRAINT billing_invoice_status CHECK (status IN ('pending', 'paid', 'expired', 'cancelled')),
    CONSTRAINT billing_invoice_amount_pos CHECK (amount_usdc > 0 AND amount_raw > 0)
);

-- 每用户最多一张未结清发票（金额认领不撞车）
CREATE UNIQUE INDEX uq_billing_invoices_user_pending
    ON account.billing_invoices (user_id)
    WHERE status = 'pending';

CREATE INDEX idx_billing_invoices_user
    ON account.billing_invoices (user_id, created_at DESC);

CREATE INDEX idx_billing_invoices_pending_due
    ON account.billing_invoices (expires_at)
    WHERE status = 'pending';

CREATE TABLE account.billing_payments (
    id              uuid        PRIMARY KEY DEFAULT uuid_generate_v4(),
    invoice_id      uuid        NOT NULL REFERENCES account.billing_invoices(id) ON DELETE CASCADE,
    user_id         uuid        NOT NULL REFERENCES account.users(id) ON DELETE CASCADE,
    chain_id        int         NOT NULL DEFAULT 137,
    tx_hash         text        NOT NULL,                -- 小写 0x + 64 hex
    log_index       int,                                  -- 确认后填充；submitted 时可空
    from_address    text,                                 -- 付款方；确认后填充
    to_address      text,                                 -- 应收 = invoice.treasury
    amount_raw      numeric,                              -- 确认后填充
    block_number    bigint,
    status          text        NOT NULL DEFAULT 'submitted',
    note            text,
    created_at      timestamptz NOT NULL DEFAULT now(),
    confirmed_at    timestamptz,
    CONSTRAINT billing_payment_status CHECK (status IN ('submitted', 'confirmed', 'rejected')),
    CONSTRAINT billing_payment_tx_hash CHECK (tx_hash ~ '^0x[0-9a-f]{64}$')
);

-- 链上 Transfer 幂等（同一 log 只认一次）
CREATE UNIQUE INDEX uq_billing_payments_chain_tx_log
    ON account.billing_payments (chain_id, tx_hash, log_index)
    WHERE log_index IS NOT NULL;

-- 一发票至多一笔 confirmed
CREATE UNIQUE INDEX uq_billing_payments_invoice_confirmed
    ON account.billing_payments (invoice_id)
    WHERE status = 'confirmed';

-- 用户提交 tx 后 worker 优先扫
CREATE INDEX idx_billing_payments_submitted
    ON account.billing_payments (created_at)
    WHERE status = 'submitted';

CREATE INDEX idx_billing_payments_user
    ON account.billing_payments (user_id, created_at DESC);
