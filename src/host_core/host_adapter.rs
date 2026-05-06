// Host adapter relay DTOs are intentionally introduced before the public gRPC surface consumes every item.
// 宿主适配器中转 DTO 会先于公开 gRPC 表面使用全部条目而引入。
#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

use super::HostRuntime;

/// Stable host identifiers used by Vulcan adapter relay planning.
/// Vulcan 适配器中转规划使用的稳定宿主标识。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HostKind {
    /// OpenCode native plugin host.
    /// OpenCode 原生插件宿主。
    Opencode,
    /// OpenClaw host with plugin and hook surfaces.
    /// 具备插件与 hook 表面的 OpenClaw 宿主。
    Openclaw,
    /// Claude Code host with plugin, skill, hook, and MCP support.
    /// 具备 plugin、skill、hook 与 MCP 支持的 Claude Code 宿主。
    ClaudeCode,
    /// Qwen Code host with hook-rich integration surfaces.
    /// 具备丰富 hook 集成面的 Qwen Code 宿主。
    QwenCode,
    /// Hermes Agent host with Python hook and skill preprocessing surfaces.
    /// 具备 Python hook 与 skill 预处理面的 Hermes Agent 宿主。
    HermesAgent,
    /// Generic MCP-compatible host without native plugin guarantees.
    /// 不保证原生插件能力的通用 MCP 兼容宿主。
    GenericMcp,
    /// Unknown host with conservative fallback assumptions.
    /// 使用保守降级假设的未知宿主。
    Unknown,
}

impl HostKind {
    /// Return the stable string form used by logs and JSON diagnostics.
    /// 返回日志与 JSON 诊断使用的稳定字符串形式。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Opencode => "opencode",
            Self::Openclaw => "openclaw",
            Self::ClaudeCode => "claude-code",
            Self::QwenCode => "qwen-code",
            Self::HermesAgent => "hermes-agent",
            Self::GenericMcp => "generic-mcp",
            Self::Unknown => "unknown",
        }
    }
}

/// Capability support strength for one host feature.
/// 单项宿主能力的支持强度。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CapabilityLevel {
    /// The host supports the capability with adapter-grade fidelity.
    /// 宿主以适配器级保真度支持该能力。
    Full,
    /// The host has a usable but degraded or unproven capability surface.
    /// 宿主具备可用但降级或尚未完全验证的能力面。
    Limited,
    /// The host does not have evidence for this capability.
    /// 宿主没有该能力的支持证据。
    None,
}

/// Evidence category explaining one capability decision.
/// 用于解释单项能力判定的证据类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CapabilityEvidence {
    /// Evidence comes from the current OpenCode plugin implementation.
    /// 证据来自当前 OpenCode 插件实现。
    CurrentPlugin,
    /// Evidence comes from source-code analysis of the target host.
    /// 证据来自目标宿主源码分析。
    SourceAnalysis,
    /// Evidence comes from official host documentation.
    /// 证据来自宿主官方文档。
    OfficialDocs,
    /// Evidence is a conservative assumption.
    /// 证据是保守假设。
    Assumption,
    /// Evidence is a cross-host fallback rule.
    /// 证据是跨宿主 fallback 规则。
    Fallback,
}

/// One host capability tracked by the adapter relay contract.
/// 适配器中转契约跟踪的一项宿主能力。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CapabilityName {
    /// Whether the host has a native plugin surface.
    /// 宿主是否具备原生插件表面。
    NativePlugin,
    /// Whether the host can call MCP tools.
    /// 宿主是否可以调用 MCP tools。
    McpTools,
    /// Whether tool changes can be refreshed live.
    /// tool 变化是否可以动态刷新。
    DynamicToolRefresh,
    /// Whether tool changes are safely handled by restart or reconnect.
    /// tool 变化是否可通过重启或重连安全处理。
    RestartRequiredToolRefresh,
    /// Whether native session identity is available.
    /// 是否可获取原生 session 身份。
    SessionIdAccess,
    /// Whether explicit WorkMem identity can act as fallback.
    /// 显式 WorkMem 身份是否可以作为 fallback。
    WorkmemIdFallback,
    /// Whether precheck-style injection or gating is available.
    /// 是否支持 precheck 风格的注入或拦截。
    Precheck,
    /// Whether postaction-style processing is available.
    /// 是否支持 postaction 风格的处理。
    Postaction,
    /// Whether prompt material can be scoped to one turn.
    /// 提示词材料是否可以限定到单回合。
    TurnScopedPrompt,
    /// Whether workflow prompts can be represented as durable skills.
    /// 工作流提示词是否可表达为持久 skill。
    PersistentWorkflowSkills,
    /// Whether tool lifecycle hooks are available.
    /// 是否具备 tool 生命周期 hook。
    ToolLifecycleHooks,
    /// Whether session lifecycle hooks are available.
    /// 是否具备 session 生命周期 hook。
    SessionLifecycleHooks,
    /// Whether compact lifecycle hooks are available.
    /// 是否具备 compact 生命周期 hook。
    CompactLifecycleHooks,
}

/// All capability names used to build complete host profiles.
/// 用于构建完整宿主画像的全部能力名称。
const ALL_CAPABILITY_NAMES: [CapabilityName; 13] = [
    CapabilityName::NativePlugin,
    CapabilityName::McpTools,
    CapabilityName::DynamicToolRefresh,
    CapabilityName::RestartRequiredToolRefresh,
    CapabilityName::SessionIdAccess,
    CapabilityName::WorkmemIdFallback,
    CapabilityName::Precheck,
    CapabilityName::Postaction,
    CapabilityName::TurnScopedPrompt,
    CapabilityName::PersistentWorkflowSkills,
    CapabilityName::ToolLifecycleHooks,
    CapabilityName::SessionLifecycleHooks,
    CapabilityName::CompactLifecycleHooks,
];

/// Refresh strategy selected after comparing one host profile with tool changes.
/// 对比宿主画像与 tool 变化后选择的刷新策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ToolRefreshMode {
    /// The host can refresh tool changes without restart.
    /// 宿主可以在不重启的情况下刷新 tool 变化。
    Dynamic,
    /// The host should restart or reconnect before relying on changed tools.
    /// 宿主应在依赖变化后的 tools 前重启或重连。
    RestartRequired,
    /// The host has no known safe refresh path.
    /// 宿主没有已知安全刷新路径。
    Unsupported,
}

/// Support metadata for one capability in one host profile.
/// 某个宿主画像中单项能力的支持元信息。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilitySupport {
    /// Support strength used by relay planning.
    /// 中转规划使用的支持强度。
    pub level: CapabilityLevel,
    /// Human-readable reason explaining the support decision.
    /// 解释支持判定的人类可读原因。
    pub reason: String,
    /// Evidence category behind the support decision.
    /// 支持判定背后的证据类别。
    pub evidence: CapabilityEvidence,
    /// Whether this capability normally requires host restart or explicit reload.
    /// 该能力通常是否需要宿主重启或显式 reload。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires_restart: Option<bool>,
}

/// Complete capability profile for one host family.
/// 单个宿主家族的完整能力画像。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostCapabilityProfile {
    /// Stable host identifier.
    /// 稳定宿主标识。
    pub host_kind: HostKind,
    /// Display name used in diagnostics.
    /// 诊断中使用的展示名称。
    pub display_name: String,
    /// Capability support map keyed by stable capability name.
    /// 以稳定能力名为键的能力支持映射。
    pub capabilities: BTreeMap<CapabilityName, CapabilitySupport>,
    /// Additional planning notes.
    /// 额外规划说明。
    pub notes: Vec<String>,
}

/// Runtime integration mode used by one host adapter.
/// 单个宿主适配器使用的运行时集成模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HostAdapterMode {
    /// Native plugin integration.
    /// 原生插件集成。
    NativePlugin,
    /// MCP-compatible degraded integration.
    /// MCP 兼容降级集成。
    McpCompatible,
    /// Hybrid integration combining native and MCP-like surfaces.
    /// 结合原生与类 MCP 表面的混合集成。
    Hybrid,
    /// Unknown integration surface.
    /// 未知集成表面。
    Unknown,
}

/// Identity strategy used by tools that need memory attribution.
/// 需要记忆归因的 tools 使用的身份策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HostAdapterIdentityMode {
    /// A native host session id is required.
    /// 需要原生宿主 session id。
    NativeSession,
    /// Either session id or WorkMem id can be used.
    /// 可使用 session id 或 WorkMem id。
    SessionOrWorkmem,
    /// Only WorkMem fallback identity is expected.
    /// 仅预期使用 WorkMem fallback 身份。
    WorkmemOnly,
}

/// Stable adapter descriptor consumed by relay clients and diagnostics.
/// 中转客户端与诊断消费的稳定适配器描述符。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostAdapterDescriptor {
    /// Stable adapter id.
    /// 稳定适配器标识。
    pub adapter_id: HostKind,
    /// Host kind served by this adapter.
    /// 该适配器服务的宿主类型。
    pub host_kind: HostKind,
    /// Human-readable adapter name.
    /// 人类可读适配器名称。
    pub display_name: String,
    /// Runtime integration mode.
    /// 运行时集成模式。
    pub mode: HostAdapterMode,
    /// Identity strategy for memory-aware tools.
    /// memory-aware tools 使用的身份策略。
    pub identity_mode: HostAdapterIdentityMode,
    /// Host capability profile backing this adapter.
    /// 支撑该适配器的宿主能力画像。
    pub profile: HostCapabilityProfile,
    /// Safest known tool refresh mode.
    /// 已知最安全 tool 刷新模式。
    pub refresh_mode: ToolRefreshMode,
    /// Whether real session-bound memory writes are supported in principle.
    /// 原则上是否支持真实 session 绑定记忆写入。
    pub supports_session_bound_memory_write: bool,
    /// Whether WorkMem fallback is available.
    /// 是否可使用 WorkMem fallback。
    pub supports_workmem_fallback: bool,
    /// Adapter notes used by diagnostics.
    /// 诊断使用的适配器说明。
    pub notes: Vec<String>,
}

/// Source used to derive the effective WorkMem id for one adapter call.
/// 单次适配器调用中有效 WorkMem id 的来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WorkmemIdSource {
    /// WorkMem id was derived from a native session id.
    /// WorkMem id 从原生 session id 派生。
    SessionId,
    /// WorkMem id was supplied explicitly by the caller.
    /// WorkMem id 由调用方显式提供。
    ProvidedWorkmemId,
    /// WorkMem id was generated from workspace identity.
    /// WorkMem id 从 workspace 身份生成。
    GeneratedFromWorkspace,
    /// No WorkMem id is available.
    /// 没有可用 WorkMem id。
    Missing,
}

/// Raw runtime context accepted by the host adapter relay.
/// 宿主适配器中转接收的原始运行时上下文。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostRuntimeContextInput {
    /// Host kind or alias reported by the caller.
    /// 调用方上报的宿主类型或别名。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host_kind: Option<String>,
    /// Native host session id when available.
    /// 可用时的原生宿主 session id。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Explicit WorkMem id supplied by degraded callers.
    /// 降级调用方提供的显式 WorkMem id。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workmem_id: Option<String>,
    /// Turn id or message id supplied by the host.
    /// 宿主提供的 turn id 或 message id。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    /// Workspace path or project key used for deterministic fallback.
    /// 用于确定性 fallback 的工作区路径或项目键。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    /// Current user message used by diagnostics or prompt planning.
    /// 诊断或提示词规划使用的当前用户消息。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_message: Option<String>,
    /// Conversation id used by hosts that do not call it session id.
    /// 不把会话标识称为 session id 的宿主使用的 conversation id。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,
    /// Root session id used by forked or nested-session hosts.
    /// fork 或嵌套会话宿主使用的 root session id。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_session_id: Option<String>,
}

/// Normalized host runtime context consumed by relay wrappers.
/// 中转包装器消费的归一化宿主运行上下文。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostRuntimeContext {
    /// Stable host kind after alias normalization.
    /// 别名归一化后的稳定宿主类型。
    pub host_kind: HostKind,
    /// Capability profile for the normalized host kind.
    /// 归一化宿主类型对应的能力画像。
    pub capabilities: HostCapabilityProfile,
    /// Native or session-like host id when available.
    /// 可用时的原生或类 session 宿主身份。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Effective WorkMem id after fallback resolution.
    /// fallback 解析后的有效 WorkMem id。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workmem_id: Option<String>,
    /// Source used to derive the effective WorkMem id.
    /// 有效 WorkMem id 的来源。
    pub workmem_source: WorkmemIdSource,
    /// Whether tools requiring real session attribution may run.
    /// 依赖真实 session 归因的 tools 是否可以运行。
    pub can_use_session_bound_tools: bool,
    /// Whether tools accepting WorkMem fallback identity may run.
    /// 接受 WorkMem fallback 身份的 tools 是否可以运行。
    pub can_use_workmem_bound_tools: bool,
    /// Normalized turn id when supplied by the host.
    /// 宿主提供时的归一化 turn id。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    /// Normalized workspace path or project key.
    /// 归一化后的工作区路径或项目键。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    /// Normalized user message for diagnostics and prompt planning.
    /// 用于诊断和提示词规划的归一化用户消息。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_message: Option<String>,
    /// Degradation reasons detected while normalizing context.
    /// 上下文归一化过程中检测到的降级原因。
    pub degraded_reasons: Vec<String>,
}

/// Runtime input accepted by adapter runtime binding.
/// 适配器运行时绑定接收的运行时输入。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostAdapterRuntimeInput {
    /// Host kind or alias reported by the caller.
    /// 调用方上报的宿主类型或别名。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host_kind: Option<String>,
    /// Optional host alias that overrides `host_kind`.
    /// 用于覆盖 `host_kind` 的可选宿主别名。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapter_host_kind: Option<String>,
    /// Native host session id when available.
    /// 可用时的原生宿主 session id。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Explicit WorkMem id supplied by degraded callers.
    /// 降级调用方提供的显式 WorkMem id。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workmem_id: Option<String>,
    /// Turn id or message id supplied by the host.
    /// 宿主提供的 turn id 或 message id。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    /// Workspace path or project key used for deterministic fallback.
    /// 用于确定性 fallback 的工作区路径或项目键。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    /// Current user message used by diagnostics or prompt planning.
    /// 诊断或提示词规划使用的当前用户消息。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_message: Option<String>,
    /// Conversation id used by hosts that do not call it session id.
    /// 不把会话标识称为 session id 的宿主使用的 conversation id。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,
    /// Root session id used by forked or nested-session hosts.
    /// fork 或嵌套会话宿主使用的 root session id。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_session_id: Option<String>,
}

/// Runtime object returned after binding one adapter descriptor to one context.
/// 把一个适配器描述符绑定到一份上下文后返回的运行时对象。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostAdapterRuntime {
    /// Adapter descriptor selected for this call.
    /// 本次调用选中的适配器描述符。
    pub descriptor: HostAdapterDescriptor,
    /// Normalized host runtime context.
    /// 归一化宿主运行时上下文。
    pub context: HostRuntimeContext,
    /// Whether the selected identity strategy has enough data to run.
    /// 选中的身份策略是否具备足够数据可运行。
    pub identity_ready: bool,
    /// Degradation reasons from context and adapter strategy checks.
    /// 来自上下文与适配器策略检查的降级原因。
    pub degraded_reasons: Vec<String>,
}

/// Source category for one runtime tool descriptor.
/// 单个运行时 tool 描述符的来源类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ToolDescriptorSource {
    /// Tool comes from LuaSkills.
    /// tool 来自 LuaSkills。
    Luaskill,
    /// Tool comes from MCP wrapping.
    /// tool 来自 MCP 包装。
    Mcp,
    /// Tool comes from the native host.
    /// tool 来自原生宿主。
    Native,
    /// Tool source is unknown.
    /// tool 来源未知。
    Unknown,
}

/// Stable descriptor snapshot for one host-visible tool.
/// 单个宿主可见 tool 的稳定描述符快照。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDescriptorSnapshot {
    /// Stable tool id used by the model-facing registry.
    /// 面向模型的注册表使用的稳定 tool id。
    pub id: String,
    /// Optional display name exposed by the source registry.
    /// 来源注册表暴露的可选展示名称。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Optional model-facing description.
    /// 面向模型的可选描述。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Optional JSON-schema-like input contract.
    /// 可选的类 JSON Schema 输入契约。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<Value>,
    /// Optional version or revision string.
    /// 可选版本或修订字符串。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Source category used for diagnostics.
    /// 用于诊断的来源类别。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<ToolDescriptorSource>,
    /// Number of workflow prompt entries attached to this tool.
    /// 挂接在该 tool 上的工作流提示词条数量。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_count: Option<u32>,
}

/// Tool registry snapshot captured before or after one mutation.
/// 在一次变更前后捕获的 tool 注册表快照。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolRegistrySnapshot {
    /// Host-visible tool descriptors.
    /// 宿主可见的 tool 描述符。
    pub tools: Vec<ToolDescriptorSnapshot>,
}

/// Options used when deciding whether one diff requires host restart.
/// 判断一次差异是否需要宿主重启时使用的选项。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolRegistryDiffOptions {
    /// Explicit refresh mode selected by an adapter.
    /// 适配器选中的显式刷新模式。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_mode: Option<ToolRefreshMode>,
    /// Explicit dynamic refresh flag for low-level callers.
    /// 低层调用方使用的显式动态刷新标记。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dynamic_tool_refresh_supported: Option<bool>,
    /// Explicit restart requirement after a lifecycle operation.
    /// 生命周期操作后的显式重启要求。
    pub host_restart_required: bool,
}

/// Diff result between two tool registry snapshots.
/// 两份 tool 注册表快照之间的差异结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolRegistryDiff {
    /// Tools present only in the next snapshot.
    /// 仅存在于新快照中的 tools。
    pub added: Vec<ToolDescriptorSnapshot>,
    /// Tools present only in the previous snapshot.
    /// 仅存在于旧快照中的 tools。
    pub removed: Vec<ToolDescriptorSnapshot>,
    /// Tools whose id is stable but descriptor fingerprint changed.
    /// id 稳定但描述符指纹发生变化的 tools。
    pub updated: Vec<ToolDescriptorSnapshot>,
    /// Tools whose id and descriptor fingerprint are unchanged.
    /// id 与描述符指纹均未变化的 tools。
    pub unchanged: Vec<ToolDescriptorSnapshot>,
    /// Sorted ids for added, removed, or updated tools.
    /// 新增、删除或更新的 tool id 排序列表。
    pub changed_tool_ids: Vec<String>,
    /// Whether restart or reconnect is required.
    /// 是否需要重启或重连。
    pub restart_required: bool,
    /// Compact human-readable summary.
    /// 紧凑的人类可读摘要。
    pub summary: String,
}

/// Severity used by tool refresh notices.
/// tool 刷新提示使用的严重级别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ToolRefreshNoticeSeverity {
    /// No visible notice is needed.
    /// 不需要可见提示。
    None,
    /// Informational notice.
    /// 信息提示。
    Info,
    /// Warning notice.
    /// 警告提示。
    Warning,
    /// Error-level notice.
    /// 错误级提示。
    Error,
}

/// Structured notice produced after tool registry changes are analyzed.
/// 分析 tool 注册表变化后生成的结构化提示。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolRefreshNotice {
    /// Whether any tool id or descriptor changed.
    /// 是否有任何 tool id 或描述符发生变化。
    pub changed: bool,
    /// Host display name used in messages.
    /// 消息中使用的宿主展示名称。
    pub host_display_name: String,
    /// Refresh mode selected for this host.
    /// 为该宿主选中的刷新模式。
    pub refresh_mode: ToolRefreshMode,
    /// Notice severity for UI, logs, and tool results.
    /// UI、日志和 tool 结果使用的提示严重级别。
    pub severity: ToolRefreshNoticeSeverity,
    /// Whether host restart or reconnect is required.
    /// 是否需要宿主重启或重连。
    pub restart_required: bool,
    /// Tool ids added by the lifecycle operation.
    /// 生命周期操作新增的 tool id。
    pub added_tool_ids: Vec<String>,
    /// Tool ids removed by the lifecycle operation.
    /// 生命周期操作删除的 tool id。
    pub removed_tool_ids: Vec<String>,
    /// Tool ids updated by the lifecycle operation.
    /// 生命周期操作更新的 tool id。
    pub updated_tool_ids: Vec<String>,
    /// All changed tool ids in sorted order.
    /// 所有发生变化的 tool id 排序列表。
    pub changed_tool_ids: Vec<String>,
    /// Original diff summary from the snapshot layer.
    /// 快照层生成的原始差异摘要。
    pub diff_summary: String,
    /// Short model-facing instruction.
    /// 简短模型可见指令。
    pub model_message: String,
    /// User-facing notice suitable for TUI or toast surfaces.
    /// 适合 TUI 或 toast 表面的用户可见提示。
    pub user_message: String,
}

impl HostRuntime {
    /// Describe one host adapter through the internal host-core boundary.
    /// 通过内部 host-core 边界描述一个宿主适配器。
    pub fn describe_host_adapter(&self, host_kind: Option<&str>) -> HostAdapterDescriptor {
        get_host_adapter_descriptor(host_kind)
    }

    /// Build one adapter runtime context through the internal host-core boundary.
    /// 通过内部 host-core 边界构建一个适配器运行时上下文。
    pub fn build_host_adapter_runtime(&self, input: HostAdapterRuntimeInput) -> HostAdapterRuntime {
        build_host_adapter_runtime(input)
    }

    /// Build one tool refresh notice through the internal host-core boundary.
    /// 通过内部 host-core 边界构建一条 tool 刷新提示。
    pub fn build_tool_refresh_notice_for_adapter(
        &self,
        previous: ToolRegistrySnapshot,
        next: ToolRegistrySnapshot,
        host_kind: Option<&str>,
    ) -> Result<ToolRefreshNotice, String> {
        let adapter = self.describe_host_adapter(host_kind);
        build_tool_refresh_notice(previous, next, &adapter)
    }
}

/// Normalize arbitrary host text into a stable host kind.
/// 把任意宿主文本归一化为稳定宿主类型。
pub fn normalize_host_kind(value: Option<&str>) -> HostKind {
    let normalized = value.unwrap_or_default().trim().to_lowercase();
    let normalized = normalized.split_whitespace().collect::<Vec<_>>().join("-");
    match normalized.as_str() {
        "claude" | "claude_code" | "claude-code" | "claudecode" => HostKind::ClaudeCode,
        "generic" | "generic-mcp" | "mcp" => HostKind::GenericMcp,
        "hermes" | "hermes-agent" | "hermes_agent" | "hermesagent" => HostKind::HermesAgent,
        "open-claw" | "open_claw" | "openclaw" => HostKind::Openclaw,
        "open-code" | "open_code" | "opencode" => HostKind::Opencode,
        "qwen" | "qwen-code" | "qwen_code" | "qwencode" => HostKind::QwenCode,
        "unknown" => HostKind::Unknown,
        _ => HostKind::Unknown,
    }
}

/// Return the capability profile for one host kind or alias.
/// 返回某个宿主类型或别名对应的能力画像。
pub fn get_host_capability_profile(host_kind: Option<&str>) -> HostCapabilityProfile {
    let kind = normalize_host_kind(host_kind);
    match kind {
        HostKind::Opencode => opencode_profile(),
        HostKind::Openclaw => openclaw_profile(),
        HostKind::ClaudeCode => claude_code_profile(),
        HostKind::QwenCode => qwen_code_profile(),
        HostKind::HermesAgent => hermes_agent_profile(),
        HostKind::GenericMcp => generic_mcp_profile(),
        HostKind::Unknown => unknown_profile(),
    }
}

/// Build one adapter descriptor from a host kind or alias.
/// 从宿主类型或别名构建一个适配器描述符。
pub fn get_host_adapter_descriptor(host_kind: Option<&str>) -> HostAdapterDescriptor {
    let profile = get_host_capability_profile(host_kind);
    let identity_mode = resolve_host_adapter_identity_mode(&profile);
    HostAdapterDescriptor {
        adapter_id: profile.host_kind,
        host_kind: profile.host_kind,
        display_name: format!("{} Adapter", profile.display_name),
        mode: host_adapter_mode(profile.host_kind),
        identity_mode,
        refresh_mode: resolve_tool_refresh_mode(&profile),
        supports_session_bound_memory_write: profile
            .capabilities
            .get(&CapabilityName::SessionIdAccess)
            .is_some_and(|support| support.level != CapabilityLevel::None),
        supports_workmem_fallback: profile
            .capabilities
            .get(&CapabilityName::WorkmemIdFallback)
            .is_some_and(|support| support.level != CapabilityLevel::None),
        notes: profile.notes.clone(),
        profile,
    }
}

/// Build one runtime adapter binding for a host context.
/// 为一份宿主上下文构建一个运行时适配器绑定。
pub fn build_host_adapter_runtime(input: HostAdapterRuntimeInput) -> HostAdapterRuntime {
    let host_kind_text = input
        .adapter_host_kind
        .as_deref()
        .or(input.host_kind.as_deref());
    let host_kind = normalize_host_kind(host_kind_text);
    let descriptor = get_host_adapter_descriptor(Some(host_kind.as_str()));
    let context = build_host_runtime_context(HostRuntimeContextInput {
        host_kind: Some(host_kind.as_str().to_string()),
        session_id: input.session_id,
        workmem_id: input.workmem_id,
        turn_id: input.turn_id,
        workspace: input.workspace,
        user_message: input.user_message,
        conversation_id: input.conversation_id,
        root_session_id: input.root_session_id,
    });
    let adapter_reasons = resolve_adapter_identity_degradation_reasons(&descriptor, &context);
    let mut degraded_reasons = context.degraded_reasons.clone();
    degraded_reasons.extend(adapter_reasons);
    degraded_reasons = dedupe_strings(degraded_reasons);
    HostAdapterRuntime {
        descriptor,
        context,
        identity_ready: degraded_reasons
            .iter()
            .all(|reason| !reason.starts_with("adapter-requires-")),
        degraded_reasons,
    }
}

/// Normalize raw host context into the shared adapter relay contract.
/// 把原始宿主上下文归一化为共享的适配器中转契约。
pub fn build_host_runtime_context(input: HostRuntimeContextInput) -> HostRuntimeContext {
    let host_kind = normalize_host_kind(input.host_kind.as_deref());
    let capabilities = get_host_capability_profile(Some(host_kind.as_str()));
    let session_id = first_normalized_text([
        input.session_id.as_deref(),
        input.root_session_id.as_deref(),
        input.conversation_id.as_deref(),
    ]);
    let explicit_workmem_id = normalize_context_text(input.workmem_id.as_deref());
    let workspace = normalize_context_text(input.workspace.as_deref());
    let turn_id = normalize_context_text(input.turn_id.as_deref());
    let user_message = normalize_context_text(input.user_message.as_deref());
    let (workmem_id, workmem_source) = resolve_workmem_identity(
        host_kind,
        session_id.as_deref(),
        explicit_workmem_id,
        workspace.as_deref(),
    );
    let mut degraded_reasons = Vec::new();

    if session_id.is_none() {
        degraded_reasons.push(
            "missing-session-id: session-bound tools must use WorkMem fallback or stay disabled"
                .to_string(),
        );
    }
    if workmem_id.is_none() {
        degraded_reasons.push(
            "missing-workmem-id: WorkMem-compatible tools need an explicit id or workspace fallback"
                .to_string(),
        );
    }
    if capabilities
        .capabilities
        .get(&CapabilityName::SessionIdAccess)
        .is_some_and(|support| support.level == CapabilityLevel::None)
    {
        degraded_reasons.push(
            "host-has-no-session-id-access: native memory attribution is unavailable".to_string(),
        );
    }

    HostRuntimeContext {
        host_kind,
        capabilities,
        session_id: session_id.map(str::to_string),
        workmem_id,
        workmem_source,
        can_use_session_bound_tools: session_id.is_some(),
        can_use_workmem_bound_tools: workmem_source != WorkmemIdSource::Missing,
        turn_id,
        workspace,
        user_message,
        degraded_reasons,
    }
}

/// Resolve the safest tool refresh mode for one host profile.
/// 为某个宿主画像解析最安全的 tool 刷新模式。
pub fn resolve_tool_refresh_mode(profile: &HostCapabilityProfile) -> ToolRefreshMode {
    if profile
        .capabilities
        .get(&CapabilityName::DynamicToolRefresh)
        .is_some_and(|support| support.level == CapabilityLevel::Full)
    {
        return ToolRefreshMode::Dynamic;
    }
    if profile
        .capabilities
        .get(&CapabilityName::RestartRequiredToolRefresh)
        .is_some_and(|support| support.level != CapabilityLevel::None)
    {
        return ToolRefreshMode::RestartRequired;
    }
    ToolRefreshMode::Unsupported
}

/// Normalize one tool descriptor snapshot before diffing or fingerprinting.
/// 在差异比较或指纹计算前归一化单个 tool 描述符快照。
pub fn normalize_tool_descriptor_snapshot(
    tool: &ToolDescriptorSnapshot,
) -> Result<ToolDescriptorSnapshot, String> {
    let id = normalize_context_text(Some(&tool.id))
        .ok_or_else(|| "Tool descriptor id is required".to_string())?;
    Ok(ToolDescriptorSnapshot {
        id,
        name: normalize_context_text(tool.name.as_deref()),
        description: normalize_context_text(tool.description.as_deref()),
        input_schema: tool.input_schema.as_ref().map(stable_json_value),
        version: normalize_context_text(tool.version.as_deref()),
        source: tool.source,
        workflow_count: tool.workflow_count,
    })
}

/// Build a deterministic descriptor fingerprint for update detection.
/// 为更新检测构建一条确定性的描述符指纹。
pub fn build_tool_descriptor_fingerprint(tool: &ToolDescriptorSnapshot) -> Result<String, String> {
    let normalized = normalize_tool_descriptor_snapshot(tool)?;
    serde_json::to_string(&normalized).map_err(|error| error.to_string())
}

/// Diff two tool registry snapshots and derive restart guidance.
/// 对比两份 tool 注册表快照并推导重启提示。
pub fn diff_tool_registry_snapshots(
    previous: &ToolRegistrySnapshot,
    next: &ToolRegistrySnapshot,
    options: &ToolRegistryDiffOptions,
) -> Result<ToolRegistryDiff, String> {
    let previous_index = index_tool_snapshots(&previous.tools)?;
    let next_index = index_tool_snapshots(&next.tools)?;
    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut updated = Vec::new();
    let mut unchanged = Vec::new();

    for (id, next_tool) in &next_index {
        if let Some(previous_tool) = previous_index.get(id) {
            if build_tool_descriptor_fingerprint(previous_tool)?
                == build_tool_descriptor_fingerprint(next_tool)?
            {
                unchanged.push(next_tool.clone());
            } else {
                updated.push(next_tool.clone());
            }
        } else {
            added.push(next_tool.clone());
        }
    }

    for (id, previous_tool) in &previous_index {
        if !next_index.contains_key(id) {
            removed.push(previous_tool.clone());
        }
    }

    sort_tools_by_id(&mut added);
    sort_tools_by_id(&mut removed);
    sort_tools_by_id(&mut updated);
    sort_tools_by_id(&mut unchanged);

    let mut changed_tool_ids = added
        .iter()
        .chain(removed.iter())
        .chain(updated.iter())
        .map(|tool| tool.id.clone())
        .collect::<Vec<_>>();
    changed_tool_ids.sort();
    let restart_required = !changed_tool_ids.is_empty() && should_require_restart(options);
    Ok(ToolRegistryDiff {
        summary: build_tool_registry_diff_summary(
            added.len(),
            removed.len(),
            updated.len(),
            restart_required,
        ),
        added,
        removed,
        updated,
        unchanged,
        changed_tool_ids,
        restart_required,
    })
}

/// Build one refresh notice directly from previous and next snapshots.
/// 直接从旧快照和新快照构建一条刷新提示。
pub fn build_tool_refresh_notice(
    previous: ToolRegistrySnapshot,
    next: ToolRegistrySnapshot,
    adapter: &HostAdapterDescriptor,
) -> Result<ToolRefreshNotice, String> {
    let diff = diff_tool_registry_snapshots(
        &previous,
        &next,
        &ToolRegistryDiffOptions {
            refresh_mode: Some(adapter.refresh_mode),
            dynamic_tool_refresh_supported: Some(adapter.refresh_mode == ToolRefreshMode::Dynamic),
            host_restart_required: adapter.refresh_mode == ToolRefreshMode::RestartRequired,
        },
    )?;
    Ok(build_tool_refresh_notice_from_diff(
        diff,
        &adapter.profile.display_name,
        adapter.refresh_mode,
    ))
}

/// Build one refresh notice from an already computed tool registry diff.
/// 从已经计算好的 tool 注册表差异构建一条刷新提示。
pub fn build_tool_refresh_notice_from_diff(
    diff: ToolRegistryDiff,
    host_display_name: &str,
    refresh_mode: ToolRefreshMode,
) -> ToolRefreshNotice {
    let changed = !diff.changed_tool_ids.is_empty();
    let severity = resolve_notice_severity(changed, refresh_mode);
    let restart_required = changed && refresh_mode == ToolRefreshMode::RestartRequired;
    ToolRefreshNotice {
        changed,
        host_display_name: host_display_name.to_string(),
        refresh_mode,
        severity,
        restart_required,
        added_tool_ids: tool_ids(&diff.added),
        removed_tool_ids: tool_ids(&diff.removed),
        updated_tool_ids: tool_ids(&diff.updated),
        changed_tool_ids: diff.changed_tool_ids.clone(),
        diff_summary: diff.summary,
        model_message: build_model_refresh_message(
            host_display_name,
            changed,
            refresh_mode,
            &diff.changed_tool_ids,
        ),
        user_message: build_user_refresh_message(
            host_display_name,
            changed,
            refresh_mode,
            &diff.changed_tool_ids,
        ),
    }
}

/// Build one capability support object.
/// 构建一个能力支持对象。
fn capability(
    level: CapabilityLevel,
    reason: &str,
    evidence: CapabilityEvidence,
    requires_restart: Option<bool>,
) -> CapabilitySupport {
    CapabilitySupport {
        level,
        reason: reason.to_string(),
        evidence,
        requires_restart,
    }
}

/// Build a complete capability map while defaulting unspecified entries to unsupported.
/// 构建完整能力映射，并把未声明能力默认标记为不支持。
fn define_capabilities(
    overrides: Vec<(CapabilityName, CapabilitySupport)>,
) -> BTreeMap<CapabilityName, CapabilitySupport> {
    let mut capabilities = ALL_CAPABILITY_NAMES
        .iter()
        .copied()
        .map(|name| {
            (
                name,
                capability(
                    CapabilityLevel::None,
                    "No host-level evidence for this capability yet.",
                    CapabilityEvidence::Assumption,
                    None,
                ),
            )
        })
        .collect::<BTreeMap<_, _>>();
    for (name, support) in overrides {
        capabilities.insert(name, support);
    }
    capabilities
}

/// Build the OpenCode host capability profile.
/// 构建 OpenCode 宿主能力画像。
fn opencode_profile() -> HostCapabilityProfile {
    HostCapabilityProfile {
        host_kind: HostKind::Opencode,
        display_name: "OpenCode".to_string(),
        capabilities: define_capabilities(vec![
            (
                CapabilityName::NativePlugin,
                capability(
                    CapabilityLevel::Full,
                    "The current repository has a working OpenCode native plugin.",
                    CapabilityEvidence::CurrentPlugin,
                    None,
                ),
            ),
            (
                CapabilityName::McpTools,
                capability(
                    CapabilityLevel::Full,
                    "OpenCode can consume MCP tools outside the native plugin path.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::DynamicToolRefresh,
                capability(
                    CapabilityLevel::None,
                    "OpenCode tool ids should be treated as restart-bound after registry changes.",
                    CapabilityEvidence::CurrentPlugin,
                    None,
                ),
            ),
            (
                CapabilityName::RestartRequiredToolRefresh,
                capability(
                    CapabilityLevel::Full,
                    "Install, uninstall, and update may change tool ids; restart is the safe path.",
                    CapabilityEvidence::CurrentPlugin,
                    Some(true),
                ),
            ),
            (
                CapabilityName::SessionIdAccess,
                capability(
                    CapabilityLevel::Full,
                    "OpenCode plugin contexts expose the active session id.",
                    CapabilityEvidence::CurrentPlugin,
                    None,
                ),
            ),
            (
                CapabilityName::WorkmemIdFallback,
                capability(
                    CapabilityLevel::Full,
                    "WorkMem can use explicit or generated ids when native session identity is unavailable.",
                    CapabilityEvidence::Fallback,
                    None,
                ),
            ),
            (
                CapabilityName::Precheck,
                capability(
                    CapabilityLevel::Full,
                    "OpenCode message transform paths can inject precheck material.",
                    CapabilityEvidence::CurrentPlugin,
                    None,
                ),
            ),
            (
                CapabilityName::Postaction,
                capability(
                    CapabilityLevel::Full,
                    "OpenCode plugin paths own memory post-processing after model/tool activity.",
                    CapabilityEvidence::CurrentPlugin,
                    None,
                ),
            ),
            (
                CapabilityName::TurnScopedPrompt,
                capability(
                    CapabilityLevel::Full,
                    "OpenCode message transform paths can scope prompt material to one turn.",
                    CapabilityEvidence::CurrentPlugin,
                    None,
                ),
            ),
            (
                CapabilityName::PersistentWorkflowSkills,
                capability(
                    CapabilityLevel::Limited,
                    "Workflow prompts can be plugin-managed but are not yet a native durable registry.",
                    CapabilityEvidence::CurrentPlugin,
                    None,
                ),
            ),
            (
                CapabilityName::ToolLifecycleHooks,
                capability(
                    CapabilityLevel::Full,
                    "OpenCode plugin paths include before-tool and after-tool hooks.",
                    CapabilityEvidence::CurrentPlugin,
                    None,
                ),
            ),
            (
                CapabilityName::SessionLifecycleHooks,
                capability(
                    CapabilityLevel::Full,
                    "OpenCode plugin paths receive session lifecycle events.",
                    CapabilityEvidence::CurrentPlugin,
                    None,
                ),
            ),
            (
                CapabilityName::CompactLifecycleHooks,
                capability(
                    CapabilityLevel::Full,
                    "OpenCode exposes compacting hook paths used by the current plugin.",
                    CapabilityEvidence::CurrentPlugin,
                    None,
                ),
            ),
        ]),
        notes: vec![
            "Treat OpenCode as the highest-fidelity native adapter today.".to_string(),
            "Tool id changes should produce a host restart notice.".to_string(),
        ],
    }
}

/// Build the OpenClaw host capability profile.
/// 构建 OpenClaw 宿主能力画像。
fn openclaw_profile() -> HostCapabilityProfile {
    HostCapabilityProfile {
        host_kind: HostKind::Openclaw,
        display_name: "OpenClaw".to_string(),
        capabilities: define_capabilities(vec![
            (
                CapabilityName::NativePlugin,
                capability(
                    CapabilityLevel::Full,
                    "OpenClaw has plugin and internal hook installation paths.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::McpTools,
                capability(
                    CapabilityLevel::Full,
                    "OpenClaw exposes MCP-oriented integration paths.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::DynamicToolRefresh,
                capability(
                    CapabilityLevel::Limited,
                    "Control-plane refresh points exist, but adapter reconciliation must prove live replacement.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::RestartRequiredToolRefresh,
                capability(
                    CapabilityLevel::Limited,
                    "Restart remains the conservative fallback when dynamic refresh cannot be proven.",
                    CapabilityEvidence::SourceAnalysis,
                    Some(true),
                ),
            ),
            (
                CapabilityName::SessionIdAccess,
                capability(
                    CapabilityLevel::Full,
                    "OpenClaw source exposes session keys and control-plane session operations.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::WorkmemIdFallback,
                capability(
                    CapabilityLevel::Full,
                    "WorkMem can degrade to explicit ids when host session wiring is incomplete.",
                    CapabilityEvidence::Fallback,
                    None,
                ),
            ),
            (
                CapabilityName::Precheck,
                capability(
                    CapabilityLevel::Limited,
                    "Message preprocessing hooks can support precheck-like behavior subject to exact timing.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::Postaction,
                capability(
                    CapabilityLevel::Limited,
                    "Message and tool hook paths exist, but multi-stage turn merge requires validation.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::TurnScopedPrompt,
                capability(
                    CapabilityLevel::Limited,
                    "Message preprocessing can inject prompt content but one-turn scope must be enforced.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::PersistentWorkflowSkills,
                capability(
                    CapabilityLevel::Limited,
                    "Workflow prompts can map through hooks, but durable skill registry needs adapter work.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::ToolLifecycleHooks,
                capability(
                    CapabilityLevel::Limited,
                    "Tool events are visible, but blocking and result mutation semantics need checks.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::SessionLifecycleHooks,
                capability(
                    CapabilityLevel::Full,
                    "Internal hooks include session-level events.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::CompactLifecycleHooks,
                capability(
                    CapabilityLevel::Limited,
                    "Compact-specific parity still needs adapter proof.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
        ]),
        notes: vec![
            "OpenClaw is the best next native-adapter candidate.".to_string(),
            "Turn boundaries must be validated before marking full fidelity.".to_string(),
        ],
    }
}

/// Build the Claude Code host capability profile.
/// 构建 Claude Code 宿主能力画像。
fn claude_code_profile() -> HostCapabilityProfile {
    HostCapabilityProfile {
        host_kind: HostKind::ClaudeCode,
        display_name: "Claude Code".to_string(),
        capabilities: define_capabilities(vec![
            (
                CapabilityName::NativePlugin,
                capability(
                    CapabilityLevel::Limited,
                    "Claude Code has plugins, commands, hooks, agents, and skills but not OpenCode's adapter surface.",
                    CapabilityEvidence::OfficialDocs,
                    None,
                ),
            ),
            (
                CapabilityName::McpTools,
                capability(
                    CapabilityLevel::Full,
                    "Claude Code plugins can include MCP server configuration.",
                    CapabilityEvidence::OfficialDocs,
                    None,
                ),
            ),
            (
                CapabilityName::DynamicToolRefresh,
                capability(
                    CapabilityLevel::Limited,
                    "The documented reload flow exists, but live model prompt/tool refresh should not be assumed.",
                    CapabilityEvidence::OfficialDocs,
                    Some(true),
                ),
            ),
            (
                CapabilityName::RestartRequiredToolRefresh,
                capability(
                    CapabilityLevel::Full,
                    "Plugin changes should be treated as reload or restart bound unless live refresh is confirmed.",
                    CapabilityEvidence::OfficialDocs,
                    Some(true),
                ),
            ),
            (
                CapabilityName::SessionIdAccess,
                capability(
                    CapabilityLevel::Limited,
                    "Documentation does not guarantee the same session id access required by VMM attribution.",
                    CapabilityEvidence::OfficialDocs,
                    None,
                ),
            ),
            (
                CapabilityName::WorkmemIdFallback,
                capability(
                    CapabilityLevel::Full,
                    "Explicit WorkMem ids can support MCP-compatible tools when session binding is missing.",
                    CapabilityEvidence::Fallback,
                    None,
                ),
            ),
            (
                CapabilityName::Precheck,
                capability(
                    CapabilityLevel::Limited,
                    "Hooks may support prompt or tool gating, but one-turn precheck injection is not proven.",
                    CapabilityEvidence::OfficialDocs,
                    None,
                ),
            ),
            (
                CapabilityName::Postaction,
                capability(
                    CapabilityLevel::Limited,
                    "Hooks may support post-action behavior, but multi-stage turn merge needs proof.",
                    CapabilityEvidence::OfficialDocs,
                    None,
                ),
            ),
            (
                CapabilityName::TurnScopedPrompt,
                capability(
                    CapabilityLevel::Limited,
                    "Skills and slash commands exist, but temporary per-turn injection should be degraded.",
                    CapabilityEvidence::OfficialDocs,
                    None,
                ),
            ),
            (
                CapabilityName::PersistentWorkflowSkills,
                capability(
                    CapabilityLevel::Full,
                    "Claude Code plugins document skills as plugin content.",
                    CapabilityEvidence::OfficialDocs,
                    None,
                ),
            ),
            (
                CapabilityName::ToolLifecycleHooks,
                capability(
                    CapabilityLevel::Limited,
                    "Hooks and MCP tools can support lifecycle interception in some form, but guarantees are unclear.",
                    CapabilityEvidence::OfficialDocs,
                    None,
                ),
            ),
            (
                CapabilityName::SessionLifecycleHooks,
                capability(
                    CapabilityLevel::Limited,
                    "Lifecycle hooks may exist, but adapter-grade session identity needs validation.",
                    CapabilityEvidence::OfficialDocs,
                    None,
                ),
            ),
            (
                CapabilityName::CompactLifecycleHooks,
                capability(
                    CapabilityLevel::Limited,
                    "Compact parity and per-turn memory timing are not guaranteed.",
                    CapabilityEvidence::OfficialDocs,
                    None,
                ),
            ),
        ]),
        notes: vec![
            "Prefer MCP-compatible mode first for Claude Code.".to_string(),
            "Do not assume per-turn scoped prompt injection until hook payloads are verified."
                .to_string(),
        ],
    }
}

/// Build the Qwen Code host capability profile.
/// 构建 Qwen Code 宿主能力画像。
fn qwen_code_profile() -> HostCapabilityProfile {
    HostCapabilityProfile {
        host_kind: HostKind::QwenCode,
        display_name: "Qwen Code".to_string(),
        capabilities: define_capabilities(vec![
            (
                CapabilityName::NativePlugin,
                capability(
                    CapabilityLevel::Limited,
                    "Qwen Code exposes hooks and settings rather than OpenCode-style plugin packaging.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::McpTools,
                capability(
                    CapabilityLevel::Full,
                    "Qwen Code has MCP server configuration and tool registry paths.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::DynamicToolRefresh,
                capability(
                    CapabilityLevel::Limited,
                    "Tool discovery exists, but live tool id replacement still needs adapter proof.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::RestartRequiredToolRefresh,
                capability(
                    CapabilityLevel::Full,
                    "Tool id changes should fall back to host or session restart until dynamic replacement is proven.",
                    CapabilityEvidence::SourceAnalysis,
                    Some(true),
                ),
            ),
            (
                CapabilityName::SessionIdAccess,
                capability(
                    CapabilityLevel::Full,
                    "Qwen Code config paths expose session id data.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::WorkmemIdFallback,
                capability(
                    CapabilityLevel::Full,
                    "Qwen Code can degrade to explicit WorkMem ids if session binding is unavailable.",
                    CapabilityEvidence::Fallback,
                    None,
                ),
            ),
            (
                CapabilityName::Precheck,
                capability(
                    CapabilityLevel::Full,
                    "Hook tests include UserPromptSubmit and PreToolUse events.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::Postaction,
                capability(
                    CapabilityLevel::Limited,
                    "Stop and PostToolUse hooks exist, but final turn merge semantics should be verified.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::TurnScopedPrompt,
                capability(
                    CapabilityLevel::Limited,
                    "Prompt submit hooks can add context, but exact one-turn scope remains adapter-defined.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::PersistentWorkflowSkills,
                capability(
                    CapabilityLevel::Limited,
                    "Hook-provided context can emulate workflow prompts, but durable skill registry is not proven.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::ToolLifecycleHooks,
                capability(
                    CapabilityLevel::Full,
                    "Hook tests include PreToolUse, PostToolUse, and PostToolUseFailure.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::SessionLifecycleHooks,
                capability(
                    CapabilityLevel::Full,
                    "Hook tests include SessionStart and SessionEnd.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::CompactLifecycleHooks,
                capability(
                    CapabilityLevel::Full,
                    "Hook tests include PreCompact coverage.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
        ]),
        notes: vec![
            "Qwen Code looks hook-rich, but package deployment differs from OpenCode.".to_string(),
            "Use hook evidence to shape the gRPC adapter contract before native packaging."
                .to_string(),
        ],
    }
}

/// Build the Hermes Agent host capability profile.
/// 构建 Hermes Agent 宿主能力画像。
fn hermes_agent_profile() -> HostCapabilityProfile {
    HostCapabilityProfile {
        host_kind: HostKind::HermesAgent,
        display_name: "Hermes Agent".to_string(),
        capabilities: define_capabilities(vec![
            (
                CapabilityName::NativePlugin,
                capability(
                    CapabilityLevel::Full,
                    "Hermes exposes shell hook and skill preprocessing surfaces suitable for a native adapter.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::McpTools,
                capability(
                    CapabilityLevel::Full,
                    "Hermes exposes MCP configuration and serving commands.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::DynamicToolRefresh,
                capability(
                    CapabilityLevel::Limited,
                    "ACP/MCP paths can refresh parts of the tool surface, while CLI configuration may need new sessions.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::RestartRequiredToolRefresh,
                capability(
                    CapabilityLevel::Limited,
                    "New sessions remain the conservative fallback for MCP configuration changes.",
                    CapabilityEvidence::SourceAnalysis,
                    Some(true),
                ),
            ),
            (
                CapabilityName::SessionIdAccess,
                capability(
                    CapabilityLevel::Full,
                    "Hermes skill preprocessing includes session id substitution paths.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::WorkmemIdFallback,
                capability(
                    CapabilityLevel::Full,
                    "Hermes can degrade to explicit WorkMem ids when host session id is absent.",
                    CapabilityEvidence::Fallback,
                    None,
                ),
            ),
            (
                CapabilityName::Precheck,
                capability(
                    CapabilityLevel::Full,
                    "Hermes shell hooks include pre-tool blocking and Claude-style decision conversion.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::Postaction,
                capability(
                    CapabilityLevel::Limited,
                    "Post-tool hooks exist, but final answer memory merge must be proven.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::TurnScopedPrompt,
                capability(
                    CapabilityLevel::Limited,
                    "Skill preprocessing can inject prompt material, but exact one-turn scope needs adapter ownership.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::PersistentWorkflowSkills,
                capability(
                    CapabilityLevel::Full,
                    "Hermes skill command and preprocessing code can carry workflow-style prompt material.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::ToolLifecycleHooks,
                capability(
                    CapabilityLevel::Full,
                    "Hermes shell hooks include pre_tool_call and post_tool_call surfaces.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::SessionLifecycleHooks,
                capability(
                    CapabilityLevel::Full,
                    "Hermes ACP adapter includes explicit session flows.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
            (
                CapabilityName::CompactLifecycleHooks,
                capability(
                    CapabilityLevel::Limited,
                    "Compact lifecycle parity still needs adapter proof.",
                    CapabilityEvidence::SourceAnalysis,
                    None,
                ),
            ),
        ]),
        notes: vec![
            "Hermes should use the same vulcan-agent-service relay contract as TypeScript hosts."
                .to_string(),
            "Python runtime differences argue for gRPC adapter contracts.".to_string(),
        ],
    }
}

/// Build the generic MCP host capability profile.
/// 构建通用 MCP 宿主能力画像。
fn generic_mcp_profile() -> HostCapabilityProfile {
    HostCapabilityProfile {
        host_kind: HostKind::GenericMcp,
        display_name: "Generic MCP Host".to_string(),
        capabilities: define_capabilities(vec![
            (
                CapabilityName::McpTools,
                capability(
                    CapabilityLevel::Full,
                    "The host can call MCP tools but has no known native plugin contract.",
                    CapabilityEvidence::Fallback,
                    None,
                ),
            ),
            (
                CapabilityName::RestartRequiredToolRefresh,
                capability(
                    CapabilityLevel::Full,
                    "Most MCP clients discover tools at connection or session start.",
                    CapabilityEvidence::Fallback,
                    Some(true),
                ),
            ),
            (
                CapabilityName::WorkmemIdFallback,
                capability(
                    CapabilityLevel::Full,
                    "Without host session identity, tools can require explicit or generated WorkMem ids.",
                    CapabilityEvidence::Fallback,
                    None,
                ),
            ),
        ]),
        notes: vec![
            "This profile is the minimum viable compatibility layer.".to_string(),
            "It should not claim precheck, postaction, or host session memory attribution."
                .to_string(),
        ],
    }
}

/// Build the unknown host capability profile.
/// 构建未知宿主能力画像。
fn unknown_profile() -> HostCapabilityProfile {
    HostCapabilityProfile {
        host_kind: HostKind::Unknown,
        display_name: "Unknown Host".to_string(),
        capabilities: define_capabilities(vec![
            (
                CapabilityName::McpTools,
                capability(
                    CapabilityLevel::Limited,
                    "Unknown hosts may support MCP, but this must be configured explicitly.",
                    CapabilityEvidence::Assumption,
                    None,
                ),
            ),
            (
                CapabilityName::RestartRequiredToolRefresh,
                capability(
                    CapabilityLevel::Limited,
                    "Without host-specific refresh evidence, restart or reconnect is the safe guidance.",
                    CapabilityEvidence::Fallback,
                    Some(true),
                ),
            ),
            (
                CapabilityName::WorkmemIdFallback,
                capability(
                    CapabilityLevel::Full,
                    "Explicit WorkMem ids are the safest cross-host fallback.",
                    CapabilityEvidence::Fallback,
                    None,
                ),
            ),
        ]),
        notes: vec![
            "Use this profile until the adapter can positively identify the host.".to_string(),
            "Never assume precheck, postaction, or session id access for unknown hosts."
                .to_string(),
        ],
    }
}

/// Resolve the adapter mode for one host kind.
/// 为单个宿主类型解析适配器模式。
fn host_adapter_mode(host_kind: HostKind) -> HostAdapterMode {
    match host_kind {
        HostKind::Opencode => HostAdapterMode::NativePlugin,
        HostKind::GenericMcp => HostAdapterMode::McpCompatible,
        HostKind::Unknown => HostAdapterMode::Unknown,
        HostKind::Openclaw | HostKind::ClaudeCode | HostKind::QwenCode | HostKind::HermesAgent => {
            HostAdapterMode::Hybrid
        }
    }
}

/// Resolve the identity strategy for one host capability profile.
/// 为一个宿主能力画像解析身份策略。
fn resolve_host_adapter_identity_mode(profile: &HostCapabilityProfile) -> HostAdapterIdentityMode {
    match profile
        .capabilities
        .get(&CapabilityName::SessionIdAccess)
        .map(|support| support.level)
        .unwrap_or(CapabilityLevel::None)
    {
        CapabilityLevel::Full => HostAdapterIdentityMode::NativeSession,
        CapabilityLevel::Limited => HostAdapterIdentityMode::SessionOrWorkmem,
        CapabilityLevel::None => HostAdapterIdentityMode::WorkmemOnly,
    }
}

/// Resolve adapter-level identity degradation reasons.
/// 解析适配器层面的身份降级原因。
fn resolve_adapter_identity_degradation_reasons(
    descriptor: &HostAdapterDescriptor,
    context: &HostRuntimeContext,
) -> Vec<String> {
    match descriptor.identity_mode {
        HostAdapterIdentityMode::NativeSession if !context.can_use_session_bound_tools => vec![
            "adapter-requires-native-session: this host path needs a real session id for full memory attribution"
                .to_string(),
        ],
        HostAdapterIdentityMode::SessionOrWorkmem
            if !context.can_use_session_bound_tools && !context.can_use_workmem_bound_tools =>
        {
            vec![
                "adapter-requires-session-or-workmem: provide a session id, workmem id, or workspace fallback"
                    .to_string(),
            ]
        }
        HostAdapterIdentityMode::WorkmemOnly if !context.can_use_workmem_bound_tools => vec![
            "adapter-requires-workmem: this degraded host path needs an explicit or generated workmem id"
                .to_string(),
        ],
        _ => Vec::new(),
    }
}

/// Normalize optional runtime context text into a trimmed optional string.
/// 把可选运行时上下文文本归一化为裁剪后的可选字符串。
fn normalize_context_text(value: Option<&str>) -> Option<String> {
    let normalized = value.unwrap_or_default().trim();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized.to_string())
    }
}

/// Pick the first non-empty text value from a list of candidates.
/// 从候选值列表中选择第一条非空文本。
fn first_normalized_text<const N: usize>(values: [Option<&str>; N]) -> Option<&str> {
    values
        .into_iter()
        .find(|value| value.is_some_and(|text| !text.trim().is_empty()))
        .flatten()
        .map(str::trim)
}

/// Resolve the effective WorkMem identity and its source.
/// 解析有效 WorkMem 身份及其来源。
fn resolve_workmem_identity(
    host_kind: HostKind,
    session_id: Option<&str>,
    explicit_workmem_id: Option<String>,
    workspace: Option<&str>,
) -> (Option<String>, WorkmemIdSource) {
    if let Some(workmem_id) = explicit_workmem_id {
        return (Some(workmem_id), WorkmemIdSource::ProvidedWorkmemId);
    }
    if let Some(session_id) = session_id {
        return (Some(session_id.to_string()), WorkmemIdSource::SessionId);
    }
    if let Some(workspace) = workspace {
        return (
            Some(format!(
                "vwm_fallback_{}_{}",
                host_kind.as_str().replace('-', "_"),
                stable_text_hash(workspace)
            )),
            WorkmemIdSource::GeneratedFromWorkspace,
        );
    }
    (None, WorkmemIdSource::Missing)
}

/// Build a short deterministic hash for non-secret fallback identifiers.
/// 为非敏感 fallback 标识构建一条短确定性哈希。
fn stable_text_hash(text: &str) -> String {
    let mut hash = 0x811c9dc5_u32;
    for byte in text.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x01000193);
    }
    format!("{hash:08x}")
}

/// Build an id-indexed map and reject duplicate ids early.
/// 构建以 id 为键的映射，并尽早拒绝重复 id。
fn index_tool_snapshots(
    tools: &[ToolDescriptorSnapshot],
) -> Result<BTreeMap<String, ToolDescriptorSnapshot>, String> {
    let mut index = BTreeMap::new();
    for tool in tools {
        let normalized = normalize_tool_descriptor_snapshot(tool)?;
        if index.contains_key(&normalized.id) {
            return Err(format!("Duplicate tool descriptor id: {}", normalized.id));
        }
        index.insert(normalized.id.clone(), normalized);
    }
    Ok(index)
}

/// Sort tool descriptors by stable id in-place.
/// 按稳定 id 对 tool 描述符进行原地排序。
fn sort_tools_by_id(tools: &mut [ToolDescriptorSnapshot]) {
    tools.sort_by(|left, right| left.id.cmp(&right.id));
}

/// Decide whether a changed registry requires host restart.
/// 判断发生变化的注册表是否需要宿主重启。
fn should_require_restart(options: &ToolRegistryDiffOptions) -> bool {
    if options.host_restart_required {
        return true;
    }
    if let Some(refresh_mode) = options.refresh_mode {
        return refresh_mode == ToolRefreshMode::RestartRequired;
    }
    options.dynamic_tool_refresh_supported != Some(true)
}

/// Build a compact diff summary suitable for logs and diagnostics.
/// 构建适合日志与诊断的紧凑差异摘要。
fn build_tool_registry_diff_summary(
    added_count: usize,
    removed_count: usize,
    updated_count: usize,
    restart_required: bool,
) -> String {
    let changed_count = added_count + removed_count + updated_count;
    let restart_text = if restart_required {
        "restart required"
    } else {
        "restart not required"
    };
    format!(
        "{changed_count} changed tool(s): {added_count} added, {removed_count} removed, {updated_count} updated; {restart_text}."
    )
}

/// Resolve notice severity from change state and refresh mode.
/// 根据变化状态和刷新模式解析提示严重级别。
fn resolve_notice_severity(
    changed: bool,
    refresh_mode: ToolRefreshMode,
) -> ToolRefreshNoticeSeverity {
    if !changed {
        return ToolRefreshNoticeSeverity::None;
    }
    match refresh_mode {
        ToolRefreshMode::Dynamic => ToolRefreshNoticeSeverity::Info,
        ToolRefreshMode::RestartRequired => ToolRefreshNoticeSeverity::Warning,
        ToolRefreshMode::Unsupported => ToolRefreshNoticeSeverity::Error,
    }
}

/// Extract stable ids from one tool descriptor bucket.
/// 从一个 tool 描述符分组中提取稳定 id。
fn tool_ids(tools: &[ToolDescriptorSnapshot]) -> Vec<String> {
    let mut ids = tools.iter().map(|tool| tool.id.clone()).collect::<Vec<_>>();
    ids.sort();
    ids
}

/// Build a model-facing refresh message.
/// 构建模型可见的刷新提示消息。
fn build_model_refresh_message(
    host_display_name: &str,
    changed: bool,
    refresh_mode: ToolRefreshMode,
    changed_tool_ids: &[String],
) -> String {
    if !changed {
        return "Tool registry unchanged. Continue using the existing tool surface.".to_string();
    }
    let ids = changed_tool_ids.join(", ");
    match refresh_mode {
        ToolRefreshMode::Dynamic => format!(
            "Tool registry changed for {host_display_name}, and dynamic refresh is supported. Changed tools: {ids}."
        ),
        ToolRefreshMode::RestartRequired => format!(
            "Tool registry changed for {host_display_name}. Restart or reconnect the host before relying on these changed tools: {ids}."
        ),
        ToolRefreshMode::Unsupported => format!(
            "Tool registry changed for {host_display_name}, but this host has no safe refresh path. Changed tools: {ids}."
        ),
    }
}

/// Build a user-facing refresh message.
/// 构建用户可见的刷新提示消息。
fn build_user_refresh_message(
    host_display_name: &str,
    changed: bool,
    refresh_mode: ToolRefreshMode,
    changed_tool_ids: &[String],
) -> String {
    if !changed {
        return "工具注册表没有变化，无需重启宿主。".to_string();
    }
    let ids = changed_tool_ids.join(", ");
    match refresh_mode {
        ToolRefreshMode::Dynamic => {
            format!(
                "{host_display_name} 已支持动态刷新，本次 tool 变化可以继续使用。变化项：{ids}。"
            )
        }
        ToolRefreshMode::RestartRequired => {
            format!(
                "{host_display_name} 的 tool 表面已变化，请重启或重新连接宿主后再依赖这些 tool：{ids}。"
            )
        }
        ToolRefreshMode::Unsupported => {
            format!(
                "{host_display_name} 的 tool 表面已变化，但当前没有安全刷新路径。变化项：{ids}。"
            )
        }
    }
}

/// Normalize arbitrary JSON values into stable-key-order structures.
/// 把任意 JSON 值归一化为键顺序稳定的结构。
fn stable_json_value(value: &Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.iter().map(stable_json_value).collect()),
        Value::Object(map) => {
            let mut entries = map.iter().collect::<Vec<_>>();
            entries.sort_by(|(left_key, _), (right_key, _)| left_key.cmp(right_key));
            let mut stable = serde_json::Map::new();
            for (key, value) in entries {
                stable.insert(key.clone(), stable_json_value(value));
            }
            Value::Object(stable)
        }
        _ => value.clone(),
    }
}

/// Remove duplicate strings while preserving deterministic order.
/// 去除重复字符串并保持确定性顺序。
fn dedupe_strings(values: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    values
        .into_iter()
        .filter(|value| seen.insert(value.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Verify that common host aliases normalize to stable host kinds.
    /// 验证常见宿主别名会归一化为稳定宿主类型。
    #[test]
    fn normalizes_common_host_aliases() {
        assert_eq!(normalize_host_kind(Some("open code")), HostKind::Opencode);
        assert_eq!(normalize_host_kind(Some("claude")), HostKind::ClaudeCode);
        assert_eq!(normalize_host_kind(Some("qwen_code")), HostKind::QwenCode);
        assert_eq!(normalize_host_kind(Some("mcp")), HostKind::GenericMcp);
        assert_eq!(normalize_host_kind(Some("missing")), HostKind::Unknown);
    }

    /// Verify that OpenCode adapter stays restart-bound for tool changes.
    /// 验证 OpenCode 适配器对 tool 变化保持重启绑定。
    #[test]
    fn opencode_adapter_is_restart_bound() {
        let descriptor = get_host_adapter_descriptor(Some("opencode"));
        assert_eq!(descriptor.mode, HostAdapterMode::NativePlugin);
        assert_eq!(
            descriptor.identity_mode,
            HostAdapterIdentityMode::NativeSession
        );
        assert_eq!(descriptor.refresh_mode, ToolRefreshMode::RestartRequired);
        assert!(descriptor.supports_session_bound_memory_write);
        assert!(descriptor.supports_workmem_fallback);
    }

    /// Verify that generic MCP keeps only degraded WorkMem-oriented identity.
    /// 验证 generic MCP 仅保留 WorkMem 导向的降级身份。
    #[test]
    fn generic_mcp_adapter_is_workmem_only() {
        let descriptor = get_host_adapter_descriptor(Some("generic-mcp"));
        assert_eq!(descriptor.mode, HostAdapterMode::McpCompatible);
        assert_eq!(
            descriptor.identity_mode,
            HostAdapterIdentityMode::WorkmemOnly
        );
        assert!(!descriptor.supports_session_bound_memory_write);
        assert!(descriptor.supports_workmem_fallback);
    }

    /// Verify that runtime context derives WorkMem identity from native session id.
    /// 验证运行时上下文会从原生 session id 派生 WorkMem 身份。
    #[test]
    fn context_uses_session_as_workmem_identity() {
        let context = build_host_runtime_context(HostRuntimeContextInput {
            host_kind: Some("opencode".to_string()),
            session_id: Some(" session-a ".to_string()),
            ..Default::default()
        });
        assert_eq!(context.session_id.as_deref(), Some("session-a"));
        assert_eq!(context.workmem_id.as_deref(), Some("session-a"));
        assert_eq!(context.workmem_source, WorkmemIdSource::SessionId);
        assert!(context.can_use_session_bound_tools);
        assert!(context.degraded_reasons.is_empty());
    }

    /// Verify that generic MCP can use deterministic workspace fallback identity.
    /// 验证 generic MCP 可以使用确定性的 workspace fallback 身份。
    #[test]
    fn generic_mcp_context_uses_workspace_fallback() {
        let runtime = build_host_adapter_runtime(HostAdapterRuntimeInput {
            host_kind: Some("generic-mcp".to_string()),
            workspace: Some("D:/projects/demo".to_string()),
            ..Default::default()
        });
        assert_eq!(
            runtime.context.workmem_source,
            WorkmemIdSource::GeneratedFromWorkspace
        );
        assert!(!runtime.context.can_use_session_bound_tools);
        assert!(runtime.context.can_use_workmem_bound_tools);
        assert!(runtime.identity_ready);
    }

    /// Verify that descriptor fingerprints ignore JSON object key ordering.
    /// 验证描述符指纹会忽略 JSON 对象键顺序。
    #[test]
    fn tool_fingerprint_ignores_schema_key_order() {
        let left = ToolDescriptorSnapshot {
            id: "tool-a".to_string(),
            input_schema: Some(json!({
                "type": "object",
                "properties": {
                    "b": { "type": "number" },
                    "a": { "type": "string" }
                }
            })),
            name: None,
            description: None,
            version: None,
            source: None,
            workflow_count: None,
        };
        let right = ToolDescriptorSnapshot {
            id: "tool-a".to_string(),
            input_schema: Some(json!({
                "properties": {
                    "a": { "type": "string" },
                    "b": { "type": "number" }
                },
                "type": "object"
            })),
            name: None,
            description: None,
            version: None,
            source: None,
            workflow_count: None,
        };
        assert_eq!(
            build_tool_descriptor_fingerprint(&left).unwrap(),
            build_tool_descriptor_fingerprint(&right).unwrap()
        );
    }

    /// Verify that tool diff groups added, removed, and updated descriptors.
    /// 验证 tool diff 会分组新增、删除和更新的描述符。
    #[test]
    fn tool_diff_groups_added_removed_and_updated() {
        let diff = diff_tool_registry_snapshots(
            &ToolRegistrySnapshot {
                tools: vec![
                    tool("removed-tool", Some("old")),
                    tool("updated-tool", Some("old")),
                    tool("same-tool", Some("same")),
                ],
            },
            &ToolRegistrySnapshot {
                tools: vec![
                    tool("added-tool", Some("new")),
                    tool("updated-tool", Some("new")),
                    tool("same-tool", Some("same")),
                ],
            },
            &ToolRegistryDiffOptions {
                refresh_mode: Some(ToolRefreshMode::RestartRequired),
                dynamic_tool_refresh_supported: None,
                host_restart_required: false,
            },
        )
        .unwrap();
        assert_eq!(tool_ids(&diff.added), vec!["added-tool"]);
        assert_eq!(tool_ids(&diff.removed), vec!["removed-tool"]);
        assert_eq!(tool_ids(&diff.updated), vec!["updated-tool"]);
        assert_eq!(
            diff.changed_tool_ids,
            vec!["added-tool", "removed-tool", "updated-tool"]
        );
        assert!(diff.restart_required);
    }

    /// Verify that refresh notice reports OpenCode registry changes as restart-required.
    /// 验证刷新提示会把 OpenCode 注册表变化报告为需要重启。
    #[test]
    fn refresh_notice_marks_opencode_changes_restart_required() {
        let adapter = get_host_adapter_descriptor(Some("opencode"));
        let notice = build_tool_refresh_notice(
            ToolRegistrySnapshot {
                tools: vec![tool("tool-a", None)],
            },
            ToolRegistrySnapshot {
                tools: vec![tool("tool-a", None), tool("tool-b", None)],
            },
            &adapter,
        )
        .unwrap();
        assert!(notice.changed);
        assert_eq!(notice.severity, ToolRefreshNoticeSeverity::Warning);
        assert!(notice.restart_required);
        assert_eq!(notice.added_tool_ids, vec!["tool-b"]);
        assert!(notice.model_message.contains("Restart or reconnect"));
    }

    /// Build a compact test descriptor.
    /// 构建一个紧凑测试描述符。
    fn tool(id: &str, description: Option<&str>) -> ToolDescriptorSnapshot {
        ToolDescriptorSnapshot {
            id: id.to_string(),
            name: None,
            description: description.map(str::to_string),
            input_schema: None,
            version: None,
            source: None,
            workflow_count: None,
        }
    }
}
