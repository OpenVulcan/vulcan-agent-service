## 任务目标

为 `codeview_ast` 实现更稳健的大结果返回机制，重点完成以下事项：

1. 调整搜索约束：
   - 指定文件搜索时，文件数量不可超过 20 个；
   - 不允许“文件路径 + 目录路径”混用；
   - 目录搜索时返回相对路径；
   - 指定文件搜索时返回对应输入文件路径。
2. 优化输出格式：
   - 主节点不做无意义缩写；
   - 关闭输入参数原样回显；
   - 使用压缩 JSON 返回，避免数组和对象逐项换行；
   - 去掉 `language` 说明；
   - `linecount` 更名为 `lines`。
3. 为分页能力设计并实现通用缓存：
   - 由 Vulcan 主程序提供带时效的共享缓存能力；
   - 缓存创建时必须传入工具标记，避免不同工具冲突；
   - 默认有效期使用系统默认值；
   - 最大有效期 30 分钟，可配置；
   - 最大缓存条目数默认 1000，可配置；
   - 超出有效期自动删除；
   - 主程序维护缓存并处理并发锁。
4. 在 `codeview_ast` 中接入缓存与分页：
   - 目录场景按内容预算分页；
   - 后续分页通过缓存编号续取；
   - 返回清晰的分页与缓存信息。

## 执行步骤

1. 定位 `codeview_ast` 的 Lua 技能实现、技能元数据以及当前输出 JSON 生成逻辑。
2. 定位 Vulcan 主程序中适合承载“带 TTL 的共享缓存”的模块与扩展点。
3. 设计缓存数据结构、配置项、过期清理与并发访问策略。
4. 实现主程序缓存能力，并提供 Lua 可调用接口。
5. 改造 `codeview_ast`：
   - 增加路径校验与约束；
   - 调整输出字段；
   - 启用压缩 JSON；
   - 接入缓存式分页。
6. 进行本地验证，覆盖：
   - 指定文件数量超限；
   - 文件/目录混用；
   - 相对路径/指定路径输出差异；
   - 首次查询缓存创建；
   - 翻页读取；
   - 过期与非法缓存编号处理。
7. 完成执行变更总结并归档。

## 技术选型

- Lua 技能层负责搜索结果整理、路径规则、分页切片与返回结构。
- Rust 主程序负责缓存容器、TTL、上限控制、清理与锁机制。
- 返回格式优先保证模型稳定理解，不做主节点缩写，但使用紧凑 JSON 序列化降低体积。

## 验收标准

1. `codeview_ast` 能正确拒绝非法路径组合与超量指定文件。
2. 返回 JSON 为紧凑格式，且字段调整符合要求。
3. 主程序具备通用共享缓存能力，支持 TTL、条目上限与工具隔离。
4. `codeview_ast` 可通过缓存编号稳定分页。
5. 相关改动完成验证，并形成中文结论与归档记录。

---

## 执行变更总结

### 1. 核心修复与调整概述

本次完成了 `codeview_ast` 的分页、紧凑输出与主程序级共享缓存底座改造，核心结果如下：

1. `codeview_ast` 现在禁止文件路径与目录路径混用，并将显式文件搜索上限收紧为 20 个。
2. 目录扫描结果改为相对路径展示，显式文件搜索则保留对应输入文件路径。
3. 文件结果结构移除了 `language` 字段，`line_count` 改为 `lines`，并停止回显 `path/paths/recursive/ignore/comment` 等原始输入参数。
4. Lua 技能与 `runlua` 的返回改为紧凑 JSON，不再使用 `pretty` 格式，减少结果外壳体积。
5. 新增主程序级共享缓存能力，支持：
   - 工具命名空间隔离
   - TTL
   - 默认/最大 TTL 配置
   - 最大条目数配置
   - 自动过期清理
   - 超限淘汰最旧条目
6. `codeview_ast` 增加了 `cache_id`、`page`、`truncate_chars`、`cache_ttl_sec` 参数。
7. 分页默认字符预算为 `20000`，且在工具说明中明确“不建议手动覆盖，除非用户明确要求或提示词要求”。
8. 只有实际发生分页/截断时才会创建缓存；单页结果不会生成缓存条目。

### 2. 📂文件变更清单

新增：
- `D:\projects\vulcan-mcp-client\src\tool_cache.rs`

修改：
- `D:\projects\vulcan-mcp-client\src\config.rs`
- `D:\projects\vulcan-mcp-client\src\main.rs`
- `D:\projects\vulcan-mcp-client\src\lua_engine.rs`
- `D:\projects\vulcan-mcp-client\src\server.rs`
- `D:\projects\vulcan-mcp-client\runtime\lua_skills\codeview_ast\main.lua`
- `D:\projects\vulcan-mcp-client\runtime\lua_skills\codeview_ast\skill.json`
- `D:\projects\vulcan-mcp-client\output\lua_skills\codeview_ast\main.lua`
- `D:\projects\vulcan-mcp-client\output\lua_skills\codeview_ast\skill.json`
- `D:\projects\vulcan-mcp-client\docs\plan\20260414-02-CODEVIEW_AST_PAGINATION_AND_CACHE.md`

删除：
- 无

### 3. 💻关键代码调整详情

1. 主程序共享缓存
   - 新增 `src/tool_cache.rs`
   - 提供进程级共享缓存容器
   - 通过 `RwLock + AtomicU64 + OnceLock` 管理并发访问、缓存编号与全局单例
   - 支持 `create / get / delete`
   - 支持默认 TTL、最大 TTL、最大条目数限制与自动清理

2. Lua 运行时能力扩展
   - 在 `src/lua_engine.rs` 的 `vulcan` 模块中新增：
     - `vulcan.cache_put(tool_name, value, ttl_sec?)`
     - `vulcan.cache_get(tool_name, cache_id)`
     - `vulcan.cache_delete(tool_name, cache_id)`
   - 供 Lua 技能实现共享短时分页缓存

3. 主程序配置项扩展
   - 在 `src/config.rs` 新增：
     - `tool_cache_max_entries`
     - `tool_cache_default_ttl_secs`
     - `tool_cache_max_ttl_secs`
   - 在 `src/main.rs` 启动阶段初始化全局工具缓存配置

4. 紧凑 JSON 输出
   - `src/server.rs` 中 `runlua` 与 Lua skill 返回由 `serde_json::to_string_pretty(...)` 改为 `serde_json::to_string(...)`
   - 直接减少 MCP 文本输出体积

5. `codeview_ast` 规则与分页
   - 新增路径模式判定，拒绝文件/目录混用
   - 新增显式文件数量上限 20
   - 目录模式输出相对路径
   - 使用紧凑 JSON 长度估算单页体积
   - 默认每页 `20000` 字符预算
   - 新增分页结果字段：
     - `page`
     - `total_pages`
     - `has_next_page`
     - `next_page`
     - `cache_id`
     - `truncated`
   - 仅在多页时创建缓存

### 4. ⚠️遗留问题与注意事项

1. `cache_put/cache_get` 属于主程序新增能力，运行中的旧 `vulcan-mcp` 进程不会自动获得；必须重启服务后分页缓存才会真正生效。
2. 已完成 `cargo build` 编译验证，且当前会话中的 `codeview_ast` 已验证到：
   - `mixed_path_modes_not_supported`
   - `too_many_explicit_files`
   - 新字段 `lines/page/total_pages/truncated`
   说明 Lua 逻辑更新已生效。
3. 当前会话连接的老进程在触发分页缓存时会报 `cache_put` 缺失，这不是新代码错误，而是旧进程未重启导致的能力差异。
4. 紧凑 JSON 的主程序级实际返回效果，同样需要重启到新二进制后才能完全体现在外部调用上。
