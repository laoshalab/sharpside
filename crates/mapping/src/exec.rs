//! 执行参数校验。对应 `docs/VENUE_DESIGN.md` §6.4 `apply_exec_params`。
//!
//! Copier 下单前按 Venue 差异化套用执行参数：
//! - 滑点保护：`(order.price - mid).abs() / mid` 超过 `max_slippage_bps` → 拒单
//! - 最小 notional：`order.size * order.price < min_notional` → 拒单
//! - 费率从成交回报里扣减，此处仅校验

use crate::types::{ExecError, ExecParams};
use sharpside_venues_core::{Order, OrderBook};

/// 校验订单是否满足执行参数约束。对应 `docs/VENUE_DESIGN.md` §6.4。
///
/// 顺序：空盘口 → 滑点 → 最小 notional。任一不满足返回对应 [`ExecError`]。
/// 费率不在此时扣减（从成交回报里扣），仅做约束校验。
pub fn apply_exec_params(order: &Order, book: &OrderBook, p: &ExecParams) -> Result<(), ExecError> {
    let mid = mid_price(book).ok_or(ExecError::EmptyBook)?;

    if mid <= 0.0 {
        return Err(ExecError::EmptyBook);
    }

    let slip = (order.price - mid).abs() / mid;
    let actual_bps = slip * 10_000.0;
    if actual_bps > p.max_slippage_bps {
        return Err(ExecError::SlippageExceeded {
            actual_bps,
            max_bps: p.max_slippage_bps,
        });
    }

    let notional = order.size * order.price;
    if notional < p.min_notional {
        return Err(ExecError::BelowMinNotional {
            notional,
            min_notional: p.min_notional,
        });
    }

    Ok(())
}

/// 从盘口算中间价。需同时有买一价和卖一价。
fn mid_price(book: &OrderBook) -> Option<f64> {
    let best_bid = book.bids.first().map(|l| l.price);
    let best_ask = book.asks.first().map(|l| l.price);
    match (best_bid, best_ask) {
        (Some(b), Some(a)) => Some((b + a) / 2.0),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sharpside_shared::Side;
    use sharpside_venues_core::{Order, OrderBook, OrderBookLevel};

    fn book(bid: f64, ask: f64) -> OrderBook {
        OrderBook {
            market_id: "m".into(),
            token_id: "t".into(),
            bids: vec![OrderBookLevel {
                price: bid,
                size: 100.0,
            }],
            asks: vec![OrderBookLevel {
                price: ask,
                size: 100.0,
            }],
        }
    }

    fn order(price: f64, size: f64) -> Order {
        Order {
            market_id: "m".into(),
            token_id: "t".into(),
            side: Side::Buy,
            price,
            size,
            idempotency_salt: None,
            order_timestamp_ms: None,
            order_type: sharpside_venues_core::OrderType::Gtc,
            expiration: None,
            post_only: false,
        }
    }

    fn params(min_notional: f64, max_slip_bps: f64) -> ExecParams {
        ExecParams {
            taker_fee_bps: 100.0,
            min_notional,
            max_slippage_bps: max_slip_bps,
            min_size: 0.0,
        }
    }

    #[test]
    fn ok_within_limits() {
        // mid = (0.49 + 0.51)/2 = 0.50；order@0.50 slip=0；notional=0.50*100=50 > 1
        let result = apply_exec_params(&order(0.50, 100.0), &book(0.49, 0.51), &params(1.0, 200.0));
        assert!(result.is_ok());
    }

    #[test]
    fn slippage_exceeded() {
        // mid=0.50, order@0.55 → slip = 0.05/0.50 = 0.10 = 1000bps > 200bps
        let result = apply_exec_params(&order(0.55, 100.0), &book(0.49, 0.51), &params(1.0, 200.0));
        assert!(matches!(
            result,
            Err(ExecError::SlippageExceeded { actual_bps, max_bps }) if (actual_bps - 1000.0).abs() < 1e-6 && (max_bps - 200.0).abs() < 1e-6
        ));
    }

    #[test]
    fn below_min_notional() {
        // notional = 0.50 * 1 = 0.5 < 1.0
        let result = apply_exec_params(&order(0.50, 1.0), &book(0.49, 0.51), &params(1.0, 200.0));
        assert!(matches!(
            result,
            Err(ExecError::BelowMinNotional { notional, min_notional }) if (notional - 0.5).abs() < 1e-9 && (min_notional - 1.0).abs() < 1e-9
        ));
    }

    #[test]
    fn empty_book_bid() {
        let mut b = book(0.49, 0.51);
        b.bids.clear();
        let result = apply_exec_params(&order(0.50, 100.0), &b, &params(1.0, 200.0));
        assert_eq!(result, Err(ExecError::EmptyBook));
    }

    #[test]
    fn empty_book_ask() {
        let mut b = book(0.49, 0.51);
        b.asks.clear();
        let result = apply_exec_params(&order(0.50, 100.0), &b, &params(1.0, 200.0));
        assert_eq!(result, Err(ExecError::EmptyBook));
    }

    #[test]
    fn slippage_checked_before_notional() {
        // 同时超滑点和超 notional：先报滑点
        let result = apply_exec_params(&order(0.99, 0.1), &book(0.49, 0.51), &params(100.0, 200.0));
        assert!(matches!(result, Err(ExecError::SlippageExceeded { .. })));
    }

    #[test]
    fn near_max_slippage_within_limit_ok() {
        // mid≈0.50, order@0.508 → slip ≈ 0.008/0.50 = 1.6% = 160bps < 200bps（高但未超）
        let result =
            apply_exec_params(&order(0.508, 100.0), &book(0.49, 0.51), &params(1.0, 200.0));
        assert!(result.is_ok());
    }

    #[test]
    fn boundary_notional_equal_ok() {
        // notional = 0.50 * 2 = 1.0 == min_notional 1.0（不超）
        let result = apply_exec_params(&order(0.50, 2.0), &book(0.49, 0.51), &params(1.0, 200.0));
        assert!(result.is_ok());
    }
}
