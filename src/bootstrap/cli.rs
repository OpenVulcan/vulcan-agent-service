use crate::service::{
    DEFAULT_SERVICE_NAME, ServiceCommand, ServiceInstallOptions, ServiceRunOptions, ServiceScope,
    ServiceStartup, ServiceTargetOptions,
};
use luaskills::SkillInstallSourceType;
use serde_json::{Value, json};
/// Command-line runtime mode.
/// 命令行运行模式。
pub(super) enum RuntimeMode {
    /// Start the MCP server on stdio using Content-Length framed JSON-RPC.
    /// 使用 Content-Length 分帧 JSON-RPC 的 stdio 方式启动 MCP 服务。
    Stdio,
    /// Start the regular HTTP/gRPC services.
    /// 正常启动 HTTP/gRPC 服务。
    Serve,
    /// Run one cross-platform service management command.
    /// 运行一条跨平台服务管理命令。
    Service(ServiceCommand),
    /// Initialize the tool runtime only and directly invoke a single tool for local debugging.
    /// 仅初始化工具运行环境，并直接调用单个 tool 做本地调试。
    /// This mode simulates a fixed client context, does not read `config.yaml`, and does not open any ports.
    /// 该模式会模拟一个固定客户端上下文，不读取 `config.yaml`，也不启动任何端口。
    CallTool {
        tool_name: String,
        arguments: Value,
        simulated_client_name: String,
    },
    /// Install one managed LuaSkill into the host-controlled ROOT layer without starting MCP transports.
    /// 在不启动 MCP 传输服务的情况下，将单个受管 LuaSkill 安装到宿主控制的 ROOT 层。
    RootSkillInstall {
        /// Source locator such as `LuaSkills/vulcan-codekit` or a GitHub repository URL.
        /// 来源定位值，例如 `LuaSkills/vulcan-codekit` 或 GitHub 仓库 URL。
        source: String,
        /// Optional source type override parsed from the CLI.
        /// 从 CLI 解析得到的可选来源类型覆盖。
        source_type: Option<SkillInstallSourceType>,
    },
    /// Update every managed LuaSkill declared in the host-controlled ROOT layer without starting MCP transports.
    /// 在不启动 MCP 传输服务的情况下，更新宿主控制的 ROOT 层内所有受管 LuaSkill。
    RootSkillsUpdate,
    /// Internal-only luaexec subprocess execution mode.
    /// 内部专用的 luaexec 子进程执行模式。
    InternalLuaexecRequest { request_file: String },
}

/// Default simulated client name used by the `--call-tools` debug mode.
/// `--call-tools` 调试模式使用的默认模拟客户端名称。
pub(super) const DEFAULT_CALL_TOOL_CLIENT_NAME: &str = "VulcanMcpTest";

/// Return whether one raw CLI token carries an inline `--runtime-root=value` or `-runtime-root=value` assignment.
/// 返回某个原始 CLI 片段是否携带内联 `--runtime-root=value` 或 `-runtime-root=value` 赋值。
fn is_inline_runtime_root_flag_arg(arg: &str) -> bool {
    arg.starts_with("-runtime-root=") || arg.starts_with("--runtime-root=")
}

/// Parse the runtime mode from CLI arguments.
/// 根据命令行参数解析运行模式。
/// Supported forms:
/// 支持以下形式：
/// - `--call-tools <tool_name> [json_arguments]`
/// - `--call-tools <tool_name> [json_arguments]`
/// - `--call-client-name <name>`: set the simulated client name
/// - `--call-client-name <name>`：指定模拟客户端名称
/// - `--install-root-skill <source> [--source-type github|url]`
/// - `--install-root-skill <source> [--source-type github|url]`
/// - `--update-root-skills`
/// - `--update-root-skills`
pub(super) fn parse_runtime_mode() -> Result<RuntimeMode, Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    parse_runtime_mode_from_args(&args)
}

/// Parse the runtime mode from an explicit argv slice so CLI behavior stays unit-testable.
/// 从显式 argv 切片解析运行模式，以保证 CLI 行为可被单元测试覆盖。
pub(super) fn parse_runtime_mode_from_args(
    args: &[String],
) -> Result<RuntimeMode, Box<dyn std::error::Error>> {
    if args.get(1).map(String::as_str) == Some("service") {
        return Ok(RuntimeMode::Service(parse_service_command_from_args(args)?));
    }
    validate_non_service_flag_names(args)?;
    for index in 0..args.len() {
        if args[index] == "--internal-luaexec-request" {
            let request_file = args
                .get(index + 1)
                .ok_or("--internal-luaexec-request requires a file path")?
                .clone();
            return Ok(RuntimeMode::InternalLuaexecRequest { request_file });
        }
    }
    for argument in args {
        if argument == "--stdio" {
            return Ok(RuntimeMode::Stdio);
        }
    }
    for index in 0..args.len() {
        if args[index] == "--install-root-skill" {
            // Extract the install source before parsing command-local optional flags.
            // 在解析命令局部可选标志前提取安装来源。
            let source = args
                .get(index + 1)
                .filter(|value| !value.starts_with('-'))
                .ok_or("--install-root-skill requires a source")?
                .clone();
            // Parse the optional install source type after the source locator.
            // 在来源定位值之后解析可选安装来源类型。
            let source_type = parse_root_skill_install_source_type_from_args(args, index + 2)?;
            return Ok(RuntimeMode::RootSkillInstall {
                source,
                source_type,
            });
        }
    }
    for index in 0..args.len() {
        if args[index] == "--update-root-skills" {
            validate_root_skills_update_args(args, index + 1)?;
            return Ok(RuntimeMode::RootSkillsUpdate);
        }
    }
    for index in 0..args.len() {
        if args[index] == "--call-tools" {
            let tool_name = args
                .get(index + 1)
                .ok_or("--call-tools requires a tool name")?
                .clone();

            let mut arguments = json!({});
            let mut simulated_client_name = DEFAULT_CALL_TOOL_CLIENT_NAME.to_string();
            let mut cursor = index + 2;
            while cursor < args.len() {
                match args[cursor].as_str() {
                    "--call-client-name" => {
                        let client_name = args
                            .get(cursor + 1)
                            .ok_or("--call-client-name requires a value")?;
                        simulated_client_name = client_name.clone();
                        cursor += 2;
                    }
                    "--call-tools" => {
                        break;
                    }
                    value if is_inline_runtime_root_flag_arg(value) => {
                        if value.ends_with('=') {
                            return Err("--runtime-root requires a value".into());
                        }
                        cursor += 1;
                    }
                    "-runtime-root" | "--runtime-root" => {
                        require_cli_flag_value(args, cursor, args[cursor].as_str())?;
                        cursor += 2;
                    }
                    value if value.starts_with('-') => {
                        return Err(format!("Unknown --call-tools flag: {}", value).into());
                    }
                    raw_json => {
                        arguments = serde_json::from_str::<Value>(raw_json)?;
                        cursor += 1;
                    }
                }
            }

            return Ok(RuntimeMode::CallTool {
                tool_name,
                arguments,
                simulated_client_name,
            });
        }
    }

    Ok(RuntimeMode::Serve)
}

/// Require one value after a CLI flag so malformed call-tools invocations fail early.
/// 要求某个 CLI 标志后必须跟随一个值，以便尽早拒绝格式错误的 call-tools 调用。
fn require_cli_flag_value(
    args: &[String],
    index: usize,
    flag: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(next_value) = args.get(index + 1) else {
        return Err(format!("{flag} requires a value").into());
    };
    if next_value.starts_with("--") || next_value.starts_with('-') {
        return Err(format!("{flag} requires a value").into());
    }
    Ok(())
}

/// Parse optional `--source-type` arguments accepted by the ROOT install command.
/// 解析 ROOT 安装命令接受的可选 `--source-type` 参数。
fn parse_root_skill_install_source_type_from_args(
    args: &[String],
    start_index: usize,
) -> Result<Option<SkillInstallSourceType>, Box<dyn std::error::Error>> {
    // Track the optional source type while scanning command-local flags.
    // 扫描命令局部标志时跟踪可选来源类型。
    let mut source_type = None;
    // Walk only the suffix after the install source so global flags before the command stay valid.
    // 仅遍历安装来源之后的参数后缀，使命令之前的全局标志仍然有效。
    let mut cursor = start_index;
    while cursor < args.len() {
        match args[cursor].as_str() {
            "--source-type" => {
                require_cli_flag_value(args, cursor, "--source-type")?;
                // Parse the explicit source type value immediately after the flag.
                // 解析紧跟在标志后的显式来源类型值。
                let raw_source_type = args
                    .get(cursor + 1)
                    .ok_or("--source-type requires a value")?;
                source_type = Some(parse_skill_install_source_type(raw_source_type)?);
                cursor += 2;
            }
            "-runtime-root" | "--runtime-root" => {
                require_cli_flag_value(args, cursor, args[cursor].as_str())?;
                cursor += 2;
            }
            value if is_inline_runtime_root_flag_arg(value) => {
                if value.ends_with('=') {
                    return Err("--runtime-root requires a value".into());
                }
                cursor += 1;
            }
            value if value.starts_with('-') => {
                return Err(format!("Unknown --install-root-skill flag: {}", value).into());
            }
            value => {
                return Err(format!(
                    "Unexpected --install-root-skill argument after source: {}",
                    value
                )
                .into());
            }
        }
    }
    Ok(source_type)
}

/// Validate command-local arguments accepted by the ROOT update-all command.
/// 校验 ROOT 全量更新命令接受的命令局部参数。
fn validate_root_skills_update_args(
    args: &[String],
    start_index: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    // Walk only the suffix after `--update-root-skills` so global flags before the command stay valid.
    // 仅遍历 `--update-root-skills` 之后的参数后缀，使命令之前的全局标志仍然有效。
    let mut cursor = start_index;
    while cursor < args.len() {
        match args[cursor].as_str() {
            "-runtime-root" | "--runtime-root" => {
                require_cli_flag_value(args, cursor, args[cursor].as_str())?;
                cursor += 2;
            }
            value if is_inline_runtime_root_flag_arg(value) => {
                if value.ends_with('=') {
                    return Err("--runtime-root requires a value".into());
                }
                cursor += 1;
            }
            value if value.starts_with('-') => {
                return Err(format!("Unknown --update-root-skills flag: {}", value).into());
            }
            value => {
                return Err(format!("Unexpected --update-root-skills argument: {}", value).into());
            }
        }
    }
    Ok(())
}

/// Reject every non-service flag that is not part of the current command-line contract.
/// 拒绝所有不属于当前命令行契约的非 service 标志。
/// Parameters: `args` is the complete process argument vector including the executable name.
/// 参数：`args` 是包含可执行文件名的完整进程参数向量。
/// Returns success when every flag is current, or one generic unknown-flag error.
/// 所有标志均属于当前契约时返回成功，否则返回通用未知标志错误。
fn validate_non_service_flag_names(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    for argument in args
        .iter()
        .skip(1)
        .filter(|argument| argument.starts_with('-'))
    {
        let known_flag = matches!(
            argument.as_str(),
            "--internal-luaexec-request"
                | "--stdio"
                | "--install-root-skill"
                | "--source-type"
                | "--update-root-skills"
                | "--call-tools"
                | "--call-client-name"
                | "-runtime-root"
                | "--runtime-root"
        ) || is_inline_runtime_root_flag_arg(argument);
        if !known_flag {
            return Err(format!("Unknown CLI flag: {argument}").into());
        }
    }
    Ok(())
}

/// Parse one CLI source-type token into the LuaSkills install source enum.
/// 将单个 CLI 来源类型片段解析为 LuaSkills 安装来源枚举。
fn parse_skill_install_source_type(
    value: &str,
) -> Result<SkillInstallSourceType, Box<dyn std::error::Error>> {
    match value.trim().to_ascii_lowercase().as_str() {
        "github" => Ok(SkillInstallSourceType::Github),
        "url" => Ok(SkillInstallSourceType::Url),
        _ => Err(format!(
            "unsupported source type '{}'; expected github or url",
            value
        )
        .into()),
    }
}

/// Parse one `service ...` subcommand family from the explicit argv slice.
/// 从显式 argv 切片解析一组 `service ...` 子命令。
fn parse_service_command_from_args(
    args: &[String],
) -> Result<ServiceCommand, Box<dyn std::error::Error>> {
    let subcommand = args
        .get(2)
        .map(String::as_str)
        .ok_or("service requires a subcommand")?;
    match subcommand {
        "install" => Ok(ServiceCommand::Install(parse_service_install_options(
            args, 3,
        )?)),
        "uninstall" => Ok(ServiceCommand::Uninstall(parse_service_target_options(
            args, 3,
        )?)),
        "start" => Ok(ServiceCommand::Start(parse_service_target_options(
            args, 3,
        )?)),
        "stop" => Ok(ServiceCommand::Stop(parse_service_target_options(args, 3)?)),
        "restart" => Ok(ServiceCommand::Restart(parse_service_target_options(
            args, 3,
        )?)),
        "status" => Ok(ServiceCommand::Status(parse_service_target_options(
            args, 3,
        )?)),
        "run" => Ok(ServiceCommand::Run(parse_service_run_options(args, 3)?)),
        "print-definition" => Ok(ServiceCommand::PrintDefinition(
            parse_service_install_options(args, 3)?,
        )),
        _ => Err(format!("unsupported service subcommand '{}'", subcommand).into()),
    }
}

/// Parse shared install-style options from `service install` and `service print-definition`.
/// 从 `service install` 与 `service print-definition` 解析共享安装类选项。
fn parse_service_install_options(
    args: &[String],
    start_index: usize,
) -> Result<ServiceInstallOptions, Box<dyn std::error::Error>> {
    let mut runtime_root = None;
    let mut service_name = DEFAULT_SERVICE_NAME.to_string();
    let mut display_name = None;
    let mut description = None;
    let mut scope = ServiceScope::System;
    let mut startup = ServiceStartup::Auto;
    let mut start_immediately = false;
    let mut force = false;
    let mut cursor = start_index;
    while cursor < args.len() {
        let current = args[cursor].as_str();
        if let Some(value) = current.strip_prefix("--runtime-root=") {
            runtime_root = Some(non_empty_inline_flag_value("--runtime-root", value)?);
            cursor += 1;
            continue;
        }
        if let Some(value) = current.strip_prefix("--service-name=") {
            service_name = non_empty_inline_flag_value("--service-name", value)?;
            cursor += 1;
            continue;
        }
        if let Some(value) = current.strip_prefix("--display-name=") {
            display_name = Some(non_empty_inline_flag_value("--display-name", value)?);
            cursor += 1;
            continue;
        }
        if let Some(value) = current.strip_prefix("--description=") {
            description = Some(non_empty_inline_flag_value("--description", value)?);
            cursor += 1;
            continue;
        }
        if let Some(value) = current.strip_prefix("--scope=") {
            scope = parse_service_scope(value)?;
            cursor += 1;
            continue;
        }
        if let Some(value) = current.strip_prefix("--startup=") {
            startup = parse_service_startup(value)?;
            cursor += 1;
            continue;
        }
        match current {
            "--runtime-root" => {
                runtime_root = Some(require_following_flag_value(
                    args,
                    cursor,
                    "--runtime-root",
                )?);
                cursor += 2;
            }
            "--service-name" => {
                service_name = require_following_flag_value(args, cursor, "--service-name")?;
                cursor += 2;
            }
            "--display-name" => {
                display_name = Some(require_following_flag_value(
                    args,
                    cursor,
                    "--display-name",
                )?);
                cursor += 2;
            }
            "--description" => {
                description = Some(require_following_flag_value(args, cursor, "--description")?);
                cursor += 2;
            }
            "--scope" => {
                scope =
                    parse_service_scope(&require_following_flag_value(args, cursor, "--scope")?)?;
                cursor += 2;
            }
            "--startup" => {
                startup = parse_service_startup(&require_following_flag_value(
                    args,
                    cursor,
                    "--startup",
                )?)?;
                cursor += 2;
            }
            "--start" => {
                start_immediately = true;
                cursor += 1;
            }
            "--force" => {
                force = true;
                cursor += 1;
            }
            value if value.starts_with("--") => {
                return Err(format!("unknown service install flag: {}", value).into());
            }
            value => {
                return Err(format!("unexpected service install argument: {}", value).into());
            }
        }
    }
    Ok(ServiceInstallOptions {
        runtime_root: runtime_root.map(std::path::PathBuf::from),
        service_name,
        display_name,
        description,
        scope,
        startup,
        start_immediately,
        force,
    })
}

/// Parse target-only lifecycle options from `service start/stop/restart/status/uninstall`.
/// 从 `service start/stop/restart/status/uninstall` 解析目标类生命周期选项。
fn parse_service_target_options(
    args: &[String],
    start_index: usize,
) -> Result<ServiceTargetOptions, Box<dyn std::error::Error>> {
    let mut service_name = DEFAULT_SERVICE_NAME.to_string();
    let mut scope = ServiceScope::System;
    let mut force = false;
    let mut cursor = start_index;
    while cursor < args.len() {
        let current = args[cursor].as_str();
        if let Some(value) = current.strip_prefix("--service-name=") {
            service_name = non_empty_inline_flag_value("--service-name", value)?;
            cursor += 1;
            continue;
        }
        if let Some(value) = current.strip_prefix("--scope=") {
            scope = parse_service_scope(value)?;
            cursor += 1;
            continue;
        }
        match current {
            "--service-name" => {
                service_name = require_following_flag_value(args, cursor, "--service-name")?;
                cursor += 2;
            }
            "--scope" => {
                scope =
                    parse_service_scope(&require_following_flag_value(args, cursor, "--scope")?)?;
                cursor += 2;
            }
            "--force" => {
                force = true;
                cursor += 1;
            }
            value if value.starts_with("--") => {
                return Err(format!("unknown service lifecycle flag: {}", value).into());
            }
            value => {
                return Err(format!("unexpected service lifecycle argument: {}", value).into());
            }
        }
    }
    Ok(ServiceTargetOptions {
        service_name,
        scope,
        force,
    })
}

/// Parse the internal service host run options.
/// 解析内部服务宿主运行选项。
fn parse_service_run_options(
    args: &[String],
    start_index: usize,
) -> Result<ServiceRunOptions, Box<dyn std::error::Error>> {
    let mut runtime_root = None;
    let mut service_name = DEFAULT_SERVICE_NAME.to_string();
    let mut cursor = start_index;
    while cursor < args.len() {
        let current = args[cursor].as_str();
        if let Some(value) = current.strip_prefix("--runtime-root=") {
            runtime_root = Some(non_empty_inline_flag_value("--runtime-root", value)?);
            cursor += 1;
            continue;
        }
        if let Some(value) = current.strip_prefix("--service-name=") {
            service_name = non_empty_inline_flag_value("--service-name", value)?;
            cursor += 1;
            continue;
        }
        match current {
            "--runtime-root" => {
                runtime_root = Some(require_following_flag_value(
                    args,
                    cursor,
                    "--runtime-root",
                )?);
                cursor += 2;
            }
            "--service-name" => {
                service_name = require_following_flag_value(args, cursor, "--service-name")?;
                cursor += 2;
            }
            value if value.starts_with("--") => {
                return Err(format!("unknown service run flag: {}", value).into());
            }
            value => {
                return Err(format!("unexpected service run argument: {}", value).into());
            }
        }
    }
    Ok(ServiceRunOptions {
        runtime_root: runtime_root.map(std::path::PathBuf::from),
        service_name,
    })
}

/// Parse one service scope token into the stable cross-platform enum.
/// 将单个服务作用域片段解析为稳定的跨平台枚举。
fn parse_service_scope(value: &str) -> Result<ServiceScope, Box<dyn std::error::Error>> {
    match value.trim().to_ascii_lowercase().as_str() {
        "system" => Ok(ServiceScope::System),
        "user" => Ok(ServiceScope::User),
        _ => Err(format!(
            "unsupported service scope '{}'; expected system or user",
            value
        )
        .into()),
    }
}

/// Parse one service startup token into the stable cross-platform enum.
/// 将单个服务启动策略片段解析为稳定的跨平台枚举。
fn parse_service_startup(value: &str) -> Result<ServiceStartup, Box<dyn std::error::Error>> {
    match value.trim().to_ascii_lowercase().as_str() {
        "auto" => Ok(ServiceStartup::Auto),
        "manual" => Ok(ServiceStartup::Manual),
        _ => Err(format!(
            "unsupported service startup '{}'; expected auto or manual",
            value
        )
        .into()),
    }
}

/// Require one following non-flag value for a service CLI flag.
/// 要求某个服务 CLI 标志后跟随一个非标志值。
fn require_following_flag_value(
    args: &[String],
    index: usize,
    flag: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let value = args
        .get(index + 1)
        .ok_or_else(|| format!("{flag} requires a value"))?;
    if value.starts_with('-') {
        return Err(format!("{flag} requires a value").into());
    }
    Ok(value.clone())
}

/// Reject empty inline `--flag=value` payloads and return the normalized string.
/// 拒绝空的内联 `--flag=value` 载荷，并返回规范化字符串。
fn non_empty_inline_flag_value(
    flag: &str,
    value: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!("{flag} requires a value").into());
    }
    Ok(trimmed.to_string())
}
