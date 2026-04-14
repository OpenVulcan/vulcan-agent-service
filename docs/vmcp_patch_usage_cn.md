# vmcp-patch 使用说明

`vmcp-patch` 是一个**函数级 AST 重定位整段替换工具**。

它的定位非常明确：

1. 重新扫描目标文件 AST
2. 通过结构 selector 重新定位函数/方法
3. 用你传入的**完整函数源码**替换目标函数

它**不是**文本 diff 工具，也**不是**函数体局部 patch 工具。

## 一、严格规则

`vmcp-patch` 只接受这一种输入形态：

- `replacement` 必须是**完整函数源码**
- 必须从函数声明行开始
- 不允许只传函数体
- 不允许传 `mode`

也就是说，下面这种才是合法输入：

```rust
pub async fn with_vmm(self, endpoint: &str) -> Result<Self, String> {
    let client = endpoint.to_uppercase();
    eprintln!("patched {}", client);
    Ok(Self { id: self.id + 1 })
}
```

下面这种会被拒绝：

```rust
let client = endpoint.to_uppercase();
eprintln!("patched {}", client);
Ok(Self { id: self.id + 1 })
```

这样做的原因很简单：

- 不做兼容猜测
- 不做 `body/auto/full` 多模式分流
- 不把 AI 的“可能是函数体，也可能是完整函数”当成合法输入

规则越死，工具越稳。

## 二、适合使用的场景

优先用于以下场景：

- 已经通过 `vmcp-ast` 或 `vmcp-rg` 找到了目标函数
- 目标函数所在文件仍存在，但行号已经漂移
- 希望整段替换函数，而不是按旧行号做 patch
- 希望避免传统文本 diff 带来的错位风险

不适合用于以下场景：

- 修改类名、结构名、字段名、局部变量名
- 修改 import / use / require 等非函数节点
- 修改函数内部某几行但又不愿意提供完整函数源码

`vmcp-patch` 当前只支持 patch `function / method` 这类完整代码节点。

## 三、参数说明

### `file`

目标文件完整路径。

要求：

- 必须存在
- 必须是普通文件

### `selector`

用于重定位函数的结构选择器。

支持宽松表达，例如：

- `with_vmm`
- `McpServer/with_vmm`
- `impl McpServer/with_vmm`
- `fn with_vmm`
- `pub async fn with_vmm`

匹配原则是：

- 优先做路径后缀匹配
- 同时支持函数声明文本别名匹配
- 如果唯一命中，则直接 patch
- 如果命中多个，则返回候选结构路径，要求继续收紧 selector

### `replacement`

完整函数源码。

要求：

- 必须包含目标函数名
- 必须从函数声明开始
- 不能只传函数体片段

## 四、推荐使用流程

### 场景 1：先用 `vmcp-rg` 找目标函数，再 patch

1. 先调用 `vmcp-rg`
2. 看结果里的结构树
3. 选一个最短但足够唯一的 selector
4. 准备完整函数源码
5. 调用 `vmcp-patch`

示例思路：

- `vmcp-rg` 找到：
  - `impl McpServer`
  - `pub async fn with_vmm`

那么可以先尝试：

- `McpServer/with_vmm`

如果歧义，再升级成：

- `impl McpServer/pub async fn with_vmm`

## 五、调用示例

```json
{
  "file": "D:\\projects\\vulcan-mcp-client\\src\\server.rs",
  "selector": "McpServer/with_vmm",
  "replacement": "pub async fn with_vmm(self, endpoint: &str) -> Result<Self, Box<dyn std::error::Error>> {\\n    let client = VmmClient::connect(endpoint).await?;\\n    eprintln!(\"[MCP] VMM client connected: {}\", endpoint);\\n    Ok(Self { vmm: Some(client), ..self })\\n}"
}
```

## 六、安全机制

`vmcp-patch` 当前已经内置以下安全机制：

- patch 前会重新扫描 AST 定位目标函数
- 如果 selector 命中多个函数，不会盲改
- 写入前会先生成临时文件
- 临时文件会先做 AST 校验
- 校验不是某一种语言编译器专属校验，而是基于 ast-grep 的通用 `ERROR` 节点扫描
- 真正替换后还会再次做 AST 校验与目标函数重定位校验
- 如果最终校验失败，会回滚原文件

这意味着：

- 多写一个 `}`、多写一个 `end`、缺括号、缺关键结构，都有机会被直接拦下
- 语法损坏不会静默写入成功
- 工具会优先保证文件结构仍可被 AST 工具继续读取

## 七、失败后的处理方式

如果 patch 失败，优先根据错误类型处理：

### 1. `ambiguous_selector`

说明 selector 太短，同时命中了多个函数。

处理方式：

- 直接使用返回的 `candidates[].path`
- 选择更长的结构路径重试

### 2. `replacement_must_be_full_function`

说明你传的不是完整函数源码，而是函数体片段、空内容，或没有从声明开始。

处理方式：

- 补全整个函数定义
- 从函数声明行开始重新传入
- 不要再传 `mode`

### 3. `syntax_error_nodes_detected`

说明 replacement 写入后产生了解析错误节点。

常见原因：

- 多写了一个 `}`
- 多写了一个 `end`
- 漏掉分隔符、括号或关键结构

处理方式：

- 参考返回的 `details.diagnostics`
- 优先检查第一处报错附近的结构闭合是否正确

### 4. `patched_target_not_found` / `patched_target_ambiguous`

说明 patch 后工具已经无法把结果重新识别为原来的那一个函数。

处理方式：

- 检查 replacement 是否把函数名改掉了
- 检查是否把父级结构上下文一起改坏了
- 尽量保持目标函数的结构身份稳定

### 5. `patched_target_identity_changed`

说明 replacement 把目标函数改成了另一个结构身份。

处理方式：

- 保持函数名一致
- 保持目标函数仍然是原来的那个 function/method

## 八、最佳实践

建议遵循这几条：

1. 先 `vmcp-rg` 或 `vmcp-ast`，后 `vmcp-patch`
2. selector 从短到长逐步收敛
3. replacement 永远传完整函数源码
4. patch 后最好再跑一次 `vmcp-ast` 或 `vmcp-rg` 做结果确认
5. 不要把 `vmcp-patch` 当作“顺便改几行”的工具

## 九、当前边界

当前版本的优点是规则极其清晰：

- 不猜测
- 不兼容函数体片段
- 不区分 `body/full/auto`
- 只做完整函数替换

这会牺牲一部分灵活性，但换来更低的误用率和更高的稳定性。
