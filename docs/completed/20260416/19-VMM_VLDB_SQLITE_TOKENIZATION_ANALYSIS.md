# 任务目标

分析 `D:\projects\VulcanMemoryMesh` 中 `vldb-sqlite` 相关的中文分词、全文检索与 BM25 处理方式，梳理其当前 Go 实现的完整链路，并结合当前 `vulcan-mcp-client` / `vldb-sqlite` 未来 Rust 重构方向，给出需要同步迁移或重构的关键点。

# 执行步骤

1. 定位 `VulcanMemoryMesh` 中与 `vldb-sqlite`、SQLite、FTS、BM25、中文分词、Jieba 相关的代码与文档入口。
2. 梳理当前 Go 实现的整体处理流程，包括：
   - 文本写入前是否进行分词
   - 分词结果如何落库
   - BM25 / FTS 查询如何构造
   - 查询结果如何回排或融合
3. 分析当前实现中哪些逻辑是“必须整体迁移”的，哪些可以在 Rust 侧重新抽象。
4. 结合 `vldb-lancedb` 已有的 `lib + grpc + ffi` 形态，评估 `vldb-sqlite` 若按同一路线改造，需要补齐的核心模块与边界。
5. 输出建议方案，明确：
   - Rust 版 `vldb-sqlite` 应保留哪些能力
   - 中文分词和 BM25 应如何处理
   - 哪些 Go 侧逻辑需要一并迁移，避免只迁一半导致行为不一致

# 技术选型

- 优先通过代码阅读和结构梳理完成分析，不直接做实现改动
- 优先基于真实代码路径和现有调用链得出结论，不依赖猜测
- 输出结果时以工程可迁移性和行为一致性为核心判断标准

# 验收标准

1. 已准确定位 `VulcanMemoryMesh` 中 `vldb-sqlite` 分词与 BM25 的关键代码入口。
2. 已说明当前 Go 实现的中文分词与 BM25 处理流程。
3. 已明确指出 Rust 重构时必须同步迁移的逻辑点。
4. 已形成对 `vldb-sqlite` 重构路线的清晰建议，而不是仅停留在概念层。

---

# 执行变更总结

## 1. 核心修复与调整概述

本次未直接修改业务代码，而是完成了对 `D:\projects\VulcanMemoryMesh` 中 `vldb-sqlite` 中文分词与 BM25 检索链路的完整梳理，并形成了面向后续 Rust 重构的迁移建议。

核心结论如下：

- 当前 `VulcanMemoryMesh` 的 SQLite 中文检索**不是依赖 SQLite 自定义 tokenizer 扩展**，而是采用“应用层预分词 + SQLite FTS5 `unicode61`”的方案。
- 预分词核心位于 `internal/platform/textutil/lexical_tokenizer.go`，当前使用 **GSE（`github.com/go-ego/gse`）** 与内嵌词典完成中文切词，并额外保留 ASCII 标识符回退逻辑。
- `vldb_sqlite.Store` 在启动时初始化 `LexicalTokenizer`，并在写入 FTS 镜像与构造 `MATCH` 查询表达式时统一调用它。
- `SearchLexicalMemory` 当前采用 `bm25(vmm_memory_nodes_fts, 2.0, 1.0)`，并将 SQLite 返回的 `rank` 取负值后回传给上层融合逻辑。
- FTS 表是**手工镜像维护**的，不依赖 trigger；写入、新建 direct memory、turn analysis apply、supersede、recycle/idle clean 等流程都需要同步维护 `vmm_memory_nodes_fts`。
- 上层 `MemoryUseCase` 通过 `SearchLexicalMemory -> materializeLexicalHits -> RRF 融合` 把 lexical recall 与向量召回整合起来。

## 2. 📂文件变更清单

### 新增文件

- `docs/plan/20260416-19-VMM_VLDB_SQLITE_TOKENIZATION_ANALYSIS.md`（本文件）

### 修改文件

- 无

### 删除文件

- 无

## 3. 💻关键代码调整详情

本次为分析任务，没有直接调整生产代码。核心分析结论如下：

### 3.1 分词与 BM25 的真实处理链

关键入口：

- `D:\projects\VulcanMemoryMesh\internal\platform\textutil\lexical_tokenizer.go`
- `D:\projects\VulcanMemoryMesh\internal\adapters\outbound\vldb_sqlite\store.go`
- `D:\projects\VulcanMemoryMesh\internal\app\usecase\memory_query_search.go`
- `D:\projects\VulcanMemoryMesh\internal\app\runtime_storage.go`
- `D:\projects\VulcanMemoryMesh\internal\config\config.go`

当前真实链路为：

1. `runtime_storage.buildRelational(...)` 把 `cfg.MemoryPipeline.LexicalPreTokenize` 传给 `vldb_sqlite.NewStore(...)`
2. `NewStore(...)` 在建立 gRPC 连接前调用 `textutil.NewLexicalTokenizer(...)`
3. `BuildSQLiteFTSIndexText(...)` 把 `abstract/details` 预分词后写入 `vmm_memory_nodes_fts`
4. `BuildSQLiteFTSMatchExpression(...)` 把查询词转换为 FTS5 `MATCH` 表达式
5. `SearchLexicalMemory(...)` 执行 `bm25(vmm_memory_nodes_fts, 2.0, 1.0)` 并回表拿 `memory_id`
6. `MemoryUseCase.hybridizeSearchHits(...)` 对 lexical hit 回表 materialize，再与向量召回做 RRF 融合

### 3.2 当前不是 SQLite 中文 tokenizer 方案，而是“外部分词 + unicode61”

当前 SQLite FTS 表定义位于 `vldb_sqlite/store.go` 中：

- `CREATE VIRTUAL TABLE IF NOT EXISTS vmm_memory_nodes_fts USING fts5(...)`
- tokenizer 固定为：`unicode61 remove_diacritics 2`

也就是说：

- 中文支持并不是通过 SQLite 插件 tokenizer 获得
- 而是通过应用层先把中文切成带空格的 token 序列，再交给 `unicode61`

### 3.3 当前 GSE 分词契约是行为核心，不只是“用了一个库”

`LexicalTokenizer` 当前行为包括：

- 中文文本走 GSE 分词
- 搜索时构造：
  - `\"文本 排序 模型\" OR \"文本\" OR \"排序\" OR \"模型\"`
- 索引写入时构造：
  - 有序主 token
  - 加上 ASCII 标识符回退 token
  - 例如：`vmm-local FTS5 中文分词` -> `vmm local fts5 中文 分词 vmm-local`

这意味着后续如果 Rust 侧改用 `jieba-rs`，**检索行为大概率会变化**，而不只是换了实现语言。

### 3.4 Rust 版若要“行为一致”，必须迁移的不只是 CRUD

后续 `vldb-sqlite` Rust 重构时，至少必须整体迁移以下能力：

- 预分词器初始化时机（启动即初始化）
- 文本归一化规则
- 中文 token 序列构造
- ASCII 标识符补词规则
- 查询 `MATCH` 表达式构造规则
- FTS 镜像表维护点
- `bm25(...)` 权重与排序方向
- lexical hit -> materialized row -> RRF 融合这条上层契约

也就是说，不能只重做“SQLite gRPC + FFI”，而把词法层和融合契约留在旧实现里。

## 4. ⚠️遗留问题与注意事项

### 4.1 如果 Rust 侧使用 `jieba-rs`，要接受行为变更

这是当前最大的迁移风险。

因为当前 Go 版使用的是 **GSE**，而不是 Jieba。即使两者都做中文分词，输出 token 边界也可能不同，进而影响：

- FTS 索引文本
- MATCH 查询表达式
- BM25 排名
- 混合召回结果

如果坚持使用 `jieba-rs`，建议：

- 明确接受“检索行为会变化”
- 重新建立 tokenizer golden cases
- 更新相关测试断言与线上预期

### 4.2 如果目标是优先保持行为一致，建议先抽象“分词契约”

更稳的路线是先定义项目内稳定的 lexical tokenizer contract，例如：

- 索引输入 -> token 输出
- 查询输入 -> MATCH 表达式输出

然后再决定 Rust 侧是：

- 尽量模拟 GSE 输出
- 还是改用 Jieba 并整体重做基线

### 4.3 Rust 重构建议

后续如果真的开始做 `vldb-sqlite` 重构，建议优先采用：

- `lib/core`
- `grpc`
- `ffi`

三层拆分，并保持：

- `core` 承载分词、FTS、BM25、回表、事务和并发
- `grpc` 仅做 transport adapter
- `ffi` 仅暴露稳定宿主调用面

同时建议保留：

- FTS5 + `unicode61`
- 应用层预分词

而不是先去做 SQLite 原生中文 tokenizer 扩展。
