use libloading::Library;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::ffi::{CStr, CString, c_char, c_uchar};
use std::path::{Path, PathBuf};
use std::ptr;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::lua_skill::{SkillLanceDbLogLevel, SkillLanceDbMeta};

/// 中文：FFI 运行时句柄前置声明，仅用于跨动态库传递裸指针。
/// English: Forward declaration of the FFI runtime handle used only for raw cross-library pointers.
#[repr(C)]
struct VldbLancedbRuntimeHandle {
    _private: [u8; 0],
}

/// 中文：FFI 引擎句柄前置声明，仅用于跨动态库传递裸指针。
/// English: Forward declaration of the FFI engine handle used only for raw cross-library pointers.
#[repr(C)]
struct VldbLancedbEngineHandle {
    _private: [u8; 0],
}

/// 中文：LanceDB FFI 原始字节缓冲区定义，需与动态库头文件保持一致。
/// English: Raw LanceDB FFI byte-buffer definition kept identical to the exported dynamic-library header.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct VldbLancedbByteBuffer {
    data: *mut c_uchar,
    len: usize,
    /// 原始分配容量，仅供动态库在释放时恢复 Vec 布局。
    /// Original allocation capacity used only by the dynamic library when reconstructing the Vec during free.
    cap: usize,
}

/// 中文：LanceDB FFI 运行时选项，需与导出的头文件严格对齐。
/// English: LanceDB FFI runtime options that must stay ABI-compatible with the exported header.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct VldbLancedbRuntimeOptions {
    default_db_path: *const c_char,
    db_root: *const c_char,
    read_consistency_interval_ms: u64,
    has_read_consistency_interval: u8,
    max_upsert_payload: usize,
    max_search_limit: usize,
    max_concurrent_requests: usize,
}

type RuntimeOptionsDefaultFn = unsafe extern "C" fn() -> VldbLancedbRuntimeOptions;
type RuntimeCreateFn =
    unsafe extern "C" fn(VldbLancedbRuntimeOptions) -> *mut VldbLancedbRuntimeHandle;
type RuntimeDestroyFn = unsafe extern "C" fn(*mut VldbLancedbRuntimeHandle);
type RuntimeOpenDefaultEngineFn =
    unsafe extern "C" fn(*mut VldbLancedbRuntimeHandle) -> *mut VldbLancedbEngineHandle;
type RuntimeDatabasePathForNameFn =
    unsafe extern "C" fn(*mut VldbLancedbRuntimeHandle, *const c_char) -> *mut c_char;
type EngineCreateTableJsonFn =
    unsafe extern "C" fn(*mut VldbLancedbEngineHandle, *const c_char) -> *mut c_char;
type EngineVectorUpsertFn = unsafe extern "C" fn(
    *mut VldbLancedbEngineHandle,
    *const c_char,
    *const u8,
    usize,
) -> *mut c_char;
type EngineVectorSearchFn = unsafe extern "C" fn(
    *mut VldbLancedbEngineHandle,
    *const c_char,
    *mut VldbLancedbByteBuffer,
) -> *mut c_char;
type EngineDeleteJsonFn =
    unsafe extern "C" fn(*mut VldbLancedbEngineHandle, *const c_char) -> *mut c_char;
type EngineDropTableJsonFn =
    unsafe extern "C" fn(*mut VldbLancedbEngineHandle, *const c_char) -> *mut c_char;
type EngineDestroyFn = unsafe extern "C" fn(*mut VldbLancedbEngineHandle);
type BytesFreeFn = unsafe extern "C" fn(VldbLancedbByteBuffer);
type StringFreeFn = unsafe extern "C" fn(*mut c_char);
type LastErrorMessageFn = unsafe extern "C" fn() -> *const c_char;
type ClearLastErrorFn = unsafe extern "C" fn();

/// 中文：已加载的 LanceDB FFI API 表，负责持有动态库生命周期与导出函数指针。
/// English: Loaded LanceDB FFI API table that owns the dynamic-library lifetime and exported function pointers.
struct LoadedLanceDbApi {
    _library: Library,
    library_path: PathBuf,
    runtime_options_default: RuntimeOptionsDefaultFn,
    runtime_create: RuntimeCreateFn,
    runtime_destroy: RuntimeDestroyFn,
    runtime_open_default_engine: RuntimeOpenDefaultEngineFn,
    runtime_database_path_for_name: RuntimeDatabasePathForNameFn,
    engine_create_table_json: EngineCreateTableJsonFn,
    engine_vector_upsert: EngineVectorUpsertFn,
    engine_vector_search: EngineVectorSearchFn,
    engine_delete_json: EngineDeleteJsonFn,
    engine_drop_table_json: EngineDropTableJsonFn,
    engine_destroy: EngineDestroyFn,
    bytes_free: BytesFreeFn,
    string_free: StringFreeFn,
    last_error_message: LastErrorMessageFn,
    clear_last_error: ClearLastErrorFn,
}

/// 中文：动态库句柄与函数指针在初始化后只读，调用端通过外层互斥量串行化访问。
/// English: The loaded library handle and copied function pointers stay immutable after initialization, while callers serialize use via outer mutexes.
unsafe impl Send for LoadedLanceDbApi {}
unsafe impl Sync for LoadedLanceDbApi {}

impl LoadedLanceDbApi {
    /// 中文：按宿主约定加载 LanceDB 动态库，优先查找显式环境变量与运行时 libs 目录。
    /// English: Load the LanceDB dynamic library using host conventions, preferring an explicit environment variable and runtime libs directories.
    fn load() -> Result<Self, String> {
        let mut last_error = String::from("no candidate path attempted");
        for candidate in candidate_library_paths() {
            if !candidate.exists() {
                continue;
            }

            let library =
                unsafe { Library::new(&candidate) }.map_err(|error| error.to_string());
            match library {
                Ok(library) => {
                    return unsafe { Self::from_library(candidate, library) };
                }
                Err(error) => {
                    last_error = format!("failed to load {}: {}", candidate.display(), error);
                }
            }
        }

        Err(format!(
            "LanceDB dynamic library not found or failed to load / 未找到或无法加载 LanceDB 动态库: {}",
            last_error
        ))
    }

    /// 中文：从已打开的动态库中复制需要的函数指针，并保留库句柄防止提前卸载。
    /// English: Copy all required exported function pointers from an opened dynamic library and keep the library handle alive.
    unsafe fn from_library(library_path: PathBuf, library: Library) -> Result<Self, String> {
        macro_rules! load_symbol {
            ($name:literal, $ty:ty) => {{
                unsafe {
                    *library
                        .get::<$ty>(concat!($name, "\0").as_bytes())
                        .map_err(|error| {
                            format!(
                                "failed to load symbol {} from {}: {}",
                                $name,
                                library_path.display(),
                                error
                            )
                        })?
                }
            }};
        }

        Ok(Self {
            runtime_options_default: load_symbol!(
                "vldb_lancedb_runtime_options_default",
                RuntimeOptionsDefaultFn
            ),
            runtime_create: load_symbol!("vldb_lancedb_runtime_create", RuntimeCreateFn),
            runtime_destroy: load_symbol!("vldb_lancedb_runtime_destroy", RuntimeDestroyFn),
            runtime_open_default_engine: load_symbol!(
                "vldb_lancedb_runtime_open_default_engine",
                RuntimeOpenDefaultEngineFn
            ),
            runtime_database_path_for_name: load_symbol!(
                "vldb_lancedb_runtime_database_path_for_name",
                RuntimeDatabasePathForNameFn
            ),
            engine_create_table_json: load_symbol!(
                "vldb_lancedb_engine_create_table_json",
                EngineCreateTableJsonFn
            ),
            engine_vector_upsert: load_symbol!(
                "vldb_lancedb_engine_vector_upsert",
                EngineVectorUpsertFn
            ),
            engine_vector_search: load_symbol!(
                "vldb_lancedb_engine_vector_search",
                EngineVectorSearchFn
            ),
            engine_delete_json: load_symbol!(
                "vldb_lancedb_engine_delete_json",
                EngineDeleteJsonFn
            ),
            engine_drop_table_json: load_symbol!(
                "vldb_lancedb_engine_drop_table_json",
                EngineDropTableJsonFn
            ),
            engine_destroy: load_symbol!("vldb_lancedb_engine_destroy", EngineDestroyFn),
            bytes_free: load_symbol!("vldb_lancedb_bytes_free", BytesFreeFn),
            string_free: load_symbol!("vldb_lancedb_string_free", StringFreeFn),
            last_error_message: load_symbol!(
                "vldb_lancedb_last_error_message",
                LastErrorMessageFn
            ),
            clear_last_error: load_symbol!("vldb_lancedb_clear_last_error", ClearLastErrorFn),
            _library: library,
            library_path,
        })
    }

    /// 中文：读取最近一次 FFI 调用错误文本，并返回稳定 Rust 字符串。
    /// English: Read the latest FFI error text and return it as a stable Rust string.
    fn take_last_error_message(&self) -> String {
        unsafe {
            let ptr = (self.last_error_message)();
            let text = if ptr.is_null() {
                "unknown LanceDB host error / 未知 LanceDB 宿主错误".to_string()
            } else {
                CStr::from_ptr(ptr).to_string_lossy().to_string()
            };
            (self.clear_last_error)();
            text
        }
    }

    /// 中文：释放由动态库分配的字符串并转成 Rust `String`。
    /// English: Convert a dynamic-library allocated string into a Rust `String` and free the original allocation.
    fn take_owned_string(&self, ptr: *mut c_char) -> Result<String, String> {
        if ptr.is_null() {
            return Err(self.take_last_error_message());
        }

        unsafe {
            let text = CStr::from_ptr(ptr).to_string_lossy().to_string();
            (self.string_free)(ptr);
            Ok(text)
        }
    }

    /// 中文：释放由动态库分配的字节缓冲区并转成 Rust `Vec<u8>`。
    /// English: Convert a dynamic-library allocated byte buffer into a Rust `Vec<u8>` and free the original allocation.
    fn take_owned_bytes(&self, buffer: VldbLancedbByteBuffer) -> Vec<u8> {
        if buffer.data.is_null() || buffer.len == 0 {
            return Vec::new();
        }

        unsafe {
            let bytes = std::slice::from_raw_parts(buffer.data, buffer.len).to_vec();
            (self.bytes_free)(buffer);
            bytes
        }
    }
}

/// 中文：单个 skill 的 LanceDB 句柄集合，由宿主管理其生命周期。
/// English: One skill-scoped LanceDB handle set whose lifetime is managed entirely by the host.
struct SkillHandleState {
    runtime: *mut VldbLancedbRuntimeHandle,
    engine: *mut VldbLancedbEngineHandle,
}

/// 中文：FFI 句柄只通过宿主互斥量串行访问，跨线程共享由宿主统一控制。
/// English: FFI handles are accessed only behind host-side mutexes, and cross-thread sharing is controlled centrally by the host.
unsafe impl Send for SkillHandleState {}

/// 中文：某个启用 LanceDB 的 skill 所绑定的数据库上下文。
/// English: Database context bound to one LanceDB-enabled skill.
pub struct LanceDbSkillBinding {
    api: Arc<LoadedLanceDbApi>,
    skill_name: String,
    skill_dir_name: String,
    database_path: String,
    config: SkillLanceDbMeta,
    handles: Mutex<SkillHandleState>,
}

impl LanceDbSkillBinding {
    /// 中文：返回当前 skill 的稳定 LanceDB 状态信息；无论启用与否，返回结构都应稳定。
    /// English: Return the stable LanceDB status payload for the current skill; the shape stays stable whether enabled or disabled.
    pub fn status_json(&self) -> Value {
        json!({
            "enabled": true,
            "initialized": true,
            "skill_name": self.skill_name,
            "skill_dir_name": self.skill_dir_name,
            "database_path": self.database_path,
            "integration_mode": "dynamic_library",
            "library_path": self.api.library_path.to_string_lossy().to_string(),
            "log_level": self.config.log_level.as_str(),
            "slow_log_enabled": self.config.slow_log_enabled,
            "slow_log_threshold_ms": self.config.slow_log_threshold_ms,
        })
    }

    /// 中文：返回当前 skill 所绑定 LanceDB 的基础信息，供 Lua 或诊断输出使用。
    /// English: Return base information about the LanceDB instance bound to the current skill for Lua and diagnostics.
    pub fn info_json(&self) -> Value {
        self.status_json()
    }

    /// 中文：执行建表操作，输入必须符合宿主约定的 JSON 结构。
    /// English: Execute create-table using the host-defined JSON input shape.
    pub fn create_table_json(&self, input: &Value) -> Result<Value, String> {
        self.call_json_string("create_table", input, |api, state, input_ptr| unsafe {
            (api.engine_create_table_json)(state.engine, input_ptr)
        })
    }

    /// 中文：执行向量写入；调用方负责提供已经编码好的原始载荷。
    /// English: Execute vector upsert; callers must provide an already encoded raw payload.
    pub fn vector_upsert_json(&self, input: &Value, data: &[u8]) -> Result<Value, String> {
        let input_text = serde_json::to_string(input).map_err(|error| error.to_string())?;
        let input_cstr = CString::new(input_text).map_err(|_| {
            "input json contains interior NUL bytes / 输入 JSON 含有 NUL 字节".to_string()
        })?;
        self.log_info("vector_upsert", Some(format!("payload_bytes={}", data.len())));
        let started_at = Instant::now();
        let guard = self.handles.lock().map_err(|_| {
            "failed to acquire LanceDB handle lock / 获取 LanceDB 句柄锁失败".to_string()
        })?;
        unsafe {
            let response = (self.api.engine_vector_upsert)(
                guard.engine,
                input_cstr.as_ptr(),
                data.as_ptr(),
                data.len(),
            );
            let text = match self.api.take_owned_string(response) {
                Ok(text) => text,
                Err(error) => {
                    drop(guard);
                    self.log_warning("vector_upsert", &error);
                    return Err(error);
                }
            };
            let value = serde_json::from_str(&text).map_err(|error| {
                format!(
                    "failed to parse LanceDB upsert response JSON / 无法解析 LanceDB 写入返回 JSON: {}",
                    error
                )
            })?;
            drop(guard);
            self.log_if_slow(
                "vector_upsert",
                started_at,
                Some(format!("payload_bytes={}", data.len())),
            );
            Ok(value)
        }
    }

    /// 中文：执行向量检索并返回元信息 JSON 与原始结果字节。
    /// English: Execute vector search and return both metadata JSON and raw result bytes.
    pub fn vector_search_json(&self, input: &Value) -> Result<(Value, Vec<u8>), String> {
        let input_text = serde_json::to_string(input).map_err(|error| error.to_string())?;
        let input_cstr = CString::new(input_text).map_err(|_| {
            "input json contains interior NUL bytes / 输入 JSON 含有 NUL 字节".to_string()
        })?;
        self.log_info("vector_search", None);
        let started_at = Instant::now();
        let guard = self.handles.lock().map_err(|_| {
            "failed to acquire LanceDB handle lock / 获取 LanceDB 句柄锁失败".to_string()
        })?;
        let mut buffer = VldbLancedbByteBuffer {
            data: ptr::null_mut(),
            len: 0,
            cap: 0,
        };
        unsafe {
            let response =
                (self.api.engine_vector_search)(guard.engine, input_cstr.as_ptr(), &mut buffer);
            let text = match self.api.take_owned_string(response) {
                Ok(text) => text,
                Err(error) => {
                    drop(guard);
                    self.log_warning("vector_search", &error);
                    return Err(error);
                }
            };
            let meta: Value = serde_json::from_str(&text).map_err(|error| {
                format!(
                    "failed to parse LanceDB search response JSON / 无法解析 LanceDB 检索返回 JSON: {}",
                    error
                )
            })?;
            let bytes = self.api.take_owned_bytes(buffer);
            drop(guard);
            self.log_if_slow(
                "vector_search",
                started_at,
                Some(format!("result_bytes={}", bytes.len())),
            );
            Ok((meta, bytes))
        }
    }

    /// 中文：执行删除操作。
    /// English: Execute delete.
    pub fn delete_json(&self, input: &Value) -> Result<Value, String> {
        self.call_json_string("delete", input, |api, state, input_ptr| unsafe {
            (api.engine_delete_json)(state.engine, input_ptr)
        })
    }

    /// 中文：执行删表操作。
    /// English: Execute drop-table.
    pub fn drop_table_json(&self, input: &Value) -> Result<Value, String> {
        self.call_json_string("drop_table", input, |api, state, input_ptr| unsafe {
            (api.engine_drop_table_json)(state.engine, input_ptr)
        })
    }

    /// 中文：统一执行“输入 JSON -> 返回 JSON 字符串”的 FFI 调用。
    /// English: Execute an FFI call that maps a JSON input into a JSON-string response.
    fn call_json_string<F>(
        &self,
        operation: &str,
        input: &Value,
        invoke: F,
    ) -> Result<Value, String>
    where
        F: Fn(&LoadedLanceDbApi, &SkillHandleState, *const c_char) -> *mut c_char,
    {
        let input_text = serde_json::to_string(input).map_err(|error| error.to_string())?;
        let input_cstr = CString::new(input_text).map_err(|_| {
            "input json contains interior NUL bytes / 输入 JSON 含有 NUL 字节".to_string()
        })?;
        self.log_info(operation, None);
        let started_at = Instant::now();
        let guard = self.handles.lock().map_err(|_| {
            "failed to acquire LanceDB handle lock / 获取 LanceDB 句柄锁失败".to_string()
        })?;
        let response = invoke(&self.api, &guard, input_cstr.as_ptr());
        let text = match self.api.take_owned_string(response) {
            Ok(text) => text,
            Err(error) => {
                drop(guard);
                self.log_warning(operation, &error);
                return Err(error);
            }
        };
        let value = serde_json::from_str(&text).map_err(|error| {
            format!(
                "failed to parse LanceDB response JSON / 无法解析 LanceDB 返回 JSON: {}",
                error
            )
        })?;
        drop(guard);
        self.log_if_slow(operation, started_at, None);
        Ok(value)
    }

    /// 中文：按 skill 配置输出普通信息级日志。
    /// English: Emit regular informational logs according to the skill-scoped log policy.
    fn log_info(&self, operation: &str, extra: Option<String>) {
        if self.config.log_level == SkillLanceDbLogLevel::Info {
            match extra {
                Some(extra) => eprintln!(
                    "[LanceDb:info] skill={} db={} op={} {}",
                    self.skill_name, self.skill_dir_name, operation, extra
                ),
                None => eprintln!(
                    "[LanceDb:info] skill={} db={} op={}",
                    self.skill_name, self.skill_dir_name, operation
                ),
            }
        }
    }

    /// 中文：按慢日志配置输出耗时告警；该日志与普通日志开关独立。
    /// English: Emit a slow-operation warning according to the slow-log policy; this is independent from regular log verbosity.
    fn log_if_slow(&self, operation: &str, started_at: Instant, extra: Option<String>) {
        if !self.config.slow_log_enabled {
            return;
        }

        let elapsed_ms = started_at.elapsed().as_millis() as u64;
        if elapsed_ms < self.config.slow_log_threshold_ms {
            return;
        }

        match extra {
            Some(extra) => eprintln!(
                "[LanceDb:slow] skill={} db={} op={} elapsed_ms={} {}",
                self.skill_name, self.skill_dir_name, operation, elapsed_ms, extra
            ),
            None => eprintln!(
                "[LanceDb:slow] skill={} db={} op={} elapsed_ms={}",
                self.skill_name, self.skill_dir_name, operation, elapsed_ms
            ),
        }
    }

    /// 中文：按 skill 配置输出告警级日志，通常用于 FFI 调用失败或宿主检测到的异常情况。
    /// English: Emit warning-level logs according to the skill policy, usually for FFI call failures or host-detected anomalies.
    fn log_warning(&self, operation: &str, message: &str) {
        if matches!(
            self.config.log_level,
            SkillLanceDbLogLevel::Info | SkillLanceDbLogLevel::Warning
        ) {
            eprintln!(
                "[LanceDb:warn] skill={} db={} op={} message={}",
                self.skill_name, self.skill_dir_name, operation, message
            );
        }
    }
}

impl Drop for LanceDbSkillBinding {
    /// 中文：由宿主在 skill 生命周期结束时统一释放引擎与运行时句柄。
    /// English: The host releases engine and runtime handles together when the skill binding is dropped.
    fn drop(&mut self) {
        if let Ok(mut guard) = self.handles.lock() {
            unsafe {
                if !guard.engine.is_null() {
                    (self.api.engine_destroy)(guard.engine);
                    guard.engine = ptr::null_mut();
                }
                if !guard.runtime.is_null() {
                    (self.api.runtime_destroy)(guard.runtime);
                    guard.runtime = ptr::null_mut();
                }
            }
        }
    }
}

/// 中文：按 skill 维度维护 LanceDB 绑定，负责技能启用后的自动创建与长期复用。
/// English: Maintain skill-scoped LanceDB bindings, auto-creating and reusing them for enabled skills.
pub struct LanceDbSkillHost {
    api: Arc<LoadedLanceDbApi>,
    skills: Mutex<HashMap<String, Arc<LanceDbSkillBinding>>>,
}

impl LanceDbSkillHost {
    /// 中文：创建宿主级 LanceDB 技能管理器，并立即加载动态库。
    /// English: Create the host-side LanceDB skill manager and load the dynamic library immediately.
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            api: Arc::new(LoadedLanceDbApi::load()?),
            skills: Mutex::new(HashMap::new()),
        })
    }

    /// 中文：为启用 LanceDB 的 skill 注册固定数据库绑定；同一个 skill 只会创建一次。
    /// English: Register the fixed database binding for a LanceDB-enabled skill; each skill is created only once.
    pub fn register_skill(
        &self,
        skill_name: &str,
        skill_dir: &Path,
        config: SkillLanceDbMeta,
    ) -> Result<Arc<LanceDbSkillBinding>, String> {
        let mut guard = self.skills.lock().map_err(|_| {
            "failed to acquire LanceDB skill registry lock / 获取 LanceDB 技能注册表锁失败".to_string()
        })?;
        if let Some(existing) = guard.get(skill_name) {
            return Ok(existing.clone());
        }

        let skill_dir_name = skill_dir
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                format!(
                    "invalid skill directory name for {} / 无法解析 skill 目录名: {}",
                    skill_name,
                    skill_dir.display()
                )
            })?
            .to_string();
        let skills_root = skill_dir.parent().ok_or_else(|| {
            format!(
                "skill directory has no parent / skill 目录缺少父目录: {}",
                skill_dir.display()
            )
        })?;
        let db_path = skills_root.join("__lancedb").join(&skill_dir_name);
        std::fs::create_dir_all(&db_path).map_err(|error| {
            format!(
                "failed to create LanceDB directory {}: {} / 创建 LanceDB 目录失败: {}",
                db_path.display(),
                error,
                error
            )
        })?;

        let database_path = db_path.to_string_lossy().to_string();
        let default_path = CString::new(database_path.clone()).map_err(|_| {
            "database path contains interior NUL bytes / 数据库路径包含 NUL 字节".to_string()
        })?;
        let mut options = unsafe { (self.api.runtime_options_default)() };
        options.default_db_path = default_path.as_ptr();
        options.db_root = ptr::null();
        let runtime = unsafe { (self.api.runtime_create)(options) };
        if runtime.is_null() {
            return Err(self.api.take_last_error_message());
        }

        let engine = unsafe { (self.api.runtime_open_default_engine)(runtime) };
        if engine.is_null() {
            unsafe {
                (self.api.runtime_destroy)(runtime);
            }
            return Err(self.api.take_last_error_message());
        }

        let resolved_path = unsafe {
            self.api.take_owned_string((self.api.runtime_database_path_for_name)(
                runtime,
                ptr::null(),
            ))
        }
        .unwrap_or(database_path.clone());

        let binding = Arc::new(LanceDbSkillBinding {
            api: self.api.clone(),
            skill_name: skill_name.to_string(),
            skill_dir_name,
            database_path: resolved_path,
            config,
            handles: Mutex::new(SkillHandleState { runtime, engine }),
        });
        guard.insert(skill_name.to_string(), binding.clone());
        Ok(binding)
    }

    /// 中文：按 skill 名称获取已注册绑定，供 Lua 注入与跨 skill 调用恢复上下文使用。
    /// English: Fetch a registered binding by skill name so Lua injection and cross-skill calls can restore context.
    pub fn binding_for_skill(&self, skill_name: &str) -> Option<Arc<LanceDbSkillBinding>> {
        self.skills
            .lock()
            .ok()
            .and_then(|skills| skills.get(skill_name).cloned())
    }
}

/// 中文：为未启用 LanceDB 的 skill 生成稳定状态对象，便于 Lua 侧先判断再调用。
/// English: Build a stable status object for skills that do not enable LanceDB so Lua can check before calling.
pub fn disabled_skill_status_json(skill_name: Option<&str>) -> Value {
    json!({
        "enabled": false,
        "initialized": false,
        "skill_name": skill_name.unwrap_or(""),
        "integration_mode": "dynamic_library",
        "reason": "current skill has not enabled lancedb / 当前 skill 未启用 lancedb"
    })
}

/// 中文：返回当前平台应加载的动态库文件名。
/// English: Return the platform-specific dynamic-library filename that should be loaded.
fn library_file_name() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "vldb_lancedb.dll"
    }
    #[cfg(target_os = "linux")]
    {
        "libvldb_lancedb.so"
    }
    #[cfg(target_os = "macos")]
    {
        "libvldb_lancedb.dylib"
    }
}

/// 中文：按优先级列出宿主可接受的 LanceDB 动态库候选路径。
/// English: List LanceDB dynamic-library candidate paths in host priority order.
fn candidate_library_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    if let Ok(explicit) = std::env::var("VLDB_LANCEDB_LIBRARY") {
        let trimmed = explicit.trim();
        if !trimmed.is_empty() {
            paths.push(PathBuf::from(trimmed));
        }
    }

    let file_name = library_file_name();
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let parent = exe_dir.parent().unwrap_or(exe_dir);
            paths.push(parent.join("libs").join(file_name));
            paths.push(exe_dir.join(file_name));
        }
    }

    if let Ok(current_dir) = std::env::current_dir() {
        paths.push(current_dir.join("output").join("libs").join(file_name));
        paths.push(current_dir.join("third_party").join("deps").join(file_name));
        paths.push(
            current_dir
                .join("..")
                .join("VulcanLocalDataGateway")
                .join("vldb-lancedb")
                .join("target")
                .join("release")
                .join(file_name),
        );
    }

    paths
}
