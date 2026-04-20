# 第三方版权与许可证声明

本文件用于说明 `runtime/skills/vulcan-codekit` 作为 skill 包运行时所依赖的第三方工具及其许可证信息。

适用范围如下：

- `codekit-ast-detail`
- `codekit-rg`
- `codekit-patch`
- `codekit-markdown-menu`
- 以及由 `dependencies.yaml` 下载并放置到配套工具目录中的相关可执行文件

说明如下：

- 本文件用于包内合规归档与分发说明，不替代上游项目的原始许可证文本。
- 实际分发、复制或随包提供二进制文件时，仍应同时保留上游项目要求附带的版权声明与许可证说明。
- 以下信息以对应上游仓库的官方许可证文件为准。

## 1. ast-grep

- 组件名称：`ast-grep`
- 在本 skill 包中的用途：为 `codekit-ast-detail`、`codekit-ast-tree` 与 `codekit-patch` 提供 AST 结构扫描、结构匹配与补丁验证能力
- 上游仓库：<https://github.com/ast-grep/ast-grep>
- 官方许可证文件：<https://github.com/ast-grep/ast-grep/blob/main/LICENSE>
- 许可证类型：`MIT License`
- 版权声明：`Copyright (c) 2022 Herrington Darkholme`

合规说明：

- `ast-grep` 采用 MIT License 发布。
- 根据其官方许可证，分发软件副本或其实质部分时，应保留版权声明与许可声明。
- 在本 skill 包中如随包分发 `ast-grep` 可执行文件，应继续保留其上游许可证信息。

## 2. ripgrep (rg)

- 组件名称：`ripgrep`
- 可执行文件名称：`rg` / `rg.exe`
- 在本 skill 包中的用途：为 `codekit-rg` 与 `codekit-markdown-menu` 提供快速文本检索与 Markdown 文件扫描能力
- 上游仓库：<https://github.com/BurntSushi/ripgrep>
- 官方许可证说明：<https://github.com/BurntSushi/ripgrep/blob/master/COPYING>
- MIT 许可证文本：<https://github.com/BurntSushi/ripgrep/blob/master/LICENSE-MIT>
- Unlicense 文本：<https://github.com/BurntSushi/ripgrep/blob/master/UNLICENSE>
- 许可证类型：`Unlicense OR MIT License`
- MIT 版权声明：`Copyright (c) 2015 Andrew Gallant`

合规说明：

- `ripgrep` 官方声明其项目采用 `Unlicense` 与 `MIT License` 双许可证发布，使用者可以任选其一遵循。
- 若按 MIT License 路径使用或分发，应保留对应版权声明与许可声明。
- 若随本 skill 包分发 `rg` 或 `rg.exe`，建议一并附带上游 `COPYING`、`LICENSE-MIT` 与 `UNLICENSE` 的原始说明文件或等效许可证归档材料。

## 3. 包内落地要求

为保持 `vulcan-codekit` skill 包的分发规范性，当前目录至少应保留本声明文件，用于记录：

- 第三方工具名称
- 上游来源
- 许可证类型
- 版权声明
- 随包分发时需要保留的许可证要求

若后续新增新的外部二进制工具或第三方运行时依赖，应继续在本文件中追加对应条目。
