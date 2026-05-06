use super::*;
use std::sync::{Mutex, OnceLock};

/// Return one shared mutex used to serialize PATH-dependent tests.
/// 返回一个共享互斥锁，用于串行化依赖 PATH 的测试。
fn environment_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Build one unique temporary directory path for one main-module test case.
/// 为 main 模块单个测试用例构建唯一临时目录路径。
fn unique_test_dir(name: &str) -> std::path::PathBuf {
    let unique = format!(
        "vulcan-agent-service-main-{}-{}-{}",
        name,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    );
    std::env::temp_dir().join(unique)
}

/// Write one minimal ROOT skill directory used by local CLI tests.
/// 写入一个供本地 CLI 测试使用的最小 ROOT 技能目录。
fn write_minimal_root_skill(skill_root: &std::path::Path, skill_id: &str) {
    // Create the skill directory before writing the manifest.
    // 写入清单前先创建技能目录。
    let skill_dir = skill_root.join(skill_id);
    std::fs::create_dir_all(&skill_dir).expect("skill directory should be created");
    std::fs::write(
        skill_dir.join("skill.yaml"),
        format!("name: {skill_id}\nversion: 0.1.0\nenable: true\ndebug: false\nentries: []\n"),
    )
    .expect("skill manifest should be written");
}

/// Write one managed install record under the ROOT lifecycle state directory.
/// 在 ROOT 生命周期状态目录下写入一条受管安装记录。
fn write_root_install_record(runtime_root: &std::path::Path, skill_id: &str) {
    // Match the lifecycle layout derived by build_root_skill_manager_for_cli.
    // 匹配 build_root_skill_manager_for_cli 推导出的生命周期布局。
    let install_record_root = runtime_root.join("state").join("installs");
    std::fs::create_dir_all(&install_record_root)
        .expect("install record directory should be created");
    // Persist a GitHub-managed record so update-all considers the skill updateable.
    // 持久化 GitHub 受管记录，使全量更新认为该技能可更新。
    let record = luaskills::InstalledSkillRecord {
        skill_id: skill_id.to_string(),
        version: "0.1.0".to_string(),
        managed: true,
        source: luaskills::InstalledSkillSourceRecord {
            source_type: SkillInstallSourceType::Github,
            locator: format!("LuaSkills/{skill_id}"),
            tag: Some("v0.1.0".to_string()),
        },
        installed_at_unix_ms: 1,
    };
    std::fs::write(
        install_record_root.join(format!("{skill_id}.yaml")),
        serde_yaml::to_string(&record).expect("record should serialize"),
    )
    .expect("install record should be written");
}

/// Call-tools mode should accept --runtime-root so isolated runtime validation can use the same CLI entrypoint.
/// call-tools 模式应当接受 --runtime-root，以便隔离运行根验证复用同一 CLI 入口。
#[test]
fn parse_runtime_mode_allows_runtime_root_in_call_tools_mode() {
    let args = vec![
        "vulcan-agent-service.exe".to_string(),
        "--call-tools".to_string(),
        "demo-tool".to_string(),
        "--runtime-root".to_string(),
        "runtime".to_string(),
        "{\"ok\":true}".to_string(),
    ];
    let mode = parse_runtime_mode_from_args(&args).expect("call-tools mode should parse");
    match mode {
        RuntimeMode::Stdio => {}
        RuntimeMode::CallTool {
            tool_name,
            arguments,
            simulated_client_name,
        } => {
            assert_eq!(tool_name, "demo-tool");
            assert_eq!(arguments, json!({ "ok": true }));
            assert_eq!(simulated_client_name, DEFAULT_CALL_TOOL_CLIENT_NAME);
        }
        RuntimeMode::Serve
        | RuntimeMode::RootSkillInstall { .. }
        | RuntimeMode::RootSkillsUpdate
        | RuntimeMode::InternalLuaexecRequest { .. } => {
            panic!("expected call-tools runtime mode");
        }
    }
}

/// Stdio mode should be selectable directly so MCP can run over stdin/stdout without opening ports.
/// stdio 模式应可被直接选中，以便 MCP 通过标准输入输出运行而无需打开端口。
#[test]
fn parse_runtime_mode_accepts_stdio_mode() {
    let args = vec!["vulcan-agent-service.exe".to_string(), "--stdio".to_string()];
    let mode = parse_runtime_mode_from_args(&args).expect("stdio mode should parse");
    match mode {
        RuntimeMode::Stdio => {}
        RuntimeMode::Serve
        | RuntimeMode::CallTool { .. }
        | RuntimeMode::RootSkillInstall { .. }
        | RuntimeMode::RootSkillsUpdate
        | RuntimeMode::InternalLuaexecRequest { .. } => {
            panic!("expected stdio runtime mode");
        }
    }
}

/// ROOT install mode should parse as a local command instead of falling through to service mode.
/// ROOT 安装模式应解析为本地命令，而不是落回服务模式。
#[test]
fn parse_runtime_mode_accepts_root_install_mode() {
    let args = vec![
        "vulcan-agent-service.exe".to_string(),
        "--install-root-skill".to_string(),
        "LuaSkills/vulcan-codekit".to_string(),
        "--source-type".to_string(),
        "github".to_string(),
        "--runtime-root".to_string(),
        "output".to_string(),
    ];
    let mode = parse_runtime_mode_from_args(&args).expect("root install mode should parse");
    match mode {
        RuntimeMode::RootSkillInstall {
            source,
            source_type,
        } => {
            assert_eq!(source, "LuaSkills/vulcan-codekit");
            assert_eq!(source_type, Some(SkillInstallSourceType::Github));
        }
        RuntimeMode::Serve
        | RuntimeMode::Stdio
        | RuntimeMode::CallTool { .. }
        | RuntimeMode::RootSkillsUpdate
        | RuntimeMode::InternalLuaexecRequest { .. } => {
            panic!("expected root skill install runtime mode");
        }
    }
}

/// ROOT update-all mode should parse as a local command that does not start transports.
/// ROOT 全量更新模式应解析为不启动传输服务的本地命令。
#[test]
fn parse_runtime_mode_accepts_root_update_mode() {
    let args = vec![
        "vulcan-agent-service.exe".to_string(),
        "--update-root-skills".to_string(),
        "--runtime-root=output".to_string(),
    ];
    let mode = parse_runtime_mode_from_args(&args).expect("root update mode should parse");
    match mode {
        RuntimeMode::RootSkillsUpdate => {}
        RuntimeMode::Serve
        | RuntimeMode::Stdio
        | RuntimeMode::CallTool { .. }
        | RuntimeMode::RootSkillInstall { .. }
        | RuntimeMode::InternalLuaexecRequest { .. } => {
            panic!("expected root skills update runtime mode");
        }
    }
}

/// Call-tools mode should reject runtime-root flags that do not carry a concrete value.
/// call-tools 模式应拒绝未携带实际取值的 runtime-root 标志。
#[test]
fn parse_runtime_mode_rejects_missing_runtime_root_value_in_call_tools_mode() {
    let args = vec![
        "vulcan-agent-service.exe".to_string(),
        "--call-tools".to_string(),
        "demo-tool".to_string(),
        "--runtime-root".to_string(),
        "-config".to_string(),
    ];
    let error = match parse_runtime_mode_from_args(&args) {
        Ok(_) => panic!("missing value should fail"),
        Err(error) => error,
    };
    assert!(
        error
            .to_string()
            .contains("--runtime-root requires a value"),
        "unexpected error: {error}"
    );
}

/// Call-tools mode should reject the removed legacy config flag and redirect callers to runtime-root based config discovery.
/// call-tools 模式应拒绝已移除的历史 config 标志，并引导调用方改用基于 runtime-root 的配置发现。
#[test]
fn parse_runtime_mode_rejects_legacy_config_flag_in_call_tools_mode() {
    let args = vec![
        "vulcan-agent-service.exe".to_string(),
        "--call-tools".to_string(),
        "demo-tool".to_string(),
        "--config".to_string(),
        "runtime/configs/config.yaml".to_string(),
    ];
    let error = match parse_runtime_mode_from_args(&args) {
        Ok(_) => panic!("legacy config flag should fail"),
        Err(error) => error,
    };
    assert!(
        error.to_string().contains("Unsupported CLI flag"),
        "unexpected error: {error}"
    );
}

/// Call-tools mode should reject inline `--config=...` forms too so the removed config entrypoint is blocked consistently.
/// call-tools 模式也应拒绝内联 `--config=...` 形式，保证已移除的配置入口被一致封死。
#[test]
fn parse_runtime_mode_rejects_inline_legacy_config_flag_in_call_tools_mode() {
    let args = vec![
        "vulcan-agent-service.exe".to_string(),
        "--call-tools".to_string(),
        "demo-tool".to_string(),
        "--config=runtime/configs/config.yaml".to_string(),
    ];
    let error = match parse_runtime_mode_from_args(&args) {
        Ok(_) => panic!("inline legacy config flag should fail"),
        Err(error) => error,
    };
    assert!(
        error.to_string().contains("Unsupported CLI flag"),
        "unexpected error: {error}"
    );
}

/// Call-tools mode should accept inline `--runtime-root=...` forms so local debug CLI behavior matches the main config loader.
/// call-tools 模式应接受内联 `--runtime-root=...` 形式，从而让本地调试 CLI 行为与主配置加载器保持一致。
#[test]
fn parse_runtime_mode_allows_inline_runtime_root_in_call_tools_mode() {
    let args = vec![
        "vulcan-agent-service.exe".to_string(),
        "--call-tools".to_string(),
        "demo-tool".to_string(),
        "--runtime-root=output".to_string(),
    ];
    let mode = parse_runtime_mode_from_args(&args).expect("inline runtime-root should parse");
    match mode {
        RuntimeMode::CallTool { tool_name, .. } => {
            assert_eq!(tool_name, "demo-tool");
        }
        RuntimeMode::Serve
        | RuntimeMode::Stdio
        | RuntimeMode::RootSkillInstall { .. }
        | RuntimeMode::RootSkillsUpdate
        | RuntimeMode::InternalLuaexecRequest { .. } => {
            panic!("expected call-tools runtime mode");
        }
    }
}

/// Call-tools mode should reject inline `--runtime-root=` forms when the value is empty.
/// call-tools 模式在内联 `--runtime-root=` 取值为空时应拒绝调用。
#[test]
fn parse_runtime_mode_rejects_empty_inline_runtime_root_value() {
    let args = vec![
        "vulcan-agent-service.exe".to_string(),
        "--call-tools".to_string(),
        "demo-tool".to_string(),
        "--runtime-root=".to_string(),
    ];
    let error = match parse_runtime_mode_from_args(&args) {
        Ok(_) => panic!("empty inline runtime-root should fail"),
        Err(error) => error,
    };
    assert!(
        error
            .to_string()
            .contains("--runtime-root requires a value"),
        "unexpected error: {error}"
    );
}

/// Host-only reload tools should still run even when configured skill roots are invalid, because they no longer require preloading the Lua engine.
/// 仅宿主侧的 reload 工具即使在技能根配置无效时也应能运行，因为它们不再要求预先加载 Lua 引擎。
#[test]
fn run_call_host_tool_mode_supports_reload_without_loading_invalid_skill_roots() {
    let root = unique_test_dir("reload-host-tool");
    std::fs::create_dir_all(root.join("configs")).expect("failed to create runtime config dir");
    let missing_skill_root = root.join("missing-skills");
    let config = Config {
        runtime_root: Some(root.to_string_lossy().to_string()),
        skill_roots: Some(vec![crate::config::SkillRootConfigEntry::Path(
            missing_skill_root.to_string_lossy().to_string(),
        )]),
        ..Config::default()
    };

    preload_runtime_mcp_configs(&config).expect("host runtime config preload should succeed");
    run_call_host_tool_mode(
        config,
        "reload_vulcan_mcp_configs",
        json!({}),
        DEFAULT_CALL_TOOL_CLIENT_NAME,
    )
    .expect("reload host tool should succeed without loading invalid skill roots");

    let _ = std::fs::remove_dir_all(&root);
}

/// Host-only luaskill-config should still be exposed through the built server even when no Lua skill roots are available.
/// 即使没有任何 Lua 技能根，构建出的服务也应继续对外暴露宿主侧 luaskill-config。
#[test]
fn build_server_exposes_luaskill_config_without_skill_roots() {
    let root = unique_test_dir("luaskill-config-build-server");
    std::fs::create_dir_all(root.join("configs")).expect("failed to create runtime config dir");
    let config = Config {
        runtime_root: Some(root.to_string_lossy().to_string()),
        skill_roots: Some(vec![]),
        ..Config::default()
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime should build");

    let server = runtime
        .block_on(build_server(&config))
        .expect("build_server should succeed without skills");
    let response = runtime
        .block_on(
            McpDispatcher::new(server.clone()).handle_message_with_context(
                &json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "tools/list"
                }),
                RequestContext::default(),
            ),
        )
        .expect("tools/list should return one response");
    let tool_names = response
        .get("result")
        .and_then(|value| value.get("tools"))
        .and_then(Value::as_array)
        .expect("tools array should exist in result payload")
        .iter()
        .filter_map(|tool| tool.get("name").and_then(Value::as_str))
        .collect::<Vec<_>>();

    assert!(tool_names.contains(&"luaskill-config"));
    let _ = std::fs::remove_dir_all(&root);
}

/// Empty runtime roots should still expose skill-manager and create the ROOT skills directory for first-run installs.
/// 空运行根仍应暴露 skill-manager，并为首次运行安装创建 ROOT skills 目录。
#[test]
fn build_server_exposes_skill_manager_without_existing_skills() {
    let root = unique_test_dir("skill-manager-empty-runtime");
    std::fs::create_dir_all(root.join("configs")).expect("failed to create runtime config dir");
    let config = Config {
        runtime_root: Some(root.to_string_lossy().to_string()),
        skill_roots: Some(vec![]),
        ..Config::default()
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime should build");

    let server = runtime
        .block_on(build_server(&config))
        .expect("build_server should succeed without preinstalled skills");
    assert!(
        root.join("skills").is_dir(),
        "ROOT skills directory should be created for skill-manager"
    );

    let response = runtime
        .block_on(
            McpDispatcher::new(server.clone()).handle_message_with_context(
                &json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "tools/list"
                }),
                RequestContext::default(),
            ),
        )
        .expect("tools/list should return one response");
    let tool_names = response
        .get("result")
        .and_then(|value| value.get("tools"))
        .and_then(Value::as_array)
        .expect("tools array should exist in result payload")
        .iter()
        .filter_map(|tool| tool.get("name").and_then(Value::as_str))
        .collect::<Vec<_>>();
    assert!(tool_names.contains(&"skill-manager"));

    let list_response = runtime
        .block_on(
            McpDispatcher::new(server.clone()).handle_message_with_context(
                &json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "method": "tools/call",
                    "params": {
                        "name": "skill-manager",
                        "arguments": {
                            "action": "list"
                        }
                    }
                }),
                RequestContext::default(),
            ),
        )
        .expect("skill-manager list should return one response");
    let tool_result: ToolCallResult = serde_json::from_value(
        list_response
            .get("result")
            .cloned()
            .expect("skill-manager list should return result"),
    )
    .expect("skill-manager list result should deserialize");
    let rendered = tool_result
        .content
        .first()
        .map(|item| item.text.clone())
        .unwrap_or_default();
    assert_eq!(rendered, "No LuaSkills are installed in the USER layer.");
    let _ = std::fs::remove_dir_all(&root);
}

/// Skill-manager update should report a tool error when USER does not contain the skill.
/// 当 USER 不包含该技能时，skill-manager update 应报告工具错误。
#[test]
fn skill_manager_update_missing_skill_returns_tool_error() {
    let root = unique_test_dir("skill-manager-update-missing");
    std::fs::create_dir_all(root.join("configs")).expect("failed to create runtime config dir");
    let config = Config {
        runtime_root: Some(root.to_string_lossy().to_string()),
        skill_roots: Some(vec![]),
        ..Config::default()
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime should build");

    let server = runtime
        .block_on(build_server(&config))
        .expect("build_server should succeed without preinstalled skills");
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
                            "action": "update",
                            "skill_id": "vulcan-codekit"
                        }
                    }
                }),
                RequestContext::default(),
            ),
        )
        .expect("skill-manager update should return one response");
    let tool_result: ToolCallResult = serde_json::from_value(
        response
            .get("result")
            .cloned()
            .expect("skill-manager update should return result"),
    )
    .expect("skill-manager update result should deserialize");
    let rendered = tool_result
        .content
        .first()
        .map(|item| item.text.clone())
        .unwrap_or_default();

    assert_eq!(tool_result.is_error, Some(true));
    assert!(rendered.contains("skill-manager update failed"));
    assert!(rendered.contains("not installed"));
    let _ = std::fs::remove_dir_all(&root);
}

/// The configured ROOT layer should remain the single system root when it is already present.
/// 当已存在显式 ROOT 层时，应保留它作为唯一系统根。
#[test]
fn skill_manager_root_uses_configured_root_layer_without_duplicate() {
    let root = unique_test_dir("skill-manager-root-name-collision");
    let configured_skills_dir = root.join("configured-skills");
    let runtime_root = root.join("runtime");
    std::fs::create_dir_all(&configured_skills_dir)
        .expect("failed to create configured skills dir");
    let mut skill_roots = vec![RuntimeSkillRoot {
        name: "ROOT".to_string(),
        skills_dir: configured_skills_dir.clone(),
    }];

    ensure_root_skill_manager_root(Some(&runtime_root), &mut skill_roots)
        .expect("configured ROOT should be preserved");

    assert_eq!(skill_roots.len(), 1);
    assert_eq!(skill_roots[0].name, "ROOT");
    assert_eq!(skill_roots[0].skills_dir, configured_skills_dir);
    assert!(!runtime_root.join("skills").exists());
    let _ = std::fs::remove_dir_all(&root);
}

/// Runtime root construction should keep formal ROOT before USER regardless of insertion order.
/// 运行根构造应保持正式 ROOT 位于 USER 之前，不受插入顺序影响。
#[test]
fn skill_manager_roots_are_ordered_by_formal_layers() {
    let root = unique_test_dir("skill-manager-formal-order");
    let runtime_root = root.join("runtime");
    std::fs::create_dir_all(runtime_root.join("configs"))
        .expect("failed to create runtime config dir");
    let mut skill_roots = Vec::new();

    ensure_skill_manager_runtime_roots(Some(&runtime_root), &mut skill_roots)
        .expect("formal roots should be created");

    assert!(skill_roots.len() >= 2);
    assert_eq!(skill_roots[0].name, "ROOT");
    assert_eq!(skill_roots[1].name, "USER");
    let _ = std::fs::remove_dir_all(&root);
}

/// ROOT CLI selection should choose the system layer even when ordinary layers are also present.
/// 即使普通层同时存在，ROOT CLI 选择逻辑也应选中系统层。
#[test]
fn root_skill_cli_selection_uses_root_layer() {
    let roots = vec![
        RuntimeSkillRoot {
            name: "USER".to_string(),
            skills_dir: std::path::PathBuf::from("D:/user/skills"),
        },
        RuntimeSkillRoot {
            name: "ROOT".to_string(),
            skills_dir: std::path::PathBuf::from("D:/runtime/skills"),
        },
    ];

    let root = select_root_skill_manager_root(&roots).expect("ROOT layer should resolve");

    assert_eq!(root.name, "ROOT");
    assert_eq!(
        root.skills_dir,
        std::path::PathBuf::from("D:/runtime/skills")
    );
}

/// ROOT update-all discovery should include only skill directories with managed install records.
/// ROOT 全量更新发现逻辑应只包含带受管安装记录的技能目录。
#[test]
fn collect_managed_root_skill_ids_skips_unmanaged_skills() {
    let root = unique_test_dir("root-managed-skill-ids");
    let runtime_root = root.join("runtime");
    let root_layer = RuntimeSkillRoot {
        name: "ROOT".to_string(),
        skills_dir: runtime_root.join("skills"),
    };
    write_minimal_root_skill(&root_layer.skills_dir, "managed-skill");
    write_minimal_root_skill(&root_layer.skills_dir, "unmanaged-skill");
    write_root_install_record(&runtime_root, "managed-skill");
    let host_options = LuaRuntimeHostOptions {
        temp_dir: Some(runtime_root.join("temp")),
        state_dir_name: "state".to_string(),
        allow_network_download: false,
        ..LuaRuntimeHostOptions::default()
    };
    let manager = build_root_skill_manager_for_cli(&root_layer, &host_options)
        .expect("ROOT manager should build");

    let skill_ids = collect_managed_root_skill_ids(&root_layer, &manager)
        .expect("managed skill ids should be collected");

    assert_eq!(skill_ids, vec!["managed-skill".to_string()]);
    let _ = std::fs::remove_dir_all(&root);
}

/// The host-managed root must not bypass sibling runtime-space uniqueness after it is appended.
/// 宿主管理根追加后不得绕过同级运行空间唯一性约束。
#[test]
fn skill_manager_root_rejects_sibling_runtime_space_collision() {
    let root = unique_test_dir("skill-manager-root-space-collision");
    let runtime_root = root.join("runtime");
    let configured_skills_dir = runtime_root.join("custom-skills");
    std::fs::create_dir_all(&configured_skills_dir)
        .expect("failed to create configured skills dir");
    let mut skill_roots = vec![RuntimeSkillRoot {
        name: "USER".to_string(),
        skills_dir: configured_skills_dir.clone(),
    }];

    let error = ensure_root_skill_manager_root(Some(&runtime_root), &mut skill_roots)
        .expect_err("managed root should reject sibling runtime-space collisions");

    assert!(
        error
            .to_string()
            .contains("shares the same sibling runtime space"),
        "unexpected error: {error}"
    );
    assert_eq!(
        skill_roots,
        vec![RuntimeSkillRoot {
            name: "USER".to_string(),
            skills_dir: configured_skills_dir,
        }]
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// Skill-manager target mutations should reject missing skill ids before touching the runtime.
/// skill-manager 目标变更应在触碰运行时前拒绝缺失的技能标识。
#[test]
fn skill_manager_update_requires_skill_id() {
    let root = unique_test_dir("skill-manager-update-skill-id");
    std::fs::create_dir_all(root.join("configs")).expect("failed to create runtime config dir");
    let config = Config {
        runtime_root: Some(root.to_string_lossy().to_string()),
        skill_roots: Some(vec![]),
        ..Config::default()
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime should build");
    let server = runtime
        .block_on(build_server(&config))
        .expect("build_server should succeed without preinstalled skills");

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
                            "action": "update"
                        }
                    }
                }),
                RequestContext::default(),
            ),
        )
        .expect("skill-manager update should return one response");
    let message = response
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    assert!(message.contains("requires parameter: skill_id"));
    let _ = std::fs::remove_dir_all(&root);
}

/// Host-only luaskill-config should still run even when configured skill roots are invalid, because it should bypass Lua engine loading.
/// 宿主侧 luaskill-config 即使在技能根配置无效时也应能运行，因为它应跳过 Lua 引擎加载。
#[test]
fn run_call_host_tool_mode_supports_luaskill_config_without_loading_invalid_skill_roots() {
    let root = unique_test_dir("luaskill-config-host-tool");
    std::fs::create_dir_all(root.join("configs")).expect("failed to create runtime config dir");
    let missing_skill_root = root.join("missing-skills");
    let config = Config {
        runtime_root: Some(root.to_string_lossy().to_string()),
        skill_roots: Some(vec![crate::config::SkillRootConfigEntry::Path(
            missing_skill_root.to_string_lossy().to_string(),
        )]),
        ..Config::default()
    };

    preload_runtime_mcp_configs(&config).expect("host runtime config preload should succeed");
    run_call_host_tool_mode(
        config,
        "luaskill-config",
        json!({
            "action": "set",
            "skill_id": "demo-skill",
            "key": "api_token",
            "value": "sk-local"
        }),
        DEFAULT_CALL_TOOL_CLIENT_NAME,
    )
    .expect("luaskill-config host tool should succeed without loading invalid skill roots");

    let persisted: Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("configs").join("skill_config.json"))
            .expect("luaskill-config file should be created"),
    )
    .expect("persisted luaskill-config JSON should parse");
    assert_eq!(persisted["skills"]["demo-skill"]["api_token"], "sk-local");
    let _ = std::fs::remove_dir_all(&root);
}

/// Explicit invalid runtime_root values should fail before host-side reload logic falls back to implicit runtime discovery.
/// 显式无效的 runtime_root 应在宿主 reload 逻辑回退到隐式运行根发现之前直接失败。
#[test]
fn preload_runtime_mcp_configs_rejects_invalid_explicit_runtime_root() {
    let root = unique_test_dir("invalid-runtime-root");
    std::fs::create_dir_all(&root).expect("failed to create temp root");
    let config = Config {
        runtime_root: Some(root.join("missing-runtime").to_string_lossy().to_string()),
        ..Config::default()
    };

    let error = preload_runtime_mcp_configs(&config)
        .expect_err("invalid explicit runtime_root should fail");
    assert!(
        error
            .to_string()
            .contains("configured runtime_root does not exist"),
        "unexpected error: {error}"
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// File-shaped runtime libs paths should be rejected before PATH mutation begins.
/// 文件形态的运行时 libs 路径应在修改 PATH 之前被拒绝。
#[test]
fn add_libs_to_path_rejects_file_shaped_runtime_libs_dir() {
    let _guard = environment_lock().lock().expect("lock should succeed");
    let root = unique_test_dir("runtime-libs-file");
    std::fs::create_dir_all(&root).expect("failed to create runtime root");
    let libs_file = root.join("libs");
    std::fs::write(&libs_file, b"not-a-directory").expect("failed to create libs file");
    let original_path = std::env::var("PATH").unwrap_or_default();
    let config = Config {
        runtime_root: Some(root.to_string_lossy().to_string()),
        ..Config::default()
    };

    let error = add_libs_to_path(&config).expect_err("file-shaped libs path should fail");
    assert!(
        error
            .to_string()
            .contains("runtime libs path is not a directory"),
        "unexpected error: {error}"
    );
    assert_eq!(
        std::env::var("PATH").unwrap_or_default(),
        original_path,
        "PATH should remain unchanged when libs path is invalid"
    );

    let _ = std::fs::remove_dir_all(&root);
}
