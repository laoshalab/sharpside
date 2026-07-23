-- 0043_signal_ledger_trades_index.sql
-- 第 3 层（方案 A）：raw_trades 充当"交易信号账"，供 diff 对账做覆盖检查。
--
-- trade_watch worker 把每笔新成交写入 raw_trades（已有 uq_raw_trades_tx/uq_raw_trades_id 去重）；
-- diff 对账按 (platform, address, token_id, [from, to]) 窗口对 raw_trades 求带符号 size 之和，
-- 与仓位 Δ 比较得残差；残差非零才补发 diff 信号（trades 漏的 / 非交易仓位变化）。
--
-- 现有 idx_raw_trades_trader (platform, address, ts) 不含 token_id，覆盖查询需逐 token 过滤，
-- 跟随地址多 token 多时退化为扫描。本迁移加 (platform, address, token_id, ts) 复合索引，
-- 让覆盖查询走索引范围扫描。
--
-- signal_id 语义变更（无需改列）：逐笔信号 key 追加 |source_id（成交 ID），diff 信号 key 不变。
-- 两种 key 段数不同，跨源永不碰撞；跨源去重由 diff 覆盖检查保证（不靠 signal_id）。

CREATE INDEX IF NOT EXISTS idx_raw_trades_trader_token
    ON trader_hub.raw_trades (platform, address, token_id, ts);
