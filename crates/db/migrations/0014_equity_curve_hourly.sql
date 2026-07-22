-- 0014: trader_equity_curve 由日频提粒度到小时级。
-- `date date` → `ts timestamptz`，PK 与索引同步。
-- 旧日频行不迁移（worker 下一轮会按小时级重新物化覆盖）。

ALTER TABLE trader_hub.trader_equity_curve DROP CONSTRAINT IF EXISTS trader_equity_curve_pkey;
DROP INDEX IF EXISTS trader_hub.idx_equity_trader;

ALTER TABLE trader_hub.trader_equity_curve
    RENAME COLUMN date TO ts;
ALTER TABLE trader_hub.trader_equity_curve
    ALTER COLUMN ts TYPE timestamptz USING (date_trunc('hour', ts::timestamp) AT TIME ZONE 'UTC');
ALTER TABLE trader_hub.trader_equity_curve
    ALTER COLUMN ts SET NOT NULL;

ALTER TABLE trader_hub.trader_equity_curve
    ADD CONSTRAINT trader_equity_curve_pkey PRIMARY KEY (platform, address, ts);
CREATE INDEX idx_equity_trader ON trader_hub.trader_equity_curve (platform, address, ts);
