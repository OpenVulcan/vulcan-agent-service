use serde::Deserialize;

// ============================================================
// Lua Skill metadata (loaded from skill.json)
// ============================================================

/// Skill-level metadata shared by all grouped entries.
/// Skill 级元数据，供其下所有分组入口共享。
#[derive(Deserialize, Debug, Clone)]
pub struct SkillMeta {
    /// Internal skill name, for example "ast-grep".
    /// 内部 skill 名称，例如 "ast-grep"。
    pub name: String,
    /// Debug mode: reload Lua source from disk on each invocation.
    /// 调试模式：每次调用时都从磁盘热加载 Lua 源文件。
    #[serde(default)]
    pub debug: bool,
    /// Grouped MCP entries declared by the skill.
    /// Skill 声明的分组化 MCP 入口集合。
    #[serde(default)]
    pub groups: Vec<SkillGroupMeta>,
}

/// One logical group inside a skill.json file.
/// skill.json 内的一个逻辑分组。
#[derive(Deserialize, Debug, Clone, Default)]
pub struct SkillGroupMeta {
    /// Group name used for organization and diagnostics.
    /// 用于组织和诊断输出的分组名称。
    #[serde(default)]
    pub name: String,
    /// Optional group description.
    /// 可选的分组描述。
    #[serde(default)]
    #[allow(dead_code)]
    pub description: String,
    /// Tool entries declared inside this group.
    /// 当前分组内声明的工具入口集合。
    #[serde(default)]
    pub tools: Vec<SkillToolMeta>,
    /// Resource entries declared inside this group.
    /// 当前分组内声明的资源入口集合。
    #[serde(default)]
    pub resources: Vec<SkillResourceMeta>,
    /// Resource template entries declared inside this group.
    /// 当前分组内声明的资源模板入口集合。
    #[serde(default)]
    pub resource_templates: Vec<SkillResourceTemplateMeta>,
    /// Prompt entries declared inside this group.
    /// 当前分组内声明的提示词入口集合。
    #[serde(default)]
    pub prompts: Vec<SkillPromptMeta>,
}

/// MCP tool entry metadata inside a skill group.
/// skill 分组中的 MCP tool 入口元数据。
#[derive(Deserialize, Debug, Clone)]
pub struct SkillToolMeta {
    /// MCP tool name exposed through tools/list.
    /// 通过 tools/list 暴露的 MCP 工具名称。
    pub name: String,
    /// Human-readable tool description shown in tools/list.
    /// 展示在 tools/list 中的人类可读描述。
    #[serde(default)]
    pub description: String,
    /// Lua entry filename relative to the skill directory, for example "main.lua".
    /// 相对 skill 目录的 Lua 入口文件名，例如 "main.lua"。
    pub lua_entry: String,
    /// Lua module registration name, for example "ast_grep_main".
    /// Lua 模块注册名称，例如 "ast_grep_main"。
    pub lua_module: String,
    /// Parameter definitions specific to this tool entry.
    /// 当前工具入口独有的参数定义。
    #[serde(default)]
    pub parameters: Vec<SkillParam>,
    /// Expected return type: "table" | "string" | "number".
    /// 预期返回类型："table" | "string" | "number"。
    #[serde(default = "default_return_type")]
    #[allow(dead_code)]
    pub return_type: String,
    /// AI prompt hint appended to the tool description.
    /// 追加到工具描述后的 AI 使用提示。
    #[serde(default)]
    pub prompt: String,
}

/// Shared parameter metadata used by tool entries.
/// 工具入口共用的参数元数据。
#[derive(Deserialize, Debug, Clone)]
pub struct SkillParam {
    /// Parameter name.
    /// 参数名称。
    pub name: String,
    /// Parameter type string used by JSON Schema.
    /// JSON Schema 使用的参数类型字符串。
    #[serde(rename = "type")]
    pub param_type: String,
    /// Parameter description.
    /// 参数描述。
    pub description: String,
    /// Whether the parameter is required.
    /// 参数是否必填。
    #[serde(default)]
    pub required: bool,
}

/// Resource entry metadata inside a skill group.
/// skill 分组中的资源入口元数据。
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

/// Resource template entry metadata inside a skill group.
/// skill 分组中的资源模板入口元数据。
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
    /// Relative file path used by the template.
    /// 资源模板使用的相对文件路径。
    pub file: String,
}

/// Prompt argument metadata used by prompt entries.
/// 提示词入口使用的参数元数据。
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
    /// Optional completion candidates shown by completion/complete.
    /// 通过 completion/complete 暴露的可选候选项列表。
    #[serde(default)]
    pub completions: Vec<String>,
}

/// Prompt entry metadata inside a skill group.
/// skill 分组中的提示词入口元数据。
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
    /// Relative file path that contains the prompt payload or generator.
    /// 存放提示词内容或生成器的相对文件路径。
    pub file: String,
    /// Message role used when building PromptMessage.
    /// 生成 PromptMessage 时使用的角色。
    #[serde(default = "default_prompt_role")]
    pub role: String,
}

/// Default return type for tool entries.
/// 工具入口默认返回类型。
fn default_return_type() -> String {
    "table".to_string()
}

/// Default prompt role for prompt entries.
/// 提示词入口默认角色。
fn default_prompt_role() -> String {
    "user".to_string()
}

impl SkillMeta {
    /// Iterate over all tool entries across every group.
    /// 遍历当前 skill 所有分组下的工具入口。
    pub fn tools(&self) -> impl Iterator<Item = &SkillToolMeta> {
        self.groups.iter().flat_map(|group| group.tools.iter())
    }

    /// Iterate over all resource entries across every group.
    /// 遍历当前 skill 所有分组下的资源入口。
    pub fn resources(&self) -> impl Iterator<Item = &SkillResourceMeta> {
        self.groups.iter().flat_map(|group| group.resources.iter())
    }

    /// Iterate over all resource template entries across every group.
    /// 遍历当前 skill 所有分组下的资源模板入口。
    pub fn resource_templates(&self) -> impl Iterator<Item = &SkillResourceTemplateMeta> {
        self.groups
            .iter()
            .flat_map(|group| group.resource_templates.iter())
    }

    /// Iterate over all prompt entries across every group.
    /// 遍历当前 skill 所有分组下的提示词入口。
    pub fn prompts(&self) -> impl Iterator<Item = &SkillPromptMeta> {
        self.groups.iter().flat_map(|group| group.prompts.iter())
    }

    /// Find a tool entry by its MCP tool name.
    /// 根据 MCP 工具名称查找工具入口。
    pub fn find_tool(&self, tool_name: &str) -> Option<&SkillToolMeta> {
        self.tools().find(|tool| tool.name == tool_name)
    }

    /// Find a tool entry together with its owning group.
    /// 根据工具名称查找其入口以及所属分组。
    pub fn find_tool_with_group(
        &self,
        tool_name: &str,
    ) -> Option<(&SkillGroupMeta, &SkillToolMeta)> {
        for group in &self.groups {
            if let Some(tool) = group.tools.iter().find(|tool| tool.name == tool_name) {
                return Some((group, tool));
            }
        }
        None
    }

    /// Find a resource entry together with its owning group.
    /// 根据 URI 查找资源入口以及所属分组。
    pub fn find_resource_with_group(
        &self,
        uri: &str,
    ) -> Option<(&SkillGroupMeta, &SkillResourceMeta)> {
        for group in &self.groups {
            if let Some(resource) = group.resources.iter().find(|resource| resource.uri == uri) {
                return Some((group, resource));
            }
        }
        None
    }

    /// Find a prompt entry together with its owning group.
    /// 根据提示词名称查找提示词入口以及所属分组。
    pub fn find_prompt_with_group(
        &self,
        prompt_name: &str,
    ) -> Option<(&SkillGroupMeta, &SkillPromptMeta)> {
        for group in &self.groups {
            if let Some(prompt) = group
                .prompts
                .iter()
                .find(|prompt| prompt.name == prompt_name)
            {
                return Some((group, prompt));
            }
        }
        None
    }
}
