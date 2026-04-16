# vldb-sqlite 重构设计与构建链改造计划

## 任务目标

在 `D:\projects\VulcanLocalDataGateway\vldb-sqlite` 中推进 `vldb-sqlite` 的重构设计与实现准备工作，使其能够从当前以 gRPC 为主的单一服务形态，逐步演进为具备 `core/lib + grpc + ffi` 三层结构的本地 SQLite 检索引擎，并为后续支持中文分词、Jieba tokenizer、内建词典热更新、FFI JSON 边界以及 Docker / GitHub 编译发布做铺垫。

## 执行步骤

1. 盘点 `vldb-sqlite` 当前目录结构、Cargo 配置、gRPC 接口、Docker 与 GitHub Actions 工作流。
2. 分析当前 SQLite、FTS、中文检索、分词、连接池与服务层实现边界，确认哪些逻辑需要下沉到 core/lib。
3. 设计新的能力边界：
   - core/lib：SQLite 引擎、可选 tokenizer、词典闭环、检索与查询构造
   - grpc：协议适配与远程调用入口
   - ffi：JSON 入参 / JSON 出参的平坦接口
4. 明确中文分词与词典闭环方案：
   - `tokenize='jieba'` 能力定位
   - `_vulcan_dict` 伴生表
   - `UpsertCustomWord/RemoveCustomWord` 的 gRPC 与 FFI 导出要求
5. 评估并改造 Dockerfile 与 GitHub 编译/发布工作流，确保未来服务包与库模式包均可构建。
6. 在仓库中落地第一阶段必要改动，并执行本地验证。
7. 任务完成后补充执行变更总结，并将计划文件迁移至 `docs/completed/20260416/`。

## 技术选型关注点

- SQLite 内核：`rusqlite`
- 并发/池化：优先评估 `deadpool-sqlite`
- 中文分词：以 `jieba-rs` 为重点方向，但设计要允许 tokenizer 模式切换
- FFI 边界：仅暴露 C 风格字符串 JSON 接口，不向上层暴露复杂 Rust 结构体指针
- 混合检索预留：FTS 结果需统一返回 `id/file_path/score/rank/raw_score`

## 验收标准

- 明确 `vldb-sqlite` 当前架构问题与改造方向。
- 形成可执行的 `core/lib + grpc + ffi` 设计收敛方案。
- 明确中文分词、词典热更新、RRF 预留字段与 FFI JSON 协议。
- Docker 与 GitHub 构建/发布链的调整点被识别并至少完成第一阶段修正。
- 完成必要代码改动并通过可行的本地验证。

## 执行变更总结（阶段一）

### 1. 核心修复与调整概述
- 为 `vldb-sqlite` 增加了 `rlib + cdylib` 的库模式骨架，正式具备独立库包发布前提。
- 新增基础 FFI 引导层与头文件，先打通“库可加载、版本信息可读、错误通道可用”的最小闭环。
- 调整 Dockerfile 与 GitHub Native Release 工作流，使其支持“服务包 + 独立库包”双产物发布。
- 新增库模式说明文档，并把顶层 README 与 docs 索引同步到最新结构。

### 2. 📂文件变更清单
#### 新增
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\src\lib.rs`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\src\library.rs`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\src\ffi.rs`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\include\vldb_sqlite.h`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\docs\LIBRARY_USAGE.zh-CN.md`

#### 修改
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\Cargo.toml`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\Dockerfile`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\README.md`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\docs\README.zh-CN.md`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\docs\README.en.md`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\.github\workflows\build-native-release.yml`

### 3. 💻关键代码调整详情
- `Cargo.toml`
  - 增加 `[lib]` 段并声明 `crate-type = ["rlib", "cdylib"]`，让仓库同时具备 Rust 内部复用与动态库导出的能力。
- `src/library.rs`
  - 新增 `VldbSqliteLibraryInfo` 与 `library_info()`，为宿主和未来包装层提供统一的库模式元信息。
- `src/ffi.rs`
  - 新增基础 FFI 导出：
    - `vldb_sqlite_library_info_json`
    - `vldb_sqlite_string_free`
    - `vldb_sqlite_last_error_message`
    - `vldb_sqlite_clear_last_error`
    - `vldb_sqlite_json_is_null`
  - FFI 统一采用 JSON 字符串边界，符合后续“扁平 JSON 接口”的设计方向。
- `include/vldb_sqlite.h`
  - 新增 C 头文件，明确当前可被非 Rust 调用方消费的基础 ABI。
- `Dockerfile`
  - 将服务镜像构建收窄为 `cargo build --locked --release --bin vldb-sqlite`，避免服务镜像构建额外拖入库模式产物编译链。
- `.github/workflows/build-native-release.yml`
  - 构建阶段改为一次性生成 `--bin vldb-sqlite --lib`。
  - 打包阶段拆分为：
    - 常规服务包：`vldb-sqlite-v<version>-<target>`
    - 独立库包：`vldb-sqlite-lib-v<version>-<target>`
  - 库包会携带动态库、头文件与库模式文档。

### 4. ⚠️遗留问题与注意事项
- 当前库模式仍处于 **bootstrap** 阶段，仅提供最小 FFI 引导能力，还没有进入 SQLite 引擎实例、Jieba tokenizer、词典热更新与 FTS/BM25 检索接口的实装阶段。
- 当前尚未引入 `jieba-rs`、自定义 tokenizer 注册、`_vulcan_dict` 伴生表，也未扩展 gRPC 新接口，这些属于下一阶段改造重点。
- 当前计划文件保持在 `docs/plan/`，因为整体重构任务仍在继续，尚未进入最终完成归档状态。

## 执行变更总结（阶段二）

### 1. 核心修复与调整概述
- 为 `vldb-sqlite` 增加了内建分词核心模块，先把“常规分词 / Jieba 分词 / 伴生词典热更新”沉到库内，而不是继续让上层应用各自实现。
- 在 gRPC 协议中新增 `TokenizeText`、`UpsertCustomWord`、`RemoveCustomWord` 三个接口，使任意语言都可以通过服务端统一复用分词与词典管理能力。
- 在 FFI 层新增对应 JSON 入参 / JSON 出参接口，让本地宿主、LuaJIT、Python 等调用方也能直接使用同一套分词与词典逻辑。
- 将 `_vulcan_dict` 伴生表机制正式落地到 Rust 实现中，为后续词典闭环、Jieba tokenizer 注册和 FTS/BM25 重构打下可复用基础。

### 2. 📂文件变更清单
#### 新增
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\src\tokenizer.rs`

#### 修改
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\Cargo.toml`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\proto\v1\sqlite.proto`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\src\lib.rs`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\src\main.rs`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\src\service.rs`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\src\ffi.rs`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\src\library.rs`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\include\vldb_sqlite.h`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\docs\LIBRARY_USAGE.zh-CN.md`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\README.md`

### 3. 💻关键代码调整详情
- `src/tokenizer.rs`
  - 新增 `TokenizerMode`，明确 `none / jieba` 两种模式。
  - 落地 `_vulcan_dict` 伴生表。
  - 实现：
    - `ensure_vulcan_dict_table`
    - `upsert_custom_word`
    - `remove_custom_word`
    - `load_custom_words`
    - `tokenize_text`
  - `jieba` 模式下会把 `_vulcan_dict` 中的热更新专有词注入到 `Jieba` 实例中。
  - 增加单元测试，验证词典热更新确实会影响 Jieba 切词结果。
- `proto/v1/sqlite.proto`
  - 新增：
    - `TokenizerMode`
    - `TokenizeTextRequest/Response`
    - `UpsertCustomWordRequest`
    - `RemoveCustomWordRequest`
    - `DictionaryMutationResponse`
  - `SqliteService` 新增三条 RPC：
    - `TokenizeText`
    - `UpsertCustomWord`
    - `RemoveCustomWord`
- `src/service.rs`
  - 为现有 gRPC 服务补齐三条新方法。
  - 新增对应 worker 逻辑：
    - `run_tokenize_text`
    - `run_upsert_custom_word`
    - `run_remove_custom_word`
  - 分词与词典管理正式通过 gRPC 出口暴露。
- `src/ffi.rs`
  - 新增 JSON FFI 接口：
    - `vldb_sqlite_tokenize_text_json`
    - `vldb_sqlite_upsert_custom_word_json`
    - `vldb_sqlite_remove_custom_word_json`
  - 统一使用扁平 JSON 边界，保持对上层语言友好。
- `src/library.rs`
  - `ffi_stage` 从 `bootstrap` 升级为 `tokenizer-foundation`。
  - `capabilities` 同步增加分词与词典热更新能力标识。

### 4. ⚠️遗留问题与注意事项
- 当前第二阶段已经把“分词能力”和“伴生词典闭环”正式纳入 `vldb-sqlite`，但**SQLite 内部 `tokenize='jieba'` 的 tokenizer 注册尚未实装**。
- 当前 `TokenizeText` / JSON FFI 主要解决的是：
  - 统一分词逻辑
  - 统一词典热更新逻辑
  - 避免 MCP / VMM / 其他语言各自实现 Jieba
- 后续仍需要继续推进：
  - SQLite FTS tokenizer 注册
  - 统一 `SearchFts` / BM25 接口
  - 标准化 `id/file_path/score/rank/raw_score` 返回结构
  - 与 LanceDB 的 RRF 融合预留位

## 执行变更总结（阶段三）

### 1. 核心修复与调整概述
- 在 `vldb-sqlite` 内部正式完成了 SQLite FTS5 `jieba` tokenizer 的连接级注册，不再只是“外部分词基础层”。
- 将 `_vulcan_dict` 伴生词典表与 SQLite tokenizer 注册打通，做到同库词典热更新后可直接影响 SQLite tokenizer 行为。
- 通过共享词典状态与连接级注册管理，确保同一数据库的不同连接能够复用同一份热更新词典快照。
- 新增基于 `fts5vocab` 的单元测试，真实验证 `tokenize='jieba'` 已经在 SQLite FTS 索引层生效。

### 2. 📂文件变更清单
#### 修改
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\Cargo.toml`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\src\tokenizer.rs`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\src\service.rs`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\src\ffi.rs`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\src\library.rs`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\README.md`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\docs\LIBRARY_USAGE.zh-CN.md`

### 3. 💻关键代码调整详情
- `Cargo.toml`
  - 为 `rusqlite` 增加 `pointer` feature，使 Rust 侧可以从 SQLite 连接中读取 `fts5_api` 指针并完成 tokenizer 注册。
- `src/tokenizer.rs`
  - 新增 SQLite FTS5 `jieba` tokenizer 注册全链路：
    - `ensure_jieba_tokenizer_registered`
    - `fetch_fts5_api`
    - `sqlite_jieba_tokenizer_create`
    - `sqlite_jieba_tokenizer_delete`
    - `sqlite_jieba_tokenizer_registration_destroy`
    - `sqlite_jieba_tokenizer_tokenize`
  - 增加共享词典状态与连接注册表：
    - 用数据库注册键统一维护 `_vulcan_dict` 快照
    - 用连接句柄集合防止重复注册
  - `jieba` tokenization 改为使用 `jieba.tokenize(...)`，并显式把 Unicode 字符位置转换成 UTF-8 字节偏移，满足 SQLite FTS5 callback 要求。
  - 新增单元测试 `sqlite_fts_jieba_tokenizer_is_registered`，通过 `fts5vocab` 验证 `田-女士` 已真实进入 FTS 词项索引。
- `src/service.rs`
  - 在连接初始化的 `apply_connection_pragmas` 末尾接入 `ensure_jieba_tokenizer_registered`，保证服务模式下每个池化连接都会自动注册 `jieba` tokenizer。
- `src/library.rs`
  - `ffi_stage` 升级为 `sqlite-jieba-registered`，表示库模式已完成 SQLite 内建 tokenizer 注册。
- `README.md` / `docs/LIBRARY_USAGE.zh-CN.md`
  - 同步修正文档口径，不再写“SQLite 内部 tokenizer 注册仍在后续阶段”，而是明确说明当前已支持连接级 `tokenize='jieba'`。

### 4. ⚠️遗留问题与注意事项
- 当前已经完成 SQLite 内建 `jieba` tokenizer 注册，但仍未进入正式的 `SearchFts` / BM25 业务接口设计与导出阶段。
- 现阶段 `_vulcan_dict` 热更新会刷新共享词典快照，但后续仍建议补一层显式的索引重建/刷新策略，以应对更复杂的词典变更场景。
- 当前 tokenizer 注册是连接级自动完成的，后续在 `vulcan-mcp-client` 或其他宿主接入时，应继续坚持“上层不自行实现 jieba，而是统一调用 `vldb-sqlite`”的原则。

## 执行变更总结（阶段四）

### 1. 核心修复与调整概述
- 为 `vldb-sqlite` 增加了最小但完整的 FTS 业务能力面，覆盖“建索引、写入文档、删除文档、执行 BM25 检索”四个动作。
- gRPC 与 FFI 现在都已经同步暴露这套 FTS 业务接口，不再只有分词与词典管理能力。
- `SearchFts` 返回结构正式带上 `id / file_path / title / score / rank / raw_score`，为后续与 LanceDB 进行 RRF 混合检索预留统一字段面。
- 修正了 `jieba` 搜索模式下 MATCH 表达式的语义：查询模式现在使用 `OR` 拼接分词结果，避免搜索模式细粒度切词把自己“AND 卡死”。

### 2. 📂文件变更清单
#### 新增
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\src\fts.rs`

#### 修改
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\proto\v1\sqlite.proto`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\src\lib.rs`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\src\main.rs`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\src\service.rs`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\src\ffi.rs`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\src\library.rs`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\src\tokenizer.rs`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\include\vldb_sqlite.h`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\README.md`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\docs\LIBRARY_USAGE.zh-CN.md`

### 3. 💻关键代码调整详情
- `src/fts.rs`
  - 新增 FTS 统一业务模块，提供：
    - `ensure_fts_index`
    - `upsert_fts_document`
    - `delete_fts_document`
    - `search_fts`
  - FTS 表采用统一结构：
    - `id UNINDEXED`
    - `file_path UNINDEXED`
    - `title`
    - `content`
  - `search_fts` 使用 SQLite `bm25()`，并同时返回：
    - `score`（标准化为“越大越好”）
    - `raw_score`（SQLite 原始分值）
    - `rank`
    - `id`
    - `file_path`
    - `title`
- `proto/v1/sqlite.proto`
  - 新增 RPC：
    - `EnsureFtsIndex`
    - `UpsertFtsDocument`
    - `DeleteFtsDocument`
    - `SearchFts`
  - 新增消息：
    - `EnsureFtsIndexRequest/Response`
    - `UpsertFtsDocumentRequest`
    - `DeleteFtsDocumentRequest`
    - `FtsMutationResponse`
    - `SearchFtsRequest`
    - `SearchFtsHit`
    - `SearchFtsResponse`
- `src/service.rs`
  - 为上述四条新 RPC 接入完整 worker 逻辑，统一复用已有连接池、日志与错误映射机制。
- `src/ffi.rs`
  - 新增 JSON FFI 导出：
    - `vldb_sqlite_ensure_fts_index_json`
    - `vldb_sqlite_upsert_fts_document_json`
    - `vldb_sqlite_delete_fts_document_json`
    - `vldb_sqlite_search_fts_json`
- `src/tokenizer.rs`
  - 将查询模式的 FTS 表达式从空格拼接改为 `OR` 拼接，避免 `jieba` 搜索模式返回的细粒度词元在 MATCH 中被错误当作强制同时命中条件。
- `src/library.rs`
  - `ffi_stage` 升级为 `sqlite-fts-foundation`，准确反映当前库模式已具备最小 FTS 业务面。

### 4. ⚠️遗留问题与注意事项
- 当前已经具备最小 FTS 检索闭环，但仍未进入更通用的表级 schema 管理与索引迁移机制。
- `SearchFts` 目前返回 `title` 但还没有 `snippet/highlight` 一类摘要信息；如果后续上层强依赖检索摘要，还需要继续补充。
- 当前 `score = -raw_score` 的标准化方式可以满足“越大越好”的统一接口需求，但如果后续要跨多种检索源做更精细的归一化，仍建议在融合层再做统一校准。

## 执行变更总结（阶段五）

### 1. 核心修复与调整概述
- 在阶段四的基础上，正式把 FTS 业务面同步导出到 gRPC 与 FFI，两条调用链不再只有设计稿里的接口名字，而是已经能真实编译和测试通过。
- `SearchFts` 的结果结构已经稳定携带 `id / file_path / title / score / rank / raw_score`，这套返回面可以直接供后续 RRF 融合层消费。
- 修正了 `jieba` 搜索模式下的 MATCH 语法策略：查询模式下由空格隐式 AND 改为显式 OR，避免细粒度分词把中文召回错误收窄。
- 同步更新库模式阶段标记，从 `sqlite-jieba-registered` 升级为 `sqlite-fts-foundation`，确保文档、FFI 元信息与真实能力面一致。

### 2. 📂文件变更清单
#### 修改
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\proto\v1\sqlite.proto`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\src\service.rs`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\src\ffi.rs`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\src\library.rs`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\src\tokenizer.rs`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\include\vldb_sqlite.h`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\README.md`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\docs\LIBRARY_USAGE.zh-CN.md`

### 3. 💻关键代码调整详情
- `proto/v1/sqlite.proto`
  - 新增并固化 FTS 业务接口：
    - `EnsureFtsIndex`
    - `UpsertFtsDocument`
    - `DeleteFtsDocument`
    - `SearchFts`
  - 新增结果结构：
    - `SearchFtsHit`
    - `SearchFtsResponse`
  - 结果里包含：
    - `id`
    - `file_path`
    - `title`
    - `score`
    - `rank`
    - `raw_score`
- `src/service.rs`
  - 将上述四条 FTS 能力完整接入 gRPC service 实现层，并统一复用现有的连接池、日志、错误映射与 worker 模式。
- `src/ffi.rs`
  - 新增 JSON FFI 导出：
    - `vldb_sqlite_ensure_fts_index_json`
    - `vldb_sqlite_upsert_fts_document_json`
    - `vldb_sqlite_delete_fts_document_json`
    - `vldb_sqlite_search_fts_json`
  - 保持“入参 JSON / 出参 JSON”的扁平边界，不向上层暴露复杂 Rust 结构体。
- `src\tokenizer.rs`
  - 将搜索模式下的 FTS 查询表达式改为 `OR` 连接，避免 `jieba` 搜索模式的多词元在 MATCH 中被错误解释为强制同时命中。
- `src/library.rs`
  - 将 `ffi_stage` 更新为 `sqlite-fts-foundation`，并扩展 capability 列表以反映当前 FTS 能力面。
- `README.md` / `docs\LIBRARY_USAGE.zh-CN.md`
  - 同步对外说明，确认当前库模式已不仅是 tokenizer 与词典能力，而是具备最小 FTS 业务闭环。

### 4. ⚠️遗留问题与注意事项
- 当前 `SearchFts` 已具备 RRF 友好的核心字段，但仍未提供 `snippet/highlight` 之类面向展示层的增强信息。
- 当前 FTS 索引 schema 仍是统一固定结构（`id/file_path/title/content`），后续若要做更通用的业务场景支持，仍需设计更灵活的 schema 与迁移策略。
- 当前混合召回层本身还未实现；现阶段只是把 `vldb-sqlite` 的输出面收成了更容易融合的形态。

## 执行变更总结（阶段六）

### 1. 核心修复与调整概述
- 在阶段五的基础上，把 `SearchFts` 的结果面进一步升级成“可展示、可融合”的富结果结构。
- 检索结果现在直接包含标题高亮和正文摘要片段，调用方不再需要自行拼接 FTS `highlight/snippet` 展示内容。
- 同时补上 `source` 与 `query_mode` 两个结果元信息，让后续 RRF 混合检索层可以直接识别来源与检索语义，而不必依赖调用约定。
- 文档与 FFI 阶段标记同步升级，保证 README、库模式说明和 FFI 元信息与当前真实能力一致。

### 2. 📂文件变更清单
#### 修改
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\src\fts.rs`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\proto\v1\sqlite.proto`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\src\service.rs`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\src\ffi.rs`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\src\library.rs`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\README.md`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\docs\LIBRARY_USAGE.zh-CN.md`

### 3. 💻关键代码调整详情
- `src/fts.rs`
  - `SearchFtsHit` 新增：
    - `title_highlight`
    - `content_snippet`
  - `SearchFtsResult` 新增：
    - `source`
    - `query_mode`
  - 检索 SQL 现在直接使用 SQLite FTS5 内建：
    - `highlight(index, 2, '<mark>', '</mark>')`
    - `snippet(index, 3, '<mark>', '</mark>', '...', 12)`
  - 这样上层拿到结果后即可直接用于展示层或融合层调试。
- `proto/v1/sqlite.proto`
  - `SearchFtsHit` 新增：
    - `title_highlight`
    - `content_snippet`
  - `SearchFtsResponse` 新增：
    - `source`
    - `query_mode`
  - 继续保留：
    - `score`
    - `rank`
    - `raw_score`
    以兼容后续 RRF 融合。
- `src/service.rs`
  - 将新增的高亮、摘要与结果元信息完整映射到 gRPC 返回结构。
- `src/ffi.rs`
  - `SearchFtsResult` 的 JSON 序列化结果会自动带出新增字段。
  - `library_info_json` 中的 `ffi_stage` 从 `sqlite-fts-foundation` 升级为 `sqlite-fts-rich-results`。
- `README.md` / `docs\LIBRARY_USAGE.zh-CN.md`
  - 同步更新 `SearchFts` 返回字段说明与 JSON 示例，明确当前结果面已包含展示层与融合层都可直接复用的信息。

### 4. ⚠️遗留问题与注意事项
- 当前 `source` 固定为 `sqlite_fts`，`query_mode` 固定为 `fts`；后续如果继续扩展过滤检索、结构检索或混合查询模式，需要把这两个字段收敛成更稳定的枚举语义。
- 当前 `snippet/highlight` 直接依赖 SQLite FTS5 内建函数；如果后续对摘要窗口、字段优先级或高亮标签样式有更细要求，可能还需要补一层策略配置。
- 当前结果面已经足以支持与 `vldb-lancedb` 做 RRF 融合，但真正的混合召回编排层仍未实现。

## 执行变更总结（阶段七）

### 1. 核心修复与调整概述
- 在阶段六“富结果检索”基础上，继续把 SQLite 侧的维护闭环补齐，新增了“列出词典”和“基于当前词典重建 FTS 索引”两条正式能力。
- 这意味着 `_vulcan_dict` 不再只是能热更新，却缺少可观测和可修复手段；现在词典可查询、索引可重建，真正形成了数据库内的专有词闭环。
- 新能力同时导出到 gRPC 与 FFI(JSON) 两条边界，保证宿主、本地库调用和未来远程调用的行为一致。
- 文档、头文件和能力探测元信息同步更新，避免“内部已经支持，但外部不知道怎么用”的割裂状态。

### 2. 📂文件变更清单
#### 修改
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\src\service.rs`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\src\ffi.rs`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\src\library.rs`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\include\vldb_sqlite.h`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\README.md`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\docs\LIBRARY_USAGE.zh-CN.md`

### 3. 💻关键代码调整详情
- `src/service.rs`
  - 新增 gRPC 接口实现：
    - `ListCustomWords`
    - `RebuildFtsIndex`
  - 两条接口都继续复用现有连接池、日志和错误映射模型，不额外引入新执行路径。
- `src/ffi.rs`
  - 新增 JSON FFI 导出：
    - `vldb_sqlite_list_custom_words_json`
    - `vldb_sqlite_rebuild_fts_index_json`
  - 继续保持“入参 JSON / 出参 JSON”的扁平边界，不向上层暴露复杂 Rust 结构体指针。
- `src/library.rs`
  - `capabilities` 增补：
    - `list_custom_words_json`
    - `rebuild_fts_index_json`
  - 让宿主可通过库元信息探测当前是否具备词典查询与索引重建能力。
- `include\vldb_sqlite.h`
  - 同步新增上述两个 FFI 函数声明，确保库包头文件与实际动态库导出一致。
- `README.md` / `docs\LIBRARY_USAGE.zh-CN.md`
  - 明确新增：
    - `ListCustomWords`
    - `RebuildFtsIndex`
  - 并补充说明：词典热更新后，如果旧文档需要按新词典重新分词，必须执行重建索引。

### 4. ⚠️遗留问题与注意事项
- 当前词典热更新后，查询分词立即生效，但旧文档索引不会自动重建；仍需调用 `RebuildFtsIndex` 主动回填，这是当前有意保留的显式控制策略。
- `RebuildFtsIndex` 现在基于固定 FTS schema（`id/file_path/title/content`）重建，如果未来引入更通用的字段映射或多索引策略，需要再抽象一层 schema 管理。
- 当前 SQLite 侧已经具备“词典可维护 + 索引可修复 + 检索可消费”的完整单路闭环，下一步更适合继续打磨检索质量与配置能力，而不是引入跨库混合接口。

## 执行变更总结（阶段八）

### 1. 核心修复与调整概述
- 继续按“只打磨 sqlite，不改 lancedb 协议”的方向推进，这一阶段重点解决了 `vldb-sqlite` 的库模式边界问题。
- 新增了纯 Rust 的多库 runtime，使 `lib` 在**不依赖配置文件**的前提下，能够以程序化方式动态管理多个 SQLite 数据库实例。
- 同时保留 gRPC 原有的配置文件单库启动模式，不让新增加的多库能力反向污染现有服务配置与运行逻辑。
- 将 gRPC 侧使用的 SQLite 连接初始化逻辑下沉到共享 runtime 内核，保证 gRPC 与 lib 共用同一套 pragma、WAL 校验与 `jieba` tokenizer 初始化规则。

### 2. 📂文件变更清单
#### 新增
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\src\runtime.rs`

#### 修改
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\src\lib.rs`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\src\main.rs`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\src\service.rs`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\README.md`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\docs\LIBRARY_USAGE.zh-CN.md`

### 3. 💻关键代码调整详情
- `src/runtime.rs`
  - 新增纯库运行时模型：
    - `SqliteRuntime`
    - `SqliteDatabaseHandle`
    - `SqliteOpenOptions`
    - `SqlitePragmaOptions`
    - `SqliteHardeningOptions`
  - 支持：
    - 程序化打开数据库
    - 多库缓存与复用
    - 数据库目录自动创建
    - 可选数据库文件锁
    - 共享 SQLite pragma / `jieba` tokenizer 初始化
  - 新增单元测试，验证：
    - 无需配置文件即可打开多个数据库
    - 多库词典互不串扰
- `src/service.rs`
  - gRPC 侧的 `open_connection` 与 `apply_connection_pragmas` 现在改为复用 runtime 内核，不再单独维护另一份 SQLite 初始化逻辑。
  - 保留现有基于 `Config` 的单库启动方式，只在内部通过 `runtime_options_from_config` 做桥接，不改变外部配置协议。
- `src/main.rs`
  - 接入 `runtime` 模块，确保 bin 与 lib 共用同一套底层连接初始化实现。
  - 同时针对库 API 在 bin 侧天然未完全消费的情况，局部抑制无意义的 dead_code 告警，避免干扰真正的编译问题判断。
- `README.md` / `docs\LIBRARY_USAGE.zh-CN.md`
  - 明确写出：
    - gRPC = 配置文件驱动单库
    - lib = 程序化多库 runtime
  - 让两条能力线的职责边界更加清晰。

### 4. ⚠️遗留问题与注意事项
- 当前新增的 `SqliteRuntime` 先解决了“多库管理”和“与配置文件解耦”的核心问题，但还没有把现有 JSON FFI 完全改造成显式 runtime/handle 模式；当前 FFI 仍保持按 `db_path` 直接调用以兼容既有接口。
- `SqliteRuntime` 当前的多库能力已经足够给宿主或上层 Rust 代码使用，但如果未来要进一步做更细粒度的实例日志隔离或连接池策略，还需要继续扩展 runtime 设计。
- gRPC 仍然明确维持单库配置模式，这个边界当前是刻意保持的，不建议为了“接口统一”而回头把多库能力塞回配置文件。 
