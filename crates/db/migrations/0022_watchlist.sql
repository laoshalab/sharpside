-- 0022: account.watchlist — 用户观察名单（纯收藏，不进执行路径）
-- 对应 Watchlist 功能规划：用户把感兴趣的 trader / identity 收藏观察，
-- 不派生信号、不下单、不受 botfilter / identity manual_verified 门控。
-- 与 account.follow_relation 物理隔离：语义不同，避免污染信号派生查询。
-- 一键升级为 Follow 时在事务内删除本表对应行（消费式升级）。

CREATE TABLE account.watchlist (
    id                  uuid        PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id             uuid        NOT NULL REFERENCES account.users(id) ON DELETE CASCADE,
    -- 监视对象：单 Venue trader 或跨 Venue identity（二选一，与 follow_relation 同构）
    watch_platform      text,                               -- 跟随 trader 时的 platform
    watch_address       text,                               -- 跟随 trader 时的 address
    watch_identity_id   uuid,                               -- 跟随 identity 时的 identity_id
    created_at          timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT watchlist_target CHECK (
        (watch_platform IS NOT NULL AND watch_address IS NOT NULL AND watch_identity_id IS NULL)
        OR (watch_platform IS NULL AND watch_address IS NULL AND watch_identity_id IS NOT NULL)
    )
);

-- 同一用户对同一目标只能收藏一次（部分唯一索引，因二选一约束无法用单列 UNIQUE 表达）
CREATE UNIQUE INDEX uq_watchlist_user_trader
    ON account.watchlist (user_id, watch_platform, watch_address)
    WHERE watch_platform IS NOT NULL;
CREATE UNIQUE INDEX uq_watchlist_user_identity
    ON account.watchlist (user_id, watch_identity_id)
    WHERE watch_identity_id IS NOT NULL;

-- 用户视角列出（按收藏时间倒序）
CREATE INDEX idx_watchlist_user ON account.watchlist (user_id, created_at DESC);
