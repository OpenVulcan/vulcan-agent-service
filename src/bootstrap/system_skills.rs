//! Initialize configured system skills through the existing ROOT lifecycle API.
//! 通过现有 ROOT 生命周期接口初始化配置中的系统技能。

use super::runtime_init::{
    build_root_skill_cli_context, build_root_skill_manager_for_cli, ensure_root_skill_manager_root,
    select_root_skill_manager_root,
};
use crate::config::Config;
use crate::config::system_skills::{SystemSkillSpec, load_system_skills};
use crate::luaskills_adapter::{
    build_luaskills_cache_config, build_luaskills_engine_options,
    resolve_application_root_from_config, resolve_skill_roots_from_config,
};
use luaskills::{
    LuaVmPoolConfig, SkillInstallRequest, SkillInstallSourceType, SkillManagementAuthority,
    SkillManager,
};
use std::fs::{self, OpenOptions};
use std::io::ErrorKind;
use std::path::Path;

/// Create the minimal LuaSkills directory layout below the already selected application root.
/// 在已选应用根下创建最小 LuaSkills 目录布局。
/// `config` supplies authoritative paths; returns success or a path/creation error.
/// `config` 提供权威路径；返回成功或路径及创建错误。
pub(super) fn prepare_runtime_directory(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    // Existing configuration and release resources remain untouched during directory creation.
    // 创建目录时保留现有配置与发行资源。
    let application_root = resolve_application_root_from_config(config)?
        .ok_or("Cannot initialize LuaSkills without an application runtime root")?;
    for name in ["skills", "state", "temp"] {
        fs::create_dir_all(application_root.join("lua_runtime").join(name))?;
    }
    Ok(())
}

/// Prepare writable directories and install missing configured ROOT skills.
/// 准备可写目录，并安装配置中缺失的 ROOT 技能。
/// `config` selects the application layout; `explicit` allows init when automatic installation is off.
/// `config` 指定应用布局；`explicit` 允许显式 init 在自动安装关闭时执行。
/// Returns success after all enabled missing skills install, or an explicit initialization error.
/// 所有启用且缺失的技能安装完成后返回成功，否则返回明确的初始化错误。
pub(super) fn initialize_system_skills(
    config: &Config,
    explicit: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    // Resolve the same application root used by host configuration and normal skill loading.
    // 解析宿主配置与普通技能加载共同使用的应用根目录。
    let application_root = resolve_application_root_from_config(config)?
        .ok_or("Cannot initialize system skills without an application runtime root")?;
    // A supplied configuration is authoritative, including explicit false values and an empty list.
    // 显式提供的配置具备权威性，包括 false 值和空技能列表。
    let policy = load_system_skills(&application_root)?;
    // Create only runtime directories; base native resources remain owned by release packaging.
    // 仅创建运行目录；基础原生资源仍由发行打包流程提供。
    let runtime_root = application_root.join("lua_runtime");
    fs::create_dir_all(runtime_root.join("skills"))?;
    fs::create_dir_all(runtime_root.join("state"))?;
    if !explicit && !policy.auto_install {
        eprintln!("[system-skills] Automatic installation disabled by configuration");
        return Ok(());
    }
    if policy.skills.iter().all(|skill| !skill.enabled) {
        eprintln!("[system-skills] No enabled system skills to initialize");
        return Ok(());
    }

    // Resolve configured ROOT ownership before checking installed packages or writing files.
    // 在检查已安装包或写入文件前解析配置中的 ROOT 归属。
    let mut roots = resolve_skill_roots_from_config(config)?;
    ensure_root_skill_manager_root(Some(&runtime_root), &mut roots)?;
    // The target and manager share the same runtime options as actual lifecycle operations.
    // 安装目标与管理器和实际生命周期操作使用相同的运行选项。
    let target = select_root_skill_manager_root(&roots)?;
    // Lock the actual ROOT state so applications sharing a configured ROOT serialize installation.
    // 锁定实际 ROOT 状态目录，确保共享同一配置 ROOT 的应用串行执行安装。
    let state_root = target
        .skills_dir
        .parent()
        .ok_or("System skill ROOT must have a parent runtime directory")?
        .join("state");
    fs::create_dir_all(&state_root)?;
    // An OS-owned lock is released automatically even after a process crash.
    // 操作系统持有的锁会在进程崩溃后自动释放。
    let initialization_lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(state_root.join("system-skills-init.lock"))?;
    initialization_lock.lock()?;
    // Preserve non-default host cache settings across initialization and subsequent service startup.
    // 在初始化与后续服务启动之间保留宿主的非默认缓存设置。
    let options = build_luaskills_engine_options(
        config,
        LuaVmPoolConfig {
            min_size: 1,
            max_size: 1,
            idle_ttl_secs: 300,
        },
        build_luaskills_cache_config(
            config.tool_cache_max_entries,
            config.tool_cache_default_ttl_secs,
            config.tool_cache_max_ttl_secs,
        ),
    )?;
    // Read persisted lifecycle records through the upstream manager rather than duplicating its schema.
    // 通过上游管理器读取持久化生命周期记录，避免复制其结构定义。
    let manager = build_root_skill_manager_for_cli(&target, &options.host_options)?;
    // Only missing enabled entries reach the network-backed install stage.
    // 只有缺失且启用的条目会进入需要网络的安装阶段。
    let mut pending = Vec::new();
    for skill in &policy.skills {
        if !skill.enabled
            || options
                .host_options
                .ignored_skill_ids
                .iter()
                .any(|id| id.eq_ignore_ascii_case(&skill.name))
        {
            eprintln!("[system-skills] {}: disabled by configuration", skill.name);
        } else if manager.disabled_record(&skill.name)?.is_some() {
            eprintln!(
                "[system-skills] {}: disabled by persisted state",
                skill.name
            );
        } else if system_skill_is_installed(&target.skills_dir, &manager, skill)? {
            eprintln!("[system-skills] {}: already installed", skill.name);
        } else {
            pending.push(skill);
        }
    }
    if pending.is_empty() {
        return Ok(());
    }

    crate::luaskills_adapter::install_luaskills_log_callback();
    super::runtime_init::initialize_runtime_temp_root_from_config(config)?;
    super::runtime_preload::preload_runtime_mcp_configs(config)?;
    super::runtime_init::add_libs_to_path(config)?;

    // Construct one lifecycle context for the batch, outside Tokio to preserve blocking-library ownership.
    // 在 Tokio 外为整批创建一个生命周期上下文，保持阻塞库的所有权边界。
    let mut context = build_root_skill_cli_context(config)?;
    // Retain successful package transactions while reporting every failed package to the caller.
    // 保留已成功的包事务，同时向调用方报告所有失败的包。
    let mut failures = Vec::new();
    for skill in pending {
        eprintln!(
            "[system-skills] Installing {} from {}",
            skill.name, skill.github
        );
        // Bind the expected package identity before download and manifest validation.
        // 在下载与清单校验前绑定预期的包标识。
        let request = SkillInstallRequest {
            skill_id: Some(skill.name.clone()),
            source: Some(skill.github.clone()),
            source_type: SkillInstallSourceType::Github,
        };
        match context.engine.system_install_skill_in_root(
            &context.skill_roots,
            &context.target_root,
            SkillManagementAuthority::System,
            &request,
        ) {
            Ok(result) => eprintln!(
                "[system-skills] {}: {} ({})",
                result.skill_id, result.status, result.message
            ),
            Err(error) => {
                eprintln!("[system-skills] {}: failed: {error}", skill.name);
                failures.push(format!("{}: {error}", skill.name));
            }
        }
    }
    if !failures.is_empty() {
        return Err(format!(
            "System skill initialization failed: {}",
            failures.join("; ")
        )
        .into());
    }
    Ok(())
}

/// Check whether a configured skill already owns a complete package at the ROOT location.
/// 检查配置中的技能是否已在 ROOT 位置拥有完整的包。
/// `skills_root` is the selected ROOT directory, `manager` reads records, and `skill` supplies expected identity.
/// `skills_root` 是已选 ROOT 目录，`manager` 读取记录，`skill` 提供预期标识。
/// Returns false for missing packages and errors for conflicting sources or malformed filesystem shapes.
/// 包缺失时返回 false；来源冲突或文件系统形态异常时返回错误。
fn system_skill_is_installed(
    skills_root: &Path,
    manager: &SkillManager,
    skill: &SystemSkillSpec,
) -> Result<bool, String> {
    // The configured skill name is validated before it becomes a filesystem component.
    // 配置中的技能名在成为文件系统路径组件前已完成校验。
    let directory = skills_root.join(&skill.name);
    match fs::metadata(&directory) {
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(format!(
                "Failed to inspect {}: {error}",
                directory.display()
            ));
        }
        Ok(metadata) if !metadata.is_dir() => {
            return Err(format!(
                "System skill path is not a directory: {}",
                directory.display()
            ));
        }
        Ok(_) => {}
    }
    // Existing hand-managed directories are never overwritten by automatic initialization.
    // 自动初始化不会覆盖已经存在的手工管理目录。
    let manifest = directory.join("skill.yaml");
    match fs::metadata(&manifest) {
        Ok(metadata) if metadata.is_file() => {}
        Ok(_) => {
            return Err(format!(
                "System skill manifest is not a file: {}",
                manifest.display()
            ));
        }
        Err(error) => {
            return Err(format!(
                "Incomplete system skill {}: {error}; repair or remove its directory before init",
                directory.display()
            ));
        }
    }
    if let Some(record) = manager.install_record(&skill.name)? {
        if record.source.source_type != SkillInstallSourceType::Github
            || !record.source.locator.eq_ignore_ascii_case(&skill.github)
        {
            return Err(format!(
                "System skill {} source conflicts with configured GitHub repository {}",
                skill.name, skill.github
            ));
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests;
