# LuaSkills AI 原生工具未来路线图

## 1. 总体定位

LuaSkills 的核心价值不是简单封装已有命令，而是把传统工具输出转换成适合 AI 使用的上下文协议。

传统工具主要面向人类或 IDE：输出完整、细碎、噪声多，适合人慢慢看。AI Coding 场景需要的是另一种形态：更少噪声、更明确的结构、更稳定的证据引用、更直接的下一步行动建议。

因此 LuaSkills 可以定位为：

> Raw Tool Output -> Agent-Native Context Adapter

也就是把原始工具输出转换为 AI 可以直接推理、决策和继续调用工具的结构化信息层。

## 2. 核心设计原则

### 2.1 不直接转发原始输出

工具的默认输出不应该是命令行日志、完整 AST、完整 diff、完整编译输出，而应该是已经筛选后的摘要、证据、风险和下一步建议。

原始内容可以保留，但应作为 `raw_pointer`、分页读取入口或调试模式存在。

### 2.2 永远保留证据来源

AI 友好不等于丢失可验证性。每个结论都应尽量附带 source refs，例如文件路径、函数名、行号、测试名、依赖路径、日志片段位置。

推荐输出包含：

- `summary`：面向 AI 的结论摘要。
- `evidence`：支撑结论的关键证据。
- `source_refs`：可回跳的文件、函数、测试、命令或日志位置。
- `next_actions`：下一步建议调用什么工具、看什么文件、改什么范围。
- `raw_pointer`：必要时回看原始内容的入口。

### 2.3 输出下一步行动，而不只是信息

AI Coding 工具的输出应尽量回答两个问题：

- 当前最重要的事实是什么？
- 下一步最应该做什么？

这会显著减少 Agent 在“读完以后还要自己猜下一步”的上下文消耗。

### 2.4 默认压缩，允许展开

默认输出应是紧凑结构。只有在用户或 Agent 明确需要时，再通过分页、精确节点、函数级详情、原始日志片段等方式展开。

### 2.5 与工作记忆协同

关键结论、已读文件、核心决策、任务目标、风险点应能写入 WorkMemory。长任务、上下文压缩、跨工具连续分析时，工作记忆负责保留“为什么走到这里”。

## 3. 推荐优先级

建议按以下顺序逐步实现：

1. CodeKit
2. TestKit
3. DiffKit
4. WorkMemory
5. DepKit
6. LogKit
7. ConfigKit
8. DocKit
9. RunKit

前三个工具解决 AI Coding 的主要上下文污染来源：

- CodeKit 减少读代码污染。
- TestKit 减少编译和测试反馈污染。
- DiffKit 减少审查变更污染。

WorkMemory 则解决长任务中的连续性问题，尤其是上下文压缩后的恢复问题。

## 4. 未来工具清单

### 4.1 CodeKit

定位：代码结构理解与安全修改工具。

核心价值：

- 用 AST tree 快速建立项目结构认知。
- 用 AST detail 精确读取函数、类、接口上下文。
- 用 rg -> AST bridge 从文本线索定位到结构节点。
- 用结构化 patch 替换完整函数，降低行号漂移和局部替换风险。

适合解决的问题：

- 大项目快速理解。
- 从日志、错误、符号名回到代码结构。
- 减少无目的文件读取。
- 函数级安全编辑。

后续增强方向：

- 输出模块职责摘要。
- 输出调用关系的轻量视图。
- 输出“建议下一步读取文件”。
- 与 TestKit 联动，从测试失败直接定位拥有函数。

### 4.2 TestKit

定位：测试、编译、构建输出的 AI 原生压缩器。

核心价值：

- 大项目编译输出往往巨大，直接塞给 AI 会造成严重上下文污染。
- TestKit 应将原始日志压缩成“真正错误、级联错误、警告分组、失败测试、相关文件、下一步建议”。

适合解决的问题：

- Cargo、npm、pnpm、pytest、go test、mvn、gradle 等命令输出过长。
- 编译失败中存在大量重复错误或级联错误。
- 测试失败日志包含大量无关上下文。
- Agent 不需要完整日志，只需要知道首个根因和最相关文件。

推荐输出：

- `command`：执行的命令。
- `status`：成功、失败、超时、部分失败。
- `root_errors`：最可能的根因错误。
- `cascade_errors`：被折叠的级联错误数量和类型。
- `warnings`：按类别折叠的警告。
- `failed_tests`：失败测试列表和失败原因摘要。
- `owner_hints`：可能需要查看的文件、函数或模块。
- `next_actions`：建议下一步读取、修改或复跑的范围。
- `raw_pointer`：原始日志保存位置或分页读取入口。

MVP 建议：

- 先支持 cargo test、cargo check、npm test、pytest。
- 先做日志分类和首个真实错误提取。
- 先不追求完美根因推理，但必须显著减少输出体积。

### 4.3 DiffKit

定位：语义化 Git diff 分析工具。

核心价值：

- 原始 diff 对 AI 来说经常过细，容易把上下文预算花在格式变化上。
- DiffKit 应把变更转换为模块影响、行为变化、风险点、测试缺口和审查候选问题。

适合解决的问题：

- 审核未提交代码。
- PR 变更摘要。
- 判断变更是否影响公共 API。
- 判断是否需要补测试。

推荐输出：

- `changed_modules`：变更模块。
- `behavior_changes`：可能的行为变化。
- `public_api_changes`：接口、配置、协议、命令行参数变化。
- `risky_files`：高风险文件。
- `test_coverage_gap`：测试缺口。
- `review_findings_candidates`：潜在审查发现。
- `next_actions`：建议继续查看的文件或测试。

MVP 建议：

- 先支持 git diff、git diff --cached、git show。
- 文件级统计和函数级归属结合 CodeKit。
- 对纯格式化、锁文件、文档变更做降噪。

### 4.4 WorkMemory

定位：任务态工作记忆与上下文压缩恢复工具。

核心价值：

- 长任务中 AI 会读取大量文件、做出阶段性判断、形成局部决策。
- 上下文压缩后，最容易丢失的是“已经知道什么、为什么这么做、下一步该干什么”。
- WorkMemory 应保存任务中的关键节点，而不是保存全部聊天内容。

推荐保存内容：

- `active_goal`：当前任务目标。
- `session_id`：会话标识。
- `touched_files`：已读或已改文件。
- `key_findings`：关键发现。
- `decisions`：已经做出的设计或实现决策。
- `next_steps`：下一步计划。
- `unresolved_risks`：未解决风险。
- `completion_policy`：任务结束后如何清理记忆。

关于 session_id：

- 对 OpenCode、Claude Code 等原生插件或 gRPC 接入场景，优先使用宿主提供的 session_id。
- 对普通 MCP 场景，如果宿主无法提供 session_id，可以允许 AI 自定义 session_id。
- AI 自定义 session_id 时，工具输出必须反复携带该 ID，并要求后续调用继续传入。
- 为降低上下文压缩丢失 ID 的风险，可增加 `memory_bootstrap` 能力：根据项目路径、分支、任务标题、最近活跃时间、调用者标签恢复候选 session。
- session_id 不应只依赖模型记忆，应该在每次 WorkMemory 响应中显式返回，并在压缩恢复协议里作为高优先级字段保留。

MVP 建议：

- 支持 create、append、list-active、recall、complete。
- 先按 project_path + session_id 隔离。
- 支持任务完成后删除或归档。
- 输出必须短，只返回当前任务需要的记忆摘要。

### 4.5 DepKit

定位：依赖、锁文件、漏洞告警解释工具。

核心价值：

- 依赖告警通常包含包名、版本、漏洞编号，但 AI 还需要判断项目是否受影响、应该如何升级、升级风险在哪里。
- DepKit 应把漏洞和依赖变更转换成可执行的修复建议。

适合解决的问题：

- GitHub Dependabot 告警。
- cargo audit、npm audit、pip-audit、go list -m -u 输出。
- 锁文件版本升级审查。

推荐输出：

- `affected_package`：受影响包。
- `dependency_path`：依赖链。
- `locked_version`：当前锁定版本。
- `fixed_version`：修复版本。
- `advisory_ids`：漏洞编号。
- `reachability_signal`：项目是否可能触达该依赖。
- `safe_update_command`：建议升级命令。
- `risk_notes`：兼容性风险。

MVP 建议：

- 先支持 Cargo.lock 和 package-lock/pnpm-lock。
- 先做告警到锁文件版本的映射。
- 与 DiffKit 联动，审查锁文件变化是否只包含目标升级。

### 4.6 LogKit

定位：日志、panic、stack trace 的归因工具。

核心价值：

- 原始日志通常很长，AI 真正需要的是错误签名、关键栈帧、归属模块、可能原因和下一步代码定位。

适合解决的问题：

- 服务端 panic。
- CLI 崩溃。
- 测试失败 stack trace。
- 运行时错误日志。

推荐输出：

- `error_signature`：错误签名。
- `top_frames`：关键栈帧。
- `owning_module`：归属模块。
- `owning_function`：归属函数。
- `likely_cause`：可能原因。
- `next_codekit_query`：建议 CodeKit 查询。
- `raw_pointer`：原始日志入口。

MVP 建议：

- 先支持 Rust、Node.js、Python 常见 stack trace。
- 与 CodeKit 的 rg -> AST bridge 联动。

### 4.7 ConfigKit

定位：配置文件理解与有效配置解释工具。

核心价值：

- 配置文件常常分散在 yaml、json、toml、env、命令行参数中。
- AI 需要知道的是当前生效配置、默认值覆盖、无效配置、安全敏感项和运行时影响。

适合解决的问题：

- MCP server 配置。
- tool_configs.yaml、client_budgets.yaml 等运行时配置。
- CI 配置、构建配置、应用配置。

推荐输出：

- `active_settings`：当前生效配置。
- `defaults_vs_overrides`：默认值与覆盖值。
- `invalid_or_unused_keys`：无效或未使用字段。
- `security_sensitive_keys`：敏感配置。
- `related_runtime_behavior`：配置影响的运行时行为。
- `reload_required`：是否需要重载或重启。

MVP 建议：

- 先支持 JSON、YAML、TOML。
- 先从 schema 或已知默认值中提取差异。
- 对敏感字段做脱敏。

### 4.8 DocKit

定位：文档导航、约束提取和命令配方工具。

核心价值：

- 文档通常适合人读，不适合 AI 在有限上下文中快速提取约束。
- DocKit 应从 README、设计文档、AGENTS、help 文档中提取规则、接口契约、迁移说明和可执行命令。

适合解决的问题：

- 快速理解项目规则。
- 从文档中提取工具使用协议。
- 生成后续实现计划的约束清单。

推荐输出：

- `section_map`：文档结构。
- `contracts`：接口或行为契约。
- `constraints`：必须遵守的限制。
- `migration_notes`：迁移相关说明。
- `command_recipes`：可执行命令。
- `next_sections`：建议继续读取的章节。

MVP 建议：

- 基于 Markdown heading menu 做第一层导航。
- 支持精确读取指定章节。
- 支持把文档约束写入 WorkMemory。

### 4.9 RunKit

定位：通用命令输出归一化工具。

核心价值：

- 并不是所有命令都值得做专用 Kit。
- RunKit 可以作为通用命令执行后的结果标准化层，把命令状态、关键错误、产物和下一步建议统一输出。

适合解决的问题：

- 小型脚本执行。
- 代码生成命令。
- 格式化命令。
- 临时诊断命令。

推荐输出：

- `command`：执行命令。
- `exit_code`：退出码。
- `duration_ms`：耗时。
- `key_output`：关键输出。
- `errors`：错误摘要。
- `warnings`：警告摘要。
- `artifacts`：生成文件。
- `next_actions`：下一步建议。

MVP 建议：

- 不替代 shell，而是归一化 shell 输出。
- 支持输出长度限制。
- 支持将完整输出保存为 raw artifact。

## 5. 建议的统一输出协议

所有 LuaSkills 工具可以尽量收敛到类似结构：

```json
{
  "status": "ok | warning | error",
  "summary": "面向 AI 的短结论",
  "evidence": [],
  "source_refs": [],
  "next_actions": [],
  "raw_pointer": null,
  "memory_suggestion": null
}
```

其中 `memory_suggestion` 用于提示 WorkMemory 是否值得保存本次结果，例如：

- 发现核心文件。
- 形成关键设计决策。
- 识别根因。
- 完成阶段性验证。
- 遇到未解决风险。

## 6. 与 Vulcan MCP、gRPC、VMM 的关系建议

Vulcan MCP 可以继续扩展为工具中转层和运行时宿主：

- 面向 MCP 客户端提供工具调用。
- 面向 OpenCode、Claude Code 等插件提供 gRPC 接入。
- 面向 LuaSkills 提供统一的工具执行、配置、存储、预算控制和结果协议。

VMM 则更适合收敛为服务端记忆网络：

- 前置记忆预判。
- 后置记忆筛选。
- 长期记忆图谱。
- 跨任务、跨项目的知识关联。

工作记忆建议从 VMM 本体中剥离，优先放入 LuaSkills 机制：

- LuaSkills 负责任务态、短周期、可清理的工作记忆。
- VMM 负责长期、筛选后、跨会话的记忆网络。
- Vulcan MCP 负责连接不同客户端、插件、工具和记忆服务。

## 7. 后续逐项分析模板

后续分析每一个工具时，建议统一使用以下结构：

1. 工具目标
2. 目标用户与典型场景
3. 输入参数设计
4. 输出协议设计
5. 噪声过滤策略
6. 与 CodeKit 的联动方式
7. 与 WorkMemory 的联动方式
8. MVP 范围
9. 边界与风险
10. 验收标准

## 8. 当前最建议优先展开的问题

优先分析 TestKit。

原因是 TestKit 直接命中大项目编译和测试场景中的上下文污染问题。CodeKit 解决“读代码太多”，TestKit 解决“验证输出太多”。这两个工具组合起来，会明显改变 AI Coding 的工作效率。

TestKit 的第一阶段不需要做复杂智能推理，只要能稳定完成以下三点，就已经有明显价值：

- 找出第一个真实错误。
- 折叠级联错误和重复警告。
- 给出最相关的文件、测试和下一步建议。

