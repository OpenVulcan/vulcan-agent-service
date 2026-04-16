# 任务目标

为 `D:\projects\VulcanLocalDataGateway\vldb-lancedb` 补充完整的库模式说明文档，重点说明动态库产物的用途、头文件与导出接口的使用方式、典型调用流程以及与原有 gRPC 二进制入口的边界关系。

# 执行步骤

1. 梳理当前 `vldb-lancedb` 的导出面与现有文档状态。
   - 检查 `README.md`
   - 检查 `include/vldb_lancedb.h`
   - 检查 `src/lib.rs`、`src/ffi.rs`、`src/runtime.rs` 中实际已导出的能力

2. 设计文档结构。
   - 明确区分二进制模式与库模式
   - 补充动态库产物说明
   - 补充头文件、句柄生命周期、字符串/字节释放规则
   - 补充典型调用顺序与错误处理说明

3. 更新 `vldb-lancedb` 文档。
   - 优先在现有 `README.md` 中补齐核心说明
   - 如现有篇幅不适合承载全部内容，则新增独立 Markdown 文档并在 README 中链接

4. 回归验证。
   - 确认文档内容与当前代码导出接口一致
   - 检查示例中的函数名、结构名、路径与行为描述是否准确

5. 归档。
   - 在计划文件末尾追加执行变更总结
   - 完成后归档到 `docs/completed/20260416/`

# 技术选型

- 文档以中文为主，面向当前工程维护者和后续接入方
- 优先复用 `README.md`，避免接口说明分散
- 文档说明严格以当前已经实现的 FFI 接口为准，不预写尚未落地的能力

# 验收标准

1. `README.md` 或新增文档中明确说明库模式的使用目的与适用场景。
2. 清楚说明动态库文件、头文件、句柄创建/销毁、字符串/字节释放规则。
3. 明确列出当前已导出的主要接口及其职责。
4. 提供至少一段典型调用流程说明，使接入方能理解完整生命周期。
5. 文档内容与当前代码实现一致，不误导后续接入。

---

# 执行变更总结

## 1. 核心修复与调整概述

- 为 `vldb-lancedb` 新增了一份专门面向库模式与 FFI 使用方的中文文档，系统说明了 Rust 嵌入、动态库产物、头文件、句柄生命周期、JSON 输入结构与资源释放规则。
- 更新了仓库根 README 与中文完整指南，增加库模式文档入口，避免后续接入方只看到 gRPC 说明而找不到动态库与头文件的使用方式。
- 文档内容严格以当前已经实现的 `lib` 与 `ffi` 接口为准，没有预写尚未落地的能力。

## 2. 📂文件变更清单

### 新增

- `D:\projects\VulcanLocalDataGateway\vldb-lancedb\docs\LIBRARY_USAGE.zh-CN.md`

### 修改

- `D:\projects\VulcanLocalDataGateway\vldb-lancedb\README.md`
- `D:\projects\VulcanLocalDataGateway\vldb-lancedb\docs\README.zh-CN.md`

## 3. 💻关键代码调整详情

- 本轮未修改运行时代码或 FFI 导出逻辑，核心工作聚焦在文档说明补全。
- `docs/LIBRARY_USAGE.zh-CN.md`
  - 补充了 `rlib/cdylib` 产物说明
  - 补充了 `include/vldb_lancedb.h` 的定位与使用边界
  - 补充了 Runtime / Engine / ByteBuffer 的生命周期说明
  - 补充了建表、写入、检索、删除、删表的 JSON 输入结构和返回结构示例
  - 补充了一个完整的最小 FFI 调用顺序
- `README.md` 与 `docs/README.zh-CN.md`
  - 增加库模式文档入口与简要定位说明

## 4. ⚠️遗留问题与注意事项

- 当前仅补充了中文版库模式说明，English Guide 还没有同步同等粒度的 library/ffi 文档。
- `vldb-lancedb` 仓库工作区中还存在本轮之前的既有代码改动与未跟踪文件；本次文档任务没有处理这些既有状态，只在其上追加了文档说明。
- 如果后续继续扩展新的 FFI 导出函数，必须同步更新 `include/vldb_lancedb.h` 与 `docs/LIBRARY_USAGE.zh-CN.md`，否则文档会很快失真。
