use super::*;
use crate::host_core::projections::render_help_list_markdown;
use crate::host_core::skill_tools::{
    parse_skill_manager_tool_arguments, select_skill_manager_user_root,
};
use crate::transport::mcp::McpDispatcher;
use crate::transport::mcp::protocol::{RequestContext, ToolCallResult};
use luaskills::{RuntimeHelpNodeDescriptor, RuntimeSkillHelpDescriptor};
use serde_json::Value;
use serde_json::json;
use std::collections::HashSet;

/// Build one unique temporary directory path for one server-module test case.
/// 为 server 模块单个测试用例构建唯一的临时目录路径。
fn unique_test_dir(name: &str) -> PathBuf {
    let unique = format!(
        "vulcan-agent-service-server-{}-{}-{}",
        name,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    );
    std::env::temp_dir().join(unique)
}

/// Write one minimal enabled LuaSkill fixture into a specific skills root.
/// 将一个最小可启用 LuaSkill 夹具写入指定 skills 根目录。
fn write_minimal_skill_to_root(skill_root: &std::path::Path, skill_id: &str) -> PathBuf {
    let skill_dir = skill_root.join(skill_id);
    std::fs::create_dir_all(skill_dir.join("runtime"))
        .expect("minimal skill runtime directory should be created");
    std::fs::write(
            skill_dir.join("skill.yaml"),
            format!(
                "name: {skill_id}\nversion: 0.1.0\nenable: true\ndebug: false\nentries:\n  - name: ping\n    description: Minimal ping entry.\n    lua_entry: runtime/ping.lua\n    lua_module: {skill_id}.ping\n"
            ),
        )
        .expect("minimal skill manifest should be written");
    std::fs::write(
        skill_dir.join("runtime").join("ping.lua"),
        "return function(args)\n  return 'ok'\nend\n",
    )
    .expect("minimal skill runtime entry should be written");
    skill_dir
}

fn make_help_descriptor() -> RuntimeSkillHelpDescriptor {
    RuntimeSkillHelpDescriptor {
        skill_id: "demo-skill".to_string(),
        skill_name: "Demo Skill".to_string(),
        skill_version: "1.2.3".to_string(),
        root_name: "ROOT".to_string(),
        skill_dir: "D:/runtime/lua_runtime/skills/demo-skill".to_string(),
        main: RuntimeHelpNodeDescriptor {
            flow_name: "main".to_string(),
            description: "Summarize the package-level capability surface.".to_string(),
            related_entries: vec![],
            is_main: true,
        },
        flows: vec![RuntimeHelpNodeDescriptor {
            flow_name: "search".to_string(),
            description: "Search indexed project files.".to_string(),
            related_entries: vec![],
            is_main: false,
        }],
    }
}

/// Skill-manager arguments should parse without any layer selector.
/// 不携带任何层级选择器的 skill-manager 参数应能正常解析。
#[test]
fn skill_manager_arguments_parse_without_layer() {
    let request = parse_skill_manager_tool_arguments(&json!({
        "action": "list"
    }))
    .expect("skill-manager arguments should parse");

    assert!(request.source.is_none());
}

/// Explicit layer arguments should be rejected because skill-manager is locked to USER.
/// 显式层级参数应被拒绝，因为 skill-manager 已固定到 USER。
#[test]
fn skill_manager_arguments_reject_layer() {
    let error = parse_skill_manager_tool_arguments(&json!({
        "action": "list",
        "layer": "ROOT"
    }))
    .expect_err("skill-manager should reject layer arguments");

    assert_eq!(error.0, -32602);
    assert!(error.1.contains("does not accept a layer parameter"));
}

/// USER selection should ignore other formal layers and return only the user root.
/// USER 选择应忽略其他正式层级，只返回用户根。
#[test]
fn skill_manager_user_selection_uses_only_user_layer() {
    let roots = vec![
        RuntimeSkillRoot {
            name: "ROOT".to_string(),
            skills_dir: PathBuf::from("D:/runtime/skills"),
        },
        RuntimeSkillRoot {
            name: "PROJECT".to_string(),
            skills_dir: PathBuf::from("D:/project/skills"),
        },
        RuntimeSkillRoot {
            name: "USER".to_string(),
            skills_dir: PathBuf::from("D:/user/skills"),
        },
    ];

    let user_root = select_skill_manager_user_root(&roots).expect("USER layer should resolve");

    assert_eq!(user_root.name, "USER");
}

/// Skill-manager uninstall should inject USER as the lifecycle target when ROOT shadows the same skill id.
/// 当 ROOT 遮蔽同名技能时，skill-manager 卸载应将 USER 注入为生命周期目标。
#[test]
fn skill_manager_uninstall_forces_user_target_when_root_shadows_skill() {
    let runtime_root = unique_test_dir("skill-manager-user-target");
    std::fs::create_dir_all(runtime_root.join("lua_runtime"))
        .expect("LuaSkills runtime root should be created");
    let root_layer = RuntimeSkillRoot {
        name: "ROOT".to_string(),
        skills_dir: runtime_root.join("root-space").join("skills"),
    };
    let user_layer = RuntimeSkillRoot {
        name: "USER".to_string(),
        skills_dir: runtime_root.join("user-space").join("skills"),
    };
    let skill_id = "user-shadow-skill";
    let root_skill_dir = write_minimal_skill_to_root(&root_layer.skills_dir, skill_id);
    let user_skill_dir = write_minimal_skill_to_root(&user_layer.skills_dir, skill_id);
    let config = Config {
        runtime_root: Some(runtime_root.to_string_lossy().to_string()),
        ..Config::default()
    };
    let server = HostRuntime::new()
        .with_lua_skills(
            &config,
            &[root_layer.clone(), user_layer.clone()],
            LuaVmPoolConfig {
                min_size: 1,
                max_size: 1,
                idle_ttl_secs: 60,
            },
            ToolCacheConfig::default(),
        )
        .expect("server should load shadowed skill roots");
    let _lifecycle_callback_guard = lock_luaskills_lifecycle_callback();
    let lifecycle_events = std::sync::Arc::new(std::sync::Mutex::new(Vec::<
        RuntimeSkillLifecycleEvent,
    >::new()));
    let lifecycle_events_callback = lifecycle_events.clone();
    set_skill_lifecycle_callback(Some(std::sync::Arc::new(
        move |event: &RuntimeSkillLifecycleEvent| {
            lifecycle_events_callback
                .lock()
                .expect("lifecycle events should not be poisoned")
                .push(event.clone());
        },
    )));
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime should build");

    let response = runtime
        .block_on(
            McpDispatcher::new(server.clone()).handle_message_with_context(
                &json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "tools/call",
                    "params": {
                        "name": "skill-manager",
                        "arguments": {
                            "action": "uninstall",
                            "skill_id": skill_id
                        }
                    }
                }),
                RequestContext::default(),
            ),
        )
        .expect("skill-manager uninstall should return one response");
    let tool_result: ToolCallResult = serde_json::from_value(
        response
            .get("result")
            .cloned()
            .expect("skill-manager uninstall should return result"),
    )
    .expect("skill-manager uninstall result should deserialize");
    let rendered = tool_result
        .content
        .first()
        .map(|item| item.text.clone())
        .unwrap_or_default();

    assert_eq!(tool_result.is_error, None);
    assert!(rendered.contains("- layer: USER"));
    assert!(
        root_skill_dir.exists(),
        "ROOT skill should remain untouched by USER-locked uninstall"
    );
    assert!(
        !user_skill_dir.exists(),
        "USER skill should be removed even when ROOT owns the effective skill id"
    );
    let observed_events = lifecycle_events
        .lock()
        .expect("lifecycle events should not be poisoned");
    assert!(
        observed_events.iter().any(|event| {
            event.plane == luaskills::SkillOperationPlane::Skills
                && event.root_name.as_deref() == Some("USER")
                && event.skill_id == skill_id
        }),
        "skill-manager USER target should execute through the ordinary Skills plane"
    );
    drop(observed_events);
    set_skill_lifecycle_callback(None);
    let _ = std::fs::remove_dir_all(&runtime_root);
}

/// Layer arguments should be rejected before lifecycle dispatch.
/// 层级参数应在生命周期分发前被拒绝。
#[test]
fn skill_manager_layer_parameter_returns_json_rpc_error() {
    let server = HostRuntime::new();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime should build");

    let response = runtime
        .block_on(
            McpDispatcher::new(server.clone()).handle_message_with_context(
                &json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "tools/call",
                    "params": {
                        "name": "skill-manager",
                        "arguments": {
                            "action": "install",
                            "layer": "ROOT",
                            "source": "LuaSkills/vulcan-codekit"
                        }
                    }
                }),
                RequestContext::default(),
            ),
        )
        .expect("skill-manager layer install should return one response");

    let message = response
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    assert!(message.contains("does not accept a layer parameter"));
}

#[test]
fn render_help_list_markdown_omits_runtime_metadata_fields() {
    let markdown = render_help_list_markdown(&[make_help_descriptor()]);

    assert!(markdown.contains("## `demo-skill`"));
    assert!(markdown.contains("- `main`: skill package description."));
    assert!(!markdown.contains("version:"));
    assert!(!markdown.contains("root:"));
    assert!(!markdown.contains("dir:"));
}

#[test]
fn render_help_list_markdown_labels_main_as_package_description() {
    let markdown = render_help_list_markdown(&[make_help_descriptor()]);

    assert!(markdown.contains(
        "- `main`: skill package description. Summarize the package-level capability surface."
    ));
    assert!(markdown.contains("- `search`: Search indexed project files."));
}

/// Minimal servers without a Lua engine should expose default host tools but hide Lua help wrappers.
/// 未加载 Lua 引擎的最小服务应暴露默认宿主工具，但隐藏 Lua help 包装工具。
#[test]
fn tools_list_hides_help_tools_when_lua_engine_is_unavailable() {
    let server = HostRuntime::new();
    let tool_names: HashSet<String> = server
        .list_runtime_tools()
        .expect("tool listing should succeed on minimal server")
        .into_iter()
        .map(|tool| tool.name)
        .collect();

    assert!(tool_names.contains("reload_vulcan_mcp_configs"));
    assert!(tool_names.contains("skill-manager"));
    assert!(!tool_names.contains("luaskill-config"));
    assert!(!tool_names.contains("vulcan-help-list"));
    assert!(!tool_names.contains("vulcan-help-detail"));
}

/// Servers with one resolved runtime skill-config file path should expose luaskill-config even before Lua engine initialization.
/// 具备已解析统一 Skill 配置文件路径的服务，即使尚未初始化 Lua 引擎，也应暴露 luaskill-config。
#[test]
fn tools_list_exposes_luaskill_config_when_host_path_is_available() {
    let root = unique_test_dir("luaskill-config-tools-list");
    let config_file_path = root.join("configs").join("skill_config.json");
    let server = HostRuntime::new()
        .with_runtime_skill_config_file_path(config_file_path)
        .expect("luaskill-config tool should register while runtime is uniquely owned");
    let tool_names: HashSet<String> = server
        .list_runtime_tools()
        .expect("tool listing should succeed after luaskill-config registration")
        .into_iter()
        .map(|tool| tool.name)
        .collect();

    assert!(tool_names.contains("luaskill-config"));
}

/// Builder-only mutation should fail explicitly after HostRuntime has been cloned.
/// HostRuntime 被克隆后，构建期专用变更应明确失败。
#[test]
fn builder_mutation_rejects_shared_runtime_after_clone() {
    let root = unique_test_dir("shared-builder-mutation");
    let config_file_path = root.join("configs").join("skill_config.json");
    let server = HostRuntime::new();
    let _shared_runtime = server.clone();

    let error = match server.with_runtime_skill_config_file_path(config_file_path) {
        Ok(_) => panic!("builder mutation should reject already shared runtime"),
        Err(error) => error,
    };

    assert!(
        error
            .to_string()
            .contains("requires a uniquely owned runtime")
    );
}

/// Lua help tools should become visible only after the Lua runtime capability has been registered explicitly.
/// Lua help 工具只应在显式注册了 Lua 运行时能力后才对外可见。
#[test]
fn register_lua_help_tools_exposes_help_tools_after_runtime_ready() {
    let mut server = HostRuntime::new();
    server
        .register_lua_help_tools()
        .expect("Lua help tools should register while runtime is uniquely owned");
    let tool_names: HashSet<String> = server
        .list_runtime_tools()
        .expect("tool listing should succeed after help registration")
        .into_iter()
        .map(|tool| tool.name)
        .collect();

    assert!(tool_names.contains("vulcan-help-list"));
    assert!(tool_names.contains("vulcan-help-detail"));
}

/// URL installs should fail with the wrapper's explicit unsupported-source message.
/// URL 安装应使用包装层明确的不支持来源提示失败。
#[test]
fn skill_manager_url_install_reports_not_implemented() {
    let server = HostRuntime::new();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime should build");

    let response = runtime
        .block_on(
            McpDispatcher::new(server.clone()).handle_message_with_context(
                &json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "tools/call",
                    "params": {
                        "name": "skill-manager",
                        "arguments": {
                            "action": "install",
                            "source_type": "url",
                            "source": "https://example.test/vulcan-codekit.source.yaml"
                        }
                    }
                }),
                RequestContext::default(),
            ),
        )
        .expect("skill-manager URL install should return one response");

    assert!(
        response.get("error").is_none(),
        "URL install should return a tool-level error, not JSON-RPC error: {response}"
    );
    let tool_result: ToolCallResult = serde_json::from_value(
        response
            .get("result")
            .cloned()
            .expect("skill-manager URL install should return result"),
    )
    .expect("skill-manager URL install result should deserialize");
    let rendered = tool_result
        .content
        .first()
        .map(|item| item.text.clone())
        .unwrap_or_default();

    assert_eq!(tool_result.is_error, Some(true));
    assert!(rendered.contains("managed URL install is not implemented yet"));
    assert!(!rendered.contains("requires skill_id"));
}

/// Luaskill-config should remain callable without any Lua engine because it now uses the standalone skill-config store directly.
/// luaskill-config 现在直接使用独立 Skill 配置存储，因此在没有 Lua 引擎时也应可调用。
#[test]
fn luaskill_config_tool_works_without_lua_engine() {
    let root = unique_test_dir("luaskill-config-without-engine");
    let config_file_path = root.join("configs").join("skill_config.json");
    let server = HostRuntime::new()
        .with_runtime_skill_config_file_path(config_file_path.clone())
        .expect("luaskill-config tool should register while runtime is uniquely owned");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime should build");

    let set_response = runtime
        .block_on(
            McpDispatcher::new(server.clone()).handle_message_with_context(
                &json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "tools/call",
                    "params": {
                        "name": "luaskill-config",
                        "arguments": {
                            "action": "set",
                            "skill_id": "demo-skill",
                            "key": "api_token",
                            "value": "sk-runtime"
                        }
                    }
                }),
                RequestContext::default(),
            ),
        )
        .expect("luaskill-config set should produce one response");
    assert!(
        set_response.get("error").is_none(),
        "unexpected luaskill-config error: {set_response}"
    );

    let persisted: Value = serde_json::from_str(
        &std::fs::read_to_string(&config_file_path)
            .expect("luaskill-config file should be created"),
    )
    .expect("persisted luaskill-config JSON should parse");
    assert_eq!(persisted["skills"]["demo-skill"]["api_token"], "sk-runtime");

    let get_response = runtime
        .block_on(
            McpDispatcher::new(server.clone()).handle_message_with_context(
                &json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "method": "tools/call",
                    "params": {
                        "name": "luaskill-config",
                        "arguments": {
                            "action": "get",
                            "skill_id": "demo-skill",
                            "key": "api_token"
                        }
                    }
                }),
                RequestContext::default(),
            ),
        )
        .expect("luaskill-config get should produce one response");
    let tool_result: ToolCallResult = serde_json::from_value(
        get_response
            .get("result")
            .cloned()
            .expect("luaskill-config get should return a result"),
    )
    .expect("luaskill-config get result should deserialize");
    let rendered = tool_result
        .content
        .first()
        .map(|item| item.text.clone())
        .unwrap_or_default();

    assert!(rendered.contains("skill_id: demo-skill"));
    assert!(rendered.contains("- api_token = \"sk-runtime\""));
    assert!(!rendered.contains("```json"));
    assert!(!rendered.contains("skill_config.json"));
    let _ = std::fs::remove_dir_all(&root);
}

/// MCP tools/call should treat an omitted arguments field as an empty JSON object before host-tool parsing.
/// MCP tools/call 应在宿主工具解析前把省略的 arguments 字段视为空 JSON 对象。
#[test]
fn tools_call_missing_arguments_uses_empty_object_contract() {
    let root = unique_test_dir("tools-call-missing-arguments");
    let config_file_path = root.join("configs").join("skill_config.json");
    let server = HostRuntime::new()
        .with_runtime_skill_config_file_path(config_file_path)
        .expect("luaskill-config tool should register while runtime is uniquely owned");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime should build");

    let response = runtime
        .block_on(McpDispatcher::new(server).handle_message_with_context(
            &json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": {
                    "name": "luaskill-config"
                }
            }),
            RequestContext::default(),
        ))
        .expect("tools/call without arguments should return one response");
    let error = response
        .get("error")
        .expect("missing luaskill-config action should return a JSON-RPC error");
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .expect("error should contain a message");

    assert_eq!(error.get("code").and_then(Value::as_i64), Some(-32602));
    assert!(message.contains("missing field `action`"));
    assert!(!message.contains("invalid type: null"));
    let _ = std::fs::remove_dir_all(&root);
}

/// Empty luaskill-config listings should explicitly report that no configuration exists yet.
/// 空的 luaskill-config 列表结果应明确提示当前还没有任何配置。
#[test]
fn luaskill_config_list_reports_empty_state() {
    let root = unique_test_dir("luaskill-config-empty-list");
    let config_file_path = root.join("configs").join("skill_config.json");
    let server = HostRuntime::new()
        .with_runtime_skill_config_file_path(config_file_path)
        .expect("luaskill-config tool should register while runtime is uniquely owned");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime should build");

    let response = runtime
        .block_on(
            McpDispatcher::new(server.clone()).handle_message_with_context(
                &json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "tools/call",
                    "params": {
                        "name": "luaskill-config",
                        "arguments": {
                            "action": "list"
                        }
                    }
                }),
                RequestContext::default(),
            ),
        )
        .expect("luaskill-config list should produce one response");
    let tool_result: ToolCallResult = serde_json::from_value(
        response
            .get("result")
            .cloned()
            .expect("luaskill-config list should return a result"),
    )
    .expect("luaskill-config list result should deserialize");
    let rendered = tool_result
        .content
        .first()
        .map(|item| item.text.clone())
        .unwrap_or_default();

    assert_eq!(rendered, "No luaskill configuration is currently set.");
    assert!(!rendered.contains("skill_config.json"));
    let _ = std::fs::remove_dir_all(&root);
}

/// Non-empty luaskill-config listings should group entries by skill id and show stable key-value lines.
/// 非空的 luaskill-config 列表结果应按 skill_id 分组并稳定展示键值行。
#[test]
fn luaskill_config_list_groups_entries_by_skill_id() {
    let root = unique_test_dir("luaskill-config-grouped-list");
    let config_file_path = root.join("configs").join("skill_config.json");
    let server = HostRuntime::new()
        .with_runtime_skill_config_file_path(config_file_path)
        .expect("luaskill-config tool should register while runtime is uniquely owned");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime should build");

    for (skill_id, key, value) in [
        ("alpha-skill", "endpoint", "https://api.example.com"),
        ("alpha-skill", "token", "sk-alpha"),
        ("beta-skill", "region", "cn-sh"),
    ] {
        runtime
            .block_on(
                McpDispatcher::new(server.clone()).handle_message_with_context(
                    &json!({
                        "jsonrpc": "2.0",
                        "id": 1,
                        "method": "tools/call",
                        "params": {
                            "name": "luaskill-config",
                            "arguments": {
                                "action": "set",
                                "skill_id": skill_id,
                                "key": key,
                                "value": value
                            }
                        }
                    }),
                    RequestContext::default(),
                ),
            )
            .expect("luaskill-config set should succeed for grouped list setup");
    }

    let response = runtime
        .block_on(
            McpDispatcher::new(server.clone()).handle_message_with_context(
                &json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "method": "tools/call",
                    "params": {
                        "name": "luaskill-config",
                        "arguments": {
                            "action": "list"
                        }
                    }
                }),
                RequestContext::default(),
            ),
        )
        .expect("luaskill-config list should produce one response");
    let tool_result: ToolCallResult = serde_json::from_value(
        response
            .get("result")
            .cloned()
            .expect("luaskill-config grouped list should return a result"),
    )
    .expect("luaskill-config grouped list result should deserialize");
    let rendered = tool_result
        .content
        .first()
        .map(|item| item.text.clone())
        .unwrap_or_default();

    assert!(rendered.contains("Found 2 luaskill configuration namespaces:"));
    assert!(rendered.contains("skill_id: alpha-skill"));
    assert!(rendered.contains("- endpoint = \"https://api.example.com\""));
    assert!(rendered.contains("- token = \"sk-alpha\""));
    assert!(rendered.contains("skill_id: beta-skill"));
    assert!(rendered.contains("- region = \"cn-sh\""));
    assert!(!rendered.contains("```json"));
    assert!(!rendered.contains("skill_config.json"));
    let _ = std::fs::remove_dir_all(&root);
}

/// MCP initialize should advertise only the retained tool surface for a plain host runtime.
/// 普通宿主运行时的 MCP initialize 只应声明保留的工具能力面。
#[test]
fn initialize_advertises_dynamic_tools_without_prompt_or_resource_surfaces() {
    let server = HostRuntime::new();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime should build");

    let response = runtime
        .block_on(McpDispatcher::new(server).handle_message_with_context(
            &json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2025-11-25",
                    "capabilities": {},
                    "clientInfo": {
                        "name": "unit-test-client",
                        "version": "1.0.0"
                    }
                }
            }),
            RequestContext::default(),
        ))
        .expect("initialize should produce one response");
    let capabilities = response
        .get("result")
        .and_then(|result| result.get("capabilities"))
        .expect("initialize result should include capabilities");

    assert_eq!(
        capabilities
            .get("tools")
            .and_then(|tools| tools.get("listChanged"))
            .and_then(Value::as_bool),
        Some(false)
    );
    assert!(capabilities.get("prompts").is_none());
    assert!(capabilities.get("resources").is_none());
    assert!(capabilities.get("resourceTemplates").is_none());
    assert!(capabilities.get("sampling").is_none());
    assert!(capabilities.get("logging").is_none());
    assert!(capabilities.get("completions").is_none());
}

/// MCP prompt, resource, and completion methods should stay closed while the host does not implement those surfaces.
/// 宿主未实现 prompts、resources 与 completions 能力面时，对应 MCP 方法应保持关闭。
#[test]
fn prompt_resource_and_completion_methods_are_not_supported() {
    let server = HostRuntime::new();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime should build");

    for method in [
        "prompts/list",
        "prompts/get",
        "resources/list",
        "resources/read",
        "resources/templates/list",
        "completion/complete",
    ] {
        let response = runtime
            .block_on(
                McpDispatcher::new(server.clone()).handle_message_with_context(
                    &json!({
                        "jsonrpc": "2.0",
                        "id": method,
                        "method": method,
                        "params": {}
                    }),
                    RequestContext::default(),
                ),
            )
            .expect(
                "unsupported prompt/resource/completion method should produce an error response",
            );
        let error = response
            .get("error")
            .expect("unsupported prompt/resource/completion method should return an error");

        assert_eq!(error.get("code").and_then(Value::as_i64), Some(-32601));
        assert!(
            error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .contains("Method not found")
        );
    }
}
