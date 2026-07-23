-- 0033_copy_order_submission.sql
-- 成交对账（P0）：copy_order 提交到 Venue 后进入 submitted 状态，由 reconcile worker
-- 轮询 Venue 订单状态回写真实成交。需在 copy_order 上持久化 Venue 返回的订单 ID 与提交时刻。
--
-- 背景：此前 place_order 返回 orderID 后立即记 filled_size=order.size（把"提交"当"全部成交"），
-- 限价单实际可能挂单未成交 / 部分成交，导致账实不符。现改为：
--   pending → dispatched(claim) → submitted(place_order Ok) → filled/cancelled(reconcile)
-- submitted 状态的指令由 reconcile worker 调 Venue::order_state 对账，dispatched 仍由
-- reclaim worker 兜底（崩溃前）。

ALTER TABLE account.copy_order
    ADD COLUMN IF NOT EXISTS venue_order_id text,
    ADD COLUMN IF NOT EXISTS submitted_at timestamptz;

-- reconcile worker 扫 submitted 单用：status='submitted' 按提交时间升序。
CREATE INDEX IF NOT EXISTS idx_copy_submitted_at
    ON account.copy_order (submitted_at)
    WHERE status = 'submitted';
