use std::collections::HashMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};

use super::HostRenderOptions;
use crate::support::{hosted_application_root_from_executable, luaskills_runtime_root};

/// Shared runtime roots used by overflow template discovery so hosted and isolated runtimes read the same template set.
/// 供超限模板发现链使用的共享运行根信息，确保托管运行与隔离运行读取同一套模板。
static TOOL_RESULT_TEMPLATE_RUNTIME: OnceLock<RwLock<ToolResultTemplateRuntime>> = OnceLock::new();

/// Runtime template discovery roots used by host overflow rendering.
/// 宿主超限渲染使用的模板发现根集合。
#[derive(Debug, Clone, Default)]
struct ToolResultTemplateRuntime {
    /// Ordered skill-root directories from highest priority to lowest priority.
    /// 按从高到低优先级排列的技能根目录列表。
    skill_roots: Vec<PathBuf>,
    /// Shared resources root used for fallback templates.
    /// 用于兜底模板的共享资源根目录。
    resources_root: Option<PathBuf>,
}

/// Return the shared runtime template discovery store.
/// 返回共享的运行时模板发现存储。
fn tool_result_template_runtime() -> &'static RwLock<ToolResultTemplateRuntime> {
    TOOL_RESULT_TEMPLATE_RUNTIME.get_or_init(|| RwLock::new(ToolResultTemplateRuntime::default()))
}

/// Initialize the runtime template discovery roots so overflow templates follow the selected runtime root and skill-root chain.
/// 初始化运行时模板发现根，使超限模板跟随当前选中的运行根与技能根链。
pub fn initialize_tool_result_template_roots(
    skill_roots: &[PathBuf],
    resources_root: Option<&Path>,
) -> Result<(), String> {
    let mut guard = tool_result_template_runtime()
        .write()
        .map_err(|_| "tool result template runtime lock poisoned".to_string())?;
    *guard = ToolResultTemplateRuntime {
        skill_roots: skill_roots.to_vec(),
        resources_root: resources_root.map(Path::to_path_buf),
    };
    Ok(())
}

/// Locate the skill root according to the current runtime layout, preferring the hosted runtime directory and then the repository layout.
/// 根据当前运行形态定位技能根目录；优先使用宿主运行目录，其次回退到仓库目录。
fn resolve_runtime_skills_root() -> Result<Option<PathBuf>, String> {
    let exe_path = std::env::current_exe().map_err(|error| {
        format!(
            "failed to resolve current executable while resolving template skills root: {error}"
        )
    })?;
    let current_dir = std::env::current_dir().map_err(|error| {
        format!("failed to resolve current directory while resolving template skills root: {error}")
    })?;
    resolve_runtime_skills_root_from_paths(&current_dir, &exe_path)
}

/// Resolve the implicit fallback skills root from the current directory and executable path.
/// 基于当前工作目录与可执行文件路径解析隐式回退技能根目录。
pub(super) fn resolve_runtime_skills_root_from_paths(
    current_dir: &Path,
    exe_path: &Path,
) -> Result<Option<PathBuf>, String> {
    // Hosted template skills live under `<application_root>/lua_runtime/skills`.
    // 宿主模板技能位于 `<application_root>/lua_runtime/skills`。
    if let Some(parent) = hosted_application_root_from_executable(exe_path) {
        let hosted_root = luaskills_runtime_root(parent).join("skills");
        if optional_template_directory_present(&hosted_root, "hosted template skills root")? {
            return Ok(Some(hosted_root));
        }
    }

    let repository_root = current_dir
        .join("output")
        .join("lua_runtime")
        .join("skills");
    if optional_template_directory_present(&repository_root, "repository template skills root")? {
        return Ok(Some(repository_root));
    }

    Ok(None)
}

/// Resolve the ordered skill-root chain used by template discovery.
/// 解析模板发现链使用的有序技能根目录集合。
fn resolve_runtime_skill_roots(render_options: &HostRenderOptions) -> Result<Vec<PathBuf>, String> {
    if !render_options.template_skill_roots.is_empty() {
        return Ok(render_options.template_skill_roots.clone());
    }
    let guard = tool_result_template_runtime()
        .read()
        .map_err(|_| "tool result template runtime lock poisoned".to_string())?;
    if !guard.skill_roots.is_empty() {
        return Ok(guard.skill_roots.clone());
    }

    Ok(resolve_runtime_skills_root()?.into_iter().collect())
}

/// Locate the shared resources root according to the current runtime layout, preferring the hosted runtime directory and then the repository layout.
/// 根据当前运行形态定位共享资源根目录；优先使用宿主运行目录，其次回退到仓库目录。
fn resolve_runtime_resources_root(
    render_options: &HostRenderOptions,
) -> Result<Option<PathBuf>, String> {
    if let Some(resources_root) = &render_options.template_resources_root {
        return Ok(Some(resources_root.clone()));
    }
    let guard = tool_result_template_runtime()
        .read()
        .map_err(|_| "tool result template runtime lock poisoned".to_string())?;
    if let Some(resources_root) = &guard.resources_root {
        return Ok(Some(resources_root.clone()));
    }

    let exe_path = std::env::current_exe().map_err(|error| {
        format!(
            "failed to resolve current executable while resolving template resources root: {error}"
        )
    })?;
    let current_dir = std::env::current_dir().map_err(|error| {
        format!(
            "failed to resolve current directory while resolving template resources root: {error}"
        )
    })?;
    resolve_runtime_resources_root_from_paths(&current_dir, &exe_path)
}

/// Resolve the implicit fallback resources root from the current directory and executable path.
/// 基于当前工作目录与可执行文件路径解析隐式回退共享资源根目录。
pub(super) fn resolve_runtime_resources_root_from_paths(
    current_dir: &Path,
    exe_path: &Path,
) -> Result<Option<PathBuf>, String> {
    // Hosted shared resources live under `<application_root>/lua_runtime/resources`.
    // 宿主共享资源位于 `<application_root>/lua_runtime/resources`。
    if let Some(parent) = hosted_application_root_from_executable(exe_path) {
        let hosted_root = luaskills_runtime_root(parent).join("resources");
        if optional_template_directory_present(&hosted_root, "hosted template resources root")? {
            return Ok(Some(hosted_root));
        }
    }

    let repository_root = current_dir.join("resources");
    if optional_template_directory_present(&repository_root, "repository template resources root")?
    {
        return Ok(Some(repository_root));
    }

    Ok(None)
}

/// Return whether one optional template directory exists and reject non-directory shapes.
/// 返回一个可选模板目录是否存在，并拒绝非目录形态。
/// Parameters: `path` is the candidate template directory path.
/// 参数：`path` 是候选模板目录路径。
/// Parameters: `path_label` names the discovery source in diagnostics.
/// 参数：`path_label` 用于在诊断中标识发现来源。
/// Returns `true` when present, `false` when absent, or an inspection/shape error.
/// 目录存在时返回 `true`，缺失时返回 `false`，否则返回检查/形态错误。
fn optional_template_directory_present(path: &Path, path_label: &str) -> Result<bool, String> {
    match std::fs::metadata(path) {
        Ok(metadata) => {
            if !metadata.is_dir() {
                return Err(format!(
                    "{} is not a directory: {}",
                    path_label,
                    path.display()
                ));
            }
            Ok(true)
        }
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!(
            "failed to inspect {} {}: {}",
            path_label,
            path.display(),
            error
        )),
    }
}

/// Load template text with skill-local templates taking priority over shared fallback templates.
/// 按“skill 本地模板优先，公共模板兜底”的顺序查找模板文本。
/// Parameters: `skill_name` is the optional owning skill name for skill-local templates.
/// 参数：`skill_name` 是用于查找 skill 本地模板的可选所属 skill 名称。
/// Parameters: `template_name` is the overflow template file name to locate.
/// 参数：`template_name` 是需要定位的超限模板文件名。
/// Parameters: `render_options` carries per-call template roots and shared resources root.
/// 参数：`render_options` 携带单次调用模板根与共享资源根。
/// Returns the template text, `None` when absent, or a discovery/read error.
/// 返回模板文本、缺失时的 `None`，或发现/读取错误。
pub(super) fn load_template_text(
    skill_name: Option<&str>,
    template_name: &str,
    render_options: &HostRenderOptions,
) -> Result<Option<String>, String> {
    let skill_roots = resolve_runtime_skill_roots(render_options)?;
    let resource_root = resolve_runtime_resources_root(render_options)?;
    let mut candidates = Vec::new();

    if let Some(skill_name) = skill_name {
        for skill_root in skill_roots {
            candidates.push(
                skill_root
                    .join(skill_name)
                    .join("overflow_templates")
                    .join(template_name),
            );
        }
    }
    if let Some(resource_root) = resource_root {
        candidates.push(resource_root.join("overflow_templates").join(template_name));
    }

    for path in candidates {
        if let Some(text) = read_optional_template_file(&path)? {
            return Ok(Some(text));
        }
    }

    Ok(None)
}

/// Read one optional overflow template file without hiding metadata or shape errors.
/// 读取一个可选超限模板文件，且不隐藏元数据或形态错误。
/// Parameters: `path` is the candidate overflow template path.
/// 参数：`path` 是候选超限模板路径。
/// Returns the template text when present, `None` when absent, or an inspection/read error.
/// 模板存在时返回文本，缺失时返回 `None`，否则返回检查/读取错误。
fn read_optional_template_file(path: &Path) -> Result<Option<String>, String> {
    match std::fs::metadata(path) {
        Ok(metadata) => {
            if !metadata.is_file() {
                return Err(format!(
                    "overflow template path is not a file: {}",
                    path.display()
                ));
            }
            fs::read_to_string(path).map(Some).map_err(|error| {
                format!(
                    "failed to read overflow template {}: {}",
                    path.display(),
                    error
                )
            })
        }
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!(
            "failed to inspect overflow template {}: {}",
            path.display(),
            error
        )),
    }
}

/// Perform simple template variable substitution; unknown variables are replaced with an empty string.
/// 执行简单的模板变量替换；未命中的变量替换为空字符串。
pub(super) fn render_template_text(
    template_text: &str,
    context: &HashMap<&'static str, String>,
) -> String {
    let mut rendered = template_text.to_string();
    for (key, value) in context {
        rendered = rendered.replace(&format!("{{{{{}}}}}", key), value);
    }
    rendered
}

/// Render the built-in default page template; used only when no external template exists.
/// 渲染分页模式的默认模板内容；仅在外部模板缺失时使用。
pub(super) fn render_page_default(context: &HashMap<&'static str, String>) -> String {
    render_template_text(
        "# LARGE RESULT POINTER\n\n## Raw File\n- raw_file: {{raw_file}}\n- format: {{format}}\n- total_bytes: {{total_bytes}}\n- total_lines: {{total_lines}}\n- safe_inline_limit_bytes: {{safe_inline_limit_bytes}}\n- safe_inline_limit_lines: {{safe_inline_limit_lines}}\n- safe_read_limit_bytes: {{safe_read_limit_bytes}}\n- safe_read_limit_lines: {{safe_read_limit_lines}}\n- chunk_count: {{chunk_count}}\n\n{{host_safe_read_chunks_section}}\n\n{{read_strategy_section}}",
        context,
    )
}
