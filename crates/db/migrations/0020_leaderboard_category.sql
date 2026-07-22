-- 0020: 排行榜分类维度 — trader_performance 加 category 列 + 填充 category_mapping。
-- 对应 docs/VENUEHUB_STORAGE.md §6 / docs/PERFORMANCE_PIPELINE.md。
--
-- category 维度：per (platform, address, period, category) 物化绩效。
--   - 'OVERALL' = 全部成交（兼容旧行为，旧数据回填为 'OVERALL'）。
--   - 其余 = 站内分类（由 category_mapping 把 venue 官方 category 归一化而来）。
-- perf worker 每轮按 (period × {OVERALL ∪ 该 trader 成交涉及的分类}) 切片重算并覆盖写。

-- 1) trader_performance 加 category 列，默认 'OVERALL'，NOT NULL。
ALTER TABLE trader_hub.trader_performance
    ADD COLUMN IF NOT EXISTS category text NOT NULL DEFAULT 'OVERALL';

-- 2) 旧数据回填（默认值已覆盖，此处显式兜底）。
UPDATE trader_hub.trader_performance SET category = 'OVERALL' WHERE category IS NULL;

-- 3) 主键升级 (platform, address, period) → (platform, address, period, category)。
ALTER TABLE trader_hub.trader_performance DROP CONSTRAINT trader_performance_pkey;
ALTER TABLE trader_hub.trader_performance
    ADD PRIMARY KEY (platform, address, period, category);

-- 4) 旧索引不含 category，重建为含 category 的覆盖索引。
DROP INDEX IF EXISTS trader_hub.idx_perf_period;
DROP INDEX IF EXISTS trader_hub.idx_perf_platform_period;
CREATE INDEX idx_perf_category ON trader_hub.trader_performance (category);
CREATE INDEX idx_perf_platform_period_category
    ON trader_hub.trader_performance (platform, period, category);

-- 5) category_mapping 种子：Polymarket 官方分类 → 站内分类。
--    站内分类口径与 Polymarket 排行榜 category 参数对齐，便于 ingest 复用。
INSERT INTO trader_hub.category_mapping (platform, official_category, site_category, display_name) VALUES
    ('polymarket', 'OVERALL',    'OVERALL',    '全部'),
    ('polymarket', 'POLITICS',   'POLITICS',   '政治'),
    ('polymarket', 'CRYPTO',     'CRYPTO',     '加密'),
    ('polymarket', 'SPORTS',     'SPORTS',     '体育'),
    ('polymarket', 'ESPORTS',    'ESPORTS',    '电竞'),
    ('polymarket', 'ECONOMY',    'ECONOMY',    '经济'),
    ('polymarket', 'GEOPOLITICS','GEOPOLITICS','地缘政治'),
    ('polymarket', 'TECH',       'TECH',       '科技'),
    ('polymarket', 'CULTURE',    'CULTURE',    '文化'),
    ('polymarket', 'FINANCE',    'FINANCE',    '财务'),
    ('polymarket', 'ELECTION',   'ELECTION',   '选举'),
    ('polymarket', 'IRAN',       'IRAN',       '伊朗'),
    ('polymarket', 'WEATHER',    'WEATHER',    '天气'),
    ('polymarket', 'ART',        'ART',        '艺术')
ON CONFLICT (platform, official_category) DO NOTHING;
