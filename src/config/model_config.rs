use super::runtime_root::{clone_runtime_root_override, find_optional_runtime_config_file};
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[cfg(test)]
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};

/// Runtime model-config cache that stores the explicitly committed state or a not-preloaded error.
/// 运行时模型配置缓存，保存显式提交的状态或未预载错误。
static MODEL_CONFIG_RUNTIME: OnceLock<RwLock<ModelConfigRuntimeState>> = OnceLock::new();
/// Optional explicit runtime-root override used to keep model-config discovery aligned with one selected runtime.
/// 可选的显式 runtime_root 覆盖，用于让模型配置发现链与当前选中的运行根保持一致。
static MODEL_CONFIG_RUNTIME_ROOT: OnceLock<RwLock<Option<PathBuf>>> = OnceLock::new();

/// Root model configuration owned by the host and never exposed to Lua for mutation.
/// 宿主管理的模型配置根对象，不允许 Lua 侧直接修改。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelConfig {
    /// Strict schema version for this host model configuration.
    /// 这份宿主模型配置使用的严格结构版本。
    pub format_version: u32,
    /// OpenAI-compatible provider settings used by the current host.
    /// 当前宿主使用的 OpenAI-compatible 供应商设置。
    #[serde(default)]
    pub openai_compatible: OpenAiCompatibleModelConfig,
}

/// OpenAI-compatible provider configuration for embeddings and one-shot LLM calls.
/// OpenAI-compatible 供应商配置，用于向量与单轮 LLM 调用。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenAiCompatibleModelConfig {
    /// Whether the provider is globally enabled before individual capabilities are considered.
    /// 是否全局启用该供应商，单项能力会在此基础上再判断。
    #[serde(default)]
    pub enabled: bool,
    /// Embedding capability configuration.
    /// 向量能力配置。
    #[serde(default)]
    pub embedding: EmbeddingModelConfig,
    /// LLM capability configuration.
    /// LLM 能力配置。
    #[serde(default)]
    pub llm: LlmModelConfig,
}

/// Embedding capability configuration for a single text input.
/// 单文本向量能力配置。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmbeddingModelConfig {
    /// Whether the embedding capability is enabled under the selected provider.
    /// 是否在当前供应商下启用向量能力。
    #[serde(default)]
    pub enabled: bool,
    /// API base URL, usually ending with `/v1`, used before `/embeddings`.
    /// API 基础地址，通常以 `/v1` 结尾，调用时会继续拼接 `/embeddings`。
    pub base_url: Option<String>,
    /// API key literal or an `${env:NAME}` reference resolved by the host at preload/reload time.
    /// API key 字面值或 `${env:NAME}` 引用，由宿主在预载/重载阶段解析。
    pub api_key: Option<String>,
    /// Provider model name used for embedding requests.
    /// 向量请求使用的供应商模型名称。
    pub model: Option<String>,
    /// Request timeout in milliseconds for embedding calls.
    /// 向量请求的超时时间，单位为毫秒。
    pub timeout_ms: Option<u64>,
    /// Host-owned provider-specific request fields merged into the request body except reserved keys.
    /// 宿主管理的供应商专用请求字段，会在排除保留键后合并到请求体。
    #[serde(default)]
    pub request_overrides: Value,
}

/// LLM capability configuration for one non-streaming chat-completion request.
/// 单轮非流式 chat-completion 请求的 LLM 能力配置。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LlmModelConfig {
    /// Whether the LLM capability is enabled under the selected provider.
    /// 是否在当前供应商下启用 LLM 能力。
    #[serde(default)]
    pub enabled: bool,
    /// API base URL, usually ending with `/v1`, used before `/chat/completions`.
    /// API 基础地址，通常以 `/v1` 结尾，调用时会继续拼接 `/chat/completions`。
    pub base_url: Option<String>,
    /// API key literal or an `${env:NAME}` reference resolved by the host at preload/reload time.
    /// API key 字面值或 `${env:NAME}` 引用，由宿主在预载/重载阶段解析。
    pub api_key: Option<String>,
    /// Provider model name used for LLM requests.
    /// LLM 请求使用的供应商模型名称。
    pub model: Option<String>,
    /// Optional sampling temperature supplied by the host configuration.
    /// 由宿主配置提供的可选采样温度。
    pub temperature: Option<f64>,
    /// Optional maximum generated token count supplied by the host configuration.
    /// 由宿主配置提供的可选最大生成 token 数。
    pub max_tokens: Option<u64>,
    /// Request timeout in milliseconds for LLM calls.
    /// LLM 请求的超时时间，单位为毫秒。
    pub timeout_ms: Option<u64>,
    /// Host-owned provider-specific request fields merged into the request body except reserved keys.
    /// 宿主管理的供应商专用请求字段，会在排除保留键后合并到请求体。
    #[serde(default)]
    pub request_overrides: Value,
}

/// Effective model configuration snapshot with secrets resolved for provider calls.
/// 已解析密钥的模型配置快照，供供应商调用层使用。
#[derive(Debug, Clone, Default)]
pub(crate) struct EffectiveModelConfig {
    /// Parsed non-secret model configuration.
    /// 已解析的非密模型配置。
    pub config: ModelConfig,
    /// Resolved embedding API key from the embedding capability configuration.
    /// 从向量能力配置解析得到的向量 API key。
    pub embedding_api_key: Option<String>,
    /// Resolved LLM API key from the LLM capability configuration.
    /// 从 LLM 能力配置解析得到的 LLM API key。
    pub llm_api_key: Option<String>,
    /// Source path that contributed the effective model configuration.
    /// 提供当前生效模型配置的来源路径。
    pub source_path: Option<PathBuf>,
}

/// Internal runtime state for the cached model configuration store.
/// 模型配置缓存的内部运行时状态。
#[derive(Debug, Clone, Default)]
pub(super) struct ModelConfigRuntime {
    /// Effective model configuration snapshot.
    /// 当前生效的模型配置快照。
    effective: EffectiveModelConfig,
}

/// Cached model-config runtime state with explicit load-error preservation.
/// 带显式加载错误保留能力的模型配置运行时缓存状态。
pub(super) type ModelConfigRuntimeState = Result<ModelConfigRuntime, String>;

/// Model-config load report used by startup logging and hot-reload results.
/// 模型配置加载结果，便于启动日志与热重载结果输出。
#[derive(Debug, Clone, Serialize)]
pub struct ModelConfigLoadReport {
    /// Source path used for the loaded model config.
    /// 本次加载使用的模型配置来源路径。
    pub source_path: Option<String>,
    /// Stable provider label currently supported by the host.
    /// 当前宿主支持的稳定供应商标签。
    pub provider: String,
    /// Whether the provider is globally enabled.
    /// 供应商是否全局启用。
    pub provider_enabled: bool,
    /// Whether a usable API key was resolved.
    /// 是否为任一模型能力解析到了可用 API key。
    pub api_key_configured: bool,
    /// Whether a usable API base URL was configured.
    /// 是否为任一模型能力配置了可用 API 基础地址。
    pub base_url_configured: bool,
    /// Whether embedding has a usable API key in its own capability configuration.
    /// 向量能力自身配置中是否具备可用 API key。
    pub embedding_api_key_configured: bool,
    /// Whether embedding has a usable API base URL in its own capability configuration.
    /// 向量能力自身配置中是否具备可用 API 基础地址。
    pub embedding_base_url_configured: bool,
    /// Whether embedding calls are currently enabled.
    /// 当前是否启用向量调用。
    pub embedding_enabled: bool,
    /// Configured embedding model name when present.
    /// 已配置的向量模型名称。
    pub embedding_model: Option<String>,
    /// Whether LLM calls are currently enabled.
    /// 当前是否启用 LLM 调用。
    pub llm_enabled: bool,
    /// Whether LLM has a usable API key in its own capability configuration.
    /// LLM 能力自身配置中是否具备可用 API key。
    pub llm_api_key_configured: bool,
    /// Whether LLM has a usable API base URL in its own capability configuration.
    /// LLM 能力自身配置中是否具备可用 API 基础地址。
    pub llm_base_url_configured: bool,
    /// Configured LLM model name when present.
    /// 已配置的 LLM 模型名称。
    pub llm_model: Option<String>,
}

/// Return the explicitly preloaded model-config cache without request-time disk or environment I/O.
/// 返回显式预载的模型配置缓存，且不在请求期执行磁盘或环境变量 I/O。
/// Returns the shared cache lock, initialized to an explicit not-preloaded error when necessary.
/// 返回共享缓存锁；必要时以显式的未预载错误初始化。
pub(super) fn model_config_runtime() -> &'static RwLock<ModelConfigRuntimeState> {
    MODEL_CONFIG_RUNTIME.get_or_init(|| {
        RwLock::new(Err(
            "model config runtime has not been preloaded".to_string()
        ))
    })
}

/// Return the shared runtime-root override store used by model-config discovery.
/// 返回模型配置发现链使用的共享运行根覆盖存储。
fn model_config_runtime_root() -> &'static RwLock<Option<PathBuf>> {
    MODEL_CONFIG_RUNTIME_ROOT.get_or_init(|| RwLock::new(None))
}

/// Initialize the runtime-root override used by model-config preload and reload.
/// 初始化供模型配置预载与热重载使用的运行根覆盖值。
pub fn initialize_model_config_runtime_root(runtime_root: Option<&Path>) -> Result<(), String> {
    let mut guard = model_config_runtime_root()
        .write()
        .map_err(|_| "model config runtime-root lock poisoned".to_string())?;
    *guard = runtime_root.map(std::path::Path::to_path_buf);
    Ok(())
}

/// Read the current runtime-root override used by model-config discovery.
/// 读取当前模型配置发现链使用的运行根覆盖值。
fn current_model_config_runtime_root() -> Result<Option<PathBuf>, String> {
    clone_runtime_root_override(
        model_config_runtime_root().read(),
        "model config runtime-root lock poisoned",
    )
}

/// Preload model config during startup so format and required-field issues are discovered early.
/// 启动时预载模型配置，便于尽早发现配置格式与必填字段问题。
#[cfg(test)]
pub(crate) fn preload_model_config() -> Result<ModelConfigLoadReport, String> {
    let (runtime, report) = stage_model_config_runtime()?;
    // Serialize standalone preload commits with aggregate reloads and request-time readers.
    // 将独立预载提交与聚合重载及请求期读取串行化。
    let _transaction_guard = super::runtime_config_write_guard()?;
    let mut guard = model_config_runtime()
        .write()
        .map_err(|_| "model config runtime lock poisoned".to_string())?;
    *guard = Ok(runtime);
    Ok(report)
}

/// Return a cloned effective model configuration for the provider layer.
/// 返回一份供供应商调用层使用的生效模型配置快照。
pub(crate) fn current_effective_model_config() -> Result<EffectiveModelConfig, String> {
    // Hold the shared generation while cloning the committed model configuration.
    // 克隆已提交模型配置期间持有共享版本。
    let _transaction_guard = super::runtime_config_read_guard()?;
    // Clone the cached state so provider calls never hold the model-config lock.
    // 克隆缓存状态，避免供应商调用期间持有模型配置锁。
    let runtime_state = model_config_runtime()
        .read()
        .map_err(|_| "model config runtime lock poisoned".to_string())?;
    effective_model_config_from_runtime_state(&runtime_state)
}

/// Clone the effective config from one cached runtime state without hiding load errors.
/// 从单个缓存运行时状态克隆生效配置，且不隐藏加载错误。
/// Parameters: `runtime_state` is the cached model-config state to inspect.
/// 参数：`runtime_state` 是待检查的模型配置缓存状态。
/// Returns the cloned effective config or the cached load error.
/// 返回克隆后的生效配置或缓存的加载错误。
fn effective_model_config_from_runtime_state(
    runtime_state: &ModelConfigRuntimeState,
) -> Result<EffectiveModelConfig, String> {
    runtime_state
        .as_ref()
        .map(|runtime| runtime.effective.clone())
        .map_err(|error| error.clone())
}

/// Load the model-config runtime state from disk.
/// 从磁盘加载模型配置运行时状态。
fn load_model_config_runtime() -> Result<ModelConfigRuntime, String> {
    let source_path = find_model_config_path()?;
    let Some(path) = source_path else {
        return Ok(ModelConfigRuntime::default());
    };

    let content = fs::read_to_string(&path).map_err(|error| {
        format!(
            "Failed to read model config file {}: {}",
            path.display(),
            error
        )
    })?;
    let config: ModelConfig = serde_yaml::from_str(&content).map_err(|error| {
        format!(
            "Failed to parse model config YAML {}: {}",
            path.display(),
            error
        )
    })?;
    if config.format_version != super::HOST_CONFIG_FORMAT_VERSION {
        return Err(format!(
            "Invalid model config file {}: unsupported format_version {}; expected {}",
            path.display(),
            config.format_version,
            super::HOST_CONFIG_FORMAT_VERSION
        ));
    }
    let embedding_api_key =
        resolve_optional_secret(config.openai_compatible.embedding.api_key.as_deref());
    let llm_api_key = resolve_optional_secret(config.openai_compatible.llm.api_key.as_deref());
    let effective = EffectiveModelConfig {
        config,
        embedding_api_key,
        llm_api_key,
        source_path: Some(path.clone()),
    };
    validate_effective_model_config(&effective)
        .map_err(|error| format!("Invalid model config file {}: {}", path.display(), error))?;

    Ok(ModelConfigRuntime { effective })
}

/// Stage one validated model-config runtime and report without mutating the shared cache.
/// 分阶段加载一份已校验模型配置运行时及报告，不修改共享缓存。
/// Returns the staged runtime/report pair or the first discovery, parse, secret, or validation error.
/// 返回分阶段运行时与报告，或首个发现、解析、密钥或校验错误。
pub(super) fn stage_model_config_runtime()
-> Result<(ModelConfigRuntime, ModelConfigLoadReport), String> {
    let runtime = load_model_config_runtime()?;
    let report = build_model_config_load_report(&runtime.effective);
    Ok((runtime, report))
}

/// Validate provider-level required fields only for enabled capabilities.
/// 仅对已启用的能力校验供应商级必填字段。
fn validate_effective_model_config(effective: &EffectiveModelConfig) -> Result<(), String> {
    let provider = &effective.config.openai_compatible;
    if !provider.enabled {
        return Ok(());
    }
    if !provider.embedding.enabled && !provider.llm.enabled {
        return Ok(());
    }
    if provider.embedding.enabled {
        if effective_embedding_base_url(effective).is_none() {
            return Err(
                "openai_compatible.embedding.base_url is required when embedding is enabled"
                    .to_string(),
            );
        }
        if normalized_optional_text(effective.embedding_api_key.as_deref()).is_none() {
            return Err(build_missing_api_key_error(
                "openai_compatible.embedding.api_key",
                "embedding",
                provider.embedding.api_key.as_deref(),
            ));
        }
        if normalized_optional_text(provider.embedding.model.as_deref()).is_none() {
            return Err(
                "openai_compatible.embedding.model is required when embedding is enabled"
                    .to_string(),
            );
        }
    }
    if provider.llm.enabled {
        if effective_llm_base_url(effective).is_none() {
            return Err(
                "openai_compatible.llm.base_url is required when llm is enabled".to_string(),
            );
        }
        if normalized_optional_text(effective.llm_api_key.as_deref()).is_none() {
            return Err(build_missing_api_key_error(
                "openai_compatible.llm.api_key",
                "llm",
                provider.llm.api_key.as_deref(),
            ));
        }
        if normalized_optional_text(provider.llm.model.as_deref()).is_none() {
            return Err("openai_compatible.llm.model is required when llm is enabled".to_string());
        }
    }
    Ok(())
}

/// Build one actionable API-key validation error that explains the missing-secret source.
/// 构建一条可执行的 API key 校验错误，并说明缺失密钥的来源。
fn build_missing_api_key_error(
    field_path: &str,
    capability_name: &str,
    configured_value: Option<&str>,
) -> String {
    let configured_value = normalized_optional_text(configured_value);
    if let Some(env_name) = configured_value
        .as_deref()
        .and_then(parse_exact_env_reference)
    {
        return format!(
            "{field_path} is required when {capability_name} is enabled; configured value references environment variable {env_name}, but that variable is missing or blank in the current process environment{}",
            windows_service_environment_hint()
        );
    }
    format!(
        "{field_path} is required when {capability_name} is enabled; set a literal API key or use an exact ${{env:NAME}} reference"
    )
}

/// Return one Windows-service-specific hint for env-backed secrets without affecting other platforms.
/// 返回一条仅针对 Windows 服务的环境变量提示，同时不影响其他平台。
fn windows_service_environment_hint() -> &'static str {
    if cfg!(windows) {
        "; on Windows services, LocalSystem cannot read user-level environment variables, so use a machine-level variable or run the service under an account that owns the variable"
    } else {
        ""
    }
}

/// Resolve the effective embedding API base URL from the embedding capability configuration only.
/// 仅从向量能力配置解析实际向量 API 基础地址。
pub(crate) fn effective_embedding_base_url(effective: &EffectiveModelConfig) -> Option<String> {
    normalized_optional_text(
        effective
            .config
            .openai_compatible
            .embedding
            .base_url
            .as_deref(),
    )
}

/// Resolve the effective LLM API base URL from the LLM capability configuration only.
/// 仅从 LLM 能力配置解析实际 LLM API 基础地址。
pub(crate) fn effective_llm_base_url(effective: &EffectiveModelConfig) -> Option<String> {
    normalized_optional_text(effective.config.openai_compatible.llm.base_url.as_deref())
}

/// Resolve the effective embedding API key from the embedding capability configuration only.
/// 仅从向量能力配置解析实际向量 API key。
pub(crate) fn effective_embedding_api_key(effective: &EffectiveModelConfig) -> Option<String> {
    normalized_optional_text(effective.embedding_api_key.as_deref())
}

/// Resolve the effective LLM API key from the LLM capability configuration only.
/// 仅从 LLM 能力配置解析实际 LLM API key。
pub(crate) fn effective_llm_api_key(effective: &EffectiveModelConfig) -> Option<String> {
    normalized_optional_text(effective.llm_api_key.as_deref())
}

/// Resolve a literal secret or an exact `${env:NAME}` reference into an optional value.
/// 将字面密钥或精确的 `${env:NAME}` 引用解析为可选值。
fn resolve_optional_secret(raw_value: Option<&str>) -> Option<String> {
    let value = normalized_optional_text(raw_value)?;
    let Some(env_name) = parse_exact_env_reference(&value) else {
        return Some(value);
    };
    std::env::var(env_name)
        .ok()
        .and_then(|value| normalized_optional_text(Some(&value)))
}

/// Parse an exact `${env:NAME}` reference and return the environment variable name.
/// 解析精确的 `${env:NAME}` 引用并返回环境变量名。
fn parse_exact_env_reference(value: &str) -> Option<&str> {
    value
        .strip_prefix("${env:")
        .and_then(|rest| rest.strip_suffix('}'))
        .map(str::trim)
        .filter(|name| !name.is_empty())
}

/// Normalize optional text by trimming whitespace and dropping blank values.
/// 规范化可选文本：去除首尾空白并丢弃空值。
fn normalized_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

/// Find the model-config file, preferring the runtime output directory and then falling back to the repository template directory.
/// 查找模型配置文件；优先使用运行时输出目录，其次回退到仓库模板目录。
fn find_model_config_path() -> Result<Option<PathBuf>, String> {
    find_optional_runtime_config_file(
        current_model_config_runtime_root()?,
        "model_config.yaml",
        "model config",
    )
}

/// Build a normalized load report from the current effective model configuration.
/// 根据当前生效模型配置构建统一的加载报告。
fn build_model_config_load_report(effective: &EffectiveModelConfig) -> ModelConfigLoadReport {
    let provider = &effective.config.openai_compatible;
    let embedding_base_url_configured = effective_embedding_base_url(effective).is_some();
    let embedding_api_key_configured = effective_embedding_api_key(effective).is_some();
    let llm_base_url_configured = effective_llm_base_url(effective).is_some();
    let llm_api_key_configured = effective_llm_api_key(effective).is_some();
    let embedding_enabled = provider.enabled
        && provider.embedding.enabled
        && embedding_base_url_configured
        && embedding_api_key_configured
        && normalized_optional_text(provider.embedding.model.as_deref()).is_some();
    let llm_enabled = provider.enabled
        && provider.llm.enabled
        && llm_base_url_configured
        && llm_api_key_configured
        && normalized_optional_text(provider.llm.model.as_deref()).is_some();
    ModelConfigLoadReport {
        source_path: effective
            .source_path
            .as_ref()
            .map(|path| path.to_string_lossy().to_string()),
        provider: "openai_compatible".to_string(),
        provider_enabled: provider.enabled,
        api_key_configured: embedding_api_key_configured || llm_api_key_configured,
        base_url_configured: embedding_base_url_configured || llm_base_url_configured,
        embedding_api_key_configured,
        embedding_base_url_configured,
        embedding_enabled,
        embedding_model: normalized_optional_text(provider.embedding.model.as_deref()),
        llm_enabled,
        llm_api_key_configured,
        llm_base_url_configured,
        llm_model: normalized_optional_text(provider.llm.model.as_deref()),
    }
}

#[cfg(test)]
mod tests;
