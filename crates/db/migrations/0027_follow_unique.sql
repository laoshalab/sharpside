-- 0027_follow_unique.sql
-- 每个 (user_id, target) 仅允许一条 active follow，避免同一信号派生多笔 copy_order 重复下单。
--
-- 步骤：
-- 1. 去重：已存在的 active 重复关系，保留最新一条，其余置 active=false（软删，等价于暂停）。
-- 2. 建部分唯一索引（WHERE active）：允许暂停/删除后重建，但同一时刻同一 (user, target) 仅一条 active。

-- 1a. trader 目标去重
WITH dupes AS (
    SELECT id,
           ROW_NUMBER() OVER (
               PARTITION BY user_id, follow_platform, follow_address
               ORDER BY updated_at DESC, created_at DESC
           ) AS rn
    FROM account.follow_relation
    WHERE active AND follow_platform IS NOT NULL AND follow_address IS NOT NULL
)
UPDATE account.follow_relation fr
SET active = false, updated_at = now()
FROM dupes
WHERE fr.id = dupes.id AND dupes.rn > 1;

-- 1b. identity 目标去重
WITH dupes AS (
    SELECT id,
           ROW_NUMBER() OVER (
               PARTITION BY user_id, follow_identity_id
               ORDER BY updated_at DESC, created_at DESC
           ) AS rn
    FROM account.follow_relation
    WHERE active AND follow_identity_id IS NOT NULL
)
UPDATE account.follow_relation fr
SET active = false, updated_at = now()
FROM dupes
WHERE fr.id = dupes.id AND dupes.rn > 1;

-- 2. 部分唯一索引（仅约束 active 行）
CREATE UNIQUE INDEX IF NOT EXISTS uq_follow_user_trader_active
    ON account.follow_relation (user_id, follow_platform, follow_address)
    WHERE active AND follow_platform IS NOT NULL AND follow_address IS NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS uq_follow_user_identity_active
    ON account.follow_relation (user_id, follow_identity_id)
    WHERE active AND follow_identity_id IS NOT NULL;
