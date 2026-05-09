use crate::config::{Config, SkillRootConfigEntry};
use luaskills::RuntimeSkillRoot;
use std::collections::HashSet;
use std::path::PathBuf;
/// Resolve the unified skill-config file path strictly from the runtime root using the fixed product layout.
/// 严格基于运行根与固定产品目录结构解析统一 Skill 配置文件路径。
pub fn resolve_skill_config_file_path(runtime_root: &std::path::Path) -> Result<PathBuf, String> {
    let resolved_path = runtime_root.join("configs").join("skill_config.json");

    if resolved_path.exists() && !resolved_path.is_file() {
        return Err(format!(
            "runtime skill config path is not a file: {}",
            resolved_path.display()
        ));
    }

    Ok(resolved_path)
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

/// Resolve the runtime root directory according to host configuration first and fallback layouts second.
/// 优先按宿主配置、其次按回退布局解析运行根目录。
pub fn resolve_runtime_root_from_config(config: &Config) -> Result<Option<PathBuf>, String> {
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
        if !normalized_root.exists() {
            return Err(format!(
                "configured runtime_root does not exist: {}",
                normalized_root.display()
            ));
        }
        if !normalized_root.is_dir() {
            return Err(format!(
                "configured runtime_root is not a directory: {}",
                normalized_root.display()
            ));
        }
        return Ok(Some(normalized_root));
    }

    let Some(exe_path) = std::env::current_exe().ok() else {
        return Ok(None);
    };
    let Some(current_dir) = std::env::current_dir().ok() else {
        return Ok(None);
    };
    Ok(resolve_implicit_runtime_root_from_paths(
        &current_dir,
        &exe_path,
    ))
}

/// Resolve one implicit runtime root from the current directory and executable path fallback chain.
/// 基于当前工作目录与可执行文件路径的回退链解析一份隐式运行根。
pub(super) fn resolve_implicit_runtime_root_from_paths(
    current_dir: &std::path::Path,
    exe_path: &std::path::Path,
) -> Option<PathBuf> {
    let exe_dir = exe_path.parent()?;
    let exe_parent = exe_dir.parent().unwrap_or(exe_dir);
    let hosted_root = exe_parent.to_path_buf();
    let hosted_skills_dir = hosted_root.join("skills");
    let hosted_configs_dir = hosted_root.join("configs");
    if (hosted_skills_dir.exists() && hosted_skills_dir.is_dir())
        || (hosted_configs_dir.exists() && hosted_configs_dir.is_dir())
    {
        return Some(hosted_root);
    }

    let repository_root = current_dir.join("runtime");
    if repository_root.exists() && repository_root.is_dir() {
        return Some(repository_root);
    }

    None
}

/// Resolve the ordered formal skill roots from host configuration and runtime layout.
/// 从宿主配置与运行时布局解析默认环境使用的有序正式技能根目录列表。
pub fn resolve_skill_roots_from_config(config: &Config) -> Result<Vec<RuntimeSkillRoot>, String> {
    let mut ordered_roots = Vec::new();
    let mut seen_roots = HashSet::new();
    let mut seen_root_names = HashSet::new();
    let mut synthesized_index = 1usize;
    let config_base_dir = resolve_config_base_dir(config);
    let runtime_root = resolve_runtime_root_from_config(config)?;

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
        let normalized_path = normalize_skill_root_key(&normalized_storage_path);
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
        for (index, value) in configured_roots.iter().enumerate() {
            match value {
                SkillRootConfigEntry::Named(named) => {
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
                SkillRootConfigEntry::Path(path) => {
                    let trimmed = path.trim();
                    if trimmed.is_empty() {
                        return Err(format!("skill_roots[{}] path must not be empty", index));
                    }
                    let generated = synthesized_skill_root_name(synthesized_index).ok_or_else(|| {
                        format!(
                            "skill_roots[{}] cannot be mapped to a formal layer; use named ROOT, PROJECT, or USER entries",
                            index
                        )
                    })?;
                    synthesized_index += 1;
                    push_unique_root(generated, resolve_configured_path(trimmed))?;
                }
            }
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
        if !root.skills_dir.exists() {
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

/// Return the formal layer name represented by one legacy path-only skill-root slot.
/// 返回旧式纯路径技能根槽位对应的正式层级名称。
fn synthesized_skill_root_name(index: usize) -> Option<String> {
    match index {
        1 => Some("ROOT".to_string()),
        2 => Some("PROJECT".to_string()),
        3 => Some("USER".to_string()),
        _ => None,
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
fn sort_formal_skill_roots(skill_roots: &mut [RuntimeSkillRoot]) -> Result<(), String> {
    skill_roots.sort_by_key(|root| formal_skill_root_rank(&root.name).unwrap_or(usize::MAX));
    for root in skill_roots {
        formal_skill_root_rank(&root.name)?;
    }
    Ok(())
}

/// Validate one skill root path according to strict or implicit runtime-root rules.
/// 按严格模式或隐式根规则校验单个技能根路径是否合法。
fn validate_skill_root_directory(
    root: &RuntimeSkillRoot,
    strict_missing: bool,
) -> Result<(), String> {
    if !root.skills_dir.exists() {
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

    if !root.skills_dir.is_dir() {
        return Err(format!(
            "skill root '{}' is not a directory: {}",
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
    Ok(std::fs::canonicalize(&absolute_path).unwrap_or(absolute_path))
}

pub fn normalize_skill_root_key(path: &std::path::Path) -> String {
    let normalized_path = normalize_skill_root_path(path).unwrap_or_else(|_| path.to_path_buf());
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
        let normalized_parent = normalize_skill_root_key(&parent);
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

/// Resolve the Lua resources directory according to the current MCP host layout.
/// 按当前 MCP 宿主布局解析 Lua 资源目录。
pub(super) fn resolve_runtime_resources_dir(
    runtime_root: &std::path::Path,
) -> Result<Option<PathBuf>, String> {
    let runtime_resources_dir = runtime_root.join("resources");
    if runtime_resources_dir.exists() {
        if !runtime_resources_dir.is_dir() {
            return Err(format!(
                "runtime resources path is not a directory: {}",
                runtime_resources_dir.display()
            ));
        }
        return Ok(Some(runtime_resources_dir));
    }

    Ok(None)
}

/// Resolve the host-provided tool root and reject file-shaped runtime bin/tools paths early.
/// 解析宿主提供工具根目录，并在 runtime bin/tools 为文件形态时尽早拒绝。
pub(super) fn resolve_host_provided_tool_root(
    runtime_root: &std::path::Path,
) -> Result<Option<PathBuf>, String> {
    let tool_root = runtime_root.join("bin").join("tools");
    if tool_root.exists() && !tool_root.is_dir() {
        return Err(format!(
            "host-provided tool root is not a directory: {}",
            tool_root.display()
        ));
    }
    Ok(Some(tool_root))
}

/// Resolve the generic host-provided native library root used by Lua C modules and other runtime FFI payloads.
/// 解析 Lua C 模块及其他运行时 FFI 载荷使用的通用宿主原生库根目录。
pub(super) fn resolve_host_ffi_root(
    runtime_root: &std::path::Path,
) -> Result<Option<PathBuf>, String> {
    let ffi_root = runtime_root.join("libs");
    if ffi_root.exists() {
        if !ffi_root.is_dir() {
            return Err(format!(
                "runtime ffi root is not a directory: {}",
                ffi_root.display()
            ));
        }
        return Ok(Some(ffi_root));
    }
    Ok(None)
}

/// Resolve the host-managed lua_packages directory according to runtime output first and repository output second.
/// 先按运行时输出目录、再按仓库输出目录解析宿主管理的 lua_packages 目录。
pub(super) fn resolve_lua_packages_dir(
    runtime_root: &std::path::Path,
) -> Result<Option<PathBuf>, String> {
    let runtime_path = runtime_root.join("lua_packages");
    if runtime_path.exists() {
        if !runtime_path.is_dir() {
            return Err(format!(
                "runtime lua_packages path is not a directory: {}",
                runtime_path.display()
            ));
        }
        return Ok(Some(runtime_path));
    }
    Ok(None)
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
