use libloading::Library;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::ffi::{CStr, CString, c_char};
use std::path::{Path, PathBuf};
use std::ptr;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::lua_skill::{SkillSqliteLogLevel, SkillSqliteMeta};

/// 中文：FFI runtime 句柄前置声明，仅用于跨动态库传递裸指针。
/// English: Forward declaration of the FFI runtime handle used only for raw cross-library pointers.
#[repr(C)]
struct VldbSqliteRuntimeHandle {
    _private: [u8; 0],
}

/// 中文：FFI 数据库句柄前置声明，仅用于跨动态库传递裸指针。
/// English: Forward declaration of the FFI database handle used only for raw cross-library pointers.
#[repr(C)]
struct VldbSqliteDatabaseHandle {
    _private: [u8; 0],
}

/// 中文：FFI 分词结果句柄前置声明。
/// English: Forward declaration of the FFI tokenize-result handle.
#[repr(C)]
struct VldbSqliteTokenizeResultHandle {
    _private: [u8; 0],
}

/// 中文：FFI 自定义词列表句柄前置声明。
/// English: Forward declaration of the FFI custom-word list handle.
#[repr(C)]
struct VldbSqliteCustomWordListHandle {
    _private: [u8; 0],
}

/// 中文：FFI 检索结果句柄前置声明。
/// English: Forward declaration of the FFI search-result handle.
#[repr(C)]
struct VldbSqliteSearchResultHandle {
    _private: [u8; 0],
}

/// 中文：SQLite FFI 返回状态码。
/// English: SQLite FFI status code.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VldbSqliteStatusCode {
    Success = 0,
}

/// 中文：SQLite FFI 分词模式枚举，需与导出头文件严格保持一致。
/// English: SQLite FFI tokenizer-mode enum kept ABI-compatible with the exported header.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VldbSqliteFfiTokenizerMode {
    None = 0,
    Jieba = 1,
}

/// 中文：自定义词修改结果 POD 结构。
/// English: POD result structure for custom-word mutations.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct VldbSqliteDictionaryMutationResultPod {
    success: u8,
    affected_rows: u64,
}

/// 中文：FTS 索引创建结果 POD 结构。
/// English: POD result structure for FTS ensure-index operations.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct VldbSqliteEnsureFtsIndexResultPod {
    success: u8,
    tokenizer_mode: u32,
}

/// 中文：FTS 索引重建结果 POD 结构。
/// English: POD result structure for FTS rebuild-index operations.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct VldbSqliteRebuildFtsIndexResultPod {
    success: u8,
    tokenizer_mode: u32,
    reindexed_rows: u64,
}

/// 中文：FTS 文档写入/删除结果 POD 结构。
/// English: POD result structure for FTS document mutations.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct VldbSqliteFtsMutationResultPod {
    success: u8,
    affected_rows: u64,
}

type RuntimeCreateDefaultFn = unsafe extern "C" fn() -> *mut VldbSqliteRuntimeHandle;
type RuntimeDestroyFn = unsafe extern "C" fn(*mut VldbSqliteRuntimeHandle);
type RuntimeOpenDatabaseFn = unsafe extern "C" fn(
    *mut VldbSqliteRuntimeHandle,
    *const c_char,
) -> *mut VldbSqliteDatabaseHandle;
type DatabaseDestroyFn = unsafe extern "C" fn(*mut VldbSqliteDatabaseHandle);
type DatabaseDbPathFn = unsafe extern "C" fn(*mut VldbSqliteDatabaseHandle) -> *mut c_char;
type StringFreeFn = unsafe extern "C" fn(*mut c_char);
type LastErrorMessageFn = unsafe extern "C" fn() -> *const c_char;
type ClearLastErrorFn = unsafe extern "C" fn();
type DatabaseTokenizeTextFn = unsafe extern "C" fn(
    *mut VldbSqliteDatabaseHandle,
    VldbSqliteFfiTokenizerMode,
    *const c_char,
    u8,
) -> *mut VldbSqliteTokenizeResultHandle;
type TokenizeResultDestroyFn = unsafe extern "C" fn(*mut VldbSqliteTokenizeResultHandle);
type TokenizeResultNormalizedTextFn =
    unsafe extern "C" fn(*mut VldbSqliteTokenizeResultHandle) -> *mut c_char;
type TokenizeResultFtsQueryFn =
    unsafe extern "C" fn(*mut VldbSqliteTokenizeResultHandle) -> *mut c_char;
type TokenizeResultTokenCountFn =
    unsafe extern "C" fn(*mut VldbSqliteTokenizeResultHandle) -> u64;
type TokenizeResultGetTokenFn =
    unsafe extern "C" fn(*mut VldbSqliteTokenizeResultHandle, u64) -> *mut c_char;
type DatabaseUpsertCustomWordFn = unsafe extern "C" fn(
    *mut VldbSqliteDatabaseHandle,
    *const c_char,
    u64,
    *mut VldbSqliteDictionaryMutationResultPod,
) -> i32;
type DatabaseRemoveCustomWordFn = unsafe extern "C" fn(
    *mut VldbSqliteDatabaseHandle,
    *const c_char,
    *mut VldbSqliteDictionaryMutationResultPod,
) -> i32;
type DatabaseListCustomWordsFn =
    unsafe extern "C" fn(*mut VldbSqliteDatabaseHandle) -> *mut VldbSqliteCustomWordListHandle;
type CustomWordListDestroyFn = unsafe extern "C" fn(*mut VldbSqliteCustomWordListHandle);
type CustomWordListLenFn = unsafe extern "C" fn(*mut VldbSqliteCustomWordListHandle) -> u64;
type CustomWordListGetWordFn =
    unsafe extern "C" fn(*mut VldbSqliteCustomWordListHandle, u64) -> *mut c_char;
type CustomWordListGetWeightFn =
    unsafe extern "C" fn(*mut VldbSqliteCustomWordListHandle, u64) -> u64;
type DatabaseEnsureFtsIndexFn = unsafe extern "C" fn(
    *mut VldbSqliteDatabaseHandle,
    *const c_char,
    VldbSqliteFfiTokenizerMode,
    *mut VldbSqliteEnsureFtsIndexResultPod,
) -> i32;
type DatabaseRebuildFtsIndexFn = unsafe extern "C" fn(
    *mut VldbSqliteDatabaseHandle,
    *const c_char,
    VldbSqliteFfiTokenizerMode,
    *mut VldbSqliteRebuildFtsIndexResultPod,
) -> i32;
type DatabaseUpsertFtsDocumentFn = unsafe extern "C" fn(
    *mut VldbSqliteDatabaseHandle,
    *const c_char,
    VldbSqliteFfiTokenizerMode,
    *const c_char,
    *const c_char,
    *const c_char,
    *const c_char,
    *mut VldbSqliteFtsMutationResultPod,
) -> i32;
type DatabaseDeleteFtsDocumentFn = unsafe extern "C" fn(
    *mut VldbSqliteDatabaseHandle,
    *const c_char,
    *const c_char,
    *mut VldbSqliteFtsMutationResultPod,
) -> i32;
type DatabaseSearchFtsFn = unsafe extern "C" fn(
    *mut VldbSqliteDatabaseHandle,
    *const c_char,
    VldbSqliteFfiTokenizerMode,
    *const c_char,
    u32,
    u32,
) -> *mut VldbSqliteSearchResultHandle;
type SearchResultDestroyFn = unsafe extern "C" fn(*mut VldbSqliteSearchResultHandle);
type SearchResultTotalFn = unsafe extern "C" fn(*mut VldbSqliteSearchResultHandle) -> u64;
type SearchResultLenFn = unsafe extern "C" fn(*mut VldbSqliteSearchResultHandle) -> u64;
type SearchResultSourceFn =
    unsafe extern "C" fn(*mut VldbSqliteSearchResultHandle) -> *mut c_char;
type SearchResultQueryModeFn =
    unsafe extern "C" fn(*mut VldbSqliteSearchResultHandle) -> *mut c_char;
type SearchResultGetIdFn =
    unsafe extern "C" fn(*mut VldbSqliteSearchResultHandle, u64) -> *mut c_char;
type SearchResultGetFilePathFn =
    unsafe extern "C" fn(*mut VldbSqliteSearchResultHandle, u64) -> *mut c_char;
type SearchResultGetTitleFn =
    unsafe extern "C" fn(*mut VldbSqliteSearchResultHandle, u64) -> *mut c_char;
type SearchResultGetTitleHighlightFn =
    unsafe extern "C" fn(*mut VldbSqliteSearchResultHandle, u64) -> *mut c_char;
type SearchResultGetContentSnippetFn =
    unsafe extern "C" fn(*mut VldbSqliteSearchResultHandle, u64) -> *mut c_char;
type SearchResultGetScoreFn = unsafe extern "C" fn(*mut VldbSqliteSearchResultHandle, u64) -> f64;
type SearchResultGetRankFn = unsafe extern "C" fn(*mut VldbSqliteSearchResultHandle, u64) -> u64;
type SearchResultGetRawScoreFn =
    unsafe extern "C" fn(*mut VldbSqliteSearchResultHandle, u64) -> f64;

/// 中文：已加载的 SQLite FFI API 表，持有动态库生命周期与全部导出函数指针。
/// English: Loaded SQLite FFI API table that owns the dynamic-library lifetime and all exported function pointers.
struct LoadedSqliteApi {
    _library: Library,
    library_path: PathBuf,
    runtime_create_default: RuntimeCreateDefaultFn,
    runtime_destroy: RuntimeDestroyFn,
    runtime_open_database: RuntimeOpenDatabaseFn,
    database_destroy: DatabaseDestroyFn,
    database_db_path: DatabaseDbPathFn,
    string_free: StringFreeFn,
    last_error_message: LastErrorMessageFn,
    clear_last_error: ClearLastErrorFn,
    database_tokenize_text: DatabaseTokenizeTextFn,
    tokenize_result_destroy: TokenizeResultDestroyFn,
    tokenize_result_normalized_text: TokenizeResultNormalizedTextFn,
    tokenize_result_fts_query: TokenizeResultFtsQueryFn,
    tokenize_result_token_count: TokenizeResultTokenCountFn,
    tokenize_result_get_token: TokenizeResultGetTokenFn,
    database_upsert_custom_word: DatabaseUpsertCustomWordFn,
    database_remove_custom_word: DatabaseRemoveCustomWordFn,
    database_list_custom_words: DatabaseListCustomWordsFn,
    custom_word_list_destroy: CustomWordListDestroyFn,
    custom_word_list_len: CustomWordListLenFn,
    custom_word_list_get_word: CustomWordListGetWordFn,
    custom_word_list_get_weight: CustomWordListGetWeightFn,
    database_ensure_fts_index: DatabaseEnsureFtsIndexFn,
    database_rebuild_fts_index: DatabaseRebuildFtsIndexFn,
    database_upsert_fts_document: DatabaseUpsertFtsDocumentFn,
    database_delete_fts_document: DatabaseDeleteFtsDocumentFn,
    database_search_fts: DatabaseSearchFtsFn,
    search_result_destroy: SearchResultDestroyFn,
    search_result_total: SearchResultTotalFn,
    search_result_len: SearchResultLenFn,
    search_result_source: SearchResultSourceFn,
    search_result_query_mode: SearchResultQueryModeFn,
    search_result_get_id: SearchResultGetIdFn,
    search_result_get_file_path: SearchResultGetFilePathFn,
    search_result_get_title: SearchResultGetTitleFn,
    search_result_get_title_highlight: SearchResultGetTitleHighlightFn,
    search_result_get_content_snippet: SearchResultGetContentSnippetFn,
    search_result_get_score: SearchResultGetScoreFn,
    search_result_get_rank: SearchResultGetRankFn,
    search_result_get_raw_score: SearchResultGetRawScoreFn,
}

/// 中文：动态库句柄与函数表初始化后只读，跨线程共享由外层锁负责保护。
/// English: The loaded library and function table stay immutable after initialization, while outer locks protect shared access.
unsafe impl Send for LoadedSqliteApi {}
unsafe impl Sync for LoadedSqliteApi {}

impl LoadedSqliteApi {
    /// 中文：按宿主约定加载 SQLite 动态库，优先查找显式环境变量和运行时目录。
    /// English: Load the SQLite dynamic library using host conventions, preferring an explicit environment variable and runtime directories.
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
            "SQLite dynamic library not found or failed to load / 未找到或无法加载 SQLite 动态库: {}",
            last_error
        ))
    }

    /// 中文：从已打开的动态库中复制所需函数指针，并保留库句柄防止提前卸载。
    /// English: Copy required exported function pointers from the opened dynamic library while retaining the library handle.
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
            runtime_create_default: load_symbol!(
                "vldb_sqlite_runtime_create_default",
                RuntimeCreateDefaultFn
            ),
            runtime_destroy: load_symbol!("vldb_sqlite_runtime_destroy", RuntimeDestroyFn),
            runtime_open_database: load_symbol!(
                "vldb_sqlite_runtime_open_database",
                RuntimeOpenDatabaseFn
            ),
            database_destroy: load_symbol!("vldb_sqlite_database_destroy", DatabaseDestroyFn),
            database_db_path: load_symbol!("vldb_sqlite_database_db_path", DatabaseDbPathFn),
            string_free: load_symbol!("vldb_sqlite_string_free", StringFreeFn),
            last_error_message: load_symbol!(
                "vldb_sqlite_last_error_message",
                LastErrorMessageFn
            ),
            clear_last_error: load_symbol!("vldb_sqlite_clear_last_error", ClearLastErrorFn),
            database_tokenize_text: load_symbol!(
                "vldb_sqlite_database_tokenize_text",
                DatabaseTokenizeTextFn
            ),
            tokenize_result_destroy: load_symbol!(
                "vldb_sqlite_tokenize_result_destroy",
                TokenizeResultDestroyFn
            ),
            tokenize_result_normalized_text: load_symbol!(
                "vldb_sqlite_tokenize_result_normalized_text",
                TokenizeResultNormalizedTextFn
            ),
            tokenize_result_fts_query: load_symbol!(
                "vldb_sqlite_tokenize_result_fts_query",
                TokenizeResultFtsQueryFn
            ),
            tokenize_result_token_count: load_symbol!(
                "vldb_sqlite_tokenize_result_token_count",
                TokenizeResultTokenCountFn
            ),
            tokenize_result_get_token: load_symbol!(
                "vldb_sqlite_tokenize_result_get_token",
                TokenizeResultGetTokenFn
            ),
            database_upsert_custom_word: load_symbol!(
                "vldb_sqlite_database_upsert_custom_word",
                DatabaseUpsertCustomWordFn
            ),
            database_remove_custom_word: load_symbol!(
                "vldb_sqlite_database_remove_custom_word",
                DatabaseRemoveCustomWordFn
            ),
            database_list_custom_words: load_symbol!(
                "vldb_sqlite_database_list_custom_words",
                DatabaseListCustomWordsFn
            ),
            custom_word_list_destroy: load_symbol!(
                "vldb_sqlite_custom_word_list_destroy",
                CustomWordListDestroyFn
            ),
            custom_word_list_len: load_symbol!(
                "vldb_sqlite_custom_word_list_len",
                CustomWordListLenFn
            ),
            custom_word_list_get_word: load_symbol!(
                "vldb_sqlite_custom_word_list_get_word",
                CustomWordListGetWordFn
            ),
            custom_word_list_get_weight: load_symbol!(
                "vldb_sqlite_custom_word_list_get_weight",
                CustomWordListGetWeightFn
            ),
            database_ensure_fts_index: load_symbol!(
                "vldb_sqlite_database_ensure_fts_index",
                DatabaseEnsureFtsIndexFn
            ),
            database_rebuild_fts_index: load_symbol!(
                "vldb_sqlite_database_rebuild_fts_index",
                DatabaseRebuildFtsIndexFn
            ),
            database_upsert_fts_document: load_symbol!(
                "vldb_sqlite_database_upsert_fts_document",
                DatabaseUpsertFtsDocumentFn
            ),
            database_delete_fts_document: load_symbol!(
                "vldb_sqlite_database_delete_fts_document",
                DatabaseDeleteFtsDocumentFn
            ),
            database_search_fts: load_symbol!(
                "vldb_sqlite_database_search_fts",
                DatabaseSearchFtsFn
            ),
            search_result_destroy: load_symbol!(
                "vldb_sqlite_search_result_destroy",
                SearchResultDestroyFn
            ),
            search_result_total: load_symbol!(
                "vldb_sqlite_search_result_total",
                SearchResultTotalFn
            ),
            search_result_len: load_symbol!("vldb_sqlite_search_result_len", SearchResultLenFn),
            search_result_source: load_symbol!(
                "vldb_sqlite_search_result_source",
                SearchResultSourceFn
            ),
            search_result_query_mode: load_symbol!(
                "vldb_sqlite_search_result_query_mode",
                SearchResultQueryModeFn
            ),
            search_result_get_id: load_symbol!(
                "vldb_sqlite_search_result_get_id",
                SearchResultGetIdFn
            ),
            search_result_get_file_path: load_symbol!(
                "vldb_sqlite_search_result_get_file_path",
                SearchResultGetFilePathFn
            ),
            search_result_get_title: load_symbol!(
                "vldb_sqlite_search_result_get_title",
                SearchResultGetTitleFn
            ),
            search_result_get_title_highlight: load_symbol!(
                "vldb_sqlite_search_result_get_title_highlight",
                SearchResultGetTitleHighlightFn
            ),
            search_result_get_content_snippet: load_symbol!(
                "vldb_sqlite_search_result_get_content_snippet",
                SearchResultGetContentSnippetFn
            ),
            search_result_get_score: load_symbol!(
                "vldb_sqlite_search_result_get_score",
                SearchResultGetScoreFn
            ),
            search_result_get_rank: load_symbol!(
                "vldb_sqlite_search_result_get_rank",
                SearchResultGetRankFn
            ),
            search_result_get_raw_score: load_symbol!(
                "vldb_sqlite_search_result_get_raw_score",
                SearchResultGetRawScoreFn
            ),
            _library: library,
            library_path,
        })
    }

    /// 中文：读取最近一次 FFI 调用错误并转换成稳定 Rust 字符串。
    /// English: Read the latest FFI error and convert it into a stable Rust string.
    fn take_last_error_message(&self) -> String {
        unsafe {
            let ptr = (self.last_error_message)();
            let text = if ptr.is_null() {
                "unknown SQLite host error / 未知 SQLite 宿主错误".to_string()
            } else {
                CStr::from_ptr(ptr).to_string_lossy().to_string()
            };
            (self.clear_last_error)();
            text
        }
    }

    /// 中文：释放动态库分配的字符串并转换成 Rust `String`。
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

    /// 中文：将动态库分配的可选字符串转换成 Rust `Option<String>`。
    /// English: Convert a dynamic-library allocated optional string into Rust `Option<String>`.
    fn take_optional_string(&self, ptr: *mut c_char) -> Option<String> {
        if ptr.is_null() {
            return None;
        }
        unsafe {
            let text = CStr::from_ptr(ptr).to_string_lossy().to_string();
            (self.string_free)(ptr);
            Some(text)
        }
    }
}

/// 中文：单个 skill 的 SQLite 句柄集合，由宿主统一管理生命周期。
/// English: SQLite handle set for a single skill, with lifetime managed centrally by the host.
struct SkillHandleState {
    runtime: *mut VldbSqliteRuntimeHandle,
    database: *mut VldbSqliteDatabaseHandle,
}

/// 中文：FFI 句柄仅通过宿主互斥量串行访问，跨线程共享由宿主统一控制。
/// English: FFI handles are accessed only behind a host-side mutex, with all cross-thread sharing managed by the host.
unsafe impl Send for SkillHandleState {}

/// 中文：启用 SQLite 的 skill 所绑定的数据库上下文。
/// English: Database context bound to one SQLite-enabled skill.
pub struct SqliteSkillBinding {
    api: Arc<LoadedSqliteApi>,
    skill_name: String,
    skill_dir_name: String,
    database_path: String,
    config: SkillSqliteMeta,
    handles: Mutex<SkillHandleState>,
}

impl SqliteSkillBinding {
    /// 中文：返回当前 skill 的稳定 SQLite 状态信息；无论启用与否，结构都保持稳定。
    /// English: Return the stable SQLite status payload for the current skill; the response shape stays stable whether enabled or disabled.
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

    /// 中文：返回当前 skill 所绑定 SQLite 的基础信息。
    /// English: Return basic information about the SQLite binding for the current skill.
    pub fn info_json(&self) -> Value {
        self.status_json()
    }

    /// 中文：执行文本分词，并返回标准化结果。
    /// English: Execute text tokenization and return a normalized result payload.
    pub fn tokenize_text_json(&self, input: &Value) -> Result<Value, String> {
        let tokenizer_mode = parse_tokenizer_mode(
            input.get("tokenizer_mode")
                .or_else(|| input.get("mode"))
                .and_then(Value::as_str)
                .unwrap_or("none"),
        )?;
        let text = require_string_field(input, "text")?;
        let search_mode = input
            .get("search_mode")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        self.log_info(
            "tokenize_text",
            Some(format!(
                "tokenizer_mode={} search_mode={}",
                tokenizer_mode_name(tokenizer_mode),
                search_mode
            )),
        );
        let started_at = Instant::now();
        let guard = self.lock_handles()?;
        let text_cstr = to_cstring(text, "text")?;
        unsafe {
            let handle = (self.api.database_tokenize_text)(
                guard.database,
                tokenizer_mode,
                text_cstr.as_ptr(),
                bool_to_u8(search_mode),
            );
            if handle.is_null() {
                drop(guard);
                let error = self.api.take_last_error_message();
                self.log_warning("tokenize_text", &error);
                return Err(error);
            }

            let normalized_text = self
                .api
                .take_owned_string((self.api.tokenize_result_normalized_text)(handle))?;
            let fts_query = self
                .api
                .take_owned_string((self.api.tokenize_result_fts_query)(handle))?;
            let token_count = (self.api.tokenize_result_token_count)(handle);
            let mut tokens = Vec::with_capacity(token_count as usize);
            for index in 0..token_count {
                if let Some(token) = self
                    .api
                    .take_optional_string((self.api.tokenize_result_get_token)(handle, index))
                {
                    tokens.push(Value::String(token));
                }
            }
            (self.api.tokenize_result_destroy)(handle);
            drop(guard);
            self.log_if_slow("tokenize_text", started_at, None);
            Ok(json!({
                "success": true,
                "tokenizer_mode": tokenizer_mode_name(tokenizer_mode),
                "normalized_text": normalized_text,
                "fts_query": fts_query,
                "tokens": tokens,
            }))
        }
    }

    /// 中文：写入或更新自定义词。
    /// English: Insert or update a custom dictionary word.
    pub fn upsert_custom_word_json(&self, input: &Value) -> Result<Value, String> {
        let word = require_string_field(input, "word")?;
        let weight = input.get("weight").and_then(Value::as_u64).unwrap_or(1);
        self.log_info("upsert_custom_word", Some(format!("word={}", word)));
        let started_at = Instant::now();
        let guard = self.lock_handles()?;
        let word_cstr = to_cstring(word, "word")?;
        let mut result = VldbSqliteDictionaryMutationResultPod {
            success: 0,
            affected_rows: 0,
        };
        let status = unsafe {
            (self.api.database_upsert_custom_word)(
                guard.database,
                word_cstr.as_ptr(),
                weight,
                &mut result,
            )
        };
        drop(guard);
        self.log_if_slow("upsert_custom_word", started_at, None);
        ensure_status(&self.api, status, "upsert_custom_word")?;
        Ok(json!({
            "success": u8_to_bool(result.success),
            "affected_rows": result.affected_rows,
            "word": word,
            "weight": weight,
        }))
    }

    /// 中文：删除自定义词。
    /// English: Remove a custom dictionary word.
    pub fn remove_custom_word_json(&self, input: &Value) -> Result<Value, String> {
        let word = require_string_field(input, "word")?;
        self.log_info("remove_custom_word", Some(format!("word={}", word)));
        let started_at = Instant::now();
        let guard = self.lock_handles()?;
        let word_cstr = to_cstring(word, "word")?;
        let mut result = VldbSqliteDictionaryMutationResultPod {
            success: 0,
            affected_rows: 0,
        };
        let status = unsafe {
            (self.api.database_remove_custom_word)(guard.database, word_cstr.as_ptr(), &mut result)
        };
        drop(guard);
        self.log_if_slow("remove_custom_word", started_at, None);
        ensure_status(&self.api, status, "remove_custom_word")?;
        Ok(json!({
            "success": u8_to_bool(result.success),
            "affected_rows": result.affected_rows,
            "word": word,
        }))
    }

    /// 中文：列出当前数据库中启用的自定义词。
    /// English: List enabled custom dictionary words from the current database.
    pub fn list_custom_words_json(&self) -> Result<Value, String> {
        self.log_info("list_custom_words", None);
        let started_at = Instant::now();
        let guard = self.lock_handles()?;
        unsafe {
            let list_handle = (self.api.database_list_custom_words)(guard.database);
            if list_handle.is_null() {
                drop(guard);
                let error = self.api.take_last_error_message();
                self.log_warning("list_custom_words", &error);
                return Err(error);
            }

            let len = (self.api.custom_word_list_len)(list_handle);
            let mut words = Vec::with_capacity(len as usize);
            for index in 0..len {
                let word = self
                    .api
                    .take_optional_string((self.api.custom_word_list_get_word)(list_handle, index))
                    .unwrap_or_default();
                let weight = (self.api.custom_word_list_get_weight)(list_handle, index);
                words.push(json!({
                    "word": word,
                    "weight": weight,
                }));
            }
            (self.api.custom_word_list_destroy)(list_handle);
            drop(guard);
            self.log_if_slow("list_custom_words", started_at, Some(format!("count={}", len)));
            Ok(json!({
                "success": true,
                "total": len,
                "words": words,
            }))
        }
    }

    /// 中文：确保指定 FTS 索引存在。
    /// English: Ensure the specified FTS index exists.
    pub fn ensure_fts_index_json(&self, input: &Value) -> Result<Value, String> {
        let index_name = require_string_field(input, "index_name")?;
        let tokenizer_mode = parse_tokenizer_mode(
            input.get("tokenizer_mode")
                .or_else(|| input.get("mode"))
                .and_then(Value::as_str)
                .unwrap_or("none"),
        )?;
        self.log_info("ensure_fts_index", Some(format!("index_name={}", index_name)));
        let started_at = Instant::now();
        let guard = self.lock_handles()?;
        let index_cstr = to_cstring(index_name, "index_name")?;
        let mut result = VldbSqliteEnsureFtsIndexResultPod {
            success: 0,
            tokenizer_mode: tokenizer_mode as u32,
        };
        let status = unsafe {
            (self.api.database_ensure_fts_index)(
                guard.database,
                index_cstr.as_ptr(),
                tokenizer_mode,
                &mut result,
            )
        };
        drop(guard);
        self.log_if_slow("ensure_fts_index", started_at, None);
        ensure_status(&self.api, status, "ensure_fts_index")?;
        Ok(json!({
            "success": u8_to_bool(result.success),
            "index_name": index_name,
            "tokenizer_mode": tokenizer_mode_name_from_u32(result.tokenizer_mode),
        }))
    }

    /// 中文：使用当前词典和分词模式重建 FTS 索引。
    /// English: Rebuild an FTS index using the current dictionary and tokenizer mode.
    pub fn rebuild_fts_index_json(&self, input: &Value) -> Result<Value, String> {
        let index_name = require_string_field(input, "index_name")?;
        let tokenizer_mode = parse_tokenizer_mode(
            input.get("tokenizer_mode")
                .or_else(|| input.get("mode"))
                .and_then(Value::as_str)
                .unwrap_or("none"),
        )?;
        self.log_info("rebuild_fts_index", Some(format!("index_name={}", index_name)));
        let started_at = Instant::now();
        let guard = self.lock_handles()?;
        let index_cstr = to_cstring(index_name, "index_name")?;
        let mut result = VldbSqliteRebuildFtsIndexResultPod {
            success: 0,
            tokenizer_mode: tokenizer_mode as u32,
            reindexed_rows: 0,
        };
        let status = unsafe {
            (self.api.database_rebuild_fts_index)(
                guard.database,
                index_cstr.as_ptr(),
                tokenizer_mode,
                &mut result,
            )
        };
        drop(guard);
        self.log_if_slow("rebuild_fts_index", started_at, None);
        ensure_status(&self.api, status, "rebuild_fts_index")?;
        Ok(json!({
            "success": u8_to_bool(result.success),
            "index_name": index_name,
            "tokenizer_mode": tokenizer_mode_name_from_u32(result.tokenizer_mode),
            "reindexed_rows": result.reindexed_rows,
        }))
    }

    /// 中文：写入或更新一条 FTS 文档。
    /// English: Insert or update a single FTS document.
    pub fn upsert_fts_document_json(&self, input: &Value) -> Result<Value, String> {
        let index_name = require_string_field(input, "index_name")?;
        let tokenizer_mode = parse_tokenizer_mode(
            input.get("tokenizer_mode")
                .or_else(|| input.get("mode"))
                .and_then(Value::as_str)
                .unwrap_or("none"),
        )?;
        let id = require_string_field(input, "id")?;
        let file_path = require_string_field(input, "file_path")?;
        let title = input.get("title").and_then(Value::as_str).unwrap_or("");
        let content = input.get("content").and_then(Value::as_str).unwrap_or("");
        self.log_info(
            "upsert_fts_document",
            Some(format!("index_name={} id={}", index_name, id)),
        );
        let started_at = Instant::now();
        let guard = self.lock_handles()?;
        let index_cstr = to_cstring(index_name, "index_name")?;
        let id_cstr = to_cstring(id, "id")?;
        let file_path_cstr = to_cstring(file_path, "file_path")?;
        let title_cstr = to_cstring(title, "title")?;
        let content_cstr = to_cstring(content, "content")?;
        let mut result = VldbSqliteFtsMutationResultPod {
            success: 0,
            affected_rows: 0,
        };
        let status = unsafe {
            (self.api.database_upsert_fts_document)(
                guard.database,
                index_cstr.as_ptr(),
                tokenizer_mode,
                id_cstr.as_ptr(),
                file_path_cstr.as_ptr(),
                title_cstr.as_ptr(),
                content_cstr.as_ptr(),
                &mut result,
            )
        };
        drop(guard);
        self.log_if_slow("upsert_fts_document", started_at, None);
        ensure_status(&self.api, status, "upsert_fts_document")?;
        Ok(json!({
            "success": u8_to_bool(result.success),
            "affected_rows": result.affected_rows,
            "index_name": index_name,
            "id": id,
        }))
    }

    /// 中文：删除一条 FTS 文档。
    /// English: Delete a single FTS document.
    pub fn delete_fts_document_json(&self, input: &Value) -> Result<Value, String> {
        let index_name = require_string_field(input, "index_name")?;
        let id = require_string_field(input, "id")?;
        self.log_info(
            "delete_fts_document",
            Some(format!("index_name={} id={}", index_name, id)),
        );
        let started_at = Instant::now();
        let guard = self.lock_handles()?;
        let index_cstr = to_cstring(index_name, "index_name")?;
        let id_cstr = to_cstring(id, "id")?;
        let mut result = VldbSqliteFtsMutationResultPod {
            success: 0,
            affected_rows: 0,
        };
        let status = unsafe {
            (self.api.database_delete_fts_document)(
                guard.database,
                index_cstr.as_ptr(),
                id_cstr.as_ptr(),
                &mut result,
            )
        };
        drop(guard);
        self.log_if_slow("delete_fts_document", started_at, None);
        ensure_status(&self.api, status, "delete_fts_document")?;
        Ok(json!({
            "success": u8_to_bool(result.success),
            "affected_rows": result.affected_rows,
            "index_name": index_name,
            "id": id,
        }))
    }

    /// 中文：执行 FTS 检索并返回富结果结构。
    /// English: Execute FTS search and return a rich result payload.
    pub fn search_fts_json(&self, input: &Value) -> Result<Value, String> {
        let index_name = require_string_field(input, "index_name")?;
        let tokenizer_mode = parse_tokenizer_mode(
            input.get("tokenizer_mode")
                .or_else(|| input.get("mode"))
                .and_then(Value::as_str)
                .unwrap_or("none"),
        )?;
        let query = require_string_field(input, "query")?;
        let limit = input.get("limit").and_then(Value::as_u64).unwrap_or(10) as u32;
        let offset = input.get("offset").and_then(Value::as_u64).unwrap_or(0) as u32;
        self.log_info(
            "search_fts",
            Some(format!(
                "index_name={} tokenizer_mode={} limit={} offset={}",
                index_name,
                tokenizer_mode_name(tokenizer_mode),
                limit,
                offset
            )),
        );
        let started_at = Instant::now();
        let guard = self.lock_handles()?;
        let index_cstr = to_cstring(index_name, "index_name")?;
        let query_cstr = to_cstring(query, "query")?;
        unsafe {
            let result_handle = (self.api.database_search_fts)(
                guard.database,
                index_cstr.as_ptr(),
                tokenizer_mode,
                query_cstr.as_ptr(),
                limit,
                offset,
            );
            if result_handle.is_null() {
                drop(guard);
                let error = self.api.take_last_error_message();
                self.log_warning("search_fts", &error);
                return Err(error);
            }

            let total = (self.api.search_result_total)(result_handle);
            let len = (self.api.search_result_len)(result_handle);
            let source = self
                .api
                .take_optional_string((self.api.search_result_source)(result_handle))
                .unwrap_or_else(|| "sqlite_fts".to_string());
            let query_mode = self
                .api
                .take_optional_string((self.api.search_result_query_mode)(result_handle))
                .unwrap_or_else(|| "fts".to_string());
            let mut hits = Vec::with_capacity(len as usize);
            for index in 0..len {
                hits.push(json!({
                    "id": self.api.take_optional_string((self.api.search_result_get_id)(result_handle, index)).unwrap_or_default(),
                    "file_path": self.api.take_optional_string((self.api.search_result_get_file_path)(result_handle, index)).unwrap_or_default(),
                    "title": self.api.take_optional_string((self.api.search_result_get_title)(result_handle, index)).unwrap_or_default(),
                    "title_highlight": self.api.take_optional_string((self.api.search_result_get_title_highlight)(result_handle, index)).unwrap_or_default(),
                    "content_snippet": self.api.take_optional_string((self.api.search_result_get_content_snippet)(result_handle, index)).unwrap_or_default(),
                    "score": (self.api.search_result_get_score)(result_handle, index),
                    "rank": (self.api.search_result_get_rank)(result_handle, index),
                    "raw_score": (self.api.search_result_get_raw_score)(result_handle, index),
                }));
            }
            (self.api.search_result_destroy)(result_handle);
            drop(guard);
            self.log_if_slow("search_fts", started_at, Some(format!("hits={}", len)));
            Ok(json!({
                "success": true,
                "index_name": index_name,
                "tokenizer_mode": tokenizer_mode_name(tokenizer_mode),
                "source": source,
                "query_mode": query_mode,
                "total": total,
                "hits": hits,
            }))
        }
    }

    /// 中文：按配置输出普通信息级日志。
    /// English: Emit informational logs according to the configured skill policy.
    fn log_info(&self, operation: &str, extra: Option<String>) {
        if self.config.log_level == SkillSqliteLogLevel::Info {
            match extra {
                Some(extra) => eprintln!(
                    "[Sqlite:info] skill={} db={} op={} {}",
                    self.skill_name, self.skill_dir_name, operation, extra
                ),
                None => eprintln!(
                    "[Sqlite:info] skill={} db={} op={}",
                    self.skill_name, self.skill_dir_name, operation
                ),
            }
        }
    }

    /// 中文：按慢日志配置输出慢操作告警。
    /// English: Emit slow-operation warnings according to the slow-log configuration.
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
                "[Sqlite:slow] skill={} db={} op={} elapsed_ms={} {}",
                self.skill_name, self.skill_dir_name, operation, elapsed_ms, extra
            ),
            None => eprintln!(
                "[Sqlite:slow] skill={} db={} op={} elapsed_ms={}",
                self.skill_name, self.skill_dir_name, operation, elapsed_ms
            ),
        }
    }

    /// 中文：按配置输出告警级日志，通常用于 FFI 调用失败。
    /// English: Emit warning-level logs according to configuration, usually for FFI call failures.
    fn log_warning(&self, operation: &str, message: &str) {
        if matches!(
            self.config.log_level,
            SkillSqliteLogLevel::Info | SkillSqliteLogLevel::Warning
        ) {
            eprintln!(
                "[Sqlite:warn] skill={} db={} op={} message={}",
                self.skill_name, self.skill_dir_name, operation, message
            );
        }
    }

    /// 中文：获取句柄锁，确保同一个 skill 的 SQLite FFI 调用按顺序串行执行。
    /// English: Acquire the handle lock so SQLite FFI calls for the same skill execute serially.
    fn lock_handles(&self) -> Result<std::sync::MutexGuard<'_, SkillHandleState>, String> {
        self.handles.lock().map_err(|_| {
            "failed to acquire SQLite handle lock / 获取 SQLite 句柄锁失败".to_string()
        })
    }
}

impl Drop for SqliteSkillBinding {
    /// 中文：在 skill 生命周期结束时统一释放数据库句柄与 runtime。
    /// English: Release the database handle and runtime together when the skill binding is dropped.
    fn drop(&mut self) {
        if let Ok(mut guard) = self.handles.lock() {
            unsafe {
                if !guard.database.is_null() {
                    (self.api.database_destroy)(guard.database);
                    guard.database = ptr::null_mut();
                }
                if !guard.runtime.is_null() {
                    (self.api.runtime_destroy)(guard.runtime);
                    guard.runtime = ptr::null_mut();
                }
            }
        }
    }
}

/// 中文：按 skill 维度维护 SQLite 绑定，负责启用后的自动创建与长期复用。
/// English: Maintain SQLite bindings per skill, auto-creating and reusing them for enabled skills.
pub struct SqliteSkillHost {
    api: Arc<LoadedSqliteApi>,
    skills: Mutex<HashMap<String, Arc<SqliteSkillBinding>>>,
}

impl SqliteSkillHost {
    /// 中文：创建宿主级 SQLite 技能管理器，并立即加载动态库。
    /// English: Create the host-side SQLite skill manager and load the dynamic library immediately.
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            api: Arc::new(LoadedSqliteApi::load()?),
            skills: Mutex::new(HashMap::new()),
        })
    }

    /// 中文：为启用 SQLite 的 skill 注册固定数据库绑定；同一个 skill 只会创建一次。
    /// English: Register a fixed database binding for an SQLite-enabled skill; each skill is created only once.
    pub fn register_skill(
        &self,
        skill_name: &str,
        skill_dir: &Path,
        config: SkillSqliteMeta,
    ) -> Result<Arc<SqliteSkillBinding>, String> {
        let mut guard = self.skills.lock().map_err(|_| {
            "failed to acquire SQLite skill registry lock / 获取 SQLite 技能注册表锁失败".to_string()
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
        let db_dir = skills_root.join("__database").join(&skill_dir_name);
        std::fs::create_dir_all(&db_dir).map_err(|error| {
            format!(
                "failed to create SQLite directory {}: {} / 创建 SQLite 目录失败: {}",
                db_dir.display(),
                error,
                error
            )
        })?;
        let db_path = db_dir.join(format!("{}.sqlite3", skill_dir_name));
        let database_path = db_path.to_string_lossy().to_string();
        let database_cstr = CString::new(database_path.clone()).map_err(|_| {
            "database path contains interior NUL bytes / 数据库路径包含 NUL 字节".to_string()
        })?;

        let runtime = unsafe { (self.api.runtime_create_default)() };
        if runtime.is_null() {
            return Err(self.api.take_last_error_message());
        }

        let database = unsafe { (self.api.runtime_open_database)(runtime, database_cstr.as_ptr()) };
        if database.is_null() {
            unsafe {
                (self.api.runtime_destroy)(runtime);
            }
            return Err(self.api.take_last_error_message());
        }

        let resolved_path =
            unsafe { self.api.take_owned_string((self.api.database_db_path)(database)) }
                .unwrap_or(database_path.clone());

        let binding = Arc::new(SqliteSkillBinding {
            api: self.api.clone(),
            skill_name: skill_name.to_string(),
            skill_dir_name,
            database_path: resolved_path,
            config,
            handles: Mutex::new(SkillHandleState { runtime, database }),
        });
        guard.insert(skill_name.to_string(), binding.clone());
        Ok(binding)
    }

    /// 中文：按 skill 名称获取已注册绑定，供 Lua 注入与跨 skill 调用恢复上下文使用。
    /// English: Fetch a registered binding by skill name so Lua injection and cross-skill calls can restore context.
    pub fn binding_for_skill(&self, skill_name: &str) -> Option<Arc<SqliteSkillBinding>> {
        self.skills
            .lock()
            .ok()
            .and_then(|skills| skills.get(skill_name).cloned())
    }
}

/// 中文：为未启用 SQLite 的 skill 生成稳定状态对象，便于 Lua 侧先判断再调用。
/// English: Build a stable status object for skills without SQLite enabled so Lua can check before calling.
pub fn disabled_skill_status_json(skill_name: Option<&str>) -> Value {
    json!({
        "enabled": false,
        "initialized": false,
        "skill_name": skill_name.unwrap_or(""),
        "integration_mode": "dynamic_library",
        "reason": "current skill has not enabled sqlite / 当前 skill 未启用 sqlite"
    })
}

/// 中文：将文本分词模式字符串解析为 FFI 枚举。
/// English: Parse a tokenizer-mode text label into the FFI enum.
fn parse_tokenizer_mode(text: &str) -> Result<VldbSqliteFfiTokenizerMode, String> {
    match text.trim().to_ascii_lowercase().as_str() {
        "" | "none" => Ok(VldbSqliteFfiTokenizerMode::None),
        "jieba" => Ok(VldbSqliteFfiTokenizerMode::Jieba),
        other => Err(format!(
            "unsupported sqlite tokenizer mode: {} / 不支持的 sqlite 分词模式: {}",
            other, other
        )),
    }
}

/// 中文：将 FFI 分词模式转换成稳定字符串。
/// English: Convert the FFI tokenizer mode into a stable string label.
fn tokenizer_mode_name(mode: VldbSqliteFfiTokenizerMode) -> &'static str {
    match mode {
        VldbSqliteFfiTokenizerMode::None => "none",
        VldbSqliteFfiTokenizerMode::Jieba => "jieba",
    }
}

/// 中文：将 FFI 返回的分词模式数值转换成稳定字符串。
/// English: Convert the tokenizer-mode integer returned by FFI into a stable string label.
fn tokenizer_mode_name_from_u32(mode: u32) -> &'static str {
    match mode {
        1 => "jieba",
        _ => "none",
    }
}

/// 中文：将布尔值编码为 FFI 所使用的 `u8`。
/// English: Encode a boolean value as the `u8` representation used by the FFI.
fn bool_to_u8(value: bool) -> u8 {
    if value { 1 } else { 0 }
}

/// 中文：将 FFI `u8` 布尔值转换为 Rust 布尔值。
/// English: Convert an FFI `u8` boolean into a Rust boolean.
fn u8_to_bool(value: u8) -> bool {
    value != 0
}

/// 中文：确保 JSON 请求中存在指定字符串字段。
/// English: Ensure that a required string field exists in the JSON request.
fn require_string_field<'a>(input: &'a Value, field_name: &str) -> Result<&'a str, String> {
    input
        .get(field_name)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            format!(
                "missing or empty field `{}` / 缺少或为空的字段 `{}`",
                field_name, field_name
            )
        })
}

/// 中文：将 Rust 字符串转换为 C 字符串，统一校验 NUL 字节。
/// English: Convert a Rust string into a C string while uniformly validating interior NUL bytes.
fn to_cstring(text: &str, field_name: &str) -> Result<CString, String> {
    CString::new(text).map_err(|_| {
        format!(
            "field `{}` contains interior NUL bytes / 字段 `{}` 包含 NUL 字节",
            field_name, field_name
        )
    })
}

/// 中文：检查 FFI 返回状态码，并在失败时转换成宿主级错误文本。
/// English: Check the FFI return status code and convert failures into a host-level error string.
fn ensure_status(api: &LoadedSqliteApi, status: i32, operation: &str) -> Result<(), String> {
    if status == VldbSqliteStatusCode::Success as i32 {
        return Ok(());
    }
    let error = api.take_last_error_message();
    Err(format!(
        "{} failed: {} / {} 失败: {}",
        operation, error, operation, error
    ))
}

/// 中文：返回当前平台应加载的 SQLite 动态库文件名。
/// English: Return the platform-specific SQLite dynamic-library filename to load.
fn library_file_name() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "vldb_sqlite.dll"
    }
    #[cfg(target_os = "linux")]
    {
        "libvldb_sqlite.so"
    }
    #[cfg(target_os = "macos")]
    {
        "libvldb_sqlite.dylib"
    }
}

/// 中文：按优先级列出宿主可接受的 SQLite 动态库候选路径。
/// English: List SQLite dynamic-library candidate paths in host priority order.
fn candidate_library_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    if let Ok(explicit) = std::env::var("VLDB_SQLITE_LIBRARY") {
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
                .join("vldb-sqlite")
                .join("target")
                .join("release")
                .join(file_name),
        );
    }

    paths
}
