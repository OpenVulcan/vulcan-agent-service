# 第三方 Rust 依赖许可证报告

本文件由 `python scripts/generate_cargo_deny_notices.py` 通过 `cargo deny list --format json --layout crate` 自动生成。

适用范围：`ast-grep-ffi` Rust 动态库构建时进入非 dev 依赖图的第三方 crates。

说明：

- 本报告不替代各上游项目的原始许可证文本。
- 发布 FFI 动态库时，应随包保留本报告、`THIRD_PARTY_NOTICES.md` 与仓库 `LICENSE`。
- 本报告排除了当前仓库自身的 workspace crate，仅列出第三方依赖。

## 许可证统计

| License | Crates |
| --- | ---: |
| `Apache-2.0` | 38 |
| `BSL-1.0` | 1 |
| `MIT` | 75 |
| `Unicode-3.0` | 1 |
| `Unlicense` | 7 |

## 依赖清单

| Crate | Version | Declared License | cargo-deny Licenses | Source | Repository |
| --- | --- | --- | --- | --- | --- |
| `aho-corasick` | 1.1.4 | `Unlicense OR MIT` | `MIT`, `Unlicense` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/BurntSushi/aho-corasick |
| `ast-grep-config` | 0.42.1 | `MIT` | `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/ast-grep/ast-grep |
| `ast-grep-core` | 0.42.1 | `MIT` | `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/ast-grep/ast-grep |
| `ast-grep-language` | 0.42.1 | `MIT` | `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/ast-grep/ast-grep |
| `bit-set` | 0.10.0 | `Apache-2.0 OR MIT` | `Apache-2.0`, `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/contain-rs/bit-set |
| `bit-vec` | 0.9.1 | `Apache-2.0 OR MIT` | `Apache-2.0`, `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/contain-rs/bit-vec |
| `bstr` | 1.12.1 | `MIT OR Apache-2.0` | `Apache-2.0`, `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/BurntSushi/bstr |
| `cc` | 1.2.61 | `MIT OR Apache-2.0` | `Apache-2.0`, `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/rust-lang/cc-rs |
| `crossbeam-deque` | 0.8.6 | `MIT OR Apache-2.0` | `Apache-2.0`, `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/crossbeam-rs/crossbeam |
| `crossbeam-epoch` | 0.9.18 | `MIT OR Apache-2.0` | `Apache-2.0`, `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/crossbeam-rs/crossbeam |
| `crossbeam-utils` | 0.8.21 | `MIT OR Apache-2.0` | `Apache-2.0`, `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/crossbeam-rs/crossbeam |
| `dyn-clone` | 1.0.20 | `MIT OR Apache-2.0` | `Apache-2.0`, `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/dtolnay/dyn-clone |
| `equivalent` | 1.0.2 | `Apache-2.0 OR MIT` | `Apache-2.0`, `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/indexmap-rs/equivalent |
| `find-msvc-tools` | 0.1.9 | `MIT OR Apache-2.0` | `Apache-2.0`, `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/rust-lang/cc-rs |
| `globset` | 0.4.18 | `Unlicense OR MIT` | `MIT`, `Unlicense` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/BurntSushi/ripgrep/tree/master/crates/globset |
| `hashbrown` | 0.17.0 | `MIT OR Apache-2.0` | `Apache-2.0`, `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/rust-lang/hashbrown |
| `ignore` | 0.4.25 | `Unlicense OR MIT` | `MIT`, `Unlicense` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/BurntSushi/ripgrep/tree/master/crates/ignore |
| `indexmap` | 2.14.0 | `Apache-2.0 OR MIT` | `Apache-2.0`, `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/indexmap-rs/indexmap |
| `itoa` | 1.0.18 | `MIT OR Apache-2.0` | `Apache-2.0`, `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/dtolnay/itoa |
| `log` | 0.4.29 | `MIT OR Apache-2.0` | `Apache-2.0`, `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/rust-lang/log |
| `memchr` | 2.8.0 | `Unlicense OR MIT` | `MIT`, `Unlicense` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/BurntSushi/memchr |
| `proc-macro2` | 1.0.106 | `MIT OR Apache-2.0` | `Apache-2.0`, `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/dtolnay/proc-macro2 |
| `quote` | 1.0.45 | `MIT OR Apache-2.0` | `Apache-2.0`, `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/dtolnay/quote |
| `ref-cast` | 1.0.25 | `MIT OR Apache-2.0` | `Apache-2.0`, `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/dtolnay/ref-cast |
| `ref-cast-impl` | 1.0.25 | `MIT OR Apache-2.0` | `Apache-2.0`, `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/dtolnay/ref-cast |
| `regex` | 1.12.3 | `MIT OR Apache-2.0` | `Apache-2.0`, `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/rust-lang/regex |
| `regex-automata` | 0.4.14 | `MIT OR Apache-2.0` | `Apache-2.0`, `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/rust-lang/regex |
| `regex-syntax` | 0.8.10 | `MIT OR Apache-2.0` | `Apache-2.0`, `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/rust-lang/regex |
| `ryu` | 1.0.23 | `Apache-2.0 OR BSL-1.0` | `Apache-2.0`, `BSL-1.0` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/dtolnay/ryu |
| `same-file` | 1.0.6 | `Unlicense/MIT` | `MIT`, `Unlicense` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/BurntSushi/same-file |
| `schemars` | 1.2.1 | `MIT` | `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/GREsau/schemars |
| `schemars_derive` | 1.2.1 | `MIT` | `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/GREsau/schemars |
| `serde` | 1.0.228 | `MIT OR Apache-2.0` | `Apache-2.0`, `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/serde-rs/serde |
| `serde_core` | 1.0.228 | `MIT OR Apache-2.0` | `Apache-2.0`, `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/serde-rs/serde |
| `serde_derive` | 1.0.228 | `MIT OR Apache-2.0` | `Apache-2.0`, `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/serde-rs/serde |
| `serde_derive_internals` | 0.29.1 | `MIT OR Apache-2.0` | `Apache-2.0`, `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/serde-rs/serde |
| `serde_json` | 1.0.149 | `MIT OR Apache-2.0` | `Apache-2.0`, `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/serde-rs/json |
| `serde_yaml` | 0.9.34+deprecated | `MIT OR Apache-2.0` | `Apache-2.0`, `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/dtolnay/serde-yaml |
| `shlex` | 1.3.0 | `MIT OR Apache-2.0` | `Apache-2.0`, `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/comex/rust-shlex |
| `streaming-iterator` | 0.1.9 | `MIT OR Apache-2.0` | `Apache-2.0`, `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/sfackler/streaming-iterator |
| `syn` | 2.0.117 | `MIT OR Apache-2.0` | `Apache-2.0`, `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/dtolnay/syn |
| `thiserror` | 2.0.18 | `MIT OR Apache-2.0` | `Apache-2.0`, `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/dtolnay/thiserror |
| `thiserror-impl` | 2.0.18 | `MIT OR Apache-2.0` | `Apache-2.0`, `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/dtolnay/thiserror |
| `tree-sitter` | 0.26.8 | `MIT` | `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/tree-sitter/tree-sitter |
| `tree-sitter-bash` | 0.25.1 | `MIT` | `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/tree-sitter/tree-sitter-bash |
| `tree-sitter-c` | 0.24.2 | `MIT` | `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/tree-sitter/tree-sitter-c |
| `tree-sitter-c-sharp` | 0.23.5 | `MIT` | `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/tree-sitter/tree-sitter-c-sharp |
| `tree-sitter-cpp` | 0.23.4 | `MIT` | `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/tree-sitter/tree-sitter-cpp |
| `tree-sitter-css` | 0.25.0 | `MIT` | `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/tree-sitter/tree-sitter-css |
| `tree-sitter-dart` | 0.1.0 | `MIT` | `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/nielsenko/tree-sitter-dart |
| `tree-sitter-elixir` | 0.3.5 | `Apache-2.0` | `Apache-2.0` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/elixir-lang/tree-sitter-elixir |
| `tree-sitter-go` | 0.25.0 | `MIT` | `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/tree-sitter/tree-sitter-go |
| `tree-sitter-haskell` | 0.23.1 | `MIT` | `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/tree-sitter/tree-sitter-haskell |
| `tree-sitter-hcl` | 1.1.0 | `Apache-2.0` | `Apache-2.0` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/tree-sitter-grammars/tree-sitter-hcl |
| `tree-sitter-html` | 0.23.2 | `MIT` | `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/tree-sitter/tree-sitter-html |
| `tree-sitter-java` | 0.23.5 | `MIT` | `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/tree-sitter/tree-sitter-java |
| `tree-sitter-javascript` | 0.25.0 | `MIT` | `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/tree-sitter/tree-sitter-javascript |
| `tree-sitter-json` | 0.23.0 | `MIT` | `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/tree-sitter/tree-sitter-json |
| `tree-sitter-kotlin-sg` | 0.4.0 | `MIT` | `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/fwcd/tree-sitter-kotlin |
| `tree-sitter-language` | 0.1.7 | `MIT` | `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/tree-sitter/tree-sitter |
| `tree-sitter-lua` | 0.5.0 | `MIT` | `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/tree-sitter-grammars/tree-sitter-lua |
| `tree-sitter-nix` | 0.3.0 | `MIT` | `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/nix-community/tree-sitter-nix |
| `tree-sitter-php` | 0.24.2 | `MIT` | `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/tree-sitter/tree-sitter-php |
| `tree-sitter-python` | 0.25.0 | `MIT` | `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/tree-sitter/tree-sitter-python |
| `tree-sitter-ruby` | 0.23.1 | `MIT` | `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/tree-sitter/tree-sitter-ruby |
| `tree-sitter-rust` | 0.24.2 | `MIT` | `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/tree-sitter/tree-sitter-rust |
| `tree-sitter-scala` | 0.25.1 | `MIT` | `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/tree-sitter/tree-sitter-scala |
| `tree-sitter-solidity` | 1.2.13 | `MIT` | `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/JoranHonig/tree-sitter-solidity |
| `tree-sitter-swift` | 0.7.1 | `MIT` | `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/alex-pinkus/tree-sitter-swift |
| `tree-sitter-typescript` | 0.23.2 | `MIT` | `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/tree-sitter/tree-sitter-typescript |
| `tree-sitter-yaml` | 0.7.2 | `MIT` | `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/tree-sitter-grammars/tree-sitter-yaml |
| `unicode-ident` | 1.0.24 | `(MIT OR Apache-2.0) AND Unicode-3.0` | `Apache-2.0`, `MIT`, `Unicode-3.0` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/dtolnay/unicode-ident |
| `unsafe-libyaml` | 0.2.11 | `MIT` | `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/dtolnay/unsafe-libyaml |
| `walkdir` | 2.5.0 | `Unlicense/MIT` | `MIT`, `Unlicense` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/BurntSushi/walkdir |
| `winapi-util` | 0.1.11 | `Unlicense OR MIT` | `MIT`, `Unlicense` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/BurntSushi/winapi-util |
| `windows-link` | 0.2.1 | `MIT OR Apache-2.0` | `Apache-2.0`, `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/microsoft/windows-rs |
| `windows-sys` | 0.61.2 | `MIT OR Apache-2.0` | `Apache-2.0`, `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/microsoft/windows-rs |
| `zmij` | 1.0.21 | `MIT` | `MIT` | registry+https://github.com/rust-lang/crates.io-index | https://github.com/dtolnay/zmij |
