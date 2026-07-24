use crate::config::client_budget::{ClientBudgetSnapshot, EffectiveBudgetScope};
pub use luaskills::RuntimeInvocationResult;
use luaskills::ToolOverflowMode;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

mod templates;

pub use templates::initialize_tool_result_template_roots;
use templates::{load_template_text, render_page_default, render_template_text};

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
    /// Optional ordered skill-root chain used to resolve overflow templates for the current render call.
    /// 当前渲染调用可选使用的有序技能根目录链，用于解析超限模板。
    pub template_skill_roots: Vec<PathBuf>,
    /// Optional shared resources root used to resolve fallback overflow templates for the current render call.
    /// 当前渲染调用可选使用的共享资源根目录，用于解析兜底超限模板。
    pub template_resources_root: Option<PathBuf>,
}

/// Unified host-side renderer that accepts the runtime intermediate result and decides between inline, truncate, or page under the host policy.
/// 工具结果统一渲染入口；只接受 runtime 中间结果，再由宿主按统一策略决定是原文、截断还是分页。
/// Parameters: `output` is the runtime-produced invocation result to render.
/// 参数：`output` 是运行时产出的待渲染调用结果。
/// Parameters: `skill_name` is the optional owning skill name used for template lookup.
/// 参数：`skill_name` 是用于模板查找的可选所属 skill 名称。
/// Parameters: `client_budget` is the optional resolved budget snapshot for overflow decisions.
/// 参数：`client_budget` 是用于超限决策的可选预算快照。
/// Parameters: `render_options` carries spill and template discovery roots.
/// 参数：`render_options` 携带超限文件与模板发现根配置。
/// Returns the rendered text or a host-side rendering error.
/// 返回渲染后的文本或宿主侧渲染错误。
pub fn render_tool_result_text(
    output: &RuntimeInvocationResult,
    skill_name: Option<&str>,
    client_budget: Option<&ClientBudgetSnapshot>,
    render_options: &HostRenderOptions,
) -> Result<String, String> {
    let policy = resolve_overflow_policy(skill_name, output);
    match policy.mode {
        OverflowMode::Truncate => {
            render_truncate_text(output, skill_name, &policy, client_budget, render_options)
        }
        OverflowMode::Page => {
            render_page_text(output, skill_name, &policy, client_budget, render_options)
        }
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

/// Mutable accumulator for the paging chunk currently being built.
/// 当前正在构建的分页块的可变累积器。
#[derive(Debug, Default)]
struct OverflowChunkBuilder {
    /// First 1-based line number in the current chunk.
    /// 当前分页块的首个 1 基行号。
    current_start: Option<usize>,
    /// Current chunk byte count, including retained newline separators.
    /// 当前分页块的字节数，包含保留的换行分隔符。
    current_bytes: usize,
    /// Current chunk line count.
    /// 当前分页块的行数。
    current_line_count: usize,
}

impl OverflowChunkBuilder {
    /// Decide whether the next line must start a new chunk before being appended.
    /// 判断下一行在追加前是否必须开启新的分页块。
    fn should_flush_before(
        &self,
        line_bytes: usize,
        safe_limit_bytes: usize,
        safe_limit_lines: i64,
    ) -> bool {
        let lines_limit_hit =
            safe_limit_lines > 0 && self.current_line_count >= safe_limit_lines as usize;
        let bytes_limit_hit = self.current_start.is_some()
            && self.current_bytes.saturating_add(line_bytes) > safe_limit_bytes;
        bytes_limit_hit || lines_limit_hit
    }

    /// Append one line to the current chunk, starting the chunk when needed.
    /// 向当前分页块追加一行，并在需要时启动该分页块。
    fn append_line(&mut self, start_line: usize, line_bytes: usize) {
        if self.current_start.is_none() {
            self.current_start = Some(start_line);
        }
        self.current_bytes += line_bytes;
        self.current_line_count += 1;
    }

    /// Flush the current chunk into the finished chunk list and reset the accumulator.
    /// 将当前分页块写入已完成列表，并重置累积器。
    fn flush_into(&mut self, chunks: &mut Vec<OverflowChunk>, end_line: usize) {
        if let Some(start_line) = self.current_start
            && self.current_line_count > 0
        {
            chunks.push(OverflowChunk {
                offset: start_line - 1,
                limit: self.current_line_count,
                start_line,
                end_line,
                byte_count: self.current_bytes,
            });
        }
        self.current_start = None;
        self.current_bytes = 0;
        self.current_line_count = 0;
    }
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
    render_options: &HostRenderOptions,
) -> Result<String, String> {
    let tool_result_budget = resolve_budget_scope(client_budget, BudgetScopeKind::ToolResult);
    if content_fits_budget(output, &tool_result_budget) {
        return Ok(normalize_text(&output.content));
    }

    let Some(truncated_content) =
        truncate_content_at_line_boundary(&output.content, &tool_result_budget)
    else {
        return Ok(TOOL_OUTPUT_EXCEEDS_LIMIT_ERROR.to_string());
    };

    let mut context = HashMap::new();
    context.insert("truncated_content", truncated_content);
    context.insert("truncate_notice", DEFAULT_TRUNCATE_NOTICE.to_string());

    if let Some(template_text) =
        load_template_text(skill_name, policy.template_name.as_str(), render_options)?
    {
        return Ok(render_template_text(&template_text, &context));
    }

    Ok(render_template_text(
        "{{truncated_content}}\n...\n# {{truncate_notice}}",
        &context,
    ))
}

/// Render `page`; return the original text when it fits, otherwise let the host generate the spill file and read directory.
/// 渲染 `page` 模式；未超限时直接返回原文，超限后由宿主统一生成 spill 文件和读取目录。
fn render_page_text(
    output: &RuntimeInvocationResult,
    skill_name: Option<&str>,
    policy: &OverflowPolicy,
    client_budget: Option<&ClientBudgetSnapshot>,
    render_options: &HostRenderOptions,
) -> Result<String, String> {
    let tool_result_budget = resolve_budget_scope(client_budget, BudgetScopeKind::ToolResult);
    if content_fits_budget(output, &tool_result_budget) {
        return Ok(normalize_text(&output.content));
    }

    let file_read_budget = resolve_budget_scope(client_budget, BudgetScopeKind::FileRead);
    let chunk_plan = match build_chunk_plan(&output.content, &file_read_budget) {
        Ok(plan) => plan,
        Err(_) => return Ok(TOOL_OUTPUT_EXCEEDS_LIMIT_ERROR.to_string()),
    };

    let Some(spill_root) = render_options.spill_root.as_deref() else {
        return Err("page overflow rendering requires a spill_root".to_string());
    };

    let raw_file = write_overflow_text_file(&output.content, policy, spill_root)?;

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

    if let Some(template_text) =
        load_template_text(skill_name, policy.template_name.as_str(), render_options)?
    {
        return Ok(render_template_text(&template_text, &context));
    }

    Ok(render_page_default(&context))
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
fn content_fits_budget(output: &RuntimeInvocationResult, budget: &EffectiveBudgetScope) -> bool {
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
    let safe_limit_bytes_usize = safe_limit_bytes as usize;
    let safe_limit_lines = file_read_budget.lines;

    let mut chunks = Vec::new();
    let mut chunk_builder = OverflowChunkBuilder::default();

    for (index, line) in lines.iter().enumerate() {
        let mut line_bytes = line.len();
        if index + 1 < total_lines {
            line_bytes += 1;
        }
        if line_bytes > safe_limit_bytes_usize {
            return Err(TOOL_OUTPUT_EXCEEDS_LIMIT_ERROR.to_string());
        }

        if chunk_builder.should_flush_before(line_bytes, safe_limit_bytes_usize, safe_limit_lines) {
            chunk_builder.flush_into(&mut chunks, index);
        }
        chunk_builder.append_line(index + 1, line_bytes);
    }

    chunk_builder.flush_into(&mut chunks, total_lines);

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
    let file_name = format!(
        "tool_output_{}_{}.md",
        policy.template_name.replace('.', "_"),
        now.as_millis()
    );
    let file_path = spill_root.join(file_name);
    fs::write(&file_path, content)
        .map_err(|error| format!("failed to write overflow file: {}", error))?;
    Ok(file_path)
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
mod tests;
