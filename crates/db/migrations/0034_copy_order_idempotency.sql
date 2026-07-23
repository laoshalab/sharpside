-- 0034_copy_order_idempotency.sql
-- 订单级幂等键（P1 / Phase 2e-H5）：让 reclaim worker 可安全重试 place_order。
--
-- 背景：Polymarket orderID = keccak256(Order struct)，struct 同时含 salt 与 timestamp
-- （见 crates/clob-auth/src/lib.rs Order 定义）。此前 build_v2_input 每次用 now() 生成
-- salt+timestamp，重试必然产生不同 orderID → Venue 端重复下单（真钱损失），故 reclaim worker
-- 只能对卡死 dispatched 单一律置 failed 不敢重下。
--
-- 现在 claim 时一次性生成并持久化：
--   idempotency_salt   —— 按 copy_order.id 确定性派生的 salt（≤2^53，JSON 整数安全）
--   order_timestamp_ms —— 签名用 timestamp（ms），重试复用同一值 → 逐字节相同已签订单 → 相同 orderID → 幂等
--   exec_price / exec_size —— 单位换算后的目标 Venue 价格/股数，让重试自洽（无需重跑映射/换算）
-- place_order 复用这些值；reclaim worker 对超时 dispatched 单重试 place_order 一次，成功置 submitted，
-- 失败（含 Venue 端判重）回退 failed + 人工核对。

ALTER TABLE account.copy_order
    ADD COLUMN IF NOT EXISTS idempotency_salt    bigint,
    ADD COLUMN IF NOT EXISTS order_timestamp_ms bigint,
    ADD COLUMN IF NOT EXISTS exec_price         double precision,
    ADD COLUMN IF NOT EXISTS exec_size           double precision;
