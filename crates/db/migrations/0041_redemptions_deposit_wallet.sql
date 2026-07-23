-- 0041: redemptions 按 deposit_wallet 区分防重。
-- 同一用户可在「当前 DW」与「归档旧 DW」上分别赎回同一市场（仓位在不同地址）。

ALTER TABLE account.redemptions
    ADD COLUMN IF NOT EXISTS deposit_wallet text NOT NULL DEFAULT '';

DROP INDEX IF EXISTS account.uq_redemptions_user_market_outcome;

CREATE UNIQUE INDEX IF NOT EXISTS uq_redemptions_user_market_outcome_dw
    ON account.redemptions (user_id, condition_id, outcome, deposit_wallet)
    WHERE status IN ('pending', 'mined');
