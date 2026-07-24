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
use std::path::PathBuf;

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

/// Write one enabled LuaSkill fixture with typed package configuration declarations.
/// 写入一个带类型化技能包配置声明的已启用 LuaSkill 夹具。
/// Parameters `skill_root` and `skill_id` select the package directory and identifier.
/// 参数：`skill_root` 和 `skill_id` 指定技能包目录与标识符。
/// Returns the concrete package directory.
/// 返回具体的技能包目录。
fn write_configurable_skill_to_root(skill_root: &std::path::Path, skill_id: &str) -> PathBuf {
    let skill_dir = skill_root.join(skill_id);
    std::fs::create_dir_all(skill_dir.join("runtime"))
        .expect("configurable skill runtime directory should be created");
    std::fs::write(
        skill_dir.join("skill.yaml"),
        format!(
            "name: {skill_id}\nversion: 0.1.0\nenable: true\ndebug: false\nconfig:\n  - key: api_token\n    type: string\n    required: true\n    sensitive: true\n    description: Service access token.\n    constraints:\n      min_length: 1\n      max_length: 4096\n  - key: retries\n    type: integer\n    default: 2\n    description: Retry count.\n    constraints:\n      minimum: 0\n      maximum: 10\n  - key: temperature\n    type: float\n    description: Sampling temperature.\n    constraints:\n      minimum: 0.0\n      maximum: 2.0\n  - key: provider\n    type: enum\n    description: Service provider.\n    options:\n      - value: openai\n        label: OpenAI\n        description: OpenAI service.\n      - value: local\n        label: Local\n        description: Local service.\n  - key: feature_enabled\n    type: boolean\n    description: Feature switch.\nentries:\n  - name: ping\n    description: Config ping entry.\n    lua_entry: runtime/ping.lua\n    lua_module: {skill_id}.ping\n"
        ),
    )
    .expect("configurable skill manifest should be written");
    std::fs::write(
        skill_dir.join("runtime").join("ping.lua"),
        "return function(args)\n  return 'ok'\nend\n",
    )
    .expect("configurable skill runtime entry should be written");
    skill_dir
}

/// Build one initialized HostRuntime fixture whose runtime-config tool can access one declared package.
/// 构建一个已初始化 HostRuntime 夹具，使 runtime-config 工具可访问一个已声明配置的技能包。
/// Parameters `name`, `root_name`, and `skill_id` select the isolated test path and package layer.
/// 参数：`name`、`root_name` 和 `skill_id` 指定隔离测试路径与技能包层级。
/// Returns the fixture root, configuration root, and initialized runtime.
/// 返回夹具根目录、配置根目录和已初始化运行时。
fn build_runtime_config_test_server(
    name: &str,
    root_name: &str,
    skill_id: &str,
) -> (PathBuf, PathBuf, HostRuntime) {
    let fixture_root = unique_test_dir(name);
    let application_root = fixture_root.join("application");
    std::fs::create_dir_all(application_root.join("lua_runtime"))
        .expect("LuaSkills runtime root should be created");
    let skill_root = RuntimeSkillRoot {
        name: root_name.to_string(),
        skills_dir: fixture_root.join("skill-root").join("skills"),
    };
    write_configurable_skill_to_root(&skill_root.skills_dir, skill_id);
    // LuaSkills requires a formal ROOT layer even when the configurable package belongs to USER or PROJECT.
    // 即使可配置技能包属于 USER 或 PROJECT，LuaSkills 也要求正式根链包含 ROOT 层。
    let skill_roots = if root_name.eq_ignore_ascii_case("ROOT") {
        vec![skill_root]
    } else {
        let root_layer = RuntimeSkillRoot {
            name: "ROOT".to_string(),
            skills_dir: fixture_root.join("root-layer").join("skills"),
        };
        std::fs::create_dir_all(&root_layer.skills_dir)
            .expect("formal ROOT skills directory should be created");
        vec![root_layer, skill_root]
    };
    let skill_config_root = fixture_root.join("skill-config");
    let config = Config {
        runtime_root: Some(application_root.to_string_lossy().to_string()),
        skill_config_root: Some(skill_config_root.to_string_lossy().to_string()),
        ..Config::default()
    };
    let server = HostRuntime::new()
        .with_lua_skills(
            &config,
            &skill_roots,
            LuaVmPoolConfig {
                min_size: 1,
                max_size: 1,
                idle_ttl_secs: 60,
            },
            ToolCacheConfig::default(),
        )
        .expect("runtime-config test server should initialize");
    (fixture_root, skill_config_root, server)
}

/// Invoke runtime-config through the MCP dispatcher and decode its stable JSON response envelope.
/// 通过 MCP 分发器调用 runtime-config，并解码其稳定 JSON 响应包络。
/// Parameters select the Tokio runtime, host runtime, JSON-RPC id, and optional tool arguments.
/// 参数：指定 Tokio 运行时、宿主运行时、JSON-RPC 标识及可选工具参数。
/// Returns the decoded RuntimeSkillConfigToolResponse JSON value.
/// 返回解码后的 RuntimeSkillConfigToolResponse JSON 值。
fn call_runtime_config(
    runtime: &tokio::runtime::Runtime,
    server: &HostRuntime,
    request_id: u64,
    arguments: Option<Value>,
) -> Value {
    let mut params = serde_json::Map::from_iter([(
        "name".to_string(),
        Value::String("runtime-config".to_string()),
    )]);
    if let Some(arguments) = arguments {
        params.insert("arguments".to_string(), arguments);
    }
    let response = runtime
        .block_on(
            McpDispatcher::new(server.clone()).handle_message_with_context(
                &json!({
                    "jsonrpc": "2.0",
                    "id": request_id,
                    "method": "tools/call",
                    "params": Value::Object(params)
                }),
                RequestContext::default(),
            ),
        )
        .expect("runtime-config should produce one JSON-RPC response");
    assert!(
        response.get("error").is_none(),
        "runtime-config transport should return a tool result: {response}"
    );
    let tool_result: ToolCallResult = serde_json::from_value(
        response
            .get("result")
            .cloned()
            .expect("runtime-config should return a tool result"),
    )
    .expect("runtime-config tool result should deserialize");
    let response_text = tool_result
        .content
        .first()
        .map(|item| item.text.as_str())
        .expect("runtime-config should return one text content block");
    let decoded: Value = serde_json::from_str(response_text)
        .expect("runtime-config text content should contain the stable JSON envelope");
    let response_ok = decoded["ok"]
        .as_bool()
        .expect("runtime-config envelope should contain a boolean ok field");
    let expected_tool_error = (!response_ok).then_some(true);
    assert_eq!(tool_result.is_error, expected_tool_error);
    decoded
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
        skill_config_root: Some(
            runtime_root
                .join("skill-config")
                .to_string_lossy()
                .to_string(),
        ),
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
    assert!(!tool_names.contains("runtime-config"));
    assert!(!tool_names.contains("vulcan-help-list"));
    assert!(!tool_names.contains("vulcan-help-detail"));
}

/// Servers should expose runtime-config only after the LuaSkills engine and declarations are ready.
/// 服务只应在 LuaSkills 引擎和声明就绪后暴露 runtime-config。
#[test]
fn tools_list_exposes_runtime_config_after_lua_engine_initialization() {
    let (fixture_root, _skill_config_root, server) =
        build_runtime_config_test_server("runtime-config-tools-list", "USER", "config-tool-skill");
    let tools = server
        .list_runtime_tools()
        .expect("tool listing should succeed after runtime-config registration");
    let runtime_config = tools
        .iter()
        .find(|tool| tool.name == "runtime-config")
        .expect("runtime-config should be registered");
    let annotations = runtime_config
        .annotations
        .as_ref()
        .expect("runtime-config should declare security annotations");
    let properties = runtime_config
        .input_schema
        .properties
        .as_ref()
        .expect("runtime-config should expose an input schema");

    assert_eq!(annotations.user_confirmation_required, Some(true));
    assert_eq!(annotations.read_only_hint, Some(false));
    assert_eq!(annotations.destructive_hint, Some(true));
    assert_eq!(
        properties["store_scope"]["enum"],
        json!(["skills", "system-skills"])
    );
    drop(server);
    let _ = std::fs::remove_dir_all(&fixture_root);
}

/// Builder-only mutation should fail explicitly after HostRuntime has been cloned.
/// HostRuntime 被克隆后，构建期专用变更应明确失败。
#[test]
fn builder_mutation_rejects_shared_runtime_after_clone() {
    let mut server = HostRuntime::new();
    let _shared_runtime = server.clone();

    let error = server
        .register_lua_help_tools()
        .expect_err("builder mutation should reject already shared runtime");

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

/// Missing runtime-config arguments should stay inside the stable upstream tool envelope.
/// runtime-config 缺少参数时应继续使用稳定的上游工具包络返回错误。
#[test]
fn runtime_config_missing_action_returns_stable_tool_error() {
    let (fixture_root, _skill_config_root, server) = build_runtime_config_test_server(
        "runtime-config-missing-action",
        "USER",
        "config-missing-action-skill",
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime should build");

    let response = call_runtime_config(&runtime, &server, 1, None);

    assert_eq!(response["ok"], false);
    assert!(response["action"].is_null());
    assert_eq!(response["error"]["code"], "CONFIG_DECLARATION_INVALID");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("missing field `action`"))
    );
    drop(server);
    let _ = std::fs::remove_dir_all(&fixture_root);
}

/// The canonical dispatcher should support every declared action, typed batch writes, revisions, and CAS.
/// 标准分发器应支持全部声明动作、类型化批量写入、修订号和 CAS。
#[test]
fn runtime_config_dispatcher_supports_full_declared_config_lifecycle() {
    let skill_id = "config-lifecycle-skill";
    let (fixture_root, skill_config_root, server) =
        build_runtime_config_test_server("runtime-config-lifecycle", "USER", skill_id);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime should build");

    let describe = call_runtime_config(
        &runtime,
        &server,
        1,
        Some(json!({
            "action": "describe",
            "skill_id": skill_id,
            "mode": "effective",
            "include_values": false
        })),
    );
    assert_eq!(describe["ok"], true);
    let descriptors = describe["result"]
        .as_array()
        .expect("describe should return an array");
    assert_eq!(descriptors.len(), 1);
    let descriptor = &descriptors[0];
    assert_eq!(descriptor["skill_id"], skill_id);
    assert_eq!(descriptor["complete"], false);
    assert_eq!(descriptor["revision"], "0");
    assert_eq!(descriptor["store_scope"], "skills");
    let retry_item = descriptor["items"]
        .as_array()
        .expect("describe should return declared items")
        .iter()
        .find(|item| item["key"] == "retries")
        .expect("describe should retain the retries declaration");
    assert_eq!(retry_item["type"], "integer");
    assert_eq!(retry_item["default"], 2);
    assert_eq!(retry_item["constraints"]["minimum"], 0);
    assert_eq!(retry_item["constraints"]["maximum"], 10);
    let token_item = descriptor["items"]
        .as_array()
        .expect("describe should return declared items")
        .iter()
        .find(|item| item["key"] == "api_token")
        .expect("describe should retain the sensitive token declaration");
    assert_eq!(token_item["sensitive"], true);
    assert!(token_item.get("value").is_none());

    let validate = call_runtime_config(
        &runtime,
        &server,
        2,
        Some(json!({"action": "validate", "skill_id": skill_id})),
    );
    assert_eq!(validate["ok"], true);
    assert_eq!(validate["result"]["complete"], false);
    assert_eq!(validate["result"]["revision"], "0");
    assert_eq!(validate["result"]["store_scope"], "skills");
    assert_eq!(
        validate["result"]["missing"]
            .as_array()
            .expect("validate should report missing declarations")
            .len(),
        1
    );
    assert!(validate["result"]["invalid"].as_array().is_some());
    assert!(validate["result"]["business_issues"].as_array().is_some());
    assert!(validate["result"]["orphaned"].as_array().is_some());

    let set = call_runtime_config(
        &runtime,
        &server,
        3,
        Some(json!({
            "action": "set",
            "skill_id": skill_id,
            "values": {
                "api_token": "sk-runtime",
                "retries": 3,
                "temperature": 0.7,
                "provider": "local",
                "feature_enabled": true
            }
        })),
    );
    assert_eq!(set["ok"], true);
    assert_eq!(set["result"]["changed"], true);
    assert_eq!(set["result"]["values"]["retries"], "3");
    assert_eq!(set["result"]["values"]["temperature"], "0.7");
    assert_eq!(set["result"]["values"]["provider"], "local");
    assert_eq!(set["result"]["values"]["feature_enabled"], "true");
    let committed_revision = set["result"]["revision"]
        .as_str()
        .expect("set should return a canonical revision")
        .to_string();

    let persisted_path = skill_config_root.join("skills").join("config.json");
    let persisted: Value = serde_json::from_str(
        &std::fs::read_to_string(&persisted_path)
            .expect("versioned ordinary skill config should be created"),
    )
    .expect("versioned ordinary skill config should parse");
    assert_eq!(persisted["format_version"], 1);
    assert_eq!(persisted["revision"], committed_revision);
    assert_eq!(persisted["skills"][skill_id]["api_token"], "sk-runtime");

    let get = call_runtime_config(
        &runtime,
        &server,
        4,
        Some(json!({
            "action": "get",
            "skill_id": skill_id,
            "key": "api_token"
        })),
    );
    assert_eq!(get["ok"], true);
    assert_eq!(get["result"]["found"], true);
    assert_eq!(get["result"]["value"], "sk-runtime");

    let list = call_runtime_config(
        &runtime,
        &server,
        5,
        Some(json!({"action": "list", "skill_id": skill_id})),
    );
    assert_eq!(list["ok"], true);
    assert_eq!(
        list["result"]
            .as_array()
            .expect("list result should be an array")
            .len(),
        5
    );

    let rejected_batch = call_runtime_config(
        &runtime,
        &server,
        6,
        Some(json!({
            "action": "set",
            "skill_id": skill_id,
            "values": {
                "retries": 5,
                "undeclared_key": "must-fail"
            },
            "expected_revision": committed_revision
        })),
    );
    assert_eq!(rejected_batch["ok"], false);
    assert_eq!(rejected_batch["error"]["code"], "CONFIG_KEY_UNDECLARED");
    let unchanged = call_runtime_config(
        &runtime,
        &server,
        7,
        Some(json!({
            "action": "get",
            "skill_id": skill_id,
            "key": "retries"
        })),
    );
    assert_eq!(unchanged["result"]["value"], "3");

    let cas_set = call_runtime_config(
        &runtime,
        &server,
        8,
        Some(json!({
            "action": "set",
            "skill_id": skill_id,
            "key": "retries",
            "value": 4,
            "expected_revision": committed_revision
        })),
    );
    assert_eq!(cas_set["ok"], true);
    let cas_revision = cas_set["result"]["revision"]
        .as_str()
        .expect("successful CAS set should return a revision")
        .to_string();
    let committed_revision_number = committed_revision
        .parse::<u64>()
        .expect("committed revision should be a canonical decimal string");
    let cas_revision_number = cas_revision
        .parse::<u64>()
        .expect("CAS revision should be a canonical decimal string");
    assert!(cas_revision_number > committed_revision_number);

    let conflict = call_runtime_config(
        &runtime,
        &server,
        9,
        Some(json!({
            "action": "set",
            "skill_id": skill_id,
            "key": "retries",
            "value": 5,
            "expected_revision": committed_revision
        })),
    );
    assert_eq!(conflict["ok"], false);
    assert_eq!(conflict["error"]["code"], "CONFIG_REVISION_CONFLICT");

    let unrelated_field = call_runtime_config(
        &runtime,
        &server,
        10,
        Some(json!({
            "action": "get",
            "skill_id": skill_id,
            "key": "api_token",
            "store_scope": "skills"
        })),
    );
    assert_eq!(unrelated_field["ok"], false);
    assert_eq!(
        unrelated_field["error"]["code"],
        "CONFIG_BATCH_ARGUMENT_CONFLICT"
    );

    let unknown_field = call_runtime_config(
        &runtime,
        &server,
        11,
        Some(json!({
            "action": "describe",
            "skill_id": skill_id,
            "unexpected_field": true
        })),
    );
    assert_eq!(unknown_field["ok"], false);
    assert_eq!(unknown_field["error"]["code"], "CONFIG_DECLARATION_INVALID");

    let redacted_describe = call_runtime_config(
        &runtime,
        &server,
        12,
        Some(json!({
            "action": "describe",
            "skill_id": skill_id
        })),
    );
    assert_eq!(redacted_describe["ok"], true);
    assert!(!redacted_describe.to_string().contains("sk-runtime"));

    let delete = call_runtime_config(
        &runtime,
        &server,
        13,
        Some(json!({
            "action": "delete",
            "skill_id": skill_id,
            "key": "feature_enabled",
            "expected_revision": cas_revision
        })),
    );
    assert_eq!(delete["ok"], true);
    assert_eq!(delete["result"]["deleted"], true);
    let delete_revision_number = delete["result"]["revision"]
        .as_str()
        .expect("delete should return a canonical revision")
        .parse::<u64>()
        .expect("delete revision should be a canonical decimal string");
    assert!(delete_revision_number > cas_revision_number);

    let refresh = call_runtime_config(
        &runtime,
        &server,
        14,
        Some(json!({"action": "refresh", "store_scope": "skills"})),
    );
    assert_eq!(refresh["ok"], true);
    assert!(refresh["result"].is_array());
    let refresh_all =
        call_runtime_config(&runtime, &server, 15, Some(json!({"action": "refresh"})));
    assert_eq!(refresh_all["ok"], true);
    assert_eq!(
        refresh_all["result"]
            .as_array()
            .expect("all-store refresh should return an array")
            .len(),
        2
    );

    drop(server);
    let _ = std::fs::remove_dir_all(&fixture_root);
}

/// ROOT package writes should route only to the dedicated system-skills store.
/// ROOT 技能包写入应只路由到专用的 system-skills 存储。
#[test]
fn runtime_config_routes_root_packages_to_system_store() {
    let skill_id = "root-config-skill";
    let (fixture_root, skill_config_root, server) =
        build_runtime_config_test_server("runtime-config-root-store", "ROOT", skill_id);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime should build");

    let response = call_runtime_config(
        &runtime,
        &server,
        1,
        Some(json!({
            "action": "set",
            "skill_id": skill_id,
            "key": "api_token",
            "value": "root-secret"
        })),
    );

    assert_eq!(response["ok"], true);
    assert!(
        skill_config_root
            .join("system-skills")
            .join("config.json")
            .is_file()
    );
    assert!(
        !skill_config_root
            .join("skills")
            .join("config.json")
            .is_file()
    );
    drop(server);
    let _ = std::fs::remove_dir_all(&fixture_root);
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
