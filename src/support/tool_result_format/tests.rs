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
        "vulcan-agent-service-tool-result-{}-{}-{}",
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
    )
    .expect("plain result should render");
    assert_eq!(rendered, "short");
}

/// Verify truncate mode renders the configured overflow notice when content exceeds the budget.
/// 验证截断模式在内容超出预算时会渲染配置的超限提示。
#[test]
fn truncate_mode_returns_notice_when_overflowed() {
    // Serialize default-template assertions with tests that replace process-wide template roots.
    // 将默认模板断言与替换进程级模板根的测试串行化。
    let _guard = template_runtime_lock().lock().expect("lock should succeed");
    let rendered = render_tool_result_text(
        &RuntimeInvocationResult::from_content_parts(
            "line1\nline2\nline3".to_string(),
            Some(ToolOverflowMode::Truncate),
            None,
            None,
        ),
        Some("vulcan-codekit"),
        Some(&sample_budget()),
        &HostRenderOptions::default(),
    )
    .expect("truncate result should render");
    assert!(rendered.contains("Content has been truncated"));
}

/// Directory-shaped template paths should surface as render errors instead of falling back silently.
/// 目录形态模板路径应显式返回渲染错误，而不是静默回退。
#[test]
fn render_tool_result_reports_directory_shaped_template_path() {
    let root = unique_test_dir("directory-template");
    let skill_root = root.join("skills");
    let template_path = skill_root
        .join("vulcan-codekit")
        .join("overflow_templates")
        .join("overflow_truncate.md");
    std::fs::create_dir_all(&template_path).expect("failed to create directory-shaped template");

    let error = render_tool_result_text(
        &RuntimeInvocationResult::from_content_parts(
            "line1\nline2\nline3".to_string(),
            Some(ToolOverflowMode::Truncate),
            None,
            None,
        ),
        Some("vulcan-codekit"),
        Some(&sample_budget()),
        &HostRenderOptions {
            template_skill_roots: vec![skill_root],
            ..HostRenderOptions::default()
        },
    )
    .expect_err("directory-shaped template should fail rendering");

    assert!(
        error.contains("overflow template path is not a file"),
        "unexpected error: {error}"
    );
    let _ = std::fs::remove_dir_all(&root);
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
            None,
        ),
        Some("vulcan-codekit"),
        Some(&sample_budget()),
        &HostRenderOptions {
            spill_root: Some(PathBuf::from("target/test-runtime-page-output")),
            ..HostRenderOptions::default()
        },
    )
    .expect("page result should render");
    assert!(rendered.contains("# LARGE RESULT POINTER"));
    assert!(rendered.contains("raw_file:"));
    assert!(rendered.contains("read_01:"));
    assert!(rendered.contains("read_02:"));
}

/// Page mode should report a render error when overflow content needs a spill file but no spill root is available.
/// 分页模式需要写出超限文件但缺少 spill root 时应返回渲染错误。
#[test]
fn page_mode_requires_spill_root_when_overflowed() {
    let error = render_tool_result_text(
        &RuntimeInvocationResult::from_content_parts(
            "line1\nline2\nline3\nline4".to_string(),
            Some(ToolOverflowMode::Page),
            None,
            None,
        ),
        Some("vulcan-codekit"),
        Some(&sample_budget()),
        &HostRenderOptions::default(),
    )
    .expect_err("page overflow without spill root should fail rendering");

    assert!(error.contains("spill_root"), "unexpected error: {error}");
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
            None,
        ),
        Some("vulcan-codekit"),
        Some(&budget),
        &HostRenderOptions {
            spill_root: Some(PathBuf::from("target/test-runtime-page-error")),
            ..HostRenderOptions::default()
        },
    )
    .expect("page overflow error text should render");
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
            None,
        ),
        Some("vulcan-codekit"),
        Some(&sample_budget()),
        &HostRenderOptions::default(),
    )
    .expect("initialized template result should render");

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
            None,
        ),
        Some("vulcan-codekit"),
        Some(&sample_budget()),
        &HostRenderOptions {
            template_skill_roots: vec![project_skill_root.clone()],
            template_resources_root: Some(resources_root.clone()),
            ..HostRenderOptions::default()
        },
    )
    .expect("per-call template result should render");

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
    let fake_exe = exe_dir.join("vulcan-agent-service.exe");
    std::fs::create_dir_all(&exe_dir).expect("failed to create fake exe directory");
    std::fs::write(&fake_exe, b"fake-exe").expect("failed to create fake exe");
    std::fs::create_dir_all(root.join("output")).expect("failed to create hosted root parent");
    std::fs::create_dir_all(root.join("output").join("lua_runtime"))
        .expect("failed to create hosted LuaSkills root");
    std::fs::write(
        root.join("output").join("lua_runtime").join("skills"),
        b"not-a-directory",
    )
    .expect("failed to create file-shaped hosted skills path");
    std::fs::write(root.join("resources"), b"not-a-directory")
        .expect("failed to create file-shaped repository resources path");

    // The hosted executable-side skills marker is a file and should now fail explicitly.
    // 宿主可执行文件侧的 skills 标记为文件，现在应显式失败。
    let hosted_cwd = root.join("repo");
    std::fs::create_dir_all(&hosted_cwd).expect("failed to create cwd");
    let hosted_error = resolve_runtime_skills_root_from_paths(&hosted_cwd, &fake_exe)
        .expect_err("file-shaped hosted skills path should fail");
    assert!(
        hosted_error.contains("hosted template skills root is not a directory"),
        "unexpected error: {hosted_error}"
    );

    // The repository resources marker is a file and should fail after the missing hosted resources path is skipped.
    // 仓库 resources 标记为文件，应在缺失的宿主 resources 路径被跳过后失败。
    let repo_cwd = root.clone();
    let repository_error = resolve_runtime_resources_root_from_paths(&repo_cwd, &fake_exe)
        .expect_err("file-shaped repository resources path should fail");
    assert!(
        repository_error.contains("repository template resources root is not a directory"),
        "unexpected error: {repository_error}"
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// Relative executable paths without a stable absolute grandparent should not establish hosted template roots.
/// 没有稳定绝对祖父目录的相对可执行文件路径不应建立宿主模板根。
#[test]
fn implicit_template_roots_reject_relative_executable_without_hosted_parent() {
    // Build an isolated repository path without template markers; the explicit inputs fully determine discovery.
    // 构建一个不含模板标记的隔离仓库路径；发现结果完全由显式输入决定。
    let root = unique_test_dir("template-runtime-relative-exe");
    let repository_cwd = root.join("repo");
    std::fs::create_dir_all(&repository_cwd).expect("repository cwd should be created");

    let skills_root =
        resolve_runtime_skills_root_from_paths(&repository_cwd, std::path::Path::new("host.exe"))
            .expect("relative executable skills inspection should succeed");
    let resources_root = resolve_runtime_resources_root_from_paths(
        &repository_cwd,
        std::path::Path::new("host.exe"),
    )
    .expect("relative executable resources inspection should succeed");

    assert!(
        skills_root.is_none(),
        "relative executable without hosted parent should not resolve template skills"
    );
    assert!(
        resources_root.is_none(),
        "relative executable without hosted parent should not resolve template resources"
    );
    std::fs::remove_dir_all(&root).expect("test runtime root should be removed");
}
