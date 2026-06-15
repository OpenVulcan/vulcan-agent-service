use super::*;
use std::sync::{Mutex, OnceLock};

/// Return one shared mutex used to serialize runtime-root override tests for model-config loading.
/// 返回一个共享互斥锁，用于串行化模型配置加载中的运行根覆盖测试。
fn runtime_root_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Return one shared mutex used to serialize environment-variable tests for model-config loading.
/// 返回一个共享互斥锁，用于串行化模型配置加载中的环境变量测试。
fn environment_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Build one unique temporary directory path for a model-config test case.
/// 为模型配置测试用例构建唯一临时目录路径。
fn unique_test_dir(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "vulcan-agent-service-model-config-{}-{}-{}",
        name,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ))
}

/// Verify that the repository model-config template stays parseable.
/// 验证仓库内模型配置模板始终可解析。
#[test]
fn model_config_template_parses() {
    let yaml = include_str!("../../../runtime/configs/model_config.yaml");
    let parsed: ModelConfig = serde_yaml::from_str(yaml).expect("model_config.yaml parses");

    assert!(!parsed.openai_compatible.enabled);
}

/// Verify that disabled providers do not require a present API key.
/// 验证未启用供应商时不要求 API key 存在。
#[test]
fn disabled_provider_allows_missing_env_secret() {
    let parsed: ModelConfig = serde_yaml::from_value(
        serde_yaml::to_value(json!({
            "openai_compatible": {
                "enabled": false,
                "embedding": {
                    "enabled": true,
                    "base_url": "https://example.test/v1",
                    "api_key": "${env:VULCAN_MISSING_MODEL_KEY}",
                    "model": "embed-small"
                }
            }
        }))
        .expect("json converts to yaml"),
    )
    .expect("config should parse");
    let effective = EffectiveModelConfig {
        config: parsed,
        embedding_api_key: None,
        llm_api_key: None,
        source_path: None,
    };

    validate_effective_model_config(&effective).expect("disabled provider should pass");
}

/// Verify that enabled capabilities require an API key after environment resolution.
/// 验证启用能力时必须能在环境解析后获得 API key。
#[test]
fn enabled_capability_requires_resolved_api_key() {
    let parsed: ModelConfig = serde_yaml::from_value(
        serde_yaml::to_value(json!({
            "openai_compatible": {
                "enabled": true,
                "embedding": {
                    "enabled": true,
                    "base_url": "https://example.test/v1",
                    "api_key": "${env:VULCAN_MISSING_MODEL_KEY}",
                    "model": "embed-small"
                }
            }
        }))
        .expect("json converts to yaml"),
    )
    .expect("config should parse");
    let effective = EffectiveModelConfig {
        config: parsed,
        embedding_api_key: None,
        llm_api_key: None,
        source_path: None,
    };

    let error =
        validate_effective_model_config(&effective).expect_err("missing resolved key should fail");
    assert!(error.contains("api_key"));
    assert!(error.contains("VULCAN_MISSING_MODEL_KEY"));
    assert!(error.contains("current process environment"));
    if cfg!(windows) {
        assert!(error.contains("LocalSystem"));
    }
}

/// Verify that an enabled embedding capability can use its own provider credentials without LLM credentials.
/// 验证启用向量能力时可以只使用自身供应商凭据，而不要求 LLM 凭据存在。
#[test]
fn enabled_embedding_accepts_capability_specific_credentials() {
    let parsed: ModelConfig = serde_yaml::from_value(
        serde_yaml::to_value(json!({
            "openai_compatible": {
                "enabled": true,
                "embedding": {
                    "enabled": true,
                    "base_url": "https://embedding.example.test/v1",
                    "api_key": "sk-embed",
                    "model": "embed-small"
                },
                "llm": {
                    "enabled": false,
                    "base_url": "${env:VULCAN_MISSING_LLM_URL}",
                    "api_key": "${env:VULCAN_MISSING_LLM_KEY}",
                    "model": "llm-small"
                }
            }
        }))
        .expect("json converts to yaml"),
    )
    .expect("config should parse");
    let effective = EffectiveModelConfig {
        config: parsed,
        embedding_api_key: Some("sk-embed".to_string()),
        llm_api_key: None,
        source_path: None,
    };

    validate_effective_model_config(&effective)
        .expect("embedding-specific credentials should pass");
}

/// Verify that provider-level credentials are rejected instead of being treated as fallbacks.
/// 验证供应商级凭据会被拒绝，而不是被当作回退配置。
#[test]
fn shared_provider_credentials_are_rejected() {
    let error = serde_yaml::from_value::<ModelConfig>(
        serde_yaml::to_value(json!({
            "openai_compatible": {
                "enabled": true,
                "base_url": "https://shared.example.test/v1",
                "api_key": "sk-shared",
                "embedding": {
                    "enabled": true,
                    "model": "embed-small"
                }
            }
        }))
        .expect("json converts to yaml"),
    )
    .expect_err("provider-level credentials should be rejected");

    let error_text = error.to_string();
    assert!(error_text.contains("unknown field"));
    assert!(error_text.contains("base_url") || error_text.contains("api_key"));
}

/// Verify that explicit runtime-root overrides redirect model-config preload to the selected runtime.
/// 验证显式 runtime_root 覆盖会把模型配置预载重定向到选中的运行根。
#[test]
fn preload_model_config_prefers_explicit_runtime_root() {
    let _runtime_guard = runtime_root_lock()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let _environment_guard = environment_lock()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let previous = std::env::var("VULCAN_TEST_MODEL_KEY").ok();
    unsafe {
        std::env::set_var("VULCAN_TEST_MODEL_KEY", "sk-test");
    }
    let root = unique_test_dir("runtime-root");
    let config_path = root.join("configs").join("model_config.yaml");
    std::fs::create_dir_all(config_path.parent().expect("config dir should exist"))
        .expect("failed to create config directory");
    std::fs::write(
        &config_path,
        r#"
openai_compatible:
  enabled: true
  embedding:
    enabled: true
    base_url: "https://example.test/v1"
    api_key: "${env:VULCAN_TEST_MODEL_KEY}"
    model: "embed-small"
"#,
    )
    .expect("failed to write model config");

    initialize_model_config_runtime_root(Some(&root)).expect("runtime root init should work");
    let report = preload_model_config().expect("model config preload should succeed");
    assert_eq!(
        report.source_path.as_deref(),
        Some(config_path.to_string_lossy().as_ref())
    );
    assert!(report.embedding_enabled);
    assert!(report.embedding_api_key_configured);
    assert!(report.embedding_base_url_configured);

    if let Some(value) = previous {
        unsafe {
            std::env::set_var("VULCAN_TEST_MODEL_KEY", value);
        }
    } else {
        unsafe {
            std::env::remove_var("VULCAN_TEST_MODEL_KEY");
        }
    }
    initialize_model_config_runtime_root(None).expect("runtime root clear should work");
    let _ = std::fs::remove_dir_all(root);
}
