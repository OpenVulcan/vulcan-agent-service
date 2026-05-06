# Host Render Boundary & Pagination Protocol v0.1

## 1. 文档目标

本文用于定义 host/render 层与 runtime 层在超限输出处理上的职责边界。

本文重点回答：

- 分页应该由谁执行
- 截断应该由谁执行
- 模板建议由谁给出、由谁真正使用
- 超限文件应由谁落盘、落到哪里

## 2. 核心结论

### 2.1 分页不在 runtime

runtime 只负责返回：

- 完整内容
- split 建议
- template 建议
- bytes/lines 等预算感知信息

真正的分页切块，应由 host 执行。

### 2.2 截断不在 runtime

runtime 可以建议：

- 推荐截断
- 推荐展示模式

但真正的截断行为，应由 host 执行。

### 2.3 模板处理权最终在 host

模板规则可以是通用的，模板资产可以由 skill 提供，但：

- runtime 只负责给出模板建议
- host 决定是否采用该模板
- host 决定如何结合自身展示模型进行拼装

### 2.4 落盘位置由 host 决定

如果宿主决定把超限内容写入文件：

- 是否写
- 写到哪里
- 文件命名方式
- 返回给用户什么路径/引用

都应由 host 决定，而不是 runtime。

## 3. Runtime 与 Host 的职责分工

### 3.1 Runtime 负责

- 返回完整内容
- 返回内容字节数/行数
- 返回是否触发超限建议
- 返回 split hint
- 返回 template hint
- 返回 diagnostics

### 3.2 Host 负责

- 判断是否真正 inline
- 判断是否真正 truncate
- 判断是否真正 paginate
- 判断是否写入超限文件
- 决定文件存放位置
- 生成最终用户可见输出

## 4. 宿主为何必须掌握渲染权

因为不同宿主的交互模式差异很大。

### 4.1 `vulcan-agent-service` 的 MCP / 通用适配面

在 `vulcan-agent-service` 中，MCP / 通用协议适配面通常需要：

- 文本型结果
- 分页指针块
- 后续读取入口
- 与 MCP 客户端兼容的结构

### 4.2 IDE 宿主

IDE 宿主通常可以：

- 直接折叠大结果
- 把结果写入项目目录
- 提供点击展开入口
- 提供 `/` 或 `$` 工作流快捷入口

因此如果把分页/截断逻辑写死在 runtime，会显著降低宿主自由度。

## 5. 存储边界

### 5.1 Runtime 不决定落盘

runtime 不应负责：

- 创建分页文件
- 创建截断文件
- 创建宿主可读的中间产物目录

### 5.2 Host 决定落盘位置

典型例子：

- `vulcan-agent-service`
  - 当前通常只能将超限产物放到自己的运行目录体系或托管目录中
- IDE 宿主
  - 则可以将超限产物放到项目目录，例如 `.vscode/` 等宿主管理目录

因此协议上不应要求 runtime 返回已落盘文件路径。

### 5.3 Host 可选择完全不落盘

宿主也可以：

- 直接内存分页
- 只展示摘要
- 提供交互式展开

这再次说明分页/截断与存储不应下沉到 runtime。

## 6. 建议的宿主输入

host 在消费 `Runtime Invocation Result` 时，至少应关注：

- `content_blocks`
- `budget_hint`
- `render_hint`
- `split_hint`
- `template_hint`
- `diagnostics`

然后根据自身策略决定：

- 最终输出模式
- 是否落盘
- 是否生成后续读取入口

## 7. 与超限模板的关系

`overflow_templates/` 目录可以保留，但其使用方式应为：

- runtime 返回模板建议
- host 选择是否采用
- host 结合自己的分页与展示逻辑进行真正拼装

也就是说：

- 模板资产可以由 skill 提供
- 模板处理规则可以是协议通用的
- 但最终的渲染决定权必须留给 host

## 8. 一句话结论

Host Render Boundary 的核心原则是：runtime 只返回完整内容与建议，host 才真正决定是否分页、是否截断、是否落盘以及最终如何向用户呈现。
