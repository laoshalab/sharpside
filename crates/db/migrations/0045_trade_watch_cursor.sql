-- 0045: trade_watch 显式游标表。
--
-- 旧逻辑：bootstrap 状态靠 raw_trades 的 MAX(ts) 推断（None=从未轮询）。
-- 问题：raw_trades 被清空、首笔 upsert 持续失败、或人工删数据时，MAX(ts) 回到 None，
-- 每轮重复 bootstrap（只记基线不 emit），该地址信号永久丢失。
--
-- 改：独立 cursor 表记 (platform, address, last_ts, last_trade_id, bootstrapped)。
-- bootstrap 完成后 bootstrapped=true，后续轮询直接用 last_ts/last_trade_id 作游标，
-- 不再依赖 raw_trades 是否有数据。raw_trades 仍作覆盖账（hot 对账用），但游标独立性更强。
-- upsert_raw_trade 成功后同步推进 cursor，保证游标与账一致。

CREATE TABLE IF NOT EXISTS trader_hub.trade_watch_cursor (
    platform        text        NOT NULL,
    address         text        NOT NULL,
    -- 已成功写入 raw_trades 的最新成交（ts, trade_id）作下轮游标。
    last_ts         timestamptz,
    last_trade_id   text,
    -- 是否完成 bootstrap（false=仅记基线阶段，true=可正常增量 emit）。
    bootstrapped    boolean     NOT NULL DEFAULT false,
    updated_at      timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (platform, address)
);
