-- 0036_copy_execution_unique.sql
-- 安全修复 1.4：copy_execution 对 copy_order_id 加 UNIQUE，DB 级防重复成交行。
-- 应用层 CAS 为主（/result 抢占 pending/dispatched → filled），UNIQUE 为底线
-- （daemon 重复上报 / reconcile 与 daemon 跨通道竞争的兜底）。

-- 1. 清理历史重复（若有）：每组 copy_order_id 仅保留最早一条，删其余。
--    若无重复，DELETE 命中 0 行，无副作用。
DELETE FROM account.copy_execution
WHERE id NOT IN (
    SELECT DISTINCT ON (copy_order_id) id
    FROM account.copy_execution
    ORDER BY copy_order_id, executed_at ASC
);

-- 2. 删旧非唯一索引，建唯一索引（替代 idx_exec_order）。
DROP INDEX IF EXISTS account.idx_exec_order;
CREATE UNIQUE INDEX IF NOT EXISTS uq_exec_copy_order ON account.copy_execution (copy_order_id);
