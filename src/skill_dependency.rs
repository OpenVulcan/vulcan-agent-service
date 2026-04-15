use indicatif::{ProgressBar, ProgressStyle};
use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue, USER_AGENT};
use serde::Deserialize;
use serde_json::Value;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Fixed dependency manifest filename searched in every skill directory.
/// 每个 skill 目录固定查找的依赖清单文件名。
pub const SKILL_DEPENDENCIES_FILE: &str = "dependencies.yaml";

/// YAML manifest describing one skill's external tool dependencies.
/// 描述单个 skill 外部工具依赖的 YAML 清单。
#[derive(Debug, Clone, Deserialize, Default)]
pub struct SkillDependenciesManifest {
    /// Dependency entries declared by the skill.
    /// 当前 skill 声明的依赖条目列表。
    #[serde(default)]
    pub dependencies: Vec<SkillDependencyEntry>,
}

/// One downloadable dependency entry.
/// 一个可下载的依赖条目。
#[derive(Debug, Clone, Deserialize)]
pub struct SkillDependencyEntry {
    /// Logical dependency name used in logs.
    /// 日志中使用的逻辑依赖名称。
    pub name: String,
    /// Local installed filename used as the skip guard in __tools/bin.
    /// 作为跳过判断依据的本地落库文件名，存放在 __tools/bin 下。
    pub install_as: String,
    /// GitHub-specific source configuration.
    /// GitHub 源配置。
    pub github: GithubDependencySource,
    /// Per-platform asset selection rules.
    /// 按平台选择资源的规则。
    pub targets: Vec<DependencyTarget>,
}

/// GitHub source configuration for a dependency.
/// 依赖的 GitHub 源配置。
#[derive(Debug, Clone, Deserialize)]
pub struct GithubDependencySource {
    /// Human-facing repository URL.
    /// 面向人类展示的仓库地址。
    pub repo: String,
    /// API endpoint used to resolve the latest tag.
    /// 用于解析最新标签的 API 地址。
    pub tag_api: String,
    /// Optional download URL template. Supports {tag}, {version}, and {asset_name}.
    /// 可选的下载 URL 模板，支持 {tag}、{version}、{asset_name} 占位符。
    #[serde(default)]
    pub download_url_template: Option<String>,
}

/// One system-specific asset mapping rule.
/// 一条按系统匹配的资源映射规则。
#[derive(Debug, Clone, Deserialize)]
pub struct DependencyTarget {
    /// Operating system key, for example windows/linux/macos.
    /// 操作系统键，例如 windows/linux/macos。
    pub os: String,
    /// CPU architecture key, for example x86_64/aarch64.
    /// CPU 架构键，例如 x86_64/aarch64。
    pub arch: String,
    /// Downloaded asset filename or template. Supports {tag}/{version}.
    /// 远程资源文件名或模板，支持 {tag}/{version}。
    pub asset_name: String,
    /// Optional platform-specific installed filename override.
    /// 可选的平台级落库文件名覆盖值。
    #[serde(default)]
    pub install_as: Option<String>,
    /// Optional file path inside an archive.
    /// 压缩包内部的目标文件路径，可选，支持 {tag}/{version}。
    #[serde(default)]
    pub archive_path: Option<String>,
    /// Optional executable bit hint for Unix-like systems.
    /// Unix 类系统下是否需要赋予可执行权限的提示，可选。
    #[serde(default)]
    #[allow(dead_code)]
    pub executable: Option<bool>,
}

/// Ordered system identity used to match dependency targets.
/// 用于匹配依赖目标的当前系统标识。
#[derive(Debug, Clone)]
struct CurrentSystem {
    /// Operating system key.
    /// 操作系统键。
    os: String,
    /// CPU architecture key.
    /// CPU 架构键。
    arch: String,
}

/// Ensure one skill directory's dependencies are installed into __tools/bin.
/// 确保单个 skill 目录声明的依赖已经安装到 __tools/bin。
pub fn ensure_skill_dependencies(skill_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let manifest_path = skill_dir.join(SKILL_DEPENDENCIES_FILE);
    if !manifest_path.exists() {
        return Ok(());
    }

    let manifest_text = fs::read_to_string(&manifest_path)?;
    let manifest: SkillDependenciesManifest = serde_yaml::from_str(&manifest_text)?;
    if manifest.dependencies.is_empty() {
        return Ok(());
    }

    let tools_bin_dir = resolve_tools_bin_dir(skill_dir)?;
    fs::create_dir_all(&tools_bin_dir)?;

    let client = build_github_client()?;
    let current_system = detect_current_system();

    for dependency in &manifest.dependencies {
        ensure_one_dependency(&client, &current_system, &tools_bin_dir, dependency)?;
    }

    Ok(())
}

/// Resolve the shared __tools/bin directory for one skill.
/// 解析单个 skill 对应的共享 __tools/bin 目录。
fn resolve_tools_bin_dir(skill_dir: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let skills_root = skill_dir
        .parent()
        .ok_or("skill directory does not have a parent lua_skills directory")?;
    Ok(skills_root.join("__tools").join("bin"))
}

/// Build a GitHub-capable blocking HTTP client.
/// 构造一个可访问 GitHub 的阻塞式 HTTP 客户端。
fn build_github_client() -> Result<Client, Box<dyn std::error::Error>> {
    let mut headers = HeaderMap::new();
    headers.insert(
        USER_AGENT,
        HeaderValue::from_static("vulcan-mcp-skill-downloader"),
    );
    headers.insert(
        ACCEPT,
        HeaderValue::from_static("application/vnd.github+json"),
    );

    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        if !token.trim().is_empty() {
            let header_value = HeaderValue::from_str(&format!("Bearer {}", token.trim()))?;
            headers.insert(AUTHORIZATION, header_value);
        }
    }

    let client = Client::builder().default_headers(headers).build()?;
    Ok(client)
}

/// Ensure a single dependency is present, otherwise download and install it.
/// 确保单个依赖存在；若不存在则下载并安装。
fn ensure_one_dependency(
    client: &Client,
    current_system: &CurrentSystem,
    tools_bin_dir: &Path,
    dependency: &SkillDependencyEntry,
) -> Result<(), Box<dyn std::error::Error>> {
    let install_name = target_install_name(dependency, current_system)?;
    let install_path = tools_bin_dir.join(&install_name);
    if install_path.exists() {
        eprintln!(
            "[LuaSkill:deps] Skip {} because {} already exists",
            dependency.name,
            install_path.display()
        );
        return Ok(());
    }

    let target = dependency
        .targets
        .iter()
        .find(|target| target.os == current_system.os && target.arch == current_system.arch)
        .ok_or_else(|| {
            format!(
                "No dependency target matched current system {}-{} for {}",
                current_system.os, current_system.arch, dependency.name
            )
        })?;

    let tag = fetch_latest_tag(client, &dependency.github.tag_api)?;
    let version = tag.trim_start_matches('v').to_string();
    let resolved_target = render_dependency_target(target, &tag, &version);
    let asset_name = resolved_target.asset_name.clone();
    let download_url = render_download_url(&dependency.github, &tag, &version, &asset_name);

    eprintln!(
        "[LuaSkill:deps] Resolving {} from {} at tag {}",
        dependency.name, dependency.github.repo, tag
    );

    let download_path = download_with_progress(client, &download_url, &asset_name, tools_bin_dir)?;
    install_downloaded_asset(&download_path, &resolved_target, &install_path)?;
    cleanup_download_file(&download_path);

    eprintln!(
        "[LuaSkill:deps] Installed {} -> {}",
        dependency.name,
        install_path.display()
    );
    Ok(())
}

/// Render one dependency target with tag/version placeholders resolved.
/// 使用 tag/version 渲染单个平台目标中的模板字段。
fn render_dependency_target(
    target: &DependencyTarget,
    tag: &str,
    version: &str,
) -> DependencyTarget {
    DependencyTarget {
        os: target.os.clone(),
        arch: target.arch.clone(),
        asset_name: render_template(&target.asset_name, tag, version),
        install_as: target
            .install_as
            .clone()
            .map(|value| render_template(&value, tag, version)),
        archive_path: target
            .archive_path
            .clone()
            .map(|value| render_template(&value, tag, version)),
        executable: target.executable,
    }
}

/// Resolve the final installed filename for the current platform target.
/// 解析当前平台目标最终使用的落库文件名。
fn target_install_name(
    dependency: &SkillDependencyEntry,
    current_system: &CurrentSystem,
) -> Result<String, Box<dyn std::error::Error>> {
    let target = dependency
        .targets
        .iter()
        .find(|target| target.os == current_system.os && target.arch == current_system.arch)
        .ok_or_else(|| {
            format!(
                "No dependency target matched current system {}-{} for {}",
                current_system.os, current_system.arch, dependency.name
            )
        })?;
    Ok(target
        .install_as
        .clone()
        .unwrap_or_else(|| dependency.install_as.clone()))
}

/// Fetch the latest tag string from a GitHub API endpoint.
/// 从 GitHub API 端点获取最新标签字符串。
fn fetch_latest_tag(client: &Client, tag_api: &str) -> Result<String, Box<dyn std::error::Error>> {
    let response = client.get(tag_api).send()?.error_for_status()?;
    let payload: Value = response.json()?;

    if let Some(tag_name) = payload.get("tag_name").and_then(|value| value.as_str()) {
        return Ok(tag_name.to_string());
    }

    if let Some(tag_name) = payload.get("name").and_then(|value| value.as_str()) {
        return Ok(tag_name.to_string());
    }

    if let Some(items) = payload.as_array() {
        if let Some(first_item) = items.first() {
            if let Some(tag_name) = first_item.get("name").and_then(|value| value.as_str()) {
                return Ok(tag_name.to_string());
            }
        }
    }

    Err(format!("Unable to resolve latest tag from {}", tag_api).into())
}

/// Render an asset or URL template with the resolved tag and version.
/// 使用解析出的 tag 和 version 渲染资源名或 URL 模板。
fn render_template(template: &str, tag: &str, version: &str) -> String {
    template.replace("{tag}", tag).replace("{version}", version)
}

/// Render the final download URL for one dependency asset.
/// 为单个依赖资源渲染最终下载 URL。
fn render_download_url(
    source: &GithubDependencySource,
    tag: &str,
    version: &str,
    asset_name: &str,
) -> String {
    let template = source.download_url_template.clone().unwrap_or_else(|| {
        format!(
            "{}/releases/download/{{tag}}/{{asset_name}}",
            source.repo.trim_end_matches('/')
        )
    });

    render_template(&template.replace("{asset_name}", asset_name), tag, version)
}

/// Download one asset file and show a terminal progress bar while streaming bytes.
/// 下载单个资源文件，并在流式写入时显示终端进度条。
fn download_with_progress(
    client: &Client,
    download_url: &str,
    asset_name: &str,
    tools_bin_dir: &Path,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let mut response = client.get(download_url).send()?.error_for_status()?;
    let total_size = response.content_length().unwrap_or(0);
    let download_path = tools_bin_dir.join(asset_name);
    let mut file = File::create(&download_path)?;
    let mut buffer = [0_u8; 16 * 1024];

    let progress = if total_size > 0 {
        let progress = ProgressBar::new(total_size);
        progress.set_style(
            ProgressStyle::with_template(
                "{msg} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})",
            )?
            .progress_chars("=>-"),
        );
        progress.set_message(format!("Downloading {}", asset_name));
        Some(progress)
    } else {
        let progress = ProgressBar::new_spinner();
        progress.set_message(format!("Downloading {}", asset_name));
        progress.enable_steady_tick(std::time::Duration::from_millis(120));
        Some(progress)
    };

    loop {
        let read_bytes = response.read(&mut buffer)?;
        if read_bytes == 0 {
            break;
        }
        file.write_all(&buffer[..read_bytes])?;
        if let Some(progress) = &progress {
            progress.inc(read_bytes as u64);
        }
    }

    if let Some(progress) = &progress {
        progress.finish_with_message(format!("Downloaded {}", asset_name));
    }

    Ok(download_path)
}

/// Install the downloaded file into the final tool path, extracting archives when needed.
/// 将下载文件安装到最终工具路径；若为压缩包则先解压。
fn install_downloaded_asset(
    download_path: &Path,
    target: &DependencyTarget,
    install_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let file_name = download_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("downloaded file name is invalid")?
        .to_lowercase();

    if file_name.ends_with(".zip") {
        extract_zip_asset(download_path, target, install_path)?;
    } else if file_name.ends_with(".tar.gz") || file_name.ends_with(".tgz") {
        extract_targz_asset(download_path, target, install_path)?;
    } else {
        fs::copy(download_path, install_path)?;
    }

    apply_executable_bit_if_needed(install_path, target)?;
    Ok(())
}

/// Extract one file from a ZIP archive into the install path.
/// 从 ZIP 压缩包中提取单个文件到安装路径。
fn extract_zip_asset(
    archive_path: &Path,
    target: &DependencyTarget,
    install_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let archive_file = File::open(archive_path)?;
    let mut archive = zip::ZipArchive::new(archive_file)?;
    let target_name = target.archive_path.as_deref().unwrap_or_else(|| {
        install_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("")
    });

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let normalized_name = entry.name().replace('\\', "/");
        if normalized_name.ends_with(target_name) {
            let mut output = File::create(install_path)?;
            std::io::copy(&mut entry, &mut output)?;
            return Ok(());
        }
    }

    Err(format!(
        "Archive entry {} not found in {}",
        target_name,
        archive_path.display()
    )
    .into())
}

/// Extract one file from a tar.gz archive into the install path.
/// 从 tar.gz 压缩包中提取单个文件到安装路径。
fn extract_targz_asset(
    archive_path: &Path,
    target: &DependencyTarget,
    install_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let archive_file = File::open(archive_path)?;
    let decoder = flate2::read::GzDecoder::new(archive_file);
    let mut archive = tar::Archive::new(decoder);
    let target_name = target.archive_path.as_deref().unwrap_or_else(|| {
        install_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("")
    });

    for entry_result in archive.entries()? {
        let mut entry = entry_result?;
        let entry_path = entry.path()?.to_string_lossy().replace('\\', "/");
        if entry_path.ends_with(target_name) {
            let mut output = File::create(install_path)?;
            std::io::copy(&mut entry, &mut output)?;
            return Ok(());
        }
    }

    Err(format!(
        "Archive entry {} not found in {}",
        target_name,
        archive_path.display()
    )
    .into())
}

/// Apply executable bit to Unix-like targets when requested.
/// 在需要时为 Unix 类系统目标设置可执行权限。
fn apply_executable_bit_if_needed(
    install_path: &Path,
    target: &DependencyTarget,
) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if target.executable.unwrap_or(true) {
            let mut permissions = fs::metadata(install_path)?.permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(install_path, permissions)?;
        }
    }

    #[cfg(not(unix))]
    {
        let _ = (install_path, target);
    }

    Ok(())
}

/// Delete the downloaded archive or binary after installation.
/// 安装完成后删除临时下载文件。
fn cleanup_download_file(download_path: &Path) {
    let _ = fs::remove_file(download_path);
}

/// Detect the current runtime system in normalized keys.
/// 检测当前运行时系统，并归一化为统一键值。
fn detect_current_system() -> CurrentSystem {
    let os = match std::env::consts::OS {
        "windows" => "windows",
        "linux" => "linux",
        "macos" => "macos",
        other => other,
    }
    .to_string();

    let arch = match std::env::consts::ARCH {
        "x86_64" => "x86_64",
        "aarch64" => "aarch64",
        "arm64" => "aarch64",
        "x86" => "i686",
        other => other,
    }
    .to_string();

    CurrentSystem { os, arch }
}
