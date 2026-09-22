use crate::host_core::model::{RuntimeInputSchema, RuntimeToolAnnotations, RuntimeToolDescriptor};
use luaskills::RuntimeEntryDescriptor;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

/// Host-managed LuaSkills session identity field hidden from supported clients.
/// 在受支持客户端中被隐藏的宿主管理 LuaSkills 会话身份字段。
pub const HOST_MANAGED_LUASKILL_SID_FIELD: &str = "LUASKILL_SID";

/// Host-managed LuaSkills session identity prefix reserved by the cross-host contract.
/// 由跨宿主契约保留的宿主管理 LuaSkills 会话身份前缀。
pub const HOST_MANAGED_LUASKILL_SID_PREFIX: &str = "LUASKILLS-SID-";

/// Projection options applied while exposing one LuaSkills tool to a host surface.
/// 向宿主表面暴露单个 LuaSkills 工具时应用的投影选项。
#[derive(Debug, Clone, Default)]
pub struct LuaSkillToolProjectionOptions {
    /// Whether LUASKILL_SID should be hidden from the AI-facing schema.
    /// 是否应从面向 AI 的 schema 中隐藏 LUASKILL_SID。
    pub hide_managed_luaskill_sid: bool,
}

/// Map one generic runtime entry descriptor into the runtime tool descriptor exposed by host core.
/// 把一份通用运行时入口描述映射为 host core 暴露的运行时工具描述。
pub fn map_runtime_entry_to_mcp_tool(entry: &RuntimeEntryDescriptor) -> RuntimeToolDescriptor {
    map_runtime_entry_to_mcp_tool_with_projection(entry, &LuaSkillToolProjectionOptions::default())
}

/// Map one runtime entry descriptor while applying host projection policy for AI-facing schemas.
/// 在面向 AI 的 schema 上应用宿主投影策略后映射一份运行时入口描述。
pub fn map_runtime_entry_to_mcp_tool_with_projection(
    entry: &RuntimeEntryDescriptor,
    projection: &LuaSkillToolProjectionOptions,
) -> RuntimeToolDescriptor {
    let input_schema = build_runtime_input_schema(entry, projection);

    // Reuse the normalized entry description exported by LuaSkills instead of rebuilding tool copy inside the host.
    // 直接复用 LuaSkills 导出的规范化入口说明，而不是在宿主侧重新拼装工具文案。
    RuntimeToolDescriptor {
        name: entry.canonical_name.clone(),
        description: Some(entry.description.clone()),
        input_schema,
        annotations: Some(RuntimeToolAnnotations {
            read_only_hint: Some(true),
            destructive_hint: Some(false),
            user_confirmation_required: Some(false),
            idempotent_hint: Some(true),
        }),
    }
}

/// Build one host-neutral input schema by preferring the LuaSkills 0.5.7 exported AI-facing schema and using its parameter descriptors when the schema is absent.
/// 优先使用 LuaSkills 0.5.7 导出的 AI-facing schema 构建宿主中立输入 schema，并在 schema 缺失时使用其参数描述。
fn build_runtime_input_schema(
    entry: &RuntimeEntryDescriptor,
    projection: &LuaSkillToolProjectionOptions,
) -> RuntimeInputSchema {
    if let Some(schema_object) = entry.input_schema.as_object() {
        let schema_type = schema_object
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("object")
            .to_string();
        let properties = schema_object.get("properties").cloned();
        let required = schema_object.get("required").and_then(|value| {
            value.as_array().map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
        });
        if properties.is_some() || required.is_some() || schema_object.contains_key("type") {
            let mut schema = RuntimeInputSchema {
                schema_type,
                properties,
                required,
            };
            if projection.hide_managed_luaskill_sid {
                remove_managed_luaskill_sid_from_schema(&mut schema);
            }
            return schema;
        }
    }

    build_runtime_input_schema_from_parameters(entry, projection)
}

/// Build one input schema from exported entry parameter descriptors when the AI-facing schema is absent.
/// 当导出的 AI-facing schema 缺失时，根据入口参数描述构建输入 schema。
fn build_runtime_input_schema_from_parameters(
    entry: &RuntimeEntryDescriptor,
    projection: &LuaSkillToolProjectionOptions,
) -> RuntimeInputSchema {
    let mut props = Map::new();
    let mut required = Vec::new();
    for parameter in &entry.parameters {
        // Hide host-managed LUASKILL_SID from capable clients so the model never has to supply it manually.
        // 对具备能力的客户端隐藏宿主管理的 LUASKILL_SID，确保模型不需要手动提供该字段。
        if projection.hide_managed_luaskill_sid && parameter.name == HOST_MANAGED_LUASKILL_SID_FIELD
        {
            continue;
        }
        // Preserve LuaSkills-authored parameter descriptions verbatim so upstream normalization fixes flow through to host protocols unchanged.
        // 原样保留 LuaSkills 产出的参数说明文本，让上游规范化修复可以无损传递到宿主协议层。
        props.insert(
            parameter.name.clone(),
            json!({
                "type": parameter.param_type,
                "description": parameter.description
            }),
        );
        if parameter.required {
            required.push(parameter.name.clone());
        }
    }

    RuntimeInputSchema {
        schema_type: "object".to_string(),
        properties: Some(Value::Object(props)),
        required: Some(required),
    }
}

/// Remove the managed LUASKILL_SID field from one host-neutral schema after host-side projection.
/// 在宿主侧投影后，从宿主中立 schema 中移除受管 LUASKILL_SID 字段。
fn remove_managed_luaskill_sid_from_schema(schema: &mut RuntimeInputSchema) {
    if let Some(properties) = schema.properties.as_mut().and_then(Value::as_object_mut) {
        properties.remove(HOST_MANAGED_LUASKILL_SID_FIELD);
    }
    if let Some(required) = schema.required.as_mut() {
        required.retain(|name| name != HOST_MANAGED_LUASKILL_SID_FIELD);
        if required.is_empty() {
            schema.required = None;
        }
    }
}

/// Return true when one runtime tool schema still contains the host-managed LUASKILL_SID field.
/// 当某个运行时工具 schema 仍包含宿主管理的 LUASKILL_SID 字段时返回 true。
pub fn runtime_tool_uses_managed_luaskill_sid(tool: &RuntimeToolDescriptor) -> bool {
    tool.input_schema
        .properties
        .as_ref()
        .and_then(Value::as_object)
        .is_some_and(|properties| properties.contains_key(HOST_MANAGED_LUASKILL_SID_FIELD))
        || tool.input_schema.required.as_ref().is_some_and(|required| {
            required
                .iter()
                .any(|name| name == HOST_MANAGED_LUASKILL_SID_FIELD)
        })
}

/// Project one runtime tool descriptor for a session-capable host without mutating registry state.
/// 为支持会话托管的宿主投影一份运行时工具描述，但不修改注册表状态。
pub fn project_runtime_tool_descriptor(
    tool: &RuntimeToolDescriptor,
    projection: &LuaSkillToolProjectionOptions,
) -> RuntimeToolDescriptor {
    if !projection.hide_managed_luaskill_sid || !runtime_tool_uses_managed_luaskill_sid(tool) {
        return tool.clone();
    }

    let mut projected = tool.clone();

    // Remove the host-managed field from visible JSON schema properties so model-facing hosts see the simplified contract.
    // 从可见 JSON schema properties 中移除宿主管理字段，让面向模型的宿主看到简化后的契约。
    if let Some(properties) = projected
        .input_schema
        .properties
        .as_mut()
        .and_then(Value::as_object_mut)
    {
        properties.remove(HOST_MANAGED_LUASKILL_SID_FIELD);
    }

    // Also remove LUASKILL_SID from required so host-managed tools remain callable after projection.
    // 同时把 LUASKILL_SID 从 required 中移除，确保托管后的工具仍然可调用。
    if let Some(required) = projected.input_schema.required.as_mut() {
        required.retain(|name| name != HOST_MANAGED_LUASKILL_SID_FIELD);
    }
    if projected
        .input_schema
        .required
        .as_ref()
        .is_some_and(|required| required.is_empty())
    {
        projected.input_schema.required = None;
    }

    projected
}

/// Inject the trusted session identity into LUASKILL_SID when the host has claimed managed support.
/// 当宿主声明托管支持时，把受信任会话身份注入 LUASKILL_SID。
pub fn inject_managed_luaskill_sid_argument(
    tool: &RuntimeToolDescriptor,
    arguments: Value,
    managed_client_name: &str,
    managed_session_id: Option<&str>,
) -> Result<Value, (i64, String)> {
    if !runtime_tool_uses_managed_luaskill_sid(tool) {
        return Ok(arguments);
    }

    let session_id = managed_session_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            (
                -32602,
                format!(
                    "Dynamic LuaSkill tool {} requires projection.session_id when LUASKILL_SID is host-managed",
                    tool.name
                ),
            )
        })?;

    // Normalize managed calls back into the object-shaped LuaSkills argument contract before injecting the trusted sid.
    // 在注入受信任 sid 之前，把托管调用统一归一化回 LuaSkills 需要的对象形参数契约。
    let mut object = match arguments {
        Value::Null => Map::new(),
        Value::Object(map) => map,
        _ => {
            return Err((
                -32602,
                format!(
                    "Dynamic LuaSkill tool {} requires JSON object arguments when LUASKILL_SID is host-managed",
                    tool.name
                ),
            ));
        }
    };
    object.insert(
        HOST_MANAGED_LUASKILL_SID_FIELD.to_string(),
        Value::String(build_host_managed_luaskill_sid(
            managed_client_name,
            session_id,
        )),
    );
    Ok(Value::Object(object))
}

/// Build the host-managed LUASKILL_SID value that skills can recognize and redact safely.
/// 构建供技能识别并安全脱敏的宿主管理 LUASKILL_SID 值。
fn build_host_managed_luaskill_sid(client_name: &str, session_id: &str) -> String {
    // Hash host-managed identities into one bounded stable token so skills keep a portable redactable marker
    // without inheriting transport-specific session length or raw session contents.
    // 把宿主管理身份哈希成有界稳定标识，让技能保留可移植的脱敏标记，
    // 同时避免继承传输层特定的 session 长度或泄露原始 session 内容。
    let mut hasher = Sha256::new();
    hasher.update(client_name.trim().as_bytes());
    hasher.update(b"\0");
    hasher.update(session_id.as_bytes());
    let digest = hasher.finalize();
    let digest_hex = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("{HOST_MANAGED_LUASKILL_SID_PREFIX}{}", &digest_hex[..48])
}
