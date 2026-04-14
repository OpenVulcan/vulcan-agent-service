## 任务目标

移除 Lua Skill 的初始化脚本机制，改为固定的 `dependencies.yaml` 依赖声明文件，由 Rust 统一解析并完成依赖下载。新的下载链路需要支持 GitHub 最新标签解析、按当前系统选择合适资源文件、下载进度条展示，以及基于“工具落库名称”的跳过机制，最终将工具下载到 `lua_skills/__tools/bin/` 下。

## 执行步骤

1. 梳理当前 `init_scripts` 机制、现有 GitHub 下载脚本与工具目录约定，确认需要替换的运行时路径。
2. 设计 `dependencies.yaml` 结构，覆盖仓库地址、标签解析地址、平台与文件名映射、版本占位符、落库名称与跳过条件。
3. 在 Rust 侧实现统一下载器，包括：
   - GitHub 最新标签解析
   - 当前系统目标匹配
   - 版本占位符替换
   - 下载到 `lua_skills/__tools/bin/`
   - 进度条显示
4. 移除 `init_scripts` 元数据与加载逻辑，改为 skill 加载前自动检查固定 `dependencies.yaml`。
5. 用 `codeview_ast` 迁移为新依赖声明格式，更新 `__demo`、说明文档与运行时副本。
6. 完成构建验证与必要的最小行为验证，补写执行变更总结并归档。

## 技术选型

- `dependencies.yaml` 作为固定文件名，放在每个 skill 根目录下；无该文件则表示该 skill 无外部依赖初始化需求。
- Rust 下载器直接处理 GitHub HTTP 请求，不再依赖 PowerShell / shell 脚本。
- 通过统一的 `__tools/bin` 工具缓存目录保存下载结果，使用“落库名称”判断是否已存在。
- 下载进度条优先使用终端友好型库输出，保证本地运行时可观察。

## 验收标准

1. Skill 不再使用 `init_scripts` 字段，加载逻辑改为检查固定 `dependencies.yaml`。
2. Rust 可根据 `dependencies.yaml` 自动解析 GitHub 最新 tag，并按当前系统下载正确资源。
3. 当 `lua_skills/__tools/bin/` 中已存在配置的落库文件时，会跳过重复初始化。
4. 下载过程可显示进度条或清晰的实时进度输出。
5. `codeview_ast` 已迁移为新格式，相关文档与模板同步完成。
6. `cargo build` 通过。

## 执行变更总结

### 1. 核心修复与调整概述

- 已新增 Rust 侧统一依赖下载器，使用固定 `dependencies.yaml` 取代原有 `init.ps1/init.sh` 初始化脚本机制。
- 新下载器支持：
  - GitHub 最新 tag 解析（兼容 `releases/latest` 与 tags 列表接口）
  - 按系统与架构匹配资源
  - `{tag}` / `{version}` 占位符替换
  - 下载进度条展示
  - 直接文件、`.zip`、`.tar.gz` / `.tgz` 安装
  - 基于落库文件名的跳过逻辑
- `codeview_ast` 已迁移为 `dependencies.yaml`，并改为从共享 `lua_skills/__tools/bin/` 查找 `ast-grep`。

### 2. 📂文件变更清单

- 新增：`src/skill_dependency.rs`
- 修改：`src/main.rs`
- 修改：`src/lua_engine.rs`
- 修改：`src/lua_skill.rs`
- 修改：`Cargo.toml`
- 修改：`runtime/lua_skills/codeview_ast/skill.json`
- 新增：`runtime/lua_skills/codeview_ast/dependencies.yaml`
- 删除：`runtime/lua_skills/codeview_ast/init.ps1`
- 删除：`runtime/lua_skills/codeview_ast/init.sh`
- 新增：`runtime/lua_skills/__demo/dependencies.yaml`
- 修改：`runtime/lua_skills/codeview_ast/main.lua`
- 修改：`docs/lua_skills.md`
- 同步：`output/lua_skills/**`
- 新增：`docs/plan/20260414-07-SKILL_DEPENDENCIES_YAML_AND_RUST_DOWNLOADER.md`

### 3. 💻关键代码调整详情

- 在 `src/skill_dependency.rs` 中实现了依赖清单解析、GitHub 请求、下载进度条、压缩包解压与安装逻辑。
- 在 `src/lua_engine.rs` 的 skill 加载流程中接入 `ensure_skill_dependencies`，使依赖初始化成为固定、统一的加载前步骤。
- 在 `src/lua_skill.rs` 中移除了 `init_scripts` 元数据，仅保留与 MCP 入口定义相关的结构。
- 在 `runtime/lua_skills/codeview_ast/main.lua` 中将二进制查找路径切换到共享的 `../__tools/bin/`。
- 在 `docs/lua_skills.md` 中将技能开发文档整体切换为 `dependencies.yaml` 新约定，并补充四类推荐平台目标说明。

### 4. ⚠️遗留问题与注意事项

- 当前平台匹配至少覆盖 `windows/x86_64`、`linux/x86_64`、`linux/aarch64`、`macos/aarch64`；如果后续要支持更多平台，需要在对应 skill 的 `dependencies.yaml` 中继续补 target。
- 本次最小验证以构建通过和运行时副本同步为主；真正下载行为依赖外网与 GitHub 可用性。
- `output/lua_skills` 已同步，但若正式运行中的进程仍在使用旧副本，需要重启服务后才会真正生效。
