use crate::client_budget::{ClientBudgetSnapshot, EffectiveBudgetScope};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
pub use vulcan_luaskills::RuntimeInvocationResult;
use vulcan_luaskills::ToolOverflowMode;

/// Unified English error message returned when even a single line exceeds the current client budget.
/// 当工具结果单行就超出当前客户端预算时，统一返回的英文错误提示。
const TOOL_OUTPUT_EXCEEDS_LIMIT_ERROR: &str = "Tool output exceeds the current MCP client limit.";

/// Default notice text for `truncate`, rendered as the trailing explanation inside the template.
/// `truncate` 默认提示文本；会在模板中作为尾部说明输出。
const DEFAULT_TRUNCATE_NOTICE: &str =
    "Content has been truncated because it exceeds the current MCP client limit.";


/// Host render options that make the final rendering decisions explicit at the host layer.
/// 宿主渲染选项，明确哪些最终处理决定属于宿主层。
#[derive(Debug, Clone, Default)]
pub struct HostRenderOptions {
    /// Host-managed spill directory used only when page mode needs to persist oversized output.
    /// 宿主管理的超限文件输出目录；仅在分页模式真正落盘时使用。
    pub spill_root: Option<PathBuf>,
}

/// Unified host-side renderer that accepts the runtime intermediate result and decides between inline, truncate, or page under the host policy.
/// 工具结果统一渲染入口；只接受 runtime 中间结果，再由宿主按统一策略决定是原文、截断还是分页。
pub fn render_tool_result_text(
    output: &RuntimeInvocationResult,
    skill_name: Option<&str>,
    client_budget: Option<&ClientBudgetSnapshot>,
    render_options: &HostRenderOptions,
) -> String {
    let policy = resolve_overflow_policy(skill_name, output);
    match policy.mode {
        OverflowMode::Truncate => render_truncate_text(output, skill_name, &policy, client_budget),
        OverflowMode::Page => render_page_text(
            output,
            skill_name,
            &policy,
            client_budget,
            render_options,
        ),
    }
}

/// Host-side unified overflow modes.
/// 宿主统一的超限模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OverflowMode {
    Truncate,
    Page,
}

/// Final rendering policy for one tool, including its mode and template name.
/// 工具最终渲染策略，包含模式与模板名。
#[derive(Debug, Clone)]
struct OverflowPolicy {
    mode: OverflowMode,
    template_name: String,
}

/// Safe chunk plan used by page-mode templates to render the read directory.
/// 分页模式下的安全分块计划，供模板渲染读取目录时使用。
#[derive(Debug, Clone)]
struct OverflowChunkPlan {
    chunks: Vec<OverflowChunk>,
    chunk_count: usize,
    total_lines: usize,
    total_bytes: usize,
    safe_limit_bytes: u64,
    safe_limit_lines: i64,
}

/// Offset and line-span information for a single paging chunk.
/// 单个分页块的偏移与行号范围。
#[derive(Debug, Clone)]
struct OverflowChunk {
    offset: usize,
    limit: usize,
    start_line: usize,
    end_line: usize,
    byte_count: usize,
}

/// Resolve the unified overflow policy from the mode and template name returned by Lua.
/// 根据 Lua 返回的模式与模板名解析统一超限策略。
fn resolve_overflow_policy(
    skill_name: Option<&str>,
    output: &RuntimeInvocationResult,
) -> OverflowPolicy {
    let mode = match output.overflow_mode {
        Some(ToolOverflowMode::Page) => OverflowMode::Page,
        Some(ToolOverflowMode::Truncate) | None => OverflowMode::Truncate,
    };

    OverflowPolicy {
        mode,
        template_name: resolve_template_name(skill_name, output.template_hint.as_deref(), mode),
    }
}

/// Resolve the template name, preferring the explicit template returned from Lua and otherwise falling back to the fixed shared template for the current mode.
/// 获取模板名称，优先使用 Lua 显式返回的模板名，否则回退到当前模式的固定公共模板名。
fn resolve_template_name(
    _skill_name: Option<&str>,
    explicit_template_name: Option<&str>,
    mode: OverflowMode,
) -> String {
    explicit_template_name
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(match mode {
            OverflowMode::Truncate => "overflow_truncate.md",
            OverflowMode::Page => "overflow_page.md",
        })
        .to_string()
}

/// Render `truncate`; return the original text when it fits, or the unified error message when even one line cannot fit.
/// 渲染 `truncate` 模式；若未超限则直接返回原文，若单行都无法容纳则返回统一错误提示。
fn render_truncate_text(
    output: &RuntimeInvocationResult,
    skill_name: Option<&str>,
    policy: &OverflowPolicy,
    client_budget: Option<&ClientBudgetSnapshot>,
) -> String {
    let tool_result_budget = resolve_budget_scope(client_budget, BudgetScopeKind::ToolResult);
    if content_fits_budget(output, &tool_result_budget) {
        return normalize_text(&output.content);
    }

    let Some(truncated_content) =
        truncate_content_at_line_boundary(&output.content, &tool_result_budget)
    else {
        return TOOL_OUTPUT_EXCEEDS_LIMIT_ERROR.to_string();
    };

    let mut context = HashMap::new();
    context.insert("truncated_content", truncated_content);
    context.insert("truncate_notice", DEFAULT_TRUNCATE_NOTICE.to_string());

    if let Some(template_text) = load_template_text(skill_name, policy.template_name.as_str()) {
        return render_template_text(&template_text, &context);
    }

    render_template_text(
        "{{truncated_content}}\n...\n# {{truncate_notice}}",
        &context,
    )
}

/// Render `page`; return the original text when it fits, otherwise let the host generate the spill file and read directory.
/// 渲染 `page` 模式；未超限时直接返回原文，超限后由宿主统一生成 spill 文件和读取目录。
fn render_page_text(
    output: &RuntimeInvocationResult,
    skill_name: Option<&str>,
    policy: &OverflowPolicy,
    client_budget: Option<&ClientBudgetSnapshot>,
    render_options: &HostRenderOptions,
) -> String {
    let tool_result_budget = resolve_budget_scope(client_budget, BudgetScopeKind::ToolResult);
    if content_fits_budget(output, &tool_result_budget) {
        return normalize_text(&output.content);
    }

    let file_read_budget = resolve_budget_scope(client_budget, BudgetScopeKind::FileRead);
    let chunk_plan = match build_chunk_plan(&output.content, &file_read_budget) {
        Ok(plan) => plan,
        Err(_) => return TOOL_OUTPUT_EXCEEDS_LIMIT_ERROR.to_string(),
    };

    let Some(spill_root) = render_options.spill_root.as_deref() else {
        return TOOL_OUTPUT_EXCEEDS_LIMIT_ERROR.to_string();
    };

    let raw_file = match write_overflow_text_file(&output.content, policy, spill_root) {
        Ok(path) => path,
        Err(_) => return TOOL_OUTPUT_EXCEEDS_LIMIT_ERROR.to_string(),
    };

    let mut context = HashMap::new();
    context.insert("raw_file", raw_file.to_string_lossy().to_string());
    context.insert("format", "markdown".to_string());
    context.insert("total_bytes", chunk_plan.total_bytes.to_string());
    context.insert("total_lines", chunk_plan.total_lines.to_string());
    context.insert(
        "safe_inline_limit_bytes",
        tool_result_budget.bytes.to_string(),
    );
    context.insert(
        "safe_inline_limit_lines",
        tool_result_budget.lines.to_string(),
    );
    context.insert(
        "safe_read_limit_bytes",
        chunk_plan.safe_limit_bytes.to_string(),
    );
    context.insert(
        "safe_read_limit_lines",
        chunk_plan.safe_limit_lines.to_string(),
    );
    context.insert("chunk_count", chunk_plan.chunk_count.to_string());
    context.insert(
        "host_safe_read_chunks_section",
        render_chunk_lines(&chunk_plan),
    );
    context.insert(
        "read_strategy_section",
        "## Read Strategy\n- Read the raw_file directly.\n- Read by line ranges using the chunk table below.\n- Use the chunk table offset as the 0-based line offset and limit as the line count.".to_string(),
    );
    context.insert("optional_summary_section", String::new());
    context.insert("scope_advice_section", String::new());

    if let Some(template_text) = load_template_text(skill_name, policy.template_name.as_str()) {
        return render_template_text(&template_text, &context);
    }

    render_page_default(&context)
}

/// Budget scope kind used to distinguish between tool-result budgets and client file-read budgets.
/// 预算场景类型，用于区分“工具结果返回预算”和“客户端文件读取预算”。
#[derive(Debug, Clone, Copy)]
enum BudgetScopeKind {
    ToolResult,
    FileRead,
}

/// Pick the target scope from the budget snapshot and fall back to stable defaults when missing.
/// 从预算快照中选择目标场景，缺失时回退到稳定默认值。
fn resolve_budget_scope(
    client_budget: Option<&ClientBudgetSnapshot>,
    scope_kind: BudgetScopeKind,
) -> EffectiveBudgetScope {
    match (client_budget, scope_kind) {
        (Some(snapshot), BudgetScopeKind::ToolResult) => snapshot.tool_result.clone(),
        (Some(snapshot), BudgetScopeKind::FileRead) => snapshot.file_read.clone(),
        (None, _) => EffectiveBudgetScope {
            bytes: 10_000,
            lines: -1,
        },
    }
}

/// Decide whether the content already fits within the target budget and can bypass host overflow handling.
/// 判断正文是否已在目标预算内，无需再进入宿主超限处理。
fn content_fits_budget(
    output: &RuntimeInvocationResult,
    budget: &EffectiveBudgetScope,
) -> bool {
    let within_bytes = output.content_bytes <= budget.bytes as usize;
    let within_lines = budget.lines <= 0 || output.content_lines <= budget.lines as usize;
    within_bytes && within_lines
}

/// Truncate content on real line boundaries; return `None` when even the first line cannot fit into the budget.
/// 按真实行边界截断正文；若连第一行都无法纳入预算，则返回 `None`。
fn truncate_content_at_line_boundary(
    content: &str,
    budget: &EffectiveBudgetScope,
) -> Option<String> {
    let normalized = normalize_text(content);
    let source_lines = split_lines(&normalized);
    if source_lines.is_empty() {
        return Some(String::new());
    }

    let mut accepted_lines = Vec::new();
    let max_lines = if budget.lines > 0 {
        budget.lines as usize
    } else {
        usize::MAX
    };

    for line in source_lines {
        if accepted_lines.len() >= max_lines {
            break;
        }
        let candidate = join_lines_with_trailing_newline(&accepted_lines, &line);
        if candidate.len() > budget.bytes as usize {
            return if accepted_lines.is_empty() {
                None
            } else {
                Some(accepted_lines.join("\n"))
            };
        }
        accepted_lines.push(line);
    }

    if accepted_lines.is_empty() {
        None
    } else {
        Some(accepted_lines.join("\n"))
    }
}

/// Build the safe chunk plan for page mode; error out immediately when any raw line exceeds the file-read budget.
/// 构建分页模式的安全 chunk 计划；若任一原始行都超出文件读取预算，则直接报错。
fn build_chunk_plan(
    content: &str,
    file_read_budget: &EffectiveBudgetScope,
) -> Result<OverflowChunkPlan, String> {
    let normalized = normalize_text(content);
    let lines = split_lines(&normalized);
    let total_lines = lines.len();
    let total_bytes = normalized.len();
    let safe_limit_bytes = file_read_budget.bytes.max(1);
    let safe_limit_lines = file_read_budget.lines;

    let mut chunks = Vec::new();
    let mut current_start: Option<usize> = None;
    let mut current_bytes = 0usize;
    let mut current_line_count = 0usize;

    let flush_chunk = |chunks: &mut Vec<OverflowChunk>,
                       current_start: &mut Option<usize>,
                       current_bytes: &mut usize,
                       current_line_count: &mut usize,
                       end_line: usize| {
        if let Some(start_line) = *current_start {
            if *current_line_count > 0 {
                chunks.push(OverflowChunk {
                    offset: start_line - 1,
                    limit: *current_line_count,
                    start_line,
                    end_line,
                    byte_count: *current_bytes,
                });
            }
        }
        *current_start = None;
        *current_bytes = 0;
        *current_line_count = 0;
    };

    for (index, line) in lines.iter().enumerate() {
        let mut line_bytes = line.len();
        if index + 1 < total_lines {
            line_bytes += 1;
        }
        if line_bytes > safe_limit_bytes as usize {
            return Err(TOOL_OUTPUT_EXCEEDS_LIMIT_ERROR.to_string());
        }

        let lines_limit_hit =
            safe_limit_lines > 0 && current_line_count >= safe_limit_lines as usize;
        let bytes_limit_hit = current_start.is_some()
            && current_bytes.saturating_add(line_bytes) > safe_limit_bytes as usize;
        if bytes_limit_hit || lines_limit_hit {
            flush_chunk(
                &mut chunks,
                &mut current_start,
                &mut current_bytes,
                &mut current_line_count,
                index,
            );
        }

        if current_start.is_none() {
            current_start = Some(index + 1);
        }
        current_bytes += line_bytes;
        current_line_count += 1;
    }

    flush_chunk(
        &mut chunks,
        &mut current_start,
        &mut current_bytes,
        &mut current_line_count,
        total_lines,
    );

    Ok(OverflowChunkPlan {
        chunk_count: chunks.len(),
        chunks,
        total_lines,
        total_bytes,
        safe_limit_bytes,
        safe_limit_lines,
    })
}

/// Render the chunk plan into a list block that can be directly inserted into a template.
/// 把 chunk 计划渲染成模板可直接替换的列表文本。
fn render_chunk_lines(chunk_plan: &OverflowChunkPlan) -> String {
    if chunk_plan.chunks.is_empty() {
        return "- read_01: unavailable".to_string();
    }

    chunk_plan
        .chunks
        .iter()
        .enumerate()
        .map(|(index, chunk)| {
            format!("- read_{:02}: offset={}, limit={}, start_line={}, end_line={}, bytes={}", index + 1, chunk.offset, chunk.limit, chunk.start_line, chunk.end_line, chunk.byte_count)
        })
        .collect::<Vec<String>>()
        .join("\n")
}

/// Write oversized content into the host-provided spill directory for later page-mode reads.
/// 把超长正文写入宿主指定的 spill 目录，供分页模式后续读取。
fn write_overflow_text_file(
    content: &str,
    policy: &OverflowPolicy,
    spill_root: &Path,
) -> Result<PathBuf, String> {
    fs::create_dir_all(spill_root)
        .map_err(|error| format!("failed to create spill dir: {}", error))?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("failed to read system time: {}", error))?;
    let file_name = format!("tool_output_{}_{}.md", policy.template_name.replace('.', "_"), now.as_millis());
    let file_path = spill_root.join(file_name);
    fs::write(&file_path, content)
        .map_err(|error| format!("failed to write overflow file: {}", error))?;
    Ok(file_path)
}

/// Locate the skill root according to the current runtime layout, preferring the hosted runtime directory and then the repository layout.
/// 根据当前运行形态定位技能根目录；优先使用宿主运行目录，其次回退到仓库目录。
fn resolve_runtime_skills_root() -> Option<PathBuf> {
    let exe_path = std::env::current_exe().ok()?;
    let exe_dir = exe_path.parent()?;
    let parent = exe_dir.parent().unwrap_or(exe_dir);
    let hosted_root = parent.join("skills");
    if hosted_root.exists() {
        return Some(hosted_root);
    }

    let repository_root = std::env::current_dir().ok()?.join("runtime").join("skills");
    if repository_root.exists() {
        return Some(repository_root);
    }

    None
}

/// Locate the shared resources root according to the current runtime layout, preferring the hosted runtime directory and then the repository layout.
/// 根据当前运行形态定位共享资源根目录；优先使用宿主运行目录，其次回退到仓库目录。
fn resolve_runtime_resources_root() -> Option<PathBuf> {
    let exe_path = std::env::current_exe().ok()?;
    let exe_dir = exe_path.parent()?;
    let parent = exe_dir.parent().unwrap_or(exe_dir);
    let hosted_root = parent.join("resources");
    if hosted_root.exists() {
        return Some(hosted_root);
    }

    let repository_root = std::env::current_dir().ok()?.join("runtime").join("resources");
    if repository_root.exists() {
        return Some(repository_root);
    }

    None
}

/// Load template text with skill-local templates taking priority over shared fallback templates.
/// 按“skill 本地模板优先，公共模板兜底”的顺序查找模板文本。
fn load_template_text(skill_name: Option<&str>, template_name: &str) -> Option<String> {
    let skill_root = resolve_runtime_skills_root()?;
    let resource_root = resolve_runtime_resources_root()?;
    let mut candidates = Vec::new();

    if let Some(skill_name) = skill_name {
        candidates.push(
            skill_root
                .join(skill_name)
                .join("overflow_templates")
                .join(template_name),
        );
    }
    candidates.push(
        resource_root
            .join("overflow_templates")
            .join(template_name),
    );

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
fn render_template_text(template_text: &str, context: &HashMap<&'static str, String>) -> String {
    let mut rendered = template_text.to_string();
    for (key, value) in context {
        rendered = rendered.replace(&format!("{{{{{}}}}}", key), value);
    }
    rendered
}

/// Render the built-in default page template; used only when no external template exists.
/// 渲染分页模式的默认模板内容；仅在外部模板缺失时使用。
fn render_page_default(context: &HashMap<&'static str, String>) -> String {
    render_template_text(
        "# LARGE RESULT POINTER\n\n## Raw File\n- raw_file: {{raw_file}}\n- format: {{format}}\n- total_bytes: {{total_bytes}}\n- total_lines: {{total_lines}}\n- safe_inline_limit_bytes: {{safe_inline_limit_bytes}}\n- safe_inline_limit_lines: {{safe_inline_limit_lines}}\n- safe_read_limit_bytes: {{safe_read_limit_bytes}}\n- safe_read_limit_lines: {{safe_read_limit_lines}}\n- chunk_count: {{chunk_count}}\n\n{{host_safe_read_chunks_section}}\n\n{{read_strategy_section}}",
        context,
    )
}

/// Normalize line endings so Windows CRLF does not affect line counting or chunking.
/// 统一正文换行风格，避免 Windows CRLF 影响按行统计与切块。
fn normalize_text(content: &str) -> String {
    content.replace("\r\n", "\n")
}

/// Split text into lines and return an empty array for empty input.
/// 按行拆分文本；空字符串返回空数组。
fn split_lines(content: &str) -> Vec<String> {
    let normalized = normalize_text(content);
    if normalized.is_empty() {
        Vec::new()
    } else {
        normalized
            .split('\n')
            .map(|line| line.to_string())
            .collect()
    }
}

/// Build the candidate body when appending the next line, used for strict line-boundary truncation checks.
/// 在尝试追加下一行时生成候选正文，用于严格的行级截断判断。
fn join_lines_with_trailing_newline(existing_lines: &[String], next_line: &str) -> String {
    if existing_lines.is_empty() {
        next_line.to_string()
    } else {
        format!("{}\n{}", existing_lines.join("\n"), next_line)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        HostRenderOptions, RuntimeInvocationResult, ToolOverflowMode, render_template_text,
        render_tool_result_text,
    };
    use crate::client_budget::{ClientBudgetSnapshot, EffectiveBudgetScope};
    use serde_json::json;
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn sample_budget() -> ClientBudgetSnapshot {
        ClientBudgetSnapshot {
            client_name: Some("test".to_string()),
            tool_name: Some("codekit-rg".to_string()),
            skill_name: Some("vulcan-codekit".to_string()),
            matched_client_pattern: Some("*test*".to_string()),
            tool_result: EffectiveBudgetScope {
                bytes: 12,
                lines: -1,
            },
            file_read: EffectiveBudgetScope {
                bytes: 12,
                lines: 2,
            },
            tool_config: json!({}),
        }
    }

    #[test]
    fn plain_result_defaults_to_truncate_policy() {
        let rendered = render_tool_result_text(
            &RuntimeInvocationResult::plain("short".to_string()),
            Some("vulcan-codekit"),
            Some(&sample_budget()),
            &HostRenderOptions::default(),
        );
        assert_eq!(rendered, "short");
    }

    #[test]
    fn truncate_mode_returns_notice_when_overflowed() {
        let rendered = render_tool_result_text(
            &RuntimeInvocationResult::from_content_parts(
                "line1\nline2\nline3".to_string(),
                Some(ToolOverflowMode::Truncate),
                None,
            ),
            Some("vulcan-codekit"),
            Some(&sample_budget()),
            &HostRenderOptions::default(),
        );
        assert!(rendered.contains("Content has been truncated"));
    }

    #[test]
    fn page_mode_returns_pointer_block_when_overflowed() {
        let rendered = render_tool_result_text(
            &RuntimeInvocationResult::from_content_parts(
                "line1\nline2\nline3\nline4".to_string(),
                Some(ToolOverflowMode::Page),
                None,
            ),
            Some("vulcan-codekit"),
            Some(&sample_budget()),
            &HostRenderOptions {
                spill_root: Some(PathBuf::from("target/test-runtime-page-output")),
            },
        );
        assert!(rendered.contains("# LARGE RESULT POINTER"));
        assert!(rendered.contains("raw_file:"));
        assert!(rendered.contains("read_01:"));
    }

    #[test]
    fn page_tool_returns_error_when_single_line_exceeds_file_read_limit() {
        let budget = ClientBudgetSnapshot {
            file_read: EffectiveBudgetScope { bytes: 5, lines: 2 },
            ..sample_budget()
        };
        let rendered = render_tool_result_text(
            &RuntimeInvocationResult::from_content_parts(
                "this-line-is-too-long".to_string(),
                Some(ToolOverflowMode::Page),
                None,
            ),
            Some("vulcan-codekit"),
            Some(&budget),
            &HostRenderOptions {
                spill_root: Some(PathBuf::from("target/test-runtime-page-error")),
            },
        );
        assert_eq!(
            rendered,
            "Tool output exceeds the current MCP client limit."
        );
    }

    #[test]
    fn render_template_replaces_placeholders() {
        let mut context = HashMap::new();
        context.insert("truncated_content", "abc".to_string());
        context.insert("truncate_notice", "cut".to_string());
        let rendered = render_template_text("{{truncated_content}}\n{{truncate_notice}}", &context);
        assert_eq!(rendered, "abc\ncut");
    }
}
