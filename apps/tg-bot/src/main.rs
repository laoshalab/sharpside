//! TG bot · 通道 A（平台代签 session wallet）。对应 `docs/ARCHITECTURE.md` §6.3 / `docs/FLOWS.md` §6。
//!
//! teloxide 长轮询：`/start` 绑定账户（account `/auth/tg` 换 JWT），
//! `/follow` 建跟随（channel=tg），`/follows` `/unfollow` 管理，
//! `/traders` `/perf` 查看交易者与绩效。平台代签 session wallet，
//! 法律/产品定位为「平台代签」非「非托管」。

mod config;

use crate::config::Config;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::Deserialize;
use teloxide::dispatching::{Dispatcher, UpdateHandler};
use teloxide::prelude::*;
use teloxide::utils::command::BotCommands;

type Res = Result<String, String>;

/// 兼容 number / string 的 f64 提取（DB Decimal 默认序列化为字符串）。
fn num(v: &serde_json::Value) -> f64 {
    v.as_f64()
        .or_else(|| v.as_str().and_then(|s| s.parse::<f64>().ok()))
        .unwrap_or(0.0)
}

#[derive(Debug)]
struct BotState {
    cfg: Config,
    http: reqwest::Client,
    tokens: Mutex<HashMap<i64, String>>,
    amounts: Mutex<HashMap<i64, f64>>,
}

impl BotState {
    fn new(cfg: Config) -> Self {
        Self {
            cfg,
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(20))
                .build()
                .expect("reqwest client"),
            tokens: Mutex::new(HashMap::new()),
            amounts: Mutex::new(HashMap::new()),
        }
    }

    fn amount_for(&self, tg_id: i64) -> f64 {
        self.amounts
            .lock()
            .unwrap()
            .get(&tg_id)
            .copied()
            .unwrap_or(self.cfg.default_amount)
    }

    /// 代 TG 用户换 JWT（带 X-TG-Bot-Secret），缓存到内存。
    async fn ensure_token(&self, tg_id: i64) -> Result<String, String> {
        if let Some(t) = self.tokens.lock().unwrap().get(&tg_id).cloned() {
            return Ok(t);
        }
        let url = format!("{}/auth/tg", self.cfg.account_url.trim_end_matches('/'));
        let resp = self
            .http
            .post(&url)
            .header("X-TG-Bot-Secret", &self.cfg.tg_bot_secret)
            .json(&serde_json::json!({"tg_id": tg_id}))
            .send()
            .await
            .map_err(|e| format!("account 不可达: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("account /auth/tg 返回 {}", resp.status()));
        }
        let v: serde_json::Value = resp.json().await.map_err(|e| format!("解析失败: {e}"))?;
        let token = v["token"]
            .as_str()
            .ok_or_else(|| "account 未返回 token".to_string())?
            .to_string();
        self.tokens.lock().unwrap().insert(tg_id, token.clone());
        Ok(token)
    }

    /// 401 时清缓存，重试一次。闭包收 `Arc<Self>`（owned），future 不借 `&Self`，规避 HRTB 借期问题。
    async fn with_token<F, Fut>(self: Arc<Self>, tg_id: i64, f: F) -> Res
    where
        F: Fn(Arc<Self>, String) -> Fut + Send + Sync,
        Fut: std::future::Future<Output = Res> + Send,
    {
        let token = self.ensure_token(tg_id).await?;
        match f(self.clone(), token.clone()).await {
            Ok(s) => Ok(s),
            Err(e) if e.contains("401") || e.contains("Unauthorized") => {
                self.tokens.lock().unwrap().remove(&tg_id);
                let token = self.ensure_token(tg_id).await?;
                f(self.clone(), token).await
            }
            Err(e) => Err(e),
        }
    }
}

#[derive(BotCommands, Clone)]
#[command(
    rename_rule = "lowercase",
    description = "Sharpside 跟单 bot（通道A·平台代签）"
)]
enum Command {
    #[command(description = "开始并绑定账户")]
    Start,
    #[command(description = "显示帮助")]
    Help,
    #[command(description = "跟随 <platform> <address> [amount] —— 通道A(tg)跟单")]
    Follow { args: String },
    #[command(description = "列出我的跟随")]
    Follows,
    #[command(description = "取消跟随 <id>")]
    Unfollow { id: String },
    #[command(description = "列出热门交易者")]
    Traders,
    #[command(description = "查绩效 <platform> <address>")]
    Perf { args: String },
    #[command(description = "设置默认下单金额 <amount>")]
    SetAmount { amount: f64 },
}

async fn answer(bot: Bot, msg: Message, cmd: Command, state: Arc<BotState>) -> ResponseResult<()> {
    let tg_id = msg.from.as_ref().map(|u| u.id.0 as i64).unwrap_or(0);
    if tg_id == 0 {
        bot.send_message(msg.chat.id, "无法识别用户").await?;
        return Ok(());
    }
    let res: Res = match cmd {
        Command::Start => cmd_start(state.as_ref(), tg_id).await,
        Command::Help => Ok(help_text()),
        Command::Follow { args } => {
            let parts: Vec<&str> = args.split_whitespace().collect();
            let platform = parts.first().copied().unwrap_or("").to_string();
            let address = parts.get(1).copied().unwrap_or("").to_string();
            let amount = parts.get(2).and_then(|s| s.parse::<f64>().ok());
            if platform.is_empty() || address.is_empty() {
                Err("用法：/follow <platform> <address> [amount]".into())
            } else {
                cmd_follow(state.clone(), tg_id, &platform, &address, amount).await
            }
        }
        Command::Follows => cmd_follows(state.clone(), tg_id).await,
        Command::Unfollow { id } => cmd_unfollow(state.clone(), tg_id, &id).await,
        Command::Traders => cmd_traders(state.as_ref()).await,
        Command::Perf { args } => {
            let parts: Vec<&str> = args.split_whitespace().collect();
            let platform = parts.first().copied().unwrap_or("").to_string();
            let address = parts.get(1).copied().unwrap_or("").to_string();
            if platform.is_empty() || address.is_empty() {
                Err("用法：/perf <platform> <address>".into())
            } else {
                cmd_perf(state.as_ref(), &platform, &address).await
            }
        }
        Command::SetAmount { amount } => {
            state.amounts.lock().unwrap().insert(tg_id, amount);
            Ok(format!("默认下单金额已设为 {amount} USDC"))
        }
    };
    let text = match res {
        Ok(s) => s,
        Err(e) => format!("❌ {e}"),
    };
    bot.send_message(msg.chat.id, text).await?;
    Ok(())
}

fn help_text() -> String {
    format!(
        "Sharpside 跟单 bot（通道A·平台代签）\n\n{}\n\n\
         说明：跟随后，被跟随交易者仓位变化会触发跟单指令，\
         平台用你的 session wallet 代签下单（非完全非托管，请知悉风险）。",
        Command::descriptions()
    )
}

async fn cmd_start(state: &BotState, tg_id: i64) -> Res {
    state.ensure_token(tg_id).await?;
    Ok(format!(
        "已绑定账户。欢迎来到 Sharpside！\n\n{}",
        help_text()
    ))
}

async fn cmd_follow(
    state: Arc<BotState>,
    tg_id: i64,
    platform: &str,
    address: &str,
    amount: Option<f64>,
) -> Res {
    let amt = amount.unwrap_or_else(|| state.amount_for(tg_id));
    let body = serde_json::json!({
        "follow_platform": platform,
        "follow_address": address,
        "execute_venue": "polymarket",
        "channel": "tg",
        "config": {
            "sizing": {"mode":"fixed","value":{"amount": amt}},
            "execute_venue": "polymarket",
            "channel": "tg",
            "same_venue_only": false
        }
    });
    state
        .with_token(tg_id, |s, token| {
            let body = body.clone();
            async move {
                let url = format!("{}/follows", s.cfg.follow_url.trim_end_matches('/'));
                let resp = s
                    .http
                    .post(&url)
                    .header("Authorization", format!("Bearer {token}"))
                    .header("Content-Type", "application/json")
                    .body(body.to_string())
                    .send()
                    .await
                    .map_err(|e| format!("follow 不可达: {e}"))?;
                let status = resp.status();
                let txt = resp.text().await.unwrap_or_default();
                if status.as_u16() == 401 {
                    return Err("401 Unauthorized".into());
                }
                if !status.is_success() {
                    return Err(format!(
                        "follow 返回 {status}: {}",
                        txt.chars().take(200).collect::<String>()
                    ));
                }
                let v: serde_json::Value =
                    serde_json::from_str(&txt).map_err(|e| format!("解析失败: {e}"))?;
                let id = v["id"].as_str().unwrap_or("?");
                Ok(format!(
                    "✅ 已跟随 {platform}/{address}（通道A·金额 {amt} USDC）\nid={id}"
                ))
            }
        })
        .await
}

async fn cmd_follows(state: Arc<BotState>, tg_id: i64) -> Res {
    state
        .with_token(tg_id, |s, token| async move {
            let url = format!("{}/me/follows", s.cfg.follow_url.trim_end_matches('/'));
            let resp = s
                .http
                .get(&url)
                .header("Authorization", format!("Bearer {token}"))
                .send()
                .await
                .map_err(|e| format!("follow 不可达: {e}"))?;
            let status = resp.status();
            if status.as_u16() == 401 {
                return Err("401 Unauthorized".into());
            }
            if !status.is_success() {
                return Err(format!("follow 返回 {status}"));
            }
            let arr: Vec<serde_json::Value> =
                resp.json().await.map_err(|e| format!("解析失败: {e}"))?;
            if arr.is_empty() {
                return Ok("暂无跟随".into());
            }
            let mut out = String::from("我的跟随：\n");
            for r in arr {
                let id = r["id"].as_str().unwrap_or("?");
                let fp = r["follow_platform"].as_str().unwrap_or("?");
                let fa = r["follow_address"].as_str().unwrap_or("?");
                let ch = r["channel"].as_str().unwrap_or("?");
                let active = r["active"].as_bool().unwrap_or(true);
                out.push_str(&format!(
                    "• {id}  {fp}/{fa}  [{ch}]  {}\n",
                    if active { "active" } else { "paused" }
                ));
            }
            Ok(out)
        })
        .await
}

async fn cmd_unfollow(state: Arc<BotState>, tg_id: i64, id: &str) -> Res {
    state
        .with_token(tg_id, |s, token| async move {
            let url = format!("{}/follows/{}", s.cfg.follow_url.trim_end_matches('/'), id);
            let resp = s
                .http
                .delete(&url)
                .header("Authorization", format!("Bearer {token}"))
                .send()
                .await
                .map_err(|e| format!("follow 不可达: {e}"))?;
            let status = resp.status();
            if status.as_u16() == 401 {
                return Err("401 Unauthorized".into());
            }
            if !status.is_success() {
                return Err(format!("unfollow 返回 {status}"));
            }
            Ok(format!("✅ 已取消跟随 {id}"))
        })
        .await
}

async fn cmd_traders(state: &BotState) -> Res {
    let url = format!("{}/traders", state.cfg.venue_hub_url.trim_end_matches('/'));
    let resp = state
        .http
        .get(&url)
        .query(&[("platform", "polymarket"), ("limit", "10")])
        .send()
        .await
        .map_err(|e| format!("venue-hub 不可达: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("venue-hub 返回 {}", resp.status()));
    }
    let arr: Vec<serde_json::Value> = resp.json().await.map_err(|e| format!("解析失败: {e}"))?;
    if arr.is_empty() {
        return Ok("暂无交易者（ingest worker 未跑或无数据）".into());
    }
    let mut out = String::from("热门交易者（polymarket）：\n");
    for (i, t) in arr.iter().enumerate() {
        let addr = t["address"].as_str().unwrap_or("?");
        let alias = t["alias"]
            .as_str()
            .or_else(|| t["user_name"].as_str())
            .unwrap_or("?");
        let verified = t["verified_badge"].as_bool().unwrap_or(false);
        out.push_str(&format!(
            "{}. {alias}  {}{}\n   {addr}\n",
            i + 1,
            if verified { "✅" } else { "" },
            "",
        ));
    }
    out.push_str("\n/perf polymarket <address> 查看绩效");
    Ok(out)
}

#[derive(Debug, Deserialize)]
struct PerfOut {
    performance: Vec<serde_json::Value>,
    tags: Vec<String>,
}

async fn cmd_perf(state: &BotState, platform: &str, address: &str) -> Res {
    let url = format!(
        "{}/traders/{}/{}/performance",
        state.cfg.venue_hub_url.trim_end_matches('/'),
        platform,
        address
    );
    let resp = state
        .http
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("venue-hub 不可达: {e}"))?;
    if resp.status().as_u16() == 404 {
        return Err(format!(
            "无绩效数据：{platform}/{address}（先 /traders/import 触发回填+perf）"
        ));
    }
    if !resp.status().is_success() {
        return Err(format!("venue-hub 返回 {}", resp.status()));
    }
    let v: PerfOut = resp.json().await.map_err(|e| format!("解析失败: {e}"))?;
    if v.performance.is_empty() {
        return Err(format!("无绩效数据：{platform}/{address}"));
    }
    let mut out = format!("绩效 {platform}/{address}\n");
    for p in &v.performance {
        let period = p["period"].as_str().unwrap_or("?");
        let roi = num(&p["roi"]);
        let win = num(&p["win_rate"]);
        let pnl = num(&p["realized_pnl"]);
        let pos = p["position_count"].as_i64().unwrap_or(0);
        out.push_str(&format!(
            "• {period}: ROI {:.2}%  win_rate {:.0}%  realized {pnl:.2}  positions {pos}\n",
            roi * 100.0,
            win * 100.0,
        ));
    }
    if !v.tags.is_empty() {
        out.push_str(&format!("标签: {}", v.tags.join(", ")));
    }
    Ok(out)
}

fn handler_tree() -> UpdateHandler<teloxide::RequestError> {
    dptree::entry().branch(
        Update::filter_message()
            .filter_command::<Command>()
            .endpoint(answer),
    )
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 安全修复 4.2：生产或 LOG_FORMAT=json → JSON 结构化日志。
    {
        let filter = tracing_subscriber::EnvFilter::from_default_env();
        let use_json = sharpside_shared::secrets::is_production()
            || std::env::var("LOG_FORMAT").ok().as_deref() == Some("json");
        if use_json {
            tracing_subscriber::fmt()
                .json()
                .with_env_filter(filter)
                .with_current_span(false)
                .with_span_list(false)
                .init();
        } else {
            tracing_subscriber::fmt().with_env_filter(filter).init();
        }
    }

    let cfg = Config::from_env();
    if !cfg.is_configured() {
        anyhow::bail!("TG_BOT_TOKEN 未配置，无法启动 tg-bot");
    }
    tracing::info!(account = %cfg.account_url, follow = %cfg.follow_url, venue_hub = %cfg.venue_hub_url, "tg-bot 启动");

    let bot = Bot::new(cfg.tg_token.clone());
    let state = Arc::new(BotState::new(cfg));
    Dispatcher::builder(bot, handler_tree())
        .dependencies(dptree::deps![state])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;
    Ok(())
}
