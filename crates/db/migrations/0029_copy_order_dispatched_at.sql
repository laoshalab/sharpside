-- 0029_copy_order_dispatched_at.sql
-- 跟单指令 dispatched 超时回收所需的时间戳。
--
-- 背景：通道 A 在 place_order 前置 status=dispatched 占单锁。若 copier 进程在
-- dispatched 之后、record_fill 之前崩溃，指令会永久卡在 dispatched（无客户端幂等键，
-- 不能安全重试 place_order，否则可能真钱重复下单）。reclaim worker 需要一个
-- 「何时进入 dispatched」的时间戳来判断超时：dispatched_at < now() - timeout 的指令
-- 被原子置为 failed + 原因，交人工核对 Venue 端是否已挂单。
--
-- enqueued_at 不能替代：指令可能在 pending 队列等待较久才被 claim，用 enqueued_at
-- 判超时会误伤刚 dispatched 的正常单。dispatched_at 精确反映占单时刻。

ALTER TABLE account.copy_order
    ADD COLUMN IF NOT EXISTS dispatched_at timestamptz;

-- reclaim worker 扫超时 dispatched 单用：status='dispatched' AND dispatched_at < cutoff。
CREATE INDEX IF NOT EXISTS idx_copy_dispatched_at
    ON account.copy_order (dispatched_at)
    WHERE status = 'dispatched';
