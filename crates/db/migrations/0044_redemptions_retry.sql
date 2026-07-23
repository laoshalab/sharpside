-- 0044: 赎回失败有限重试。
--
-- 旧逻辑：redeem_worker 链上失败即 status=failed 终态，永不重试，只能手动端点兜底。
-- 瞬态失败（relayer 5xx / RPC 超时 / 代理抖动）被误判为永久失败，用户持仓无法自动赎回。
--
-- 改：failed 行加 attempts（已尝试次数）与 next_attempt_at（下次可重试时刻）。
-- worker 扫 status='failed' AND attempts < 3 AND next_attempt_at <= now 的行重试：
--   重试前先查链上 balanceOf，0 则直接标 mined（已赎回，可能是上轮已成功但回报丢失）；
--   否则改回 pending（被唯一约束保护防并发）→ venue.redeem → 成功 mined / 失败 attempts+1。
-- attempts 达 3 仍 failed 则不再自动重试，保留 failed 交人工。

ALTER TABLE account.redemptions
    ADD COLUMN IF NOT EXISTS attempts       int         NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS next_attempt_at timestamptz;

-- 失败重试候选索引：按下次可重试时刻扫，避免全表扫 failed 行。
CREATE INDEX IF NOT EXISTS idx_redemptions_retry
    ON account.redemptions (next_attempt_at)
    WHERE status = 'failed' AND attempts < 3;
