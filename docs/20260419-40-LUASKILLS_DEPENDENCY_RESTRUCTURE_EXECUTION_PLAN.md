# LuaSkills 依赖管理与目录结构重构执行方案

## 一、目标

本次改造的主体目标是把 `luaskills` 从“单层源码堆叠”调整为“运行时、技能管理、依赖管理、下载管理分层明确”的结构，并为后续实现完整的 `install / reload / uninstall / enable / disable` 打下基础。

本次执行不是一次性做完完整依赖管理，而是先完成：

1. `src/` 目录结构重排
2. 各职责模块的边界划分
3. 依赖管理与下载管理的骨架落位
4. 保持当前运行时与宿主接入链继续可编译

## 二、设计原则

### 1. 对外仍然只有一个 `luaskills`

对 Rust 宿主或未来 FFI 宿主来说，仍然只接一个库。  
不会再额外拆出第二个“依赖管理 ffi”或第二个“包管理库”让宿主自己拼装。

### 2. 对内必须强制分层

虽然对外是一个库，但内部必须拆成四层：

- `runtime`
  - 负责 LuaSkills 运行时、Lua VM、`vulcan.*` / `vulcan.runtime.*`
- `skill`
  - 负责 `skill.yaml`、技能描述、技能生命周期管理
- `dependency`
  - 负责依赖模型、依赖解析、平台匹配、安装策略
- `download`
  - 负责下载源、GitHub、URL、skilllist、归档与校验

### 3. 先结构归位，再逐步补功能

当前第一阶段不要求立即完成：

- skill 名安装
- skillhub 对接
- 完整依赖自动下载

而是先把目录与模块边界立住，再逐步补内容。

## 三、目标目录结构

计划重构为：

```text
src/
├─ lib.rs
├─ runtime/
│  ├─ mod.rs
│  ├─ engine.rs
│  ├─ context.rs
│  ├─ help.rs
│  ├─ logging.rs
│  ├─ result.rs
│  ├─ cache.rs
│  └─ entry.rs
├─ skill/
│  ├─ mod.rs
│  ├─ manifest.rs
│  └─ manager.rs
├─ dependency/
│  ├─ mod.rs
│  ├─ types.rs
│  ├─ manager.rs
│  └─ platform.rs
├─ download/
│  ├─ mod.rs
│  ├─ manager.rs
│  ├─ github.rs
│  ├─ url.rs
│  └─ skilllist.rs
├─ providers/
│  ├─ mod.rs
│  ├─ sqlite.rs
│  └─ lancedb.rs
└─ host/
   ├─ mod.rs
   └─ options.rs
```

## 四、职责划分

### runtime

负责：

- 加载已存在的 skill
- 注入运行时上下文
- 调用 Lua entry
- 返回 runtime result
- 管理 help 树
- 运行时缓存能力

### skill

负责：

- `skill.yaml` 元数据解析
- skill 生命周期抽象
- install / reload / uninstall / enable / disable 的主体入口

### dependency

负责：

- 依赖分类
- 依赖声明结构
- 平台判断
- 安装与校验计划

重点分类：

- tool 依赖
- lua 依赖
- ffi 依赖

### download

负责：

- GitHub 下载
- URL 下载
- skilllist 下载
- 后续 skillhub 下载
- 归档与校验

## 五、依赖管理设计方向

### 1. tool 依赖

面向：

- ast-grep
- rg
- 未来其他外部命令工具

需要支持：

- GitHub release 下载
- 指定 URL 下载
- skilllist 文件来源
- 根据平台选择资产
- 校验导出文件是否存在
- 可由宿主改写 GitHub 源

### 2. lua 依赖

面向：

- Lua 纯脚本库
- `package.path` / `package.cpath` 相关依赖

### 3. ffi 依赖

面向：

- `.dll / .so / .dylib`
- FFI 所需动态库
- 分系统与分架构版本

## 六、当前阶段实施顺序

### 第一阶段

- 重构 `src/` 目录结构
- 创建 `skill / dependency / download / host / providers / runtime` 分层
- 把现有运行时代码搬迁到对应目录
- 用 `lib.rs` 保持对外导出兼容

### 第二阶段

- 为 `dependency` 增加依赖声明结构
- 为 `download` 增加下载源抽象
- 先落 GitHub / URL / skilllist 三类下载骨架

### 第三阶段

- 在 `skill/manager` 里接入 install / reload / uninstall / enable / disable 主链
- 把依赖解析与下载逐步接到技能生命周期上

## 七、验收标准

本阶段完成时应满足：

1. `luaskills` 目录结构已完成分层
2. 现有 `cargo check` / `cargo test --lib` 仍然通过
3. `vulcan-agent-service` 作为宿主仍然能成功依赖并编译
4. `dependency` 与 `download` 已经有可继续扩展的骨架

## 八、执行变更总结

### 当前阶段进展（进行中）

已完成：

1. `luaskills` 的 `src/` 目录已按 `runtime / skill / dependency / download / providers / host` 分层完成重排
2. `dependencies.yaml` 新格式已正式落地，支持：
   - `tool_dependencies`
   - `lua_dependencies`
   - `ffi_dependencies`
3. 依赖来源已支持三类抽象：
   - `github_release`
   - `url`
   - `skilllist`
4. 依赖包结构已支持：
   - 平台键选择
   - 归档类型
   - 导出文件规则
   - 基于导出文件的存在性检测
5. 共享下载层已接入：
   - GitHub Release 资产解析
   - URL 下载
   - skilllist 远程列表获取
   - zip / tar.gz / raw 三类安装载荷处理
6. 运行时加载链已开始接入依赖准备：
   - `load_from_dirs()` 在加载 skill 前会尝试处理 `dependencies.yaml`
7. 技能状态管理已落地基础能力：
   - `enable`
   - `disable`
   - `uninstall`
   - `reload`
8. `vulcan-codekit` 与 `__demo` 的 `dependencies.yaml` 已迁移到新格式

当前仍在继续推进：

1. skill 安装主链（按 skill 名称 / 描述文件 / 地址安装）尚未接入
2. 宿主侧对 `enable / disable / reload / uninstall` 的公开包装尚未接入
3. `skillhub` 来源尚未接入
4. 更完整的依赖校验（如 checksum / lockfile）尚未接入
