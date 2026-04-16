# vldb-sqlite 库模式多库运行时改造计划

## 任务目标

在 **不改变现有 gRPC 单库配置文件运行逻辑** 的前提下，继续打磨 `vldb-sqlite` 的库模式能力，使其满足以下目标：

1. `lib` **完全不依赖配置文件**；
2. `lib` 能以**程序化方式动态管理多个 SQLite 数据库实例**；
3. gRPC 仍然保持“**一个配置文件绑定一个库**”的原有运行方式，不被新的库模式侵入；
4. 库模式后续能够作为 MCP 宿主和其他语言调用的稳定底座。

## 设计原则

### 1. gRPC 与 lib 职责彻底分离

- `gRPC`：
  - 继续读取 `vldb-sqlite.json`
  - 继续维持单库服务模式
  - 不引入多库配置
- `lib`：
  - 不读取配置文件
  - 仅接收运行时参数或直接接收数据库路径
  - 由调用方决定打开哪些库、何时释放

### 2. 多库能力属于 lib，不属于配置层

- 一个 `vldb-sqlite` 服务实例继续只服务一个配置库；
- 一个 `vldb-sqlite lib runtime` 可以动态管理多个数据库；
- 多库的存在不应反向污染现有 gRPC 配置模型。

### 3. 渐进式改造，不破坏现有接口

- 现有：
  - gRPC 接口
  - 现有 JSON FFI 按 `db_path` 直接调用的方式
  先保持兼容；
- 在此基础上新增：
  - 纯库运行时模型
  - 多库管理器
  - 可被后续宿主封装复用的程序化 API。

## 详细执行步骤

### 步骤 1：梳理当前边界并确认不变项

- 检查 `vldb-sqlite` 当前：
  - `lib.rs`
  - `config.rs`
  - `main.rs`
  - `service.rs`
  - `ffi.rs`
- 明确哪些逻辑必须留在 gRPC 配置启动链；
- 明确哪些逻辑应该下沉到库运行时。

### 步骤 2：新增纯库运行时模块

- 新增独立运行时模块，例如：
  - `runtime.rs`
  - 或 `manager.rs`
- 提供不依赖配置文件的多库入口，例如概念上：
  - `SqliteRuntime`
  - `SqliteDatabaseHandle`
  - `open_database(path)`
  - `get_database(path)`
  - `close_database(path)`
- 运行时内部负责：
  - 路径归一化
  - 数据库目录创建
  - 连接初始化
  - 每库独立状态缓存

### 步骤 3：提炼纯库连接初始化能力

- 将当前 `service.rs` 内与配置文件无关、但与连接初始化有关的能力抽出来；
- 形成“库模式可直接调用”的连接初始化逻辑；
- 保证：
  - `jieba` tokenizer 注册
  - `_vulcan_dict`
  - FTS 能力
  - 词典与索引闭环
  都能在 lib runtime 下正常工作。

### 步骤 4：调整对外导出结构

- `lib.rs` 导出新的 runtime/manager 模块；
- `ffi.rs` 先保持现有按 `db_path` 的 JSON 调用兼容；
- 如有必要，补充库模式元信息，反映“多库 runtime 已可用”。

### 步骤 5：验证不干扰 gRPC

- 验证：
  - `main.rs` 仍然基于配置文件启动单库服务
  - 现有 gRPC 行为不受影响
- 同时验证：
  - lib runtime 能动态打开多个数据库
  - 多库互不干扰

### 步骤 6：文档与说明同步

- 更新：
  - `README.md`
  - `docs/LIBRARY_USAGE.zh-CN.md`
- 明确说明：
  - gRPC 是配置驱动单库
  - lib 是程序化多库
  - 两条链路是并行而非互相覆盖

## 技术选型

- 继续使用 `rusqlite`
- 继续复用现有：
  - `jieba-rs`
  - `_vulcan_dict`
  - FTS tokenizer 注册链路
- 优先新增纯 Rust runtime/manager
- 暂不引入新的配置文件格式和新的外部依赖

## 验收标准

完成后必须满足：

1. `lib` 层不依赖 `config.rs`
2. `gRPC` 继续基于配置文件控制单库
3. `lib` 可以动态管理多个数据库实例
4. 多库实例间的词典、FTS、数据互不串扰
5. 现有 `cargo check`、`cargo test --lib` 通过
6. 文档能清楚说明两条模式的边界

## 执行变更总结

### 1. 核心修复与调整概述
- 新增 `vldb-sqlite` 纯库多库运行时 `SqliteRuntime`，使库模式在不依赖任何配置文件的前提下，可以程序化管理多个数据库实例。
- gRPC 侧维持原有 `vldb-sqlite.json -> 单库服务` 模型，没有引入多库配置，也没有改变原本的运行方式。
- 将 SQLite 连接初始化能力（pragma、WAL 校验、`jieba` tokenizer 注册）抽为共享 runtime 内核，保证 lib 和 gRPC 使用同一套底层初始化逻辑。
- 文档同步明确：
  - gRPC 是配置驱动单库模式
  - lib 是程序化多库 runtime 模式

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
  - 新增：
    - `SqliteRuntime`
    - `SqliteDatabaseHandle`
    - `SqliteOpenOptions`
    - `SqlitePragmaOptions`
    - `SqliteHardeningOptions`
  - 具备：
    - 多库动态打开
    - 库句柄缓存与关闭
    - 目录自动创建
    - 可选文件锁
    - 与现有 `jieba` / FTS / `_vulcan_dict` 链路兼容的连接初始化能力
- `src/service.rs`
  - gRPC 不改协议，只改内部实现：
    - `open_connection`
    - `apply_connection_pragmas`
  - 现在都复用 runtime 的共享连接初始化能力
- `src/lib.rs`
  - 导出 `runtime` 模块，使 Rust 调用方可直接使用多库 runtime
- `README.md` / `docs/LIBRARY_USAGE.zh-CN.md`
  - 新增并明确说明 gRPC 单库与 lib 多库的职责边界

### 4. ⚠️遗留问题与注意事项
- 当前 JSON FFI 仍然是“按 `db_path` 直接调用”的兼容模式，还没有改造成显式 runtime/handle 形式；这是为了先保证兼容现有上层调用。
- `SqliteRuntime` 已足够支撑多库程序化调用，但如果后续需要每库独立日志策略或更细粒度连接池策略，还需要在 runtime 层继续扩展。
- gRPC 单库配置模式是当前刻意保留的稳定边界，后续不建议为了“统一接口”而把多库能力再塞回配置文件。
