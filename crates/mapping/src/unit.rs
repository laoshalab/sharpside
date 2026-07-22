//! 单位换算。对应 `docs/VENUE_DESIGN.md` §6.4。
//!
//! 各 Venue 价格/数量单位不同，跨 Venue 跟单须换算：
//! - Polymarket CTF：价格 0.0–1.0 USDC
//! - Kalshi：价格 1–99 cents（USD）
//! - Manifold：mana（玩钱）
//! - 链上原生

use sharpside_venues_core::Unit;

/// 价格换算。对应 `docs/VENUE_DESIGN.md` §6.4 `convert_price`。
///
/// - `UsdcCtf → UsdCents`：`price * 100`（0.5 USDC → 50 cents）
/// - `UsdCents → UsdcCtf`：`price / 100`
/// - 同单位：原样返回
/// - 其他（Mana/Native）：暂原样返回，按需扩展
pub fn convert_price(from: Unit, to: Unit, price: f64) -> f64 {
    match (from, to) {
        (Unit::UsdcCtf, Unit::UsdCents) => price * 100.0,
        (Unit::UsdCents, Unit::UsdcCtf) => price / 100.0,
        (Unit::UsdcCtf, Unit::UsdcCtf) => price,
        (Unit::UsdCents, Unit::UsdCents) => price,
        // 链上/玩钱场景按需扩展，当前原样返回
        _ => price,
    }
}

/// 数量换算。对应 `docs/VENUE_DESIGN.md` §6.4 `convert_size`。
///
/// 按 USDC notional 等价换算：`notional = size * price`，
/// 目标 `size = notional / target_price`。
/// `target_price ≤ 0` 时原样返回 `size`（避免除零，由调用方上游校验价格）。
pub fn convert_size(from: Unit, to: Unit, size: f64, price: f64) -> f64 {
    let notional = size * price;
    let target_price = convert_price(from, to, price);
    if target_price > 0.0 {
        notional / target_price
    } else {
        size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convert_price_usdc_to_cents() {
        assert!((convert_price(Unit::UsdcCtf, Unit::UsdCents, 0.50) - 50.0).abs() < 1e-9);
        assert!((convert_price(Unit::UsdcCtf, Unit::UsdCents, 0.30) - 30.0).abs() < 1e-9);
    }

    #[test]
    fn convert_price_cents_to_usdc() {
        assert!((convert_price(Unit::UsdCents, Unit::UsdcCtf, 50.0) - 0.50).abs() < 1e-9);
        assert!((convert_price(Unit::UsdCents, Unit::UsdcCtf, 30.0) - 0.30).abs() < 1e-9);
    }

    #[test]
    fn convert_price_same_unit() {
        assert!((convert_price(Unit::UsdcCtf, Unit::UsdcCtf, 0.42) - 0.42).abs() < 1e-9);
        assert!((convert_price(Unit::UsdCents, Unit::UsdCents, 42.0) - 42.0).abs() < 1e-9);
    }

    #[test]
    fn convert_price_unsupported_passthrough() {
        // Mana/Native 暂原样返回
        assert!((convert_price(Unit::Mana, Unit::UsdcCtf, 0.42) - 0.42).abs() < 1e-9);
        assert!((convert_price(Unit::Native, Unit::UsdCents, 42.0) - 42.0).abs() < 1e-9);
    }

    #[test]
    fn convert_size_preserves_notional() {
        // 100 shares @ 0.50 USDC → notional 50 → @ 50 cents → 1 share? 不对
        // notional = 100 * 0.50 = 50 USDC；target_price = 50 cents；target_size = 50/50 = 1
        // 这表示在 Kalshi 1 份合约代表 1 USD notional，价格 50 cents
        let s = convert_size(Unit::UsdcCtf, Unit::UsdCents, 100.0, 0.50);
        assert!((s - 1.0).abs() < 1e-9);
    }

    #[test]
    fn convert_size_same_unit_identity() {
        let s = convert_size(Unit::UsdcCtf, Unit::UsdcCtf, 100.0, 0.50);
        assert!((s - 100.0).abs() < 1e-9);
    }

    #[test]
    fn convert_size_zero_target_price_passthrough() {
        // target_price = 0 → 返回原 size（避免除零）
        let s = convert_size(Unit::UsdcCtf, Unit::UsdCents, 100.0, 0.0);
        assert!((s - 100.0).abs() < 1e-9);
    }

    #[test]
    fn convert_roundtrip_preserves_notional() {
        // USDC → cents → USDC 应保持 notional 等价
        let price = 0.42;
        let size = 100.0;
        let notional_orig = size * price;
        let cents_price = convert_price(Unit::UsdcCtf, Unit::UsdCents, price);
        let cents_size = convert_size(Unit::UsdcCtf, Unit::UsdCents, size, price);
        let notional_cents = cents_size * cents_price;
        assert!((notional_orig - notional_cents).abs() < 1e-6);
    }
}
