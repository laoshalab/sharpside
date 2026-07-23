-- 0032: 迁移存量 percent_of_balance sizing 为 fixed。
-- SizingMode::PercentOfBalance 枚举 variant 已从 Rust 删除（从未实现，signal.rs 一直 skip），
-- 故存量 config 须改为合法值，否则 FollowConfig 反序列化失败（该跟随被静默跳过）。
-- 统一改为 fixed amount=10（与前端表单默认一致）；这些跟随原本就是 no-op，无历史成交影响。
-- 用户可后续自行在 UI 调整金额。
UPDATE account.follow_relation
SET config = jsonb_set(
        config,
        '{sizing}',
        '{"mode":"fixed","value":{"amount":10}}'::jsonb
    ),
    updated_at = now()
WHERE config->'sizing'->>'mode' = 'percent_of_balance';
