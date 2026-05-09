use super::{ServiceScope, ServiceStartup};
use crate::service::definition::ServiceInstallArtifact;
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Persistent service installation manifest written under the runtime root.
/// 写入运行根下的持久化服务安装 manifest。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct HostServiceManifest {
    /// Platform OS name used during installation.
    /// 安装时使用的平台操作系统名称。
    pub(crate) platform: String,
    /// Service manager kind used during installation.
    /// 安装时使用的服务管理器种类。
    pub(crate) manager: String,
    /// Stable service name used by the platform manager.
    /// 平台管理器使用的稳定服务名。
    pub(crate) service_name: String,
    /// Stable display name shown to the operator.
    /// 展示给运维人员的稳定显示名称。
    pub(crate) display_name: String,
    /// Optional service description.
    /// 可选的服务描述。
    pub(crate) description: Option<String>,
    /// Installed runtime root used by service mode.
    /// 服务模式使用的已安装运行根。
    pub(crate) runtime_root: PathBuf,
    /// Installed executable path used by the service definition.
    /// 服务定义使用的已安装可执行文件路径。
    pub(crate) executable_path: PathBuf,
    /// Optional file-based service definition path.
    /// 可选的文件型服务定义路径。
    pub(crate) definition_path: Option<PathBuf>,
    /// Optional label or unit name written by the platform manager.
    /// 平台管理器写入的可选标签或单元名。
    pub(crate) label_or_unit_name: Option<String>,
    /// Installation scope.
    /// 安装作用域。
    pub(crate) scope: ServiceScope,
    /// Installation startup policy.
    /// 安装启动策略。
    pub(crate) startup: ServiceStartup,
    /// Rendered install timestamp in local time.
    /// 以本地时间渲染的安装时间戳。
    pub(crate) installed_at: String,
}

impl HostServiceManifest {
    /// Build one manifest from a prepared install artifact and timestamp.
    /// 基于已准备好的安装产物和时间戳构建一份 manifest。
    pub(crate) fn from_artifact(
        artifact: &ServiceInstallArtifact,
        installed_at: DateTime<Local>,
    ) -> Self {
        Self {
            platform: std::env::consts::OS.to_string(),
            manager: artifact.manager.as_str().to_string(),
            service_name: artifact.service_name.clone(),
            display_name: artifact.display_name.clone(),
            description: artifact.description.clone(),
            runtime_root: artifact.runtime_root.clone(),
            executable_path: artifact.executable_path.clone(),
            definition_path: artifact.definition_path.clone(),
            label_or_unit_name: artifact.label_or_unit_name.clone(),
            scope: artifact.scope,
            startup: artifact.startup,
            installed_at: installed_at.to_rfc3339(),
        }
    }

    /// Load the best-effort manifest from the standard runtime-root search paths.
    /// 从标准运行根搜索路径中尽力加载 manifest。
    pub(crate) fn load_best_effort(
        service_name: &str,
        scope: ServiceScope,
    ) -> Result<Option<Self>, Box<dyn std::error::Error>> {
        let candidate_paths = candidate_manifest_paths(service_name, scope)?;
        for path in candidate_paths {
            if !path.exists() {
                continue;
            }
            let content = std::fs::read_to_string(&path)?;
            let manifest = serde_json::from_str::<Self>(&content)?;
            if manifest.service_name != service_name {
                continue;
            }
            return Ok(Some(manifest));
        }
        Ok(None)
    }
}

/// Write one manifest into the standard runtime-root service state location.
/// 将一份 manifest 写入标准运行根服务状态位置。
pub(crate) fn write_manifest_file(
    manifest: &HostServiceManifest,
) -> Result<(), Box<dyn std::error::Error>> {
    let manifest_path = manifest_path(&manifest.runtime_root, &manifest.service_name);
    let parent = manifest_path
        .parent()
        .ok_or_else(|| format!("manifest path has no parent: {}", manifest_path.display()))?;
    std::fs::create_dir_all(parent)?;
    std::fs::write(manifest_path, serde_json::to_string_pretty(manifest)?)?;
    Ok(())
}

/// Remove the persisted manifest file from one runtime root when it exists.
/// 当存在时，从某个运行根移除持久化 manifest 文件。
pub(crate) fn remove_manifest_file(
    runtime_root: &Path,
    service_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let manifest_path = manifest_path(runtime_root, service_name);
    if manifest_path.exists() {
        std::fs::remove_file(manifest_path)?;
    }
    let legacy_manifest_path = legacy_manifest_path(runtime_root);
    if legacy_manifest_path.exists() {
        std::fs::remove_file(legacy_manifest_path)?;
    }
    Ok(())
}

/// Resolve the stable manifest path under one runtime root.
/// 解析某个运行根下的稳定 manifest 路径。
pub(crate) fn manifest_path(runtime_root: &Path, service_name: &str) -> PathBuf {
    runtime_root.join("state").join("service").join(format!(
        "host-service-{}.json",
        sanitize_service_name_for_path(service_name)
    ))
}

/// Resolve the legacy single-file manifest path used before service-name partitioning.
/// 解析按服务名分文件之前使用的旧版单文件 manifest 路径。
fn legacy_manifest_path(runtime_root: &Path) -> PathBuf {
    runtime_root
        .join("state")
        .join("service")
        .join("host-service.json")
}

/// Convert one service name into a stable filesystem-safe manifest suffix.
/// 将服务名转换为稳定且适合文件系统使用的 manifest 后缀。
fn sanitize_service_name_for_path(service_name: &str) -> String {
    service_name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

/// Push both current and legacy manifest candidates for one runtime root.
/// 为某个运行根压入当前与旧版 manifest 候选路径。
fn push_manifest_candidates(
    candidates: &mut Vec<PathBuf>,
    runtime_root: &Path,
    service_name: &str,
) {
    candidates.push(manifest_path(runtime_root, service_name));
    candidates.push(legacy_manifest_path(runtime_root));
}

/// Enumerate best-effort manifest candidate paths for lifecycle commands without an explicit runtime root.
/// 为未显式提供运行根的生命周期命令枚举尽力搜索的 manifest 候选路径。
fn candidate_manifest_paths(
    service_name: &str,
    scope: ServiceScope,
) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    let mut candidates = Vec::new();
    if let Ok(current_dir) = std::env::current_dir() {
        push_manifest_candidates(&mut candidates, &current_dir, service_name);
        push_manifest_candidates(&mut candidates, &current_dir.join("output"), service_name);
    }
    if let Some(home_dir) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        let home_dir = PathBuf::from(home_dir);
        push_manifest_candidates(
            &mut candidates,
            &home_dir.join(".vulcan").join("agent-service"),
            service_name,
        );
    }
    if scope == ServiceScope::System {
        if cfg!(windows) {
            push_manifest_candidates(
                &mut candidates,
                &PathBuf::from("C:\\vulcan\\agent-service\\output"),
                service_name,
            );
        } else if cfg!(target_os = "linux") {
            push_manifest_candidates(
                &mut candidates,
                &PathBuf::from("/opt/vulcan-agent-service/output"),
                service_name,
            );
        } else if cfg!(target_os = "macos") {
            push_manifest_candidates(
                &mut candidates,
                &PathBuf::from("/Applications/VulcanAgentService/output"),
                service_name,
            );
        }
    }
    Ok(candidates)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    /// Return one shared mutex used to serialize current-directory-sensitive manifest tests.
    /// 返回一个共享互斥锁，用于串行化依赖当前目录的 manifest 测试。
    fn manifest_test_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    /// Build one unique temporary directory path for a manifest test case.
    /// 为 manifest 测试用例构建一个唯一临时目录路径。
    fn unique_manifest_test_dir(name: &str) -> PathBuf {
        let unique = format!(
            "vulcan-agent-service-manifest-{}-{}-{}",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or_default()
        );
        std::env::temp_dir().join(unique)
    }

    /// Manifest search should ignore files that belong to a different service name.
    /// manifest 搜索应忽略属于其他服务名的文件。
    #[test]
    fn load_best_effort_skips_manifest_for_other_service_name() {
        let _guard = manifest_test_lock()
            .lock()
            .expect("manifest test lock should be acquired");
        let original_dir = std::env::current_dir().expect("current dir should resolve");
        let temp_dir = unique_manifest_test_dir("skip-other-service");
        std::fs::create_dir_all(temp_dir.join("output"))
            .expect("temporary output directory should be created");
        std::env::set_current_dir(&temp_dir).expect("temporary current dir should be set");
        let runtime_root = temp_dir.join("output");
        let manifest = HostServiceManifest {
            platform: "windows".to_string(),
            manager: "windows-scm".to_string(),
            service_name: "other-service".to_string(),
            display_name: "Other Service".to_string(),
            description: None,
            runtime_root: runtime_root.clone(),
            executable_path: runtime_root.join("bin").join("other-service.exe"),
            definition_path: None,
            label_or_unit_name: None,
            scope: ServiceScope::System,
            startup: ServiceStartup::Auto,
            installed_at: "2026-05-07T00:00:00+08:00".to_string(),
        };
        write_manifest_file(&manifest).expect("manifest should be written");
        let loaded_manifest =
            HostServiceManifest::load_best_effort("vulcan-agent-service", ServiceScope::System)
                .expect("best-effort load should succeed");
        assert!(
            loaded_manifest.is_none(),
            "manifest for another service should be ignored"
        );
        std::env::set_current_dir(&original_dir).expect("original current dir should be restored");
        std::fs::remove_dir_all(&temp_dir).expect("temporary directory should be removed");
    }

    /// Manifest search should still return the matching service manifest from the candidate roots.
    /// manifest 搜索在候选根中仍应返回匹配服务名的文件。
    #[test]
    fn load_best_effort_returns_matching_service_name() {
        let _guard = manifest_test_lock()
            .lock()
            .expect("manifest test lock should be acquired");
        let original_dir = std::env::current_dir().expect("current dir should resolve");
        let temp_dir = unique_manifest_test_dir("match-service");
        std::fs::create_dir_all(temp_dir.join("output"))
            .expect("temporary output directory should be created");
        std::env::set_current_dir(&temp_dir).expect("temporary current dir should be set");
        let runtime_root = temp_dir.join("output");
        let manifest = HostServiceManifest {
            platform: "windows".to_string(),
            manager: "windows-scm".to_string(),
            service_name: "vulcan-agent-service".to_string(),
            display_name: "Vulcan Agent Service".to_string(),
            description: None,
            runtime_root: runtime_root.clone(),
            executable_path: runtime_root.join("bin").join("vulcan-agent-service.exe"),
            definition_path: None,
            label_or_unit_name: None,
            scope: ServiceScope::System,
            startup: ServiceStartup::Auto,
            installed_at: "2026-05-07T00:00:00+08:00".to_string(),
        };
        write_manifest_file(&manifest).expect("manifest should be written");
        let loaded_manifest =
            HostServiceManifest::load_best_effort("vulcan-agent-service", ServiceScope::System)
                .expect("best-effort load should succeed")
                .expect("matching manifest should be returned");
        assert_eq!(loaded_manifest.service_name, "vulcan-agent-service");
        std::env::set_current_dir(&original_dir).expect("original current dir should be restored");
        std::fs::remove_dir_all(&temp_dir).expect("temporary directory should be removed");
    }

    /// Different service names under one runtime root should use distinct manifest files.
    /// 同一运行根下的不同服务名应使用彼此独立的 manifest 文件。
    #[test]
    fn write_manifest_file_partitions_by_service_name() {
        let temp_dir = unique_manifest_test_dir("partition-by-service-name");
        let runtime_root = temp_dir.join("output");
        std::fs::create_dir_all(runtime_root.join("state").join("service"))
            .expect("temporary manifest directory should be created");
        let first_manifest = HostServiceManifest {
            platform: "windows".to_string(),
            manager: "windows-scm".to_string(),
            service_name: "vulcan-agent-service".to_string(),
            display_name: "Vulcan Agent Service".to_string(),
            description: None,
            runtime_root: runtime_root.clone(),
            executable_path: runtime_root.join("bin").join("vulcan-agent-service.exe"),
            definition_path: None,
            label_or_unit_name: None,
            scope: ServiceScope::System,
            startup: ServiceStartup::Auto,
            installed_at: "2026-05-07T00:00:00+08:00".to_string(),
        };
        let second_manifest = HostServiceManifest {
            service_name: "vulcan-agent-service-alt".to_string(),
            executable_path: runtime_root
                .join("bin")
                .join("vulcan-agent-service-alt.exe"),
            ..first_manifest.clone()
        };
        write_manifest_file(&first_manifest).expect("first manifest should be written");
        write_manifest_file(&second_manifest).expect("second manifest should be written");
        let first_path = manifest_path(&runtime_root, &first_manifest.service_name);
        let second_path = manifest_path(&runtime_root, &second_manifest.service_name);
        assert!(first_path.exists(), "first manifest path should exist");
        assert!(second_path.exists(), "second manifest path should exist");
        assert_ne!(
            first_path, second_path,
            "different service names should not collide on one manifest file"
        );
        std::fs::remove_dir_all(&temp_dir).expect("temporary directory should be removed");
    }
}
