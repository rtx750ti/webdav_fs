//! 测试模块入口：公共逻辑在 `lib` 子模块，集成测试在 `internal`。

#[cfg(test)]
mod lib;
#[cfg(test)]
pub use lib::*;

pub mod internal;
