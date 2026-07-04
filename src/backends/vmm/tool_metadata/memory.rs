use super::*;

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
        vmm_memory_delete_descriptor(),
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

/// Build the memory-delete descriptor used by host plugins for explicit memory replacement flows.
/// 构建宿主插件用于明确记忆替换流程的记忆删除工具描述。
fn vmm_memory_delete_descriptor() -> VmmMemoryToolDescriptor {
    let schema = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["memoryIds", "reason"],
        "properties": {
            "memoryIds": {
                "type": "array",
                "minItems": 1,
                "maxItems": MAX_MEMORY_DELETE_ITEMS,
                "description": "Explicit durable memory ids to delete after the user clearly asks to delete, replace, or remove those memories. Use exact memory_id values returned by vmm_memory_search, vulcan_memory_search, or PreCheck VMM_ID context items; never use turn_id/source_turn_id and never guess ids.",
                "items": {
                    "type": "string",
                    "pattern": "^[1-9][0-9]*$",
                    "description": "One decimal memory_id that belongs to the current resolved user/project scope."
                }
            },
            "reason": {
                "type": "string",
                "minLength": 1,
                "description": "Brief durable reason for deletion, such as replaced_by_newer_memory, user_requested_correction, stale_requirement, or invalid_fact."
            }
        }
    });
    let description = format!(
        "Delete explicit durable memories for the current workspace. Use this only after the user clearly instructs you to delete, remove, or replace specific remembered information, and only when the exact memory_id is available. Deletion must target memory_id values returned by vmm_memory_search, vulcan_memory_search, or PreCheck VMM_ID context items; never delete by turn_id/source_turn_id, never infer ids from text, and never use this for broad cleanup, temporary recall filtering, profile/persona changes, or guessed ids. Prefer writing a replacement with vmm_memory_write after deletion when the user is correcting a durable fact. VMM profile data is managed by the profile pipeline.\\n\\nInput parameters:\\n- memoryIds: Array of explicit decimal memory_id strings, max {MAX_MEMORY_DELETE_ITEMS}.\\n- reason: Required short deletion reason for audit and later debugging."
    );
    build_memory_descriptor(
        "vmm_memory_delete",
        &description,
        schema,
        VMM_MEMORY_SURFACE_RAW,
        VMM_TOOL_VISIBILITY_ADVANCED,
    )
}
