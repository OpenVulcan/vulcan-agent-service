# Vulcan Curl Help

这是 `vulcan-curl` 的主帮助节点。

当前推荐工作流：

- `vulcan-curl-get`
  - 适合 API 调试、Webhook 校验和文件下载类的简单 GET 请求
- `vulcan-curl-post`
  - 适合 API 写入、Webhook 测试和 multipart 上传类的简单 POST 请求
- `vulcan-curl-request`
  - 适合需要受支持 curl 风格 argv 子集的高级 API / HTTP 调试入口
  - 需要 HTTPS 代理证书控制时，使用 `--proxy-cacert`、`--proxy-capath` 或 `--proxy-insecure`
  - 不支持 shell 展开、stdin/TTY 交互、交互式提示或未实现的 curl 选项

边界说明：

- 该 skill 主要面向 API 调试、接口验签、请求头/响应头排查、Webhook 联调和文件下载验证
- 该 skill 返回的是原始 HTTP 响应内容，不负责网页渲染、HTML 提取或 Markdown 转换
- 需要抓取网页正文、处理 JS 渲染页面或把网页内容整理成适合阅读的格式时，应改用浏览器类工具
