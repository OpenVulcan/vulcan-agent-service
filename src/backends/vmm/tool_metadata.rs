//! Stable host-facing metadata for VMM memory, binding, and profile tools.
//! VMM 记忆、绑定与画像工具的稳定宿主可见元信息。
//!
//! This module belongs to the VMM backend adapter layer. Host plugins use the
//! gRPC HostAdapterService to fetch these descriptors before registering their
//! own model-facing tools.
//! 本模块属于 VMM 后端适配层。宿主插件会先通过 gRPC HostAdapterService
//! 获取这些描述，再注册自身面向模型的工具。

use serde_json::{Value, json};

/// Maximum hit count accepted by the public memory search tool.
/// 公开记忆搜索工具接受的最大召回数量。
const MAX_MEMORY_SEARCH_TOP_K: u64 = 64;

/// Maximum query count accepted by the public memory search tool.
/// 公开记忆搜索工具接受的最大查询数量。
const MAX_MEMORY_SEARCH_QUERIES: u64 = 16;

/// Maximum write batch size accepted by the public memory write tool.
/// 公开记忆写入工具接受的最大批量写入数量。
const MAX_MEMORY_WRITE_ITEMS: u64 = 8;

/// Metadata authority string returned to host plugins for diagnostics.
/// 返回给宿主插件用于诊断的元信息权威来源字符串。
const VMM_MEMORY_TOOL_METADATA_SOURCE: &str = "vmm.grpc-integration-contract";

/// Tool-group annotation used by all stable VMM memory descriptors.
/// 全部稳定 VMM 记忆描述共用的工具分组注解。
const VMM_MEMORY_TOOL_GROUP: &str = "vmm-memory";

/// Execution-mode annotation used by all stable VMM memory descriptors.
/// 全部稳定 VMM 记忆描述共用的执行模式注解。
const VMM_MEMORY_EXECUTION_MODE: &str = "remote";

/// Registration surface annotation used to distinguish canonical host-memory tools from compat and raw VMM tools.
/// 用于区分 canonical 宿主记忆工具、兼容工具与原始 VMM 工具的注册面注解。
const VMM_MEMORY_REGISTRATION_SURFACE: &str = "registration_surface";

/// Registration surface value used by canonical host-memory tools.
/// canonical 宿主记忆工具使用的注册面取值。
const VMM_MEMORY_SURFACE_CANONICAL: &str = "host-memory-canonical";

/// Registration surface value used by Vulcan compatibility memory tools.
/// Vulcan 兼容记忆工具使用的注册面取值。
const VMM_MEMORY_SURFACE_COMPAT: &str = "host-memory-compat";

/// Registration surface value used by raw VMM helper tools that are not part of the default host memory manifest.
/// 默认宿主记忆 manifest 不直接注册的原始 VMM 辅助工具使用的注册面取值。
const VMM_MEMORY_SURFACE_RAW: &str = "vmm-raw";

/// Visibility annotation key shared by stable host-visible tool descriptors.
/// 稳定宿主可见工具描述共用的可见性注解键。
const VMM_TOOL_VISIBILITY_ANNOTATION: &str = "visibility";

/// Optional-context annotation key used to describe trusted host context that improves one tool but is not strictly required.
/// 用于描述“能增强工具但不是硬前置”的受信任宿主上下文注解键。
const VMM_TOOL_OPTIONAL_CONTEXT_ANNOTATION: &str = "optional_context";

/// Public visibility marks tools that hosts may expose to the model by default.
/// public 可见性表示宿主可默认暴露给模型的工具。
const VMM_TOOL_VISIBILITY_PUBLIC: &str = "public";

/// Advanced visibility marks stable low-level helpers that are not ideal for every host's default surface.
/// advanced 可见性表示稳定但偏底层的辅助工具，不适合所有宿主的默认表面。
const VMM_TOOL_VISIBILITY_ADVANCED: &str = "advanced";

/// Admin visibility marks management tools that usually appear only when a host lacks a native control surface.
/// admin 可见性表示管理型工具，通常只在宿主缺少原生控制面时出现。
const VMM_TOOL_VISIBILITY_ADMIN: &str = "admin";

/// Agent optional-context hint marks tools that can derive a better target from trusted host agent context.
/// agent 可选上下文提示表示工具可从受信任宿主 agent 上下文推导更好的目标。
const VMM_TOOL_OPTIONAL_CONTEXT_AGENT: &str = "agent";

/// Metadata authority string returned for host-visible VMM binding/admin tools.
/// 返回给宿主插件用于 VMM 绑定与管理工具的元信息权威来源字符串。
const VMM_BINDING_TOOL_METADATA_SOURCE: &str = "vmm.host-binding-contract";

/// Metadata authority string returned for host-visible VMM profile-adjust tools.
/// 返回给宿主插件用于 VMM 画像调整工具的元信息权威来源字符串。
const VMM_PROFILE_TOOL_METADATA_SOURCE: &str = "vmm.host-profile-contract";

/// Registration-surface annotation key used by stable VMM binding descriptors.
/// 稳定 VMM 绑定描述使用的注册面注解键。
const VMM_BINDING_REGISTRATION_SURFACE: &str = "registration_surface";

/// Consolidated binding surface used by hosts that want one compact binding tool.
/// 希望使用单一精简绑定工具的宿主使用的聚合注册面取值。
const VMM_BINDING_SURFACE_CONSOLIDATED: &str = "host-binding-consolidated";

/// Legacy binding surface used by hosts that still materialize one tool per binding action.
/// 仍按单动作拆分绑定工具的宿主使用的旧式注册面取值。
const VMM_BINDING_SURFACE_LEGACY: &str = "host-binding-legacy";

/// Registration-surface annotation key used by stable VMM profile descriptors.
/// 稳定 VMM 画像描述使用的注册面注解键。
const VMM_PROFILE_REGISTRATION_SURFACE: &str = "registration_surface";

/// Consolidated profile-adjust surface used by hosts that want one natural-language profile correction tool.
/// 希望使用单一自然语言画像纠偏工具的宿主使用的聚合画像注册面取值。
const VMM_PROFILE_SURFACE_CONSOLIDATED: &str = "host-profile-adjust";

/// Optional visibility marks tools that are implemented and supported but should not be part of the default model-facing surface.
/// optional 可见性表示工具已实现且受支持，但不应进入默认面向模型的工具面。
const VMM_TOOL_VISIBILITY_OPTIONAL: &str = "optional";

/// Scope-level explanation aligned with the VMM gRPC integration contract.
/// 与 VMM gRPC 集成契约对齐的 scope_level 说明。
const MEMORY_SCOPE_LEVEL_DESCRIPTION: &str = "Scope of applicability. 1 = session for short-horizon working context, 2 = project for project-wide rules or knowledge, 3 = user for cross-project user-level preferences or standing facts, 0 or omit = let the backend choose its default scope; the current backend default is project.";

/// Priority explanation aligned with the VMM gRPC integration contract.
/// 与 VMM gRPC 集成契约对齐的 priority 说明。
const MEMORY_PRIORITY_DESCRIPTION: &str = "Recall importance. 1 = P0 critical, 2 = P1 important, 3 = P2 normal, 0 or omit = let the backend choose its default priority; the current backend default is P2. Priority answers how important it is to surface this memory again when relevant; it does not describe durability or abstraction.";

/// Memory-level explanation aligned with the VMM gRPC integration contract.
/// 与 VMM gRPC 集成契约对齐的 memory_level 说明。
const MEMORY_LEVEL_DESCRIPTION: &str = "Durability and abstraction tier. 1 = L0 one-off or short-horizon durable context, 2 = L1 reusable situational/project-operational memory, 3 = L2 stable project or domain knowledge, 4 = L3 foundational invariant or strong constraint, 0 or omit = let the backend derive a default level from scope. Memory level answers how durable and broadly reusable the memory is over time; it does not describe recall urgency.";

/// Category explanation aligned with the VMM PostAction prompt contract.
/// 与 VMM PostAction 提示词契约对齐的 category 说明。
const MEMORY_CATEGORY_DESCRIPTION: &str = "Memory category code. 0 = general, 1 = architecture_decision, 2 = tech_spec_api, 3 = business_logic, 4 = requirement_todo, 5 = project_context, 6 = logical_bug_debt, 7 = security_policy.";

/// Descriptor returned through the host adapter metadata endpoint.
/// 通过宿主适配器元信息端点返回的工具描述。
#[derive(Clone, Debug)]
pub struct VmmMemoryToolDescriptor {
    /// Canonical tool name exposed by host plugins.
    /// 宿主插件暴露的标准工具名称。
    pub name: String,
    /// Model-facing tool description with argument guidance.
    /// 面向模型并带参数指导的工具描述。
    pub description: String,
    /// JSON-encoded input schema for tool arguments.
    /// JSON 编码的工具入参 schema。
    pub input_schema_json: String,
    /// JSON-encoded annotations for diagnostics.
    /// JSON 编码的诊断注解。
    pub annotations_json: String,
    /// Metadata authority source for host diagnostics.
    /// 供宿主诊断使用的元信息权威来源。
    pub source: String,
}

/// Build all stable VMM memory tool descriptors.
/// 构建全部稳定的 VMM 记忆工具描述。
pub fn vmm_memory_tool_descriptors() -> Vec<VmmMemoryToolDescriptor> {
    vec![
        canonical_memory_search_descriptor(),
        canonical_memory_get_descriptor(),
        compat_memory_search_descriptor(),
        compat_memory_get_descriptor(),
        vmm_memory_search_descriptor(),
        vmm_turn_details_descriptor(),
        vmm_memory_write_descriptor(),
    ]
}

/// Build the legacy bridge search descriptor used by hosts that still need the canonical memory_search name.
/// 构建供仍需要 canonical memory_search 名称的宿主使用的旧式桥接搜索描述。
fn canonical_memory_search_descriptor() -> VmmMemoryToolDescriptor {
    let schema = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["query"],
        "properties": {
            "query": {
                "type": "string",
                "description": "Search query for prior project facts, source-turn context, user preferences, or decisions."
            },
            "maxResults": {
                "type": "number",
                "description": "Optional maximum hit count. Prefer 3 to 8 for targeted recall."
            },
            "minScore": {
                "type": "number",
                "description": "Optional minimum synthetic score threshold between 0 and 1."
            },
            "corpus": {
                "type": "string",
                "enum": ["memory", "sessions", "all", "wiki"],
                "description": "Restrict results to durable VMM memories or session-backed hits. `all` currently behaves like Vulcan memory-only recall, and `wiki` is unsupported."
            }
        }
    });
    let description = "Legacy bridge surface for hosts that still require the canonical `memory_search` name. Search durable VMM memories and session-backed source turns before answering when prior project facts, preferences, requirements, bugs, or decisions may matter. Hosts with a stronger Vulcan-native tool surface may choose to hide this bridge and prefer `vulcan_memory_search` instead.";
    build_memory_descriptor(
        "memory_search",
        description,
        schema,
        VMM_MEMORY_SURFACE_CANONICAL,
        VMM_TOOL_VISIBILITY_PUBLIC,
    )
}

/// Build the legacy bridge read descriptor used by hosts that still need the canonical memory_get name.
/// 构建供仍需要 canonical memory_get 名称的宿主使用的旧式桥接读取描述。
fn canonical_memory_get_descriptor() -> VmmMemoryToolDescriptor {
    let schema = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["path"],
        "properties": {
            "path": {
                "type": "string",
                "description": "Pseudo-path returned by memory_search, such as `vulcan-turns/123.md` or `vulcan-memories/456.md`."
            },
            "from": {
                "type": "number",
                "description": "Optional 1-based start line for paged reads."
            },
            "lines": {
                "type": "number",
                "description": "Optional line count for paged reads."
            },
            "corpus": {
                "type": "string",
                "enum": ["memory", "all", "wiki"],
                "description": "`wiki` is unsupported by Vulcan memory and will return an unavailable result."
            }
        }
    });
    let description = "Legacy bridge surface for hosts that still require the canonical `memory_get` name. Read one exact Vulcan memory pseudo-document returned by `memory_search`, including source turn documents and durable memory previews. Hosts with a stronger Vulcan-native tool surface may choose to hide this bridge and prefer `vulcan_memory_get` instead.";
    build_memory_descriptor(
        "memory_get",
        description,
        schema,
        VMM_MEMORY_SURFACE_CANONICAL,
        VMM_TOOL_VISIBILITY_PUBLIC,
    )
}

/// Build the primary Vulcan-native grouped search descriptor used by hosts that prefer explicit Vulcan memory tools.
/// 构建供偏好显式 Vulcan 记忆工具的宿主使用的主分组搜索描述。
fn compat_memory_search_descriptor() -> VmmMemoryToolDescriptor {
    let schema = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["queries"],
        "properties": {
            "queries": {
                "type": "array",
                "description": "Concrete memory search queries. Prefer 1-3 precise strings; broad scans may use more when triaging.",
                "items": { "type": "string" },
                "minItems": 1,
                "maxItems": MAX_MEMORY_SEARCH_QUERIES
            },
            "topK": {
                "type": "number",
                "description": "Optional maximum hit count per query. Prefer 3 to 8 for direct recall."
            }
        }
    });
    let description = "Primary Vulcan-native memory search surface for the current host runtime. Search durable Vulcan Memory Mesh memories when prior project facts, requirements, decisions, bugs, preferences, or durable context may matter. The grouped result format keeps raw hits, memory ids, category labels, and source_turn_id values available for precise follow-up inspection.";
    build_memory_descriptor(
        "vulcan_memory_search",
        description,
        schema,
        VMM_MEMORY_SURFACE_COMPAT,
        VMM_TOOL_VISIBILITY_PUBLIC,
    )
}

/// Build the primary Vulcan-native grouped read descriptor used by hosts that prefer explicit Vulcan memory tools.
/// 构建供偏好显式 Vulcan 记忆工具的宿主使用的主分组读取描述。
fn compat_memory_get_descriptor() -> VmmMemoryToolDescriptor {
    let schema = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["turnIds"],
        "properties": {
            "turnIds": {
                "type": "array",
                "description": "Decimal source_turn_id strings returned by vulcan_memory_search. Skip 0 because it has no source turn.",
                "items": { "type": "string" },
                "minItems": 1
            }
        }
    });
    let description = "Primary Vulcan-native follow-up reader for non-zero source_turn_id values returned by `vulcan_memory_search`. Use this when grouped search results point at one or more real source turns and you need structured turn details instead of pseudo-document reads.";
    build_memory_descriptor(
        "vulcan_memory_get",
        description,
        schema,
        VMM_MEMORY_SURFACE_COMPAT,
        VMM_TOOL_VISIBILITY_PUBLIC,
    )
}

/// Build all stable VMM binding/admin tool descriptors used by hosts without a native TUI.
/// 构建面向无原生 TUI 宿主的稳定 VMM 绑定与管理工具描述。
pub fn vmm_binding_tool_descriptors() -> Vec<VmmMemoryToolDescriptor> {
    vec![
        vulcan_bind_descriptor(),
        vmm_get_bindings_descriptor(),
        vmm_list_users_descriptor(),
        vmm_bind_default_user_descriptor(),
        vmm_list_projects_descriptor(),
        vmm_bind_default_project_descriptor(),
        vmm_bind_agent_project_descriptor(),
        vmm_clear_agent_project_descriptor(),
    ]
}

/// Build all stable VMM profile-adjust descriptors used by hosts that want one optional AI-facing correction tool.
/// 构建供希望使用可选 AI 画像纠偏工具的宿主使用的全部稳定 VMM 画像调整描述。
pub fn vmm_profile_tool_descriptors() -> Vec<VmmMemoryToolDescriptor> {
    vec![vulcan_profile_adjust_descriptor()]
}

/// Build the memory-search descriptor used by host plugins.
/// 构建宿主插件使用的记忆搜索工具描述。
fn vmm_memory_search_descriptor() -> VmmMemoryToolDescriptor {
    let schema = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["queries"],
        "properties": {
            "queries": {
                "type": "array",
                "minItems": 1,
                "maxItems": MAX_MEMORY_SEARCH_QUERIES,
                "description": "Simple query string list. Prefer 1-3 concrete queries for precision; max 16 is reserved for broad scans. The backend searches each query independently and echoes query_index plus query in the result.",
                "items": {
                    "type": "string",
                    "minLength": 1,
                    "description": "One complete search query. Prefer a concrete recall target such as a requirement, architecture decision, bug, preference, project rule, or API constraint."
                }
            },
            "topK": {
                "type": "integer",
                "minimum": 1,
                "maximum": MAX_MEMORY_SEARCH_TOP_K,
                "description": "Maximum hit count returned for each query. Prefer small values such as 3 to 8 unless broader recall is truly necessary."
            }
        }
    });
    let description = "Search durable memories for the current workspace. Use this before answering when you need stored facts, prior decisions, requirements, bugs, preferences, or long-lived project context. Prefer one to three concrete query strings. If a hit includes a non-zero source_turn_id, you may follow up with vmm_turn_details to inspect the original conversation behind that memory.\n\nInput parameters:\n- queries: Simple query string list. Prefer 1-3 concrete queries for precision; max 16 is reserved for broad scans.\n- topK: Optional maximum hit count per query. Prefer 3 to 8 unless broader recall is truly necessary.";
    build_memory_descriptor(
        "vmm_memory_search",
        description,
        schema,
        VMM_MEMORY_SURFACE_RAW,
        VMM_TOOL_VISIBILITY_ADVANCED,
    )
}

/// Build the turn-details descriptor used by host plugins.
/// 构建宿主插件使用的 turn 详情工具描述。
fn vmm_turn_details_descriptor() -> VmmMemoryToolDescriptor {
    let schema = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["turnIds"],
        "properties": {
            "turnIds": {
                "type": "array",
                "minItems": 1,
                "maxItems": 10,
                "description": "Ordered turn id list whose structured details should be loaded.",
                "items": {
                    "type": "string",
                    "pattern": "^[1-9][0-9]*$",
                    "description": "One decimal source_turn_id returned by vmm_memory_search. Skip hits whose source_turn_id is 0 because they have no source turn to inspect."
                }
            }
        }
    });
    let description = "Load structured turn details for source_turn_id values returned by vmm_memory_search. Use this only when a search hit has a non-zero source_turn_id and you need the original dialogue context behind that memory. This tool is for source-conversation inspection, not for broad discovery.\n\nInput parameters:\n- turnIds: Array of decimal source_turn_id strings returned by vmm_memory_search. Use values such as \"123\" and skip 0.";
    build_memory_descriptor(
        "vmm_turn_details",
        description,
        schema,
        VMM_MEMORY_SURFACE_RAW,
        VMM_TOOL_VISIBILITY_ADVANCED,
    )
}

/// Build the memory-write descriptor used by host plugins.
/// 构建宿主插件使用的记忆写入工具描述。
fn vmm_memory_write_descriptor() -> VmmMemoryToolDescriptor {
    let schema = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["items"],
        "properties": {
            "items": {
                "type": "array",
                "minItems": 1,
                "maxItems": MAX_MEMORY_WRITE_ITEMS,
                "description": "One or more atomic durable memory items to persist. Prefer small batches and keep each item focused on one stable fact, rule, decision, requirement, or known issue.",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["abstract", "details", "category"],
                    "properties": {
                        "abstract": {
                            "type": "string",
                            "minLength": 1,
                            "description": "Required short summary used for indexing, embedding, and quick recall. Do not leave it empty even when details are short."
                        },
                        "details": {
                            "type": "string",
                            "minLength": 1,
                            "description": "Required full durable memory text. Write the complete fact, rule, decision, requirement, or lasting issue here instead of only a fragment."
                        },
                        "category": {
                            "type": "integer",
                            "minimum": 0,
                            "maximum": 7,
                            "description": MEMORY_CATEGORY_DESCRIPTION
                        },
                        "scopeLevel": {
                            "type": "integer",
                            "minimum": 0,
                            "maximum": 3,
                            "description": MEMORY_SCOPE_LEVEL_DESCRIPTION
                        },
                        "priority": {
                            "type": "integer",
                            "minimum": 0,
                            "maximum": 3,
                            "description": MEMORY_PRIORITY_DESCRIPTION
                        },
                        "memoryLevel": {
                            "type": "integer",
                            "minimum": 0,
                            "maximum": 4,
                            "description": MEMORY_LEVEL_DESCRIPTION
                        }
                    }
                }
            }
        }
    });
    let description = format!(
        "Write durable memories for the current workspace. Use this only for stable facts, confirmed constraints, architecture decisions, durable requirements, reusable project context, or known lasting bugs and debt. Do not write profile/persona information here, including user/project/team/space profile facts, behavioral preferences, traits, standing instructions, or long-lived identity/context rules; VMM handles profile extraction, review, refresh, and injection automatically through the profile pipeline. Do not use it for temporary status, one-off logs, transient errors, raw brainstorming fragments, or unconfirmed guesses. Each item should be atomic, future-reusable, and worth remembering across later tasks.\n\nInput parameters:\n- items: Array of atomic durable memory items, max {MAX_MEMORY_WRITE_ITEMS}.\n- items[].abstract: Required short summary for indexing and quick recall.\n- items[].details: Required full durable memory text.\n- items[].category: {MEMORY_CATEGORY_DESCRIPTION}\n- items[].scopeLevel: {MEMORY_SCOPE_LEVEL_DESCRIPTION}\n- items[].priority: {MEMORY_PRIORITY_DESCRIPTION}\n- items[].memoryLevel: {MEMORY_LEVEL_DESCRIPTION}"
    );
    build_memory_descriptor(
        "vmm_memory_write",
        &description,
        schema,
        VMM_MEMORY_SURFACE_RAW,
        VMM_TOOL_VISIBILITY_ADVANCED,
    )
}

/// Convert one memory-tool metadata payload plus one registration surface into a transport descriptor.
/// 将一条记忆工具元信息与注册面取值转换成传输描述对象。
fn build_memory_descriptor(
    name: &str,
    description: &str,
    input_schema: Value,
    registration_surface: &str,
    visibility: &str,
) -> VmmMemoryToolDescriptor {
    build_descriptor_with_annotations(
        name,
        description,
        input_schema,
        json!({
            "source": VMM_MEMORY_TOOL_METADATA_SOURCE,
            "stable": true,
            "schema_version": 1,
            "tool_group": VMM_MEMORY_TOOL_GROUP,
            "execution_mode": VMM_MEMORY_EXECUTION_MODE,
            VMM_TOOL_VISIBILITY_ANNOTATION: visibility,
            VMM_MEMORY_REGISTRATION_SURFACE: registration_surface,
        }),
        VMM_MEMORY_TOOL_METADATA_SOURCE,
    )
}

/// Convert one tool metadata payload plus explicit annotations into a transport descriptor.
/// 将工具元信息与显式注解转换成一个可传输的描述对象。
fn build_descriptor_with_annotations(
    name: &str,
    description: &str,
    input_schema: Value,
    annotations: Value,
    source: &str,
) -> VmmMemoryToolDescriptor {
    VmmMemoryToolDescriptor {
        name: name.to_string(),
        description: description.to_string(),
        input_schema_json: compact_json_string(&input_schema),
        annotations_json: compact_json_string(&annotations),
        source: source.to_string(),
    }
}

/// Build one binding/admin descriptor with a declared execution mode for host plugins.
/// 构建一条带执行模式声明的绑定或管理工具描述。
fn build_binding_descriptor(
    name: &str,
    description: &str,
    input_schema: Value,
    execution_mode: &str,
    registration_surface: &str,
    optional_context: Option<&[&str]>,
) -> VmmMemoryToolDescriptor {
    let mut annotations = json!({
        "source": VMM_BINDING_TOOL_METADATA_SOURCE,
        "stable": true,
        "schema_version": 1,
        "tool_group": "vmm-binding",
        "execution_mode": execution_mode,
        VMM_BINDING_REGISTRATION_SURFACE: registration_surface,
        VMM_TOOL_VISIBILITY_ANNOTATION: VMM_TOOL_VISIBILITY_ADMIN,
    });
    if let Some(context_items) = optional_context {
        annotations[VMM_TOOL_OPTIONAL_CONTEXT_ANNOTATION] = json!(context_items);
    }
    build_descriptor_with_annotations(
        name,
        description,
        input_schema,
        annotations,
        VMM_BINDING_TOOL_METADATA_SOURCE,
    )
}

/// Build one profile-adjust descriptor with explicit execution mode and optional trusted-context hints.
/// 构建一条带显式执行模式与可选受信任上下文提示的画像调整工具描述。
fn build_profile_descriptor(
    name: &str,
    description: &str,
    input_schema: Value,
    execution_mode: &str,
    registration_surface: &str,
    optional_context: Option<&[&str]>,
) -> VmmMemoryToolDescriptor {
    let mut annotations = json!({
        "source": VMM_PROFILE_TOOL_METADATA_SOURCE,
        "stable": true,
        "schema_version": 1,
        "tool_group": "vmm-profile",
        "execution_mode": execution_mode,
        VMM_PROFILE_REGISTRATION_SURFACE: registration_surface,
        VMM_TOOL_VISIBILITY_ANNOTATION: VMM_TOOL_VISIBILITY_OPTIONAL,
    });
    if let Some(context_items) = optional_context {
        annotations[VMM_TOOL_OPTIONAL_CONTEXT_ANNOTATION] = json!(context_items);
    }
    build_descriptor_with_annotations(
        name,
        description,
        input_schema,
        annotations,
        VMM_PROFILE_TOOL_METADATA_SOURCE,
    )
}

/// Build the compact host-facing binding tool descriptor used by hosts that want one parameterized binding surface.
/// 构建供希望使用单一参数化绑定表面的宿主使用的精简宿主绑定工具描述。
fn vulcan_bind_descriptor() -> VmmMemoryToolDescriptor {
    let schema = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["action", "resource"],
        "properties": {
            "action": {
                "type": "string",
                "enum": ["inspect", "list", "bind", "clear"],
                "description": "Binding operation. inspect returns the current effective binding state, list returns durable VMM identities, bind persists one host binding target, and clear removes one per-agent project override."
            },
            "resource": {
                "type": "string",
                "enum": ["bindings", "user", "project"],
                "description": "Binding resource. Use bindings with inspect, user or project with list/bind, and project with clear."
            },
            "scope": {
                "type": "string",
                "enum": ["global", "agent"],
                "description": "Binding scope. global updates the shared default host binding, while agent updates or clears one main-agent project override."
            },
            "ref": {
                "type": "string",
                "minLength": 1,
                "description": "Existing numeric user_id/project_id, durable user name, or canonical Team/Space/Project path depending on the selected resource."
            },
            "agentId": {
                "type": "string",
                "description": "Optional host main-agent id. Omit to reuse the current trusted main-agent context when the host provides one."
            },
            "createIfMissing": {
                "type": "boolean",
                "description": "Whether the host may ask VMM to create a missing durable user name or canonical Team/Space/Project path while binding."
            }
        }
    });
    let description = "Inspect, list, bind, or clear host-level VMM user/project bindings through one compact management surface. Use this when the host does not have an OpenCode-style TUI and you still need to choose a shared default user_id/project_id, inspect the active binding state, or assign one main agent to a dedicated project.\n\nInput parameters:\n- action: inspect | list | bind | clear.\n- resource: bindings | user | project.\n- scope: global | agent. Use global for shared defaults and agent for one main-agent project override.\n- ref: Existing numeric user_id/project_id, durable user name, or canonical Team/Space/Project path depending on resource.\n- agentId: Optional main-agent id for inspect, bind(scope=agent), or clear.\n- createIfMissing: Optional boolean that only applies to bind and only when the selected ref can be created safely.";
    build_binding_descriptor(
        "vulcan_bind",
        description,
        schema,
        "hybrid",
        VMM_BINDING_SURFACE_CONSOLIDATED,
        Some(&[VMM_TOOL_OPTIONAL_CONTEXT_AGENT]),
    )
}

/// Build the local binding-inspection descriptor used by hosts without an OpenCode-style TUI.
/// 构建面向无 OpenCode 风格 TUI 宿主的本地绑定查看工具描述。
fn vmm_get_bindings_descriptor() -> VmmMemoryToolDescriptor {
    let schema = json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "agentId": {
                "type": "string",
                "description": "Optional host main-agent id to inspect. Omit to inspect the current trusted agent when one is available."
            }
        }
    });
    let description = "Inspect the effective VMM user/project bindings used by the current host adapter. Use this when you need to confirm the shared default user_id/project_id, inspect whether one main agent has a dedicated project override, or debug which binding source currently wins. This is a host-level binding inspection tool, not a memory recall tool.\n\nInput parameters:\n- agentId: Optional host main-agent id to inspect. Omit to inspect the current trusted agent when one is available.";
    build_binding_descriptor(
        "vulcan_vmm_get_bindings",
        description,
        schema,
        "local",
        VMM_BINDING_SURFACE_LEGACY,
        Some(&[VMM_TOOL_OPTIONAL_CONTEXT_AGENT]),
    )
}

/// Build the remote durable-user listing descriptor used by no-TUI hosts.
/// 构建供无 TUI 宿主使用的远程长期用户列表工具描述。
fn vmm_list_users_descriptor() -> VmmMemoryToolDescriptor {
    let schema = json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {}
    });
    let description = "List durable VMM users so the host can bind one real numeric user_id instead of guessing identity. Use this before choosing or switching the shared default user binding.";
    build_binding_descriptor(
        "vulcan_vmm_list_users",
        description,
        schema,
        "remote",
        VMM_BINDING_SURFACE_LEGACY,
        None,
    )
}

/// Build the hybrid default-user binding descriptor used by hosts that must persist bindings locally.
/// 构建供需要本地持久化绑定的宿主使用的混合型默认用户绑定工具描述。
fn vmm_bind_default_user_descriptor() -> VmmMemoryToolDescriptor {
    let schema = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["userRef"],
        "properties": {
            "userRef": {
                "type": "string",
                "minLength": 1,
                "description": "Existing numeric user id or durable user name. When createIfMissing=true and the name does not exist, VMM will create it."
            },
            "createIfMissing": {
                "type": "boolean",
                "description": "Whether the host may request VMM to create the user when userRef is a missing durable user name."
            }
        }
    });
    let description = "Resolve or create one durable VMM user, then persist its real numeric user_id as the host's shared default user binding. Use this when the host does not provide an OpenCode-style TUI and you need to manage default user selection through tools instead.\n\nInput parameters:\n- userRef: Existing numeric user id or durable user name.\n- createIfMissing: Optional boolean. Set true only when the host is allowed to create a missing durable user name.";
    build_binding_descriptor(
        "vulcan_vmm_bind_default_user",
        description,
        schema,
        "hybrid",
        VMM_BINDING_SURFACE_LEGACY,
        None,
    )
}

/// Build the remote project-listing descriptor used by no-TUI hosts.
/// 构建供无 TUI 宿主使用的远程项目列表工具描述。
fn vmm_list_projects_descriptor() -> VmmMemoryToolDescriptor {
    let schema = json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {}
    });
    let description = "List durable VMM Team/Space/Project entries so the host can bind one real numeric project_id or canonical display path before enabling memory workflows.";
    build_binding_descriptor(
        "vulcan_vmm_list_projects",
        description,
        schema,
        "remote",
        VMM_BINDING_SURFACE_LEGACY,
        None,
    )
}

/// Build the hybrid default-project binding descriptor used by hosts that persist project bindings locally.
/// 构建供本地持久化项目绑定的宿主使用的混合型默认项目绑定工具描述。
fn vmm_bind_default_project_descriptor() -> VmmMemoryToolDescriptor {
    let schema = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["projectRef"],
        "properties": {
            "projectRef": {
                "type": "string",
                "minLength": 1,
                "description": "Existing numeric project id or canonical Team/Space/Project path. Creation requires a canonical Team/Space/Project path."
            },
            "createIfMissing": {
                "type": "boolean",
                "description": "Whether the host may request VMM to create the project when projectRef is a missing canonical Team/Space/Project path."
            }
        }
    });
    let description = "Resolve or create one durable VMM project, then persist its real numeric project_id as the host's shared default project binding. Use this when a host without an OpenCode-style TUI needs to manage its shared default project through tools.\n\nInput parameters:\n- projectRef: Existing numeric project id or canonical Team/Space/Project path.\n- createIfMissing: Optional boolean. Set true only when the host is allowed to create a missing canonical Team/Space/Project path.";
    build_binding_descriptor(
        "vulcan_vmm_bind_default_project",
        description,
        schema,
        "hybrid",
        VMM_BINDING_SURFACE_LEGACY,
        None,
    )
}

/// Build the hybrid per-agent project binding descriptor used by hosts that support one project override per main agent.
/// 构建供支持主 agent 单独项目覆盖的宿主使用的混合型按 agent 项目绑定工具描述。
fn vmm_bind_agent_project_descriptor() -> VmmMemoryToolDescriptor {
    let schema = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["projectRef"],
        "properties": {
            "agentId": {
                "type": "string",
                "description": "Optional host main-agent id to override. Omit to use the current trusted main agent id when one is available."
            },
            "projectRef": {
                "type": "string",
                "minLength": 1,
                "description": "Existing numeric project id or canonical Team/Space/Project path. Creation requires a canonical Team/Space/Project path."
            },
            "createIfMissing": {
                "type": "boolean",
                "description": "Whether the host may request VMM to create the project when projectRef is a missing canonical Team/Space/Project path."
            }
        }
    });
    let description = "Bind one host main agent to a dedicated VMM project_id while keeping the shared default project untouched. Use this when one agent needs an isolated project binding and all unconfigured agents should still fall back to the shared default project.\n\nInput parameters:\n- agentId: Optional main-agent id to override.\n- projectRef: Existing numeric project id or canonical Team/Space/Project path.\n- createIfMissing: Optional boolean. Set true only when the host is allowed to create a missing canonical Team/Space/Project path.";
    build_binding_descriptor(
        "vulcan_vmm_bind_agent_project",
        description,
        schema,
        "hybrid",
        VMM_BINDING_SURFACE_LEGACY,
        Some(&[VMM_TOOL_OPTIONAL_CONTEXT_AGENT]),
    )
}

/// Build the local per-agent project-clear descriptor used by hosts that persist overrides locally.
/// 构建供本地持久化覆盖关系的宿主使用的本地按 agent 清除项目覆盖工具描述。
fn vmm_clear_agent_project_descriptor() -> VmmMemoryToolDescriptor {
    let schema = json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "agentId": {
                "type": "string",
                "description": "Optional host main-agent id whose dedicated project override should be cleared. Omit to use the current trusted main agent id when one is available."
            }
        }
    });
    let description = "Clear one host main-agent project override so that agent falls back to the shared default project binding again. Use this when a dedicated per-agent project is no longer needed.";
    build_binding_descriptor(
        "vulcan_vmm_clear_agent_project",
        description,
        schema,
        "local",
        VMM_BINDING_SURFACE_LEGACY,
        Some(&[VMM_TOOL_OPTIONAL_CONTEXT_AGENT]),
    )
}

/// Build the optional natural-language profile-adjust descriptor used by hosts that want AI-driven profile correction without a full management center.
/// 构建供希望使用 AI 驱动画像纠偏、但不引入完整管理中心的宿主使用的可选自然语言画像调整描述。
fn vulcan_profile_adjust_descriptor() -> VmmMemoryToolDescriptor {
    let schema = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["scope", "instruction"],
        "properties": {
            "scope": {
                "type": "string",
                "enum": ["user", "project", "team", "space"],
                "description": "Profile scope to adjust. user and project target the current bound identities directly; team and space reuse the current project binding lineage."
            },
            "instruction": {
                "type": "string",
                "minLength": 1,
                "description": "Explicit natural-language correction or addition for the selected long-lived profile. Use this only when the user clearly asks to correct, reinforce, remove, or add durable profile information."
            }
        }
    });
    let description = "Adjust one durable VMM profile with an explicit natural-language instruction. The system already performs automatic profile extraction and refresh, so use this tool only when the user clearly asks to correct, reinforce, remove, or add long-lived profile information. Do not use it for ordinary temporary context, one-off status updates, or guesses.\\n\\nInput parameters:\\n- scope: user | project | team | space.\\n- instruction: Explicit natural-language profile adjustment for the selected scope.";
    build_profile_descriptor(
        "vulcan_profile_adjust",
        description,
        schema,
        "remote",
        VMM_PROFILE_SURFACE_CONSOLIDATED,
        Some(&[VMM_TOOL_OPTIONAL_CONTEXT_AGENT]),
    )
}

/// Serialize one JSON value for transport, falling back to an empty object.
/// 序列化一段传输用 JSON，失败时回退为空对象。
fn compact_json_string(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify the public descriptor list keeps the stable tool ids.
    /// 验证公开描述列表保持稳定工具标识。
    #[test]
    fn vmm_memory_tool_metadata_lists_stable_tools() {
        let tools = vmm_memory_tool_descriptors();
        let names = tools
            .iter()
            .map(|tool| tool.name.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            names,
            vec![
                "memory_search",
                "memory_get",
                "vulcan_memory_search",
                "vulcan_memory_get",
                "vmm_memory_search",
                "vmm_turn_details",
                "vmm_memory_write"
            ]
        );
    }

    /// Verify write metadata exposes the enum guidance needed by host plugins.
    /// 验证写入元信息暴露宿主插件所需的枚举说明。
    #[test]
    fn vmm_memory_write_metadata_carries_enum_guidance() {
        let tools = vmm_memory_tool_descriptors();
        let write_tool = tools
            .iter()
            .find(|tool| tool.name == "vmm_memory_write")
            .expect("write descriptor should exist");
        let schema: Value = serde_json::from_str(&write_tool.input_schema_json)
            .expect("schema should be valid JSON");
        let category_description = schema["properties"]["items"]["items"]["properties"]["category"]
            ["description"]
            .as_str()
            .unwrap_or_default();
        let priority_description = schema["properties"]["items"]["items"]["properties"]["priority"]
            ["description"]
            .as_str()
            .unwrap_or_default();

        assert!(write_tool.description.contains("items[].memoryLevel"));
        assert!(category_description.contains("7 = security_policy"));
        assert!(priority_description.contains("1 = P0"));
    }

    /// Verify canonical memory descriptors expose the host-facing names and common annotations.
    /// 验证 canonical 记忆描述暴露宿主可见名称与通用注解。
    #[test]
    fn canonical_memory_metadata_carries_host_visible_annotations() {
        let tools = vmm_memory_tool_descriptors();
        let search_tool = tools
            .iter()
            .find(|tool| tool.name == "memory_search")
            .expect("canonical memory_search descriptor should exist");
        let annotations: Value = serde_json::from_str(&search_tool.annotations_json)
            .expect("annotations should be valid JSON");
        let schema: Value = serde_json::from_str(&search_tool.input_schema_json)
            .expect("schema should be valid JSON");

        assert_eq!(annotations["tool_group"].as_str(), Some("vmm-memory"));
        assert_eq!(annotations["execution_mode"].as_str(), Some("remote"));
        assert_eq!(annotations["visibility"].as_str(), Some("public"));
        assert_eq!(
            annotations["registration_surface"].as_str(),
            Some("host-memory-canonical")
        );
        assert_eq!(
            schema["properties"]["query"]["type"].as_str(),
            Some("string")
        );
    }

    /// Verify raw and compat descriptors keep distinct registration surfaces so hosts can derive manifests without hard-coded tool names.
    /// 验证原始与兼容 descriptor 保持不同注册面取值，让宿主无需硬编码工具名也能推导 manifest。
    #[test]
    fn memory_descriptor_registration_surfaces_stay_distinct() {
        let tools = vmm_memory_tool_descriptors();
        let compat_tool = tools
            .iter()
            .find(|tool| tool.name == "vulcan_memory_search")
            .expect("compat memory_search descriptor should exist");
        let raw_tool = tools
            .iter()
            .find(|tool| tool.name == "vmm_memory_search")
            .expect("raw vmm_memory_search descriptor should exist");
        let compat_annotations: Value = serde_json::from_str(&compat_tool.annotations_json)
            .expect("compat annotations should be valid JSON");
        let raw_annotations: Value = serde_json::from_str(&raw_tool.annotations_json)
            .expect("raw annotations should be valid JSON");

        assert_eq!(
            compat_annotations["registration_surface"].as_str(),
            Some("host-memory-compat")
        );
        assert_eq!(compat_annotations["visibility"].as_str(), Some("public"));
        assert_eq!(
            raw_annotations["registration_surface"].as_str(),
            Some("vmm-raw")
        );
        assert_eq!(raw_annotations["visibility"].as_str(), Some("advanced"));
    }

    /// Verify binding/admin metadata keeps stable tool ids and execution-mode hints for host wrappers.
    /// 验证绑定与管理工具元信息保持稳定工具标识与宿主包装器所需的执行模式提示。
    #[test]
    fn vmm_binding_tool_metadata_lists_stable_tools() {
        let tools = vmm_binding_tool_descriptors();
        let names = tools
            .iter()
            .map(|tool| tool.name.as_str())
            .collect::<Vec<_>>();
        let compact_bind_tool = tools
            .iter()
            .find(|tool| tool.name == "vulcan_bind")
            .expect("compact bind descriptor should exist");
        let bind_agent_tool = tools
            .iter()
            .find(|tool| tool.name == "vulcan_vmm_bind_agent_project")
            .expect("bind-agent descriptor should exist");
        let compact_annotations: Value = serde_json::from_str(&compact_bind_tool.annotations_json)
            .expect("compact bind annotations should be valid JSON");
        let annotations: Value = serde_json::from_str(&bind_agent_tool.annotations_json)
            .expect("annotations should be valid JSON");

        assert_eq!(
            names,
            vec![
                "vulcan_bind",
                "vulcan_vmm_get_bindings",
                "vulcan_vmm_list_users",
                "vulcan_vmm_bind_default_user",
                "vulcan_vmm_list_projects",
                "vulcan_vmm_bind_default_project",
                "vulcan_vmm_bind_agent_project",
                "vulcan_vmm_clear_agent_project"
            ]
        );
        assert_eq!(
            compact_annotations["execution_mode"].as_str(),
            Some("hybrid")
        );
        assert_eq!(
            compact_annotations["registration_surface"].as_str(),
            Some("host-binding-consolidated")
        );
        assert_eq!(annotations["execution_mode"].as_str(), Some("hybrid"));
        assert_eq!(annotations["visibility"].as_str(), Some("admin"));
        assert_eq!(
            annotations["registration_surface"].as_str(),
            Some("host-binding-legacy")
        );
        assert_eq!(annotations["optional_context"][0].as_str(), Some("agent"));
    }

    /// Verify profile-adjust metadata keeps one stable tool id plus the expected profile annotations.
    /// 验证画像调整元信息保持稳定工具标识以及预期画像注解。
    #[test]
    fn vmm_profile_tool_metadata_lists_stable_tools() {
        let tools = vmm_profile_tool_descriptors();
        let names = tools
            .iter()
            .map(|tool| tool.name.as_str())
            .collect::<Vec<_>>();
        let descriptor = tools
            .iter()
            .find(|tool| tool.name == "vulcan_profile_adjust")
            .expect("profile adjust descriptor should exist");
        let annotations: Value = serde_json::from_str(&descriptor.annotations_json)
            .expect("profile annotations should be valid JSON");

        assert_eq!(names, vec!["vulcan_profile_adjust"]);
        assert_eq!(annotations["tool_group"].as_str(), Some("vmm-profile"));
        assert_eq!(annotations["execution_mode"].as_str(), Some("remote"));
        assert_eq!(annotations["visibility"].as_str(), Some("optional"));
        assert_eq!(
            annotations["registration_surface"].as_str(),
            Some("host-profile-adjust")
        );
    }
}
