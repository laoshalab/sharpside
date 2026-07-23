-- 0030: signal_outbox — venue-hub hot worker emit 失败的信号落表，由 signal_replay worker 重发。
-- 解决 H4：emit_signals 失败仅 warn 导致仓位变化信号静默丢失。
-- signal_id = signal_id(platform,trader_id,token_id,ts) 去重键，与 follow 侧 copy_order.signal_id 对齐，
-- 保证 outbox 重发同一信号时不会重复入表。
CREATE TABLE account.signal_outbox (
    id               bigserial    PRIMARY KEY,
    signal_id        text         NOT NULL,
    payload          jsonb        NOT NULL,                  -- SignalPayload 全量
    target_url       text         NOT NULL,                  -- follow /internal/signals 完整 URL
    attempts         int          NOT NULL DEFAULT 0,
    max_attempts     int          NOT NULL DEFAULT 5,
    next_attempt_at  timestamptz  NOT NULL DEFAULT now(),
    last_error       text,
    created_at       timestamptz  NOT NULL DEFAULT now(),
    delivered_at     timestamptz,                             -- 成功投递时间
    deadlettered_at  timestamptz,                             -- 超过 max_attempts 后置死信
    CONSTRAINT outbox_attempts CHECK (attempts >= 0)
);
-- 同一信号只入表一次：重发不重复入，热 worker 重复检出同 ts 信号也不重复入。
CREATE UNIQUE INDEX uq_signal_outbox_signal_id ON account.signal_outbox (signal_id);
-- replay worker 扫未投递且未死信且到期的行。
CREATE INDEX idx_signal_outbox_due
    ON account.signal_outbox (next_attempt_at)
    WHERE delivered_at IS NULL AND deadlettered_at IS NULL;
