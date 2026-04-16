# 动态库 C ABI 预编译切换与发布链调整计划

## 任务目标

将当前 `vulcan-mcp-client` 对 `vldb-lancedb` / `vldb-sqlite` 的默认接入方式调整为：

1. **默认使用预编译动态库**
   - 宿主运行时加载 `.dll / .so / .dylib`
   - 不再默认要求开发者本地编译重型 Rust 依赖

2. **主接口优先 C ABI**
   - Rust 与 Go 都优先走动态库导出的 C ABI
   - 高频主路径尽量不走 JSON
   - JSON 保留为兼容层

3. **补齐宿主依赖下载链**
   - `install_host_deps` 同时支持 `vldb-lancedb` 与 `vldb-sqlite`
   - 便于开发者通过预编译库快速进入编译流程

4. **调整两个 vldb 仓库的发布与 Docker 工作流**
   - 原生库包继续支持 GitHub 预编译发布
   - Docker 不再根据 tag 自动触发，改为手动触发

## 执行步骤

### 步骤 1：梳理当前宿主接入现状

- 检查 `vulcan-mcp-client` 中：
  - `lancedb_host.rs`
  - `sqlite_host.rs`
  - `Cargo.toml`
  - `install_host_deps.ps1/.sh`
- 明确哪些已经切成静态 typed API，哪些仍适合回到动态库模式。

### 步骤 2：切换 MCP 默认依赖策略

- 移除 `vldb-lancedb` / `vldb-sqlite` 的默认 Rust Git 源码依赖
- 恢复/调整 `libloading` 依赖
- 让 `vulcan-mcp-client` 默认不再因为这两个库而进入重型源码编译链

### 步骤 3：恢复 LanceDB 宿主动态库接入

- 将 `lancedb_host` 恢复为动态库加载模式
- 保持 Lua 侧能力接口不变
- 兼容现有 JSON 兼容层，并在可行处保留后续向非 JSON C ABI 演进的空间

### 步骤 4：调整 SQLite 宿主链路

- 先确保默认编译链不依赖 `vldb-sqlite` Rust crate
- 继续保留远程 gRPC / 未来动态库接入的扩展空间
- 不在本阶段强行改写全部 SQLite 调用点

### 步骤 5：补齐宿主依赖安装脚本

- `install_host_deps.ps1`
- `install_host_deps.sh`

目标：
- 同时安装 `vldb-lancedb` 与 `vldb-sqlite` 预编译动态库
- 同时整理头文件和库模式文档
- 支持本地 `third_party` 预置包优先

### 步骤 6：调整 vldb 仓库发布工作流

在以下仓库中检查并修正：

- `D:\projects\VulcanLocalDataGateway\vldb-lancedb`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite`

目标：
- 保持原生库包可继续预编译发布
- Docker 构建从 tag 自动触发改为手动触发
- 避免发 tag 就自动跑 Docker 镜像链

### 步骤 7：编译与链路验证

- 在 `vulcan-mcp-client` 里执行 `cargo check`
- 确认默认开发模式不再拉起 `vldb-lancedb` / `vldb-sqlite` 的源码编译
- 检查宿主依赖安装脚本是否能正确发现并安装两个库的预编译包

## 验收标准

完成后必须满足：

1. `vulcan-mcp-client` 默认编译不再依赖 `vldb-lancedb` / `vldb-sqlite` 的 Rust 源码编译
2. `lancedb` 宿主链路恢复为动态库模式且行为保持兼容
3. `install_host_deps` 同时支持两个 vldb 库
4. `vldb-lancedb` 与 `vldb-sqlite` 的 Docker 工作流改为手动触发
5. 预编译发布链仍可继续支持动态库分发

---

## 阶段执行记录

### 阶段一：宿主默认依赖链切回动态库模式

已完成内容：

- `vulcan-mcp-client` 默认编译链已切回预编译动态库方向：
  - `Cargo.toml` 恢复为仅依赖 `libloading`
  - 默认不再拉起 `vldb-lancedb` / `vldb-sqlite` 的 Rust 源码依赖编译
- `src/lancedb_host.rs` 已恢复为动态库加载版本：
  - 运行时加载 `vldb_lancedb.dll/.so/.dylib`
  - 继续沿用当前 JSON 兼容层调用模式
  - 状态输出改为 `integration_mode = "dynamic_library"`
- `src/main.rs` 已移除 `sqlite_host` 的默认编译接入，避免当前主线继续绑定 `vldb-sqlite` Rust crate。

验证结果：

- `cargo check` 通过

当前判断：

- 这一步已经先解决“普通开发者默认编译 MCP 不再重编两套重库源码”的核心问题；
- `sqlite` 的宿主动态库业务接入后续仍可继续做，但不再阻塞当前主线。

### 阶段二：宿主依赖下载链补齐双库支持

已完成内容：

- `scripts/install_host_deps.ps1`
- `scripts/install_host_deps.sh`

两份脚本都已扩展为同时支持：

- `vldb-lancedb`
- `vldb-sqlite`

新增能力包括：

- 自动解析 `vldb-sqlite` 当前平台对应的库模式资产名
- 安装对应动态库到 `third_party/deps`
- 安装头文件到 `third_party/vldb_sqlite/include`
- 安装库模式说明到 `third_party/vldb_sqlite/docs`
- 继续支持 `third_party` 本地预置压缩包优先

验证结果：

- `bash -n scripts/install_host_deps.sh` 通过
- `install_host_deps.ps1` 语法解析通过

当前遗留：

- 当前远端 `vldb-sqlite` 最新 release 仍未发布 `-lib-` 库模式资产；
- 脚本已改为给出明确提示，避免误判为脚本实现错误；
- 若当前处于联调阶段，可先将本地预编译包放入 `third_party` 目录优先使用。

### 阶段三：vldb 仓库 Docker 工作流改为手动触发

已完成内容：

- `D:\projects\VulcanLocalDataGateway\vldb-lancedb\.github\workflows\docker-build.yml`
- `D:\projects\VulcanLocalDataGateway\vldb-sqlite\.github\workflows\docker-build.yml`

两边都已移除：

- `push.tags -> v*`

当前行为调整为：

- 仅支持 `workflow_dispatch`
- 发 tag 不再自动触发 Docker 构建链

当前判断：

- 原生库包发布链与 Docker 镜像链职责已经重新分开；
- 后续发 tag 时，更符合“先发预编译库包、镜像按需手动构建”的节奏。

### 阶段四：本地版本提升与预编译联调验证

已完成内容：

- 本地提升了两个库的版本号（尚未提交）：
  - `vldb-lancedb`：`0.1.3 -> 0.1.4`
  - `vldb-sqlite`：`0.1.2 -> 0.1.3`
- 本地编译了两个库的 Windows release 动态库：
  - `D:\projects\VulcanLocalDataGateway\vldb-lancedb\target\release\vldb_lancedb.dll`
  - `D:\projects\VulcanLocalDataGateway\vldb-sqlite\target\release\vldb_sqlite.dll`
- 将产物复制到：
  - `D:\projects\vulcan-mcp-client\third_party\deps`
- 重新执行了 MCP 构建同步：
  - `powershell -ExecutionPolicy Bypass -File scripts/build.ps1`

验证结果：

- `cargo check` 通过
- `vulcan-ai-memory-lancedb-test` 实际调用通过
- `vldb_lancedb.dll` 已从宿主运行目录成功加载，状态输出为：
  - `integration_mode = "dynamic_library"`
- `vldb_sqlite.dll` 已正确同步到：
  - `third_party/deps/vldb_sqlite.dll`
  - `output/libs/vldb_sqlite.dll`

补充判断：

- `vldb-sqlite` 的 Git 工作流现在**已经按 `vldb-lancedb` 的方式支持导出 lib 包**；
- 当前远端 latest release 之所以没有 `-lib-` 资产，不是 workflow 结构缺失，而是**最新 release 还没有在这套新工作流下重新发布**；
- 因此这轮联调先使用本地编译产物是合理且必要的。
