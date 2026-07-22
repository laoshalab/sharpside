//! TUI 共享状态 + ratatui 渲染。
//!
//! 后台轮询任务更新 `UiState`（经 `Arc<Mutex<UiState>>`），主线程按 tick 读取并渲染。
//! 终端事件（按键）由 `spawn_blocking` 任务读取，经 mpsc 通道送主循环。

use std::collections::VecDeque;

use chrono::{DateTime, Utc};

const ORDERS_CAP: usize = 200;
const LOG_CAP: usize = 500;

#[derive(Debug, Clone)]
pub struct OrderRow {
    #[allow(dead_code)]
    pub id: String,
    pub venue: String,
    pub market: String,
    pub side: String,
    pub price: f64,
    pub size: f64,
    pub status: String,
    pub skip_reason: Option<String>,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct UiState {
    pub configured: bool,
    pub user_id: String,
    pub dry_run: bool,
    pub copier_url: String,
    pub poll_interval: u64,
    pub last_poll_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub polls: u64,
    pub orders_seen: u64,
    pub orders_filled: u64,
    pub orders_skipped: u64,
    pub orders_failed: u64,
    pub recent: VecDeque<OrderRow>,
    pub log: VecDeque<String>,
}

impl UiState {
    pub fn new(
        configured: bool,
        user_id: String,
        dry_run: bool,
        copier_url: String,
        poll_interval: u64,
    ) -> Self {
        Self {
            configured,
            user_id,
            dry_run,
            copier_url,
            poll_interval,
            last_poll_at: None,
            last_error: None,
            polls: 0,
            orders_seen: 0,
            orders_filled: 0,
            orders_skipped: 0,
            orders_failed: 0,
            recent: VecDeque::new(),
            log: VecDeque::new(),
        }
    }

    pub fn log(&mut self, line: impl Into<String>) {
        let ts = Utc::now().format("%H:%M:%S");
        self.log.push_back(format!("{ts}  {}", line.into()));
        while self.log.len() > LOG_CAP {
            self.log.pop_front();
        }
    }

    pub fn push_order(&mut self, row: OrderRow) {
        self.recent.push_back(row);
        while self.recent.len() > ORDERS_CAP {
            self.recent.pop_front();
        }
    }
}

/// 渲染整个帧。
pub fn render(frame: &mut ratatui::Frame, state: &UiState) {
    use ratatui::layout::{Alignment, Constraint, Layout};
    use ratatui::style::Style;
    use ratatui::text::Line;
    use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};

    let area = frame.area();
    let chunks = Layout::vertical([
        Constraint::Length(4),
        Constraint::Min(8),
        Constraint::Length(10),
        Constraint::Length(1),
    ])
    .split(area);

    // ── 头部：状态摘要 ──
    let status = if state.configured {
        (state.dry_run, "已配置 · dry_run 合成成交")
    } else {
        (true, "未配置（DAEMON_USER_ID/DAEMON_API_KEY 缺失）· 空跑")
    };
    let header = Block::default()
        .borders(Borders::ALL)
        .title(" sharpside-daemon · 通道 B 自托管客户端 ");
    let header_lines = vec![
        Line::from(format!(
            "状态: {}    copier: {}    轮询: {}s",
            status.1, state.copier_url, state.poll_interval
        ))
        .style(if status.0 {
            Style::default().yellow()
        } else {
            Style::default().green()
        }),
        Line::from(format!(
            "user: {}    轮询次数: {}    指令: {} (✓{} ⚠{} ✗{})    上次轮询: {}",
            state.user_id,
            state.polls,
            state.orders_seen,
            state.orders_filled,
            state.orders_skipped,
            state.orders_failed,
            state
                .last_poll_at
                .map(|t| t.to_rfc3339())
                .unwrap_or_else(|| "—".into()),
        )),
    ];
    frame.render_widget(Paragraph::new(header_lines).block(header), chunks[0]);

    // ── 中部：近期指令表 ──
    let table_block = Block::default().borders(Borders::ALL).title(" 近期指令 ");
    let header_cells = [
        "时间", "Venue", "Market", "Side", "Price", "Size", "Status", "Skip",
    ];
    let rows = state.recent.iter().rev().map(|o| {
        let status_style = match o.status.as_str() {
            "filled" => Style::default().green(),
            "skipped" => Style::default().yellow(),
            "failed" => Style::default().red(),
            _ => Style::default(),
        };
        Row::new([
            Cell::from(o.at.format("%H:%M:%S").to_string()),
            Cell::from(o.venue.clone()),
            Cell::from(o.market.clone()),
            Cell::from(o.side.clone()),
            Cell::from(format!("{:.4}", o.price)),
            Cell::from(format!("{:.4}", o.size)),
            Cell::from(o.status.clone()).style(status_style),
            Cell::from(o.skip_reason.clone().unwrap_or_default()),
        ])
    });
    let table = Table::new(
        rows,
        [
            Constraint::Length(8),
            Constraint::Length(10),
            Constraint::Min(12),
            Constraint::Length(6),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(8),
            Constraint::Min(10),
        ],
    )
    .header(Row::new(header_cells).style(Style::default().bold()))
    .block(table_block);
    frame.render_widget(table, chunks[1]);

    // ── 底部：日志 ──
    let log_block = Block::default().borders(Borders::ALL).title(" 日志 ");
    let log_lines: Vec<Line> = state
        .log
        .iter()
        .rev()
        .take(chunks[2].height as usize - 2)
        .rev()
        .map(|s| Line::from(s.as_str()))
        .collect();
    frame.render_widget(Paragraph::new(log_lines).block(log_block), chunks[2]);

    // ── 页脚：操作提示 ──
    let hint = Paragraph::new(Line::from(" q 退出 ").alignment(Alignment::Center))
        .style(Style::default().dim());
    frame.render_widget(hint, chunks[3]);
}
