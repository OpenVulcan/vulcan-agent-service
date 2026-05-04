# Vulcan Curl Help

这是 `vulcan-curl` 的主帮助节点。

当前推荐工作流：

- `vulcan-curl-get`
  - 适合简单 GET 请求
- `vulcan-curl-post`
  - 适合简单 POST / multipart 请求
- `vulcan-curl-request`
  - 适合需要受支持 curl 风格 argv 子集的基础请求入口
  - 需要 HTTPS 代理证书控制时，使用 `--proxy-cacert`、`--proxy-capath` 或 `--proxy-insecure`
  - 不支持 shell 展开、stdin/TTY 交互、交互式提示或未实现的 curl 选项
