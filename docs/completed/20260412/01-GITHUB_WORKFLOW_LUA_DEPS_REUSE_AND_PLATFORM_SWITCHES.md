# 任务目标

调整 `.github/workflows/build-lua-deps.yml`，解决手动触发场景下平台选择失准、未选平台仍执行构建、上传阶段误失败的问题，并优化本地 runner 的复用策略，尽量复用已有工具与编译产物，避免重复下载和重复编译。

# 执行步骤

1. 梳理现有 workflow 的输入参数、矩阵展开方式、缓存键与各平台步骤条件，确认导致重复构建和平台误执行的根因。
2. 将“统一平台选择”改为“各平台独立开关 + 各平台独立 runner 选择”，允许按平台彻底关闭某个系统任务。
3. 重构矩阵数据结构与步骤执行条件，确保仅对启用的平台执行安装、构建、打包和上传。
4. 优化缓存与本地 runner 复用逻辑，使工具目录与依赖产物可以基于平台、runner、依赖版本和脚本版本稳定命中，减少重复下载与重复编译。
5. 进行静态验证，重点检查 Windows、Linux、macOS 在不同开关组合下的执行路径、上传条件和缓存行为是否正确。

# 技术选型

- 保持 GitHub Actions 单工作流实现，避免引入额外 workflow 带来的维护分裂。
- 使用矩阵字段显式携带 `enabled`、`runner`、`platform`、`tool_cache_key`、`deps_cache_key` 等上下文，降低跨步骤条件表达式的重复和歧义。
- 使用 `actions/cache@v4` 对工具目录与 `deps` 构建产物分别缓存，以便本地 runner 和 GitHub Hosted Runner 都能基于键值复用。
- 用显式版本指纹或脚本版本号参与缓存键，确保 workflow 或依赖版本变更后能够自动失效旧缓存。

# 验收标准

1. 手动触发时可以分别控制 `windows-x64`、`linux-x64`、`linux-arm64`、`macos-arm64` 是否启用。
2. 未启用的平台不会进入安装、编译、打包、上传步骤。
3. 已命中缓存的平台不会重新下载工具或重新编译依赖。
4. 上传步骤只会处理当前启用且已产出压缩包的平台，不会因未生成文件而失败。
5. workflow 配置整体可读性提升，关键条件逻辑清晰且可复核。

# 执行变更总结

## 1. 核心修复与调整概述

- 移除了原先统一的 `platforms` 总开关，改为四个平台分别通过 `runner/off` 进行显式控制，实现“按平台启用或彻底关闭”。
- 新增 `prepare-matrix` 预处理任务，只把开启的平台注入构建矩阵，从源头避免未选平台进入构建、打包和上传流程。
- 重构依赖复用链路为“本地 runner 持久化状态优先，其次 GitHub Actions cache，最后才执行真实编译”。
- 将缓存键从发布标签 `version` 解耦，改为基于依赖版本、平台、runner 与 workflow 指纹生成，避免同一套依赖因为换 tag 而被迫重编。
- 将 Windows Perl 安装位置改为 runner 持久目录，同时对 Linux/macOS 的工具安装改为缺失检测后再安装，降低重复下载与重复安装概率。

## 2. 📂文件变更清单

- 新增：`docs/plan/20260412-01-GITHUB_WORKFLOW_LUA_DEPS_REUSE_AND_PLATFORM_SWITCHES.md`
- 修改：`.github/workflows/build-lua-deps.yml`

## 3. 💻关键代码调整详情

- workflow 输入层：
  将 `linux_x64_runner`、`linux_arm64_runner`、`macos_arm64_runner`、`windows_x64_runner` 统一改为带 `off` 选项的独立选择器，取消统一平台选择入口。
- 矩阵生成层：
  新增 `prepare-matrix` 任务，通过 PowerShell 函数 `Add-Target` 按输入动态生成矩阵，只输出启用的平台。
- 依赖复用层：
  新增本地状态恢复步骤，Windows 使用 `%USERPROFILE%\.cache\vulcan-mcp\lua-deps\...`，Unix 使用 `$HOME/.cache/vulcan-mcp/lua-deps/...`。
- 缓存层：
  将 `actions/cache` 的 key 改为依赖布局版本、依赖版本、平台、runner 与 workflow 文件哈希组合，避免旧缓存错误复用，同时不再绑定 release tag。
- 工具安装层：
  Windows 构建工具改为按缺失命令检查后再 `choco install`；便携版 Perl 改存放到 runner 持久目录；Linux/macOS 改为仅在缺少编译工具时才调用 `apt-get` 或 `brew`。
- 打包与上传层：
  因为矩阵仅包含启用平台，上传步骤无需再做全局平台条件判断，从而规避未生成文件导致的错误上传失败。

## 4. ⚠️遗留问题与注意事项

- 本次完成的是静态验证与条件演算，尚未在真实 GitHub Actions 环境执行一次完整工作流。
- 本地持久化依赖状态通过 `DEPS_LAYOUT_VERSION` 控制失效；若未来本地目录结构或构建语义发生变化，需要同步提升该版本号。
- 目前仍保留多平台并行向同一 release tag 上传产物的模式；如后续遇到 release 竞争写入问题，可再拆分为“构建产物上传到 artifact + 汇总发布”两段式流程。

## 5. 补充修复记录（解析错误与 Windows libyaml）

### 5.1 核心补充说明

- 修复了 GitHub Actions 在 `build` 作业级 `env` 中无法解析 `env.*` 与 `runner.*` 上下文的问题，避免 workflow 在排队阶段直接报错。
- 将 Windows x64 下 `libyaml` 的构建方案正式并入本任务记录，确认继续采用源码编译，不再依赖 `vcpkg`。
- 撤销额外拆分出的 `02` 计划文档，统一把后续修复记录沉淀回当前任务文档，保持计划闭环一致。

### 5.2 关键调整内容

- 将 `DEPS_STATE_KEY` 从作业级表达式拼接改为静态字符串，消除 `env.DEPS_LAYOUT_VERSION` 等上下文在该位置不可用的问题。
- 将 `DEPS_DIR` 的路径判断从 `runner.os` 改为 `matrix.os`，使作业级表达式使用 GitHub Actions 可识别的矩阵上下文。
- 保留 Windows 下 `libyaml` 的源码编译步骤，继续规避 self-hosted runner 中 `vcpkg` 的 manifest/builtin-baseline 限制。

### 5.3 文件补充变更

- 修改：`.github/workflows/build-lua-deps.yml`
- 删除：`docs/completed/20260412/02-WINDOWS_VCPKG_REMOVAL_FOR_LIBYAML.md`

## 6. 补充修复记录（Windows libyaml 的 CMake 兼容性）

### 6.1 核心补充说明

- 真实运行暴露出 Windows `Build libyaml (Windows)` 步骤在较新的 CMake 版本下会因为 `libyaml 0.2.5` 的旧策略声明而在配置阶段失败。
- 本次继续沿用源码编译方案，但在 CMake 调用中显式补充策略兼容参数，避免再次在 `CMakeLists.txt:2` 处中断。
- 同步提升依赖布局版本，确保 self-hosted runner 不会继续复用修复前的本地依赖状态。

### 6.2 关键调整内容

- 在 Windows 的 `libyaml` CMake 命令后追加 `-DCMAKE_POLICY_VERSION_MINIMUM=3.5`，兼容新版本 CMake 对旧项目策略的校验要求。
- 初次修复时曾将整包依赖版本提升到 `20260413-01` 以避免命中旧状态，后续已在第 7 节进一步收窄为仅跟踪 Windows `libyaml` 的局部修复版本。

### 6.3 文件补充变更

- 修改：`.github/workflows/build-lua-deps.yml`

## 7. 补充修复记录（收窄 libyaml 修复的缓存影响）

### 7.1 核心补充说明

- 针对“只有 `libyaml` 出问题，不应连带让 openssl/zlib/pcre2 重新编译”的反馈，缩小了这次修复对本地依赖状态键的影响范围。
- 恢复整套依赖的 `DEPS_LAYOUT_VERSION` 到原先的 `20260412-02`，避免 self-hosted runner 的整包本地状态因 `libyaml` 修复而全部失效。
- 新增仅针对 Windows `libyaml` 的构建版本标记，让已有的整包依赖可以复用，只在需要时单独刷新 `libyaml`。

### 7.2 关键调整内容

- 新增 `WINDOWS_LIBYAML_BUILD_VERSION` 变量，专门跟踪 Windows `libyaml` 的修复级别。
- 在 Windows 路径下新增 `Check libyaml refresh state (Windows)` 步骤，通过 `deps/libyaml/.build-version` 判断是否仅需刷新 `libyaml`。
- 将 `Setup MSVC (Windows)`、`Install build tools (Windows)`、`Build libyaml (Windows)` 的条件调整为“整包缺失或仅 libyaml 需要刷新”。
- 新增 `Write libyaml build marker (Windows)` 步骤，在 `deps/libyaml/.build-version` 写入修复版本，供后续运行做增量判断。
- 将 `actions/cache` 的 key 从整个 workflow 文件哈希改为“整包依赖状态 + 平台 + runner + Windows libyaml 修复版本”的组合，避免一次 `libyaml` 修复导致 openssl/zlib/pcre2 的远端缓存也整体失效。

### 7.3 文件补充变更

- 修改：`.github/workflows/build-lua-deps.yml`

## 8. 补充修复记录（避免后续失败导致 openssl 再次重编）

### 8.1 核心补充说明

- 进一步确认了 openssl 仍然重编的直接原因：此前的设计只有在整个 job 成功结束后才会保存整包依赖状态；一旦中途在 `libyaml` 失败，前面已经编好的 openssl/zlib/pcre2 都不会被持久化。
- 为解决这个问题，Windows 路径已改为“组件级恢复 + 组件级判定 + 组件级立即持久化”，后续即使再次在 `libyaml` 失败，也不会让 openssl 白编一遍。

### 8.2 关键调整内容

- 将 `Restore deps from local state (Windows)` 从“只接受 `.complete` 的整包命中”调整为“允许恢复未完成但已存在的局部组件”。
- 将远端 cache 的 restore 条件改为“仅在本地没有完整整包时再尝试”，同时补充 `restore-keys`，允许回退命中旧键前缀。
- 新增 `Check dependency refresh state (Windows)` 步骤，分别判断 `openssl`、`zlib`、`pcre2`、`libyaml` 是否真的需要重建。
- 将 Windows 的 `Build OpenSSL`、`Build zlib`、`Build PCRE2` 条件改为依赖各自的组件级刷新结果，而不再受整包 miss 连带触发。
- 在 Windows 的 `openssl`、`zlib`、`pcre2`、`libyaml` 各自构建完成后立即复制到外部本地状态目录，避免后续步骤失败时前序产物丢失。

### 8.3 文件补充变更

- 修改：`.github/workflows/build-lua-deps.yml`

### 8.4 闭环补充说明

- 继续修正了“局部恢复后重新补齐成功，但不会回写 `.complete` 标记”的边界问题。
- 将最终整包持久化步骤的执行条件从“本地是否命中过任何状态”改为“本地是否已经是完整状态”，确保从半成品状态恢复并补齐后，后续运行会被识别为完整命中而不是反复走补救分支。

## 9. 补充修复记录（Windows libyaml 源码包类型与 PowerShell 参数）

### 9.1 核心补充说明

- 进一步验证后确认，Windows `libyaml` 失败并不是单纯的 CMake 版本兼容问题，还包含两个具体根因：其一，原先下载的是 GitHub release 附件，该压缩包缺少 `cmake/config.h.in`、`yamlConfig.cmake.in` 等 CMake 构建必需文件；其二，PowerShell 会把未加引号的 `-DCMAKE_POLICY_VERSION_MINIMUM=3.5` 拆成错误参数，导致 CMake 实际收到的值变成 `3` 和额外的 `.5`。
- 因此本次将 Windows `libyaml` 的源码来源切换为 Git tag 的完整源码归档，并把相关 `cmake -D` 参数改为显式字符串传递，确保这条构建链路在 self-hosted Windows runner 上可执行。

### 9.2 关键调整内容

- 将 Windows `libyaml` 下载地址从 `releases/download/.../yaml-0.2.5.tar.gz` 调整为 `archive/refs/tags/0.2.5.tar.gz`。
- 将解压后的目录名从 `yaml-0.2.5` 改为 `libyaml-0.2.5`，与 Git tag 归档实际结构保持一致。
- 将 Windows 下的 CMake 参数全部改为显式字符串，并补充 `-DBUILD_TESTING=OFF`，避免 PowerShell 参数拆分和不必要的测试构建。
- 将 `WINDOWS_LIBYAML_BUILD_VERSION` 提升为 `20260413-02`，仅针对 Windows `libyaml` 修复语义做局部刷新，不影响整包依赖版本号。

### 9.3 文件补充变更

- 修改：`.github/workflows/build-lua-deps.yml`

## 10. 补充修复记录（Windows 打包步骤的 tar 参数传递）

### 10.1 核心补充说明

- 真实运行再次暴露出 Windows 打包步骤在 PowerShell 下调用 `tar.exe` 时存在参数传递问题。
- 原先写法直接把 `openssl/ zlib/ pcre2/ libyaml/` 作为裸参数传给 `tar`，在 PowerShell 环境里会被错误折叠，最终导致 `tar.exe` 收到空目录参数并以非零状态退出。

### 10.2 关键调整内容

- 将 Windows 打包步骤中的归档成员改为 PowerShell 字符串数组 `@("openssl", "zlib", "pcre2", "libyaml")`。
- 通过数组展开把目录名显式传给 `tar czf`，避免尾部斜杠在 PowerShell 参数绑定阶段被错误处理。
- 保持打包产物名称与后续 release 上传逻辑不变，仅修正 Windows 下的参数传递方式。

### 10.3 文件补充变更

- 修改：`.github/workflows/build-lua-deps.yml`
