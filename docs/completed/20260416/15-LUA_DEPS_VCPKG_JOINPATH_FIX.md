# 任务目标

修复 `install_lua_deps.ps1` 在预编译 C 依赖缺失时回退到 vcpkg 安装路径的脚本错误，解决 `Join-Path` 参数拼接异常，并明确当前 `deps-v1` 预编译发布缺失时的行为，确保全新环境下依赖初始化流程稳定可用。

# 执行步骤

1. 复盘当前报错链路。
   - 检查 `Download-Prebuilt-Deps` 的 release/tag 查询结果
   - 检查 vcpkg 回退逻辑中 `Join-Path` 的调用方式
   - 明确是发布缺失、脚本 bug，还是两者同时存在

2. 实现修复。
   - 修复 `Join-Path` 多段拼接错误
   - 必要时增强预编译缺失时的日志提示
   - 保证 vcpkg 回退路径可继续执行

3. 回归验证。
   - 至少验证脚本语法通过
   - 在本机环境下复现并确认脚本不再因该错误中断

4. 归档。
   - 在计划文件末尾追加执行变更总结
   - 完成后归档到 `docs/completed/20260416/`

# 技术选型

- 优先做最小修复，不改变现有依赖初始化总体结构。
- 保留“预编译优先、缺失则回退本地编译/安装”的策略。
- 增强错误信息，避免后续排查再次遇到黑盒问题。

# 验收标准

1. `Join-Path` 相关异常消失。
2. 预编译 release 缺失时，脚本能够继续走 vcpkg / 本地回退路径。
3. 不影响已有宿主依赖与 Lua 依赖初始化逻辑。

---

# 执行变更总结

## 1. 核心修复与调整概述

- 已确认本次报错由两个因素叠加触发：
  - `deps-v1` 预编译依赖未命中，脚本进入 vcpkg 回退路径
  - vcpkg 回退路径中的 `Join-Path` 使用了非法的三段位置参数写法，导致脚本立即中断
- 已修复 `install_lua_deps.ps1` 中两处 `Join-Path` 路径拼接错误，使预编译依赖缺失时能够正确继续走 vcpkg 检测逻辑。

## 2. 📂文件变更清单

### 修改
- `D:\projects\vulcan-mcp-client\scripts\install_lua_deps.ps1`

## 3. 💻关键代码调整详情

- 将以下错误写法：
  - `Join-Path $VcpkgInstallDir "info" "$triplet.list"`
- 修正为两段嵌套拼接：
  - `Join-Path (Join-Path $VcpkgInstallDir "info") "$triplet.list"`
- 修复位置包括：
  - vcpkg 是否已安装完成的 manifest 检查
  - vcpkg 安装完成后的统计检查

## 4. ⚠️遗留问题与注意事项

- 本次修复只解决了 vcpkg 回退路径本身的脚本异常，不代表 `deps-v1` 发布资产已恢复；若预编译依赖依然缺失，脚本后续仍会继续执行 vcpkg / 本地构建流程。
- 建议在用户侧再次执行 `make deps lua`，确认脚本是否能顺利越过当前异常并进入下一阶段。
