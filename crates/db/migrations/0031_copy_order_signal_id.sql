-- 0031: copy_order.signal_id — 信号去重键，配合 venue-hub signal_outbox 重发幂等。
-- 同一 signal_id 对同一 follow_relation 仅允许一条 copy_order：
-- outbox 重发同一信号时，follow 侧命中唯一约束即视为已派生，跳过不重复下单。
-- signal_id 可空（历史存量行 + 非 signal 派生的指令），NULL 不参与唯一约束。
ALTER TABLE account.copy_order ADD COLUMN signal_id text;
CREATE UNIQUE INDEX uq_copy_order_signal_follow
    ON account.copy_order (signal_id, follow_relation_id)
    WHERE signal_id IS NOT NULL;
