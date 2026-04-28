# vulcan-testkit

面向 Vulcan 编码 Agent 的 AI 原生验证路由器。

`vulcan-testkit` 用于运行有边界的 build、test、lint、check、typecheck 工作流，避免把冗长终端输出直接塞回上下文。它可以执行显式验证命令，也可以分析已有日志，并通过 profile 防护、日志解析和噪声折叠，返回聚焦根因与下一步动作的 Markdown 报告。

## 什么时候使用

当 Agent 需要准确、有边界、可行动的验证反馈时使用 `vulcan-testkit`：

- 代码修改后运行构建、测试、检查、lint 或类型检查。
- 将很长的验证日志压缩成根因诊断。
- 提取失败测试、源码引用、折叠噪声和下一步动作。
- 避免把大段 stdout/stderr 占满模型上下文。
- 防止验证命令被悄悄变成应用服务、安装器、watcher、调试器、benchmark、fuzz 或写入/修复命令。

短小且低噪声、需要原始输出的命令可以直接用普通 shell。文件、Git、部署等非验证工作应使用对应专用工具。

## 工具

### `vulcan-testkit-run`

当下一步动作是验证时使用这个入口。

运行模式需要传入直接允许的可执行文件和参数：

```yaml
program: cargo
args:
  - test
cwd: path/to/project
phase: test
tool_hint: cargo
```

只分析已有日志时：

```yaml
log: "<existing validation output>"
phase: check
tool_hint: cargo
```

返回的 Markdown 报告通常包含：

- `Status`
- `Run`
- `Summary`
- `Root Diagnostics`
- `Failed Tests`
- `Source Refs`
- `Collapsed Noise`
- `Next Actions`

## 支持的验证 Profile

TestKit 有意限制为验证场景。它允许验证导向的命令形态，并拒绝可能修改文件、启动长生命周期进程、安装包、发布产物或打开交互工具的命令形态。

支持范围包括：

- Rust：`cargo check`、`cargo test`、`cargo clippy`、`cargo build`
- Go：`go test`、`go vet`、`go build`
- Python：`python -m pytest`、`python -m unittest`、`python -m mypy`、`python -m ruff`
- Node：`node --check <file>`、`node --test ...`
- TypeScript：`tsc --noEmit ...`
- 包管理器：受约束的 `npm`、`pnpm`、`yarn` 验证脚本
- 已有日志分析：通过 detector 与 adapter 启发式路由

会被拒绝的例子包括应用运行命令、包安装、项目创建、formatter 写入/修复模式、打开浏览器、watcher、开发服务器、benchmark、fuzz 和交互式调试。

## 验证

```powershell
python .\scripts\validate_skill.py
python .\scripts\package_skill.py
```

发布包会生成在 `dist/` 下：

- `vulcan-testkit-v<version>-skill.zip`
- `vulcan-testkit-v<version>-checksums.txt`

## 说明

- 仓库根目录就是 skill 根目录。
- 安装后的 skill id 来自包根目录名：`vulcan-testkit`。
- 运行时代码不捆绑外部验证工具，而是路由调用方环境中已有的工具。
- 输出面向 AI Agent 设计：紧凑、聚焦源码，并明确说明被阻止或参数错误的验证调用。
