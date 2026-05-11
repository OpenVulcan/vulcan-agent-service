// Host adapter relay DTOs are intentionally introduced before the public gRPC surface consumes every item.
// 宿主适配器中转 DTO 会先于公开 gRPC 表面使用全部条目而引入。
#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

use super::HostRuntime;

mod context;
mod profiles;
#[cfg(test)]
mod tests;
mod tool_refresh;

#[allow(unused_imports)]
pub use context::{build_host_adapter_runtime, build_host_runtime_context};
pub use profiles::{get_host_adapter_descriptor, get_host_capability_profile, normalize_host_kind};
#[allow(unused_imports)]
pub use tool_refresh::{
    build_tool_descriptor_fingerprint, build_tool_refresh_notice,
    build_tool_refresh_notice_from_diff, diff_tool_registry_snapshots,
    normalize_tool_descriptor_snapshot, resolve_tool_refresh_mode,
};

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
