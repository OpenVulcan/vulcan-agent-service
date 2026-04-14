# 任务目标

为 `ast-grep` skill 的统一依赖声明增加 `rg`（ripgrep）依赖项，确保宿主在加载 skill 时可以按当前系统自动下载并安装 `rg` 到共享工具目录，供后续查询工具复用。

# 执行步骤

1. 检查当前 `ast-grep` 的 `dependencies.yaml` 结构，确认新增依赖所需字段与命名方式。
2. 调研 `ripgrep` 官方发布产物的命名规则，补齐至少以下平台：
   - windows x64
   - linux arm64
   - linux x64
   - macos arm64
3. 修改 `runtime/lua_skills/ast-grep/dependencies.yaml`，新增 `rg` 依赖项。
4. 同步更新技能说明文档，补充 `rg` 依赖示例与说明。
5. 执行校验，确认配置文件格式正确，且如有必要同步到 `output` 运行时副本。

# 技术方案

- 继续沿用 `dependencies.yaml` 的统一依赖声明结构。
- `rg` 使用 GitHub release latest API 获取最新版本。
- 由于 `ripgrep` 发布资源名通常带版本号，优先使用 `{version}` 占位符拼装 `asset_name`。
- Windows 平台产物落库为 `rg.exe`，Unix 平台落库为 `rg`。
- 若资源为压缩包，则使用宿主内核现有的 zip / tar.gz 解压能力提取目标文件。

# 验收标准

1. `ast-grep/dependencies.yaml` 中已包含 `rg` 依赖项。
2. `rg` 依赖至少覆盖 windows x64、linux arm64、linux x64、macos arm64 四类系统。
3. 文档已同步说明该依赖的用途与声明方式。
4. 必要的运行时副本已同步。
5. 相关校验通过，无明显配置错误。

## 执行变更总结

### 1. 核心修复与调整概述

- 为 `ast-grep` skill 的 `dependencies.yaml` 新增了 `rg`（ripgrep）依赖项，统一纳入 Rust 依赖下载器管理。
- 补强了 Rust 下载器的模板渲染能力，新增对 `archive_path` 与平台级 `install_as` 占位符的渲染支持，解决 ripgrep 压缩包内部目录携带版本号时的解压定位问题。
- 同步更新了 `__demo` 的依赖模板和 Lua Skill 说明文档，确保后续新增依赖时可以直接复用正确写法。

### 2. 📂文件变更清单

新增：

- 无

修改：

- `src/skill_dependency.rs`
- `runtime/lua_skills/ast-grep/dependencies.yaml`
- `runtime/lua_skills/__demo/dependencies.yaml`
- `docs/lua_skills.md`
- `output/lua_skills/ast-grep/dependencies.yaml`
- `output/lua_skills/__demo/dependencies.yaml`

删除：

- 无

### 3. 💻关键代码调整详情

- 在 `src/skill_dependency.rs` 中新增 `render_dependency_target`，统一渲染 `asset_name`、`install_as`、`archive_path` 的 `{tag}` / `{version}` 占位符。
- `ensure_one_dependency` 改为先构造渲染后的平台目标，再执行下载与安装，避免后续解压逻辑使用未展开的路径模板。
- `runtime/lua_skills/ast-grep/dependencies.yaml` 新增 `rg` 依赖，并覆盖以下系统目标：
  - `windows/x86_64`
  - `linux/x86_64`
  - `linux/aarch64`
  - `macos/aarch64`
- `__demo` 依赖模板补充了带版本号目录的 `archive_path` 示例，示范真实 GitHub release 压缩包的声明方式。

### 4. ⚠️遗留问题与注意事项

- 当前 `rg` 依赖只补入了 `ast-grep` skill 的依赖声明，尚未在 Lua 逻辑中正式使用；后续扩展查询工具时可直接从共享 `__tools/bin` 中调用。
- `ripgrep` 的 Windows ARM 版本当前未纳入，因为本次需求只要求覆盖 windows x64、linux arm、linux x64、macos arm。
- 依赖实际下载生效仍需在正式运行环境中重启进程并触发 skill 加载。
