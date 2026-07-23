//! 生产环境与密钥校验辅助。
//!
//! 对应安全审计 P0 修复：
//! - [`is_production`]：读取 `APP_ENV` / `SHARPSIDE_ENV`，值为 `production` 时返回 true。
//! - [`assert_secret`]：生产环境下，若密钥为空或命中已知默认值则 panic，拒绝启动。
//!
//! 设计目标：开发体验零损耗（缺失/默认值在 dev 下完全可用），生产强制收敛。
//! 已知默认值清单覆盖各服务 config 中的 `unwrap_or_else` 回退值与 `.env.example` 占位。

use std::env;

/// 已知不安全默认值（来自各服务 config 回退 + `.env.example` 占位）。
///
/// 命中即视为未配置。大小写敏感比较。
const KNOWN_DEFAULTS: &[&str] = &[
    "dev-secret-change-me",
    "dev_only_do_not_use_in_production",
    "dev-tg-bot-secret",
    "dev-admin-token",
    "dev-daemon-key-change-me",
];

/// 是否处于生产环境。
///
/// 判定规则：`APP_ENV` 优先，回退 `SHARPSIDE_ENV`，再回退 false。
/// 仅当值（trim 后、ASCII 小写）等于 `production` 时为 true。
pub fn is_production() -> bool {
    let raw = env::var("APP_ENV")
        .or_else(|_| env::var("SHARPSIDE_ENV"))
        .unwrap_or_default();
    raw.trim().eq_ignore_ascii_case("production")
}

/// 校验密钥：生产环境下，空值或已知默认值则 panic。
///
/// - `name`：变量名，仅用于错误信息（如 `"JWT_SECRET"`）。
/// - `value`：实际读取到的值。
///
/// 开发环境直接返回 `value`，不做任何校验，保持本地体验。
pub fn assert_secret<'a>(name: &str, value: &'a str) -> &'a str {
    if !is_production() {
        return value;
    }
    let trimmed = value.trim();
    if trimmed.is_empty() {
        panic!("{name} 未配置：生产环境禁止空值（APP_ENV=production）");
    }
    if KNOWN_DEFAULTS.contains(&trimmed) {
        panic!("{name} 命中已知默认值：生产环境必须使用独立强密钥（APP_ENV=production）");
    }
    value
}

/// 常时字节比较。用于共享 secret 校验（admin token / TG secret / daemon key /
/// internal signal secret），避免按字节短路比较带来的时序侧信道。
///
/// 长度不等仍走完整遍历后返回 false（不提前返回），与 `subtle::ConstantTimeEq` 行为一致。
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    let mut diff = 0u8;
    let n = a.len().min(b.len());
    for i in 0..n {
        diff |= a[i] ^ b[i];
    }
    // 长度差异也并入 diff，但保持常时：用 0x01 掩码吸收长度差
    diff |= (a.len() == b.len()) as u8 ^ 1;
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    // 这些测试通过修改进程级 env var 验证 is_production/assert_secret 行为，
    // 并行运行会互相覆盖 APP_ENV/SHARPSIDE_ENV 导致竞态失败，故用该锁串行化。
    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn dev_allows_defaults() {
        let _g = env_lock().lock().unwrap();
        // 默认（无 APP_ENV）即非生产，默认值放行
        std::env::remove_var("APP_ENV");
        std::env::remove_var("SHARPSIDE_ENV");
        assert_eq!(
            assert_secret("JWT_SECRET", "dev-secret-change-me"),
            "dev-secret-change-me"
        );
        assert_eq!(assert_secret("X", ""), "");
        assert!(!is_production());
    }

    #[test]
    fn production_rejects_empty_and_defaults() {
        let _g = env_lock().lock().unwrap();
        std::env::set_var("APP_ENV", "production");
        assert!(is_production());
        let res = std::panic::catch_unwind(|| assert_secret("JWT_SECRET", "dev-secret-change-me"));
        assert!(res.is_err(), "默认值在生产应 panic");
        let res = std::panic::catch_unwind(|| assert_secret("X", "  "));
        assert!(res.is_err(), "空值在生产应 panic");
        // 合法强密钥放行
        assert_eq!(
            assert_secret("JWT_SECRET", "a-very-long-random-32byte-secret-xxx"),
            "a-very-long-random-32byte-secret-xxx"
        );
        std::env::remove_var("APP_ENV");
    }

    #[test]
    fn production_detection_case_insensitive() {
        let _g = env_lock().lock().unwrap();
        std::env::set_var("APP_ENV", "Production");
        assert!(is_production());
        std::env::remove_var("APP_ENV");
        std::env::set_var("SHARPSIDE_ENV", "PRODUCTION");
        assert!(is_production());
        std::env::remove_var("SHARPSIDE_ENV");
        std::env::set_var("APP_ENV", "staging");
        assert!(!is_production());
        std::env::remove_var("APP_ENV");
    }

    #[test]
    fn constant_time_eq_matches_and_rejects() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"ab"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
        assert!(constant_time_eq(b"", b""));
        assert!(!constant_time_eq(b"", b"x"));
    }
}
