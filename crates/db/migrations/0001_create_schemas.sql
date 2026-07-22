-- 0001: 创建双 schema（trader_hub 交易者数据 + account 用户/跟随/跟单），物理隔离
-- 对应 docs/VENUEHUB_STORAGE.md §1 与 docs/ARCHITECTURE.md §6

CREATE SCHEMA IF NOT EXISTS trader_hub;
CREATE SCHEMA IF NOT EXISTS account;

-- 启用扩展
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pgcrypto";
