-- 0042_copy_status_submitted.sql
-- 修复 P0：`copy_status` CHECK 遗漏 'submitted'。
--
-- 背景：0033 引入 submitted 状态（place_order 成功后由 mark_copy_order_submitted 写入，
-- 交 reconcile worker 对账），但未同步 ALTER CHECK。0010 原始约束为：
--   status IN ('pending','dispatched','filled','skipped','failed','cancelled')
-- 实盘路径写 'submitted' 触发 CHECK 违例 → exec 捕获错误置 failed，
-- 而订单已提交 CLOB → 账实分裂（CLOB 挂单 / DB failed / reconcile 扫不到）。
--
-- 本迁移补齐约束。对已部署库同样适用：DROP IF EXISTS + ADD。
-- 顺序在 0033 之后，确保 submitted_at 列与索引已存在。

ALTER TABLE account.copy_order DROP CONSTRAINT IF EXISTS copy_status;

ALTER TABLE account.copy_order
    ADD CONSTRAINT copy_status CHECK (
        status IN (
            'pending', 'dispatched', 'submitted',
            'filled', 'skipped', 'failed', 'cancelled'
        )
    );
