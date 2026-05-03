//! Application configuration facade.
//! 应用配置 facade。

mod loading;
mod paths;
#[cfg(test)]
mod tests;
mod types;

#[allow(unused_imports)]
pub use types::{
    Config, NamedSkillRootConfig, RunLuaPoolConfigSection, SkillRootConfigEntry,
    SpaceControllerConfig, SpaceControllerProcessModeConfig,
};
