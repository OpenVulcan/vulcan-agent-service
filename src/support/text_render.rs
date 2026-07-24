/// Append one formatted Markdown line to a rendered text buffer.
/// 向渲染文本缓冲区追加一行格式化 Markdown 文本。
pub(crate) fn append_rendered_line(rendered: &mut String, line: std::fmt::Arguments<'_>) {
    rendered.push_str(&line.to_string());
    rendered.push('\n');
}

/// Append one blank Markdown line to a rendered text buffer.
/// 向渲染文本缓冲区追加一个 Markdown 空行。
pub(crate) fn append_blank_rendered_line(rendered: &mut String) {
    rendered.push('\n');
}
