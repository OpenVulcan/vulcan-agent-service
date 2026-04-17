use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
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

/// 中文：FFI 通用 SQL 执行结果句柄前置声明。
/// English: Forward declaration of the FFI shared SQL execute-result handle.
#[repr(C)]
struct VldbSqliteExecuteResultHandle {
    _private: [u8; 0],
}

/// 中文：FFI JSON 查询结果句柄前置声明。
/// English: Forward declaration of the FFI JSON-query result handle.
#[repr(C)]
struct VldbSqliteQueryJsonResultHandle {
    _private: [u8; 0],
}

/// 中文：FFI QueryStream 结果句柄前置声明。
/// English: Forward declaration of the FFI QueryStream result handle.
#[repr(C)]
struct VldbSqliteQueryStreamHandle {
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

/// 中文：FFI 字节视图结构，供 bytes 参数使用。
/// English: FFI byte-view structure used for bytes parameters.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct VldbSqliteByteView {
    data: *const u8,
    len: u64,
}

/// 中文：FFI 可释放字节缓冲区，供 QueryStream chunk getter 返回。
/// English: FFI releasable byte buffer returned by QueryStream chunk getters.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct VldbSqliteByteBuffer {
    data: *mut u8,
    len: u64,
    cap: u64,
}

/// 中文：FFI SQL 值类型枚举，必须与头文件定义保持一致。
/// English: FFI SQL value-kind enum kept ABI-compatible with the exported header.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VldbSqliteFfiValueKind {
    Null = 0,
    Int64 = 1,
    Float64 = 2,
    String = 3,
    Bytes = 4,
    Bool = 5,
}

/// 中文：FFI SQL 参数值结构。
/// English: FFI SQL parameter value structure.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct VldbSqliteFfiValue {
    kind: VldbSqliteFfiValueKind,
    int64_value: i64,
    float64_value: f64,
    string_value: *const c_char,
    bytes_value: VldbSqliteByteView,
    bool_value: u8,
}

/// 中文：FFI SQL 参数切片结构，用于批量执行。
/// English: FFI SQL parameter-slice structure used by batch execution.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct VldbSqliteFfiValueSlice {
    values: *const VldbSqliteFfiValue,
    len: u64,
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
type LibraryInfoJsonFn = unsafe extern "C" fn() -> *mut c_char;
type DatabaseExecuteScriptFn = unsafe extern "C" fn(
    *mut VldbSqliteDatabaseHandle,
    *const c_char,
    *const VldbSqliteFfiValue,
    u64,
    *const c_char,
) -> *mut VldbSqliteExecuteResultHandle;
type DatabaseExecuteBatchFn = unsafe extern "C" fn(
    *mut VldbSqliteDatabaseHandle,
    *const c_char,
    *const VldbSqliteFfiValueSlice,
    u64,
) -> *mut VldbSqliteExecuteResultHandle;
type DatabaseQueryJsonFn = unsafe extern "C" fn(
    *mut VldbSqliteDatabaseHandle,
    *const c_char,
    *const VldbSqliteFfiValue,
    u64,
    *const c_char,
) -> *mut VldbSqliteQueryJsonResultHandle;
type DatabaseQueryStreamFn = unsafe extern "C" fn(
    *mut VldbSqliteDatabaseHandle,
    *const c_char,
    *const VldbSqliteFfiValue,
    u64,
    *const c_char,
    u64,
) -> *mut VldbSqliteQueryStreamHandle;
type ExecuteResultDestroyFn = unsafe extern "C" fn(*mut VldbSqliteExecuteResultHandle);
type ExecuteResultSuccessFn = unsafe extern "C" fn(*mut VldbSqliteExecuteResultHandle) -> u8;
type ExecuteResultMessageFn = unsafe extern "C" fn(*mut VldbSqliteExecuteResultHandle) -> *mut c_char;
type ExecuteResultRowsChangedFn = unsafe extern "C" fn(*mut VldbSqliteExecuteResultHandle) -> i64;
type ExecuteResultLastInsertRowIdFn =
    unsafe extern "C" fn(*mut VldbSqliteExecuteResultHandle) -> i64;
type ExecuteResultStatementsExecutedFn =
    unsafe extern "C" fn(*mut VldbSqliteExecuteResultHandle) -> i64;
type QueryJsonResultDestroyFn = unsafe extern "C" fn(*mut VldbSqliteQueryJsonResultHandle);
type QueryJsonResultJsonDataFn =
    unsafe extern "C" fn(*mut VldbSqliteQueryJsonResultHandle) -> *mut c_char;
type QueryJsonResultRowCountFn =
    unsafe extern "C" fn(*mut VldbSqliteQueryJsonResultHandle) -> u64;
type QueryStreamDestroyFn = unsafe extern "C" fn(*mut VldbSqliteQueryStreamHandle);
type QueryStreamChunkCountFn = unsafe extern "C" fn(*mut VldbSqliteQueryStreamHandle) -> u64;
type QueryStreamRowCountFn = unsafe extern "C" fn(*mut VldbSqliteQueryStreamHandle) -> u64;
type QueryStreamTotalBytesFn = unsafe extern "C" fn(*mut VldbSqliteQueryStreamHandle) -> u64;
type QueryStreamGetChunkFn = unsafe extern "C" fn(
    *mut VldbSqliteQueryStreamHandle,
    u64,
) -> VldbSqliteByteBuffer;
type BytesFreeFn = unsafe extern "C" fn(VldbSqliteByteBuffer);
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
    library_info_json: LibraryInfoJsonFn,
    database_execute_script: DatabaseExecuteScriptFn,
    database_execute_batch: DatabaseExecuteBatchFn,
    database_query_json: DatabaseQueryJsonFn,
    database_query_stream: DatabaseQueryStreamFn,
    execute_result_destroy: ExecuteResultDestroyFn,
    execute_result_success: ExecuteResultSuccessFn,
    execute_result_message: ExecuteResultMessageFn,
    execute_result_rows_changed: ExecuteResultRowsChangedFn,
    execute_result_last_insert_rowid: ExecuteResultLastInsertRowIdFn,
    execute_result_statements_executed: ExecuteResultStatementsExecutedFn,
    query_json_result_destroy: QueryJsonResultDestroyFn,
    query_json_result_json_data: QueryJsonResultJsonDataFn,
    query_json_result_row_count: QueryJsonResultRowCountFn,
    query_stream_destroy: QueryStreamDestroyFn,
    query_stream_chunk_count: QueryStreamChunkCountFn,
    query_stream_row_count: QueryStreamRowCountFn,
    query_stream_total_bytes: QueryStreamTotalBytesFn,
    query_stream_get_chunk: QueryStreamGetChunkFn,
    bytes_free: BytesFreeFn,
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
            library_info_json: load_symbol!("vldb_sqlite_library_info_json", LibraryInfoJsonFn),
            database_execute_script: load_symbol!(
                "vldb_sqlite_database_execute_script",
                DatabaseExecuteScriptFn
            ),
            database_execute_batch: load_symbol!(
                "vldb_sqlite_database_execute_batch",
                DatabaseExecuteBatchFn
            ),
            database_query_json: load_symbol!(
                "vldb_sqlite_database_query_json",
                DatabaseQueryJsonFn
            ),
            database_query_stream: load_symbol!(
                "vldb_sqlite_database_query_stream",
                DatabaseQueryStreamFn
            ),
            execute_result_destroy: load_symbol!(
                "vldb_sqlite_execute_result_destroy",
                ExecuteResultDestroyFn
            ),
            execute_result_success: load_symbol!(
                "vldb_sqlite_execute_result_success",
                ExecuteResultSuccessFn
            ),
            execute_result_message: load_symbol!(
                "vldb_sqlite_execute_result_message",
                ExecuteResultMessageFn
            ),
            execute_result_rows_changed: load_symbol!(
                "vldb_sqlite_execute_result_rows_changed",
                ExecuteResultRowsChangedFn
            ),
            execute_result_last_insert_rowid: load_symbol!(
                "vldb_sqlite_execute_result_last_insert_rowid",
                ExecuteResultLastInsertRowIdFn
            ),
            execute_result_statements_executed: load_symbol!(
                "vldb_sqlite_execute_result_statements_executed",
                ExecuteResultStatementsExecutedFn
            ),
            query_json_result_destroy: load_symbol!(
                "vldb_sqlite_query_json_result_destroy",
                QueryJsonResultDestroyFn
            ),
            query_json_result_json_data: load_symbol!(
                "vldb_sqlite_query_json_result_json_data",
                QueryJsonResultJsonDataFn
            ),
            query_json_result_row_count: load_symbol!(
                "vldb_sqlite_query_json_result_row_count",
                QueryJsonResultRowCountFn
            ),
            query_stream_destroy: load_symbol!(
                "vldb_sqlite_query_stream_destroy",
                QueryStreamDestroyFn
            ),
            query_stream_chunk_count: load_symbol!(
                "vldb_sqlite_query_stream_chunk_count",
                QueryStreamChunkCountFn
            ),
            query_stream_row_count: load_symbol!(
                "vldb_sqlite_query_stream_row_count",
                QueryStreamRowCountFn
            ),
            query_stream_total_bytes: load_symbol!(
                "vldb_sqlite_query_stream_total_bytes",
                QueryStreamTotalBytesFn
            ),
            query_stream_get_chunk: load_symbol!(
                "vldb_sqlite_query_stream_get_chunk",
                QueryStreamGetChunkFn
            ),
            bytes_free: load_symbol!(
                "vldb_sqlite_bytes_free",
                BytesFreeFn
            ),
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

    /// 中文：调用无参 JSON FFI 接口并解析成 `serde_json::Value`。
    /// English: Invoke a zero-argument JSON FFI entrypoint and parse the response into `serde_json::Value`.
    fn call_json_noarg(&self, function: LibraryInfoJsonFn, operation: &str) -> Result<Value, String> {
        unsafe {
            let response_ptr = function();
            let response_text = self.take_owned_string(response_ptr)?;
            serde_json::from_str(&response_text).map_err(|error| {
                format!(
                    "{} returned invalid JSON: {} / {} 返回了无效 JSON: {}",
                    operation, error, operation, error
                )
            })
        }
    }

    /// 中文：把 QueryStream 返回的字节缓冲区复制成宿主拥有的 `Vec<u8>`，并回收底层分配。
    /// English: Copy a QueryStream byte buffer into a host-owned `Vec<u8>` and free the underlying allocation.
    fn take_chunk_bytes(&self, buffer: VldbSqliteByteBuffer) -> Result<Vec<u8>, String> {
        if buffer.data.is_null() {
            if buffer.len == 0 {
                return Ok(Vec::new());
            }
            return Err(self.take_last_error_message());
        }

        let len = usize::try_from(buffer.len)
            .map_err(|_| "chunk length exceeds usize / chunk 长度超过 usize".to_string())?;
        unsafe {
            let bytes = std::slice::from_raw_parts(buffer.data, len).to_vec();
            (self.bytes_free)(buffer);
            Ok(bytes)
        }
    }
}

/// 中文：单个 skill 的 SQLite 句柄集合，由宿主统一管理生命周期。
/// English: SQLite handle set for a single skill, with lifetime managed centrally by the host.
struct SkillHandleState {
    runtime: *mut VldbSqliteRuntimeHandle,
    database: *mut VldbSqliteDatabaseHandle,
    query_streams: HashMap<u64, *mut VldbSqliteQueryStreamHandle>,
    next_stream_id: u64,
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
        let library_info = self
            .api
            .call_json_noarg(self.api.library_info_json, "library_info_json")
            .unwrap_or_else(|error| {
                json!({
                    "name": "vldb-sqlite",
                    "version": "unknown",
                    "ffi_stage": "unknown",
                    "capabilities": [],
                    "warning": error,
                })
            });
        json!({
            "enabled": true,
            "initialized": true,
            "skill_name": self.skill_name,
            "skill_dir_name": self.skill_dir_name,
            "database_path": self.database_path,
            "integration_mode": "dynamic_library",
            "library_path": self.api.library_path.to_string_lossy().to_string(),
            "library_name": library_info.get("name").cloned().unwrap_or(Value::String("vldb-sqlite".to_string())),
            "library_version": library_info.get("version").cloned().unwrap_or(Value::String("unknown".to_string())),
            "ffi_stage": library_info.get("ffi_stage").cloned().unwrap_or(Value::String("unknown".to_string())),
            "capabilities": library_info.get("capabilities").cloned().unwrap_or_else(|| Value::Array(Vec::new())),
            "log_level": self.config.log_level.as_str(),
            "slow_log_enabled": self.config.slow_log_enabled,
            "slow_log_threshold_ms": self.config.slow_log_threshold_ms,
        })
    }

    /// 中文：返回当前 skill 所绑定 SQLite 的基础信息。
    /// English: Return basic information about the SQLite binding for the current skill.
    pub fn info_json(&self) -> Value {
        let mut status = self.status_json();
        if let Some(status_object) = status.as_object_mut() {
            let library_info = self
                .api
                .call_json_noarg(self.api.library_info_json, "library_info_json")
                .unwrap_or_else(|error| {
                    json!({
                        "name": "vldb-sqlite",
                        "version": "unknown",
                        "ffi_stage": "unknown",
                        "capabilities": [],
                        "warning": error,
                    })
                });
            status_object.insert("library_info".to_string(), library_info);
        }
        status
    }

    /// 中文：通过非 JSON 主接口执行脚本或单条 SQL。
    /// English: Execute a script or single SQL statement through the non-JSON primary interface.
    pub fn execute_script(&self, input: &Value) -> Result<Value, String> {
        let sql = require_string_field(input, "sql")?;
        let params = parse_single_sql_params(input)?;
        let owned_params = build_owned_ffi_values(&params)?;
        self.log_info("execute_script", None);
        let started_at = Instant::now();
        let guard = self.lock_handles()?;
        let sql_cstr = to_cstring(sql, "sql")?;
        unsafe {
            let result_handle = (self.api.database_execute_script)(
                guard.database,
                sql_cstr.as_ptr(),
                if owned_params.values.is_empty() {
                    ptr::null()
                } else {
                    owned_params.as_ptr()
                },
                owned_params.len_u64(),
                ptr::null(),
            );
            if result_handle.is_null() {
                drop(guard);
                let error = self.api.take_last_error_message();
                self.log_warning("execute_script", &error);
                return Err(error);
            }

            let result = json!({
                "success": u8_to_bool((self.api.execute_result_success)(result_handle)),
                "message": self.api.take_optional_string((self.api.execute_result_message)(result_handle)).unwrap_or_default(),
                "rows_changed": (self.api.execute_result_rows_changed)(result_handle),
                "last_insert_rowid": (self.api.execute_result_last_insert_rowid)(result_handle),
                "statements_executed": (self.api.execute_result_statements_executed)(result_handle),
            });
            (self.api.execute_result_destroy)(result_handle);
            drop(guard);
            self.log_if_slow("execute_script", started_at, None);
            Ok(result)
        }
    }

    /// 中文：通过非 JSON 主接口批量执行 SQL。
    /// English: Execute batch SQL through the non-JSON primary interface.
    pub fn execute_batch(&self, input: &Value) -> Result<Value, String> {
        let sql = require_string_field(input, "sql")?;
        let rows = parse_batch_sql_params(input)?;
        let owned_rows = build_owned_ffi_value_matrix(&rows)?;
        self.log_info("execute_batch", None);
        let started_at = Instant::now();
        let guard = self.lock_handles()?;
        let sql_cstr = to_cstring(sql, "sql")?;
        unsafe {
            let result_handle = (self.api.database_execute_batch)(
                guard.database,
                sql_cstr.as_ptr(),
                owned_rows.as_ptr(),
                owned_rows.len_u64(),
            );
            if result_handle.is_null() {
                drop(guard);
                let error = self.api.take_last_error_message();
                self.log_warning("execute_batch", &error);
                return Err(error);
            }

            let result = json!({
                "success": u8_to_bool((self.api.execute_result_success)(result_handle)),
                "message": self.api.take_optional_string((self.api.execute_result_message)(result_handle)).unwrap_or_default(),
                "rows_changed": (self.api.execute_result_rows_changed)(result_handle),
                "last_insert_rowid": (self.api.execute_result_last_insert_rowid)(result_handle),
                "statements_executed": (self.api.execute_result_statements_executed)(result_handle),
            });
            (self.api.execute_result_destroy)(result_handle);
            drop(guard);
            self.log_if_slow("execute_batch", started_at, None);
            Ok(result)
        }
    }

    /// 中文：通过非 JSON 主接口执行 JSON 行集查询。
    /// English: Execute a JSON row-set query through the non-JSON primary interface.
    pub fn query_json(&self, input: &Value) -> Result<Value, String> {
        let sql = require_string_field(input, "sql")?;
        let params = parse_single_sql_params(input)?;
        let owned_params = build_owned_ffi_values(&params)?;
        self.log_info("query_json", None);
        let started_at = Instant::now();
        let guard = self.lock_handles()?;
        let sql_cstr = to_cstring(sql, "sql")?;
        unsafe {
            let result_handle = (self.api.database_query_json)(
                guard.database,
                sql_cstr.as_ptr(),
                if owned_params.values.is_empty() {
                    ptr::null()
                } else {
                    owned_params.as_ptr()
                },
                owned_params.len_u64(),
                ptr::null(),
            );
            if result_handle.is_null() {
                drop(guard);
                let error = self.api.take_last_error_message();
                self.log_warning("query_json", &error);
                return Err(error);
            }

            let row_count = (self.api.query_json_result_row_count)(result_handle);
            let json_data = self
                .api
                .take_owned_string((self.api.query_json_result_json_data)(result_handle))?;
            let rows = serde_json::from_str::<Value>(&json_data).map_err(|error| {
                format!(
                    "query_json returned invalid json_data: {} / query_json 返回的 json_data 非法: {}",
                    error, error
                )
            })?;
            (self.api.query_json_result_destroy)(result_handle);
            drop(guard);
            self.log_if_slow("query_json", started_at, Some(format!("rows={}", row_count)));
            Ok(json!({
                "success": true,
                "row_count": row_count,
                "json_data": json_data,
                "rows": rows,
            }))
        }
    }

    /// 中文：通过非 JSON 主接口创建 QueryStream 句柄。
    /// English: Create a QueryStream handle through the non-JSON primary interface.
    pub fn query_stream(&self, input: &Value) -> Result<Value, String> {
        let sql = require_string_field(input, "sql")?;
        let params = parse_single_sql_params(input)?;
        let owned_params = build_owned_ffi_values(&params)?;
        let chunk_bytes = input
            .get("chunk_bytes")
            .or_else(|| input.get("chunk_size"))
            .and_then(Value::as_u64)
            .unwrap_or(0);
        self.log_info("query_stream", None);
        let started_at = Instant::now();
        let mut guard = self.lock_handles()?;
        let sql_cstr = to_cstring(sql, "sql")?;
        unsafe {
            let result_handle = (self.api.database_query_stream)(
                guard.database,
                sql_cstr.as_ptr(),
                if owned_params.values.is_empty() {
                    ptr::null()
                } else {
                    owned_params.as_ptr()
                },
                owned_params.len_u64(),
                ptr::null(),
                chunk_bytes,
            );
            if result_handle.is_null() {
                drop(guard);
                let error = self.api.take_last_error_message();
                self.log_warning("query_stream", &error);
                return Err(error);
            }

            let stream_id = guard.next_stream_id;
            guard.next_stream_id = guard.next_stream_id.saturating_add(1).max(1);
            guard.query_streams.insert(stream_id, result_handle);
            drop(guard);
            self.log_if_slow(
                "query_stream",
                started_at,
                Some(format!("stream_id={} metrics_ready=false", stream_id)),
            );
            Ok(json!({
                "success": true,
                "stream_id": stream_id,
                "metrics_ready": false,
            }))
        }
    }

    /// 中文：等待 QueryStream 最终统计信息就绪，并返回终态指标。
    /// English: Wait for final QueryStream metrics and return terminal statistics.
    pub fn query_stream_wait_metrics(&self, input: &Value) -> Result<Value, String> {
        let stream_id = input
            .get("stream_id")
            .and_then(Value::as_u64)
            .ok_or_else(|| "stream_id is required / 必须提供 stream_id".to_string())?;
        self.log_info("query_stream_wait_metrics", None);
        let started_at = Instant::now();
        let guard = self.lock_handles()?;
        let stream_handle = *guard.query_streams.get(&stream_id).ok_or_else(|| {
            format!(
                "query stream handle not found: {} / QueryStream 句柄不存在: {}",
                stream_id, stream_id
            )
        })?;
        unsafe {
            let row_count = (self.api.query_stream_row_count)(stream_handle);
            let chunk_count = (self.api.query_stream_chunk_count)(stream_handle);
            let total_bytes = (self.api.query_stream_total_bytes)(stream_handle);
            drop(guard);
            self.log_if_slow(
                "query_stream_wait_metrics",
                started_at,
                Some(format!(
                    "stream_id={} chunks={} rows={} bytes={}",
                    stream_id, chunk_count, row_count, total_bytes
                )),
            );
            Ok(json!({
                "success": true,
                "stream_id": stream_id,
                "metrics_ready": true,
                "row_count": row_count,
                "chunk_count": chunk_count,
                "total_bytes": total_bytes,
            }))
        }
    }

    /// 中文：读取单个 QueryStream chunk，并以 base64 形式返回。
    /// English: Read a single QueryStream chunk and return it as base64 text.
    pub fn query_stream_chunk(&self, input: &Value) -> Result<Value, String> {
        let stream_id = input
            .get("stream_id")
            .and_then(Value::as_u64)
            .ok_or_else(|| "stream_id is required / 必须提供 stream_id".to_string())?;
        let index = input
            .get("index")
            .and_then(Value::as_u64)
            .ok_or_else(|| "index is required / 必须提供 index".to_string())?;
        self.log_info("query_stream_chunk", None);
        let started_at = Instant::now();
        let guard = self.lock_handles()?;
        let stream_handle = *guard.query_streams.get(&stream_id).ok_or_else(|| {
            format!(
                "query stream handle not found: {} / QueryStream 句柄不存在: {}",
                stream_id, stream_id
            )
        })?;
        unsafe {
            let buffer = (self.api.query_stream_get_chunk)(stream_handle, index);
            let chunk = self.api.take_chunk_bytes(buffer)?;
            drop(guard);
            self.log_if_slow(
                "query_stream_chunk",
                started_at,
                Some(format!("stream_id={} index={} bytes={}", stream_id, index, chunk.len())),
            );
            Ok(json!({
                "success": true,
                "stream_id": stream_id,
                "index": index,
                "byte_count": u64::try_from(chunk.len()).unwrap_or(u64::MAX),
                "chunk_base64": BASE64_STANDARD.encode(chunk),
            }))
        }
    }

    /// 中文：关闭 QueryStream 句柄并释放宿主缓存的流结果。
    /// English: Close a QueryStream handle and release the host-cached stream result.
    pub fn query_stream_close(&self, input: &Value) -> Result<Value, String> {
        let stream_id = input
            .get("stream_id")
            .and_then(Value::as_u64)
            .ok_or_else(|| "stream_id is required / 必须提供 stream_id".to_string())?;
        self.log_info("query_stream_close", None);
        let started_at = Instant::now();
        let mut guard = self.lock_handles()?;
        let stream_handle = guard.query_streams.remove(&stream_id).ok_or_else(|| {
            format!(
                "query stream handle not found: {} / QueryStream 句柄不存在: {}",
                stream_id, stream_id
            )
        })?;
        unsafe {
            (self.api.query_stream_destroy)(stream_handle);
            drop(guard);
            self.log_if_slow("query_stream_close", started_at, Some(format!("stream_id={}", stream_id)));
            Ok(json!({
                "success": true,
                "stream_id": stream_id,
                "message": format!("query_stream handle {} closed successfully", stream_id),
            }))
        }
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
                for (_, stream_handle) in guard.query_streams.drain() {
                    if !stream_handle.is_null() {
                        (self.api.query_stream_destroy)(stream_handle);
                    }
                }
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
            handles: Mutex::new(SkillHandleState {
                runtime,
                database,
                query_streams: HashMap::new(),
                next_stream_id: 1,
            }),
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

/// 中文：宿主内部使用的 SQLite 参数值表示，负责在 Lua/JSON 与 FFI ABI 之间做稳定过渡。
/// English: Host-side SQLite parameter representation used as a stable bridge between Lua/JSON and the FFI ABI.
enum HostSqliteParamValue {
    Null,
    Int64(i64),
    Float64(f64),
    String(String),
    Bytes(Vec<u8>),
    Bool(bool),
}

/// 中文：一组已拥有生命周期的 FFI 参数数组，确保字符串和字节缓冲在调用期间保持有效。
/// English: One owned FFI parameter array that keeps strings and byte buffers alive for the entire call.
struct OwnedSqliteFfiValues {
    values: Vec<VldbSqliteFfiValue>,
    _strings: Vec<CString>,
    _bytes: Vec<Vec<u8>>,
}

impl OwnedSqliteFfiValues {
    /// 中文：返回 FFI 参数数组首指针。
    /// English: Return the pointer to the first FFI parameter value.
    fn as_ptr(&self) -> *const VldbSqliteFfiValue {
        self.values.as_ptr()
    }

    /// 中文：返回 FFI 参数数组长度。
    /// English: Return the length of the FFI parameter array.
    fn len_u64(&self) -> u64 {
        u64::try_from(self.values.len()).unwrap_or(u64::MAX)
    }
}

/// 中文：批量 SQL 所使用的已拥有生命周期的二维参数矩阵。
/// English: Owned two-dimensional parameter matrix used by batch SQL execution.
struct OwnedSqliteFfiValueMatrix {
    _rows: Vec<OwnedSqliteFfiValues>,
    slices: Vec<VldbSqliteFfiValueSlice>,
}

impl OwnedSqliteFfiValueMatrix {
    /// 中文：返回批量参数切片首指针。
    /// English: Return the pointer to the first batch-parameter slice.
    fn as_ptr(&self) -> *const VldbSqliteFfiValueSlice {
        self.slices.as_ptr()
    }

    /// 中文：返回批量参数切片数量。
    /// English: Return the number of batch-parameter slices.
    fn len_u64(&self) -> u64 {
        u64::try_from(self.slices.len()).unwrap_or(u64::MAX)
    }
}

/// 中文：把宿主参数值数组转换成拥有生命周期的 FFI 参数数组。
/// English: Convert host parameter values into an owned FFI parameter array.
fn build_owned_ffi_values(values: &[HostSqliteParamValue]) -> Result<OwnedSqliteFfiValues, String> {
    let mut ffi_values = Vec::with_capacity(values.len());
    let mut strings = Vec::new();
    let mut bytes = Vec::new();

    for value in values {
        match value {
            HostSqliteParamValue::Null => ffi_values.push(VldbSqliteFfiValue {
                kind: VldbSqliteFfiValueKind::Null,
                int64_value: 0,
                float64_value: 0.0,
                string_value: ptr::null(),
                bytes_value: VldbSqliteByteView::default(),
                bool_value: 0,
            }),
            HostSqliteParamValue::Int64(number) => ffi_values.push(VldbSqliteFfiValue {
                kind: VldbSqliteFfiValueKind::Int64,
                int64_value: *number,
                float64_value: 0.0,
                string_value: ptr::null(),
                bytes_value: VldbSqliteByteView::default(),
                bool_value: 0,
            }),
            HostSqliteParamValue::Float64(number) => ffi_values.push(VldbSqliteFfiValue {
                kind: VldbSqliteFfiValueKind::Float64,
                int64_value: 0,
                float64_value: *number,
                string_value: ptr::null(),
                bytes_value: VldbSqliteByteView::default(),
                bool_value: 0,
            }),
            HostSqliteParamValue::String(text) => {
                let c_text = to_cstring(text, "params[*].string")?;
                let ptr = c_text.as_ptr();
                strings.push(c_text);
                ffi_values.push(VldbSqliteFfiValue {
                    kind: VldbSqliteFfiValueKind::String,
                    int64_value: 0,
                    float64_value: 0.0,
                    string_value: ptr,
                    bytes_value: VldbSqliteByteView::default(),
                    bool_value: 0,
                });
            }
            HostSqliteParamValue::Bytes(blob) => {
                let owned = blob.clone();
                let view = if owned.is_empty() {
                    VldbSqliteByteView::default()
                } else {
                    VldbSqliteByteView {
                        data: owned.as_ptr(),
                        len: u64::try_from(owned.len()).unwrap_or(u64::MAX),
                    }
                };
                bytes.push(owned);
                ffi_values.push(VldbSqliteFfiValue {
                    kind: VldbSqliteFfiValueKind::Bytes,
                    int64_value: 0,
                    float64_value: 0.0,
                    string_value: ptr::null(),
                    bytes_value: view,
                    bool_value: 0,
                });
            }
            HostSqliteParamValue::Bool(flag) => ffi_values.push(VldbSqliteFfiValue {
                kind: VldbSqliteFfiValueKind::Bool,
                int64_value: 0,
                float64_value: 0.0,
                string_value: ptr::null(),
                bytes_value: VldbSqliteByteView::default(),
                bool_value: bool_to_u8(*flag),
            }),
        }
    }

    Ok(OwnedSqliteFfiValues {
        values: ffi_values,
        _strings: strings,
        _bytes: bytes,
    })
}

/// 中文：把批量参数矩阵转换成拥有生命周期的 FFI 批量参数切片。
/// English: Convert a batch parameter matrix into owned FFI batch-parameter slices.
fn build_owned_ffi_value_matrix(
    rows: &[Vec<HostSqliteParamValue>],
) -> Result<OwnedSqliteFfiValueMatrix, String> {
    let owned_rows = rows
        .iter()
        .map(|row| build_owned_ffi_values(row))
        .collect::<Result<Vec<_>, _>>()?;
    let slices = owned_rows
        .iter()
        .map(|row| VldbSqliteFfiValueSlice {
            values: row.as_ptr(),
            len: row.len_u64(),
        })
        .collect::<Vec<_>>();
    Ok(OwnedSqliteFfiValueMatrix {
        _rows: owned_rows,
        slices,
    })
}

/// 中文：把 JSON/ Lua 标量参数转换为宿主内部 SQLite 参数值。
/// English: Convert a JSON/Lua scalar parameter into the host-side SQLite parameter representation.
fn parse_scalar_sqlite_param(value: &Value, field_name: &str) -> Result<HostSqliteParamValue, String> {
    match value {
        Value::Null => Ok(HostSqliteParamValue::Null),
        Value::Bool(flag) => Ok(HostSqliteParamValue::Bool(*flag)),
        Value::Number(number) => {
            if let Some(int_value) = number.as_i64() {
                Ok(HostSqliteParamValue::Int64(int_value))
            } else if let Some(unsigned) = number.as_u64() {
                let converted = i64::try_from(unsigned).map_err(|_| {
                    format!(
                        "{} contains an unsigned integer larger than i64 / {} 包含超过 i64 范围的无符号整数",
                        field_name, field_name
                    )
                })?;
                Ok(HostSqliteParamValue::Int64(converted))
            } else if let Some(float_value) = number.as_f64() {
                Ok(HostSqliteParamValue::Float64(float_value))
            } else {
                Err(format!(
                    "{} contains an unsupported numeric value / {} 包含不支持的数值",
                    field_name, field_name
                ))
            }
        }
        Value::String(text) => Ok(HostSqliteParamValue::String(text.clone())),
        _ => Err(format!(
            "{} must contain only scalar values / {} 只能包含标量值",
            field_name, field_name
        )),
    }
}

/// 中文：把 typed 参数对象转换为宿主内部 SQLite 参数值。
/// English: Convert a typed parameter object into the host-side SQLite parameter representation.
fn parse_typed_sqlite_param(
    object: &serde_json::Map<String, Value>,
    field_name: &str,
) -> Result<HostSqliteParamValue, String> {
    let kind = object
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            format!(
                "{}.kind is required for typed parameters / typed 参数必须提供 {}.kind",
                field_name, field_name
            )
        })?;
    match kind.trim().to_ascii_lowercase().as_str() {
        "null" => Ok(HostSqliteParamValue::Null),
        "bool" => object
            .get("value")
            .and_then(Value::as_bool)
            .map(HostSqliteParamValue::Bool)
            .ok_or_else(|| format!("{}.value must be a bool / {}.value 必须是布尔值", field_name, field_name)),
        "int64" => object
            .get("value")
            .and_then(Value::as_i64)
            .map(HostSqliteParamValue::Int64)
            .ok_or_else(|| format!("{}.value must be an int64 / {}.value 必须是 int64", field_name, field_name)),
        "float64" => object
            .get("value")
            .and_then(Value::as_f64)
            .map(HostSqliteParamValue::Float64)
            .ok_or_else(|| format!("{}.value must be a float64 / {}.value 必须是 float64", field_name, field_name)),
        "string" => object
            .get("value")
            .and_then(Value::as_str)
            .map(|value| HostSqliteParamValue::String(value.to_string()))
            .ok_or_else(|| format!("{}.value must be a string / {}.value 必须是字符串", field_name, field_name)),
        "bytes" => {
            if let Some(base64_value) = object.get("base64").and_then(Value::as_str) {
                let decoded = BASE64_STANDARD.decode(base64_value).map_err(|error| {
                    format!(
                        "{}.base64 is invalid: {} / {}.base64 非法: {}",
                        field_name, error, field_name, error
                    )
                })?;
                return Ok(HostSqliteParamValue::Bytes(decoded));
            }
            let array = object
                .get("value")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    format!(
                        "{}.value must be a byte array or provide base64 / {}.value 必须是字节数组或提供 base64",
                        field_name, field_name
                    )
                })?;
            let mut bytes = Vec::with_capacity(array.len());
            for (index, item) in array.iter().enumerate() {
                let byte = item.as_u64().ok_or_else(|| {
                    format!(
                        "{}.value[{}] must be an unsigned integer / {}.value[{}] 必须是无符号整数",
                        field_name, index, field_name, index
                    )
                })?;
                let converted = u8::try_from(byte).map_err(|_| {
                    format!(
                        "{}.value[{}] exceeds u8 / {}.value[{}] 超出 u8 范围",
                        field_name, index, field_name, index
                    )
                })?;
                bytes.push(converted);
            }
            Ok(HostSqliteParamValue::Bytes(bytes))
        }
        other => Err(format!(
            "{}.kind={} is unsupported / {}.kind={} 不受支持",
            field_name, other, field_name, other
        )),
    }
}

/// 中文：把 JSON 参数值统一转换为宿主内部 SQLite 参数值。
/// English: Normalize a JSON parameter value into the host-side SQLite parameter representation.
fn parse_sqlite_param(value: &Value, field_name: &str) -> Result<HostSqliteParamValue, String> {
    match value {
        Value::Object(object) if object.contains_key("kind") => {
            parse_typed_sqlite_param(object, field_name)
        }
        other => parse_scalar_sqlite_param(other, field_name),
    }
}

/// 中文：解析 legacy `params_json` 字符串，只允许标量数组。
/// English: Parse legacy `params_json` text, allowing scalar arrays only.
fn parse_legacy_params_json_text(params_json: &str) -> Result<Vec<HostSqliteParamValue>, String> {
    if params_json.trim().is_empty() {
        return Ok(Vec::new());
    }
    let parsed: Value = serde_json::from_str(params_json).map_err(|error| {
        format!(
            "params_json must be a JSON array of scalar values: {} / params_json 必须是标量数组: {}",
            error, error
        )
    })?;
    let items = parsed.as_array().ok_or_else(|| {
        "params_json must be a JSON array of scalar values / params_json 必须是标量数组".to_string()
    })?;
    items
        .iter()
        .enumerate()
        .map(|(index, item)| parse_scalar_sqlite_param(item, &format!("params_json[{}]", index)))
        .collect()
}

/// 中文：从统一输入对象中解析单条 SQL 的参数列表。
/// English: Parse the parameter list for a single SQL request from the unified input object.
fn parse_single_sql_params(input: &Value) -> Result<Vec<HostSqliteParamValue>, String> {
    let params_json = input.get("params_json").and_then(Value::as_str).unwrap_or("");
    if let Some(params_value) = input.get("params") {
        if !params_json.trim().is_empty() {
            return Err(
                "provide either params or params_json, but not both / 不能同时提供 params 与 params_json"
                    .to_string(),
            );
        }
        let params_array = params_value.as_array().ok_or_else(|| {
            "params must be an array / params 必须是数组".to_string()
        })?;
        return params_array
            .iter()
            .enumerate()
            .map(|(index, item)| parse_sqlite_param(item, &format!("params[{}]", index)))
            .collect();
    }
    parse_legacy_params_json_text(params_json)
}

/// 中文：从统一输入对象中解析批量 SQL 的参数矩阵。
/// English: Parse the parameter matrix for batch SQL from the unified input object.
fn parse_batch_sql_params(input: &Value) -> Result<Vec<Vec<HostSqliteParamValue>>, String> {
    let items = input
        .get("items")
        .and_then(Value::as_array)
        .ok_or_else(|| "items must be an array of arrays / items 必须是二维数组".to_string())?;
    if items.is_empty() {
        return Err("items must not be empty / items 不能为空".to_string());
    }
    items
        .iter()
        .enumerate()
        .map(|(row_index, row)| {
            let row_items = row.as_array().ok_or_else(|| {
                format!(
                    "items[{}] must be an array / items[{}] 必须是数组",
                    row_index, row_index
                )
            })?;
            row_items
                .iter()
                .enumerate()
                .map(|(col_index, item)| {
                    parse_sqlite_param(item, &format!("items[{}][{}]", row_index, col_index))
                })
                .collect()
        })
        .collect()
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
