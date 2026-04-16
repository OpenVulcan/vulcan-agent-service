# 任务目标

统一修正当前仓库与本地 Codex skill 中 `codekite` / `Codekite` 的错误命名，收敛为正确的 `codekit` / `CodeKit`，确保文件名、注册名、prompt 名称、默认调用文案和路径引用保持一致，不再出现拼写混用。

# 执行步骤

1. 扫描仓库与本地 skill 目录，确认所有 `codekite` 相关命名与引用位置。
2. 统一调整本地 skill 的名称、目录、默认提示文案及相关引用，使其与 `codekit` 命名一致。
3. 统一调整仓库内 `vulcan-codekit` Lua skill 的 prompt 名称、prompt 文件名、`skill.json` 中的配置项与引用路径。
4. 对受影响的说明文件与路径引用做必要修正，避免出现入口名已改而文档仍指向旧名的情况。
5. 做一致性与基础校验，确认本地 skill 与公共 prompt 主体仍一致、配置引用可解析。
6. 在计划文件末尾追加执行变更总结，并归档到 `docs/completed/20260416/`。

# 技术选型

1. 以 `codekit` / `CodeKit` 作为唯一正确拼写，不保留 `codekite` 兼容命名。
2. 优先修复“仍参与运行和注册的文件、配置、路径与入口”，历史归档文档不做大面积回写，仅在必要时保留历史记录。
3. 本地 skill 与公共 prompt 继续保持主体正文一致，避免命名修复后再次出现双份漂移。

# 验收标准

1. 仓库当前运行路径与本地 skill 当前使用路径中不再存在仍参与运行的 `codekite` 错误命名。
2. `skill.json`、prompt 文件、默认提示文案、本地 skill frontmatter 与目录名保持一致。
3. 本地 skill 校验通过，公共 prompt 主体与本地 skill 正文一致。
4. 不引入旧命名兼容层，统一以 `codekit` 为准。

---

# 执行变更总结

## 1. 核心修复与调整概述

本次对当前仍参与运行的 `codekite` 错误命名做了统一收口，目标是将本地 Codex skill、仓库内 `vulcan-codekit` Lua skill 的 prompt 配置、prompt 文件名以及默认调用文案全部统一到 `codekit` / `CodeKit`。这次没有保留任何旧命名兼容层，避免继续出现“文件名、配置名、调用名三套拼写”的漂移问题。

## 2. 📂文件变更清单

### 修改文件

- `C:\Users\20000\.codex\skills\vulcan-codekit\SKILL.md`
- `C:\Users\20000\.codex\skills\vulcan-codekit\agents\openai.yaml`
- `D:\projects\vulcan-mcp-client\runtime\lua_skills\vulcan-codekit\skill.json`
- `D:\projects\vulcan-mcp-client\runtime\lua_skills\vulcan-codekit\skills\SKILL.md`
- `D:\projects\vulcan-mcp-client\runtime\lua_skills\vulcan-codekit\skills\agents\openai.yaml`
- `D:\projects\vulcan-mcp-client\docs\completed\20260416\03-CODEKIT_BOUNDARIES_TIGHTENING.md`
- `D:\projects\vulcan-mcp-client\docs\completed\20260416\04-CODEKIT_HARD_ROUTING_PROMPT_REWRITE.md`

### 重命名文件 / 目录

- `C:\Users\20000\.codex\skills\vulcan-codekite` → `C:\Users\20000\.codex\skills\vulcan-codekit`
- `D:\projects\vulcan-mcp-client\runtime\lua_skills\vulcan-codekit\prompts\vulcan_codekite_skill.lua` → `D:\projects\vulcan-mcp-client\runtime\lua_skills\vulcan-codekit\prompts\vulcan_codekit_skill.lua`

### 新增文件

- `D:\projects\vulcan-mcp-client\docs\plan\20260416-05-CODEKIT_NAME_NORMALIZATION.md`（随后归档）

## 3. 💻关键代码调整详情

### 本地 skill 命名修复

- 将本地 Codex skill 的 frontmatter `name` 从 `vulcan-codekite` 改为 `vulcan-codekit`。
- 将本地 skill 目录重命名为 `vulcan-codekit`。
- 将 `agents/openai.yaml` 中默认提示文案的 `$vulcan-codekite` 修正为 `$vulcan-codekit`。

### 仓库内 Lua skill 配置修复

- 将 `skill.json` 中 prompt 名称由 `vulcan_codekite_skill` 改为 `vulcan_codekit_skill`。
- 将 `skill.json` 中 prompt 文件引用由 `prompts/vulcan_codekite_skill.lua` 改为 `prompts/vulcan_codekit_skill.lua`。
- 将运行中的 prompt 文件同步重命名为 `vulcan_codekit_skill.lua`。
- 将 `runtime/lua_skills/vulcan-codekit/skills/` 中镜像的 skill 名称与默认调用文案同步修正为 `codekit` 拼写。

### 文档路径修正

- 对当日两份已归档计划中仍引用旧路径的记录做了修正，避免后续检索时继续看到失效路径。

## 4. ⚠️遗留问题与注意事项

- 当前运行中的路径、配置与本地 skill 已不再保留 `codekite` 错误命名；本次未大范围回写更早历史归档文档中的旧名称，以保留历史记录原貌。
- 本次没有引入兼容别名，因此旧的 `$vulcan-codekite` 或 `vulcan_codekite_skill` 调用方式应视为废弃，后续统一使用 `vulcan-codekit` / `vulcan_codekit_skill`。
- 已执行两项校验：
  - 本地 skill 与公共 prompt 主体对比结果为 `BODY_MATCHED`
  - `quick_validate.py` 结果为 `Skill is valid!`
