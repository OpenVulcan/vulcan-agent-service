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

/// Create the fixed host and LuaSkills directories required by one application-root fixture.
/// 创建单个应用根测试夹具所需的固定宿主与 LuaSkills 目录。
/// Parameters: `application_root` is the isolated root used by the test.
/// 参数：`application_root` 是测试使用的隔离根目录。
/// Returns the created `<application_root>/lua_runtime` path.
/// 返回已创建的 `<application_root>/lua_runtime` 路径。
fn create_test_application_runtime(application_root: &std::path::Path) -> std::path::PathBuf {
    // LuaRuntimeRoot mirrors the production package boundary.
    // LuaRuntimeRoot 镜像生产环境包边界。
    let lua_runtime_root = application_root.join("lua_runtime");
    std::fs::create_dir_all(application_root.join("configs"))
        .expect("failed to create application config directory");
    std::fs::create_dir_all(lua_runtime_root.join("config"))
        .expect("failed to create LuaSkills config directory");
    std::fs::create_dir_all(lua_runtime_root.join("bin"))
        .expect("failed to create LuaSkills bin directory");
    lua_runtime_root
}

/// Restores the process PATH to the value captured before one environment-mutating test.
/// 将进程 PATH 恢复为单个环境修改测试开始前捕获的值。
struct PathEnvGuard {
    /// PATH value captured before the test mutates process environment state.
    /// 测试修改进程环境状态前捕获的 PATH 值。
    original_path: Option<std::ffi::OsString>,
}

impl PathEnvGuard {
    /// Capture the current PATH value so it can be restored when the guard is dropped.
    /// 捕获当前 PATH 值，以便守卫析构时恢复。
    fn capture() -> Self {
        Self {
            original_path: std::env::var_os("PATH"),
        }
    }
}

impl Drop for PathEnvGuard {
    /// Restore the captured PATH value after a test finishes or panics.
    /// 在测试结束或 panic 后恢复捕获到的 PATH 值。
    fn drop(&mut self) {
        match &self.original_path {
            Some(value) => unsafe {
                std::env::set_var("PATH", value);
            },
            None => unsafe {
                std::env::remove_var("PATH");
            },
        }
    }
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
        | RuntimeMode::Init
        | RuntimeMode::RootSkillInstall { .. }
        | RuntimeMode::RootSkillsUpdate
        | RuntimeMode::Service(..)
        | RuntimeMode::InternalLuaexecRequest { .. } => {
            panic!("expected call-tools runtime mode");
        }
    }
}

/// Process shutdown notification should send a true update while a receiver is alive.
/// 进程关闭通知应在接收端存活时发送 true 更新。
#[tokio::test]
async fn request_process_shutdown_sends_true_update() {
    let (shutdown_tx, mut shutdown_rx) = watch::channel(false);

    assert!(request_process_shutdown(&shutdown_tx));
    shutdown_rx
        .changed()
        .await
        .expect("shutdown receiver should observe the update");

    assert!(*shutdown_rx.borrow());
}

/// Process shutdown notification should report when every receiver has already dropped.
/// 进程关闭通知应在所有接收端均已丢弃时报告失败。
#[test]
fn request_process_shutdown_reports_dropped_receiver() {
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    drop(shutdown_rx);

    assert!(!request_process_shutdown(&shutdown_tx));
}

/// Stdio mode should be selectable directly so MCP can run over stdin/stdout without opening ports.
/// stdio 模式应可被直接选中，以便 MCP 通过标准输入输出运行而无需打开端口。
#[test]
fn parse_runtime_mode_accepts_stdio_mode() {
    let args = vec![
        "vulcan-agent-service.exe".to_string(),
        "--stdio".to_string(),
    ];
    let mode = parse_runtime_mode_from_args(&args).expect("stdio mode should parse");
    match mode {
        RuntimeMode::Stdio => {}
        RuntimeMode::Serve
        | RuntimeMode::Init
        | RuntimeMode::CallTool { .. }
        | RuntimeMode::RootSkillInstall { .. }
        | RuntimeMode::RootSkillsUpdate
        | RuntimeMode::Service(..)
        | RuntimeMode::InternalLuaexecRequest { .. } => {
            panic!("expected stdio runtime mode");
        }
    }
}

/// Init mode should parse its runtime-root option without falling through to service startup.
/// init 模式应解析 runtime-root 选项，而不是落回服务启动流程。
#[test]
fn parse_runtime_mode_accepts_init_mode() {
    let args = vec![
        "vulcan-agent-service.exe".to_string(),
        "init".to_string(),
        "--runtime-root".to_string(),
        "output".to_string(),
    ];
    let mode = parse_runtime_mode_from_args(&args).expect("init mode should parse");
    match mode {
        RuntimeMode::Init => {}
        RuntimeMode::Serve
        | RuntimeMode::Stdio
        | RuntimeMode::CallTool { .. }
        | RuntimeMode::RootSkillInstall { .. }
        | RuntimeMode::RootSkillsUpdate
        | RuntimeMode::Service(..)
        | RuntimeMode::InternalLuaexecRequest { .. } => {
            panic!("expected init runtime mode");
        }
    }
}

/// Init mode should reject flags outside its command-local path contract.
/// init 模式应拒绝超出命令路径契约的标志。
#[test]
fn parse_runtime_mode_rejects_unknown_init_flag() {
    let args = vec![
        "vulcan-agent-service.exe".to_string(),
        "init".to_string(),
        "--unknown-init-flag".to_string(),
    ];
    let error = match parse_runtime_mode_from_args(&args) {
        Ok(_) => panic!("unknown init flag should fail"),
        Err(error) => error,
    };
    assert!(
        error.to_string().contains("Unknown init flag"),
        "unexpected error: {error}"
    );
}

/// Service install mode should parse into the dedicated cross-platform service command instead of falling through to normal serve mode.
/// service install 模式应解析为专用的跨平台服务命令，而不是落回普通服务模式。
#[test]
fn parse_runtime_mode_accepts_service_install_mode() {
    let args = vec![
        "vulcan-agent-service.exe".to_string(),
        "service".to_string(),
        "install".to_string(),
        "--runtime-root".to_string(),
        "output".to_string(),
        "--service-name".to_string(),
        "vas-demo".to_string(),
        "--scope".to_string(),
        "user".to_string(),
        "--startup".to_string(),
        "manual".to_string(),
        "--start".to_string(),
    ];
    let mode = parse_runtime_mode_from_args(&args).expect("service install mode should parse");
    match mode {
        RuntimeMode::Service(crate::service::ServiceCommand::Install(options)) => {
            assert_eq!(
                options.runtime_root,
                Some(std::path::PathBuf::from("output"))
            );
            assert_eq!(options.service_name, "vas-demo");
            assert_eq!(options.scope.as_str(), "user");
            assert_eq!(options.startup.as_str(), "manual");
            assert!(options.start_immediately);
        }
        _ => panic!("expected service install runtime mode"),
    }
}

/// Service run mode should parse the explicit runtime root and service name needed by installed platform managers.
/// service run 模式应解析出已安装平台管理器所需的显式运行根与服务名称。
#[test]
fn parse_runtime_mode_accepts_service_run_mode() {
    let args = vec![
        "vulcan-agent-service.exe".to_string(),
        "service".to_string(),
        "run".to_string(),
        "--runtime-root=output".to_string(),
        "--service-name=vas-demo".to_string(),
    ];
    let mode = parse_runtime_mode_from_args(&args).expect("service run mode should parse");
    match mode {
        RuntimeMode::Service(crate::service::ServiceCommand::Run(options)) => {
            assert_eq!(
                options.runtime_root,
                Some(std::path::PathBuf::from("output"))
            );
            assert_eq!(options.service_name, "vas-demo");
        }
        _ => panic!("expected service run runtime mode"),
    }
}

/// Service install mode should default to the stable Windows-friendly service name when the caller omits it.
/// service install 模式在调用方省略服务名时应默认回落到稳定的 Windows 友好服务名称。
#[test]
fn parse_runtime_mode_defaults_service_install_name() {
    let args = vec![
        "vulcan-agent-service.exe".to_string(),
        "service".to_string(),
        "install".to_string(),
    ];
    let mode = parse_runtime_mode_from_args(&args).expect("service install mode should parse");
    match mode {
        RuntimeMode::Service(crate::service::ServiceCommand::Install(options)) => {
            assert!(options.runtime_root.is_none());
            assert_eq!(options.service_name, crate::service::DEFAULT_SERVICE_NAME);
        }
        _ => panic!("expected service install runtime mode"),
    }
}

/// Service run mode should allow the hosted layout to infer the runtime root and default service name.
/// service run 模式应允许宿主布局自行推导运行根并回落到默认服务名。
#[test]
fn parse_runtime_mode_defaults_service_run_options() {
    let args = vec![
        "vulcan-agent-service.exe".to_string(),
        "service".to_string(),
        "run".to_string(),
    ];
    let mode = parse_runtime_mode_from_args(&args).expect("service run mode should parse");
    match mode {
        RuntimeMode::Service(crate::service::ServiceCommand::Run(options)) => {
            assert!(options.runtime_root.is_none());
            assert_eq!(options.service_name, crate::service::DEFAULT_SERVICE_NAME);
        }
        _ => panic!("expected service run runtime mode"),
    }
}

/// Service status mode should target the stable default service name when the caller omits `--service-name`.
/// service status 模式在调用方省略 `--service-name` 时应指向稳定的默认服务名。
#[test]
fn parse_runtime_mode_defaults_service_status_name() {
    let args = vec![
        "vulcan-agent-service.exe".to_string(),
        "service".to_string(),
        "status".to_string(),
    ];
    let mode = parse_runtime_mode_from_args(&args).expect("service status mode should parse");
    match mode {
        RuntimeMode::Service(crate::service::ServiceCommand::Status(options)) => {
            assert_eq!(options.service_name, crate::service::DEFAULT_SERVICE_NAME);
        }
        _ => panic!("expected service status runtime mode"),
    }
}

/// Service uninstall mode should target the stable default service name when the caller omits `--service-name`.
/// service uninstall 模式在调用方省略 `--service-name` 时应指向稳定的默认服务名。
#[test]
fn parse_runtime_mode_defaults_service_uninstall_name() {
    let args = vec![
        "vulcan-agent-service.exe".to_string(),
        "service".to_string(),
        "uninstall".to_string(),
    ];
    let mode = parse_runtime_mode_from_args(&args).expect("service uninstall mode should parse");
    match mode {
        RuntimeMode::Service(crate::service::ServiceCommand::Uninstall(options)) => {
            assert_eq!(options.service_name, crate::service::DEFAULT_SERVICE_NAME);
        }
        _ => panic!("expected service uninstall runtime mode"),
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
        | RuntimeMode::Init
        | RuntimeMode::Stdio
        | RuntimeMode::CallTool { .. }
        | RuntimeMode::RootSkillsUpdate
        | RuntimeMode::Service(..)
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
        | RuntimeMode::Init
        | RuntimeMode::Stdio
        | RuntimeMode::CallTool { .. }
        | RuntimeMode::RootSkillInstall { .. }
        | RuntimeMode::Service(..)
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
        "--call-client-name".to_string(),
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

/// Call-tools mode should reject every flag outside the current command contract.
/// call-tools 模式应拒绝当前命令契约之外的所有标志。
#[test]
fn parse_runtime_mode_rejects_unknown_flag_in_call_tools_mode() {
    let args = vec![
        "vulcan-agent-service.exe".to_string(),
        "--call-tools".to_string(),
        "demo-tool".to_string(),
        "--obsolete-flag".to_string(),
    ];
    let error = match parse_runtime_mode_from_args(&args) {
        Ok(_) => panic!("unknown flag should fail"),
        Err(error) => error,
    };
    assert!(
        error.to_string().contains("Unknown CLI flag"),
        "unexpected error: {error}"
    );
}

/// Serve mode should reject unknown inline flags instead of silently ignoring them.
/// serve 模式应拒绝未知内联标志，而不是静默忽略。
#[test]
fn parse_runtime_mode_rejects_unknown_inline_flag_in_serve_mode() {
    let args = vec![
        "vulcan-agent-service.exe".to_string(),
        "--unknown-setting=value".to_string(),
    ];
    let error = match parse_runtime_mode_from_args(&args) {
        Ok(_) => panic!("unknown inline flag should fail"),
        Err(error) => error,
    };
    assert!(
        error.to_string().contains("Unknown CLI flag"),
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
        | RuntimeMode::Init
        | RuntimeMode::Stdio
        | RuntimeMode::RootSkillInstall { .. }
        | RuntimeMode::RootSkillsUpdate
        | RuntimeMode::Service(..)
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
    // Hold the repository-wide runtime-config fixture lock through preload and host-tool reload.
    // 在预载与宿主工具重载全过程持有仓库级运行时配置夹具锁。
    let _runtime_config_guard = crate::config::runtime_config_test_lock()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let root = unique_test_dir("reload-host-tool");
    create_test_application_runtime(&root);
    let missing_skill_root = root.join("missing-skills");
    let config = Config {
        runtime_root: Some(root.to_string_lossy().to_string()),
        skill_roots: Some(vec![crate::config::NamedSkillRootConfig {
            name: "ROOT".to_string(),
            path: missing_skill_root.to_string_lossy().to_string(),
        }]),
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

/// runtime-config should be exposed when build_server creates and initializes the formal empty root chain.
/// build_server 创建并初始化正式空根链后应暴露 runtime-config。
#[test]
fn build_server_exposes_runtime_config_after_empty_root_chain_initialization() {
    let root = unique_test_dir("runtime-config-build-server");
    create_test_application_runtime(&root);
    let config = Config {
        runtime_root: Some(root.to_string_lossy().to_string()),
        skill_config_root: Some(root.join("skill-config").to_string_lossy().to_string()),
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

    assert!(tool_names.contains(&"runtime-config"));
    let _ = std::fs::remove_dir_all(&root);
}

/// Empty runtime roots should still expose skill-manager and create the ROOT skills directory for first-run installs.
/// 空运行根仍应暴露 skill-manager，并为首次运行安装创建 ROOT skills 目录。
#[test]
fn build_server_exposes_skill_manager_without_existing_skills() {
    let root = unique_test_dir("skill-manager-empty-runtime");
    create_test_application_runtime(&root);
    let config = Config {
        runtime_root: Some(root.to_string_lossy().to_string()),
        skill_config_root: Some(root.join("skill-config").to_string_lossy().to_string()),
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
        root.join("lua_runtime").join("skills").is_dir(),
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
    create_test_application_runtime(&root);
    let config = Config {
        runtime_root: Some(root.to_string_lossy().to_string()),
        skill_config_root: Some(root.join("skill-config").to_string_lossy().to_string()),
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

/// Formal root sorting should normalize labels and order ROOT, PROJECT, then USER.
/// 正式根排序应规范化标签并按 ROOT、PROJECT、USER 排序。
#[test]
fn sort_skill_manager_formal_roots_normalizes_and_orders_layers() {
    let mut skill_roots = vec![
        RuntimeSkillRoot {
            name: "user".to_string(),
            skills_dir: std::path::PathBuf::from("D:/user/skills"),
        },
        RuntimeSkillRoot {
            name: "project".to_string(),
            skills_dir: std::path::PathBuf::from("D:/project/skills"),
        },
        RuntimeSkillRoot {
            name: "root".to_string(),
            skills_dir: std::path::PathBuf::from("D:/runtime/skills"),
        },
    ];

    sort_skill_manager_formal_roots(&mut skill_roots).expect("formal roots should sort");

    assert_eq!(
        skill_roots
            .iter()
            .map(|root| root.name.as_str())
            .collect::<Vec<_>>(),
        vec!["ROOT", "PROJECT", "USER"]
    );
}

/// Formal root sorting should leave the input untouched when a label is invalid.
/// 正式根排序在标签无效时应保持输入不变。
#[test]
fn sort_skill_manager_formal_roots_preserves_input_on_invalid_label() {
    let mut skill_roots = vec![
        RuntimeSkillRoot {
            name: "user".to_string(),
            skills_dir: std::path::PathBuf::from("D:/user/skills"),
        },
        RuntimeSkillRoot {
            name: "BROKEN".to_string(),
            skills_dir: std::path::PathBuf::from("D:/broken/skills"),
        },
    ];
    let original_roots = skill_roots.clone();

    let error = sort_skill_manager_formal_roots(&mut skill_roots)
        .expect_err("invalid formal root label should fail");

    assert!(
        error.contains("unsupported skill root label"),
        "unexpected error: {error}"
    );
    assert_eq!(skill_roots, original_roots);
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

/// ROOT update-all discovery should treat a missing ROOT skills directory as an empty managed set.
/// ROOT 全量更新发现逻辑应把缺失的 ROOT skills 目录视为空受管集合。
#[test]
fn collect_managed_root_skill_ids_returns_empty_for_missing_root_directory() {
    let root = unique_test_dir("root-managed-missing-dir");
    let runtime_root = root.join("runtime");
    let root_layer = RuntimeSkillRoot {
        name: "ROOT".to_string(),
        skills_dir: runtime_root.join("skills"),
    };
    let host_options = LuaRuntimeHostOptions {
        temp_dir: Some(runtime_root.join("temp")),
        state_dir_name: "state".to_string(),
        allow_network_download: false,
        ..LuaRuntimeHostOptions::default()
    };
    let manager = build_root_skill_manager_for_cli(&root_layer, &host_options)
        .expect("ROOT manager should build");

    let skill_ids = collect_managed_root_skill_ids(&root_layer, &manager)
        .expect("missing ROOT skills directory should be treated as empty");

    assert!(
        skill_ids.is_empty(),
        "missing ROOT directory should be empty"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// ROOT update-all discovery should reject a file where the ROOT skills directory is required.
/// ROOT 全量更新发现逻辑应拒绝 ROOT skills 目录位置出现文件。
#[test]
fn collect_managed_root_skill_ids_rejects_file_shaped_root_directory() {
    let root = unique_test_dir("root-managed-file-dir");
    let runtime_root = root.join("runtime");
    let root_layer = RuntimeSkillRoot {
        name: "ROOT".to_string(),
        skills_dir: runtime_root.join("skills"),
    };
    std::fs::create_dir_all(&runtime_root).expect("runtime root should be created");
    std::fs::write(&root_layer.skills_dir, b"not-a-directory")
        .expect("file-shaped ROOT skills directory should be written");
    let host_options = LuaRuntimeHostOptions {
        temp_dir: Some(runtime_root.join("temp")),
        state_dir_name: "state".to_string(),
        allow_network_download: false,
        ..LuaRuntimeHostOptions::default()
    };
    let manager = build_root_skill_manager_for_cli(&root_layer, &host_options)
        .expect("ROOT manager should build");

    let error = collect_managed_root_skill_ids(&root_layer, &manager)
        .expect_err("file-shaped ROOT skills directory should fail");

    assert!(
        error
            .to_string()
            .contains("ROOT skills directory is not a directory"),
        "unexpected error: {error}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// ROOT update-all discovery should reject a directory where a skill manifest file is required.
/// ROOT 全量更新发现逻辑应拒绝技能清单文件位置出现目录。
#[test]
fn collect_managed_root_skill_ids_rejects_directory_shaped_skill_manifest() {
    let root = unique_test_dir("root-managed-directory-manifest");
    let runtime_root = root.join("runtime");
    let root_layer = RuntimeSkillRoot {
        name: "ROOT".to_string(),
        skills_dir: runtime_root.join("skills"),
    };
    let skill_dir = root_layer.skills_dir.join("broken-skill");
    std::fs::create_dir_all(skill_dir.join("skill.yaml"))
        .expect("directory-shaped skill manifest should be created");
    let host_options = LuaRuntimeHostOptions {
        temp_dir: Some(runtime_root.join("temp")),
        state_dir_name: "state".to_string(),
        allow_network_download: false,
        ..LuaRuntimeHostOptions::default()
    };
    let manager = build_root_skill_manager_for_cli(&root_layer, &host_options)
        .expect("ROOT manager should build");

    let error = collect_managed_root_skill_ids(&root_layer, &manager)
        .expect_err("directory-shaped skill manifest should fail");

    assert!(
        error
            .to_string()
            .contains("ROOT skill manifest is not a file"),
        "unexpected error: {error}"
    );
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
    create_test_application_runtime(&root);
    let config = Config {
        runtime_root: Some(root.to_string_lossy().to_string()),
        skill_config_root: Some(root.join("skill-config").to_string_lossy().to_string()),
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

/// runtime-config should fail startup when configured skill roots are invalid because it requires the LuaEngine declaration registry.
/// runtime-config 在技能根配置无效时应启动失败，因为它依赖 LuaEngine 声明注册表。
#[test]
fn run_call_host_tool_mode_rejects_runtime_config_with_invalid_skill_roots() {
    // Hold the repository-wide runtime-config fixture lock until cache-backed host-tool assertions finish.
    // 持有仓库级运行时配置夹具锁，直到依赖缓存的宿主工具断言结束。
    let _runtime_config_guard = crate::config::runtime_config_test_lock()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let root = unique_test_dir("runtime-config-host-tool");
    create_test_application_runtime(&root);
    let missing_skill_root = root.join("missing-skills");
    let config = Config {
        runtime_root: Some(root.to_string_lossy().to_string()),
        skill_config_root: Some(root.join("skill-config").to_string_lossy().to_string()),
        skill_roots: Some(vec![crate::config::NamedSkillRootConfig {
            name: "ROOT".to_string(),
            path: missing_skill_root.to_string_lossy().to_string(),
        }]),
        ..Config::default()
    };

    preload_runtime_mcp_configs(&config).expect("host runtime config preload should succeed");
    let error = run_call_host_tool_mode(
        config,
        "runtime-config",
        json!({
            "action": "describe",
            "skill_id": "demo-skill"
        }),
        DEFAULT_CALL_TOOL_CLIENT_NAME,
    )
    .expect_err("runtime-config should require valid skill roots and LuaEngine initialization");
    assert!(error.to_string().contains("configured skill root"));
    assert!(!root.join("skill-config").exists());
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
            .contains("configured application runtime_root does not exist"),
        "unexpected error: {error}"
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// Missing runtime libs paths should be skipped without mutating PATH.
/// 缺失的运行时 libs 路径应被跳过且不修改 PATH。
#[test]
fn add_libs_to_path_skips_missing_runtime_libs_dir_without_path_mutation() {
    let _guard = environment_lock().lock().expect("lock should succeed");
    let root = unique_test_dir("runtime-libs-missing");
    create_test_application_runtime(&root);
    let _path_guard = PathEnvGuard::capture();
    unsafe {
        std::env::set_var("PATH", "original-path");
    }
    let config = Config {
        runtime_root: Some(root.to_string_lossy().to_string()),
        ..Config::default()
    };

    add_libs_to_path(&config).expect("missing libs path should be skipped");

    assert_eq!(
        std::env::var("PATH").expect("PATH should remain valid unicode"),
        "original-path",
        "PATH should remain unchanged when libs path is missing"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// Existing runtime libs paths should be prepended to PATH before the original value.
/// 已存在的运行时 libs 路径应被前置到原始 PATH 之前。
#[test]
fn add_libs_to_path_prepends_existing_runtime_libs_dir() {
    let _guard = environment_lock().lock().expect("lock should succeed");
    let root = unique_test_dir("runtime-libs-existing");
    let libs_dir = root.join("lua_runtime").join("libs");
    create_test_application_runtime(&root);
    std::fs::create_dir_all(&libs_dir).expect("failed to create runtime libs dir");
    let _path_guard = PathEnvGuard::capture();
    unsafe {
        std::env::set_var("PATH", "original-path");
    }
    let config = Config {
        runtime_root: Some(root.to_string_lossy().to_string()),
        ..Config::default()
    };

    add_libs_to_path(&config).expect("existing libs path should be prepended");

    #[cfg(windows)]
    let separator = ";";
    #[cfg(not(windows))]
    let separator = ":";
    assert_eq!(
        std::env::var("PATH").expect("PATH should remain valid unicode"),
        format!("{}{}original-path", libs_dir.to_string_lossy(), separator),
        "PATH should prepend runtime libs"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// Existing runtime libs paths should become the complete PATH when no original PATH exists.
/// 当原始 PATH 不存在时，已存在的运行时 libs 路径应成为完整 PATH。
#[test]
fn add_libs_to_path_sets_only_runtime_libs_when_path_is_missing() {
    let _guard = environment_lock().lock().expect("lock should succeed");
    let root = unique_test_dir("runtime-libs-no-path");
    let libs_dir = root.join("lua_runtime").join("libs");
    create_test_application_runtime(&root);
    std::fs::create_dir_all(&libs_dir).expect("failed to create runtime libs dir");
    let _path_guard = PathEnvGuard::capture();
    unsafe {
        std::env::remove_var("PATH");
    }
    let config = Config {
        runtime_root: Some(root.to_string_lossy().to_string()),
        ..Config::default()
    };

    add_libs_to_path(&config).expect("existing libs path should define PATH");

    let current_path = std::env::var_os("PATH").expect("PATH should be set");
    let path_entries: Vec<_> = std::env::split_paths(&current_path).collect();
    assert_eq!(
        path_entries,
        vec![libs_dir],
        "PATH should contain only the runtime libs path when original PATH is missing"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// Non-UTF-8 PATH entries should survive runtime libs prepending on Unix hosts.
/// Unix 主机上的非 UTF-8 PATH 条目应在前置运行时 libs 后保留下来。
#[cfg(unix)]
#[test]
fn add_libs_to_path_preserves_non_unicode_path_entries() {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};

    let _guard = environment_lock().lock().expect("lock should succeed");
    let root = unique_test_dir("runtime-libs-non-unicode-path");
    let libs_dir = root.join("lua_runtime").join("libs");
    create_test_application_runtime(&root);
    std::fs::create_dir_all(&libs_dir).expect("failed to create runtime libs dir");
    let _path_guard = PathEnvGuard::capture();
    let non_unicode_path = std::ffi::OsString::from_vec(b"/tmp/vulcan-\xFF-path".to_vec());
    unsafe {
        std::env::set_var("PATH", &non_unicode_path);
    }
    let config = Config {
        runtime_root: Some(root.to_string_lossy().to_string()),
        ..Config::default()
    };

    add_libs_to_path(&config).expect("existing libs path should preserve non-unicode PATH");

    let current_path = std::env::var_os("PATH").expect("PATH should be set");
    let path_entries: Vec<_> = std::env::split_paths(&current_path).collect();
    assert_eq!(
        path_entries.first(),
        Some(&libs_dir),
        "PATH should prepend runtime libs before existing entries"
    );
    assert_eq!(
        path_entries
            .get(1)
            .expect("non-unicode PATH entry should remain")
            .as_os_str()
            .as_bytes(),
        non_unicode_path.as_os_str().as_bytes(),
        "PATH should preserve the original non-unicode entry bytes"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// File-shaped runtime libs paths should be rejected before PATH mutation begins.
/// 文件形态的运行时 libs 路径应在修改 PATH 之前被拒绝。
#[test]
fn add_libs_to_path_rejects_file_shaped_runtime_libs_dir() {
    let _guard = environment_lock().lock().expect("lock should succeed");
    let root = unique_test_dir("runtime-libs-file");
    create_test_application_runtime(&root);
    let libs_file = root.join("lua_runtime").join("libs");
    std::fs::write(&libs_file, b"not-a-directory").expect("failed to create libs file");
    let _path_guard = PathEnvGuard::capture();
    let original_path = std::env::var_os("PATH");
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
        std::env::var_os("PATH"),
        original_path,
        "PATH should remain unchanged when libs path is invalid"
    );

    let _ = std::fs::remove_dir_all(&root);
}
