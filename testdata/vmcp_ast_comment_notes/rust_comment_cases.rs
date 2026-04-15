/// 中文：这是 Rust 文档注释备注，用于验证三斜线风格的摘要提取与中英文合并压缩能力。
/// English: This Rust doc comment validates triple slash extraction and multilingual compaction.
pub fn collect_memory_key(input: &str) -> String {
    input.trim().to_lowercase()
}

/*
 * ========================================
 * 日本語：これは Rust ブロックコメント要約の抽出と多言語文字截断を確認するためのテストです。
 * English: This Rust block comment validates multilingual truncation and banner filtering.
 * @returns このタグは最終要約に含めてはいけません / This tag must not appear in the summary.
 */
pub fn normalize_memory_key(input: &str) -> String {
    input.trim().replace(' ', "_")
}
