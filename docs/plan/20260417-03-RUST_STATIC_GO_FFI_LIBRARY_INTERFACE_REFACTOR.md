# Rust 静态调用与 Go FFI 接口分层重构计划

## 任务目标

围绕 `vldb-sqlite` 与 `vldb-lancedb` 两个库，建立统一的库侧调用分层策略：

1. **MCP / Rust 调用侧**
   - 优先使用 Rust 原生静态库接口；
   - 不走 JSON，不走动态库字符串协议；
   - 直接依赖 typed Rust API。

2. **Go 调用侧**
   - 通过稳定的 C ABI / FFI 接入；
   - 主路径不走 JSON；
   - 使用扁平化句柄与 getter 风格接口。

3. **JSON 接口**
   - 保留为兼容层；
   - 主要服务 Lua / Python / 调试 / 其他潜在项目；
   - 不再作为 Rust 或 Go 的主路径设计中心。

## 总体原则

### 1. Rust 直连优先

- `vulcan-mcp-client` 作为 Rust 宿主，应优先走：
  - `vldb-sqlite` Rust typed API
  - `vldb-lancedb` Rust typed API
- 尽量避免在 Rust 内部再通过 FFI 或 JSON 做无意义绕行。

### 2. Go 走稳定 FFI

- Go 侧以 `cgo + C ABI` 为主；
- FFI 必须满足：
  - 不暴露复杂 Rust 结构体；
  - 不要求 Go 理解内部内存布局；
  - 高频核心路径不走 JSON。

### 3. JSON 仅做兼容层

- 保留现有 `*_json` 接口；
- 作为：
  - 脚本语言兼容层
  - 调试层
  - 未来非强类型接入预留层
- 不再扩展为主接口模型。

### 4. 不改变工具职责

- `vldb-sqlite`
  - 继续只负责 SQLite / FTS / 分词 / BM25 / 词典能力；
- `vldb-lancedb`
  - 继续只负责向量存储与向量检索；
- 不引入混合召回接口；
- 结果融合仍然属于调用侧职责。

## 详细执行步骤

### 步骤 1：盘点两个库的现状

分别梳理：

- `vldb-sqlite`
  - 当前 Rust 可复用 API
  - 当前 FFI JSON 接口
  - 当前 gRPC 接口
- `vldb-lancedb`
  - 当前 Rust 可复用 API
  - 当前 FFI JSON 接口
  - 当前 gRPC 接口

输出结果：
- 哪些接口已经适合 Rust 直连；
- 哪些接口仍然只有 JSON 兼容层；
- 哪些高频路径最值得优先脱离 JSON。

### 步骤 2：定义统一分层规范

为两个库统一定义三层：

1. **Rust API 层**
   - 强类型
   - `Result<T, E>`
   - 供 MCP / Rust 直接调用

2. **C ABI 主接口层**
   - 供 Go 调用
   - 使用句柄、标量、字符串、bytes
   - 高频路径不走 JSON

3. **JSON 兼容层**
   - 继续保留
   - 不作为主设计目标

### 步骤 3：先重构 `vldb-lancedb`

原因：
- 向量写入 / 向量检索走 JSON 的损耗最大；
- 这里收益最高。

目标：
- 设计并实现：
  - Rust typed runtime / database / table API
  - C ABI 主接口
  - JSON 兼容层保留

重点关注：
- 向量输入不要走 JSON
- 查询结果不要直接回整坨 JSON 作为主协议

### 步骤 4：再重构 `vldb-sqlite`

基于现有已完成的：
- 多库 runtime
- FTS
- tokenizer
- `_vulcan_dict`

继续补：
- Rust typed 主接口收敛
- C ABI 主接口
- JSON 兼容层降级为次要入口

### 步骤 5：宿主接入策略调整

在 `vulcan-mcp-client` 中：

- `vldb-sqlite`
  - 改为优先静态 Rust 接入
- `vldb-lancedb`
  - 评估是否也改为 Rust 静态接入
  - 若暂时保留动态库，则后续平滑迁移

同时明确：
- Go 项目继续通过 FFI 接入；
- JSON 层不再作为宿主核心调用路径。

### 步骤 6：文档同步

分别更新：
- `vldb-sqlite`
- `vldb-lancedb`
- `vulcan-mcp-client`

文档里明确写出：
- Rust 推荐调用方式
- Go 推荐调用方式
- JSON 兼容层定位

## 技术约束

### Rust API

- 必须强类型；
- 不允许通过 JSON 绕行；
- 尽量直接暴露 runtime / handle / result struct。

### C ABI

- 只暴露稳定 C 风格接口；
- 使用 opaque handle；
- 高频结果走 handle + getter；
- 不暴露复杂 Rust 结构体；
- 错误信息与资源释放机制必须清晰。

### JSON 兼容层

- 保留现有接口兼容性；
- 新增能力如必须提供 JSON 版本，也要明确标注为 compatibility。

## 验收标准

完成后必须满足：

1. `vulcan-mcp-client` 的 Rust 主路径不再依赖 JSON 调库
2. Go 可通过稳定 FFI 高效调用两个库
3. 向量与 FTS 高频核心路径不再以 JSON 为主协议
4. JSON 接口仍保留，兼容现有脚本语言与调试用途
5. 两个库的职责边界不被混合召回逻辑污染
6. 文档明确区分 Rust 主接口、Go FFI 主接口和 JSON 兼容接口

---

## 阶段执行记录

### 阶段一：`vldb-lancedb` 非 JSON 主接口落地

已完成内容：

- 在 `vldb-lancedb` 的 FFI 层补齐了面向 Go / 原生宿主的非 JSON 主接口：
  - `vldb_lancedb_engine_vector_upsert_raw`
  - `vldb_lancedb_engine_vector_search_f32`
- 新增并对外导出了稳定的 C ABI 结构与枚举：
  - `VldbLancedbStatusCode`
  - `VldbLancedbFfiInputFormat`
  - `VldbLancedbFfiOutputFormat`
  - `VldbLancedbUpsertResultPod`
  - `VldbLancedbSearchResultMeta`
- 同步更新了：
  - `include/vldb_lancedb.h`
  - `README.md`
  - `docs/LIBRARY_USAGE.zh-CN.md`
- 当前文档口径已经明确区分：
  - Rust / MCP：优先使用 typed Rust API
  - Go / 原生宿主：优先使用非 JSON FFI 主接口
  - JSON FFI：仅作为兼容层保留

验证结果：

- `cargo check` 通过
- `cargo test --lib` 通过

当前判断：

- `vldb-lancedb` 的高频向量路径已经开始脱离 JSON
- 后续如果继续打磨库侧接口，应优先对 `vldb-sqlite` 做同样的“Rust typed + Go FFI + JSON compatibility”分层收口

### 阶段二：`vldb-sqlite` 非 JSON 主接口落地

已完成内容：

- 在 `vldb-sqlite` 的 FFI 层补齐了面向 Go / 原生宿主的非 JSON 主接口：
  - `vldb_sqlite_runtime_create_default`
  - `vldb_sqlite_runtime_open_database`
  - `vldb_sqlite_runtime_close_database`
  - `vldb_sqlite_database_tokenize_text`
  - `vldb_sqlite_database_upsert_custom_word`
  - `vldb_sqlite_database_remove_custom_word`
  - `vldb_sqlite_database_list_custom_words`
  - `vldb_sqlite_database_ensure_fts_index`
  - `vldb_sqlite_database_rebuild_fts_index`
  - `vldb_sqlite_database_upsert_fts_document`
  - `vldb_sqlite_database_delete_fts_document`
  - `vldb_sqlite_database_search_fts`
- 新增并对外导出了稳定的 C ABI 结构与句柄：
  - `VldbSqliteStatusCode`
  - `VldbSqliteFfiTokenizerMode`
  - `VldbSqliteRuntimeHandle`
  - `VldbSqliteDatabaseHandle`
  - `VldbSqliteTokenizeResultHandle`
  - `VldbSqliteCustomWordListHandle`
  - `VldbSqliteSearchResultHandle`
  - `VldbSqliteDictionaryMutationResultPod`
  - `VldbSqliteEnsureFtsIndexResultPod`
  - `VldbSqliteRebuildFtsIndexResultPod`
  - `VldbSqliteFtsMutationResultPod`
- 同步更新了：
  - `include/vldb_sqlite.h`
  - `README.md`
  - `docs/LIBRARY_USAGE.zh-CN.md`
- 当前文档口径已经明确区分：
  - Rust / MCP：优先使用 typed Rust API
  - Go / 原生宿主：优先使用非 JSON FFI 主接口
  - JSON FFI：仅作为兼容层保留

验证结果：

- `cargo check` 通过
- `cargo test --lib` 通过

### 阶段三：`vulcan-mcp-client` 静态接入 `vldb-lancedb`

已完成内容：

- 在 `vulcan-mcp-client` 中移除了 `lancedb_host` 对动态库加载与 `libloading` 的依赖：
  - `src/lancedb_host.rs`
  现在直接改为基于 `vldb-lancedb` 的 typed Rust API 做宿主包装。
- `Cargo.toml` 新增了静态依赖：
  - `vldb_lancedb = { package = "vldb-lancedb", git = "...", rev = "dc49e964..." }`
- 当前宿主侧 `LanceDbSkillHost / LanceDbSkillBinding` 对 Lua 暴露的方法名保持不变：
  - `status_json`
  - `info_json`
  - `create_table_json`
  - `vector_upsert_json`
  - `vector_search_json`
  - `delete_json`
  - `drop_table_json`
- 但内部实现已经改为：
  - 基于 `LanceDbRuntime`
  - `LanceDbEngine`
  - `LanceDb*Input` typed 结构
  - 通过同步互斥 + 每 skill 专属 Tokio runtime 驱动异步 LanceDB 操作
- 宿主状态输出也同步收口为静态接入口径：
  - `integration_mode = "rust_static"`
  - `library_path = null`

验证结果：

- `cargo check` 通过

当前判断：

- `vulcan-mcp-client` 这条 Rust / MCP 主路径已经开始真正摆脱动态库 JSON 调用模式；
- 现阶段仍然保留了 Lua 侧 JSON 兼容输入输出形态，因此上层 skill 无需同步重写；
- 后续如果继续推进同一路线，下一步更值得做的是：
  - 将 `sqlite` 也按同样方式静态接入宿主；
  - 或继续减少宿主内部对 JSON 兼容层的依赖。

当前判断：

- `vldb-sqlite` 已经具备“多库 runtime + 非 JSON FFI 主接口 + JSON 兼容层”的完整分层骨架
- 后续如果继续打磨库侧接口，重点可以转向：
  - 更细的 getter 覆盖
  - 错误码规范化
  - Go 包装层示例与文档

### 阶段三：`vldb-sqlite` Go FFI 示例补齐

已完成内容：

- 新增 `vldb-sqlite/examples/go-ffi`
  - `go.mod`
  - `README.md`
  - `main.go`
  - `sqliteffi/sqliteffi.go`
  - `sqliteffi/loader_unix.go`
  - `sqliteffi/loader_windows.go`
- 示例明确走：
  - 动态库加载
  - 非 JSON FFI 主接口
  - runtime / database / result handle 风格
- 示例覆盖能力：
  - 打开 runtime
  - 打开数据库
  - 热更新自定义词
  - 建 FTS 索引
  - 写入 FTS 文档
  - 执行 FTS 检索
  - 读取 `id / file_path / score / rank / raw_score`
- 文档同步补入口：
  - `README.md`
  - `docs/LIBRARY_USAGE.zh-CN.md`

验证结果：

- `gofmt` 通过
- `go mod tidy` 通过
- `go build ./...` 通过

当前判断：

- `vldb-sqlite` 现在已经不只是“有接口”，而是已经有一套面向 Go 的最小落地样例
- 后续如果继续推进同一路线，可以考虑为 `vldb-lancedb` 也补一套对应的 Go FFI wrapper 示例

### 阶段四：`vldb-lancedb` Go FFI 示例补齐

已完成内容：

- 新增 `vldb-lancedb/examples/go-ffi`
  - `go.mod`
  - `README.md`
  - `main.go`
  - `lancedbffi/lancedbffi.go`
  - `lancedbffi/loader_unix.go`
  - `lancedbffi/loader_windows.go`
- 示例明确采用当前推荐分层：
  - 低频 DDL：继续走 JSON 兼容层
  - 高频向量写入 / 检索：走非 JSON FFI 主接口
- Go 包装层已覆盖能力：
  - 动态库加载
  - runtime 创建 / 销毁
  - 打开默认库与命名库引擎
  - 解析数据库路径
  - JSON 兼容层建表
  - 非 JSON 向量写入
  - 非 JSON 向量检索
- 文档同步补入口：
  - `README.md`
  - `docs/LIBRARY_USAGE.zh-CN.md`

验证结果：

- `gofmt` 通过
- `go mod tidy` 通过
- `go build ./...` 通过

当前判断：

- `vldb-lancedb` 与 `vldb-sqlite` 两边都已经具备可编译、可复用的 Go FFI 落地示例
- 现阶段库侧分层已经比较完整：
  - Rust / MCP：typed Rust API
  - Go / 原生宿主：非 JSON FFI 主接口
  - JSON：兼容层

### 阶段五：`vulcan-mcp-client` 宿主级 `sqlite_host` 静态包装层

已完成内容：

- 在 `vulcan-mcp-client` 中新增了宿主级 SQLite typed 包装模块：
  - `src/sqlite_host.rs`
- 该模块当前专注于“Rust / MCP 静态主路径”的宿主封装，不改变现有：
  - `--sqlite` 远程 gRPC 配置
  - `ScratchpadStore` 远程 SQLite 使用链
- 新增宿主运行时与单库绑定抽象：
  - `SqliteHostRuntime`
  - `SqliteHostBinding`
- 当前宿主包装层已经覆盖的 typed 能力包括：
  - 多库打开 / 关闭 / 列表
  - 文本分词
  - 自定义词写入 / 删除 / 列表
  - FTS 索引确保 / 重建
  - FTS 文档写入 / 删除
  - FTS 检索
- `main.rs` 已将 `sqlite_host` 纳入编译单元，确保后续可以直接从宿主调用点逐步切换到静态库能力。
- `Cargo.toml` 新增了 `vldb-sqlite` 本地 path 依赖：
  - 当前采用 path 依赖而不是 Git 依赖，是因为宿主需要接入的是本地已经重构完成的 `lib/runtime` 版本；待远端仓库同步后，可再切回 Git 依赖。

验证结果：

- `cargo check` 通过

当前判断：

- `vulcan-mcp-client` 现在已经同时具备：
  - `lancedb` 静态 typed 宿主接入
  - `sqlite` 静态 typed 宿主包装层
- 但 `sqlite` 这条线目前仍然是“宿主包装已具备、调用点尚未切换”的阶段；
- 下一步最适合做的事情，不是继续扩接口，而是选定一个真实调用点（例如 scratchpad 或未来 skill 宿主注入）进行渐进替换。

### 阶段六：双库远端同步与宿主依赖切回 Git

已完成内容：

- 将 `vldb-lancedb` 当前最新库侧改动提交并推送到远端主分支：
  - 提交哈希：`242da94e53a14b14950f3ebc1afdedea22adf6bf`
- 将 `vldb-sqlite` 当前最新库侧改动提交并推送到远端主分支：
  - 提交哈希：`ba40b422001573f6cb9c30ef8c64bd8130453b3a`
- 将 `vulcan-mcp-client` 的依赖源统一切回 Git 固定版本：
  - `vldb-lancedb` 改为远端 `rev = "242da94e53a14b14950f3ebc1afdedea22adf6bf"`
  - `vldb-sqlite` 从本地 `path` 依赖改为远端 `rev = "ba40b422001573f6cb9c30ef8c64bd8130453b3a"`
- 重新执行宿主编译验证，确认当前状态已经不再依赖本地工作副本路径。

验证结果：

- `cargo check` 通过
- `Cargo.lock` 已同步锁定两个远端 Git 依赖版本

当前判断：

- `vulcan-mcp-client` 当前已经回到“纯 Git 依赖 + Rust 静态接入”的稳定形态；
- 后续测试可以基于远端仓库直接复现，不再依赖本地 `VulcanLocalDataGateway` 工作目录；
- 这也为后续在 WSL、Linux 或其他机器上做一致性验证提供了更干净的基础。
