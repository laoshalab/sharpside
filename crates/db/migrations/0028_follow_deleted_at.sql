-- 0028_follow_deleted_at.sql
-- 区分「暂停」与「删除」：
--   暂停 = active = false, deleted_at IS NULL  （可见，可恢复，不再匹配信号）
--   删除 = deleted_at IS NOT NULL            （不可见，归档）
--
-- 背景：此前 delete_follow 与 update_follow(active=false) 都只置 active=false，
-- 且 list_follows_by_user 只返回 active=true，导致暂停后关系从列表消失、与删除不可区分。
--
-- 历史数据：旧路径下 active=false 的行既可能是暂停也可能是删除，已无法区分；
-- 统一标记为已删除（deleted_at=now），保持其不可见，与旧 list 行为一致，避免旧删除项复活。

ALTER TABLE account.follow_relation
    ADD COLUMN IF NOT EXISTS deleted_at timestamptz;

UPDATE account.follow_relation
SET deleted_at = now()
WHERE active = false AND deleted_at IS NULL;

-- list 路径将改为 WHERE deleted_at IS NULL（active + 暂停均可见）。
CREATE INDEX IF NOT EXISTS idx_follow_user_not_deleted
    ON account.follow_relation (user_id)
    WHERE deleted_at IS NULL;
