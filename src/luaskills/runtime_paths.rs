use crate::config::Config;
use crate::support::{hosted_application_root_from_executable, luaskills_runtime_root};
use luaskills::RuntimeSkillRoot;
use std::collections::HashSet;
use std::io::ErrorKind;
use std::path::PathBuf;

/// Normalize one path before it is exposed to Lua package search templates.
/// 在路径暴露给 Lua 包搜索模板前对其进行规范化。
pub(super) fn normalize_lua_visible_path(path: PathBuf) -> PathBuf {
    // Windows verbatim prefixes contain `?`, which Lua treats as the module-name placeholder inside package.path and package.cpath.
    // Windows verbatim 前缀包含 `?`，Lua 会在 package.path 与 package.cpath 中把它当作模块名占位符替换。
    #[cfg(windows)]
    {
        // Rendered path text is used only to detect and strip Windows verbatim spelling emitted by canonicalization.
        // 渲染后的路径文本仅用于检测并去除 canonicalize 产生的 Windows verbatim 写法。
        let rendered = path.to_string_lossy();
        if let Some(stripped) = rendered.strip_prefix(r"\\?\UNC\") {
            return PathBuf::from(format!(r"\\{}", stripped));
        }
        if let Some(stripped) = rendered.strip_prefix(r"\\?\") {
            return PathBuf::from(stripped);
        }
    }

    path
}

/// Resolve the absolute user-level root used by LuaSkills package configuration stores.
/// 解析 LuaSkills 技能包配置存储使用的用户级绝对根目录。
/// Parameter `config` supplies an optional explicit absolute override.
/// 参数：`config` 提供可选的显式绝对路径覆盖值。
/// Returns a normalized absolute directory path, or an explicit path/configuration error.
/// 返回规范化后的绝对目录路径，或明确的路径/配置错误。
pub fn resolve_skill_config_root_from_config(config: &Config) -> Result<PathBuf, String> {
    let resolved_root = match config.skill_config_root.as_deref() {
        Some(value) => {
            let trimmed_value = value.trim();
            if trimmed_value.is_empty() {
                return Err("skill_config_root must not be empty when configured".to_string());
            }
            PathBuf::from(trimmed_value)
        }
        None => default_skill_config_root().ok_or_else(|| {
            "failed to resolve skill_config_root because the current account home directory is unavailable"
                .to_string()
        })?,
    };
    if !resolved_root.is_absolute() {
        return Err(format!(
            "skill_config_root must be an absolute directory path: {}",
            resolved_root.display()
        ));
    }
    optional_directory_present(&resolved_root, "skill config root")?;

    match std::fs::canonicalize(&resolved_root) {
        Ok(path) => Ok(normalize_lua_visible_path(path)),
        Err(error) if error.kind() == ErrorKind::NotFound => {
            Ok(normalize_lua_visible_path(resolved_root))
        }
        Err(error) => Err(format!(
            "failed to canonicalize skill config root '{}': {}",
            resolved_root.display(),
            error
        )),
    }
}

/// Resolve the stable base directory used for relative host configuration paths.
/// 解析宿主配置中相对路径应当依附的稳定基准目录。
fn resolve_config_base_dir(config: &Config) -> Option<PathBuf> {
    config
        .loaded_config_path
        .as_ref()
        .map(PathBuf::from)
        .and_then(|path| {
            let config_dir = path.parent()?.to_path_buf();
            let use_parent_of_configs = config_dir
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.eq_ignore_ascii_case("configs"))
                .unwrap_or(false);
            if use_parent_of_configs {
                config_dir.parent().map(std::path::Path::to_path_buf)
            } else {
                Some(config_dir)
            }
        })
}

/// Resolve the application root directory according to host configuration first and fallback layouts second.
/// 优先按宿主配置、其次按回退布局解析应用根目录。
/// Parameters: `config` contains the optional application-root override and loaded config path.
/// 参数：`config` 包含可选的应用根覆盖值与已加载配置路径。
/// Returns the canonical application root, `None` when no layout exists, or an explicit discovery error.
/// 返回规范应用根、布局不存在时的 `None`，或显式发现错误。
pub fn resolve_application_root_from_config(config: &Config) -> Result<Option<PathBuf>, String> {
    if let Some(configured_root) = config
        .runtime_root
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        let candidate_root = PathBuf::from(configured_root);
        let normalized_root = if candidate_root.is_absolute() {
            candidate_root
        } else if let Some(config_base_dir) = resolve_config_base_dir(config) {
            config_base_dir.join(candidate_root)
        } else {
            std::env::current_dir()
                .map_err(|error| {
                    format!(
                        "failed to resolve current directory while normalizing runtime_root '{}': {}",
                        configured_root, error
                    )
                })?
                .join(candidate_root)
        };
        if !optional_directory_present(&normalized_root, "configured application runtime_root")? {
            return Err(format!(
                "configured application runtime_root does not exist: {}",
                normalized_root.display()
            ));
        }
        return Ok(Some(canonicalize_lua_visible_directory(
            &normalized_root,
            "configured application runtime_root",
        )?));
    }

    let exe_path = std::env::current_exe().map_err(|error| {
        format!("failed to resolve current executable while resolving runtime_root: {error}")
    })?;
    let current_dir = std::env::current_dir().map_err(|error| {
        format!("failed to resolve current directory while resolving runtime_root: {error}")
    })?;
    resolve_implicit_application_root_from_paths(&current_dir, &exe_path)
}

/// Resolve one implicit application root from the current directory and executable path fallback chain.
/// 基于当前工作目录与可执行文件路径的回退链解析一份隐式应用根。
pub(super) fn resolve_implicit_application_root_from_paths(
    current_dir: &std::path::Path,
    exe_path: &std::path::Path,
) -> Result<Option<PathBuf>, String> {
    // Hosted binaries are expected below one application binary directory, so the executable grandparent is the application root candidate.
    // 宿主二进制应位于应用二进制目录下，因此可执行文件祖父目录是应用根候选位置。
    if let Some(hosted_root) = hosted_application_root_from_executable(exe_path) {
        // The application root is authoritative only when it owns either host configs or the isolated LuaSkills package.
        // 仅当应用根拥有宿主配置或隔离的 LuaSkills 包时，才将其视为权威位置。
        let hosted_root = hosted_root.to_path_buf();
        let hosted_lua_runtime_dir = luaskills_runtime_root(&hosted_root);
        let hosted_configs_dir = hosted_root.join("configs");
        if optional_directory_present(&hosted_lua_runtime_dir, "hosted LuaSkills runtime path")?
            || optional_directory_present(&hosted_configs_dir, "hosted application configs path")?
        {
            return Ok(Some(canonicalize_lua_visible_directory(
                &hosted_root,
                "hosted application runtime_root",
            )?));
        }
    }

    let repository_root = current_dir.join("runtime");
    if optional_directory_present(&repository_root, "implicit repository application root")? {
        return Ok(Some(canonicalize_lua_visible_directory(
            &repository_root,
            "implicit repository application root",
        )?));
    }

    Ok(None)
}

/// Resolve the isolated LuaSkills root owned by the selected application root.
/// 解析由已选应用根拥有的隔离 LuaSkills 根目录。
/// Parameters: `config` contains application-root selection and relative-path context.
/// 参数：`config` 包含应用根选择信息与相对路径上下文。
/// Returns the canonical `<application_root>/lua_runtime` directory, `None` when no application layout exists, or an error when the child is missing or invalid.
/// 返回规范的 `<application_root>/lua_runtime` 目录、应用布局不存在时的 `None`，或子目录缺失/无效时的错误。
pub fn resolve_luaskills_runtime_root_from_config(
    config: &Config,
) -> Result<Option<PathBuf>, String> {
    // ApplicationRoot remains the host-facing root for binaries, configs, and logs.
    // ApplicationRoot 仍是承载二进制、配置与日志的宿主根目录。
    let Some(application_root) = resolve_application_root_from_config(config)? else {
        return Ok(None);
    };
    // RuntimeRoot is the only directory passed to LuaSkills.
    // RuntimeRoot 是唯一传递给 LuaSkills 的目录。
    let runtime_root = luaskills_runtime_root(&application_root);
    if !optional_directory_present(&runtime_root, "LuaSkills runtime_root")? {
        return Err(format!(
            "LuaSkills runtime_root does not exist: {}",
            runtime_root.display()
        ));
    }
    Ok(Some(canonicalize_lua_visible_directory(
        &runtime_root,
        "LuaSkills runtime_root",
    )?))
}

/// Resolve the ordered formal skill roots from host configuration and runtime layout.
/// 从宿主配置与运行时布局解析默认环境使用的有序正式技能根目录列表。
pub fn resolve_skill_roots_from_config(config: &Config) -> Result<Vec<RuntimeSkillRoot>, String> {
    let mut ordered_roots = Vec::new();
    let mut seen_roots = HashSet::new();
    let mut seen_root_names = HashSet::new();
    let config_base_dir = resolve_config_base_dir(config);
    let runtime_root = resolve_luaskills_runtime_root_from_config(config)?;

    let resolve_configured_path = |raw_path: &str| -> PathBuf {
        let candidate_path = PathBuf::from(raw_path);
        if candidate_path.is_absolute() {
            candidate_path
        } else if let Some(base_dir) = &config_base_dir {
            base_dir.join(candidate_path)
        } else {
            candidate_path
        }
    };

    let mut push_unique_root = |name: String, path: PathBuf| -> Result<(), String> {
        let normalized_name = normalize_formal_skill_root_name(&name)?;
        if !seen_root_names.insert(normalized_name.clone()) {
            return Err(format!(
                "duplicate skill root name '{}' is not allowed",
                normalized_name
            ));
        }
        let normalized_storage_path = normalize_skill_root_path(&path)?;
        let normalized_path = render_skill_root_key(&normalized_storage_path);
        if !seen_roots.insert(normalized_path) {
            return Err(format!(
                "duplicate skill root '{}' at {} is not allowed",
                name,
                normalized_storage_path.display()
            ));
        }
        ordered_roots.push(RuntimeSkillRoot {
            name: normalized_name,
            skills_dir: normalized_storage_path,
        });
        Ok(())
    };

    if let Some(configured_roots) = &config.skill_roots {
        for (index, named) in configured_roots.iter().enumerate() {
            let name = named.name.trim();
            let path = named.path.trim();
            if name.is_empty() || path.is_empty() {
                return Err(format!(
                    "skill_roots[{}] must declare non-empty name and path",
                    index
                ));
            }
            push_unique_root(name.to_string(), resolve_configured_path(path))?;
        }
        sort_formal_skill_roots(&mut ordered_roots)?;
    } else if let Some(runtime_root) = runtime_root.as_ref() {
        push_unique_root("ROOT".to_string(), runtime_root.join("skills"))?;
        if let Some(default_user_root) = default_user_skill_root() {
            push_unique_root("USER".to_string(), default_user_root)?;
        }
    }

    validate_unique_skill_root_spaces(&ordered_roots)?;
    if config.skill_roots.is_some() {
        for root in &ordered_roots {
            validate_skill_root_directory(root, true)?;
        }
        return Ok(ordered_roots);
    }
    let mut implicit_roots = Vec::new();
    for root in ordered_roots {
        if !optional_directory_present(
            &root.skills_dir,
            &format!("implicit skill root '{}'", root.name),
        )? {
            continue;
        }
        validate_skill_root_directory(&root, false)?;
        implicit_roots.push(root);
    }
    Ok(implicit_roots)
}

/// Normalize one configured skill-root label into the formal ROOT, PROJECT, or USER namespace.
/// 将单个配置技能根标签规范化到正式的 ROOT、PROJECT 或 USER 命名空间。
fn normalize_formal_skill_root_name(name: &str) -> Result<String, String> {
    let normalized_name = name.trim().to_ascii_uppercase();
    match normalized_name.as_str() {
        "ROOT" | "PROJECT" | "USER" => Ok(normalized_name),
        _ => Err(format!(
            "unsupported skill root name '{}'; expected ROOT, PROJECT, or USER",
            name.trim()
        )),
    }
}

/// Return the fixed priority rank for one formal skill-root label.
/// 返回单个正式技能根标签的固定优先级序号。
fn formal_skill_root_rank(name: &str) -> Result<usize, String> {
    match name.trim().to_ascii_uppercase().as_str() {
        "ROOT" => Ok(0),
        "PROJECT" => Ok(1),
        "USER" => Ok(2),
        _ => Err(format!(
            "unsupported skill root name '{}'; expected ROOT, PROJECT, or USER",
            name.trim()
        )),
    }
}

/// Sort formal skill roots into the runtime-required ROOT -> PROJECT -> USER order.
/// 将正式技能根排序为运行时要求的 ROOT -> PROJECT -> USER 顺序。
pub(super) fn sort_formal_skill_roots(skill_roots: &mut [RuntimeSkillRoot]) -> Result<(), String> {
    let mut ranked_roots = Vec::with_capacity(skill_roots.len());
    for root in skill_roots.iter() {
        let rank = formal_skill_root_rank(&root.name)?;
        ranked_roots.push((rank, root.clone()));
    }
    ranked_roots.sort_by_key(|(rank, _root)| *rank);
    for (target_root, (_rank, sorted_root)) in skill_roots.iter_mut().zip(ranked_roots) {
        *target_root = sorted_root;
    }
    Ok(())
}

/// Validate one skill root path according to strict or implicit runtime-root rules.
/// 按严格模式或隐式根规则校验单个技能根路径是否合法。
fn validate_skill_root_directory(
    root: &RuntimeSkillRoot,
    strict_missing: bool,
) -> Result<(), String> {
    if !optional_directory_present(&root.skills_dir, &format!("skill root '{}'", root.name))? {
        if strict_missing {
            return Err(format!(
                "configured skill root '{}' does not exist: {}",
                root.name,
                root.skills_dir.display()
            ));
        }
        return Err(format!(
            "implicit skill root '{}' does not exist: {}",
            root.name,
            root.skills_dir.display()
        ));
    }

    Ok(())
}

/// Normalize one skill-root path into a stable absolute path for runtime storage and validation.
/// 将单个技能根路径归一化为用于运行时存储与校验的稳定绝对路径。
pub fn normalize_skill_root_path(path: &std::path::Path) -> Result<PathBuf, String> {
    let absolute_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| {
                format!(
                    "failed to resolve current directory while normalizing skill root '{}': {}",
                    path.display(),
                    error
                )
            })?
            .join(path)
    };
    let normalized_path = match std::fs::canonicalize(&absolute_path) {
        Ok(path) => path,
        Err(error) if error.kind() == ErrorKind::NotFound => absolute_path,
        Err(error) => {
            return Err(format!(
                "failed to canonicalize skill root '{}': {}",
                absolute_path.display(),
                error
            ));
        }
    };
    Ok(normalize_lua_visible_path(normalized_path))
}

#[cfg(test)]
pub fn normalize_skill_root_key(path: &std::path::Path) -> String {
    let normalized_path = normalize_skill_root_path(path).unwrap_or_else(|_| path.to_path_buf());
    render_skill_root_key(&normalized_path)
}

/// Normalize one skill-root key while preserving path normalization errors for production callers.
/// 规范化一个技能根键，并为生产调用方保留路径规范化错误。
/// Parameters: `path` is the skill-root path that should be converted into a comparison key.
/// 参数：`path` 是需要转换为比较键的技能根路径。
/// Returns the normalized comparison key or a path normalization error.
/// 返回规范化后的比较键，或路径规范化错误。
pub fn try_normalize_skill_root_key(path: &std::path::Path) -> Result<String, String> {
    let normalized_path = normalize_skill_root_path(path)?;
    Ok(render_skill_root_key(&normalized_path))
}

/// Render one already-normalized skill-root path into a platform-stable comparison key.
/// 将一个已经规范化的技能根路径渲染为平台稳定的比较键。
/// Parameters: `normalized_path` is the normalized skill-root path to render.
/// 参数：`normalized_path` 是需要渲染的已规范化技能根路径。
/// Returns the stable string key used for duplicate detection and root replacement.
/// 返回用于重复检测与根替换的稳定字符串键。
fn render_skill_root_key(normalized_path: &std::path::Path) -> String {
    let rendered = normalized_path.to_string_lossy().replace('\\', "/");
    #[cfg(windows)]
    {
        rendered.to_ascii_lowercase()
    }
    #[cfg(not(windows))]
    {
        rendered
    }
}

/// Validate that every skill root maps to one unique sibling runtime space.
/// 校验每个技能根都映射到唯一的同级运行时空间。
pub fn validate_unique_skill_root_spaces(skill_roots: &[RuntimeSkillRoot]) -> Result<(), String> {
    let mut seen_space_parents = HashSet::new();
    let mut seen_root_names = HashSet::new();
    for root in skill_roots {
        let normalized_name = root.name.trim().to_string();
        if !seen_root_names.insert(normalized_name.clone()) {
            return Err(format!(
                "skill root name '{}' is duplicated in one runtime chain",
                normalized_name
            ));
        }
        let parent = root
            .skills_dir
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(|| root.skills_dir.clone());
        let normalized_parent = try_normalize_skill_root_key(&parent)?;
        if !seen_space_parents.insert(normalized_parent) {
            return Err(format!(
                "skill root '{}' at {} shares the same sibling runtime space with another root; each root must use a unique parent directory",
                root.name,
                root.skills_dir.display()
            ));
        }
    }
    Ok(())
}

/// Inspect one optional runtime path without hiding metadata errors.
/// 检查一个可选运行时路径，且不隐藏元数据错误。
/// Parameters: `path` is the runtime-managed file or directory path to inspect.
/// 参数：`path` 是需要检查的运行时托管文件或目录路径。
/// Parameters: `path_label` names the path kind in diagnostics.
/// 参数：`path_label` 用于在诊断中标识路径类型。
/// Returns metadata when present, `None` when absent, or an inspection error.
/// 路径存在时返回元数据，缺失时返回 `None`，否则返回检查错误。
fn optional_path_metadata(
    path: &std::path::Path,
    path_label: &str,
) -> Result<Option<std::fs::Metadata>, String> {
    match std::fs::metadata(path) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!(
            "failed to inspect {} {}: {}",
            path_label,
            path.display(),
            error
        )),
    }
}

/// Return whether one optional runtime directory exists and reject non-directory shapes.
/// 返回一个可选运行时目录是否存在，并拒绝非目录形态。
/// Parameters: `path` is the runtime-managed directory path to inspect.
/// 参数：`path` 是需要检查的运行时托管目录路径。
/// Parameters: `directory_label` names the directory kind in diagnostics.
/// 参数：`directory_label` 用于在诊断中标识目录类型。
/// Returns `true` when present, `false` when absent, or an inspection/shape error.
/// 目录存在时返回 `true`，缺失时返回 `false`，否则返回检查/形态错误。
fn optional_directory_present(
    path: &std::path::Path,
    directory_label: &str,
) -> Result<bool, String> {
    let Some(metadata) = optional_path_metadata(path, directory_label)? else {
        return Ok(false);
    };
    if !metadata.is_dir() {
        return Err(format!(
            "{} is not a directory: {}",
            directory_label,
            path.display()
        ));
    }
    Ok(true)
}

/// Canonicalize one known runtime directory and normalize it for Lua-visible path templates.
/// 规范化一个已确认存在的运行时目录，并转换为 Lua 可见路径模板可用的形式。
/// Parameters: `path` is the runtime directory path that has already passed metadata inspection.
/// 参数：`path` 是已经通过元数据检查的运行时目录路径。
/// Parameters: `path_label` names the directory kind in diagnostics.
/// 参数：`path_label` 用于在诊断中标识目录类型。
/// Returns the canonical Lua-visible directory path or a canonicalization error.
/// 返回规范化后的 Lua 可见目录路径，或规范化错误。
fn canonicalize_lua_visible_directory(
    path: &std::path::Path,
    path_label: &str,
) -> Result<PathBuf, String> {
    std::fs::canonicalize(path)
        .map(normalize_lua_visible_path)
        .map_err(|error| {
            format!(
                "failed to canonicalize {} {}: {}",
                path_label,
                path.display(),
                error
            )
        })
}

/// Resolve the current user's home directory when a default skill override root needs to be derived.
/// 在需要推导默认技能覆盖根目录时解析当前用户主目录。
fn home_dir() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var("USERPROFILE")
            .ok()
            .map(std::path::PathBuf::from)
    }

    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME").ok().map(std::path::PathBuf::from)
    }
}

/// Return the default USER layer skill root derived from the current home directory using the fixed agent-service path.
/// 返回基于当前用户主目录推导出的默认 USER 层技能根目录，固定使用 agent-service 路径。
pub fn default_user_skill_root() -> Option<std::path::PathBuf> {
    Some(
        home_dir()?
            .join(".vulcan")
            .join("agent-service")
            .join("skills"),
    )
}

/// Return the default user-level LuaSkills package configuration root.
/// 返回默认的用户级 LuaSkills 技能包配置根目录。
/// Returns `<home>/.vulcan/agent-service/config`, or `None` when the account home cannot be resolved.
/// 返回 `<home>/.vulcan/agent-service/config`；无法解析当前账户主目录时返回 `None`。
pub fn default_skill_config_root() -> Option<std::path::PathBuf> {
    Some(
        home_dir()?
            .join(".vulcan")
            .join("agent-service")
            .join("config"),
    )
}
