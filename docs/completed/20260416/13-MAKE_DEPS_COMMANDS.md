# 任务目标

在 `D:\projects\vulcan-mcp-client` 中将宿主依赖初始化与 Lua 依赖初始化接入统一的 make 指令体系，新增 `make deps host` 与 `make deps lua` 两类调用入口，便于开发者快速执行对应依赖安装流程。

# 执行步骤

1. 梳理当前仓库已有的 make / 脚本入口。
   - 检查 Makefile、PowerShell、Shell 启动方式
   - 确认现有命令命名风格与跨平台处理方式

2. 设计依赖命令映射。
   - `make deps host` 对应调用 `install_host_deps.ps1/.sh`
   - `make deps lua` 对应调用 `install_lua_deps.ps1/.sh`
   - 保证 Windows 与 Unix-like 环境下行为一致

3. 实现命令接入。
   - 修改 Makefile 或等效入口
   - 必要时增加帮助说明或注释

4. 回归验证。
   - 验证新命令可被正确解析
   - 验证至少一条命令在本机环境下可成功执行
   - 确认不破坏现有构建命令

5. 归档。
   - 在计划文件末尾追加执行变更总结
   - 完成后归档到 `docs/completed/20260416/`

# 技术选型

- 优先复用现有 `Makefile.toml` / `cargo-make` 体系，避免额外增加新的任务入口风格。
- 宿主依赖与 Lua 依赖命令保持单职责，分别调用独立脚本。
- 保持命令命名简洁，符合 `make deps host` / `make deps lua` 的层级语义。

# 验收标准

1. 仓库支持 `make deps host`。
2. 仓库支持 `make deps lua`。
3. 新命令分别调用宿主依赖初始化脚本与 Lua 依赖初始化脚本。
4. 本机至少完成一次关键路径验证。
5. 不影响现有其他 make/build 命令。

---

# 执行变更总结

## 1. 核心修复与调整概述

- 已为仓库补齐统一任务入口，支持依赖安装命令分层调用：
  - `deps host`
  - `deps lua`
- `make.ps1` 与 `make.sh` 现在都支持 `deps` 子命令，并分别委托到：
  - `install_host_deps`
  - `install_lua_deps`
- 新增轻量 `Makefile`，让 GNU Make 语义下的 `make deps host`、`make deps lua` 成立。
- 额外修复了 `make.ps1` 在 Windows PowerShell 下的兼容性问题：
  - 将参数块内注释外移
  - 将文件编码调整为 UTF-8 BOM，避免 WinPS 因中文注释误解析 `param(...)`

## 2. 📂文件变更清单

### 新增
- `D:\projects\vulcan-mcp-client\Makefile`

### 修改
- `D:\projects\vulcan-mcp-client\make.ps1`
- `D:\projects\vulcan-mcp-client\make.sh`

## 3. 💻关键代码调整详情

- `make.ps1`
  - 新增宿主依赖脚本路径与 Lua 依赖脚本路径常量
  - 新增 `Invoke-DependencyInstall` 统一依赖初始化分发函数
  - `switch` 分发新增 `deps` 分支
  - `Show-Usage` 新增 `deps host` / `deps lua` 用法说明
- `make.sh`
  - 新增 `HOST_DEPS_SCRIPT_PATH` 与 `LUA_DEPS_SCRIPT_PATH`
  - 新增 `invoke_dependency_install`
  - `case` 分发新增 `deps` 分支
  - 帮助文本同步更新
- `Makefile`
  - 新增 `deps` 空分组目标
  - 新增 `host`、`lua` 两个目标，分别映射到 `bash ./make.sh deps host` 与 `bash ./make.sh deps lua`
  - 保持现有 `build` / `release` / `run` 的统一任务入口风格

## 4. ⚠️遗留问题与注意事项

- 本机当前未安装 GNU Make，因此未实际执行 `make deps host` 命令；已验证：
  - `powershell -File make.ps1 deps host`
  - `bash ./make.sh deps host`
  - 两条关键入口均可正常工作
- `Makefile` 主要面向装有 GNU Make 的 Unix-like / Git Bash / MSYS2 环境；若在纯 Windows PowerShell 环境使用，建议继续使用：
  - `./make.ps1 deps host`
  - `./make.ps1 deps lua`
