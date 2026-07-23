//! 类型化查询封装。按业务域分模块。
//!
//! 全部使用运行时 `sqlx::query_as`（非编译期宏），`cargo check` 无需连接 DB。

pub mod account;
pub mod identities;
pub mod mappings;
pub mod monitor;
pub mod ops;
pub mod outbox;
pub mod perf;
pub mod raw;
pub mod shadow;
pub mod traders;
