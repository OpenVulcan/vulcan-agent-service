use crate::client_budget::{ClientBudgetSnapshot, EffectiveBudgetScope};
use crate::temp_maintenance::ensure_runtime_temp_dir;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// 中文：当工具返回非字符串结果时，统一返回的英文错误提示。
/// English: Unified English error message returned when a tool emits a non-string result.
pub const NON_STRING_TOOL_RESULT_ERROR: &str = "Tool results must be returned as plain strings. Structured JSON or table results are not supported.";

/// 中文：当工具结果单行就超出当前客户端预算时，统一返回的英文错误提示。
/// English: Unified English error message returned when even a single line exceeds the current client budget.
const TOOL_OUTPUT_EXCEEDS_LIMIT_ERROR: &str = "Tool output exceeds the current MCP client limit.";

/// 中文：`truncate` 默认提示文本；会在模板中作为尾部说明输出。
/// English: Default notice text for `truncate`, rendered as the trailing explanation inside the template.
const DEFAULT_TRUNCATE_NOTICE: &str =
    "Content has been truncated because it exceeds the current MCP client limit.";

/// 中文：宿主理解的统一超限模式。
/// English: Unified overflow modes understood by the host runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolOverflowMode {
    /// 中文：超限时按截断模板输出。
    /// English: Render the result with truncate behavior when it overflows.
    Truncate,
    /// 中文：超限时进入分页目录模式。
    /// English: Render the result as a paging directory when it overflows.
    Page,
}

impl ToolOverflowMode {
    /// 中文：解析来自 Lua 的超限模式字符串。
    /// English: Parse an overflow mode string returned from Lua.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "truncate" => Some(Self::Truncate),
            "page" => Some(Self::Page),
            _ => None,
        }
    }
}

/// 中文：Lua 工具返回给宿主的统一字符串结果载荷。
/// English: Unified string-result payload returned from Lua to the host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCallOutput {
    /// 中文：工具正文内容，必须始终为字符串。
    /// English: Tool body content, which must always be a string.
    pub content: String,
    /// 中文：可选超限模式；为空时宿主默认按 `truncate` 解释。
    /// English: Optional overflow mode; when absent the host defaults to `truncate`.
    pub overflow_mode: Option<ToolOverflowMode>,
    /// 中文：可选模板名；为空时宿主按模式回退到固定模板名。
    /// English: Optional template name; when absent the host falls back to the fixed template name for the chosen mode.
    pub template_name: Option<String>,
}

impl ToolCallOutput {
    /// 中文：构造只包含正文的字符串返回值。
    /// English: Build a content-only string result.
    pub fn plain(content: String) -> Self {
        Self {
            content,
            overflow_mode: None,
            template_name: None,
        }
    }
}

/// 中文：工具结果统一渲染入口；只接受宿主已解析好的字符串载荷，再由宿主按统一策略决定是原文、截断还是分页。
/// English: Unified tool-result renderer. It accepts the host-parsed string payload, then decides between inline, truncate, or page under the unified policy.
pub fn render_tool_result_text(
    output: &ToolCallOutput,
    skill_name: Option<&str>,
    client_budget: Option<&ClientBudgetSnapshot>,
) -> String {
    let text = normalize_text(&output.content);
    let policy = resolve_overflow_policy(skill_name, output);
    match policy.mode {
        OverflowMode::Truncate => render_truncate_text(&text, skill_name, &policy, client_budget),
        OverflowMode::Page => render_page_text(&text, skill_name, &policy, client_budget),
    }
}

/// 中文：宿主统一的超限模式。
/// English: Host-side unified overflow modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OverflowMode {
    Truncate,
    Page,
}

/// 中文：工具最终渲染策略，包含模式与模板名。
/// English: Final rendering policy for one tool, including its mode and template name.
#[derive(Debug, Clone)]
struct OverflowPolicy {
    mode: OverflowMode,
    template_name: String,
}

/// 中文：分页模式下的安全分块计划，供模板渲染读取目录时使用。
/// English: Safe chunk plan used by page-mode templates to render the read directory.
#[derive(Debug, Clone)]
struct OverflowChunkPlan {
    chunks: Vec<OverflowChunk>,
    chunk_count: usize,
    total_lines: usize,
    total_bytes: usize,
    safe_limit_bytes: u64,
    safe_limit_lines: i64,
}

/// 中文：单个分页块的偏移与行号范围。
/// English: Offset and line-span information for a single paging chunk.
#[derive(Debug, Clone)]
struct OverflowChunk {
    offset: usize,
    limit: usize,
    start_line: usize,
    end_line: usize,
    byte_count: usize,
}

/// 中文：根据 Lua 返回的模式与模板名解析统一超限策略。
/// English: Resolve the unified overflow policy from the mode and template name returned by Lua.
fn resolve_overflow_policy(skill_name: Option<&str>, output: &ToolCallOutput) -> OverflowPolicy {
    let mode = match output.overflow_mode {
        Some(ToolOverflowMode::Page) => OverflowMode::Page,
        Some(ToolOverflowMode::Truncate) | None => OverflowMode::Truncate,
    };

    OverflowPolicy {
        mode,
        template_name: resolve_template_name(skill_name, output.template_name.as_deref(), mode),
    }
}

/// 中文：获取模板名称，优先使用 Lua 显式返回的模板名，否则回退到当前模式的固定公共模板名。
/// English: Resolve the template name, preferring the explicit template returned from Lua and otherwise falling back to the fixed shared template for the current mode.
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

/// 中文：渲染 `truncate` 模式；若未超限则直接返回原文，若单行都无法容纳则返回统一错误提示。
/// English: Render `truncate`; return the original text when it fits, or the unified error message when even one line cannot fit.
fn render_truncate_text(
    content: &str,
    skill_name: Option<&str>,
    policy: &OverflowPolicy,
    client_budget: Option<&ClientBudgetSnapshot>,
) -> String {
    let tool_result_budget = resolve_budget_scope(client_budget, BudgetScopeKind::ToolResult);
    if content_fits_budget(content, &tool_result_budget) {
        return content.to_string();
    }

    let Some(truncated_content) = truncate_content_at_line_boundary(content, &tool_result_budget)
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

/// 中文：渲染 `page` 模式；未超限时直接返回原文，超限后由宿主统一生成 spill 文件和读取目录。
/// English: Render `page`; return the original text when it fits, otherwise let the host generate the spill file and read directory.
fn render_page_text(
    content: &str,
    skill_name: Option<&str>,
    policy: &OverflowPolicy,
    client_budget: Option<&ClientBudgetSnapshot>,
) -> String {
    let tool_result_budget = resolve_budget_scope(client_budget, BudgetScopeKind::ToolResult);
    if content_fits_budget(content, &tool_result_budget) {
        return content.to_string();
    }

    let file_read_budget = resolve_budget_scope(client_budget, BudgetScopeKind::FileRead);
    let chunk_plan = match build_chunk_plan(content, &file_read_budget) {
        Ok(plan) => plan,
        Err(_) => return TOOL_OUTPUT_EXCEEDS_LIMIT_ERROR.to_string(),
    };

    let raw_file = match write_overflow_text_file(content, policy) {
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

/// 中文：预算场景类型，用于区分“工具结果返回预算”和“客户端文件读取预算”。
/// English: Budget scope kind used to distinguish between tool-result budgets and client file-read budgets.
#[derive(Debug, Clone, Copy)]
enum BudgetScopeKind {
    ToolResult,
    FileRead,
}

/// 中文：从预算快照中选择目标场景，缺失时回退到稳定默认值。
/// English: Pick the target scope from the budget snapshot and fall back to stable defaults when missing.
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

/// 中文：判断正文是否已在目标预算内，无需再进入宿主超限处理。
/// English: Decide whether the content already fits within the target budget and can bypass host overflow handling.
fn content_fits_budget(content: &str, budget: &EffectiveBudgetScope) -> bool {
    let normalized = normalize_text(content);
    let lines = split_lines(&normalized);
    let within_bytes = normalized.len() <= budget.bytes as usize;
    let within_lines = budget.lines <= 0 || lines.len() <= budget.lines as usize;
    within_bytes && within_lines
}

/// 中文：按真实行边界截断正文；若连第一行都无法纳入预算，则返回 `None`。
/// English: Truncate content on real line boundaries; return `None` when even the first line cannot fit into the budget.
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

/// 中文：构建分页模式的安全 chunk 计划；若任一原始行都超出文件读取预算，则直接报错。
/// English: Build the safe chunk plan for page mode; error out immediately when any raw line exceeds the file-read budget.
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

/// 中文：把 chunk 计划渲染成模板可直接替换的列表文本。
/// English: Render the chunk plan into a list block that can be directly inserted into a template.
fn render_chunk_lines(chunk_plan: &OverflowChunkPlan) -> String {
    if chunk_plan.chunks.is_empty() {
        return "- read_01: unavailable".to_string();
    }

    chunk_plan
        .chunks
        .iter()
        .enumerate()
        .map(|(index, chunk)| {
            format!(
                "- read_{:02}: offset={}, limit={}, start_line={}, end_line={}, bytes={}",
                index + 1,
                chunk.offset,
                chunk.limit,
                chunk.start_line,
                chunk.end_line,
                chunk.byte_count
            )
        })
        .collect::<Vec<String>>()
        .join("\n")
}

/// 中文：把超长正文写入宿主统一的临时缓存目录，供分页模式后续读取。
/// English: Write oversized content into the host-managed temp cache directory for later page-mode reads.
fn write_overflow_text_file(content: &str, policy: &OverflowPolicy) -> Result<PathBuf, String> {
    let temp_root = ensure_runtime_temp_dir()
        .map_err(|error| format!("failed to resolve temp dir: {}", error))?;
    let cache_dir = temp_root.join("mcp").join("cache");
    fs::create_dir_all(&cache_dir)
        .map_err(|error| format!("failed to create cache dir: {}", error))?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("failed to read system time: {}", error))?;
    let file_name = format!(
        "tool_output_{}_{}.md",
        policy.template_name.replace('.', "_"),
        now.as_millis()
    );
    let file_path = cache_dir.join(file_name);
    fs::write(&file_path, content)
        .map_err(|error| format!("failed to write overflow file: {}", error))?;
    Ok(file_path)
}

/// 中文：根据当前可执行文件定位运行时 `lua_skills` 根目录；找不到时返回 `None`。
/// English: Locate the runtime `lua_skills` root relative to the current executable; return `None` when it cannot be resolved.
fn resolve_runtime_lua_skills_root() -> Option<PathBuf> {
    let exe_path = std::env::current_exe().ok()?;
    let exe_dir = exe_path.parent()?;
    let parent = exe_dir.parent().unwrap_or(exe_dir);
    let lua_skills = parent.join("lua_skills");
    if lua_skills.exists() {
        Some(lua_skills)
    } else {
        None
    }
}

/// 中文：按“skill 本地模板优先，公共模板兜底”的顺序查找模板文本。
/// English: Load template text with skill-local templates taking priority over shared fallback templates.
fn load_template_text(skill_name: Option<&str>, template_name: &str) -> Option<String> {
    let root = resolve_runtime_lua_skills_root()?;
    let mut candidates = Vec::new();

    if let Some(skill_name) = skill_name {
        candidates.push(root.join(skill_name).join("template").join(template_name));
    }
    candidates.push(root.join("__template").join(template_name));

    for path in candidates {
        if path.exists() {
            if let Ok(text) = fs::read_to_string(&path) {
                return Some(text);
            }
        }
    }

    None
}

/// 中文：执行简单的模板变量替换；未命中的变量替换为空字符串。
/// English: Perform simple template variable substitution; unknown variables are replaced with an empty string.
fn render_template_text(template_text: &str, context: &HashMap<&'static str, String>) -> String {
    let mut rendered = template_text.to_string();
    for (key, value) in context {
        rendered = rendered.replace(&format!("{{{{{}}}}}", key), value);
    }
    rendered
}

/// 中文：渲染分页模式的默认模板内容；仅在外部模板缺失时使用。
/// English: Render the built-in default page template; used only when no external template exists.
fn render_page_default(context: &HashMap<&'static str, String>) -> String {
    render_template_text(
        "# LARGE RESULT POINTER\n\n## Raw File\n- raw_file: {{raw_file}}\n- format: {{format}}\n- total_bytes: {{total_bytes}}\n- total_lines: {{total_lines}}\n- safe_inline_limit_bytes: {{safe_inline_limit_bytes}}\n- safe_inline_limit_lines: {{safe_inline_limit_lines}}\n- safe_read_limit_bytes: {{safe_read_limit_bytes}}\n- safe_read_limit_lines: {{safe_read_limit_lines}}\n- chunk_count: {{chunk_count}}\n\n{{host_safe_read_chunks_section}}\n\n{{read_strategy_section}}",
        context,
    )
}

/// 中文：统一正文换行风格，避免 Windows CRLF 影响按行统计与切块。
/// English: Normalize line endings so Windows CRLF does not affect line counting or chunking.
fn normalize_text(content: &str) -> String {
    content.replace("\r\n", "\n")
}

/// 中文：按行拆分文本；空字符串返回空数组。
/// English: Split text into lines and return an empty array for empty input.
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

/// 中文：在尝试追加下一行时生成候选正文，用于严格的行级截断判断。
/// English: Build the candidate body when appending the next line, used for strict line-boundary truncation checks.
fn join_lines_with_trailing_newline(existing_lines: &[String], next_line: &str) -> String {
    if existing_lines.is_empty() {
        next_line.to_string()
    } else {
        format!("{}\n{}", existing_lines.join("\n"), next_line)
    }
}

#[cfg(test)]
mod tests {
    use super::{ToolCallOutput, ToolOverflowMode, render_template_text, render_tool_result_text};
    use crate::client_budget::{ClientBudgetSnapshot, EffectiveBudgetScope};
    use serde_json::json;
    use std::collections::HashMap;

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
            &ToolCallOutput::plain("short".to_string()),
            Some("vulcan-codekit"),
            Some(&sample_budget()),
        );
        assert_eq!(rendered, "short");
    }

    #[test]
    fn truncate_mode_returns_notice_when_overflowed() {
        let rendered = render_tool_result_text(
            &ToolCallOutput {
                content: "line1\nline2\nline3".to_string(),
                overflow_mode: Some(ToolOverflowMode::Truncate),
                template_name: None,
            },
            Some("vulcan-codekit"),
            Some(&sample_budget()),
        );
        assert!(rendered.contains("Content has been truncated"));
    }

    #[test]
    fn page_mode_returns_pointer_block_when_overflowed() {
        let rendered = render_tool_result_text(
            &ToolCallOutput {
                content: "line1\nline2\nline3\nline4".to_string(),
                overflow_mode: Some(ToolOverflowMode::Page),
                template_name: None,
            },
            Some("vulcan-codekit"),
            Some(&sample_budget()),
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
            &ToolCallOutput {
                content: "this-line-is-too-long".to_string(),
                overflow_mode: Some(ToolOverflowMode::Page),
                template_name: None,
            },
            Some("vulcan-codekit"),
            Some(&budget),
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
