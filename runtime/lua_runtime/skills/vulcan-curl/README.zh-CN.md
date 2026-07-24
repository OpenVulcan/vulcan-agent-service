# vulcan-curl

面向 Vulcan Agent 的 AI 原生 HTTP 请求技能。

`vulcan-curl` 为 Vulcan Agent 提供一层适合 AI 调用的 HTTP 请求能力。常见 API 调试、Webhook 联调和下载排查可以使用结构化 `GET` 和 `POST` 入口，复杂请求则可以使用较底层的受支持 curl-style argv 入口，同时避免 Agent 自己拼接平台相关 shell 命令和引号转义。

## 什么时候使用

当 Agent 需要发起一次偏 API 调试 / 接口验证场景、输入可控、输出可读的 HTTP 请求时使用 `vulcan-curl`：

- 检查 API 响应、Webhook 投递结果、请求头/响应头、鉴权行为或可下载文件。
- 使用结构化 query params 和 headers，避免手写 URL 字符串。
- 使用 Bearer 或 Basic Auth 快捷参数。
- 发送 JSON、form、multipart 或 raw POST body。
- 将响应体或响应头保存到本地文件。
- 需要受支持的 curl-style argv 语义，但不想依赖 shell 引号规则。

不要把 `vulcan-curl` 当成网页抓取工具来读取网页正文，也不要指望它把 HTML 自动转换成 Markdown。交互式网页流程、JS 渲染 UI 测试、截图或网页内容检查应使用浏览器工具。只有在确实要测试原始终端 curl 行为时，才使用普通 shell。

## 工具

### `vulcan-curl-get`

用于简单 API / 调试类 HTTP 读取。

常用参数包括：

- `url`
- `params` 或 `params_list`
- `headers` 或 `header_lines`
- `bearer`、`basic` 或 `basic_text`
- `timeout_ms`
- `follow_location`
- `download_to`
- `save_headers_to`
- `flags`

### `vulcan-curl-post`

用于简单 API 写入、Webhook 测试或上传类请求，支持一种主要 payload 类型：

- `json`
- `form` / `form_lines`
- `files` / `file_lines`
- `body`

每次请求只应选择一种 payload 类型。当请求形态超出结构化 POST schema 时，切换到 `vulcan-curl-request`。

### `vulcan-curl-request`

当需要受支持的 curl-style argv 语义来处理更复杂的 API / HTTP 调试请求，但不希望依赖平台相关 shell 引号规则时使用。

它适合运行时解析器已覆盖的高级 TLS、代理、multipart 上传、重试和不常见 API / HTTP 请求组合。请求仍在 Lua runtime 层执行，而不是通过 shell 执行。

`args` 是 Lua runtime 解析的 curl-style 支持子集，不是完整 curl CLI。它不支持 shell 展开、stdin/TTY 交互、交互式提示、curl 配置文件或未实现的 `-` 开头 curl 选项；未知 curl 选项会直接报错。

## TLS 与代理证书

在 Windows 上，当请求没有显式提供 CA 文件时，`vulcan-curl` 会为目标 TLS 与 HTTPS 代理 TLS 启用 libcurl 原生 CA 查找。这样代理证书可以走 Windows 信任库完成校验，不需要关闭证书验证。

在 Linux 和 macOS 上，libcurl 会使用宿主构建所选择的 CA bundle 或信任后端。如果 HTTPS 代理使用的私有 CA 或企业 CA 不在该后端可见范围内，应通过 `vulcan-curl-request` 传入 `--proxy-cacert` 或 `--proxy-capath`。目标站点证书链仍使用 `--cacert` 或 `--capath`，`--proxy-insecure` 仅建议用于临时诊断。

## 输出控制

`flags` 是逗号分隔的渲染开关：

- `request-header`：展示请求细节
- `response-header`：展示响应头

未知 flag 会被忽略。响应体默认内联展示；使用 `download_to` 时会保存到文件。内联内容始终是原始 HTTP 响应，不会做网页渲染或 Markdown 转换。响应头可以通过 `save_headers_to` 保存。

## 运行时要求

`vulcan-curl` 期望宿主 Lua runtime 提供：

- `lcurl.safe`
- `socket`

这些运行时模块不会打包进本仓库。skill 包只包含 LuaSkill 运行时代码、帮助内容和发布元数据。

## 验证

```powershell
python .\scripts\validate_skill.py
python .\scripts\package_skill.py
```

发布包会生成在 `dist/` 下：

- `vulcan-curl-v<version>-skill.zip`
- `vulcan-curl-v<version>-checksums.txt`

## 说明

- 仓库根目录就是 skill 根目录。
- 安装后的 skill id 来自包根目录名：`vulcan-curl`。
- 输出面向 AI Agent 设计：结构化、默认简洁，并明确区分请求/响应渲染选项。
