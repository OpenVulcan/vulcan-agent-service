use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};

use super::HostRenderOptions;

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
fn resolve_runtime_skills_root() -> Option<PathBuf> {
    let exe_path = std::env::current_exe().ok()?;
    let current_dir = std::env::current_dir().ok()?;
    resolve_runtime_skills_root_from_paths(&current_dir, &exe_path)
}

/// Resolve the implicit fallback skills root from the current directory and executable path.
/// 基于当前工作目录与可执行文件路径解析隐式回退技能根目录。
pub(super) fn resolve_runtime_skills_root_from_paths(
    current_dir: &Path,
    exe_path: &Path,
) -> Option<PathBuf> {
    let exe_dir = exe_path.parent()?;
    let parent = exe_dir.parent().unwrap_or(exe_dir);
    let hosted_root = parent.join("skills");
    if hosted_root.exists() && hosted_root.is_dir() {
        return Some(hosted_root);
    }

    let repository_root = current_dir.join("runtime").join("skills");
    if repository_root.exists() && repository_root.is_dir() {
        return Some(repository_root);
    }

    None
}

/// Resolve the ordered skill-root chain used by template discovery.
/// 解析模板发现链使用的有序技能根目录集合。
fn resolve_runtime_skill_roots(render_options: &HostRenderOptions) -> Vec<PathBuf> {
    if !render_options.template_skill_roots.is_empty() {
        return render_options.template_skill_roots.clone();
    }
    if let Ok(guard) = tool_result_template_runtime().read() {
        if !guard.skill_roots.is_empty() {
            return guard.skill_roots.clone();
        }
    }

    resolve_runtime_skills_root().into_iter().collect()
}

/// Locate the shared resources root according to the current runtime layout, preferring the hosted runtime directory and then the repository layout.
/// 根据当前运行形态定位共享资源根目录；优先使用宿主运行目录，其次回退到仓库目录。
fn resolve_runtime_resources_root(render_options: &HostRenderOptions) -> Option<PathBuf> {
    if let Some(resources_root) = &render_options.template_resources_root {
        return Some(resources_root.clone());
    }
    if let Ok(guard) = tool_result_template_runtime().read() {
        if let Some(resources_root) = &guard.resources_root {
            return Some(resources_root.clone());
        }
    }

    let exe_path = std::env::current_exe().ok()?;
    let current_dir = std::env::current_dir().ok()?;
    resolve_runtime_resources_root_from_paths(&current_dir, &exe_path)
}

/// Resolve the implicit fallback resources root from the current directory and executable path.
/// 基于当前工作目录与可执行文件路径解析隐式回退共享资源根目录。
pub(super) fn resolve_runtime_resources_root_from_paths(
    current_dir: &Path,
    exe_path: &Path,
) -> Option<PathBuf> {
    let exe_dir = exe_path.parent()?;
    let parent = exe_dir.parent().unwrap_or(exe_dir);
    let hosted_root = parent.join("resources");
    if hosted_root.exists() && hosted_root.is_dir() {
        return Some(hosted_root);
    }

    let repository_root = current_dir.join("runtime").join("resources");
    if repository_root.exists() && repository_root.is_dir() {
        return Some(repository_root);
    }

    None
}

/// Load template text with skill-local templates taking priority over shared fallback templates.
/// 按“skill 本地模板优先，公共模板兜底”的顺序查找模板文本。
pub(super) fn load_template_text(
    skill_name: Option<&str>,
    template_name: &str,
    render_options: &HostRenderOptions,
) -> Option<String> {
    let skill_roots = resolve_runtime_skill_roots(render_options);
    let resource_root = resolve_runtime_resources_root(render_options);
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
        if path.exists() {
            if let Ok(text) = fs::read_to_string(&path) {
                return Some(text);
            }
        }
    }

    None
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
