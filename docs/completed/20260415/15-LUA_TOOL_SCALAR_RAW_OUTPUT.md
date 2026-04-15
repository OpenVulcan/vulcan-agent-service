# 任务目标

修复 Lua skill / runlua 的返回值输出策略：当 Lua 返回字符串、整数、浮点数、布尔等基础标量时，MCP 文本结果应原样输出；当返回 table / array / object 时，仍按 JSON 序列化输出，避免普通文本被额外包裹引号与换行转义。

# 执行步骤

1. 梳理 Lua 返回值进入 MCP `tools/call` 与 `--call-tools` 的完整链路，定位当前统一 JSON 化的位置。
2. 在 Rust 宿主侧新增统一格式化逻辑，对 `serde_json::Value` 做类型判断：
   - 标量：原样转为文本
   - 对象/数组：JSON 字符串
3. 将该逻辑应用到 Lua skill 调用链与 `runlua` 调用链，确保行为一致。
4. 进行编译检查与真实调用验证，确认：
   - 纯字符串不会再额外带 JSON 引号
   - 数字按原值文本输出
   - table 仍输出 JSON
5. 完成执行总结并归档计划文件。

# 技术选型

- 保持 Lua -> `serde_json::Value` 的宿主内部桥接不变，只调整“输出到文本内容”这一层的格式化策略，降低影响面。
- 抽取统一的 `Value -> String` 格式化辅助函数，避免 `runlua` 与 Lua skill 分支行为漂移。

# 验收标准

- Lua skill 返回字符串时，正式 `tools/call` 不再得到带引号和 `\\n` 转义的 JSON 字符串文本。
- Lua skill 返回数字/浮点数/布尔时，输出为对应文本值。
- Lua skill 返回 table / array / object 时，仍保持 JSON 文本输出。
- `runlua` 与普通 Lua skill 行为一致。
- 编译通过，并完成至少一轮真实调用验证。

# 执行变更总结

## 1. 核心修复与调整概述

- 在 Rust 宿主侧新增统一的 `Value -> 文本` 格式化逻辑，基础标量原样输出，数组和对象继续序列化为 JSON。
- 将该逻辑同时接入正式 MCP `tools/call` 的 Lua skill / `runlua` 分支，以及本地 `--call-tools` 调试模式。
- 保持 Lua 到 `serde_json::Value` 的桥接不变，仅修正“最终输出到文本层”的格式化策略，降低影响面。

## 2. 📂文件变更清单

### 新增

- `docs/plan/20260415-15-LUA_TOOL_SCALAR_RAW_OUTPUT.md`

### 修改

- `src/server.rs`
- `src/main.rs`

## 3. 💻关键代码调整详情

- 在 `src/server.rs` 中新增 `format_json_value_for_text`：
  - `String` -> 原样文本
  - `Number` -> 数字文本
  - `Bool` -> `true/false`
  - `Null` -> `null`
  - `Array/Object` -> `serde_json::to_string(...)`
- 将上述格式化函数应用到：
  - Lua `runlua` 返回结果
  - 普通 Lua skill 返回结果
- 在 `src/main.rs` 中新增 `print_call_tools_result`，让 `--call-tools` 按同样规则输出：
  - 标量直接打印
  - 数组/对象继续按 pretty JSON 打印
- 实际验证结果：
  - `codekit-ast-tree` 的字符串结果不再带外层引号和 `\\n`
  - `codekit-ast` 的 table 结果仍保持 JSON 输出

## 4. ⚠️遗留问题与注意事项

- 本次实际验证覆盖了 `--call-tools` 行为，以及正式 `tools/call` 使用的同一格式化逻辑路径；但未额外启动 HTTP/gRPC 做端到端网络回归。
- `runlua` 未单独做一轮真实调用，但已复用与 Lua skill 相同的宿主格式化策略。
- 当前 `return_type` 元数据仍未被运行时消费；这次修复依赖的是“最终 JSON Value 输出策略”，不是 `return_type` 分流。
