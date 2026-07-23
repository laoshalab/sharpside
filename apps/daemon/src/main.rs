//! daemon · 通道 B 自托管执行客户端（ratatui TUI）。
//!
//! 对应 `docs/ARCHITECTURE.md` §6.3 / `docs/FLOWS.md` §7。
//! 平台零钥：daemon 本地持有 `daemon_api_key`，轮询 copier 拉取待派发指令，
//! 本地风控后（dry_run 合成 / Phase 1b 本地签名）回传成交结果。
//!
//! 架构：主线程跑 ratatui 渲染循环 + 终端事件；后台 tokio 任务轮询 copier，
//! 状态经 `Arc<Mutex<UiState>>` 共享。dry_run 合成成交回报，演示通道 B 闭环。

mod config;
mod sign;
mod ui;

use crate::config::Config;
use crate::ui::{OrderRow, UiState};
use chrono::{DateTime, Utc};
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use serde::{Deserialize, Serialize};
use std::io::{stdout, IsTerminal};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{info, warn};

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct CopyOrderDto {
    id: uuid::Uuid,
    #[serde(default)]
    execute_venue: String,
    #[serde(default)]
    source_market_id: String,
    #[serde(default)]
    source_token_id: String,
    #[serde(default)]
    execute_market_id: Option<String>,
    #[serde(default)]
    execute_token_id: Option<String>,
    #[serde(default)]
    side: String,
    #[serde(default)]
    price: String,
    #[serde(default)]
    size: String,
}

#[derive(Debug, Serialize)]
struct ResultBody {
    status: String,
    filled_size: Option<f64>,
    filled_price: Option<f64>,
    fee: Option<f64>,
    tx_hash: Option<String>,
    venue_order_id: Option<String>,
    skip_reason: Option<String>,
    /// 回写映射后的执行市场/token（同 Venue 时等于 source；跨 Venue 由 daemon 本地映射）。
    execute_market_id: Option<String>,
    execute_token_id: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = Config::from_env();
    let configured = config.is_configured();
    if !configured {
        warn!("DAEMON_USER_ID / DAEMON_API_KEY 未配置，daemon 仅空跑（不拉指令）");
    }
    info!(
        copier = %config.copier_url,
        user = %config.user_id,
        dry_run = config.dry_run,
        "daemon 启动"
    );

    let state = Arc::new(Mutex::new(UiState::new(
        configured,
        config.user_id.clone(),
        config.dry_run,
        config.copier_url.clone(),
        config.poll_interval_secs,
    )));
    {
        let mut s = state.lock().unwrap();
        s.log(format!(
            "daemon 启动 · copier={} · dry_run={}",
            config.copier_url, config.dry_run
        ));
        if !configured {
            s.log("未配置 user/api_key，仅空跑");
        }
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;

    // 非 TTY（如 e2e.sh / systemd / CI）或显式 DAEMON_HEADLESS=1 时跑 headless：
    // 不进 ratatui，轮询循环直接在主任务跑，日志走 tracing。
    let headless = !stdout().is_terminal() || crate::config::parse_bool("DAEMON_HEADLESS", false);
    if headless {
        info!("headless 模式（无 TTY）：轮询日志走 tracing");
        poll_loop(client, config, state).await;
        return Ok(());
    }

    // 后台轮询任务
    let poll_state = state.clone();
    let poll_client = client.clone();
    let poll_cfg = config.clone();
    tokio::spawn(async move {
        poll_loop(poll_client, poll_cfg, poll_state).await;
    });

    // 终端事件读取（阻塞 → 通道）
    let (ev_tx, mut ev_rx) = mpsc::channel::<Event>(64);
    tokio::task::spawn_blocking(move || loop {
        if event::poll(Duration::from_millis(100)).unwrap_or(false) {
            if let Ok(ev) = event::read() {
                if ev_tx.blocking_send(ev).is_err() {
                    break;
                }
            }
        }
    });

    // ── TUI ──
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_ui(&mut terminal, &state, &mut ev_rx).await;

    // 恢复终端
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

async fn run_ui(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    state: &Arc<Mutex<UiState>>,
    ev_rx: &mut mpsc::Receiver<Event>,
) -> anyhow::Result<()> {
    let mut tick = tokio::time::interval(Duration::from_millis(250));
    loop {
        terminal.draw(|f| {
            let s = state.lock().unwrap();
            ui::render(f, &s);
        })?;

        tokio::select! {
            _ = tick.tick() => {}
            ev = ev_rx.recv() => {
                if let Some(Event::Key(k)) = ev {
                    if k.kind != KeyEventKind::Release
                        && (k.code == crossterm::event::KeyCode::Char('q')
                            || k.code == crossterm::event::KeyCode::Esc)
                    {
                        return Ok(());
                    }
                }
            }
        }
    }
}

async fn poll_loop(client: reqwest::Client, config: Config, state: Arc<Mutex<UiState>>) {
    let mut since: DateTime<Utc> = Utc::now() - chrono::Duration::hours(24);
    loop {
        if config.is_configured() {
            if let Err(e) = poll_once(&client, &config, &mut since, &state).await {
                let msg = format!("轮询失败: {e}");
                warn!(error = %e, "轮询失败");
                if let Ok(mut s) = state.lock() {
                    s.last_error = Some(msg.clone());
                    s.log(msg);
                }
            }
        }
        tokio::time::sleep(Duration::from_secs(config.poll_interval_secs)).await;
    }
}

async fn poll_once(
    client: &reqwest::Client,
    config: &Config,
    since: &mut DateTime<Utc>,
    state: &Arc<Mutex<UiState>>,
) -> Result<(), anyhow::Error> {
    let url = format!("{}/me/copy-orders", config.copier_url.trim_end_matches('/'));
    let since_str = since.to_rfc3339();
    let resp = client
        .get(&url)
        .header("X-User-Id", &config.user_id)
        .header("X-Daemon-Api-Key", &config.daemon_api_key)
        .query(&[
            ("since", since_str.as_str()),
            ("channel", "daemon"),
            ("limit", "100"),
        ])
        .send()
        .await?;
    if !resp.status().is_success() {
        anyhow::bail!("GET /me/copy-orders 返回 {}", resp.status());
    }
    let orders: Vec<CopyOrderDto> = resp.json().await?;

    {
        let mut s = state.lock().unwrap();
        s.polls += 1;
        s.last_poll_at = Some(Utc::now());
        s.last_error = None;
        if !orders.is_empty() {
            s.log(format!("拉取到 {} 条待派发指令", orders.len()));
            info!(n = orders.len(), "拉取到待派发指令");
        }
    }

    for o in orders {
        let price: f64 = o.price.parse().unwrap_or(0.0);
        let size: f64 = o.size.parse().unwrap_or(0.0);
        let notional = price * size;
        let market = o
            .execute_market_id
            .clone()
            .unwrap_or_else(|| o.source_market_id.clone());
        let token = o
            .execute_token_id
            .clone()
            .unwrap_or_else(|| o.source_token_id.clone());

        // 本地风控
        if config.local_max_notional > 0.0 && notional > config.local_max_notional {
            let reason = format!("本地 notional {notional:.2} 超限");
            report(
                client,
                config,
                o.id,
                ResultBody {
                    status: "skipped".into(),
                    filled_size: None,
                    filled_price: None,
                    fee: None,
                    tx_hash: None,
                    venue_order_id: None,
                    skip_reason: Some(reason.clone()),
                    execute_market_id: Some(market.clone()),
                    execute_token_id: Some(token.clone()),
                },
            )
            .await?;
            record(state, &o, &market, price, size, "skipped", Some(reason));
            continue;
        }

        if config.dry_run {
            report(
                client,
                config,
                o.id,
                ResultBody {
                    status: "filled".into(),
                    filled_size: Some(size),
                    filled_price: Some(price),
                    fee: Some(0.0),
                    tx_hash: Some(format!("dry-run-{}", o.id)),
                    venue_order_id: Some(format!("dry-{}", o.id)),
                    skip_reason: None,
                    execute_market_id: Some(market.clone()),
                    execute_token_id: Some(token.clone()),
                },
            )
            .await?;
            record(state, &o, &market, price, size, "filled", None);
            info!(order = %o.id, venue = %o.execute_venue, "dry-run 合成成交回传");
        } else {
            // 本地 EIP-712 签名（平台零钥）：按 execute_venue 分发。
            match execute_local(&o, price, size).await {
                Ok(fill) => {
                    report(
                        client,
                        config,
                        o.id,
                        ResultBody {
                            status: "filled".into(),
                            filled_size: Some(fill.filled_size),
                            filled_price: Some(fill.filled_price),
                            fee: Some(fill.fee),
                            tx_hash: Some(fill.tx_hash.clone()),
                            venue_order_id: Some(fill.venue_order_id.clone()),
                            skip_reason: None,
                            execute_market_id: Some(market.clone()),
                            execute_token_id: Some(token.clone()),
                        },
                    )
                    .await?;
                    record(state, &o, &market, price, size, "filled", None);
                    info!(
                        order = %o.id,
                        venue = %o.execute_venue,
                        dry_sign = fill.dry_sign,
                        "本地签名成交回传"
                    );
                }
                Err(reason) => {
                    report(
                        client,
                        config,
                        o.id,
                        ResultBody {
                            status: "failed".into(),
                            filled_size: None,
                            filled_price: None,
                            fee: None,
                            tx_hash: None,
                            venue_order_id: None,
                            skip_reason: Some(reason.clone()),
                            execute_market_id: Some(market.clone()),
                            execute_token_id: Some(token.clone()),
                        },
                    )
                    .await?;
                    record(state, &o, &market, price, size, "failed", Some(reason));
                }
            }
        }
    }

    *since = Utc::now();
    Ok(())
}

/// 按 execute_venue 本地签名执行（当前仅 polymarket）。
async fn execute_local(o: &CopyOrderDto, price: f64, size: f64) -> Result<sign::LocalFill, String> {
    let token_id = o
        .execute_token_id
        .as_deref()
        .unwrap_or(o.source_token_id.as_str());
    match o.execute_venue.as_str() {
        "polymarket" => sign::execute_polymarket(token_id, &o.side, price, size).await,
        other => Err(format!("venue {other} 本地签名未实现")),
    }
}

fn record(
    state: &Arc<Mutex<UiState>>,
    o: &CopyOrderDto,
    market: &str,
    price: f64,
    size: f64,
    status: &str,
    skip_reason: Option<String>,
) {
    let mut s = state.lock().unwrap();
    s.orders_seen += 1;
    match status {
        "filled" => s.orders_filled += 1,
        "skipped" => s.orders_skipped += 1,
        "failed" => s.orders_failed += 1,
        _ => {}
    }
    s.push_order(OrderRow {
        id: o.id.to_string(),
        venue: o.execute_venue.clone(),
        market: market.to_string(),
        side: o.side.clone(),
        price,
        size,
        status: status.to_string(),
        skip_reason: skip_reason.clone(),
        at: Utc::now(),
    });
    let tail = match status {
        "filled" => format!(
            "[OK] {} {} @ {:.4} ({}:{})",
            o.execute_venue, o.side, price, market, o.id
        ),
        "skipped" => format!(
            "[SKIP] {} {}",
            o.execute_venue,
            skip_reason.as_deref().unwrap_or("")
        ),
        _ => format!(
            "[FAIL] {} {}",
            o.execute_venue,
            skip_reason.as_deref().unwrap_or("")
        ),
    };
    s.log(tail);
}

async fn report(
    client: &reqwest::Client,
    config: &Config,
    id: uuid::Uuid,
    body: ResultBody,
) -> Result<(), anyhow::Error> {
    let url = format!(
        "{}/me/copy-orders/{}/result",
        config.copier_url.trim_end_matches('/'),
        id
    );
    let resp = client
        .post(&url)
        .header("X-User-Id", &config.user_id)
        .header("X-Daemon-Api-Key", &config.daemon_api_key)
        .json(&body)
        .send()
        .await?;
    if !resp.status().is_success() {
        anyhow::bail!("POST /result 返回 {} for {id}", resp.status());
    }
    Ok(())
}
