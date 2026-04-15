// ========================================
// 中文：这是一个很长的 TypeScript 行注释备注，用于验证分隔线过滤、元信息过滤与摘要截断能力。
// English: This long TypeScript line comment validates separator filtering, metadata filtering, and summary truncation.
// @param input 这个标签不应进入最终摘要 / This tag must not appear in the final summary.
export function formatDisplayName(input: string): string {
    return input.trim().toUpperCase();
}

/*
 * ----------------------------------------
 * 中文：这是一个很长的 TypeScript 块注释备注，用于验证块注释中的装饰线与星号前缀会被正确过滤。
 * English: This long TypeScript block comment validates banner filtering and leading asterisk cleanup.
 * @returns 这个标签不应进入最终摘要 / This tag must not appear in the final summary.
 */
export function normalizeHeadline(value: string): string {
    return value.trim().replace(/\s+/g, " ");
}
