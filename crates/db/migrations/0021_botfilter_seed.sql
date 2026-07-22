-- 0021: seed botfilter 阈值到 tag_rules（运营后台可调，零代码改动）。
-- params = BotFilterConfig::default() 序列化（crates/botfilter/src/lib.rs）。
-- perf worker 每 tick 读此行（rule_id='botfilter'），反序列化为 BotFilterConfig；
-- 缺行 / enabled=false / 解析失败 → 回退 default()。运营在 admin tag-rules 页编辑 params jsonb 即生效。
-- 对应 docs/BOTFILTER_RULES.md §5（阈值可调可审计）。
INSERT INTO trader_hub.tag_rules (rule_id, params, enabled, updated_by)
VALUES ('botfilter', '{
  "hf_min_trades": 500,
  "hf_min_symmetric": 0.85,
  "wash_min_count": 1,
  "wash_full_count": 5,
  "rt_min_round_trips": 50,
  "rt_max_hold_secs": 60,
  "tos_min_round_trips": 50,
  "tos_min_resolved": 10,
  "tos_max_win_rate": 0.3,
  "sc_max_conditions": 2,
  "sc_min_large_trades": 20,
  "sc_large_notional": 5000.0,
  "hc_min_trades": 2000,
  "hc_min_resolved": 20,
  "hc_max_win_rate": 0.3,
  "bot_threshold": 0.5
}'::jsonb, true, 'system')
ON CONFLICT (rule_id) DO NOTHING;
