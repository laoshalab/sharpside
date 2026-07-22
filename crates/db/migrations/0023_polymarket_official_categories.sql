-- 0023: 站内分类对齐 Polymarket Data API `/v1/leaderboard` category 枚举。
-- 官方枚举：OVERALL / POLITICS / SPORTS / ESPORTS / CRYPTO / CULTURE /
--           MENTIONS / WEATHER / ECONOMICS / TECH / FINANCE
--
-- 变更：
--   - 补 MENTIONS；ECONOMY → ECONOMICS；确保 WEATHER 存在
--   - 旧非官方分类（GEOPOLITICS/ELECTION/IRAN/ART）归入最近官方分类
--   - 回填 raw_markets / trader_performance 中的旧 category 值

-- 1) 写入/更新官方映射（含别名 official_category → 官方 site_category）。
INSERT INTO trader_hub.category_mapping (platform, official_category, site_category, display_name) VALUES
    ('polymarket', 'OVERALL',     'OVERALL',    '全部'),
    ('polymarket', 'POLITICS',    'POLITICS',   '政治'),
    ('polymarket', 'SPORTS',      'SPORTS',     '体育'),
    ('polymarket', 'ESPORTS',     'ESPORTS',    '电竞'),
    ('polymarket', 'CRYPTO',      'CRYPTO',     '加密'),
    ('polymarket', 'CULTURE',     'CULTURE',    '文化'),
    ('polymarket', 'MENTIONS',    'MENTIONS',   '提及'),
    ('polymarket', 'WEATHER',     'WEATHER',    '天气'),
    ('polymarket', 'ECONOMICS',   'ECONOMICS',  '经济'),
    ('polymarket', 'TECH',        'TECH',       '科技'),
    ('polymarket', 'FINANCE',     'FINANCE',    '金融'),
    -- 别名：旧种子 / 市场 tag → 官方站内分类
    ('polymarket', 'ECONOMY',     'ECONOMICS',  '经济'),
    ('polymarket', 'GEOPOLITICS', 'POLITICS',   '政治'),
    ('polymarket', 'ELECTION',    'POLITICS',   '政治'),
    ('polymarket', 'IRAN',        'POLITICS',   '政治'),
    ('polymarket', 'ART',         'CULTURE',    '文化')
ON CONFLICT (platform, official_category) DO UPDATE SET
    site_category = EXCLUDED.site_category,
    display_name  = EXCLUDED.display_name;

-- 2) raw_markets.category 回填到官方枚举。
UPDATE trader_hub.raw_markets SET category = 'ECONOMICS' WHERE category = 'ECONOMY';
UPDATE trader_hub.raw_markets SET category = 'POLITICS'
  WHERE category IN ('GEOPOLITICS', 'ELECTION', 'IRAN');
UPDATE trader_hub.raw_markets SET category = 'CULTURE' WHERE category = 'ART';

-- 3) trader_performance：先删会与目标行冲突的旧分类行，再改名。
--    （同一 (platform, address, period) 若已有目标 category，保留目标、丢弃旧值。）
DELETE FROM trader_hub.trader_performance p
 USING trader_hub.trader_performance keep
 WHERE p.category = 'ECONOMY'
   AND keep.category = 'ECONOMICS'
   AND p.platform = keep.platform
   AND p.address = keep.address
   AND p.period = keep.period;
UPDATE trader_hub.trader_performance SET category = 'ECONOMICS' WHERE category = 'ECONOMY';

DELETE FROM trader_hub.trader_performance p
 USING trader_hub.trader_performance keep
 WHERE p.category IN ('GEOPOLITICS', 'ELECTION', 'IRAN')
   AND keep.category = 'POLITICS'
   AND p.platform = keep.platform
   AND p.address = keep.address
   AND p.period = keep.period;
UPDATE trader_hub.trader_performance SET category = 'POLITICS'
  WHERE category IN ('GEOPOLITICS', 'ELECTION', 'IRAN');

DELETE FROM trader_hub.trader_performance p
 USING trader_hub.trader_performance keep
 WHERE p.category = 'ART'
   AND keep.category = 'CULTURE'
   AND p.platform = keep.platform
   AND p.address = keep.address
   AND p.period = keep.period;
UPDATE trader_hub.trader_performance SET category = 'CULTURE' WHERE category = 'ART';
