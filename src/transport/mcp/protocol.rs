//! MCP protocol facade with compatibility re-exports.
//! MCP 协议 facade，保留兼容导出路径。

mod capabilities;
mod client_interaction;
mod content;
mod initialization;
mod prompts;
mod resources;
mod tools;
mod version;

pub use capabilities::*;
pub use client_interaction::*;
pub use content::*;
pub use initialization::*;
pub use prompts::*;
pub use resources::*;
pub use tools::*;
pub use version::*;
