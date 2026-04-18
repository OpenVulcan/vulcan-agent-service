use mlua::{Function, Lua, MultiValue, Table, Value as LuaValue};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::client_budget::resolve_client_budget_value;
use crate::lancedb_host::{LanceDbSkillBinding, LanceDbSkillHost, disabled_skill_status_json};
use crate::lua_skill::SkillMeta;
use crate::sqlite_host::{
    SqliteSkillBinding, SqliteSkillHost,
    disabled_skill_status_json as disabled_sqlite_skill_status_json,
};
use crate::protocol::{
    Prompt, PromptArgument, PromptGetResult, PromptMessage, RequestContext, Resource,
    ResourceContents, ResourceReadResult, ResourceTemplate, TextContent, Tool, ToolAnnotations,
};
use crate::skill_dependency::ensure_skill_dependencies;
use crate::temp_maintenance::ensure_runtime_temp_dir;
use crate::tool_config::resolve_tool_config_value;
use crate::tool_cache::global_tool_cache;

// ============================================================
// Loaded skill (compiled Lua function + metadata)
// ============================================================

struct LoadedSkill {
    meta: SkillMeta,
    dir: std::path::PathBuf,
    lancedb_binding: Option<Arc<LanceDbSkillBinding>>,
    sqlite_binding: Option<Arc<SqliteSkillBinding>>,
}

/// Pool sizing configuration for Lua virtual machines.
/// Lua 虚拟机池的容量配置。
#[derive(Debug, Clone, Copy)]
pub struct LuaVmPoolConfig {
    /// Minimum number of VMs that should stay warm.
    /// 需要常驻保温的最小虚拟机数量。
    pub min_size: usize,
    /// Maximum number of VMs allowed in the pool.
    /// 池内允许存在的最大虚拟机数量。
    pub max_size: usize,
    /// Idle TTL in seconds before an excess VM can be retired.
    /// 多余虚拟机在空闲多少秒后允许回收。
    pub idle_ttl_secs: u64,
}

impl LuaVmPoolConfig {
    /// Return a normalized pool config with safe bounds.
    /// 返回经过安全边界归一化后的池配置。
    fn normalized(self) -> Self {
        let min_size = self.min_size.max(1);
        let max_size = self.max_size.max(min_size);
        let idle_ttl_secs = self.idle_ttl_secs.max(1);
        Self {
            min_size,
            max_size,
            idle_ttl_secs,
        }
    }
}

/// Runtime state of a single Lua VM instance.
/// 单个 Lua 虚拟机实例的运行时状态。
struct LuaVm {
    lua: Lua,
    last_used_at: Instant,
}

/// Shared mutable state for the Lua VM pool.
/// Lua 虚拟机池的共享可变状态。
struct LuaVmPoolState {
    available: Vec<LuaVm>,
    total_count: usize,
}

/// Pool of Lua VM instances with opportunistic scaling.
/// 支持按需扩缩容的 Lua 虚拟机池。
struct LuaVmPool {
    config: LuaVmPoolConfig,
    state: Mutex<LuaVmPoolState>,
    condvar: Condvar,
}

// ============================================================
// LuaEngine — LuaJIT VM wrapper
// ============================================================

pub struct LuaEngine {
    skills: HashMap<String, LoadedSkill>,
    pool: Arc<LuaVmPool>,
    lancedb_host: Option<Arc<LanceDbSkillHost>>,
    sqlite_host: Option<Arc<SqliteSkillHost>>,
}

/// Return a stable human-readable Lua value type name.
/// 返回稳定且可读的 Lua 值类型名称。
fn lua_value_type_name(value: &LuaValue) -> &'static str {
    match value {
        LuaValue::Nil => "nil",
        LuaValue::Boolean(_) => "boolean",
        LuaValue::LightUserData(_) => "lightuserdata",
        LuaValue::Integer(_) => "integer",
        LuaValue::Number(_) => "number",
        LuaValue::String(_) => "string",
        LuaValue::Table(_) => "table",
        LuaValue::Function(_) => "function",
        LuaValue::Thread(_) => "thread",
        LuaValue::UserData(_) => "userdata",
        LuaValue::Error(_) => "error",
        LuaValue::Other(_) => "other",
    }
}

/// Detect whether a string looks like Lua's debug-style coercion output.
/// 检测字符串是否像 Lua 对象被 `tostring` 后生成的调试文本。
fn looks_like_lua_debug_value(text: &str) -> bool {
    ["table: 0x", "function: 0x", "thread: 0x", "userdata: 0x"]
        .iter()
        .any(|prefix| text.starts_with(prefix))
}

/// Validate Windows-specific path syntax conservatively before touching the filesystem.
/// 在真正访问文件系统之前，对 Windows 路径语法做保守校验。
#[cfg(windows)]
fn has_invalid_windows_path_syntax(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.starts_with(r"\\?\") {
        return false;
    }

    let first_char = trimmed.chars().next();
    for (index, ch) in trimmed.char_indices() {
        if ch.is_control() {
            return true;
        }
        if matches!(ch, '<' | '>' | '"' | '|' | '?' | '*') {
            return true;
        }
        if ch == ':' {
            let is_drive_prefix =
                index == 1 && first_char.map(|c| c.is_ascii_alphabetic()).unwrap_or(false);
            if !is_drive_prefix {
                return true;
            }
        }
    }
    false
}

/// Require an exact UTF-8 Lua string and reject empty/blank values when needed.
/// 要求参数必须是精确的 UTF-8 Lua 字符串，并在需要时拒绝空值或纯空白值。
fn require_string_arg(
    value: LuaValue,
    fn_name: &str,
    param_name: &str,
    allow_blank: bool,
) -> mlua::Result<String> {
    let raw = match value {
        LuaValue::String(text) => text
            .to_str()
            .map_err(|_| {
                mlua::Error::runtime(format!(
                    "{fn_name}: {param_name} must be a valid UTF-8 string / 参数必须是有效的 UTF-8 字符串"
                ))
            })?
            .to_string(),
        other => {
            return Err(mlua::Error::runtime(format!(
                "{fn_name}: {param_name} must be a string, got {} / 参数必须是字符串，实际为 {}",
                lua_value_type_name(&other),
                lua_value_type_name(&other)
            )))
        }
    };

    if !allow_blank && raw.trim().is_empty() {
        return Err(mlua::Error::runtime(format!(
            "{fn_name}: {param_name} must not be empty / 参数不能为空"
        )));
    }
    if raw.contains('\0') {
        return Err(mlua::Error::runtime(format!(
            "{fn_name}: {param_name} must not contain NUL bytes / 参数不能包含 NUL 字节"
        )));
    }
    Ok(raw)
}

/// Validate path-like text before using it in filesystem operations.
/// 在文件系统函数真正使用路径文本前，先进行统一校验。
fn validate_path_text(text: &str, fn_name: &str, param_name: &str) -> mlua::Result<()> {
    if looks_like_lua_debug_value(text) {
        return Err(mlua::Error::runtime(format!(
            "{fn_name}: {param_name} looks like a coerced Lua object string `{text}` / 参数看起来像被 tostring 强转后的 Lua 对象文本"
        )));
    }

    #[cfg(windows)]
    if has_invalid_windows_path_syntax(text) {
        return Err(mlua::Error::runtime(format!(
            "{fn_name}: {param_name} contains invalid Windows path syntax / 参数包含无效的 Windows 路径语法"
        )));
    }

    Ok(())
}

/// Require a validated path string from Lua input.
/// 从 Lua 输入中提取并校验路径字符串参数。
fn require_path_arg(value: LuaValue, fn_name: &str, param_name: &str) -> mlua::Result<String> {
    let text = require_string_arg(value, fn_name, param_name, false)?;
    validate_path_text(&text, fn_name, param_name)?;
    Ok(text)
}

/// Read an optional non-negative integer argument from Lua.
/// 从 Lua 读取可选的非负整数参数。
fn optional_u64_arg(value: LuaValue, fn_name: &str, param_name: &str) -> mlua::Result<Option<u64>> {
    match value {
        LuaValue::Nil => Ok(None),
        LuaValue::Integer(v) if v >= 0 => Ok(Some(v as u64)),
        LuaValue::Number(v) if v.is_finite() && v >= 0.0 && v.fract() == 0.0 => Ok(Some(v as u64)),
        other => Err(mlua::Error::runtime(format!(
            "{fn_name}: {param_name} must be a non-negative integer / {param_name} 必须是非负整数，当前类型: {}",
            lua_value_type_name(&other)
        ))),
    }
}

/// Require a Lua table argument without silent coercion.
/// 要求参数必须是 Lua table，禁止静默类型转换。
fn require_table_arg(value: LuaValue, fn_name: &str, param_name: &str) -> mlua::Result<Table> {
    match value {
        LuaValue::Table(table) => Ok(table),
        other => Err(mlua::Error::runtime(format!(
            "{fn_name}: {param_name} must be a table, got {} / 参数必须是 table，实际为 {}",
            lua_value_type_name(&other),
            lua_value_type_name(&other)
        ))),
    }
}

/// Execution mode supported by `vulcan.exec`.
/// `vulcan.exec` 支持的执行模式。
enum ExecMode {
    Shell { command: String },
    Program { program: String, args: Vec<String> },
}

/// Parsed process execution request from Lua.
/// 从 Lua 解析得到的进程执行请求。
struct ExecRequest {
    mode: ExecMode,
    cwd: Option<String>,
    env: HashMap<String, String>,
    stdin: Option<String>,
    timeout_ms: Option<u64>,
}

/// Process execution result returned back to Lua.
/// 返回给 Lua 的进程执行结果。
struct ExecResult {
    ok: bool,
    success: bool,
    code: Option<i32>,
    stdout: String,
    stderr: String,
    timed_out: bool,
    error: Option<String>,
}

/// Require a scalar text-like value for exec arguments and environment values.
/// 为 exec 的参数和环境变量值提取标量文本，拒绝 table/function 等复杂类型。
fn require_exec_scalar_text(
    value: LuaValue,
    fn_name: &str,
    param_name: &str,
    allow_blank: bool,
) -> mlua::Result<String> {
    match value {
        LuaValue::String(_) => require_string_arg(value, fn_name, param_name, allow_blank),
        LuaValue::Integer(number) => Ok(number.to_string()),
        LuaValue::Number(number) => {
            if !number.is_finite() {
                return Err(mlua::Error::runtime(format!(
                    "{fn_name}: {param_name} must be a finite number / 参数必须是有限数值"
                )));
            }
            Ok(number.to_string())
        }
        LuaValue::Boolean(flag) => Ok(flag.to_string()),
        other => Err(mlua::Error::runtime(format!(
            "{fn_name}: {param_name} must be a string/number/boolean, got {} / 参数必须是字符串、数字或布尔值，实际为 {}",
            lua_value_type_name(&other),
            lua_value_type_name(&other)
        ))),
    }
}

/// Read an optional string field from a Lua table with strict validation.
/// 从 Lua table 中读取可选字符串字段，并执行严格校验。
fn table_get_optional_string_field(
    table: &Table,
    fn_name: &str,
    field_name: &str,
    allow_blank: bool,
) -> mlua::Result<Option<String>> {
    let value: LuaValue = table.get(field_name)?;
    match value {
        LuaValue::Nil => Ok(None),
        other => Ok(Some(require_string_arg(
            other,
            fn_name,
            field_name,
            allow_blank,
        )?)),
    }
}

/// Read an optional boolean field from a Lua table.
/// 从 Lua table 中读取可选布尔字段。
fn table_get_optional_bool_field(
    table: &Table,
    fn_name: &str,
    field_name: &str,
) -> mlua::Result<Option<bool>> {
    let value: LuaValue = table.get(field_name)?;
    match value {
        LuaValue::Nil => Ok(None),
        LuaValue::Boolean(flag) => Ok(Some(flag)),
        other => Err(mlua::Error::runtime(format!(
            "{fn_name}: {field_name} must be a boolean when provided / 提供时必须是布尔值，实际为 {}",
            lua_value_type_name(&other)
        ))),
    }
}

/// Read an optional timeout field in milliseconds from a Lua table.
/// 从 Lua table 中读取可选的毫秒级超时字段。
fn table_get_optional_timeout_field(
    table: &Table,
    fn_name: &str,
    field_name: &str,
) -> mlua::Result<Option<u64>> {
    let value: LuaValue = table.get(field_name)?;
    match value {
        LuaValue::Nil => Ok(None),
        LuaValue::Integer(number) if number > 0 => Ok(Some(number as u64)),
        LuaValue::Number(number) if number.is_finite() && number.fract() == 0.0 && number > 0.0 => {
            Ok(Some(number as u64))
        }
        other => Err(mlua::Error::runtime(format!(
            "{fn_name}: {field_name} must be a positive integer in milliseconds / 必须是正整数毫秒值，实际为 {}",
            lua_value_type_name(&other)
        ))),
    }
}

/// Read an optional string-like array field from a Lua table.
/// 从 Lua table 中读取可选的字符串类数组字段。
fn table_get_string_list_field(
    table: &Table,
    fn_name: &str,
    field_name: &str,
) -> mlua::Result<Vec<String>> {
    let value: LuaValue = table.get(field_name)?;
    match value {
        LuaValue::Nil => Ok(Vec::new()),
        other => {
            let list = require_table_arg(other, fn_name, field_name)?;
            let mut items = Vec::new();
            for (index, item) in list.sequence_values::<LuaValue>().enumerate() {
                let item = item.map_err(|error| {
                    mlua::Error::runtime(format!(
                        "{fn_name}: failed to read {field_name}[{}] / 读取 {}[{}] 失败: {}",
                        index + 1,
                        field_name,
                        index + 1,
                        error
                    ))
                })?;
                items.push(require_exec_scalar_text(
                    item,
                    fn_name,
                    &format!("{field_name}[{}]", index + 1),
                    true,
                )?);
            }
            Ok(items)
        }
    }
}

/// Read an optional string map field from a Lua table.
/// 从 Lua table 中读取可选的字符串映射字段。
fn table_get_string_map_field(
    table: &Table,
    fn_name: &str,
    field_name: &str,
) -> mlua::Result<HashMap<String, String>> {
    let value: LuaValue = table.get(field_name)?;
    match value {
        LuaValue::Nil => Ok(HashMap::new()),
        other => {
            let map_table = require_table_arg(other, fn_name, field_name)?;
            let mut items = HashMap::new();
            for pair in map_table.pairs::<LuaValue, LuaValue>() {
                let (key_value, field_value) = pair.map_err(|error| {
                    mlua::Error::runtime(format!(
                        "{fn_name}: failed to read {field_name} / 读取 {field_name} 失败: {error}"
                    ))
                })?;
                let key =
                    require_string_arg(key_value, fn_name, &format!("{field_name}.<key>"), false)?;
                let value_text = require_exec_scalar_text(
                    field_value,
                    fn_name,
                    &format!("{field_name}.{key}"),
                    true,
                )?;
                items.insert(key, value_text);
            }
            Ok(items)
        }
    }
}

/// Parse Lua input into an executable process request.
/// 将 Lua 输入解析为可执行的进程请求。
fn parse_exec_request(value: LuaValue, fn_name: &str) -> mlua::Result<ExecRequest> {
    match value {
        LuaValue::String(command_text) => Ok(ExecRequest {
            mode: ExecMode::Shell {
                command: require_string_arg(
                    LuaValue::String(command_text),
                    fn_name,
                    "command",
                    false,
                )?,
            },
            cwd: None,
            env: HashMap::new(),
            stdin: None,
            timeout_ms: None,
        }),
        LuaValue::Table(spec) => {
            let command = table_get_optional_string_field(&spec, fn_name, "command", false)?;
            let program = table_get_optional_string_field(&spec, fn_name, "program", false)?;
            let args = table_get_string_list_field(&spec, fn_name, "args")?;
            let cwd = table_get_optional_string_field(&spec, fn_name, "cwd", false)?;
            let env = table_get_string_map_field(&spec, fn_name, "env")?;
            let stdin = table_get_optional_string_field(&spec, fn_name, "stdin", true)?;
            let timeout_ms = table_get_optional_timeout_field(&spec, fn_name, "timeout_ms")?;
            let shell_override = table_get_optional_bool_field(&spec, fn_name, "shell")?;

            if let Some(current_dir) = cwd.as_deref() {
                validate_path_text(current_dir, fn_name, "cwd")?;
            }

            let mode = match (command, program) {
                (Some(command_text), None) => {
                    if matches!(shell_override, Some(false)) {
                        return Err(mlua::Error::runtime(format!(
                            "{fn_name}: shell=false cannot be used with command mode / command 模式不能与 shell=false 同时使用"
                        )));
                    }
                    if !args.is_empty() {
                        return Err(mlua::Error::runtime(format!(
                            "{fn_name}: args is only supported with program mode / args 仅支持 program 模式"
                        )));
                    }
                    ExecMode::Shell {
                        command: command_text,
                    }
                }
                (None, Some(program_path)) => {
                    if matches!(shell_override, Some(true)) {
                        return Err(mlua::Error::runtime(format!(
                            "{fn_name}: shell=true requires command mode / shell=true 需要 command 模式"
                        )));
                    }
                    ExecMode::Program {
                        program: program_path,
                        args,
                    }
                }
                (Some(_), Some(_)) => {
                    return Err(mlua::Error::runtime(format!(
                        "{fn_name}: command and program are mutually exclusive / command 与 program 不能同时提供"
                    )));
                }
                (None, None) => {
                    return Err(mlua::Error::runtime(format!(
                        "{fn_name}: expected a string command or a table with command/program / 需要字符串命令或包含 command/program 的 table"
                    )));
                }
            };

            Ok(ExecRequest {
                mode,
                cwd,
                env,
                stdin,
                timeout_ms,
            })
        }
        other => Err(mlua::Error::runtime(format!(
            "{fn_name}: expected a string or table, got {} / 需要字符串或 table，实际为 {}",
            lua_value_type_name(&other),
            lua_value_type_name(&other)
        ))),
    }
}

/// Return the default shell program and command flag for the current platform.
/// 返回当前平台默认的 shell 程序及命令参数开关。
#[cfg(windows)]
fn default_shell_launcher() -> (&'static str, &'static str) {
    ("cmd.exe", "/C")
}

/// Return the default shell program and command flag for the current platform.
/// 返回当前平台默认的 shell 程序及命令参数开关。
#[cfg(not(windows))]
fn default_shell_launcher() -> (&'static str, &'static str) {
    ("sh", "-c")
}

/// Spawn a background reader for a child process output pipe.
/// 为子进程输出管道启动后台读取线程。
fn spawn_pipe_reader<R>(mut reader: R) -> thread::JoinHandle<String>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = reader.read_to_end(&mut buffer);
        String::from_utf8_lossy(&buffer).to_string()
    })
}

/// Spawn a background writer for a child process stdin pipe.
/// 为子进程标准输入管道启动后台写入线程。
fn spawn_stdin_writer<W>(mut writer: W, input: String) -> thread::JoinHandle<()>
where
    W: Write + Send + 'static,
{
    thread::spawn(move || {
        let _ = writer.write_all(input.as_bytes());
        let _ = writer.flush();
    })
}

/// Execute a process request and capture its structured result.
/// 执行进程请求并捕获结构化结果。
fn execute_exec_request(request: ExecRequest) -> ExecResult {
    let mut command = match &request.mode {
        ExecMode::Shell { command } => {
            let (shell_program, shell_flag) = default_shell_launcher();
            let mut process = Command::new(shell_program);
            process.arg(shell_flag).arg(command);
            process
        }
        ExecMode::Program { program, args } => {
            let mut process = Command::new(program);
            process.args(args);
            process
        }
    };

    if let Some(current_dir) = &request.cwd {
        command.current_dir(current_dir);
    }
    if !request.env.is_empty() {
        command.envs(&request.env);
    }
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    command.stdin(if request.stdin.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    });

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            let error_text = format!(
                "failed to spawn process: {} / 启动进程失败: {}",
                error, error
            );
            return ExecResult {
                ok: false,
                success: false,
                code: None,
                stdout: String::new(),
                stderr: error_text.clone(),
                timed_out: false,
                error: Some(error_text),
            };
        }
    };

    let stdout_handle = child.stdout.take().map(spawn_pipe_reader);
    let stderr_handle = child.stderr.take().map(spawn_pipe_reader);
    let stdin_handle = match (request.stdin.clone(), child.stdin.take()) {
        (Some(input), Some(stdin)) => Some(spawn_stdin_writer(stdin, input)),
        _ => None,
    };

    let mut timed_out = false;
    let timeout = request.timeout_ms.map(Duration::from_millis);
    let started_at = Instant::now();

    let final_status = loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                break Some(status);
            }
            Ok(None) => {
                if let Some(limit) = timeout {
                    if started_at.elapsed() >= limit {
                        timed_out = true;
                        let _ = child.kill();
                        break child.wait().ok();
                    }
                }
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => {
                let error_text = format!(
                    "failed to wait for process: {} / 等待进程结束失败: {}",
                    error, error
                );
                return ExecResult {
                    ok: false,
                    success: false,
                    code: None,
                    stdout: String::new(),
                    stderr: error_text.clone(),
                    timed_out,
                    error: Some(error_text),
                };
            }
        }
    };

    if let Some(handle) = stdin_handle {
        let _ = handle.join();
    }

    let stdout = stdout_handle
        .map(|handle| handle.join().unwrap_or_default())
        .unwrap_or_default();
    let mut stderr = stderr_handle
        .map(|handle| handle.join().unwrap_or_default())
        .unwrap_or_default();

    let status = match final_status {
        Some(status) => status,
        None => {
            let error_text = "process finished without status / 进程结束时缺少状态信息".to_string();
            return ExecResult {
                ok: false,
                success: false,
                code: None,
                stdout,
                stderr: error_text.clone(),
                timed_out,
                error: Some(error_text),
            };
        }
    };

    let code = status.code();
    let success = !timed_out && status.success();
    let mut error = None;

    if timed_out {
        let timeout_value = request.timeout_ms.unwrap_or_default();
        let timeout_text = format!(
            "process execution timed out after {} ms / 进程执行超时（{} 毫秒）",
            timeout_value, timeout_value
        );
        if !stderr.is_empty() {
            stderr.push('\n');
        }
        stderr.push_str(&timeout_text);
        error = Some(timeout_text);
    } else if !success {
        error = Some(match code {
            Some(exit_code) => format!(
                "process exited with code {} / 进程以退出码 {} 结束",
                exit_code, exit_code
            ),
            None => "process terminated without an exit code / 进程结束时没有退出码".to_string(),
        });
    }

    ExecResult {
        ok: success,
        success,
        code,
        stdout,
        stderr,
        timed_out,
        error,
    }
}

/// Convert an exec result into a Lua table for skill consumption.
/// 将 exec 结果转换为供 skill 消费的 Lua table。
fn exec_result_to_lua_table(lua: &Lua, result: ExecResult) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("ok", result.ok)?;
    table.set("success", result.success)?;
    table.set("stdout", result.stdout)?;
    table.set("stderr", result.stderr)?;
    table.set("timed_out", result.timed_out)?;
    match result.code {
        Some(code) => table.set("code", code)?,
        None => table.set("code", LuaValue::Nil)?,
    }
    match result.error {
        Some(error_text) => table.set("error", error_text)?,
        None => table.set("error", LuaValue::Nil)?,
    }
    Ok(table)
}

/// Convert a JSON value into a best-effort template string.
/// 将 JSON 值尽量转换为适合模板替换的字符串。
fn json_value_to_template_text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number.to_string(),
        Value::Array(items) => items
            .iter()
            .map(json_value_to_template_text)
            .collect::<Vec<_>>()
            .join(", "),
        Value::Object(_) => serde_json::to_string(value).unwrap_or_default(),
    }
}

/// Read a UTF-8 text file relative to the skill directory.
/// 读取相对于技能目录的 UTF-8 文本文件。
fn read_skill_text_file(
    skill_dir: &Path,
    relative_path: &str,
    label: &str,
) -> Result<String, String> {
    let file_path = skill_dir.join(relative_path);
    std::fs::read_to_string(&file_path).map_err(|error| {
        format!(
            "Failed to read {label} file {}: {}",
            file_path.display(),
            error
        )
    })
}

/// Return whether the skill file should be treated as a Lua generator.
/// 判断 skill 文件是否应按 Lua 生成器执行。
fn is_lua_provider_file(relative_path: &str) -> bool {
    Path::new(relative_path)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("lua"))
        .unwrap_or(false)
}

/// Replace `{{key}}` placeholders using the provided variable map.
/// 使用变量映射替换 `{{key}}` 占位符。
fn apply_text_template(template_text: &str, variables: &serde_json::Map<String, Value>) -> String {
    let mut rendered = template_text.to_string();
    for (key, value) in variables {
        let placeholder = format!("{{{{{key}}}}}");
        rendered = rendered.replace(&placeholder, &json_value_to_template_text(value));
    }
    rendered
}

/// Normalize a JSON value returned from a skill generator into ResourceReadResult.
/// 将技能生成器返回的 JSON 值标准化为 ResourceReadResult。
fn normalize_resource_result(
    value: Value,
    fallback_uri: &str,
) -> Result<ResourceReadResult, String> {
    if let Ok(result) = serde_json::from_value::<ResourceReadResult>(value.clone()) {
        return Ok(result);
    }

    match value {
        Value::String(text) => Ok(ResourceReadResult {
            contents: vec![ResourceContents::text(fallback_uri, &text, None)],
        }),
        Value::Object(map) => {
            let text = map.get("text").and_then(|value| value.as_str());
            let blob = map.get("blob").and_then(|value| value.as_str());
            if text.is_none() && blob.is_none() {
                return Err(
                    "Resource generator must return ResourceReadResult or an object with text/blob"
                        .to_string(),
                );
            }
            let uri = map
                .get("uri")
                .and_then(|value| value.as_str())
                .unwrap_or(fallback_uri)
                .to_string();
            let mime_type = map
                .get("mime_type")
                .or_else(|| map.get("mimeType"))
                .and_then(|value| value.as_str())
                .map(|value| value.to_string());
            Ok(ResourceReadResult {
                contents: vec![ResourceContents {
                    uri,
                    mime_type,
                    text: text.map(|value| value.to_string()),
                    blob: blob.map(|value| value.to_string()),
                }],
            })
        }
        _ => Err("Resource generator returned an unsupported value".to_string()),
    }
}

/// Normalize a JSON value returned from a skill generator into PromptGetResult.
/// 将技能生成器返回的 JSON 值标准化为 PromptGetResult。
fn normalize_prompt_result(value: Value, fallback_role: &str) -> Result<PromptGetResult, String> {
    if let Ok(result) = serde_json::from_value::<PromptGetResult>(value.clone()) {
        return Ok(result);
    }

    match value {
        Value::String(text) => Ok(PromptGetResult {
            description: None,
            messages: vec![PromptMessage {
                role: fallback_role.to_string(),
                content: TextContent::text(&text),
            }],
        }),
        Value::Object(map) => {
            let description = map
                .get("description")
                .and_then(|value| value.as_str())
                .map(|value| value.to_string());
            let messages_value = map.get("messages").cloned();
            if let Some(messages_value) = messages_value {
                let messages: Vec<PromptMessage> =
                    serde_json::from_value(messages_value).map_err(|error| {
                        format!("Prompt generator messages shape is invalid: {}", error)
                    })?;
                return Ok(PromptGetResult {
                    description,
                    messages,
                });
            }

            if let Some(text) = map.get("text").and_then(|value| value.as_str()) {
                let role = map
                    .get("role")
                    .and_then(|value| value.as_str())
                    .unwrap_or(fallback_role)
                    .to_string();
                return Ok(PromptGetResult {
                    description,
                    messages: vec![PromptMessage {
                        role,
                        content: TextContent::text(text),
                    }],
                });
            }

            Err("Prompt generator must return PromptGetResult, {messages}, or {text}".to_string())
        }
        _ => Err("Prompt generator returned an unsupported value".to_string()),
    }
}

/// Checked-out VM guard that returns the VM back into the pool on drop.
/// 已借出的虚拟机守卫，在释放时会自动归还到池中。
struct LuaVmLease {
    pool: Arc<LuaVmPool>,
    vm: Option<LuaVm>,
}

impl LuaVmLease {
    /// Borrow the underlying Lua VM immutably for the duration of the lease.
    /// 在租约生命周期内以只读方式借用底层 Lua 虚拟机。
    fn lua(&self) -> &Lua {
        &self.vm.as_ref().expect("lua vm lease missing instance").lua
    }
}

impl Drop for LuaVmLease {
    fn drop(&mut self) {
        if let Some(mut vm) = self.vm.take() {
            vm.last_used_at = Instant::now();
            self.pool.release(vm);
        }
    }
}

impl LuaVmPool {
    /// Create a new empty Lua VM pool.
    /// 创建一个新的空 Lua 虚拟机池。
    fn new(config: LuaVmPoolConfig) -> Self {
        Self {
            config: config.normalized(),
            state: Mutex::new(LuaVmPoolState {
                available: Vec::new(),
                total_count: 0,
            }),
            condvar: Condvar::new(),
        }
    }

    /// Prewarm the pool to the configured minimum size.
    /// 预热到配置要求的最小虚拟机数量。
    fn prewarm<F>(&self, mut factory: F) -> Result<(), String>
    where
        F: FnMut() -> Result<LuaVm, String>,
    {
        while self.total_count() < self.config.min_size {
            {
                let mut state = self.state.lock().unwrap();
                state.total_count += 1;
            }
            match factory() {
                Ok(vm) => self.release(vm),
                Err(error) => {
                    let mut state = self.state.lock().unwrap();
                    state.total_count = state.total_count.saturating_sub(1);
                    return Err(error);
                }
            }
        }
        Ok(())
    }

    /// Acquire a VM from the pool, growing on demand up to the configured limit.
    /// 从池中获取虚拟机，并在未达上限时按需扩容。
    fn acquire<F>(self: &Arc<Self>, mut factory: F) -> Result<LuaVmLease, String>
    where
        F: FnMut() -> Result<LuaVm, String>,
    {
        loop {
            let mut state = self.state.lock().unwrap();
            self.reap_idle_locked(&mut state);

            if let Some(mut vm) = state.available.pop() {
                vm.last_used_at = Instant::now();
                return Ok(LuaVmLease {
                    pool: self.clone(),
                    vm: Some(vm),
                });
            }

            if state.total_count < self.config.max_size {
                state.total_count += 1;
                drop(state);
                match factory() {
                    Ok(vm) => {
                        return Ok(LuaVmLease {
                            pool: self.clone(),
                            vm: Some(vm),
                        });
                    }
                    Err(error) => {
                        let mut state = self.state.lock().unwrap();
                        state.total_count = state.total_count.saturating_sub(1);
                        self.condvar.notify_one();
                        return Err(error);
                    }
                }
            }

            let _guard = self.condvar.wait(state).unwrap();
        }
    }

    /// Return a VM back into the pool.
    /// 将虚拟机归还到池中。
    fn release(&self, vm: LuaVm) {
        let mut state = self.state.lock().unwrap();
        state.available.push(vm);
        self.reap_idle_locked(&mut state);
        self.condvar.notify_one();
    }

    /// Return the current total number of VMs in the pool.
    /// 返回当前池中的虚拟机总数。
    fn total_count(&self) -> usize {
        self.state.lock().unwrap().total_count
    }

    /// Reap idle available VMs while respecting the minimum pool size.
    /// 在保证最小池规模的前提下回收空闲虚拟机。
    fn reap_idle_locked(&self, state: &mut LuaVmPoolState) {
        if state.total_count <= self.config.min_size {
            return;
        }

        let idle_limit = Duration::from_secs(self.config.idle_ttl_secs);
        let now = Instant::now();
        let mut index = 0usize;
        while index < state.available.len() && state.total_count > self.config.min_size {
            let should_remove = now
                .checked_duration_since(state.available[index].last_used_at)
                .map(|idle| idle >= idle_limit)
                .unwrap_or(false);
            if should_remove {
                state.available.swap_remove(index);
                state.total_count = state.total_count.saturating_sub(1);
            } else {
                index += 1;
            }
        }
    }
}

/// Parse an MCP URI template and extract placeholder values from a concrete URI.
/// 解析 MCP URI 模板，并从具体 URI 中提取占位符参数。
fn match_uri_template(uri_template: &str, uri: &str) -> Option<serde_json::Map<String, Value>> {
    #[derive(Debug)]
    enum Segment {
        Literal(String),
        Variable(String),
    }

    let mut segments = Vec::new();
    let mut cursor = 0usize;
    while cursor < uri_template.len() {
        let remaining = &uri_template[cursor..];
        if let Some(open_offset) = remaining.find('{') {
            let open_index = cursor + open_offset;
            if open_index > cursor {
                segments.push(Segment::Literal(
                    uri_template[cursor..open_index].to_string(),
                ));
            }
            let after_open = open_index + 1;
            let close_offset = uri_template[after_open..].find('}')?;
            let close_index = after_open + close_offset;
            let variable_name = uri_template[after_open..close_index].trim();
            if variable_name.is_empty() {
                return None;
            }
            segments.push(Segment::Variable(variable_name.to_string()));
            cursor = close_index + 1;
        } else {
            segments.push(Segment::Literal(uri_template[cursor..].to_string()));
            break;
        }
    }

    let mut values = serde_json::Map::new();
    let mut uri_cursor = 0usize;
    for (index, segment) in segments.iter().enumerate() {
        match segment {
            Segment::Literal(literal) => {
                if !uri[uri_cursor..].starts_with(literal) {
                    return None;
                }
                uri_cursor += literal.len();
            }
            Segment::Variable(name) => {
                let next_literal =
                    segments[index + 1..]
                        .iter()
                        .find_map(|candidate| match candidate {
                            Segment::Literal(text) if !text.is_empty() => Some(text.as_str()),
                            _ => None,
                        });
                let end_index = if let Some(next_text) = next_literal {
                    let relative_end = uri[uri_cursor..].find(next_text)?;
                    uri_cursor + relative_end
                } else {
                    uri.len()
                };
                let captured = &uri[uri_cursor..end_index];
                values.insert(name.clone(), Value::String(captured.to_string()));
                uri_cursor = end_index;
            }
        }
    }

    if uri_cursor == uri.len() {
        Some(values)
    } else {
        None
    }
}

impl LuaEngine {
    /// Create a new LuaEngine with LuaJIT VM and registered globals.
    pub fn new(pool_config: LuaVmPoolConfig) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            skills: HashMap::new(),
            pool: Arc::new(LuaVmPool::new(pool_config)),
            lancedb_host: None,
            sqlite_host: None,
        })
    }

    /// Load skills from directories. `base_dir` is the system skill directory,
    /// `override_dir` is the user override directory (if any).
    pub fn load_from_dirs(
        &mut self,
        base_dir: &Path,
        override_dir: Option<&Path>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if !base_dir.exists() {
            return Ok(());
        }

        for entry in std::fs::read_dir(base_dir)? {
            let entry = entry?;
            let skill_dir = entry.path();
            if !skill_dir.is_dir() {
                continue;
            }

            let skill_name = entry.file_name().to_string_lossy().to_string();
            if skill_name.starts_with("__") {
                continue;
            }

            // Check override directory
            let actual_dir = if let Some(od) = override_dir {
                let override_skill_dir = od.join(&skill_name);
                if override_skill_dir.exists() {
                    // Empty directory = disable this skill
                    if override_skill_dir.read_dir()?.next().is_none() {
                        eprintln!("[LuaSkill] Disabled by empty override: {}", skill_name);
                        continue;
                    }
                    eprintln!("[LuaSkill] Override loaded: {}", skill_name);
                    override_skill_dir
                } else {
                    eprintln!("[LuaSkill] System loaded: {}", skill_name);
                    skill_dir
                }
            } else {
                eprintln!("[LuaSkill] System loaded: {}", skill_name);
                skill_dir
            };

            if let Err(e) = self.load_single_skill(&actual_dir) {
                eprintln!("[LuaSkill] Failed to load {}: {}", skill_name, e);
            }
        }

        self.pool
            .prewarm(|| self.create_vm())
            .map_err(|error| -> Box<dyn std::error::Error> { error.into() })?;

        eprintln!("[LuaSkill] {} skills loaded", self.skills.len());
        Ok(())
    }

    /// Load a single skill from its directory.
    fn load_single_skill(&mut self, dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
        let skill_json = dir.join("skill.json");
        if !skill_json.exists() {
            return Err(format!("skill.json not found in {}", dir.display()).into());
        }

        let json_str = std::fs::read_to_string(&skill_json)?;
        let meta: SkillMeta = serde_json::from_str(&json_str)?;

        if meta.groups.is_empty() {
            return Err(format!("skill {} must declare at least one group", meta.name).into());
        }

        // Ensure declared external dependencies are installed before Lua entry validation.
        // 在校验 Lua 入口之前，先确保 skill 声明的外部依赖已经安装完成。
        ensure_skill_dependencies(dir)?;

        for tool in meta.tools() {
            if tool.lua_entry.trim().is_empty() || tool.lua_module.trim().is_empty() {
                return Err(format!(
                    "skill {} declares tool {} but lua_entry/lua_module is missing",
                    meta.name, tool.name
                )
                .into());
            }

            let lua_path = dir.join(&tool.lua_entry);
            if !lua_path.exists() {
                return Err(format!(
                    "Lua entry {} not found in {}",
                    tool.lua_entry,
                    dir.display()
                )
                .into());
            }
        }

        let effective_lancedb = meta.effective_lancedb();
        let lancedb_binding = if effective_lancedb.enable {
            if self.lancedb_host.is_none() {
                self.lancedb_host = Some(Arc::new(LanceDbSkillHost::new().map_err(|error| {
                    format!(
                        "Failed to initialize LanceDB skill host / 初始化 LanceDB skill 宿主失败: {}",
                        error
                    )
                })?));
            }

            let host = self
                .lancedb_host
                .as_ref()
                .ok_or("LanceDB skill host missing after initialization / LanceDB skill 宿主初始化后丢失")?
                .clone();

            Some(
                host.register_skill(&meta.name, dir, effective_lancedb)
                    .map_err(|error| {
                        format!(
                            "Failed to register LanceDB for skill {} / 为 skill 注册 LanceDB 失败: {}",
                            meta.name, error
                        )
                    })?,
            )
        } else {
            None
        };

        let effective_sqlite = meta.effective_sqlite();
        let sqlite_binding = if effective_sqlite.enable {
            if self.sqlite_host.is_none() {
                self.sqlite_host = Some(Arc::new(SqliteSkillHost::new().map_err(|error| {
                    format!(
                        "Failed to initialize SQLite skill host / 初始化 SQLite skill 宿主失败: {}",
                        error
                    )
                })?));
            }

            let host = self
                .sqlite_host
                .as_ref()
                .ok_or("SQLite skill host missing after initialization / SQLite skill 宿主初始化后丢失")?
                .clone();

            Some(
                host.register_skill(&meta.name, dir, effective_sqlite)
                    .map_err(|error| {
                        format!(
                            "Failed to register SQLite for skill {} / 为 skill 注册 SQLite 失败: {}",
                            meta.name, error
                        )
                    })?,
            )
        } else {
            None
        };

        self.skills.insert(
            meta.name.clone(),
            LoadedSkill {
                meta,
                dir: dir.to_path_buf(),
                lancedb_binding,
                sqlite_binding,
            },
        );

        Ok(())
    }

    /// Build a fresh Lua VM instance with all loaded skills registered.
    /// 创建一个全新的 Lua 虚拟机实例，并注册当前已加载的全部技能。
    fn create_vm(&self) -> Result<LuaVm, String> {
        let lua = unsafe { Lua::unsafe_new() };
        Self::setup_package_paths(&lua).map_err(|error| error.to_string())?;
        Self::register_vulcan_module(&lua).map_err(|error| error.to_string())?;
        Self::register_skill_functions(&lua, &self.skills)?;
        Self::populate_vulcan_call_for_lua(
            &lua,
            &self.skills,
            self.lancedb_host.clone(),
            self.sqlite_host.clone(),
        )?;
        Ok(LuaVm {
            lua,
            last_used_at: Instant::now(),
        })
    }

    /// Borrow a Lua VM from the pool for one operation.
    /// 从虚拟机池借出一个 Lua 实例执行一次操作。
    fn acquire_vm(&self) -> Result<LuaVmLease, String> {
        self.pool.acquire(|| self.create_vm())
    }

    /// Register all tool-bearing skill entries into a specific Lua VM.
    /// 将所有声明了工具入口的 skill 条目注册到指定 Lua 虚拟机中。
    fn register_skill_functions(
        lua: &Lua,
        skills: &HashMap<String, LoadedSkill>,
    ) -> Result<(), String> {
        for skill in skills.values() {
            for tool in skill.meta.tools() {
                Self::compile_skill_into_lua(lua, skill, tool, false)?;
            }
        }
        Ok(())
    }

    /// Compile one tool entry into the target Lua VM.
    /// 将单个工具入口编译并注册到目标 Lua 虚拟机中。
    fn compile_skill_into_lua(
        lua: &Lua,
        skill: &LoadedSkill,
        tool: &crate::lua_skill::SkillToolMeta,
        always_reload: bool,
    ) -> Result<(), String> {
        let lua_path = skill.dir.join(&tool.lua_entry);
        let source = std::fs::read_to_string(&lua_path)
            .map_err(|error| format!("Failed to read {}: {}", lua_path.display(), error))?;
        if always_reload {
            eprintln!(
                "[LuaSkill] Hot reload {}: {}",
                tool.lua_module,
                lua_path.display()
            );
        }

        lua.globals()
            .set(
                format!("__skill_dir_{}", tool.lua_module),
                skill.dir.to_string_lossy().to_string(),
            )
            .map_err(|error| {
                format!(
                    "Failed to set skill dir for {}::{}: {}",
                    skill.meta.name, tool.name, error
                )
            })?;

        let chunk = lua.load(&source).set_name(&tool.lua_module);
        let outer: Function = chunk.into_function().map_err(|error| {
            format!(
                "Failed to compile skill '{}::{}': {}",
                skill.meta.name, tool.lua_module, error
            )
        })?;
        let handler: Function = outer.call(()).map_err(|error| {
            format!(
                "Failed to initialize skill '{}::{}': {}",
                skill.meta.name, tool.lua_module, error
            )
        })?;
        lua.globals()
            .set(format!("__skill_{}", tool.lua_module), handler)
            .map_err(|error| {
                format!(
                    "Failed to register skill '{}::{}': {}",
                    skill.meta.name, tool.lua_module, error
                )
            })?;
        Ok(())
    }

    /// Return MCP Tool definitions for all loaded skills.
    pub fn list_skills(&self) -> Vec<Tool> {
        self.skills
            .values()
            .flat_map(|skill| {
                skill.meta.tools().map(|tool| {
                    let mut desc = tool.description.clone();
                    if !tool.prompt.is_empty() {
                        if !desc.is_empty() {
                            desc.push_str("\n\n");
                        }
                        desc.push_str(&tool.prompt);
                    }

                    let mut props = serde_json::Map::new();
                    let mut required = Vec::new();
                    for parameter in &tool.parameters {
                        let mut prop = serde_json::Map::new();
                        prop.insert(
                            "type".to_string(),
                            Value::String(parameter.param_type.clone()),
                        );
                        prop.insert(
                            "description".to_string(),
                            Value::String(parameter.description.clone()),
                        );
                        props.insert(parameter.name.clone(), Value::Object(prop));
                        if parameter.required {
                            required.push(parameter.name.clone());
                        }
                    }

                    Tool::with_annotations(
                        &tool.name,
                        &desc,
                        Value::Object(props),
                        required,
                        ToolAnnotations {
                            read_only_hint: Some(true),
                            destructive_hint: Some(false),
                            user_confirmation_required: Some(false),
                            idempotent_hint: Some(true),
                        },
                    )
                })
            })
            .collect()
    }

    /// Return MCP Resource definitions declared by all loaded skills.
    /// 返回所有已加载技能声明的 MCP Resource 定义。
    pub fn list_resources(&self) -> Vec<Resource> {
        self.skills
            .values()
            .flat_map(|skill| {
                skill.meta.resources().map(|resource| Resource {
                    uri: resource.uri.clone(),
                    name: resource.name.clone(),
                    description: resource.description.clone(),
                    mime_type: resource.mime_type.clone(),
                    size: resource.size,
                })
            })
            .collect()
    }

    /// Return MCP ResourceTemplate definitions declared by all loaded skills.
    /// 返回所有已加载技能声明的 MCP ResourceTemplate 定义。
    pub fn list_resource_templates(&self) -> Vec<ResourceTemplate> {
        self.skills
            .values()
            .flat_map(|skill| {
                skill
                    .meta
                    .resource_templates()
                    .map(|template| ResourceTemplate {
                        uri_template: template.uri_template.clone(),
                        name: template.name.clone(),
                        description: template.description.clone(),
                        mime_type: template.mime_type.clone(),
                    })
            })
            .collect()
    }

    /// Return MCP Prompt definitions declared by all loaded skills.
    /// 返回所有已加载技能声明的 MCP Prompt 定义。
    pub fn list_prompts(&self) -> Vec<Prompt> {
        self.skills
            .values()
            .flat_map(|skill| {
                skill.meta.prompts().map(|prompt| Prompt {
                    name: prompt.name.clone(),
                    description: prompt.description.clone(),
                    arguments: Some(
                        prompt
                            .arguments
                            .iter()
                            .map(|argument| PromptArgument {
                                name: argument.name.clone(),
                                description: argument.description.clone(),
                                required: Some(argument.required),
                            })
                            .collect(),
                    ),
                })
            })
            .collect()
    }

    /// Return configured completion candidates for a prompt argument, if declared by a skill.
    /// 返回某个提示词参数在 skill 元数据中声明的候选补全项。
    pub fn prompt_argument_completions(
        &self,
        prompt_name: &str,
        argument_name: &str,
    ) -> Option<Vec<String>> {
        self.skills.values().find_map(|skill| {
            let (_, prompt) = skill.meta.find_prompt_with_group(prompt_name)?;
            let argument = prompt
                .arguments
                .iter()
                .find(|argument| argument.name == argument_name)?;
            if argument.completions.is_empty() {
                None
            } else {
                Some(argument.completions.clone())
            }
        })
    }

    /// Check if a tool_name is a Lua skill.
    pub fn is_skill(&self, name: &str) -> bool {
        self.skills
            .values()
            .any(|skill| skill.meta.find_tool(name).is_some())
    }

    /// Populate per-request context into the `vulcan` module.
    /// 将单次请求的上下文注入到 `vulcan` 模块中。
    fn populate_vulcan_request_context(
        lua: &Lua,
        request_context: Option<&RequestContext>,
        tool_name: Option<&str>,
        skill_name: Option<&str>,
    ) -> Result<(), String> {
        let vulcan: Table = lua
            .globals()
            .get("vulcan")
            .map_err(|error| format!("Failed to get vulcan module: {}", error))?;
        let context_value = match request_context {
            Some(context) => serde_json::to_value(context)
                .map_err(|error| format!("Failed to serialize request context: {}", error))?,
            None => Value::Object(serde_json::Map::new()),
        };
        let context_lua = json_value_to_lua(lua, &context_value)
            .map_err(|error| format!("Failed to convert request context to Lua: {}", error))?;
        let client_info_value = match &context_value {
            Value::Object(object) => object.get("client_info").cloned().unwrap_or(Value::Null),
            _ => Value::Null,
        };
        let client_capabilities_value = match &context_value {
            Value::Object(object) => object
                .get("client_capabilities")
                .cloned()
                .unwrap_or_else(|| Value::Object(serde_json::Map::new())),
            _ => Value::Object(serde_json::Map::new()),
        };
        let client_info_lua = json_value_to_lua(lua, &client_info_value)
            .map_err(|error| format!("Failed to convert client_info to Lua: {}", error))?;
        let client_capabilities_lua = json_value_to_lua(lua, &client_capabilities_value)
            .map_err(|error| format!("Failed to convert client_capabilities to Lua: {}", error))?;
        let client_budget_value = resolve_client_budget_value(request_context, tool_name, skill_name);
        let client_budget_lua = json_value_to_lua(lua, &client_budget_value)
            .map_err(|error| format!("Failed to convert client_budget to Lua: {}", error))?;
        let tool_config_value = resolve_tool_config_value(skill_name);
        let tool_config_lua = json_value_to_lua(lua, &tool_config_value)
            .map_err(|error| format!("Failed to convert tool_config to Lua: {}", error))?;

        vulcan
            .set("context", context_lua)
            .map_err(|error| format!("Failed to set vulcan.context: {}", error))?;
        vulcan
            .set("client_info", client_info_lua)
            .map_err(|error| format!("Failed to set vulcan.client_info: {}", error))?;
        vulcan
            .set("client_capabilities", client_capabilities_lua)
            .map_err(|error| format!("Failed to set vulcan.client_capabilities: {}", error))?;
        vulcan
            .set("client_budget", client_budget_lua)
            .map_err(|error| format!("Failed to set vulcan.client_budget: {}", error))?;
        vulcan
            .set("tool_config", tool_config_lua)
            .map_err(|error| format!("Failed to set vulcan.tool_config: {}", error))?;
        Ok(())
    }

    /// Populate the skill-scoped LanceDB host interface into the `vulcan` module.
    /// 将按 skill 作用域隔离的 LanceDB 宿主接口注入到 `vulcan` 模块中。
    fn populate_vulcan_lancedb_context(
        lua: &Lua,
        binding: Option<Arc<LanceDbSkillBinding>>,
        current_skill_name: Option<&str>,
    ) -> Result<(), String> {
        let vulcan: Table = lua
            .globals()
            .get("vulcan")
            .map_err(|error| format!("Failed to get vulcan module: {}", error))?;

        let lancedb_table = lua
            .create_table()
            .map_err(|error| format!("Failed to create vulcan.lancedb table: {}", error))?;

        let current_skill = current_skill_name.unwrap_or("");
        vulcan
            .set("__lancedb_skill_name", current_skill)
            .map_err(|error| format!("Failed to set vulcan.__lancedb_skill_name: {}", error))?;

        if let Some(binding) = binding {
            lancedb_table
                .set("enabled", true)
                .map_err(|error| format!("Failed to set vulcan.lancedb.enabled: {}", error))?;
            let info_binding = binding.clone();
            let info_fn = lua
                .create_function(move |lua, ()| {
                    json_value_to_lua(lua, &info_binding.info_json()).map_err(mlua::Error::external)
                })
                .map_err(|error| format!("Failed to create vulcan.lancedb.info: {}", error))?;
            lancedb_table
                .set("info", info_fn)
                .map_err(|error| format!("Failed to set vulcan.lancedb.info: {}", error))?;

            let status_binding = binding.clone();
            let status_fn = lua
                .create_function(move |lua, ()| {
                    json_value_to_lua(lua, &status_binding.status_json())
                        .map_err(mlua::Error::external)
                })
                .map_err(|error| format!("Failed to create vulcan.lancedb.status: {}", error))?;
            lancedb_table
                .set("status", status_fn)
                .map_err(|error| format!("Failed to set vulcan.lancedb.status: {}", error))?;

            let create_binding = binding.clone();
            let create_table_fn = lua
                .create_function(move |lua, input: LuaValue| {
                    let input_table = require_table_arg(input, "lancedb.create_table", "input")?;
                    let input_json = lua_value_to_json(&LuaValue::Table(input_table))
                        .map_err(mlua::Error::runtime)?;
                    let result = create_binding
                        .create_table_json(&input_json)
                        .map_err(mlua::Error::runtime)?;
                    json_value_to_lua(lua, &result).map_err(mlua::Error::external)
                })
                .map_err(|error| format!("Failed to create vulcan.lancedb.create_table: {}", error))?;
            lancedb_table
                .set("create_table", create_table_fn)
                .map_err(|error| format!("Failed to set vulcan.lancedb.create_table: {}", error))?;

            let upsert_binding = binding.clone();
            let vector_upsert_fn = lua
                .create_function(move |lua, input: LuaValue| {
                    let input_table = require_table_arg(input, "lancedb.vector_upsert", "input")?;
                    let mut input_json = lua_value_to_json(&LuaValue::Table(input_table))
                        .map_err(mlua::Error::runtime)?;
                    let input_object = input_json.as_object_mut().ok_or_else(|| {
                        mlua::Error::runtime(
                            "lancedb.vector_upsert input must be an object / 输入必须是对象",
                        )
                    })?;

                    let payload_value = if let Some(rows) = input_object.remove("rows") {
                        input_object
                            .entry("input_format".to_string())
                            .or_insert_with(|| Value::String("json".to_string()));
                        rows
                    } else if let Some(data) = input_object.remove("data") {
                        data
                    } else {
                        return Err(mlua::Error::runtime(
                            "lancedb.vector_upsert requires rows or data / 需要 rows 或 data 字段",
                        ));
                    };

                    let payload_bytes = match payload_value {
                        Value::String(text) => {
                            if !input_object.contains_key("input_format") {
                                input_object.insert(
                                    "input_format".to_string(),
                                    Value::String("arrow_ipc".to_string()),
                                );
                            }
                            text.into_bytes()
                        }
                        Value::Array(_) | Value::Object(_) => {
                            if !input_object.contains_key("input_format") {
                                input_object.insert(
                                    "input_format".to_string(),
                                    Value::String("json".to_string()),
                                );
                            }
                            serde_json::to_vec(&payload_value).map_err(|error| {
                                mlua::Error::runtime(format!(
                                    "failed to encode lancedb upsert payload / 编码 LanceDB 写入载荷失败: {}",
                                    error
                                ))
                            })?
                        }
                        _ => {
                            return Err(mlua::Error::runtime(
                                "lancedb.vector_upsert payload must be string/table / 载荷必须是字符串或表",
                            ))
                        }
                    };

                    let result = upsert_binding
                        .vector_upsert_json(&input_json, &payload_bytes)
                        .map_err(mlua::Error::runtime)?;
                    json_value_to_lua(lua, &result).map_err(mlua::Error::external)
                })
                .map_err(|error| format!("Failed to create vulcan.lancedb.vector_upsert: {}", error))?;
            lancedb_table
                .set("vector_upsert", vector_upsert_fn)
                .map_err(|error| format!("Failed to set vulcan.lancedb.vector_upsert: {}", error))?;

            let search_binding = binding.clone();
            let vector_search_fn = lua
                .create_function(move |lua, input: LuaValue| {
                    let input_table = require_table_arg(input, "lancedb.vector_search", "input")?;
                    let mut input_json = lua_value_to_json(&LuaValue::Table(input_table))
                        .map_err(mlua::Error::runtime)?;
                    let input_object = input_json.as_object_mut().ok_or_else(|| {
                        mlua::Error::runtime(
                            "lancedb.vector_search input must be an object / 输入必须是对象",
                        )
                    })?;
                    input_object
                        .entry("output_format".to_string())
                        .or_insert_with(|| Value::String("json".to_string()));

                    let (meta, raw_bytes) = search_binding
                        .vector_search_json(&input_json)
                        .map_err(mlua::Error::runtime)?;
                    let result_table = json_to_lua_table_inner(lua, &meta)
                        .map_err(mlua::Error::external)?;

                    if meta
                        .get("format")
                        .and_then(Value::as_str)
                        .map(|value| value == "json")
                        .unwrap_or(false)
                    {
                        let rows_json: Value =
                            serde_json::from_slice(&raw_bytes).map_err(|error| {
                                mlua::Error::runtime(format!(
                                    "failed to parse LanceDB JSON rows / 解析 LanceDB JSON 行数据失败: {}",
                                    error
                                ))
                            })?;
                        result_table
                            .set(
                                "data_json",
                                json_value_to_lua(lua, &rows_json)
                                    .map_err(mlua::Error::external)?,
                            )
                            .map_err(mlua::Error::external)?;
                    } else {
                        result_table
                            .set(
                                "data",
                                LuaValue::String(
                                    lua.create_string(&raw_bytes)
                                        .map_err(mlua::Error::external)?,
                                ),
                            )
                            .map_err(mlua::Error::external)?;
                    }
                    Ok(LuaValue::Table(result_table))
                })
                .map_err(|error| format!("Failed to create vulcan.lancedb.vector_search: {}", error))?;
            lancedb_table
                .set("vector_search", vector_search_fn)
                .map_err(|error| format!("Failed to set vulcan.lancedb.vector_search: {}", error))?;

            let delete_binding = binding.clone();
            let delete_fn = lua
                .create_function(move |lua, input: LuaValue| {
                    let input_table = require_table_arg(input, "lancedb.delete", "input")?;
                    let input_json = lua_value_to_json(&LuaValue::Table(input_table))
                        .map_err(mlua::Error::runtime)?;
                    let result = delete_binding
                        .delete_json(&input_json)
                        .map_err(mlua::Error::runtime)?;
                    json_value_to_lua(lua, &result).map_err(mlua::Error::external)
                })
                .map_err(|error| format!("Failed to create vulcan.lancedb.delete: {}", error))?;
            lancedb_table
                .set("delete", delete_fn)
                .map_err(|error| format!("Failed to set vulcan.lancedb.delete: {}", error))?;

            let drop_binding = binding;
            let drop_table_fn = lua
                .create_function(move |lua, input: LuaValue| {
                    let input_table = require_table_arg(input, "lancedb.drop_table", "input")?;
                    let input_json = lua_value_to_json(&LuaValue::Table(input_table))
                        .map_err(mlua::Error::runtime)?;
                    let result = drop_binding
                        .drop_table_json(&input_json)
                        .map_err(mlua::Error::runtime)?;
                    json_value_to_lua(lua, &result).map_err(mlua::Error::external)
                })
                .map_err(|error| format!("Failed to create vulcan.lancedb.drop_table: {}", error))?;
            lancedb_table
                .set("drop_table", drop_table_fn)
                .map_err(|error| format!("Failed to set vulcan.lancedb.drop_table: {}", error))?;
        } else {
            let disabled_status = disabled_skill_status_json(current_skill_name);
            lancedb_table
                .set("enabled", false)
                .map_err(|error| format!("Failed to set vulcan.lancedb.enabled: {}", error))?;
            let status_value = disabled_status.clone();
            let status_fn = lua
                .create_function(move |lua, ()| {
                    json_value_to_lua(lua, &status_value).map_err(mlua::Error::external)
                })
                .map_err(|error| format!("Failed to create disabled vulcan.lancedb.status: {}", error))?;
            lancedb_table
                .set("status", status_fn)
                .map_err(|error| format!("Failed to set vulcan.lancedb.status: {}", error))?;
            let info_value = disabled_status.clone();
            let info_fn = lua
                .create_function(move |lua, ()| {
                    json_value_to_lua(lua, &info_value).map_err(mlua::Error::external)
                })
                .map_err(|error| format!("Failed to create disabled vulcan.lancedb.info: {}", error))?;
            lancedb_table
                .set("info", info_fn)
                .map_err(|error| format!("Failed to set disabled vulcan.lancedb.info: {}", error))?;
            let disabled_error =
                "current skill has not enabled lancedb / 当前 skill 未启用 lancedb".to_string();
            for method_name in [
                "create_table",
                "vector_upsert",
                "vector_search",
                "delete",
                "drop_table",
            ] {
                let error_text = disabled_error.clone();
                let fn_value = lua
                    .create_function(move |_, _: MultiValue| {
                        Err::<LuaValue, _>(mlua::Error::runtime(error_text.clone()))
                    })
                    .map_err(|error| {
                        format!("Failed to create disabled vulcan.lancedb proxy: {}", error)
                    })?;
                lancedb_table
                    .set(method_name, fn_value)
                    .map_err(|error| format!("Failed to set disabled method {}: {}", method_name, error))?;
            }
        }

        vulcan
            .set("lancedb", lancedb_table)
            .map_err(|error| format!("Failed to set vulcan.lancedb: {}", error))?;
        Ok(())
    }

    /// Populate the skill-scoped SQLite host interface into the `vulcan` module.
    /// 将按 skill 作用域隔离的 SQLite 宿主接口注入到 `vulcan` 模块中。
    fn populate_vulcan_sqlite_context(
        lua: &Lua,
        binding: Option<Arc<SqliteSkillBinding>>,
        current_skill_name: Option<&str>,
    ) -> Result<(), String> {
        let vulcan: Table = lua
            .globals()
            .get("vulcan")
            .map_err(|error| format!("Failed to get vulcan module: {}", error))?;

        let sqlite_table = lua
            .create_table()
            .map_err(|error| format!("Failed to create vulcan.sqlite table: {}", error))?;

        let current_skill = current_skill_name.unwrap_or("");
        vulcan
            .set("__sqlite_skill_name", current_skill)
            .map_err(|error| format!("Failed to set vulcan.__sqlite_skill_name: {}", error))?;

        if let Some(binding) = binding {
            sqlite_table
                .set("enabled", true)
                .map_err(|error| format!("Failed to set vulcan.sqlite.enabled: {}", error))?;

            let info_binding = binding.clone();
            let info_fn = lua
                .create_function(move |lua, ()| {
                    json_value_to_lua(lua, &info_binding.info_json()).map_err(mlua::Error::external)
                })
                .map_err(|error| format!("Failed to create vulcan.sqlite.info: {}", error))?;
            sqlite_table
                .set("info", info_fn)
                .map_err(|error| format!("Failed to set vulcan.sqlite.info: {}", error))?;

            let status_binding = binding.clone();
            let status_fn = lua
                .create_function(move |lua, ()| {
                    json_value_to_lua(lua, &status_binding.status_json())
                        .map_err(mlua::Error::external)
                })
                .map_err(|error| format!("Failed to create vulcan.sqlite.status: {}", error))?;
            sqlite_table
                .set("status", status_fn)
                .map_err(|error| format!("Failed to set vulcan.sqlite.status: {}", error))?;

            let tokenize_binding = binding.clone();
            let tokenize_fn = lua
                .create_function(move |lua, input: LuaValue| {
                    let input_table = require_table_arg(input, "sqlite.tokenize_text", "input")?;
                    let input_json = lua_value_to_json(&LuaValue::Table(input_table))
                        .map_err(mlua::Error::runtime)?;
                    let result = tokenize_binding
                        .tokenize_text_json(&input_json)
                        .map_err(mlua::Error::runtime)?;
                    json_value_to_lua(lua, &result).map_err(mlua::Error::external)
                })
                .map_err(|error| format!("Failed to create vulcan.sqlite.tokenize_text: {}", error))?;
            sqlite_table
                .set("tokenize_text", tokenize_fn)
                .map_err(|error| format!("Failed to set vulcan.sqlite.tokenize_text: {}", error))?;

            let execute_script_binding = binding.clone();
            let execute_script_fn = lua
                .create_function(move |lua, input: LuaValue| {
                    let input_table = require_table_arg(input, "sqlite.execute_script", "input")?;
                    let input_json = lua_value_to_json(&LuaValue::Table(input_table))
                        .map_err(mlua::Error::runtime)?;
                    let result = execute_script_binding
                        .execute_script(&input_json)
                        .map_err(mlua::Error::runtime)?;
                    json_value_to_lua(lua, &result).map_err(mlua::Error::external)
                })
                .map_err(|error| format!("Failed to create vulcan.sqlite.execute_script: {}", error))?;
            sqlite_table
                .set("execute_script", execute_script_fn)
                .map_err(|error| format!("Failed to set vulcan.sqlite.execute_script: {}", error))?;

            let execute_batch_binding = binding.clone();
            let execute_batch_fn = lua
                .create_function(move |lua, input: LuaValue| {
                    let input_table = require_table_arg(input, "sqlite.execute_batch", "input")?;
                    let input_json = lua_value_to_json(&LuaValue::Table(input_table))
                        .map_err(mlua::Error::runtime)?;
                    let result = execute_batch_binding
                        .execute_batch(&input_json)
                        .map_err(mlua::Error::runtime)?;
                    json_value_to_lua(lua, &result).map_err(mlua::Error::external)
                })
                .map_err(|error| format!("Failed to create vulcan.sqlite.execute_batch: {}", error))?;
            sqlite_table
                .set("execute_batch", execute_batch_fn)
                .map_err(|error| format!("Failed to set vulcan.sqlite.execute_batch: {}", error))?;

            let query_json_binding = binding.clone();
            let query_json_fn = lua
                .create_function(move |lua, input: LuaValue| {
                    let input_table = require_table_arg(input, "sqlite.query_json", "input")?;
                    let input_json = lua_value_to_json(&LuaValue::Table(input_table))
                        .map_err(mlua::Error::runtime)?;
                    let result = query_json_binding
                        .query_json(&input_json)
                        .map_err(mlua::Error::runtime)?;
                    json_value_to_lua(lua, &result).map_err(mlua::Error::external)
                })
                .map_err(|error| format!("Failed to create vulcan.sqlite.query_json: {}", error))?;
            sqlite_table
                .set("query_json", query_json_fn)
                .map_err(|error| format!("Failed to set vulcan.sqlite.query_json: {}", error))?;

            let query_stream_binding = binding.clone();
            let query_stream_fn = lua
                .create_function(move |lua, input: LuaValue| {
                    let input_table = require_table_arg(input, "sqlite.query_stream", "input")?;
                    let input_json = lua_value_to_json(&LuaValue::Table(input_table))
                        .map_err(mlua::Error::runtime)?;
                    let result = query_stream_binding
                        .query_stream(&input_json)
                        .map_err(mlua::Error::runtime)?;
                    json_value_to_lua(lua, &result).map_err(mlua::Error::external)
                })
                .map_err(|error| format!("Failed to create vulcan.sqlite.query_stream: {}", error))?;
            sqlite_table
                .set("query_stream", query_stream_fn)
                .map_err(|error| format!("Failed to set vulcan.sqlite.query_stream: {}", error))?;

            let query_stream_wait_metrics_binding = binding.clone();
            let query_stream_wait_metrics_fn = lua
                .create_function(move |lua, input: LuaValue| {
                    let input_table =
                        require_table_arg(input, "sqlite.query_stream_wait_metrics", "input")?;
                    let input_json = lua_value_to_json(&LuaValue::Table(input_table))
                        .map_err(mlua::Error::runtime)?;
                    let result = query_stream_wait_metrics_binding
                        .query_stream_wait_metrics(&input_json)
                        .map_err(mlua::Error::runtime)?;
                    json_value_to_lua(lua, &result).map_err(mlua::Error::external)
                })
                .map_err(|error| {
                    format!(
                        "Failed to create vulcan.sqlite.query_stream_wait_metrics: {}",
                        error
                    )
                })?;
            sqlite_table
                .set("query_stream_wait_metrics", query_stream_wait_metrics_fn)
                .map_err(|error| {
                    format!(
                        "Failed to set vulcan.sqlite.query_stream_wait_metrics: {}",
                        error
                    )
                })?;

            let query_stream_chunk_binding = binding.clone();
            let query_stream_chunk_fn = lua
                .create_function(move |lua, input: LuaValue| {
                    let input_table =
                        require_table_arg(input, "sqlite.query_stream_chunk", "input")?;
                    let input_json = lua_value_to_json(&LuaValue::Table(input_table))
                        .map_err(mlua::Error::runtime)?;
                    let result = query_stream_chunk_binding
                        .query_stream_chunk(&input_json)
                        .map_err(mlua::Error::runtime)?;
                    json_value_to_lua(lua, &result).map_err(mlua::Error::external)
                })
                .map_err(|error| {
                    format!("Failed to create vulcan.sqlite.query_stream_chunk: {}", error)
                })?;
            sqlite_table
                .set("query_stream_chunk", query_stream_chunk_fn)
                .map_err(|error| {
                    format!("Failed to set vulcan.sqlite.query_stream_chunk: {}", error)
                })?;

            let query_stream_close_binding = binding.clone();
            let query_stream_close_fn = lua
                .create_function(move |lua, input: LuaValue| {
                    let input_table =
                        require_table_arg(input, "sqlite.query_stream_close", "input")?;
                    let input_json = lua_value_to_json(&LuaValue::Table(input_table))
                        .map_err(mlua::Error::runtime)?;
                    let result = query_stream_close_binding
                        .query_stream_close(&input_json)
                        .map_err(mlua::Error::runtime)?;
                    json_value_to_lua(lua, &result).map_err(mlua::Error::external)
                })
                .map_err(|error| {
                    format!("Failed to create vulcan.sqlite.query_stream_close: {}", error)
                })?;
            sqlite_table
                .set("query_stream_close", query_stream_close_fn)
                .map_err(|error| {
                    format!("Failed to set vulcan.sqlite.query_stream_close: {}", error)
                })?;

            let upsert_word_binding = binding.clone();
            let upsert_word_fn = lua
                .create_function(move |lua, input: LuaValue| {
                    let input_table =
                        require_table_arg(input, "sqlite.upsert_custom_word", "input")?;
                    let input_json = lua_value_to_json(&LuaValue::Table(input_table))
                        .map_err(mlua::Error::runtime)?;
                    let result = upsert_word_binding
                        .upsert_custom_word_json(&input_json)
                        .map_err(mlua::Error::runtime)?;
                    json_value_to_lua(lua, &result).map_err(mlua::Error::external)
                })
                .map_err(|error| {
                    format!("Failed to create vulcan.sqlite.upsert_custom_word: {}", error)
                })?;
            sqlite_table
                .set("upsert_custom_word", upsert_word_fn)
                .map_err(|error| {
                    format!("Failed to set vulcan.sqlite.upsert_custom_word: {}", error)
                })?;

            let remove_word_binding = binding.clone();
            let remove_word_fn = lua
                .create_function(move |lua, input: LuaValue| {
                    let input_table =
                        require_table_arg(input, "sqlite.remove_custom_word", "input")?;
                    let input_json = lua_value_to_json(&LuaValue::Table(input_table))
                        .map_err(mlua::Error::runtime)?;
                    let result = remove_word_binding
                        .remove_custom_word_json(&input_json)
                        .map_err(mlua::Error::runtime)?;
                    json_value_to_lua(lua, &result).map_err(mlua::Error::external)
                })
                .map_err(|error| {
                    format!("Failed to create vulcan.sqlite.remove_custom_word: {}", error)
                })?;
            sqlite_table
                .set("remove_custom_word", remove_word_fn)
                .map_err(|error| {
                    format!("Failed to set vulcan.sqlite.remove_custom_word: {}", error)
                })?;

            let list_words_binding = binding.clone();
            let list_words_fn = lua
                .create_function(move |lua, ()| {
                    let result = list_words_binding
                        .list_custom_words_json()
                        .map_err(mlua::Error::runtime)?;
                    json_value_to_lua(lua, &result).map_err(mlua::Error::external)
                })
                .map_err(|error| {
                    format!("Failed to create vulcan.sqlite.list_custom_words: {}", error)
                })?;
            sqlite_table
                .set("list_custom_words", list_words_fn)
                .map_err(|error| {
                    format!("Failed to set vulcan.sqlite.list_custom_words: {}", error)
                })?;

            let ensure_index_binding = binding.clone();
            let ensure_index_fn = lua
                .create_function(move |lua, input: LuaValue| {
                    let input_table =
                        require_table_arg(input, "sqlite.ensure_fts_index", "input")?;
                    let input_json = lua_value_to_json(&LuaValue::Table(input_table))
                        .map_err(mlua::Error::runtime)?;
                    let result = ensure_index_binding
                        .ensure_fts_index_json(&input_json)
                        .map_err(mlua::Error::runtime)?;
                    json_value_to_lua(lua, &result).map_err(mlua::Error::external)
                })
                .map_err(|error| {
                    format!("Failed to create vulcan.sqlite.ensure_fts_index: {}", error)
                })?;
            sqlite_table
                .set("ensure_fts_index", ensure_index_fn)
                .map_err(|error| {
                    format!("Failed to set vulcan.sqlite.ensure_fts_index: {}", error)
                })?;

            let rebuild_index_binding = binding.clone();
            let rebuild_index_fn = lua
                .create_function(move |lua, input: LuaValue| {
                    let input_table =
                        require_table_arg(input, "sqlite.rebuild_fts_index", "input")?;
                    let input_json = lua_value_to_json(&LuaValue::Table(input_table))
                        .map_err(mlua::Error::runtime)?;
                    let result = rebuild_index_binding
                        .rebuild_fts_index_json(&input_json)
                        .map_err(mlua::Error::runtime)?;
                    json_value_to_lua(lua, &result).map_err(mlua::Error::external)
                })
                .map_err(|error| {
                    format!("Failed to create vulcan.sqlite.rebuild_fts_index: {}", error)
                })?;
            sqlite_table
                .set("rebuild_fts_index", rebuild_index_fn)
                .map_err(|error| {
                    format!("Failed to set vulcan.sqlite.rebuild_fts_index: {}", error)
                })?;

            let upsert_doc_binding = binding.clone();
            let upsert_doc_fn = lua
                .create_function(move |lua, input: LuaValue| {
                    let input_table =
                        require_table_arg(input, "sqlite.upsert_fts_document", "input")?;
                    let input_json = lua_value_to_json(&LuaValue::Table(input_table))
                        .map_err(mlua::Error::runtime)?;
                    let result = upsert_doc_binding
                        .upsert_fts_document_json(&input_json)
                        .map_err(mlua::Error::runtime)?;
                    json_value_to_lua(lua, &result).map_err(mlua::Error::external)
                })
                .map_err(|error| {
                    format!("Failed to create vulcan.sqlite.upsert_fts_document: {}", error)
                })?;
            sqlite_table
                .set("upsert_fts_document", upsert_doc_fn)
                .map_err(|error| {
                    format!("Failed to set vulcan.sqlite.upsert_fts_document: {}", error)
                })?;

            let delete_doc_binding = binding.clone();
            let delete_doc_fn = lua
                .create_function(move |lua, input: LuaValue| {
                    let input_table =
                        require_table_arg(input, "sqlite.delete_fts_document", "input")?;
                    let input_json = lua_value_to_json(&LuaValue::Table(input_table))
                        .map_err(mlua::Error::runtime)?;
                    let result = delete_doc_binding
                        .delete_fts_document_json(&input_json)
                        .map_err(mlua::Error::runtime)?;
                    json_value_to_lua(lua, &result).map_err(mlua::Error::external)
                })
                .map_err(|error| {
                    format!("Failed to create vulcan.sqlite.delete_fts_document: {}", error)
                })?;
            sqlite_table
                .set("delete_fts_document", delete_doc_fn)
                .map_err(|error| {
                    format!("Failed to set vulcan.sqlite.delete_fts_document: {}", error)
                })?;

            let search_binding = binding;
            let search_fn = lua
                .create_function(move |lua, input: LuaValue| {
                    let input_table = require_table_arg(input, "sqlite.search_fts", "input")?;
                    let input_json = lua_value_to_json(&LuaValue::Table(input_table))
                        .map_err(mlua::Error::runtime)?;
                    let result = search_binding
                        .search_fts_json(&input_json)
                        .map_err(mlua::Error::runtime)?;
                    json_value_to_lua(lua, &result).map_err(mlua::Error::external)
                })
                .map_err(|error| format!("Failed to create vulcan.sqlite.search_fts: {}", error))?;
            sqlite_table
                .set("search_fts", search_fn)
                .map_err(|error| format!("Failed to set vulcan.sqlite.search_fts: {}", error))?;
        } else {
            let disabled_status = disabled_sqlite_skill_status_json(current_skill_name);
            sqlite_table
                .set("enabled", false)
                .map_err(|error| format!("Failed to set vulcan.sqlite.enabled: {}", error))?;
            let status_value = disabled_status.clone();
            let status_fn = lua
                .create_function(move |lua, ()| {
                    json_value_to_lua(lua, &status_value).map_err(mlua::Error::external)
                })
                .map_err(|error| format!("Failed to create disabled vulcan.sqlite.status: {}", error))?;
            sqlite_table
                .set("status", status_fn)
                .map_err(|error| format!("Failed to set vulcan.sqlite.status: {}", error))?;
            let info_value = disabled_status.clone();
            let info_fn = lua
                .create_function(move |lua, ()| {
                    json_value_to_lua(lua, &info_value).map_err(mlua::Error::external)
                })
                .map_err(|error| format!("Failed to create disabled vulcan.sqlite.info: {}", error))?;
            sqlite_table
                .set("info", info_fn)
                .map_err(|error| format!("Failed to set disabled vulcan.sqlite.info: {}", error))?;
            let disabled_error =
                "current skill has not enabled sqlite / 当前 skill 未启用 sqlite".to_string();
            for method_name in [
                "execute_script",
                "execute_batch",
                "query_json",
                "query_stream",
                "query_stream_wait_metrics",
                "query_stream_chunk",
                "query_stream_close",
                "tokenize_text",
                "upsert_custom_word",
                "remove_custom_word",
                "list_custom_words",
                "ensure_fts_index",
                "rebuild_fts_index",
                "upsert_fts_document",
                "delete_fts_document",
                "search_fts",
            ] {
                let error_text = disabled_error.clone();
                let fn_value = lua
                    .create_function(move |_, _: MultiValue| {
                        Err::<LuaValue, _>(mlua::Error::runtime(error_text.clone()))
                    })
                    .map_err(|error| {
                        format!("Failed to create disabled vulcan.sqlite proxy: {}", error)
                    })?;
                sqlite_table
                    .set(method_name, fn_value)
                    .map_err(|error| {
                        format!("Failed to set disabled method {}: {}", method_name, error)
                    })?;
            }
        }

        vulcan
            .set("sqlite", sqlite_table)
            .map_err(|error| format!("Failed to set vulcan.sqlite: {}", error))?;
        Ok(())
    }

    /// Call a loaded Lua skill with the given JSON arguments.
    /// This is synchronous — wrap in spawn_blocking for async contexts.
    pub fn call_skill(
        &self,
        tool_name: &str,
        args: &Value,
        request_context: Option<&RequestContext>,
    ) -> Result<Value, String> {
        let skill = self
            .skills
            .values()
            .find(|skill| skill.meta.find_tool(tool_name).is_some())
            .ok_or_else(|| format!("Lua skill '{}' not found", tool_name))?;
        let (group, tool) = skill
            .meta
            .find_tool_with_group(tool_name)
            .ok_or_else(|| format!("Lua skill '{}' not found", tool_name))?;

        let module_name = tool.lua_module.clone();
        let func_name = format!("__skill_{}", module_name);

        let lease = self.acquire_vm()?;
        let lua = lease.lua();

        if skill.meta.debug {
            Self::compile_skill_into_lua(lua, skill, tool, true)?;
        }

        Self::populate_vulcan_request_context(lua, request_context, Some(tool_name), Some(&skill.meta.name))?;
        Self::populate_vulcan_lancedb_context(
            lua,
            skill.lancedb_binding.clone(),
            Some(&skill.meta.name),
        )?;
        Self::populate_vulcan_sqlite_context(
            lua,
            skill.sqlite_binding.clone(),
            Some(&skill.meta.name),
        )?;

        let handler: Function = lua
            .globals()
            .get(func_name.as_str())
            .map_err(|e| format!("Skill function '{}' not found: {}", module_name, e))?;

        // Convert JSON args to Lua table
        let args_table = json_to_lua_table(lua, args)?;

        let call_result = (|| {
            // Call the function
            let result: LuaValue = handler.call(args_table).map_err(|e| {
                let msg = format!(
                    "Lua skill '{}::{}' error: {}",
                    skill.meta.name, group.name, e
                );
                eprintln!("[LuaSkill:error] {}", msg);
                msg
            })?;

            // Convert result back to JSON
            lua_value_to_json(&result).map_err(|e| {
                let msg = format!(
                    "Lua skill '{}::{}' JSON conversion error: {}",
                    skill.meta.name, group.name, e
                );
                eprintln!("[LuaSkill:error] {}", msg);
                msg
            })
        })();

        Self::populate_vulcan_request_context(lua, None, None, None)?;
        Self::populate_vulcan_lancedb_context(lua, None, None)?;
        Self::populate_vulcan_sqlite_context(lua, None, None)?;
        call_result
    }

    /// Execute arbitrary Lua code and return the result.
    pub fn run_lua(
        &self,
        code: &str,
        args: &Value,
        request_context: Option<&RequestContext>,
    ) -> Result<Value, String> {
        let lease = self.acquire_vm()?;
        let lua = lease.lua();
        Self::populate_vulcan_request_context(lua, request_context, None, None)?;
        Self::populate_vulcan_lancedb_context(lua, None, None)?;
        Self::populate_vulcan_sqlite_context(lua, None, None)?;

        // Build a wrapper that passes args as a local variable
        let args_table = json_to_lua_table(lua, args)?;
        lua.globals()
            .set("__runlua_args", args_table)
            .map_err(|e| format!("Failed to set args: {}", e))?;

        let wrapper = format!(
            "return (function()\n  local args = __runlua_args\n  {}\nend)()",
            code
        );

        let run_result = (|| {
            let result = lua.load(&wrapper).eval::<LuaValue>().map_err(|e| {
                let msg = format!("Lua run_lua error: {}", e);
                eprintln!("[LuaSkill:error] {}", msg);
                msg
            })?;

            lua_value_to_json(&result)
        })();

        Self::populate_vulcan_request_context(lua, None, None, None)?;
        Self::populate_vulcan_lancedb_context(lua, None, None)?;
        Self::populate_vulcan_sqlite_context(lua, None, None)?;
        run_result
    }

    /// Read a skill-provided resource or expand a skill resource template by URI.
    /// 根据 URI 读取技能提供的资源，或展开技能的资源模板。
    pub fn read_resource(
        &self,
        uri: &str,
        request_context: Option<&RequestContext>,
    ) -> Result<Option<ResourceReadResult>, String> {
        for skill in self.skills.values() {
            if let Some((group, resource)) = skill.meta.find_resource_with_group(uri) {
                if is_lua_provider_file(&resource.file) {
                    let generated = self.run_skill_helper(
                        skill,
                        &resource.file,
                        request_context,
                        &json!({
                            "uri": uri,
                            "skill_name": skill.meta.name,
                            "group_name": group.name,
                            "resource_name": resource.name,
                        }),
                    )?;
                    return Ok(Some(normalize_resource_result(generated, uri)?));
                }

                let text = read_skill_text_file(&skill.dir, &resource.file, "resource")?;
                return Ok(Some(ResourceReadResult {
                    contents: vec![ResourceContents::text(
                        uri,
                        &text,
                        resource.mime_type.clone(),
                    )],
                }));
            }

            for template in skill.meta.resource_templates() {
                if let Some(raw_params) = match_uri_template(&template.uri_template, uri) {
                    if is_lua_provider_file(&template.file) {
                        let generated = self.run_skill_helper(
                            skill,
                            &template.file,
                            request_context,
                            &json!({
                            "uri": uri,
                            "uri_template": template.uri_template,
                                "params": raw_params,
                                "skill_name": skill.meta.name,
                                "template_name": template.name,
                            }),
                        )?;
                        return Ok(Some(normalize_resource_result(generated, uri)?));
                    }

                    let template_text =
                        read_skill_text_file(&skill.dir, &template.file, "resource template")?;
                    let rendered = apply_text_template(&template_text, &raw_params);
                    return Ok(Some(ResourceReadResult {
                        contents: vec![ResourceContents::text(
                            uri,
                            &rendered,
                            template.mime_type.clone(),
                        )],
                    }));
                }
            }
        }

        Ok(None)
    }

    /// Resolve a skill-provided prompt into MCP PromptGetResult.
    /// 将技能提供的提示词解析为 MCP PromptGetResult。
    pub fn get_prompt(
        &self,
        name: &str,
        arguments: &Value,
        request_context: Option<&RequestContext>,
    ) -> Result<Option<PromptGetResult>, String> {
        for skill in self.skills.values() {
            if let Some((group, prompt)) = skill.meta.find_prompt_with_group(name) {
                if is_lua_provider_file(&prompt.file) {
                    let generated = self.run_skill_helper(
                        skill,
                        &prompt.file,
                        request_context,
                        &json!({
                            "name": prompt.name,
                            "arguments": arguments.clone(),
                            "skill_name": skill.meta.name,
                            "group_name": group.name,
                        }),
                    )?;
                    return Ok(Some(normalize_prompt_result(generated, &prompt.role)?));
                }

                let template_text = read_skill_text_file(&skill.dir, &prompt.file, "prompt")?;
                return Ok(Some(PromptGetResult {
                    description: prompt.description.clone(),
                    messages: vec![PromptMessage {
                        role: prompt.role.clone(),
                        content: TextContent::text(&template_text),
                    }],
                }));
            }
        }

        Ok(None)
    }

    /// Execute a skill-local Lua helper file that returns a callable function.
    /// 执行技能目录中的 Lua 辅助脚本，该脚本需返回可调用函数。
    fn run_skill_helper(
        &self,
        skill: &LoadedSkill,
        relative_path: &str,
        request_context: Option<&RequestContext>,
        args: &Value,
    ) -> Result<Value, String> {
        let helper_path = skill.dir.join(relative_path);
        let helper_source = std::fs::read_to_string(&helper_path).map_err(|error| {
            format!("Failed to read helper {}: {}", helper_path.display(), error)
        })?;
        let lease = self.acquire_vm()?;
        let lua = lease.lua();
        Self::populate_vulcan_request_context(lua, request_context, None, Some(&skill.meta.name))?;
        Self::populate_vulcan_lancedb_context(
            lua,
            skill.lancedb_binding.clone(),
            Some(&skill.meta.name),
        )?;
        Self::populate_vulcan_sqlite_context(
            lua,
            skill.sqlite_binding.clone(),
            Some(&skill.meta.name),
        )?;
        let args_table = json_to_lua_table(lua, args)?;
        let chunk_name = format!("{}::{}", skill.meta.name, relative_path);
        let chunk = lua.load(&helper_source).set_name(&chunk_name);
        let outer: Function = chunk.into_function().map_err(|error| {
            format!(
                "Helper compile error for {}: {}",
                helper_path.display(),
                error
            )
        })?;
        let handler: Function = outer.call(()).map_err(|error| {
            format!("Helper init error for {}: {}", helper_path.display(), error)
        })?;
        let helper_result = (|| {
            let result: LuaValue = handler.call(args_table).map_err(|error| {
                format!(
                    "Helper runtime error for {}: {}",
                    helper_path.display(),
                    error
                )
            })?;
            lua_value_to_json(&result)
        })();
        Self::populate_vulcan_request_context(lua, None, None, None)?;
        Self::populate_vulcan_lancedb_context(lua, None, None)?;
        Self::populate_vulcan_sqlite_context(lua, None, None)?;
        helper_result
    }

    /// Populate the vulcan.call function to dispatch to loaded skills.
    fn populate_vulcan_call_for_lua(
        lua: &Lua,
        skills_map: &HashMap<String, LoadedSkill>,
        lancedb_host: Option<Arc<LanceDbSkillHost>>,
        sqlite_host: Option<Arc<SqliteSkillHost>>,
    ) -> Result<(), String> {
        let vulcan: Table = lua
            .globals()
            .get("vulcan")
            .map_err(|e| format!("vulcan module not found: {}", e))?;

        // Create the call dispatcher
        let skills: Vec<(String, String, String)> = skills_map
            .iter()
            .flat_map(|(_, skill)| {
                skill
                    .meta
                    .tools()
                    .map(|tool| {
                        (
                            tool.name.clone(),
                            tool.lua_module.clone(),
                            skill.meta.name.clone(),
                        )
                    })
                    .collect::<Vec<(String, String, String)>>()
            })
            .collect();

        let skill_names: Vec<String> = skills.iter().map(|(n, _, _)| n.clone()).collect();
        let module_names: Vec<String> = skills.iter().map(|(_, m, _)| m.clone()).collect();
        let owner_skill_names: Vec<String> = skills.iter().map(|(_, _, s)| s.clone()).collect();

        let dispatcher = lua
            .create_function(move |lua, (name, args): (LuaValue, LuaValue)| {
                let name = require_string_arg(name, "call", "name", false)?;
                let args = require_table_arg(args, "call", "args")?;
                // Find the module function
                let idx = skill_names
                    .iter()
                    .position(|n| n == &name)
                    .ok_or_else(|| mlua::Error::runtime(format!("Skill '{}' not found", name)))?;
                let module = &module_names[idx];
                let owner_skill_name = &owner_skill_names[idx];
                let func_name = format!("__skill_{}", module);
                let func: Function = lua.globals().get(func_name.as_str()).map_err(|_| {
                    mlua::Error::runtime(format!("Skill function '{}' not found", module))
                })?;
                let vulcan: Table = lua
                    .globals()
                    .get("vulcan")
                    .map_err(|error| mlua::Error::runtime(error.to_string()))?;
                let previous_skill_name: String =
                    vulcan.get("__lancedb_skill_name").unwrap_or_default();
                let previous_sqlite_skill_name: String =
                    vulcan.get("__sqlite_skill_name").unwrap_or_default();
                let target_binding = lancedb_host
                    .as_ref()
                    .and_then(|host| host.binding_for_skill(owner_skill_name));
                let target_sqlite_binding = sqlite_host
                    .as_ref()
                    .and_then(|host| host.binding_for_skill(owner_skill_name));
                Self::populate_vulcan_lancedb_context(
                    lua,
                    target_binding,
                    Some(owner_skill_name.as_str()),
                )
                .map_err(mlua::Error::runtime)?;
                Self::populate_vulcan_sqlite_context(
                    lua,
                    target_sqlite_binding,
                    Some(owner_skill_name.as_str()),
                )
                .map_err(mlua::Error::runtime)?;
                let call_result = func.call::<LuaValue>(args);
                let restore_binding = if previous_skill_name.trim().is_empty() {
                    None
                } else {
                    lancedb_host
                        .as_ref()
                        .and_then(|host| host.binding_for_skill(&previous_skill_name))
                };
                let restore_sqlite_binding = if previous_sqlite_skill_name.trim().is_empty() {
                    None
                } else {
                    sqlite_host
                        .as_ref()
                        .and_then(|host| host.binding_for_skill(&previous_sqlite_skill_name))
                };
                Self::populate_vulcan_lancedb_context(
                    lua,
                    restore_binding,
                    if previous_skill_name.trim().is_empty() {
                        None
                    } else {
                        Some(previous_skill_name.as_str())
                    },
                )
                .map_err(mlua::Error::runtime)?;
                Self::populate_vulcan_sqlite_context(
                    lua,
                    restore_sqlite_binding,
                    if previous_sqlite_skill_name.trim().is_empty() {
                        None
                    } else {
                        Some(previous_sqlite_skill_name.as_str())
                    },
                )
                .map_err(mlua::Error::runtime)?;
                call_result
            })
            .map_err(|e| format!("Failed to create vulcan.call dispatcher: {}", e))?;

        vulcan
            .set("call", dispatcher)
            .map_err(|e| format!("Failed to set vulcan.call: {}", e))?;

        Ok(())
    }

    /// Configure package.path and package.cpath to include project-local luarocks tree.
    /// 配置 package.path 与 package.cpath，使其只依赖项目内统一的 lua 目录布局。
    ///
    /// This keeps runtime resolution aligned with the deployed layout under
    /// `lua_packages/share/lua/` and `lua_packages/lib/lua/`, instead of relying on
    /// versioned `5.1` subdirectories that may not exist in the shipped bundle.
    /// 这会让运行时只依赖 `lua_packages/share/lua/` 与 `lua_packages/lib/lua/`
    /// 这套已部署目录结构，而不再依赖可能并不存在的 `5.1` 子目录。
    fn setup_package_paths(lua: &Lua) -> Result<(), Box<dyn std::error::Error>> {
        // Find the lua_packages directory relative to the executable's parent directory.
        let exe_path = std::env::current_exe().ok();
        if let Some(exe_path) = exe_path {
            if let Some(exe_dir) = exe_path.parent() {
                let parent = exe_dir.parent().unwrap_or(exe_dir);
                let lua_packages = parent.join("lua_packages");

                if lua_packages.exists() {
                    // Build package.cpath entries for C modules (.dll on Windows)
                    // 中文：统一使用 lib/lua 目录，不再依赖 lib/lua/5.1。
                    #[cfg(windows)]
                    let cpath_pattern = format!(
                        "{}\\lib\\lua\\?.dll;{}\\lib\\lua\\?\\init.dll;{}\\lib\\lua\\loadall.dll;{}\\?\\?.dll;",
                        lua_packages.display(),
                        lua_packages.display(),
                        lua_packages.display(),
                        lua_packages.display()
                    );

                    // Build package.cpath entries for C modules (.so on Linux)
                    // 中文：Linux 下统一使用 lib/lua 目录，并按 .so 扩展名拼接搜索路径。
                    #[cfg(target_os = "linux")]
                    let cpath_pattern = format!(
                        "{}/lib/lua/?.so;{}/lib/lua/?/init.so;{}/lib/lua/loadall.so;{}/?.so;",
                        lua_packages.display(),
                        lua_packages.display(),
                        lua_packages.display(),
                        lua_packages.display()
                    );

                    // Build package.cpath entries for C modules (.dylib on macOS)
                    // 中文：macOS 下统一使用 lib/lua 目录，并按 .dylib 扩展名拼接搜索路径。
                    #[cfg(target_os = "macos")]
                    let cpath_pattern = format!(
                        "{}/lib/lua/?.dylib;{}/lib/lua/?/init.dylib;{}/lib/lua/loadall.dylib;{}/?.dylib;",
                        lua_packages.display(),
                        lua_packages.display(),
                        lua_packages.display(),
                        lua_packages.display()
                    );

                    // Build package.path entries for Lua modules
                    // 中文：统一使用 share/lua 目录，不再依赖 share/lua/5.1。
                    #[cfg(windows)]
                    let path_pattern = format!(
                        "{}\\share\\lua\\?.lua;{}\\share\\lua\\?\\init.lua;{}\\?.lua;",
                        lua_packages.display(),
                        lua_packages.display(),
                        lua_packages.display()
                    );

                    // Build package.path entries for Lua modules on Unix-like systems
                    // 中文：类 Unix 平台同样统一使用 share/lua 目录，不再依赖 share/lua/5.1。
                    #[cfg(unix)]
                    let path_pattern = format!(
                        "{}/share/lua/?.lua;{}/share/lua/?/init.lua;{}/?.lua;",
                        lua_packages.display(),
                        lua_packages.display(),
                        lua_packages.display()
                    );

                    // Prepend to existing paths
                    let package: Table = lua.globals().get("package")?;
                    let old_cpath: mlua::String = package.get("cpath")?;
                    let new_cpath = format!("{}{}", cpath_pattern, old_cpath.to_str()?.to_string());
                    package.set("cpath", lua.create_string(&new_cpath)?)?;

                    let old_path: mlua::String = package.get("path")?;
                    let new_path = format!("{}{}", path_pattern, old_path.to_str()?.to_string());
                    package.set("path", lua.create_string(&new_path)?)?;
                }
            }
        }
        Ok(())
    }

    /// Register the `vulcan` module in the Lua VM.
    fn register_vulcan_module(lua: &Lua) -> Result<(), Box<dyn std::error::Error>> {
        let vulcan = lua.create_table()?;

        // vulcan.log(level, message)
        let log_fn = lua.create_function(|_, (level, msg): (LuaValue, LuaValue)| {
            let level = require_string_arg(level, "log", "level", false)?;
            let msg = require_string_arg(msg, "log", "message", true)?;
            eprintln!("[LuaSkill:{}] {}", level, msg);
            Ok(())
        })?;
        vulcan.set("log", log_fn)?;

        // vulcan.print(...) — convenience varargs logger, like Lua's print()
        let print_fn = lua.create_function(|_, args: MultiValue| {
            let mut parts = Vec::new();
            for val in args.into_iter() {
                let s = match val {
                    LuaValue::String(s) => s.to_str().map(|b| b.to_string()).unwrap_or_default(),
                    LuaValue::Integer(i) => i.to_string(),
                    LuaValue::Number(f) => f.to_string(),
                    LuaValue::Boolean(b) => b.to_string(),
                    LuaValue::Nil => "nil".to_string(),
                    _ => format!("{:?}", val),
                };
                parts.push(s);
            }
            eprintln!("[LuaSkill:info] {}", parts.join("\t"));
            Ok(())
        })?;
        vulcan.set("print", print_fn)?;

        // vulcan.fs_list(dir) -> array of filenames
        let fs_list_fn = lua.create_function(|_, dir: LuaValue| {
            let dir = require_path_arg(dir, "fs_list", "dir")?;
            let entries: Vec<String> = std::fs::read_dir(&dir)
                .map_err(|e| mlua::Error::runtime(format!("fs_list: {}", e)))?
                .filter_map(|e| e.ok().and_then(|e| e.file_name().into_string().ok()))
                .collect();
            Ok(entries)
        })?;
        vulcan.set("fs_list", fs_list_fn)?;

        // vulcan.fs_read(path) -> string
        let fs_read_fn = lua.create_function(|_, path: LuaValue| {
            let path = require_path_arg(path, "fs_read", "path")?;
            std::fs::read_to_string(&path)
                .map_err(|e| mlua::Error::runtime(format!("fs_read: {}", e)))
        })?;
        vulcan.set("fs_read", fs_read_fn)?;

        // vulcan.fs_write(path, content)
        let fs_write_fn = lua.create_function(|_, (path, content): (LuaValue, LuaValue)| {
            let path = require_path_arg(path, "fs_write", "path")?;
            let content = require_string_arg(content, "fs_write", "content", true)?;
            std::fs::write(&path, content)
                .map_err(|e| mlua::Error::runtime(format!("fs_write: {}", e)))
        })?;
        vulcan.set("fs_write", fs_write_fn)?;

        // vulcan.fs_exists(path) -> bool
        let fs_exists_fn = lua.create_function(|_, path: LuaValue| {
            let path = require_path_arg(path, "fs_exists", "path")?;
            Ok(std::path::Path::new(&path).exists())
        })?;
        vulcan.set("fs_exists", fs_exists_fn)?;

        // vulcan.fs_is_dir(path) -> bool
        let fs_is_dir_fn = lua.create_function(|_, path: LuaValue| {
            let path = require_path_arg(path, "fs_is_dir", "path")?;
            Ok(std::path::Path::new(&path).is_dir())
        })?;
        vulcan.set("fs_is_dir", fs_is_dir_fn)?;

        // vulcan.path_join(...strings) -> string
        let path_join_fn = lua.create_function(|lua, parts: MultiValue| {
            if parts.is_empty() {
                return Err(mlua::Error::runtime(
                    "path_join: expected at least one path segment / 至少需要一个路径片段",
                ));
            }

            let mut joined = PathBuf::new();
            for (index, val) in parts.into_iter().enumerate() {
                let param_name = format!("part[{}]", index + 1);
                let part = require_path_arg(val, "path_join", &param_name)?;
                joined.push(part);
            }
            let result = joined.to_string_lossy().to_string();
            lua.create_string(&result)
        })?;
        vulcan.set("path_join", path_join_fn)?;

        // vulcan.cwd() -> string
        let cwd_fn = lua.create_function(|lua, ()| {
            let current_dir = std::env::current_dir()
                .map_err(|error| mlua::Error::runtime(format!("cwd: {}", error)))?;
            let current_dir_text = current_dir.to_string_lossy().to_string();
            lua.create_string(&current_dir_text)
        })?;
        vulcan.set("cwd", cwd_fn)?;

        // vulcan.temp_dir -> string
        let temp_dir_path = ensure_runtime_temp_dir()
            .map_err(|error| mlua::Error::runtime(format!("temp_dir: {}", error)))?;
        let temp_dir_text = temp_dir_path.to_string_lossy().to_string();
        vulcan.set("temp_dir", temp_dir_text)?;

        // vulcan.exec(spec) -> { ok, success, code, stdout, stderr, timed_out, error }
        let exec_fn = lua.create_function(|lua, spec: LuaValue| {
            let request = parse_exec_request(spec, "exec")?;
            let result = execute_exec_request(request);
            exec_result_to_lua_table(lua, result)
        })?;
        vulcan.set("exec", exec_fn)?;

        // vulcan.osinfo() -> { os = "windows"|"linux"|"macos", arch = "x86_64"|"aarch64" }
        let os_info = lua.create_function(|lua, ()| {
            let os = match std::env::consts::OS {
                "windows" => "windows",
                "linux" => "linux",
                "macos" => "macos",
                _ => std::env::consts::OS,
            };
            let arch = match std::env::consts::ARCH {
                "x86_64" => "x86_64",
                "x86" => "i686",
                "aarch64" => "aarch64",
                "arm" => "armv7l",
                _ => std::env::consts::ARCH,
            };
            let info = lua.create_table()?;
            info.set("os", os)?;
            info.set("arch", arch)?;
            Ok(info)
        })?;
        vulcan.set("osinfo", os_info)?;

        // vulcan.json_encode(table) -> string
        let json_encode_fn =
            lua.create_function(|lua, val: LuaValue| match lua_value_to_json(&val) {
                Ok(json) => lua.create_string(serde_json::to_string(&json).unwrap_or_default()),
                Err(e) => Err(mlua::Error::runtime(format!("json_encode: {}", e))),
            })?;
        vulcan.set("json_encode", json_encode_fn)?;

        // vulcan.json_decode(string) -> table
        let json_decode_fn = lua.create_function(|lua, s: LuaValue| {
            let s = require_string_arg(s, "json_decode", "text", false)?;
            match serde_json::from_str::<Value>(&s) {
                Ok(json) => json_to_lua_table_inner(lua, &json),
                Err(e) => Err(mlua::Error::runtime(format!("json_decode: {}", e))),
            }
        })?;
        vulcan.set("json_decode", json_decode_fn)?;

        // vulcan.cache_put(tool_name, value, ttl_sec?) -> cache_id
        let cache_put_fn = lua.create_function(
            |_, (tool_name, value, ttl_sec): (LuaValue, LuaValue, LuaValue)| {
                let tool_name = require_string_arg(tool_name, "cache_put", "tool_name", false)?;
                let ttl_secs = optional_u64_arg(ttl_sec, "cache_put", "ttl_sec")?;
                let payload = lua_value_to_json(&value)
                    .map_err(|e| mlua::Error::runtime(format!("cache_put: {}", e)))?;
                let cache_id = global_tool_cache().create(&tool_name, payload, ttl_secs);
                Ok(cache_id)
            },
        )?;
        vulcan.set("cache_put", cache_put_fn)?;

        // vulcan.cache_get(tool_name, cache_id) -> value|nil
        let cache_get_fn =
            lua.create_function(|lua, (tool_name, cache_id): (LuaValue, LuaValue)| {
                let tool_name = require_string_arg(tool_name, "cache_get", "tool_name", false)?;
                let cache_id = require_string_arg(cache_id, "cache_get", "cache_id", false)?;
                match global_tool_cache().get(&tool_name, &cache_id) {
                    Some(value) => json_value_to_lua(lua, &value),
                    None => Ok(LuaValue::Nil),
                }
            })?;
        vulcan.set("cache_get", cache_get_fn)?;

        // vulcan.cache_delete(tool_name, cache_id) -> boolean
        let cache_delete_fn =
            lua.create_function(|_, (tool_name, cache_id): (LuaValue, LuaValue)| {
                let tool_name = require_string_arg(tool_name, "cache_delete", "tool_name", false)?;
                let cache_id = require_string_arg(cache_id, "cache_delete", "cache_id", false)?;
                Ok(global_tool_cache().delete(&tool_name, &cache_id))
            })?;
        vulcan.set("cache_delete", cache_delete_fn)?;

        // vulcan.context / vulcan.client_info / vulcan.client_capabilities / vulcan.client_budget / vulcan.tool_config
        // These fields are refreshed for every request before Lua execution.
        // 这些字段会在每次 Lua 执行前刷新，用于暴露当前请求的客户端上下文、预算信息以及工具配置。
        vulcan.set("context", lua.create_table()?)?;
        vulcan.set("client_info", LuaValue::Nil)?;
        vulcan.set("client_capabilities", lua.create_table()?)?;
        vulcan.set("client_budget", lua.create_table()?)?;
        vulcan.set("tool_config", lua.create_table()?)?;

        // Placeholder for call (populated after skills load)
        let call_stub = lua.create_function(|_, _: (LuaValue, LuaValue)| {
            Err::<(), _>(mlua::Error::runtime("vulcan.call not initialized"))
        })?;
        vulcan.set("call", call_stub)?;

        lua.globals().set("vulcan", vulcan)?;

        Ok(())
    }
}

// ============================================================
// JSON ↔ Lua Value conversion
// ============================================================

fn json_to_lua_table(lua: &Lua, json: &Value) -> Result<Table, String> {
    json_to_lua_table_inner(lua, json).map_err(|e| e.to_string())
}

fn json_to_lua_table_inner(lua: &Lua, json: &Value) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    if let Value::Object(obj) = json {
        for (k, v) in obj {
            table.set(k.as_str(), json_value_to_lua(lua, v)?)?;
        }
    } else if let Value::Array(arr) = json {
        for (i, v) in arr.iter().enumerate() {
            table.set(i + 1, json_value_to_lua(lua, v)?)?;
        }
    }
    Ok(table)
}

fn json_value_to_lua(lua: &Lua, json: &Value) -> mlua::Result<LuaValue> {
    match json {
        Value::Null => Ok(LuaValue::Nil),
        Value::Bool(b) => Ok(LuaValue::Boolean(*b)),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(LuaValue::Integer(i))
            } else {
                Ok(LuaValue::Number(n.as_f64().unwrap_or(0.0)))
            }
        }
        Value::String(s) => Ok(LuaValue::String(lua.create_string(s)?)),
        Value::Array(_) | Value::Object(_) => {
            Ok(LuaValue::Table(json_to_lua_table_inner(lua, json)?))
        }
    }
}

fn lua_value_to_json(val: &LuaValue) -> Result<Value, String> {
    match val {
        LuaValue::Nil => Ok(Value::Null),
        LuaValue::Boolean(b) => Ok(Value::Bool(*b)),
        LuaValue::Integer(i) => Ok(Value::Number((*i).into())),
        LuaValue::Number(f) => {
            if let Some(n) = serde_json::Number::from_f64(*f) {
                Ok(Value::Number(n))
            } else {
                Ok(Value::Null)
            }
        }
        LuaValue::String(s) => Ok(Value::String(
            s.to_str().map(|b| b.to_string()).unwrap_or_default(),
        )),
        LuaValue::Table(t) => {
            // Heuristic: if raw_len() > 0, treat as array. Otherwise as object.
            if t.raw_len() > 0 {
                let arr = lua_table_to_array(t)?;
                Ok(Value::Array(arr))
            } else {
                lua_table_to_object(t)
            }
        }
        LuaValue::Function(_) => Err("Cannot convert Lua function to JSON".to_string()),
        LuaValue::Thread(_) => Err("Cannot convert Lua thread to JSON".to_string()),
        LuaValue::UserData(_) => Err("Cannot convert Lua userdata to JSON".to_string()),
        LuaValue::LightUserData(_) => Err("Cannot convert light userdata to JSON".to_string()),
        _ => Err("Unknown Lua value type".to_string()),
    }
}

fn lua_table_to_array(t: &Table) -> Result<Vec<Value>, String> {
    let len = t.raw_len();
    if len == 0 {
        // Could be empty object or empty array, default to array
        return Ok(Vec::new());
    }
    let mut arr = Vec::with_capacity(len);
    for i in 1..=len {
        let val: LuaValue = t.get(i).map_err(|e| format!("Array index {}: {}", i, e))?;
        arr.push(lua_value_to_json(&val)?);
    }
    Ok(arr)
}

fn lua_table_to_object(t: &Table) -> Result<Value, String> {
    let mut obj = serde_json::Map::new();
    for pair in t.pairs::<String, LuaValue>() {
        let (k, v) = pair.map_err(|e| format!("Table key: {}", e))?;
        obj.insert(k, lua_value_to_json(&v)?);
    }
    // Empty Lua table has no distinction between array and object.
    // If there are no string keys, treat as empty array.
    if obj.is_empty() && t.raw_len() == 0 {
        return Ok(Value::Array(Vec::new()));
    }
    Ok(Value::Object(obj))
}
