use serde::Deserialize;

// ============================================================
// Lua Skill metadata (loaded from skill.json)
// ============================================================

#[derive(Deserialize, Debug, Clone)]
pub struct SkillMeta {
    /// Internal skill name, e.g. "codeview_ts"
    pub name: String,
    /// MCP tool name that will be registered, e.g. "codeview_ts"
    #[serde(default)]
    pub tool_name: String,
    /// Human-readable description shown in tools/list
    #[serde(default)]
    pub description: String,
    /// Lua entry filename relative to skill directory, e.g. "main.lua"
    #[serde(default)]
    pub lua_entry: String,
    /// Module name registered in the Lua VM, e.g. "codeview_ts"
    #[serde(default)]
    pub lua_module: String,
    /// Parameter definitions
    #[serde(default)]
    pub parameters: Vec<SkillParam>,
    /// Expected return type: "table" | "string" | "number"
    #[serde(default = "default_return_type")]
    #[allow(dead_code)]
    pub return_type: String,
    /// AI prompt hint appended to the tool description
    #[serde(default)]
    pub prompt: String,
    /// Debug mode: reload Lua source from disk on each invocation.
    #[serde(default)]
    pub debug: bool,
    /// Optional OS-specific init scripts.
    /// 可选的按操作系统区分的初始化脚本配置。
    #[serde(default)]
    pub init_scripts: SkillInitScripts,
    /// Static resources exposed by the skill.
    /// 由技能暴露的静态资源定义。
    #[serde(default)]
    pub resources: Vec<SkillResourceMeta>,
    /// Resource templates exposed by the skill.
    /// 由技能暴露的资源模板定义。
    #[serde(default)]
    pub resource_templates: Vec<SkillResourceTemplateMeta>,
    /// Prompt templates exposed by the skill.
    /// 由技能暴露的提示词模板定义。
    #[serde(default)]
    pub prompts: Vec<SkillPromptMeta>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct SkillParam {
    pub name: String,
    #[serde(rename = "type")]
    pub param_type: String,
    pub description: String,
    #[serde(default)]
    pub required: bool,
}

#[derive(Deserialize, Debug, Clone, Default)]
pub struct SkillInitScripts {
    /// POSIX shell init script relative to the skill directory.
    /// 相对于技能目录的 POSIX shell 初始化脚本。
    #[serde(default)]
    #[cfg_attr(windows, allow(dead_code))]
    pub sh: String,
    /// PowerShell init script relative to the skill directory.
    /// 相对于技能目录的 PowerShell 初始化脚本。
    #[serde(default)]
    pub ps1: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct SkillResourceMeta {
    /// Unique MCP resource URI.
    /// 唯一的 MCP 资源 URI。
    pub uri: String,
    /// Human-readable resource name.
    /// 人类可读的资源名称。
    pub name: String,
    /// Optional resource description.
    /// 可选的资源描述。
    #[serde(default)]
    pub description: Option<String>,
    /// Optional MIME type for the resource content.
    /// 资源内容的可选 MIME 类型。
    #[serde(default)]
    pub mime_type: Option<String>,
    /// Relative file path that stores the resource payload.
    /// 存储资源内容的相对文件路径。
    pub file: String,
    /// Optional declared size in bytes.
    /// 可选的字节大小声明。
    #[serde(default)]
    pub size: Option<u64>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct SkillResourceTemplateMeta {
    /// URI template following MCP conventions.
    /// 遵循 MCP 约定的 URI 模板。
    pub uri_template: String,
    /// Human-readable template name.
    /// 人类可读的模板名称。
    pub name: String,
    /// Optional template description.
    /// 可选的模板描述。
    #[serde(default)]
    pub description: Option<String>,
    /// Optional MIME type for generated resource payload.
    /// 生成资源内容的可选 MIME 类型。
    #[serde(default)]
    pub mime_type: Option<String>,
    /// Relative template file path used after placeholder substitution.
    /// 占位符替换后用于生成内容的相对模板文件路径。
    pub file: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct SkillPromptArgumentMeta {
    /// Prompt argument name.
    /// 提示词参数名称。
    pub name: String,
    /// Optional argument description.
    /// 可选的参数描述。
    #[serde(default)]
    pub description: Option<String>,
    /// Whether the argument is required.
    /// 参数是否必填。
    #[serde(default)]
    pub required: bool,
}

#[derive(Deserialize, Debug, Clone)]
pub struct SkillPromptMeta {
    /// Unique prompt name exposed through prompts/list.
    /// 通过 prompts/list 暴露的唯一提示词名称。
    pub name: String,
    /// Optional prompt description.
    /// 可选的提示词描述。
    #[serde(default)]
    pub description: Option<String>,
    /// Argument definitions declared for prompts/get.
    /// 为 prompts/get 声明的参数定义。
    #[serde(default)]
    pub arguments: Vec<SkillPromptArgumentMeta>,
    /// Relative file path that contains the prompt template text.
    /// 存放提示词模板文本的相对文件路径。
    pub file: String,
    /// Message role used when building PromptMessage.
    /// 生成 PromptMessage 时使用的角色。
    #[serde(default = "default_prompt_role")]
    pub role: String,
}

fn default_return_type() -> String {
    "table".to_string()
}

fn default_prompt_role() -> String {
    "user".to_string()
}

impl SkillMeta {
    /// Determine whether the skill declares a concrete MCP tool.
    /// 判断当前技能是否声明了一个可注册的 MCP 工具。
    pub fn has_tool(&self) -> bool {
        !self.tool_name.trim().is_empty()
    }
}
