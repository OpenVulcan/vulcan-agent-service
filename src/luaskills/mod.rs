mod context;
mod engine_options;
mod runtime_paths;
mod tool_mapping;

pub use context::{
    build_grpc_runtime_invocation_context, build_grpc_runtime_request_context,
    build_runtime_invocation_context, build_runtime_request_context,
    client_budget_snapshot_for_render, grpc_client_budget_snapshot_for_render,
};
#[allow(unused_imports)]
pub use engine_options::host_reserved_tool_names;
pub use engine_options::{
    build_luaskills_cache_config, build_luaskills_engine_options, install_luaskills_log_callback,
};
pub use runtime_paths::{
    default_user_skill_root, normalize_skill_root_key, resolve_runtime_root_from_config,
    resolve_skill_config_file_path, resolve_skill_roots_from_config,
    validate_unique_skill_root_spaces,
};
pub use tool_mapping::map_runtime_entry_to_mcp_tool;
pub(crate) use tool_mapping::{
    LuaSkillToolProjectionOptions, inject_managed_luaskill_sid_argument,
    project_runtime_tool_descriptor,
};

#[cfg(test)]
use engine_options::{resolve_space_controller_options, space_controller_executable_file_name};
#[cfg(test)]
use runtime_paths::resolve_implicit_runtime_root_from_paths;
#[cfg(test)]
mod tests;
