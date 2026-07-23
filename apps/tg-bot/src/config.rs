//! TG bot 环境变量配置。对应 `docs/ARCHITECTURE.md` §6.3 / `docs/FLOWS.md` §6。

use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    /// Telegram bot token（@BotFather 颁发）
    pub tg_token: String,
    /// account 服务基址
    pub account_url: String,
    /// follow 服务基址
    pub follow_url: String,
    /// venue-hub 服务基址
    pub venue_hub_url: String,
    /// 与 account 共享的密钥（`POST /auth/tg` 须带 `X-TG-Bot-Secret`）
    pub tg_bot_secret: String,
    /// 默认固定下单金额（USDC）
    pub default_amount: f64,
}

impl Config {
    pub fn from_env() -> Self {
        // 安全修复 3.5：生产环境 TG_BOT_SECRET 走 assert_secret（空/默认/短密钥 panic）。
        let tg_bot_secret = sharpside_shared::secrets::assert_secret(
            "TG_BOT_SECRET",
            &env::var("TG_BOT_SECRET").unwrap_or_else(|_| "dev-tg-bot-secret".into()),
        )
        .to_string();
        Self {
            tg_token: env::var("TG_BOT_TOKEN").unwrap_or_default(),
            account_url: env::var("ACCOUNT_URL").unwrap_or_else(|_| "http://127.0.0.1:8084".into()),
            follow_url: env::var("FOLLOW_URL").unwrap_or_else(|_| "http://127.0.0.1:8082".into()),
            venue_hub_url: env::var("VENUE_HUB_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8081".into()),
            tg_bot_secret,
            default_amount: env::var("TG_DEFAULT_AMOUNT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(50.0),
        }
    }

    pub fn is_configured(&self) -> bool {
        !self.tg_token.trim().is_empty()
    }
}
