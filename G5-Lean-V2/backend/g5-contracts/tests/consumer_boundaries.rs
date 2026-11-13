//! Real-workspace dependency checks for contracts-only consumers.

use std::collections::BTreeMap;
use std::process::Command;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Metadata {
    packages: Vec<Package>,
}

#[derive(Debug, Deserialize)]
struct Package {
    name: String,
    dependencies: Vec<Dependency>,
    features: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct Dependency {
    name: String,
    kind: Option<String>,
    #[serde(default)]
    features: Vec<String>,
    #[serde(default = "default_true")]
    uses_default_features: bool,
}

fn default_true() -> bool {
    true
}

fn normal_dependencies(package: &Package) -> Vec<&str> {
    package
        .dependencies
        .iter()
        .filter(|dependency| dependency.kind.as_deref() != Some("dev"))
        .map(|dependency| dependency.name.as_str())
        .collect()
}

fn internal_dependencies(package: &Package) -> Vec<&str> {
    normal_dependencies(package)
        .into_iter()
        .filter(|name| name.starts_with("g5-"))
        .collect()
}

fn rust_sources(root: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(root).expect("source directory must be readable") {
        let entry = entry.expect("source entry must be readable");
        let path = entry.path();
        if path.file_name().and_then(|value| value.to_str()) == Some("target") {
            continue;
        }
        if path.is_dir() {
            rust_sources(&path, files);
        } else if path.extension().and_then(|value| value.to_str()) == Some("rs") {
            files.push(path);
        }
    }
}

#[test]
fn contracts_only_consumers_depend_on_the_physical_owner() {
    let output = Command::new(env!("CARGO"))
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .expect("g5-contracts should be inside the generation workspace"),
        )
        .output()
        .expect("cargo metadata must run");
    assert!(
        output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: Metadata =
        serde_json::from_slice(&output.stdout).expect("cargo metadata must be valid JSON");

    for package_name in ["g5-authz", "g5d-isolation"] {
        let package = metadata
            .packages
            .iter()
            .find(|package| package.name == package_name)
            .unwrap_or_else(|| panic!("{package_name} must be a workspace package"));
        let normal_dependencies = normal_dependencies(package);
        assert!(
            normal_dependencies.contains(&"g5-contracts"),
            "{package_name} must depend directly on g5-contracts"
        );
        assert!(
            !normal_dependencies.contains(&"g5-common"),
            "{package_name} must not depend directly on the g5-common facade"
        );
    }
}

#[test]
fn auth_consumers_depend_on_the_physical_owner() {
    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("g5-contracts should be inside the generation workspace");
    let output = Command::new(env!("CARGO"))
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(workspace_root)
        .output()
        .expect("cargo metadata must run");
    assert!(
        output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: Metadata =
        serde_json::from_slice(&output.stdout).expect("cargo metadata must be valid JSON");

    let auth = metadata
        .packages
        .iter()
        .find(|package| package.name == "g5-auth")
        .expect("g5-auth must be a workspace package");
    assert_eq!(internal_dependencies(auth), ["g5-contracts"]);

    for package_name in [
        "g5-api",
        "g5-api-core",
        "g5-api-routes",
        "g5d",
        "g5d-foundation",
    ] {
        let package = metadata
            .packages
            .iter()
            .find(|package| package.name == package_name)
            .unwrap_or_else(|| panic!("{package_name} must be a workspace package"));
        assert!(
            normal_dependencies(package).contains(&"g5-auth"),
            "{package_name} must depend directly on g5-auth"
        );
    }

    let mut sources = Vec::new();
    rust_sources(workspace_root, &mut sources);
    for path in sources {
        let source = std::fs::read_to_string(&path).expect("Rust source must be readable");
        assert!(
            !source.contains(concat!("g5_common", "::auth")),
            "production source must import g5-auth directly: {}",
            path.display()
        );
    }
}

#[test]
fn storage_consumers_depend_on_the_physical_owner() {
    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("g5-contracts should be inside the generation workspace");
    let output = Command::new(env!("CARGO"))
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(workspace_root)
        .output()
        .expect("cargo metadata must run");
    assert!(
        output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: Metadata =
        serde_json::from_slice(&output.stdout).expect("cargo metadata must be valid JSON");

    let storage = metadata
        .packages
        .iter()
        .find(|package| package.name == "g5-storage")
        .expect("g5-storage must be a workspace package");
    assert!(
        internal_dependencies(storage).is_empty(),
        "g5-storage must not carry an unused internal G5 dependency"
    );

    // `g5-api-data` stopped consuming storage when the capability-gated blob
    // service was removed with the `/api/v1/blobs` routes (G5Labs/G5-Lean#457).
    for package_name in [
        "g5-api",
        "g5-api-core",
        "g5-api-foundation",
        "g5-api-routes",
        "g5d",
    ] {
        let package = metadata
            .packages
            .iter()
            .find(|package| package.name == package_name)
            .unwrap_or_else(|| panic!("{package_name} must be a workspace package"));
        assert!(
            normal_dependencies(package).contains(&"g5-storage"),
            "{package_name} must depend directly on g5-storage"
        );
    }

    let mut sources = Vec::new();
    rust_sources(workspace_root, &mut sources);
    for path in sources {
        let source = std::fs::read_to_string(&path).expect("Rust source must be readable");
        assert!(
            !source.contains(concat!("g5_common", "::storage")),
            "production source must import g5-storage directly: {}",
            path.display()
        );
    }
}

#[test]
fn semantic_consumers_depend_on_the_physical_owner() {
    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("g5-contracts should be inside the generation workspace");
    let output = Command::new(env!("CARGO"))
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(workspace_root)
        .output()
        .expect("cargo metadata must run");
    assert!(
        output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: Metadata =
        serde_json::from_slice(&output.stdout).expect("cargo metadata must be valid JSON");

    let semantic = metadata
        .packages
        .iter()
        .find(|package| package.name == "g5-semantic")
        .expect("g5-semantic must be a workspace package");
    assert_eq!(
        internal_dependencies(semantic),
        vec!["g5-contracts"],
        "g5-semantic must depend internally only on g5-contracts"
    );

    for package_name in ["g5-api", "g5-api-core", "g5-api-routes", "g5d"] {
        let package = metadata
            .packages
            .iter()
            .find(|package| package.name == package_name)
            .unwrap_or_else(|| panic!("{package_name} must be a workspace package"));
        assert!(
            normal_dependencies(package).contains(&"g5-semantic"),
            "{package_name} must depend directly on g5-semantic"
        );
    }

    let mut sources = Vec::new();
    rust_sources(workspace_root, &mut sources);
    for path in sources {
        let source = std::fs::read_to_string(&path).expect("Rust source must be readable");
        for legacy_module in ["parsing", "block_extraction", "local_grounding"] {
            assert!(
                !source.contains(&format!("g5_common::{legacy_module}")),
                "production source must import g5-semantic directly: {}",
                path.display()
            );
        }
    }
}

#[test]
fn scm_consumers_depend_on_the_physical_owner() {
    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("g5-contracts should be inside the generation workspace");
    let output = Command::new(env!("CARGO"))
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(workspace_root)
        .output()
        .expect("cargo metadata must run");
    assert!(
        output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: Metadata =
        serde_json::from_slice(&output.stdout).expect("cargo metadata must be valid JSON");

    let scm = metadata
        .packages
        .iter()
        .find(|package| package.name == "g5-scm")
        .expect("g5-scm must be a workspace package");
    assert_eq!(
        internal_dependencies(scm),
        vec!["g5-contracts"],
        "g5-scm must depend internally only on g5-contracts"
    );
    assert!(
        scm.features
            .get("test-helpers")
            .expect("g5-scm test-helpers feature must remain declared")
            .iter()
            .all(|feature| !feature.contains("g5-semantic")),
        "g5-scm test helpers must not create a reverse dependency on g5-semantic"
    );

    let mut sources = Vec::new();
    rust_sources(workspace_root, &mut sources);
    for path in sources {
        let source = std::fs::read_to_string(&path).expect("Rust source must be readable");
        assert!(
            !source.contains(&["g5_common", "::git"].concat()),
            "production source must import g5-scm directly: {}",
            path.display()
        );
    }
}

/// `g5_scm::workspace` (formerly the `g5-workspace-ops` crate) is reached
/// through a direct dependency on its owner, `g5-scm`.
#[test]
fn workspace_module_consumers_depend_on_scm() {
    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("g5-contracts should be inside the generation workspace");
    let output = Command::new(env!("CARGO"))
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(workspace_root)
        .output()
        .expect("cargo metadata must run");
    assert!(
        output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: Metadata =
        serde_json::from_slice(&output.stdout).expect("cargo metadata must be valid JSON");

    // `g5d` owns the CLI (`g5d::cli`), the only runtime-side production
    // consumer of SCM workspace operations.
    for package_name in ["g5-api-core", "g5-api-routes", "g5d", "g5d-foundation"] {
        let package = metadata
            .packages
            .iter()
            .find(|package| package.name == package_name)
            .unwrap_or_else(|| panic!("{package_name} must be a workspace package"));
        assert!(
            normal_dependencies(package).contains(&"g5-scm"),
            "{package_name} must depend directly on g5-scm"
        );
    }

    // Turn-owned memory, ontology changes cards and confined formal review
    // tools use native Git:
    // no libgit2 default or provider transports. Provider runtime seams never
    // name SCM.
    let agent = metadata
        .packages
        .iter()
        .find(|p| p.name == "g5d-agent")
        .expect("g5d-agent must be a workspace package");
    let scm = agent
        .dependencies
        .iter()
        .find(|dependency| dependency.name == "g5-scm")
        .expect("g5d-agent must depend directly on g5-scm");
    assert!(
        !scm.uses_default_features && scm.features == ["native"],
        "g5d-agent must link g5-scm with only the native feature, got {:?} (defaults: {})",
        scm.features,
        scm.uses_default_features
    );
    let agent_src = workspace_root.join("g5d-agent/src");
    let seams = provider_side_sources(&agent_src);
    for seam in [
        "runner/claude.rs",
        "runner/codex.rs",
        "provider_transport.rs",
    ] {
        assert!(
            seams.contains(&agent_src.join(seam)),
            "provider-side source discovery must cover {seam}: {seams:?}"
        );
    }
    if let Some(offender) = provider_side_scm_use(&agent_src) {
        panic!(
            "the provider runtime seam {} must not reach SCM",
            offender.display()
        );
    }

    let mut sources = Vec::new();
    rust_sources(workspace_root, &mut sources);
    for path in sources {
        let source = std::fs::read_to_string(&path).expect("Rust source must be readable");
        assert!(
            !source.contains(&["g5_workspace", "_ops"].concat()),
            "workspace operations now live in g5_scm::workspace: {}",
            path.display()
        );
    }
}

/// Every source file of `g5d-agent` outside the agent-turn modules (which
/// legitimately compose SCM): the provider runtime seams, discovered by
/// walking the tree so a new runner module is covered automatically.
fn provider_side_sources(agent_src: &std::path::Path) -> Vec<std::path::PathBuf> {
    const TURN_MODULES: [&str; 10] = [
        "lib.rs",
        "codex_auth.rs",
        "formal_review.rs",
        "harness.rs",
        "memory.rs",
        // The turn's ontology start and changes card (`g5.chat.ontology-card`).
        "ontology_turn.rs",
        "policy.rs",
        "protocol.rs",
        "runtime.rs",
        "types.rs",
    ];
    let mut sources = Vec::new();
    rust_sources(agent_src, &mut sources);
    sources
        .into_iter()
        .filter(|path| {
            let relative = path
                .strip_prefix(agent_src)
                .expect("source under agent src");
            !TURN_MODULES.contains(&relative.to_string_lossy().as_ref())
        })
        .collect()
}

/// The first provider-side source that names `g5_scm`, if any.
fn provider_side_scm_use(agent_src: &std::path::Path) -> Option<std::path::PathBuf> {
    provider_side_sources(agent_src).into_iter().find(|path| {
        std::fs::read_to_string(path)
            .expect("Rust source must be readable")
            .contains("g5_scm")
    })
}

#[test]
fn provider_side_scm_guard_covers_newly_added_runner_modules() {
    let root = tempfile::tempdir().expect("temp dir");
    let src = root.path().join("src");
    std::fs::create_dir_all(src.join("runner")).expect("runner dir");
    for (name, body) in [
        ("lib.rs", "pub mod runner;\nmod runtime;\n"),
        ("runtime.rs", "use g5_scm::native;\n"),
        (
            "formal_review.rs",
            "use g5_scm::native::ReviewSnapshotRead;\n",
        ),
        ("runner/mod.rs", "pub mod claude;\npub mod scm_reader;\n"),
        ("runner/claude.rs", "pub fn project_key() {}\n"),
    ] {
        std::fs::write(src.join(name), body).expect("fixture file");
    }
    assert_eq!(provider_side_scm_use(&src), None);
    std::fs::write(
        src.join("runner/scm_reader.rs"),
        "use g5_scm::GitRepo;\npub fn read() {}\n",
    )
    .expect("fixture file");
    assert_eq!(
        provider_side_scm_use(&src).as_deref(),
        Some(src.join("runner/scm_reader.rs").as_path()),
        "a new runner module that reaches SCM must be caught"
    );
}

#[test]
fn internal_api_consumers_depend_on_the_transport_owner() {
    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("g5-contracts should be inside the generation workspace");
    let output = Command::new(env!("CARGO"))
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(workspace_root)
        .output()
        .expect("cargo metadata must run");
    assert!(
        output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: Metadata =
        serde_json::from_slice(&output.stdout).expect("cargo metadata must be valid JSON");

    let internal_api = metadata
        .packages
        .iter()
        .find(|package| package.name == "g5-internal-api")
        .expect("g5-internal-api must be a workspace package");
    assert_eq!(
        internal_dependencies(internal_api),
        vec!["g5-contracts"],
        "g5-internal-api must depend internally only on g5-contracts"
    );
    assert!(
        internal_api
            .dependencies
            .iter()
            .all(|dependency| dependency.name != "g5-auth"),
        "g5-internal-api must not declare an unused auth edge in any dependency class"
    );

    for package_name in [
        "g5-api-core",
        "g5-api-data",
        "g5-api-routes",
        "g5d",
        "g5d-foundation",
        "g5d-host",
        "g5d-agent",
    ] {
        let package = metadata
            .packages
            .iter()
            .find(|package| package.name == package_name)
            .unwrap_or_else(|| panic!("{package_name} must be a workspace package"));
        assert!(
            normal_dependencies(package).contains(&"g5-internal-api"),
            "{package_name} must depend directly on g5-internal-api"
        );

        let source_root = workspace_root.join(package_name).join("src");
        let mut sources = Vec::new();
        rust_sources(&source_root, &mut sources);
        assert!(
            !sources.is_empty(),
            "{package_name} source scan must be non-vacuous"
        );
        for path in sources {
            let source = std::fs::read_to_string(&path).expect("Rust source must be readable");
            assert!(
                !source.contains(&["g5_common", "::internal_api"].concat()),
                "production source must import g5-internal-api directly: {}",
                path.display()
            );
        }
    }
}

#[test]
fn production_packages_do_not_depend_on_the_common_facade() {
    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("g5-contracts should be inside the generation workspace");
    let output = Command::new(env!("CARGO"))
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(workspace_root)
        .output()
        .expect("cargo metadata must run");
    assert!(
        output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: Metadata =
        serde_json::from_slice(&output.stdout).expect("cargo metadata must be valid JSON");

    for package in metadata
        .packages
        .iter()
        .filter(|package| package.name.starts_with("g5-") || package.name.starts_with("g5d"))
    {
        assert!(
            !normal_dependencies(package).contains(&"g5-common"),
            "{} must not depend on the g5-common facade",
            package.name
        );

        let source_root = workspace_root.join(&package.name).join("src");
        if !source_root.is_dir() {
            continue;
        }
        let mut sources = Vec::new();
        rust_sources(&source_root, &mut sources);
        assert!(
            !sources.is_empty(),
            "{} source scan must be non-vacuous",
            package.name
        );
        for path in sources {
            let source = std::fs::read_to_string(&path).expect("Rust source must be readable");
            let executable_source = source
                .lines()
                .filter(|line| !line.trim_start().starts_with("//"))
                .collect::<Vec<_>>()
                .join("\n");
            assert!(
                !executable_source.contains(&["g5_common", "::"].concat()),
                "{} imports the compatibility facade: {}",
                package.name,
                path.display()
            );
        }
    }
}
