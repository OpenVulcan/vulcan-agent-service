package fixtures

// ========================================
// 中文：这是 Go 行备注，用于验证双斜线注释在 Go 函数上的摘要提取与长度控制。
// English: This Go line comment validates slash comment extraction and summary length control.
// @param value 这个标签不应进入最终摘要 / This tag must not appear in the final summary.
func BuildPromptName(value string) string {
	return value
}
