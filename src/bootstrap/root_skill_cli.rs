use luaskills::{
    SkillApplyResult, SkillInstallRequest, SkillInstallSourceType, SkillManagementAuthority,
};

use super::runtime_init::{
    add_libs_to_path, build_root_skill_cli_context, build_root_skill_manager_for_cli,
    collect_managed_root_skill_ids, initialize_runtime_temp_root_from_config,
};
use super::runtime_preload::preload_runtime_mcp_configs;
use crate::config::Config;
use crate::luaskills_adapter::install_luaskills_log_callback;
use crate::support::runtime_logging::set_non_error_logging_enabled;
use crate::support::temp_maintenance::{CleanupTrigger, maintain_runtime_temp_dir};
use crate::support::{append_blank_rendered_line, append_rendered_line};

/// Install one managed LuaSkill into ROOT from the local CLI without starting MCP transports.
/// 在不启动 MCP 传输服务的情况下，从本地 CLI 将单个受管 LuaSkill 安装到 ROOT。
pub(super) fn run_root_skill_install_mode(
    source: &str,
    source_type: Option<SkillInstallSourceType>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Prepare configuration, cache, PATH, and LuaSkills callbacks for a local lifecycle command.
    // 为本地生命周期命令准备配置、缓存、PATH 与 LuaSkills 回调。
    let config = initialize_root_skill_cli_config()?;
    // Build the single-process ROOT lifecycle context after runtime paths are ready.
    // 在运行路径就绪后构建单进程 ROOT 生命周期上下文。
    let mut context = build_root_skill_cli_context(&config)?;
    // Infer the install source type only when the caller did not supply an override.
    // 仅在调用方未提供覆盖时推导安装来源类型。
    let source_type = infer_root_skill_install_source_type(source, source_type);
    // Keep the CLI request shape aligned with the LuaSkills managed install API.
    // 保持 CLI 请求形态与 LuaSkills 受管安装 API 对齐。
    let request = SkillInstallRequest {
        skill_id: None,
        source: Some(source.to_string()),
        source_type,
    };
    // Execute through the system authority so ROOT remains inaccessible to delegated tools.
    // 通过 system 权限执行，确保 ROOT 仍不会暴露给委托工具。
    let result = context.engine.system_install_skill_in_root(
        &context.skill_roots,
        &context.target_root,
        SkillManagementAuthority::System,
        &request,
    )?;

    println!("{}", render_root_skill_apply_result("install", &result));
    Ok(())
}

/// Update every managed LuaSkill declared in ROOT from the local CLI without starting MCP transports.
/// 在不启动 MCP 传输服务的情况下，从本地 CLI 更新 ROOT 中声明的全部受管 LuaSkill。
pub(super) fn run_root_skills_update_mode() -> Result<(), Box<dyn std::error::Error>> {
    // Prepare configuration, cache, PATH, and LuaSkills callbacks for a local lifecycle command.
    // 为本地生命周期命令准备配置、缓存、PATH 与 LuaSkills 回调。
    let config = initialize_root_skill_cli_config()?;
    // Build the single-process ROOT lifecycle context after runtime paths are ready.
    // 在运行路径就绪后构建单进程 ROOT 生命周期上下文。
    let mut context = build_root_skill_cli_context(&config)?;
    // Build a manager for reading ROOT install records before attempting updates.
    // 构建用于在尝试更新前读取 ROOT 安装记录的管理器。
    let manager = build_root_skill_manager_for_cli(&context.target_root, &context.host_options)?;
    // Collect only managed ROOT skills, because unmanaged directories have no update source.
    // 仅收集受管 ROOT 技能，因为非受管目录没有可用的更新来源。
    let managed_skill_ids = collect_managed_root_skill_ids(&context.target_root, &manager)?;
    // Render a single command summary so partial failures remain visible to shell callers.
    // 渲染单份命令摘要，确保 shell 调用方能看到局部失败。
    let mut rendered = String::new();
    append_rendered_line(
        &mut rendered,
        format_args!("# root-skill-manager update-all"),
    );
    append_rendered_line(&mut rendered, format_args!("- layer: ROOT"));
    append_rendered_line(
        &mut rendered,
        format_args!(
            "- target_root: {}",
            context.target_root.skills_dir.display()
        ),
    );

    if managed_skill_ids.is_empty() {
        append_rendered_line(&mut rendered, format_args!("- status: no_managed_skills"));
        append_rendered_line(
            &mut rendered,
            format_args!("- message: no managed ROOT LuaSkills are installed"),
        );
        println!("{}", rendered);
        return Ok(());
    }

    // Count failed updates so the command can return a non-zero process status after printing details.
    // 统计失败更新数量，以便命令打印详情后返回非零进程状态。
    let mut failure_count = 0usize;
    for skill_id in managed_skill_ids {
        // Build one update request from the persisted managed install record identity.
        // 根据持久化受管安装记录标识构建单个更新请求。
        let request = SkillInstallRequest {
            skill_id: Some(skill_id.clone()),
            source: None,
            source_type: SkillInstallSourceType::Github,
        };
        // Execute each update through the system authority so ROOT writes stay host-controlled.
        // 每个更新都通过 system 权限执行，确保 ROOT 写入保持宿主控制。
        let result = context.engine.system_update_skill_in_root(
            &context.skill_roots,
            &context.target_root,
            SkillManagementAuthority::System,
            &request,
        );
        match result {
            Ok(result) => append_root_skill_update_result(&mut rendered, &result),
            Err(error) => {
                failure_count += 1;
                append_root_skill_update_error(&mut rendered, &skill_id, error.as_ref());
            }
        }
    }

    println!("{}", rendered);
    if failure_count > 0 {
        return Err(format!("ROOT skill update failed for {} skill(s)", failure_count).into());
    }
    Ok(())
}

/// Initialize shared runtime state for local ROOT lifecycle commands.
/// 为本地 ROOT 生命周期命令初始化共享运行时状态。
fn initialize_root_skill_cli_config() -> Result<Config, Box<dyn std::error::Error>> {
    set_non_error_logging_enabled(false);
    install_luaskills_log_callback();
    // Load config through the normal runtime-root discovery path so CLI behavior stays consistent.
    // 通过标准 runtime-root 发现路径加载配置，保持 CLI 行为一致。
    let config = Config::load()?;
    initialize_runtime_temp_root_from_config(&config)?;
    maintain_runtime_temp_dir(CleanupTrigger::Startup)?;
    preload_runtime_mcp_configs(&config)?;
    add_libs_to_path(&config)?;
    Ok(config)
}

/// Infer the ROOT install source type when the CLI caller did not provide an explicit override.
/// 当 CLI 调用方未提供显式覆盖时推导 ROOT 安装来源类型。
fn infer_root_skill_install_source_type(
    source: &str,
    explicit: Option<SkillInstallSourceType>,
) -> SkillInstallSourceType {
    if let Some(source_type) = explicit {
        return source_type;
    }
    // Treat non-GitHub HTTP(S) locators as URL sources and everything else as GitHub.
    // 将非 GitHub HTTP(S) 定位值视为 URL 来源，其余视为 GitHub 来源。
    let normalized_source = source.trim().to_ascii_lowercase();
    if (normalized_source.starts_with("http://") || normalized_source.starts_with("https://"))
        && !normalized_source.contains("github.com/")
    {
        SkillInstallSourceType::Url
    } else {
        SkillInstallSourceType::Github
    }
}

/// Render one ROOT install or update result as compact command-line Markdown.
/// 将单个 ROOT 安装或更新结果渲染为紧凑的命令行 Markdown。
fn render_root_skill_apply_result(action: &str, result: &SkillApplyResult) -> String {
    // Build output in the same high-signal shape as the MCP skill-manager result.
    // 使用与 MCP skill-manager 结果相同的高信号形态构建输出。
    let mut rendered = String::new();
    append_rendered_line(
        &mut rendered,
        format_args!("# root-skill-manager {}", action),
    );
    append_rendered_line(&mut rendered, format_args!("- layer: ROOT"));
    append_root_skill_apply_fields(&mut rendered, result);
    rendered
}

/// Append common apply-result fields shared by ROOT install and update output.
/// 追加 ROOT 安装与更新输出共享的应用结果字段。
fn append_root_skill_apply_fields(rendered: &mut String, result: &SkillApplyResult) {
    append_rendered_line(rendered, format_args!("- skill_id: {}", result.skill_id));
    append_rendered_line(rendered, format_args!("- status: {}", result.status));
    if let Some(version) = result.version.as_deref() {
        append_rendered_line(rendered, format_args!("- version: {}", version));
    }
    if let Some(source_type) = result.source_type {
        append_rendered_line(
            rendered,
            format_args!(
                "- source_type: {}",
                render_root_skill_install_source_type(source_type)
            ),
        );
    }
    if let Some(source_locator) = result.source_locator.as_deref() {
        append_rendered_line(rendered, format_args!("- source: {}", source_locator));
    }
    append_rendered_line(rendered, format_args!("- message: {}", result.message));
}

/// Append one successful ROOT update result to the update-all command summary.
/// 将单个成功的 ROOT 更新结果追加到全量更新命令摘要。
fn append_root_skill_update_result(rendered: &mut String, result: &SkillApplyResult) {
    append_blank_rendered_line(rendered);
    append_rendered_line(rendered, format_args!("## {}", result.skill_id));
    append_root_skill_apply_fields(rendered, result);
}

/// Append one failed ROOT update result to the update-all command summary.
/// 将单个失败的 ROOT 更新结果追加到全量更新命令摘要。
fn append_root_skill_update_error(
    rendered: &mut String,
    skill_id: &str,
    error: &dyn std::error::Error,
) {
    append_blank_rendered_line(rendered);
    append_rendered_line(rendered, format_args!("## {}", skill_id));
    append_rendered_line(rendered, format_args!("- skill_id: {}", skill_id));
    append_rendered_line(rendered, format_args!("- status: failed"));
    append_rendered_line(rendered, format_args!("- message: {}", error));
}

/// Render one skill install source type as a stable CLI string.
/// 将单个技能安装来源类型渲染为稳定的 CLI 字符串。
fn render_root_skill_install_source_type(source_type: SkillInstallSourceType) -> &'static str {
    match source_type {
        SkillInstallSourceType::Github => "github",
        SkillInstallSourceType::OfficialHub => "official_hub",
        SkillInstallSourceType::Url => "url",
        SkillInstallSourceType::PrivateUrlManifest => "private_url_manifest",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Render ROOT apply results with optional source fields preserved.
    /// 验证 ROOT 应用结果渲染会保留可选来源字段。
    #[test]
    fn render_root_skill_apply_result_includes_root_layer_and_source_fields() {
        // Use a complete result so every apply field is asserted.
        // 使用完整结果以断言每个应用结果字段。
        let result = SkillApplyResult {
            skill_id: "demo-skill".to_string(),
            status: "installed".to_string(),
            message: "installed successfully".to_string(),
            version: Some("1.2.3".to_string()),
            source_type: Some(SkillInstallSourceType::PrivateUrlManifest),
            source_locator: Some("https://example.test/source.yaml".to_string()),
        };

        // Render the install path without starting ROOT runtime services.
        // 在不启动 ROOT 运行时服务的情况下渲染安装路径。
        let rendered = render_root_skill_apply_result("install", &result);

        assert_eq!(
            rendered,
            concat!(
                "# root-skill-manager install\n",
                "- layer: ROOT\n",
                "- skill_id: demo-skill\n",
                "- status: installed\n",
                "- version: 1.2.3\n",
                "- source_type: private_url_manifest\n",
                "- source: https://example.test/source.yaml\n",
                "- message: installed successfully\n"
            )
        );
    }

    /// Render ROOT update errors as isolated failed skill sections.
    /// 验证 ROOT 更新错误会渲染为独立的失败技能段落。
    #[test]
    fn append_root_skill_update_error_renders_failed_section() {
        // Use a concrete error value to exercise the dyn Error rendering path.
        // 使用具体错误值覆盖 dyn Error 渲染路径。
        let error = std::io::Error::other("network denied");
        // Start from the command header because update-all appends per-skill sections.
        // 从命令标题开始，因为 update-all 会追加逐技能段落。
        let mut rendered = String::from("# root-skill-manager update-all\n");

        append_root_skill_update_error(&mut rendered, "broken-skill", &error);

        assert_eq!(
            rendered,
            concat!(
                "# root-skill-manager update-all\n",
                "\n",
                "## broken-skill\n",
                "- skill_id: broken-skill\n",
                "- status: failed\n",
                "- message: network denied\n"
            )
        );
    }
}
