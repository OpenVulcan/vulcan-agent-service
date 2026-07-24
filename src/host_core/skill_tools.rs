use serde::Deserialize;
use serde_json::Value;

use crate::host_core::model::{RuntimeTextContent, RuntimeToolCallResult};
use crate::support::append_rendered_line;
use luaskills::{
    InstalledSkillRecord, LuaRuntimeHostOptions, RuntimeSkillRoot, SkillApplyResult,
    SkillInstallSourceType, SkillManager, SkillManagerConfig, SkillUninstallResult,
};

/// Supported actions for the host-owned skill-manager tool.
/// 宿主自有 skill-manager 工具支持的动作集合。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum SkillManagerAction {
    /// List locally effective skills with paths and managed install records.
    /// 列出本地生效技能及其路径和受管安装记录。
    List,
    /// Install one managed skill from a source locator.
    /// 从来源定位值安装一个受管技能。
    Install,
    /// Update one installed managed skill by skill id.
    /// 通过技能标识更新一个已安装的受管技能。
    Update,
    /// Uninstall one installed skill by skill id while retaining databases.
    /// 通过技能标识卸载一个已安装技能并保留数据库。
    Uninstall,
}

/// Parsed arguments for one host-owned skill-manager tool call.
/// 一次宿主自有 skill-manager 工具调用解析后的参数载荷。
#[derive(Debug, Clone, Deserialize)]
pub(super) struct SkillManagerToolArguments {
    /// Action selector that chooses one of `list/install/update/uninstall`.
    /// 动作选择器，用于决定 `list/install/update/uninstall` 中的哪一种。
    pub(super) action: SkillManagerAction,
    /// Optional install source locator such as `LuaSkills/vulcan-codekit` or a source YAML URL.
    /// 可选安装来源定位值，例如 `LuaSkills/vulcan-codekit` 或 source YAML 地址。
    pub(super) source: Option<String>,
    /// Optional source type override; omitted values are inferred from `source`.
    /// 可选来源类型覆盖；未提供时从 `source` 自动推导。
    pub(super) source_type: Option<SkillInstallSourceType>,
    /// Optional target skill id required by update and uninstall actions.
    /// update 与 uninstall 动作必填的可选目标技能标识。
    pub(super) skill_id: Option<String>,
}

/// Parse one skill-manager argument payload into the strongly typed host-tool request model.
/// 把一份 skill-manager 参数载荷解析为强类型宿主工具请求模型。
pub(super) fn parse_skill_manager_tool_arguments(
    args: &Value,
) -> Result<SkillManagerToolArguments, (i64, String)> {
    if args.get("layer").is_some() {
        return Err((
            -32602,
            "skill-manager is locked to the USER layer and does not accept a layer parameter."
                .to_string(),
        ));
    }
    serde_json::from_value(args.clone()).map_err(|error| {
        (
            -32602,
            format!("Invalid skill-manager arguments: {}", error),
        )
    })
}

/// Parse an optional skill install source type supplied by a stable gRPC method.
/// 解析稳定 gRPC 方法传入的可选技能安装来源类型。
pub(super) fn parse_optional_skill_install_source_type(
    value: Option<&str>,
) -> Result<Option<SkillInstallSourceType>, (i64, String)> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    match value {
        "github" => Ok(Some(SkillInstallSourceType::Github)),
        "url" => Ok(Some(SkillInstallSourceType::Url)),
        _ => Err((
            -32602,
            format!(
                "Unsupported skill install source_type '{}'; expected 'github' or 'url'.",
                value
            ),
        )),
    }
}

/// Select the concrete USER runtime root used by the user-facing skill-manager tool.
/// 选择面向用户的 skill-manager 工具固定使用的 USER 运行时根。
pub(super) fn select_skill_manager_user_root(
    roots: &[RuntimeSkillRoot],
) -> Result<RuntimeSkillRoot, (i64, String)> {
    let selected = roots
        .iter()
        .find(|root| root.name.trim().eq_ignore_ascii_case("USER"));
    selected.cloned().ok_or_else(|| {
        (
            -32603,
            "skill-manager USER layer is not configured.".to_string(),
        )
    })
}

/// Require one non-empty install source for a skill-manager install action.
/// 要求 skill-manager 安装动作提供一个非空安装来源。
pub(super) fn require_skill_manager_source(
    value: Option<&str>,
    action: &str,
) -> Result<String, (i64, String)> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            (
                -32602,
                format!(
                    "skill-manager action '{}' requires parameter: source",
                    action
                ),
            )
        })
}

/// Require one non-empty skill id for a skill-manager target action.
/// 要求 skill-manager 目标动作提供一个非空技能标识。
pub(super) fn require_skill_manager_skill_id(
    value: Option<&str>,
    action: &str,
) -> Result<String, (i64, String)> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            (
                -32602,
                format!(
                    "skill-manager action '{}' requires parameter: skill_id",
                    action
                ),
            )
        })
}

/// Infer the install source type from one source locator unless the caller supplied an override.
/// 除非调用方提供覆盖值，否则从单个来源定位值推导安装来源类型。
pub(super) fn infer_skill_install_source_type(
    source: &str,
    explicit: Option<SkillInstallSourceType>,
) -> SkillInstallSourceType {
    if let Some(source_type) = explicit {
        return source_type;
    }
    let source = source.trim().to_ascii_lowercase();
    if (source.starts_with("http://") || source.starts_with("https://"))
        && !source.contains("github.com/")
    {
        SkillInstallSourceType::Url
    } else {
        SkillInstallSourceType::Github
    }
}

/// Build one SkillManager that mirrors LuaEngine's root-relative lifecycle layout.
/// 构造一个与 LuaEngine 根目录相对生命周期布局保持一致的 SkillManager。
pub(super) fn build_skill_manager_for_root(
    root: &RuntimeSkillRoot,
    host_options: &LuaRuntimeHostOptions,
) -> Result<SkillManager, (i64, String)> {
    let runtime_root = root
        .skills_dir
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_else(|| root.skills_dir.clone());
    let lifecycle_root = runtime_root.join(host_options.state_dir_name.as_str());
    let download_cache_root = host_options.download_cache_root.clone().unwrap_or_else(|| {
        host_options
            .temp_dir
            .clone()
            .unwrap_or_else(|| runtime_root.join("temp"))
            .join("downloads")
    });
    Ok(SkillManager::new(SkillManagerConfig {
        skill_root: root.clone(),
        lifecycle_root,
        download_cache_root,
        allow_network_download: host_options.allow_network_download,
        github_base_url: host_options.github_base_url.clone(),
        github_api_base_url: host_options.github_api_base_url.clone(),
        official_skill_hub_base_url: host_options.official_skill_hub_base_url.clone(),
        enable_private_url_skill_install: host_options.enable_private_url_skill_install,
        private_skill_source_allowlist: host_options.private_skill_source_allowlist.clone(),
    }))
}

/// Render one managed install record into the skill-manager list output.
/// 将单条受管安装记录渲染到 skill-manager 列表输出中。
pub(super) fn render_skill_install_record(rendered: &mut String, record: &InstalledSkillRecord) {
    append_rendered_line(rendered, format_args!("- managed: {}", record.managed));
    append_rendered_line(rendered, format_args!("- version: {}", record.version));
    append_rendered_line(
        rendered,
        format_args!(
            "- source: {} {}",
            render_skill_install_source_type(record.source.source_type),
            record.source.locator
        ),
    );
    if let Some(tag) = record.source.tag.as_deref() {
        append_rendered_line(rendered, format_args!("- source_tag: {}", tag));
    }
    append_rendered_line(
        rendered,
        format_args!("- installed_at_unix_ms: {}", record.installed_at_unix_ms),
    );
}

/// Render one skill install source type as a stable snake-case string.
/// 将单个技能安装来源类型渲染为稳定的蛇形命名字符串。
fn render_skill_install_source_type(source_type: SkillInstallSourceType) -> &'static str {
    match source_type {
        SkillInstallSourceType::Github => "github",
        SkillInstallSourceType::OfficialHub => "official_hub",
        SkillInstallSourceType::Url => "url",
        SkillInstallSourceType::PrivateUrlManifest => "private_url_manifest",
    }
}

/// Render the current explicit URL-install unsupported result before LuaSkills sees the request.
/// 在 LuaSkills 接收请求前渲染当前 URL 安装不支持的明确结果。
pub(super) fn render_skill_url_install_not_implemented_result() -> RuntimeToolCallResult {
    RuntimeToolCallResult {
        content: vec![RuntimeTextContent::text(
            "skill-manager install failed: managed URL install is not implemented yet; GitHub install is currently the only supported install source.",
        )],
        is_error: Some(true),
    }
}

/// Render one install or update operation result into a runtime tool result.
/// 将单个安装或更新操作结果渲染为运行时工具结果。
pub(super) fn render_skill_apply_tool_result(
    action: &str,
    result: Result<SkillApplyResult, String>,
) -> RuntimeToolCallResult {
    match result {
        Ok(result) => RuntimeToolCallResult {
            content: vec![RuntimeTextContent::text(&render_skill_apply_result(
                action, &result,
            ))],
            is_error: None,
        },
        Err(error) => RuntimeToolCallResult {
            content: vec![RuntimeTextContent::text(&format!(
                "skill-manager {} failed: {}",
                action, error
            ))],
            is_error: Some(true),
        },
    }
}

/// Render one successful install or update operation result as compact Markdown.
/// 将单个成功安装或更新操作结果渲染为紧凑 Markdown。
fn render_skill_apply_result(action: &str, result: &SkillApplyResult) -> String {
    let mut rendered = String::new();
    append_rendered_line(&mut rendered, format_args!("# skill-manager {}", action));
    append_rendered_line(&mut rendered, format_args!("- layer: USER"));
    append_rendered_line(
        &mut rendered,
        format_args!("- skill_id: {}", result.skill_id),
    );
    append_rendered_line(&mut rendered, format_args!("- status: {}", result.status));
    if let Some(version) = result.version.as_deref() {
        append_rendered_line(&mut rendered, format_args!("- version: {}", version));
    }
    if let Some(source_type) = result.source_type {
        append_rendered_line(
            &mut rendered,
            format_args!(
                "- source_type: {}",
                render_skill_install_source_type(source_type)
            ),
        );
    }
    if let Some(source_locator) = result.source_locator.as_deref() {
        append_rendered_line(&mut rendered, format_args!("- source: {}", source_locator));
    }
    append_rendered_line(&mut rendered, format_args!("- message: {}", result.message));
    rendered
}

/// Render one uninstall operation result into a runtime tool result.
/// 将单个卸载操作结果渲染为运行时工具结果。
pub(super) fn render_skill_uninstall_tool_result(
    result: Result<SkillUninstallResult, String>,
) -> RuntimeToolCallResult {
    match result {
        Ok(result) => RuntimeToolCallResult {
            content: vec![RuntimeTextContent::text(&render_skill_uninstall_result(
                &result,
            ))],
            is_error: None,
        },
        Err(error) => RuntimeToolCallResult {
            content: vec![RuntimeTextContent::text(&format!(
                "skill-manager uninstall failed: {}",
                error
            ))],
            is_error: Some(true),
        },
    }
}

/// Render one successful uninstall operation result as compact Markdown.
/// 将单个成功卸载操作结果渲染为紧凑 Markdown。
fn render_skill_uninstall_result(result: &SkillUninstallResult) -> String {
    let mut rendered = String::new();
    append_rendered_line(&mut rendered, format_args!("# skill-manager uninstall"));
    append_rendered_line(&mut rendered, format_args!("- layer: USER"));
    append_rendered_line(
        &mut rendered,
        format_args!("- skill_id: {}", result.skill_id),
    );
    append_rendered_line(
        &mut rendered,
        format_args!("- skill_removed: {}", result.skill_removed),
    );
    append_rendered_line(
        &mut rendered,
        format_args!("- sqlite_removed: {}", result.sqlite_removed),
    );
    append_rendered_line(
        &mut rendered,
        format_args!("- lancedb_removed: {}", result.lancedb_removed),
    );
    append_rendered_line(
        &mut rendered,
        format_args!("- sqlite_retained: {}", result.sqlite_retained),
    );
    append_rendered_line(
        &mut rendered,
        format_args!("- lancedb_retained: {}", result.lancedb_retained),
    );
    append_rendered_line(&mut rendered, format_args!("- message: {}", result.message));
    rendered
}
