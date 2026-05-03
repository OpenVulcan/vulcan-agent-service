use super::templates::{
    render_template_text, resolve_runtime_resources_root_from_paths,
    resolve_runtime_skills_root_from_paths,
};
use super::{
    HostRenderOptions, RuntimeInvocationResult, ToolOverflowMode,
    initialize_tool_result_template_roots, render_tool_result_text,
};
use crate::config::client_budget::{ClientBudgetSnapshot, EffectiveBudgetScope};
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

/// Return one shared mutex used to serialize template-runtime override tests.
/// 返回一个共享互斥锁，用于串行化模板运行根覆盖测试。
fn template_runtime_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Build one unique temporary directory path for one test case.
/// 为单个测试用例构建唯一的临时目录路径。
fn unique_test_dir(name: &str) -> PathBuf {
    let unique = format!(
        "vulcan-mcp-tool-result-{}-{}-{}",
        name,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    );
    std::env::temp_dir().join(unique)
}

/// Build a compact client budget fixture for tool-result rendering tests.
/// 构建工具结果渲染测试使用的紧凑客户端预算夹具。
fn sample_budget() -> ClientBudgetSnapshot {
    ClientBudgetSnapshot {
        client_name: Some("test".to_string()),
        tool_name: Some("codekit-rg".to_string()),
        skill_name: Some("vulcan-codekit".to_string()),
        matched_client_pattern: Some("*test*".to_string()),
        tool_result: EffectiveBudgetScope {
            bytes: 12,
            lines: -1,
        },
        file_read: EffectiveBudgetScope {
            bytes: 12,
            lines: 2,
        },
        tool_config: json!({}),
    }
}

/// Verify plain results pass through when they fit the effective budget.
/// 验证普通结果在未超出有效预算时会直接透传。
#[test]
fn plain_result_defaults_to_truncate_policy() {
    let rendered = render_tool_result_text(
        &RuntimeInvocationResult::plain("short".to_string()),
        Some("vulcan-codekit"),
        Some(&sample_budget()),
        &HostRenderOptions::default(),
    );
    assert_eq!(rendered, "short");
}

/// Verify truncate mode renders the configured overflow notice when content exceeds the budget.
/// 验证截断模式在内容超出预算时会渲染配置的超限提示。
#[test]
fn truncate_mode_returns_notice_when_overflowed() {
    let rendered = render_tool_result_text(
        &RuntimeInvocationResult::from_content_parts(
            "line1\nline2\nline3".to_string(),
            Some(ToolOverflowMode::Truncate),
            None,
        ),
        Some("vulcan-codekit"),
        Some(&sample_budget()),
        &HostRenderOptions::default(),
    );
    assert!(rendered.contains("Content has been truncated"));
}

/// Verify page mode writes an overflow pointer block when content must be paged.
/// 验证分页模式在内容需要分页时会写出超限指针块。
#[test]
fn page_mode_returns_pointer_block_when_overflowed() {
    let rendered = render_tool_result_text(
        &RuntimeInvocationResult::from_content_parts(
            "line1\nline2\nline3\nline4".to_string(),
            Some(ToolOverflowMode::Page),
            None,
        ),
        Some("vulcan-codekit"),
        Some(&sample_budget()),
        &HostRenderOptions {
            spill_root: Some(PathBuf::from("target/test-runtime-page-output")),
            ..HostRenderOptions::default()
        },
    );
    assert!(rendered.contains("# LARGE RESULT POINTER"));
    assert!(rendered.contains("raw_file:"));
    assert!(rendered.contains("read_01:"));
}

/// Verify page mode returns the unified error when one line exceeds the file-read budget.
/// 验证分页模式在单行超过文件读取预算时返回统一错误。
#[test]
fn page_tool_returns_error_when_single_line_exceeds_file_read_limit() {
    let budget = ClientBudgetSnapshot {
        file_read: EffectiveBudgetScope { bytes: 5, lines: 2 },
        ..sample_budget()
    };
    let rendered = render_tool_result_text(
        &RuntimeInvocationResult::from_content_parts(
            "this-line-is-too-long".to_string(),
            Some(ToolOverflowMode::Page),
            None,
        ),
        Some("vulcan-codekit"),
        Some(&budget),
        &HostRenderOptions {
            spill_root: Some(PathBuf::from("target/test-runtime-page-error")),
            ..HostRenderOptions::default()
        },
    );
    assert_eq!(
        rendered,
        "Tool output exceeds the current MCP client limit."
    );
}

/// Verify template placeholders are replaced with the provided context values.
/// 验证模板占位符会被替换为传入的上下文值。
#[test]
fn render_template_replaces_placeholders() {
    let mut context = HashMap::new();
    context.insert("truncated_content", "abc".to_string());
    context.insert("truncate_notice", "cut".to_string());
    let rendered = render_template_text("{{truncated_content}}\n{{truncate_notice}}", &context);
    assert_eq!(rendered, "abc\ncut");
}

/// Verify initialized skill roots take precedence when resolving overflow templates.
/// 验证解析超限模板时已初始化的技能根目录具有优先级。
#[test]
fn render_tool_result_prefers_initialized_skill_root_templates() {
    let _guard = template_runtime_lock().lock().expect("lock should succeed");
    let root = unique_test_dir("template-runtime");
    let skill_root = root.join("skills");
    let resources_root = root.join("resources");
    let skill_template = skill_root
        .join("vulcan-codekit")
        .join("overflow_templates")
        .join("overflow_truncate.md");
    std::fs::create_dir_all(skill_template.parent().expect("parent should exist"))
        .expect("failed to create skill template directory");
    std::fs::create_dir_all(resources_root.join("overflow_templates"))
        .expect("failed to create resources template directory");
    std::fs::write(
        &skill_template,
        "SKILL TEMPLATE\n{{truncated_content}}\n{{truncate_notice}}",
    )
    .expect("failed to write skill template");

    initialize_tool_result_template_roots(std::slice::from_ref(&skill_root), Some(&resources_root))
        .expect("template roots should initialize");

    let rendered = render_tool_result_text(
        &RuntimeInvocationResult::from_content_parts(
            "line1\nline2\nline3".to_string(),
            Some(ToolOverflowMode::Truncate),
            None,
        ),
        Some("vulcan-codekit"),
        Some(&sample_budget()),
        &HostRenderOptions::default(),
    );

    assert!(rendered.contains("SKILL TEMPLATE"));

    initialize_tool_result_template_roots(&[], None).expect("template roots should reset");
    let _ = std::fs::remove_dir_all(&root);
}

/// Verify per-call template roots override default roots for project-scoped rendering.
/// 验证项目级渲染中单次调用模板根目录会覆盖默认根目录。
#[test]
fn render_tool_result_prefers_per_call_template_roots_for_project_environment() {
    let _guard = template_runtime_lock().lock().expect("lock should succeed");
    let root = unique_test_dir("template-runtime-project");
    let default_skill_root = root.join("default-skills");
    let project_skill_root = root.join("project-skills");
    let resources_root = root.join("resources");
    let default_template = default_skill_root
        .join("vulcan-codekit")
        .join("overflow_templates")
        .join("overflow_truncate.md");
    let project_template = project_skill_root
        .join("vulcan-codekit")
        .join("overflow_templates")
        .join("overflow_truncate.md");
    std::fs::create_dir_all(default_template.parent().expect("parent should exist"))
        .expect("failed to create default template directory");
    std::fs::create_dir_all(project_template.parent().expect("parent should exist"))
        .expect("failed to create project template directory");
    std::fs::create_dir_all(resources_root.join("overflow_templates"))
        .expect("failed to create resources template directory");
    std::fs::write(&default_template, "DEFAULT TEMPLATE")
        .expect("failed to write default template");
    std::fs::write(&project_template, "PROJECT TEMPLATE")
        .expect("failed to write project template");

    initialize_tool_result_template_roots(
        std::slice::from_ref(&default_skill_root),
        Some(&resources_root),
    )
    .expect("template roots should initialize");

    let rendered = render_tool_result_text(
        &RuntimeInvocationResult::from_content_parts(
            "line1\nline2\nline3".to_string(),
            Some(ToolOverflowMode::Truncate),
            None,
        ),
        Some("vulcan-codekit"),
        Some(&sample_budget()),
        &HostRenderOptions {
            template_skill_roots: vec![project_skill_root.clone()],
            template_resources_root: Some(resources_root.clone()),
            ..HostRenderOptions::default()
        },
    );

    assert!(rendered.contains("PROJECT TEMPLATE"));
    assert!(!rendered.contains("DEFAULT TEMPLATE"));

    initialize_tool_result_template_roots(&[], None).expect("template roots should reset");
    let _ = std::fs::remove_dir_all(&root);
}

/// Verify implicit runtime root discovery rejects file-shaped fallback paths.
/// 验证隐式运行时根目录发现会拒绝文件形态的兜底路径。
#[test]
fn implicit_template_roots_reject_file_shaped_skills_and_resources_paths() {
    let root = unique_test_dir("template-runtime-file-shaped");
    let exe_dir = root.join("output").join("bin");
    let fake_exe = exe_dir.join("vulcan-mcp.exe");
    std::fs::create_dir_all(&exe_dir).expect("failed to create fake exe directory");
    std::fs::write(&fake_exe, b"fake-exe").expect("failed to create fake exe");
    std::fs::create_dir_all(root.join("output")).expect("failed to create hosted root parent");
    std::fs::write(root.join("output").join("skills"), b"not-a-directory")
        .expect("failed to create file-shaped hosted skills path");
    std::fs::create_dir_all(root.join("runtime"))
        .expect("failed to create repository runtime parent");
    std::fs::write(root.join("runtime").join("resources"), b"not-a-directory")
        .expect("failed to create file-shaped repository resources path");

    let hosted_cwd = root.join("repo");
    std::fs::create_dir_all(&hosted_cwd).expect("failed to create cwd");
    assert!(
        resolve_runtime_skills_root_from_paths(&hosted_cwd, &fake_exe).is_none(),
        "file-shaped fallback skills path should be rejected"
    );

    let repo_cwd = root.join("repo-two");
    std::fs::create_dir_all(&repo_cwd).expect("failed to create repository cwd");
    assert!(
        resolve_runtime_resources_root_from_paths(&repo_cwd, &fake_exe).is_none(),
        "file-shaped fallback resources path should be rejected"
    );

    let _ = std::fs::remove_dir_all(&root);
}
