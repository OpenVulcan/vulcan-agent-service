# vldb-sqlite 下一阶段（RRF 融合与接口收敛）实施计划

## 任务目标

在已经完成 `vldb-sqlite` 基础库模式、Jieba 分词、词典热更新、SQLite 内建 tokenizer 注册以及 FTS 富结果返回的基础上，继续推进下一阶段设计与实现，重点收敛以下三件事：

1. 为 `vldb-sqlite` 定义可稳定复用的 **混合检索预留接口**；
2. 明确 `vldb-sqlite` 与 `vldb-lancedb` 在未来 **RRF（倒数排名融合）** 中的字段契约与职责边界；
3. 在不破坏当前 `grpc + ffi + lib` 能力面的前提下，为后续 `vulcan-mcp-client` 宿主接入打好统一接口基础。

## 当前已知基础

当前 `vldb-sqlite` 已经具备：

- `none / jieba` 分词模式；
- `_vulcan_dict` 伴生词典表；
- `UpsertCustomWord / RemoveCustomWord` 的 gRPC 与 FFI 接口；
- SQLite 连接级 `tokenize='jieba'` 注册；
- `EnsureFtsIndex / UpsertFtsDocument / DeleteFtsDocument / SearchFts`；
- `SearchFts` 富结果字段：
  - `id`
  - `file_path`
  - `title`
  - `title_highlight`
  - `content_snippet`
  - `score`
  - `rank`
  - `raw_score`
  - `source`
  - `query_mode`

这意味着当前阶段已经具备进入“融合接口设计与最小实现”的前置条件。

## 计划执行步骤

### 第一步：收敛混合检索契约

对 `vldb-sqlite` 与 `vldb-lancedb` 的返回结构进行并排梳理，明确最小公共契约字段，至少确认：

- `id`
- `file_path`
- `score`
- `rank`
- `raw_score`
- `source`
- `query_mode`

同时判断是否需要补充：

- `title`
- `title_highlight`
- `content_snippet`
- `metadata/version`

输出目标：

- 一份明确的“融合层公共字段约定”
- 一份“SQLite 与 LanceDB 各自负责哪些字段”的边界说明

### 第二步：设计 RRF 融合层接口

先不急着把全部融合逻辑强塞到 gRPC 层，而是优先设计 **core/lib 可复用的融合模型**，重点包括：

- RRF 输入结构
- RRF 结果结构
- 来源打标策略
- 排序与去重策略
- 同 ID / 同 file_path 的合并规则

输出目标：

- 明确 `rrf_fuse(...)` 应该属于：
  - `vldb-sqlite core`
  - 宿主层
  - 还是共享小模块

### 第三步：补齐 `vldb-sqlite` 侧必要接口

根据第一步、第二步的结论，只补最必要的接口，不做超前过度设计。优先候选项：

- `SearchFts` 返回结构进一步标准化
- 可选增加结果调试信息
- 如有必要，增加 snippet/highlight 参数化入口

这里的原则是：

- 不破坏当前 JSON FFI 边界
- 不破坏当前 gRPC 兼容性
- 不让上层再重复拼接 SQLite 检索结果语义

### 第四步：明确宿主接入路径

从 `vulcan-mcp-client` 的角度，明确未来接入方式：

- 是直接分别调用：
  - `vldb-sqlite`
  - `vldb-lancedb`
  - 再在宿主做 RRF
- 还是未来增加某种统一融合调用入口

这里需要重点分析：

- 宿主生命周期管理
- 日志归属
- 技能隔离
- 多实例缓存
- FFI/动态库加载边界

输出目标：

- 一份宿主接入建议
- 一份“不该提前下沉到库里”的边界清单

### 第五步：最小实现与验证

在确认前四步方向后，只做一版最小可验证实现，至少保证：

- `cargo check`
- `cargo test --lib`
- 文档同步
- 阶段性总结回写计划文件

## 技术选型与决策原则

### 1. 融合逻辑优先保持纯逻辑层

RRF 本质是排序融合逻辑，优先作为纯 Rust 逻辑模型实现，而不是一开始就绑死在 gRPC 或 SQLite 连接层。

### 2. FFI 继续坚持扁平 JSON 边界

所有新增对外接口仍坚持：

- 入参：`const char*` JSON
- 出参：`char*` JSON

不得向 Lua / Python / Go 暴露复杂 Rust 结构体指针。

### 3. 不提前把 SQLite 与 LanceDB 强耦合

本阶段的目标是“融合契约清晰”，不是把两个库强行合并成单库。

优先让：

- `vldb-sqlite` 负责关键词/BM25
- `vldb-lancedb` 负责向量/语义
- 融合逻辑位于更适合协调的位置

### 4. 保持中文检索闭环原则

`vldb-sqlite` 继续保持：

- 词典内建
- 禁止外部物理词典
- 词典热更新
- tokenizer 模式清晰

不回退到让上层业务各自实现中文分词。

## 验收标准

本阶段完成时，应满足：

1. 明确 `vldb-sqlite` 与 `vldb-lancedb` 的融合字段契约；
2. 明确 RRF 融合逻辑最合适的承载层；
3. 对 `vldb-sqlite` 的接口调整仅限必要增强，不引入新的混乱；
4. 文档、代码与计划记录保持同步；
5. 所有修改通过基础编译与库测试验证。

## 需您确认的关键点

在正式进入实现前，需要您确认以下方向是否按此推进：

1. **RRF 融合层优先作为“共享纯逻辑层/宿主协调层”设计，而不是直接塞进 `vldb-sqlite` gRPC。**
2. **`vldb-sqlite` 本阶段只做“为融合准备结果面”，不直接承担完整混合检索编排。**
3. **`vldb-lancedb` 与 `vldb-sqlite` 维持职责分离，先做统一契约，再决定是否需要统一入口。**
