-- 0012: 放宽 trader_performance.period 的 CHECK 约束，
-- 由 7d/30d/all 三档改为 1d/1w/1m/1y/ytd/all 六档（对应前端周期 tab）。
-- 旧 7d/30d 行若存在会被 worker 下一轮覆盖为新周期；不在此强行迁移数据。

ALTER TABLE trader_hub.trader_performance DROP CONSTRAINT IF EXISTS perf_period;

ALTER TABLE trader_hub.trader_performance
    ADD CONSTRAINT perf_period CHECK (period IN ('1d', '1w', '1m', '1y', 'ytd', 'all'));
