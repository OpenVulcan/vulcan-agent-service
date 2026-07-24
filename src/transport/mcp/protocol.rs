//! MCP protocol facade with compatibility re-exports.
//! MCP 协议 facade，保留兼容导出路径。

mod capabilities;
mod content;
mod initialization;
mod params;
mod tools;
mod version;

pub use capabilities::*;
pub use content::*;
pub use initialization::*;
pub use params::*;
pub use tools::*;
pub use version::*;
