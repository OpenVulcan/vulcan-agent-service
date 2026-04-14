## 任务目标

统一 `vulcan-mcp-client` 的默认配置目录约定到 `runtime/configs/`，补齐全量 `config.yaml` 示例配置，并为每个可配置项提供中文作用说明与可参考示例值（默认可通过注释保留），确保运行时查找路径、仓库内示例文件与文档说明保持一致。

## 执行步骤

1. 梳理当前配置加载逻辑，确认默认配置查找路径、命令行覆盖优先级与现有配置结构。
2. 恢复并完善 `runtime/configs/config.yaml`，将当前所有应用级可配置项完整写入，并为每一项补充中文说明与示例值。
3. 同步更新配置查找逻辑中的默认目录说明，避免仍然指向旧的 `configs/config.yaml`。
4. 检查相关说明文档是否需要同步调整，确保仓库内说明与实际行为一致。
5. 进行构建或最小化验证，确认配置示例不影响程序编译，且默认路径说明正确。

## 技术选型

- 配置文件格式继续使用 YAML，保持与现有 `serde_yaml` 解析逻辑一致。
- 示例配置采用“实际可用默认值 + 注释示例”的方式组织，兼顾可读性与复制后可直接启用。
- 默认配置目录统一使用 `runtime/configs/config.yaml`，命令行 `-config/--config` 仍保留最高优先级。

## 验收标准

1. 仓库中存在 `runtime/configs/config.yaml`，且包含当前全部应用级配置项。
2. 每个配置项均有清晰中文说明，并提供示例值或示例写法。
3. 程序默认配置查找逻辑与文档说明均指向 `runtime/configs/config.yaml`。
4. 构建或最小验证通过，未引入新的配置路径错误。

## 执行变更总结

### 1. 核心修复与调整概述

- 已将 `runtime/configs/config.yaml` 完整补齐为全量配置模板，覆盖当前 `Config` 结构中的全部应用级可配置项。
- 已为每个配置项补充中文作用说明，并给出可直接参考的示例值；默认仅保留常用监听地址，其余可选项以注释示例形式保留。
- 已修正 `src/config.rs` 中的说明文本，明确仓库模板位于 `runtime/configs/config.yaml`，构建产物运行时默认读取 `<exe_parent>/configs/config.yaml`。

### 2. 📂文件变更清单

- 修改：`src/config.rs`
- 修改：`runtime/configs/config.yaml`
- 新增：`docs/plan/20260414-04-RUNTIME_CONFIG_TEMPLATE_ALIGNMENT.md`

### 3. 💻关键代码调整详情

- 调整 `Config::load` 的注释与报错提示，补充“仓库模板路径”和“构建输出路径”的区别，避免误以为仓库根目录仍使用旧的 `configs/`。
- 重写 `runtime/configs/config.yaml`，按功能分组为基础监听配置、共享工具缓存配置、Lua VM 池配置三大区块，并在每项下加入用途说明与示例。

### 4. ⚠️遗留问题与注意事项

- 当前运行时代码默认查找的仍是构建输出目录中的 `<exe_parent>/configs/config.yaml`，这是与构建脚本同步机制保持一致的设计；仓库中的 `runtime/configs/config.yaml` 作为模板源文件存在。
- 如果后续新增新的应用级配置项，需要同步更新 `src/config.rs` 与 `runtime/configs/config.yaml`，避免模板落后于代码。
