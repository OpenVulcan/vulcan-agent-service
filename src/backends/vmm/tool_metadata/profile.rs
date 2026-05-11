use super::*;

/// Build all stable VMM profile-adjust descriptors used by hosts that want one optional AI-facing correction tool.
/// 构建供希望使用可选 AI 画像纠偏工具的宿主使用的全部稳定 VMM 画像调整描述。
pub fn vmm_profile_tool_descriptors() -> Vec<VmmMemoryToolDescriptor> {
    vec![vulcan_profile_adjust_descriptor()]
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
