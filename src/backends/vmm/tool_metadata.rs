//! Stable host-facing metadata for VMM memory tools.
//! VMM 记忆工具的稳定宿主可见元信息。
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
        vmm_memory_search_descriptor(),
        vmm_turn_details_descriptor(),
        vmm_memory_write_descriptor(),
    ]
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
    build_descriptor("vmm_memory_search", description, schema)
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
    build_descriptor("vmm_turn_details", description, schema)
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
    build_descriptor("vmm_memory_write", &description, schema)
}

/// Convert a tool name, description, and JSON schema into a descriptor.
/// 将工具名、说明与 JSON schema 转换成描述对象。
fn build_descriptor(name: &str, description: &str, input_schema: Value) -> VmmMemoryToolDescriptor {
    let annotations = json!({
        "source": VMM_MEMORY_TOOL_METADATA_SOURCE,
        "stable": true,
        "schema_version": 1
    });
    VmmMemoryToolDescriptor {
        name: name.to_string(),
        description: description.to_string(),
        input_schema_json: compact_json_string(&input_schema),
        annotations_json: compact_json_string(&annotations),
        source: VMM_MEMORY_TOOL_METADATA_SOURCE.to_string(),
    }
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
            vec!["vmm_memory_search", "vmm_turn_details", "vmm_memory_write"]
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
}
