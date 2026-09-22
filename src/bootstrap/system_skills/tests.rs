use super::{initialize_system_skills, system_skill_is_installed};
use crate::config::Config;
use crate::config::system_skills::SystemSkillSpec;
use crate::luaskills_adapter::{build_luaskills_cache_config, build_luaskills_engine_options};
use luaskills::{
    LuaEngine, LuaRuntimeHostOptions, LuaVmPoolConfig, RuntimeSkillRoot, SkillInstallSourceType,
    SkillManager, SkillOperationPlane,
};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Hold one isolated application root and the exact host configuration used by initialization.
/// 持有一个隔离的应用根目录以及初始化流程使用的精确宿主配置。
struct TestApplication {
    /// Filesystem root owned by this fixture.
    /// 此夹具独占的文件系统根目录。
    root: PathBuf,
    /// Host configuration pointing every mutable runtime path into this fixture.
    /// 将所有可变运行时路径指向此夹具的宿主配置。
    config: Config,
}

impl TestApplication {
    /// Create one unique application root with no dependency on the repository checkout.
    /// 创建一个唯一应用根目录，不依赖仓库工作区。
    fn new(case_name: &str) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after Unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "vulcan-agent-service-system-skills-{case_name}-{}-{timestamp}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("isolated application root should be created");
        let config = Config {
            runtime_root: Some(root.to_string_lossy().to_string()),
            skill_config_root: Some(root.join("skill-config").to_string_lossy().to_string()),
            skill_roots: Some(Vec::new()),
            ..Config::default()
        };
        Self { root, config }
    }

    /// Return the fixed LuaSkills runtime root selected by the production path resolver.
    /// 返回生产路径解析器选定的固定 LuaSkills 运行时根目录。
    fn runtime_root(&self) -> PathBuf {
        self.root.join("lua_runtime")
    }

    /// Return the ROOT skill directory selected by the production bootstrap flow.
    /// 返回生产 bootstrap 流程选定的 ROOT 技能目录。
    fn skills_root(&self) -> PathBuf {
        self.runtime_root().join("skills")
    }

    /// Write a valid system-skill policy at the exact application-owned path.
    /// 将有效系统技能策略写入应用拥有的精确配置路径。
    fn write_policy(&self, auto_install: bool, skills: &[(&str, &str, bool)]) {
        let configured_skills = skills
            .iter()
            .map(|(name, github, enabled)| {
                json!({
                    "name": name,
                    "github": github,
                    "enabled": enabled,
                })
            })
            .collect::<Vec<_>>();
        let document = json!({
            "format_version": 1,
            "auto_install": auto_install,
            "skills": configured_skills,
        });
        let config_path = self.root.join("configs").join("system_skills.json");
        fs::create_dir_all(config_path.parent().expect("policy parent should exist"))
            .expect("policy directory should be created");
        fs::write(
            config_path,
            serde_json::to_vec_pretty(&document).expect("policy should serialize"),
        )
        .expect("policy should be written");
    }

    /// Build the same ROOT manager used by the CLI initialization implementation.
    /// 构造 CLI 初始化实现使用的相同 ROOT 管理器。
    fn manager(&self) -> SkillManager {
        fs::create_dir_all(self.skills_root()).expect("ROOT skill directory should exist");
        let root = RuntimeSkillRoot {
            name: "ROOT".to_string(),
            skills_dir: self.skills_root(),
        };
        let host_options = LuaRuntimeHostOptions::with_runtime_root(self.runtime_root());
        crate::bootstrap::runtime_init::build_root_skill_manager_for_cli(&root, &host_options)
            .expect("ROOT skill manager should be created")
    }
}

impl Drop for TestApplication {
    /// Remove the isolated fixture after each test, including on ordinary assertion failures.
    /// 每个测试结束后删除隔离夹具，包括普通断言失败的情况。
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// Write one manifest-complete package at the selected ROOT path.
/// 在选定的 ROOT 路径写入一个清单完整的软件包。
fn write_minimal_skill(skills_root: &Path, skill_id: &str, with_entry: bool) {
    let skill_dir = skills_root.join(skill_id);
    fs::create_dir_all(skill_dir.join("runtime")).expect("skill runtime directory should exist");
    let entries = if with_entry {
        "entries:\n  - name: ping\n    description: Minimal ping entry.\n    lua_entry: runtime/ping.lua\n    lua_module: demo-skill.ping\n"
    } else {
        "entries: []\n"
    };
    fs::write(
        skill_dir.join("skill.yaml"),
        format!("name: {skill_id}\nversion: 0.1.0\nenable: true\ndebug: false\n{entries}"),
    )
    .expect("skill manifest should be written");
    if with_entry {
        fs::write(
            skill_dir.join("runtime").join("ping.lua"),
            "return function(args)\n  return 'ok'\nend\n",
        )
        .expect("skill runtime entry should be written");
    }
}

/// Build one policy declaration with the same normalized identity used by initialization.
/// 构造一个使用初始化相同规范化标识的策略声明。
fn skill_spec(skill_id: &str, github: &str) -> SystemSkillSpec {
    SystemSkillSpec {
        name: skill_id.to_string(),
        github: github.to_string(),
        enabled: true,
    }
}

/// Persist one managed installation record using the upstream LuaSkills schema.
/// 使用上游 LuaSkills schema 持久化一条受管安装记录。
fn write_install_record(
    manager: &SkillManager,
    skill_id: &str,
    source_type: SkillInstallSourceType,
    locator: &str,
) {
    let record_root = manager.state_root().join("installs");
    fs::create_dir_all(&record_root).expect("install record directory should exist");
    let record = luaskills::InstalledSkillRecord {
        skill_id: skill_id.to_string(),
        version: "0.1.0".to_string(),
        managed: true,
        source: luaskills::InstalledSkillSourceRecord {
            source_type,
            locator: locator.to_string(),
            tag: Some("v0.1.0".to_string()),
        },
        installed_at_unix_ms: 1,
    };
    fs::write(
        record_root.join(format!("{skill_id}.yaml")),
        serde_yaml::to_string(&record).expect("install record should serialize"),
    )
    .expect("install record should be written");
}

/// Automatic initialization should return before manager construction when policy disables it.
/// 策略禁用自动安装时，自动初始化应在构造管理器前返回。
#[test]
fn auto_install_disabled_skips_before_install() {
    let application = TestApplication::new("auto-disabled");
    application.write_policy(false, &[("demo-skill", "LuaSkills/demo-skill", true)]);

    initialize_system_skills(&application.config, false)
        .expect("disabled automatic initialization should succeed");

    assert!(!application.skills_root().join("demo-skill").exists());
    assert!(
        !application
            .runtime_root()
            .join("state")
            .join("system-skills-init.lock")
            .exists()
    );
}

/// A disabled declaration must remain skipped even when the user explicitly invokes init.
/// 用户显式调用 init 时，单项禁用声明仍必须保持跳过。
#[test]
fn disabled_skill_skips_even_for_explicit_initialization() {
    let application = TestApplication::new("skill-disabled");
    application.write_policy(true, &[("demo-skill", "LuaSkills/demo-skill", false)]);

    initialize_system_skills(&application.config, true)
        .expect("disabled system skill should not require installation");

    assert!(!application.skills_root().join("demo-skill").exists());
    assert!(
        !application
            .runtime_root()
            .join("state")
            .join("system-skills-init.lock")
            .exists()
    );
}

/// A persisted system disabled marker must prevent reinstalling a missing package directory.
/// 持久化的系统停用标记必须阻止重新安装缺失的软件包目录。
#[test]
fn persisted_disabled_marker_skips_missing_skill_directory() {
    let application = TestApplication::new("persisted-disabled");
    application.write_policy(true, &[("demo-skill", "LuaSkills/demo-skill", true)]);
    let manager = application.manager();
    manager
        .disable_skill_in_plane(SkillOperationPlane::System, "demo-skill", Some("test"))
        .expect("system disabled marker should be persisted");

    initialize_system_skills(&application.config, false)
        .expect("disabled marker should skip installation");

    assert!(
        manager
            .disabled_record("demo-skill")
            .expect("disabled marker should be readable")
            .is_some()
    );
    assert!(!application.skills_root().join("demo-skill").exists());
}

/// A complete package and matching install record must make repeated initialization skip installation.
/// 完整软件包与匹配安装记录必须让重复初始化跳过安装。
#[test]
fn already_installed_skill_is_skipped_on_repeated_initialization() {
    let application = TestApplication::new("already-installed");
    application.write_policy(true, &[("demo-skill", "LuaSkills/demo-skill", true)]);
    write_minimal_skill(&application.skills_root(), "demo-skill", false);
    let manager = application.manager();
    write_install_record(
        &manager,
        "demo-skill",
        SkillInstallSourceType::Github,
        "LuaSkills/demo-skill",
    );
    let record_path = manager
        .state_root()
        .join("installs")
        .join("demo-skill.yaml");
    let before = fs::read_to_string(&record_path).expect("install record should be readable");

    initialize_system_skills(&application.config, false)
        .expect("first initialization should skip installed package");
    initialize_system_skills(&application.config, false)
        .expect("second initialization should skip installed package");

    assert_eq!(
        fs::read_to_string(record_path).expect("install record should remain readable"),
        before
    );
}

/// An ignored host skill id must prevent automatic installation even when its policy entry is enabled.
/// 即使策略项启用，被宿主忽略的技能标识也必须阻止自动安装。
#[test]
fn ignored_skill_id_prevents_missing_skill_installation() {
    let application = TestApplication::new("ignored-skill");
    application.write_policy(true, &[("demo-skill", "LuaSkills/demo-skill", true)]);
    let mut config = application.config.clone();
    config.ignored_skill_ids = Some(vec!["demo-skill".to_string()]);

    initialize_system_skills(&config, false)
        .expect("ignored skill should be skipped before installation");

    assert!(!application.skills_root().join("demo-skill").exists());
}

/// Disabled policy entries must be forwarded to engine options and suppress an existing package during loading.
/// 禁用策略项必须传入引擎选项，并在加载时抑制已存在的软件包。
#[test]
fn disabled_policy_skill_is_ignored_by_engine_loading() {
    let application = TestApplication::new("engine-ignored");
    application.write_policy(true, &[("demo-skill", "LuaSkills/demo-skill", false)]);
    write_minimal_skill(&application.skills_root(), "demo-skill", true);
    let options = build_luaskills_engine_options(
        &application.config,
        LuaVmPoolConfig {
            min_size: 1,
            max_size: 1,
            idle_ttl_secs: 300,
        },
        build_luaskills_cache_config(None, None, None),
    )
    .expect("engine options should be built");
    assert!(
        options
            .host_options
            .ignored_skill_ids
            .iter()
            .any(|skill_id| skill_id.eq_ignore_ascii_case("demo-skill"))
    );

    let mut engine = LuaEngine::new(options).expect("engine should be created");
    let root = RuntimeSkillRoot {
        name: "ROOT".to_string(),
        skills_dir: application.skills_root(),
    };
    engine
        .load_from_roots(&[root])
        .expect("disabled package loading should succeed");
    assert!(
        engine
            .list_entries()
            .expect("entry listing should succeed")
            .is_empty(),
        "disabled package entries must not be exposed"
    );
}

/// A managed record whose source differs from policy must fail explicitly before installation.
/// 受管记录来源与策略不一致时，安装前必须显式失败。
#[test]
fn conflicting_install_source_returns_explicit_error() {
    let application = TestApplication::new("source-conflict");
    write_minimal_skill(&application.skills_root(), "demo-skill", false);
    let manager = application.manager();
    write_install_record(
        &manager,
        "demo-skill",
        SkillInstallSourceType::Github,
        "Other/demo-skill",
    );
    let spec = skill_spec("demo-skill", "LuaSkills/demo-skill");

    let error = system_skill_is_installed(&application.skills_root(), &manager, &spec)
        .expect_err("conflicting source should fail");
    assert!(
        error.contains("source conflicts"),
        "unexpected error: {error}"
    );
    assert!(
        error.contains("LuaSkills/demo-skill"),
        "unexpected error: {error}"
    );
}

/// Missing and malformed package or record shapes must produce the documented explicit boundary errors.
/// 缺失或畸形的软件包及记录形态必须产生文档约定的明确边界错误。
#[test]
fn partial_package_and_record_shapes_return_explicit_errors() {
    let application = TestApplication::new("shapes");
    fs::create_dir_all(application.skills_root()).expect("ROOT skill directory should exist");
    let spec = skill_spec("demo-skill", "LuaSkills/demo-skill");
    let manager = application.manager();

    assert!(
        !system_skill_is_installed(&application.skills_root(), &manager, &spec)
            .expect("missing package should be reported as not installed")
    );

    let skill_path = application.skills_root().join("demo-skill");
    fs::write(&skill_path, b"file-shaped skill").expect("file-shaped skill should be created");
    let error = system_skill_is_installed(&application.skills_root(), &manager, &spec)
        .expect_err("file-shaped skill path should fail");
    assert!(
        error.contains("not a directory"),
        "unexpected error: {error}"
    );
    fs::remove_file(&skill_path).expect("file-shaped skill should be removed");

    fs::create_dir_all(&skill_path).expect("incomplete skill directory should exist");
    let error = system_skill_is_installed(&application.skills_root(), &manager, &spec)
        .expect_err("missing manifest should fail");
    assert!(
        error.contains("Incomplete system skill"),
        "unexpected error: {error}"
    );
    fs::create_dir_all(skill_path.join("skill.yaml"))
        .expect("directory-shaped manifest should exist");
    let error = system_skill_is_installed(&application.skills_root(), &manager, &spec)
        .expect_err("directory-shaped manifest should fail");
    assert!(
        error.contains("manifest is not a file"),
        "unexpected error: {error}"
    );
    fs::remove_dir_all(&skill_path).expect("malformed skill directory should be removed");

    write_minimal_skill(&application.skills_root(), "demo-skill", false);
    fs::create_dir_all(
        manager
            .state_root()
            .join("installs")
            .join("demo-skill.yaml"),
    )
    .expect("directory-shaped install record should exist");
    let error = system_skill_is_installed(&application.skills_root(), &manager, &spec)
        .expect_err("directory-shaped install record should fail");
    assert!(
        error.contains("Install record is not a file"),
        "unexpected error: {error}"
    );
}
