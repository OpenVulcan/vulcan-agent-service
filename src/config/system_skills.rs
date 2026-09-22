use std::collections::HashSet;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Strict host configuration for automatic ROOT system-skill initialization.
/// 用于自动初始化 ROOT 系统技能的严格宿主配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SystemSkillsConfig {
    /// Configuration schema version accepted by the host.
    /// 宿主接受的配置 schema 版本。
    pub(crate) format_version: u32,
    /// Whether startup may install missing enabled system skills automatically.
    /// 启动时是否允许自动安装缺失且启用的系统技能。
    #[serde(default = "default_true")]
    pub(crate) auto_install: bool,
    /// Ordered system-skill declarations supplied to the bootstrap layer.
    /// 提供给 bootstrap 层的有序系统技能声明列表。
    pub(crate) skills: Vec<SystemSkillSpec>,
}

/// One validated system-skill declaration and its GitHub release source.
/// 一个经过校验的系统技能声明及其 GitHub release 来源。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SystemSkillSpec {
    /// Stable LuaSkills skill identifier used for ROOT package ownership.
    /// 用于 ROOT 技能包归属的稳定 LuaSkills 技能标识符。
    pub(crate) name: String,
    /// GitHub repository locator normalized to `owner/repo` form.
    /// 规范化为 `owner/repo` 形式的 GitHub 仓库定位值。
    pub(crate) github: String,
    /// Whether this declaration participates in initialization and loading.
    /// 此声明是否参与初始化与加载。
    #[serde(default = "default_true")]
    pub(crate) enabled: bool,
}

/// Load and validate system-skill policy from one application root.
/// 从指定应用根目录加载并校验系统技能策略。
///
/// The application_root parameter identifies the directory containing `configs/system_skills.json`.
/// application_root 参数指定包含 `configs/system_skills.json` 的目录。
///
/// Returns the explicit file configuration, the embedded repository default when the file is absent,
/// or an explicit read/parse/validation error.
/// 返回显式文件配置、文件缺失时的仓库内置默认配置，或明确的读取/解析/校验错误。
pub fn load_system_skills(application_root: &Path) -> Result<SystemSkillsConfig, String> {
    // Resolve exactly one application-owned configuration path before selecting the embedded default.
    // 在选择内置默认值前只解析一个由应用根目录拥有的配置路径。
    let config_path = application_root.join("configs").join("system_skills.json");
    // Inspect an existing application root so a file-shaped root cannot masquerade as a missing config.
    // 检查已存在的应用根，避免文件形态的根目录伪装成缺失配置。
    match fs::metadata(application_root) {
        Ok(metadata) if !metadata.is_dir() => {
            return Err(format!(
                "system skills application root is not a directory: {}",
                application_root.display()
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "failed to inspect system skills application root '{}': {}",
                application_root.display(),
                error
            ));
        }
    }
    // Inspect the configs directory separately so a file-shaped directory is an explicit error.
    // 单独检查 configs 目录，确保文件形态的目录明确报错。
    let config_directory = application_root.join("configs");
    match fs::metadata(&config_directory) {
        Ok(metadata) if !metadata.is_dir() => {
            return Err(format!(
                "system skills config directory is not a directory: {}",
                config_directory.display()
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "failed to inspect system skills config directory '{}': {}",
                config_directory.display(),
                error
            ));
        }
    }
    // Inspect an existing config path so directory-shaped or special entries fail before reading.
    // 检查已存在的配置路径，确保目录形态或特殊条目在读取前明确失败。
    match fs::metadata(&config_path) {
        Ok(metadata) if !metadata.is_file() => {
            return Err(format!(
                "system skills config path is not a file: {}",
                config_path.display()
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "failed to inspect system skills config path '{}': {}",
                config_path.display(),
                error
            ));
        }
    }
    // Read the explicit configuration without creating or modifying any file on disk.
    // 读取显式配置且不在磁盘上创建或修改任何文件。
    let (content, origin) = match fs::read_to_string(&config_path) {
        Ok(content) => (content, config_path.display().to_string()),
        Err(error) if error.kind() == ErrorKind::NotFound => (
            include_str!("../../configs/system_skills.json").to_string(),
            "embedded default configs/system_skills.json".to_string(),
        ),
        Err(error) => {
            return Err(format!(
                "failed to read system skills config '{}': {}",
                config_path.display(),
                error
            ));
        }
    };

    // Parse and validate the selected source before returning it to bootstrap.
    // 在返回 bootstrap 前解析并校验选定的配置来源。
    parse_system_skills_config(&content, &origin)
}

/// Parse one strict system-skill JSON document and normalize every GitHub locator.
/// 解析一份严格的系统技能 JSON 文档并规范化每个 GitHub 定位值。
///
/// The content parameter is the JSON document text and origin identifies it in diagnostics.
/// content 参数是 JSON 文档文本，origin 用于错误诊断中的来源标识。
///
/// Returns the validated configuration with normalized repository locators.
/// 返回带有规范化仓库定位值的已校验配置。
fn parse_system_skills_config(content: &str, origin: &str) -> Result<SystemSkillsConfig, String> {
    // Deserialize with deny_unknown_fields so the schema cannot silently grow at runtime.
    // 使用 deny_unknown_fields 反序列化，避免运行时静默接受扩展字段。
    let mut config = serde_json::from_str::<SystemSkillsConfig>(content)
        .map_err(|error| format!("failed to parse system skills config from {origin}: {error}"))?;
    if config.format_version != crate::config::HOST_CONFIG_FORMAT_VERSION {
        return Err(format!(
            "unsupported system skills format_version {} in {origin}; expected {}",
            config.format_version,
            crate::config::HOST_CONFIG_FORMAT_VERSION
        ));
    }

    // Track normalized skill ids so duplicate declarations cannot produce ambiguous ROOT ownership.
    // 跟踪规范化技能 ID，避免重复声明产生不明确的 ROOT 归属。
    let mut seen_skill_ids = HashSet::with_capacity(config.skills.len());
    for (index, skill) in config.skills.iter_mut().enumerate() {
        // Reuse LuaSkills' public identifier validator so host and runtime accept the same id grammar.
        // 复用 LuaSkills 的公开标识符校验器，确保宿主与运行时接受相同的 ID 语法。
        luaskills::lua_skill::validate_luaskills_identifier(
            &skill.name,
            &format!("skills[{index}].name"),
        )
        .map_err(|error| format!("invalid system skill id in {origin}: {error}"))?;

        // Compare ids case-insensitively even though the upstream identifier grammar is lowercase-only.
        // 即使上游标识符语法限定小写，也按不区分大小写比较 ID。
        let normalized_skill_id = skill.name.to_ascii_lowercase();
        if !seen_skill_ids.insert(normalized_skill_id) {
            return Err(format!(
                "duplicate system skill id '{}' in {origin}",
                skill.name
            ));
        }

        // Normalize and validate the source before bootstrap constructs an install request.
        // 在 bootstrap 构造安装请求前规范化并校验来源。
        skill.github = normalize_github_source(&skill.github).map_err(|error| {
            format!(
                "invalid system skill GitHub source at skills[{index}].github in {origin}: {error}"
            )
        })?;
    }

    Ok(config)
}

/// Normalize one accepted GitHub repository locator to `owner/repo`.
/// 将一个允许的 GitHub 仓库定位值规范化为 `owner/repo`。
///
/// The source parameter accepts shorthand `owner/repo` or an HTTPS GitHub URL with an optional
/// `.git` suffix and trailing slash; credentials, query strings, fragments, and other hosts fail.
/// source 参数接受 `owner/repo` 简写或可选 `.git` 后缀和尾斜杠的 HTTPS GitHub URL；凭据、查询串、片段和其他主机均失败。
///
/// Returns the normalized repository locator or an explicit source-format error.
/// 返回规范化的仓库定位值，或明确的来源格式错误。
fn normalize_github_source(source: &str) -> Result<String, String> {
    // Trim only surrounding presentation whitespace; repository path characters remain strict.
    // 仅去除外层展示空白，仓库路径字符仍保持严格约束。
    let trimmed_source = source.trim();
    if trimmed_source.is_empty() {
        return Err("source must not be empty".to_string());
    }
    if trimmed_source.contains('?') || trimmed_source.contains('#') {
        return Err("source must not contain query or fragment".to_string());
    }

    // Extract the path only after proving that an explicit URL uses the GitHub HTTPS host.
    // 只有确认显式 URL 使用 GitHub HTTPS 主机后，才提取其路径。
    let repository_path = if let Some(url_tail) = trimmed_source.strip_prefix("https://") {
        let (host, path) = url_tail.split_once('/').ok_or_else(|| {
            "HTTPS GitHub source must use https://github.com/owner/repo form".to_string()
        })?;
        if host.contains('@') || !host.eq_ignore_ascii_case("github.com") {
            return Err(
                "HTTPS GitHub source must use the github.com host without credentials".to_string(),
            );
        }
        path
    } else if trimmed_source.starts_with("http://") {
        return Err("GitHub source must use HTTPS".to_string());
    } else {
        if trimmed_source.contains("://") || trimmed_source.contains('\\') {
            return Err("source must be owner/repo or an HTTPS GitHub URL".to_string());
        }
        trimmed_source
    };

    // Remove one optional trailing slash while rejecting nested or empty path components later.
    // 去除一个可选尾斜杠，并在后续拒绝嵌套或空路径组件。
    let repository_path = repository_path.strip_suffix('/').unwrap_or(repository_path);
    if repository_path.is_empty()
        || repository_path.starts_with('/')
        || repository_path.ends_with('/')
    {
        return Err("source must contain exactly one owner and one repository".to_string());
    }

    // Split the locator into the exact owner/repository pair consumed by LuaSkills.
    // 将定位值拆分为 LuaSkills 消费的精确 owner/repository 二元组。
    let mut segments = repository_path.split('/');
    // Capture the GitHub account or organization segment.
    // 提取 GitHub 账户或组织片段。
    let owner = segments.next().unwrap_or_default();
    // Capture the repository segment before optional `.git` normalization.
    // 提取仓库片段，并在此之前保留可选 `.git` 规范化入口。
    let mut repository = segments.next().unwrap_or_default().to_string();
    // Any third segment proves that the source is not one repository locator.
    // 第三个片段表明来源不是单个仓库定位值。
    if segments.next().is_some() || owner.is_empty() || repository.is_empty() {
        return Err("source must contain exactly one owner and one repository".to_string());
    }

    // Strip the optional Git suffix accepted by the host-facing configuration contract.
    // 去除宿主配置契约允许的可选 Git 后缀。
    if repository.to_ascii_lowercase().ends_with(".git") {
        repository.truncate(repository.len() - 4);
    }
    if repository.is_empty() {
        return Err("repository name must not be empty".to_string());
    }

    // Match the upstream manager's repository-derived skill-id grammar for the repository segment.
    // 对仓库片段使用上游管理器从仓库派生技能 ID 时采用的语法。
    luaskills::lua_skill::validate_luaskills_identifier(repository.as_str(), "GitHub repository")?;
    if !owner
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '-')
    {
        return Err("GitHub owner must contain only ASCII letters, digits, or '-'".to_string());
    }

    Ok(format!("{owner}/{repository}"))
}

/// Supply the schema default for boolean fields whose contract defaults to enabled.
/// 提供契约规定默认为启用的布尔字段 schema 默认值。
fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Build a unique temporary application root for one isolated loader test.
    /// 为单个隔离加载器测试构造唯一临时应用根目录。
    fn unique_test_root(case_name: &str) -> std::path::PathBuf {
        // Use process and timestamp components to avoid fixture collisions.
        // 使用进程号和时间戳组件避免测试夹具冲突。
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after Unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "vulcan-agent-service-system-skills-{case_name}-{}-{timestamp}",
            std::process::id()
        ))
    }

    /// Write one explicit JSON configuration under an isolated application root.
    /// 在隔离应用根目录下写入一份显式 JSON 配置。
    fn write_config(root: &Path, content: &str) -> std::path::PathBuf {
        // Resolve the exact application-owned path used by the production loader.
        // 解析生产加载器使用的精确应用配置路径。
        let config_path = root.join("configs").join("system_skills.json");
        std::fs::create_dir_all(config_path.parent().expect("config parent should exist"))
            .expect("config directory should be created");
        std::fs::write(&config_path, content).expect("system skills config should be written");
        config_path
    }

    /// Verify schema defaults, source normalization, and the six repository defaults.
    /// 验证 schema 默认值、来源规范化以及六个仓库默认项。
    #[test]
    fn schema_defaults_and_embedded_template_are_valid() {
        // Parse an explicit minimal document to verify omitted booleans default to true.
        // 解析一份最小显式文档，验证省略的布尔字段默认为 true。
        let parsed = parse_system_skills_config(
            r#"{"format_version":1,"skills":[{"name":"demo-skill","github":"https://github.com/Owner/demo-skill.git/"}]}"#,
            "test document",
        )
        .expect("minimal system skills schema should parse");
        assert!(parsed.auto_install);
        assert!(parsed.skills[0].enabled);
        assert_eq!(parsed.skills[0].github, "Owner/demo-skill");

        // Parse the embedded release template and assert the exact six system skills.
        // 解析内置发布模板并断言精确的六个系统技能。
        let embedded = parse_system_skills_config(
            include_str!("../../configs/system_skills.json"),
            "embedded default",
        )
        .expect("embedded system skills template should parse");
        let names = embedded
            .skills
            .iter()
            .map(|skill| skill.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                "vulcan-codekit",
                "vulcan-file",
                "vulcan-curl",
                "vulcan-lua",
                "vulcan-testkit",
                "vulcan-workmem",
            ]
        );
        assert!(
            !names
                .iter()
                .any(|name| name.eq_ignore_ascii_case("vulcan-ai-memory"))
        );
    }

    /// Verify an explicit disabled policy is authoritative and prevents fallback substitution.
    /// 验证显式禁用策略具有权威性，不会被默认配置替换。
    #[test]
    fn explicit_disabled_configuration_is_authoritative() {
        // Use an empty explicit list to distinguish the file from the embedded six-skill default.
        // 使用空显式列表，以区分配置文件与内置六技能默认值。
        let root = unique_test_root("explicit-disabled");
        let config_path = write_config(
            &root,
            r#"{"format_version":1,"auto_install":false,"skills":[]}"#,
        );

        // Load the explicit policy and verify that no embedded skills leak into the result.
        // 加载显式策略并验证内置技能不会泄漏到结果中。
        let config = load_system_skills(&root).expect("explicit disabled config should load");
        assert!(!config.auto_install);
        assert!(config.skills.is_empty());

        // Remove only the isolated fixture created by this test.
        // 仅删除本测试创建的隔离夹具。
        std::fs::remove_dir_all(root).expect("explicit test root should be removed");
        assert!(!config_path.exists());
    }

    /// Verify a missing explicit file uses the embedded default without writing it to disk.
    /// 验证显式文件缺失时使用内置默认值且不会写回磁盘。
    #[test]
    fn missing_configuration_uses_embedded_default_without_persisting() {
        // Keep the temporary root entirely absent so the loader exercises its missing-file branch.
        // 保持临时根目录完全不存在，使加载器进入文件缺失分支。
        let root = unique_test_root("missing-default");
        let config = load_system_skills(&root).expect("missing config should use embedded default");

        // Confirm the six default entries are available while no config directory was created.
        // 确认六个默认条目可用，同时配置目录没有被创建。
        assert_eq!(config.skills.len(), 6);
        assert!(!root.exists());
    }

    /// Verify unknown fields, duplicate ids, invalid ids, and invalid GitHub sources fail explicitly.
    /// 验证未知字段、重复 ID、非法 ID 和非法 GitHub 来源都会明确失败。
    #[test]
    fn invalid_schema_and_sources_are_rejected() {
        // Unknown root fields must not be silently ignored.
        // 根对象未知字段不得被静默忽略。
        let unknown_field = parse_system_skills_config(
            r#"{"format_version":1,"skills":[],"unexpected":true}"#,
            "unknown field",
        )
        .expect_err("unknown fields should fail");
        assert!(unknown_field.contains("unexpected"));

        // Duplicate ids are rejected using case-insensitive identity comparison.
        // 重复 ID 使用不区分大小写的身份比较并被拒绝。
        let duplicate = parse_system_skills_config(
            r#"{"format_version":1,"skills":[{"name":"demo-skill","github":"Owner/demo-skill"},{"name":"demo-skill","github":"Owner/demo-skill"}]}"#,
            "duplicate ids",
        )
        .expect_err("duplicate ids should fail");
        assert!(duplicate.contains("duplicate system skill id"));

        // LuaSkills identifier grammar rejects an invalid skill id before installation.
        // LuaSkills 标识符语法会在安装前拒绝非法技能 ID。
        let invalid_id = parse_system_skills_config(
            r#"{"format_version":1,"skills":[{"name":"Demo Skill","github":"Owner/demo-skill"}]}"#,
            "invalid id",
        )
        .expect_err("invalid skill ids should fail");
        assert!(invalid_id.contains("invalid system skill id"));

        // Arbitrary hosts, credentials, query strings, and fragments are not GitHub sources.
        // 任意主机、凭据、查询串和片段都不是允许的 GitHub 来源。
        for source in [
            "https://example.com/Owner/demo-skill",
            "https://user:pass@github.com/Owner/demo-skill",
            "https://github.com/Owner/demo-skill?ref=main",
            "https://github.com/Owner/demo-skill#readme",
        ] {
            // Build one source-specific document for the current invalid locator.
            // 为当前非法定位值构造一份来源专属文档。
            let document = format!(
                r#"{{"format_version":1,"skills":[{{"name":"demo-skill","github":"{source}"}}]}}"#
            );
            let error = parse_system_skills_config(&document, "invalid source")
                .expect_err("invalid GitHub source should fail");
            assert!(error.contains("invalid system skill GitHub source"));
        }
    }
}
