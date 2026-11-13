// Executable source-only package-boundary contracts.

use std::collections::BTreeSet;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

// Runs as the g5d `crate_architecture` integration target.
fn g5d_manifest_dir() -> &'static Path {
    static DIRECTORY: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    DIRECTORY.get_or_init(|| {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .find(|root| root.join("backend/g5d/Cargo.toml").is_file())
            .expect("architecture tests must live inside the G5 repository")
            .join("backend/g5d")
    })
}

#[derive(Debug, Deserialize)]
struct Metadata {
    packages: Vec<Package>,
    workspace_members: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Package {
    id: String,
    name: String,
    dependencies: Vec<Dependency>,
    #[serde(default)]
    targets: Vec<Target>,
}

#[derive(Debug, Deserialize)]
struct Target {
    name: String,
    kind: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Dependency {
    name: String,
    kind: Option<String>,
}

const SHARED_CAPABILITY_PACKAGES: &[(&str, &[&str])] = &[
    ("g5-contracts", &[]),
    ("g5-auth", &["g5-contracts"]),
    ("g5-storage", &[]),
    ("g5-semantic", &["g5-contracts"]),
    ("g5-scm", &["g5-contracts"]),
    ("g5-internal-api", &["g5-contracts"]),
];

fn validate_shared_capability_dependencies(
    package_name: &str,
    dependencies: &[Dependency],
) -> Result<(), String> {
    let allowed_internal = SHARED_CAPABILITY_PACKAGES
        .iter()
        .find_map(|(candidate, allowed)| (*candidate == package_name).then_some(*allowed))
        .ok_or_else(|| format!("unknown shared capability package `{package_name}`"))?;

    for dependency in dependencies
        .iter()
        .filter(|dependency| dependency.kind.as_deref() != Some("dev"))
    {
        let is_internal = dependency.name == "g5d"
            || dependency.name.starts_with("g5d-")
            || dependency.name.starts_with("g5-");
        if is_internal && !allowed_internal.contains(&dependency.name.as_str()) {
            return Err(format!(
                "{package_name} must not depend on internal package `{}`",
                dependency.name
            ));
        }
    }

    Ok(())
}

fn validate_production_package_avoids_common_facade(
    package_name: &str,
    dependencies: &[Dependency],
) -> Result<(), String> {
    if dependencies.iter().any(|dependency| {
        dependency.kind.as_deref() != Some("dev") && dependency.name == "g5-common"
    }) {
        return Err(format!(
            "production package `{package_name}` must depend on physical shared capabilities, not `g5-common`"
        ));
    }
    Ok(())
}

fn load_workspace_metadata(workspace_manifest: &Path) -> Metadata {
    let output = Command::new(env!("CARGO"))
        .args([
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
            "--offline",
            "--locked",
            "--manifest-path",
        ])
        .arg(workspace_manifest)
        .output()
        .expect("cargo metadata should start");

    assert!(
        output.status.success(),
        "cargo metadata failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    serde_json::from_slice(&output.stdout).expect("cargo metadata should be valid JSON")
}

fn validate_foundation_dependencies(dependencies: &[Dependency]) -> Result<(), String> {
    for dependency in dependencies
        .iter()
        .filter(|dependency| dependency.kind.as_deref() != Some("dev"))
    {
        let is_forbidden = dependency.name == "g5d"
            || dependency.name.starts_with("g5-api")
            || (dependency.name.starts_with("g5d-") && dependency.name != "g5d-foundation");

        if is_forbidden {
            return Err(format!(
                "g5d-foundation must not depend on downstream package `{}`",
                dependency.name
            ));
        }
    }

    Ok(())
}

fn validate_internal_dependencies(
    package_name: &str,
    dependencies: &[Dependency],
    allowed_internal: &[&str],
) -> Result<(), String> {
    for dependency in dependencies
        .iter()
        .filter(|dependency| dependency.kind.as_deref() != Some("dev"))
    {
        let is_internal = dependency.name == "g5d"
            || dependency.name == "g5-api"
            || dependency.name.starts_with("g5-api-")
            || dependency.name.starts_with("g5d-");
        if is_internal && !allowed_internal.contains(&dependency.name.as_str()) {
            return Err(format!(
                "{package_name} must not depend on internal package `{}`",
                dependency.name
            ));
        }
    }
    Ok(())
}

fn without_legacy_tracing_target_mappings(source: &str) -> String {
    const TABLE: &str = "static LEGACY_TRACING_TARGET_MAPPINGS";
    let Some(start) = source.find(TABLE) else {
        return source.to_string();
    };
    let Some(relative_end) = source[start..].find("\n];") else {
        return source.to_string();
    };
    let end = start + relative_end + "\n];".len();
    format!("{}{}", &source[..start], &source[end..])
}

fn collect_rust_files(root: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(root)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", root.display()))
    {
        let path = entry.expect("directory entry should be readable").path();
        if path.is_dir() {
            collect_rust_files(&path, files);
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
            files.push(path);
        }
    }
}

fn package_flag_count(source: &str, package_name: &str) -> usize {
    source
        .split_whitespace()
        .collect::<Vec<_>>()
        .windows(2)
        .filter(|pair| pair[0] == "-p" && pair[1].trim_end_matches('\\') == package_name)
        .count()
}

/// Cloud Build files whose `build-ci-image` step derives the CI builder image
/// tag from a hash of the files that affect the image.
const CI_BUILDER_HASH_FILES: [&str; 2] = [
    "../infra/ci/cloudbuild-ci-rust.yaml",
    "../infra/ci/cloudbuild-ci-e2e.yaml",
];

/// The `find` that feeds crate manifests and build scripts into a
/// `build-ci-image` step's `HASH=$$( ... )` computation.
#[derive(Debug)]
struct BuilderHashManifestScan {
    roots: Vec<String>,
    max_depth: Option<usize>,
    names: Vec<String>,
}

/// Extract the manifest `find` from the `build-ci-image` step of a Cloud Build
/// file. The step must run from the repository root (`dir: .`) because the
/// search roots are repository-relative.
fn builder_hash_manifest_scan(yaml: &str) -> Result<BuilderHashManifestScan, String> {
    let lines: Vec<&str> = yaml.lines().collect();
    let step_start = lines
        .iter()
        .position(|line| line.trim() == "- id: build-ci-image")
        .ok_or("no build-ci-image step")?;
    let step_end = lines[step_start + 1..]
        .iter()
        .position(|line| line.trim_start().starts_with("- id:"))
        .map_or(lines.len(), |offset| step_start + 1 + offset);
    let step = &lines[step_start..step_end];
    if !step.iter().any(|line| line.trim() == "dir: .") {
        return Err(
            "build-ci-image must run from the repository root (dir: .) because its manifest \
             search roots are repository-relative"
                .to_owned(),
        );
    }
    let hash_start = step
        .iter()
        .position(|line| line.contains("HASH=$$("))
        .ok_or("build-ci-image computes no HASH=$$( ... )")?;
    let hash_end = step[hash_start + 1..]
        .iter()
        .position(|line| line.trim() == ")")
        .map(|offset| hash_start + 1 + offset)
        .ok_or("build-ci-image has an unterminated HASH=$$( block")?;

    // Join shell line continuations so each entry is one complete command.
    let mut commands = Vec::new();
    let mut current = String::new();
    for line in &step[hash_start + 1..hash_end] {
        let trimmed = line.trim();
        if let Some(head) = trimmed.strip_suffix('\\') {
            current.push_str(head);
            current.push(' ');
        } else {
            current.push_str(trimmed);
            commands.push(std::mem::take(&mut current));
        }
    }
    let find = commands
        .iter()
        .map(|command| command.split('|').next().unwrap_or_default())
        .find(|command| command.starts_with("find ") && command.contains("Cargo.toml"))
        .ok_or("the builder hash has no find over crate manifests")?;
    let tokens: Vec<&str> = find.split_whitespace().collect();
    let roots = tokens[1..]
        .iter()
        .take_while(|token| !token.starts_with(['-', '\\', '(']))
        .map(|root| root.trim_end_matches('/').to_owned())
        .collect();
    let max_depth = tokens
        .windows(2)
        .find(|pair| pair[0] == "-maxdepth")
        .map(|pair| {
            pair[1]
                .parse::<usize>()
                .map_err(|error| format!("invalid -maxdepth {:?}: {error}", pair[1]))
        })
        .transpose()?;
    let names = tokens
        .windows(2)
        .filter(|pair| pair[0] == "-name")
        .map(|pair| pair[1].trim_matches(['\'', '"']).to_owned())
        .collect();
    Ok(BuilderHashManifestScan {
        roots,
        max_depth,
        names,
    })
}

/// Repository-relative manifests of `members` (crates under `backend/`) that
/// the scan cannot reach.
fn unreachable_member_manifests(scan: &BuilderHashManifestScan, members: &[&str]) -> Vec<String> {
    members
        .iter()
        .map(|member| format!("backend/{member}/Cargo.toml"))
        .filter(|manifest| {
            !scan.roots.iter().any(|root| {
                manifest
                    .strip_prefix(root.as_str())
                    .and_then(|rest| rest.strip_prefix('/'))
                    .is_some_and(|rest| {
                        scan.max_depth
                            .is_none_or(|depth| rest.split('/').count() <= depth)
                    })
            })
        })
        .collect()
}

/// Every CI builder hash must change when any of `members`' manifests or build
/// scripts change. Checks what the `find` in each hash step actually reaches,
/// not a hand-maintained crate list: the list the steps used before went
/// stale when the crates moved under `backend/`, and hashed no manifests.
fn assert_ci_builder_hashes_cover(members: &[&str], reason: &str) {
    let backend_root = g5d_manifest_dir()
        .parent()
        .expect("g5d should be inside the generation workspace");
    for member in members {
        let manifest = backend_root.join(member).join("Cargo.toml");
        assert!(
            manifest.is_file(),
            "{member} must still be a crate at {}",
            manifest.display()
        );
    }
    for file_name in CI_BUILDER_HASH_FILES {
        let contents = std::fs::read_to_string(backend_root.join(file_name))
            .unwrap_or_else(|error| panic!("failed to read {file_name}: {error}"));
        let scan = builder_hash_manifest_scan(&contents)
            .unwrap_or_else(|error| panic!("{file_name}: {error}"));
        for name in ["Cargo.toml", "build.rs"] {
            assert!(
                scan.names.iter().any(|scanned| scanned == name),
                "{file_name} must hash every {name} {reason}; its manifest search is {scan:?}"
            );
        }
        let unreachable = unreachable_member_manifests(&scan, members);
        assert!(
            unreachable.is_empty(),
            "{file_name} must invalidate its builder hash {reason}; its manifest search \
             {scan:?} misses {unreachable:?}"
        );
    }
}

#[test]
fn real_workspace_has_one_way_foundation_edge() {
    let workspace_manifest = g5d_manifest_dir()
        .parent()
        .expect("g5d should be inside the generation workspace")
        .join("../Cargo.toml");
    let metadata = load_workspace_metadata(&workspace_manifest);

    let foundation = metadata
        .packages
        .iter()
        .find(|package| package.name == "g5d-foundation")
        .expect("g5d-foundation must be a workspace package");
    assert!(
        metadata.workspace_members.contains(&foundation.id),
        "g5d-foundation must be a workspace member"
    );

    let g5d = metadata
        .packages
        .iter()
        .find(|package| package.name == "g5d")
        .expect("g5d must remain a workspace package");
    assert!(
        g5d.dependencies.iter().any(|dependency| {
            dependency.name == "g5d-foundation" && dependency.kind.as_deref() != Some("dev")
        }),
        "g5d must depend on g5d-foundation through a production edge"
    );

    validate_foundation_dependencies(&foundation.dependencies)
        .expect("the foundation dependency direction must remain one-way");
}

#[test]
fn reverse_foundation_edge_is_rejected() {
    let error = validate_foundation_dependencies(&[Dependency {
        name: "g5d".to_owned(),
        kind: None,
    }])
    .expect_err("a reverse edge to g5d must fail");

    assert!(error.contains("`g5d`"), "unexpected error: {error}");
}

#[test]
fn production_edge_to_api_implementation_is_rejected() {
    for name in ["g5-api-core", "g5-api-data", "g5-api-foundation", "g5-api-routes"] {
        let error = validate_foundation_dependencies(&[Dependency {
            name: name.to_owned(),
            kind: None,
        }])
        .expect_err("a production edge from the runtime foundation to an API crate must fail");
        assert!(error.contains(name), "unexpected error: {error}");
    }
}

#[test]
fn production_edge_to_legacy_react_is_rejected() {
    let error = validate_foundation_dependencies(&[Dependency {
        name: "g5d-legacy-react".to_owned(),
        kind: None,
    }])
    .expect_err("a production edge to legacy ReAct must fail");

    assert!(
        error.contains("`g5d-legacy-react`"),
        "unexpected error: {error}"
    );
}

#[test]
fn shared_capability_dependency_fixtures_reject_reverse_and_sibling_edges() {
    for (package_name, forbidden_dependency) in [
        ("g5-contracts", "g5-auth"),
        ("g5-auth", "g5-semantic"),
        ("g5-scm", "g5-semantic"),
        ("g5-internal-api", "g5d-agent"),
    ] {
        let error = validate_shared_capability_dependencies(
            package_name,
            &[Dependency {
                name: forbidden_dependency.to_owned(),
                kind: None,
            }],
        )
        .expect_err("a forbidden shared-capability edge must fail");
        assert!(
            error.contains(forbidden_dependency),
            "unexpected dependency error: {error}"
        );
    }

    let error = validate_production_package_avoids_common_facade(
        "g5-api-foundation",
        &[Dependency {
            name: "g5-common".to_owned(),
            kind: None,
        }],
    )
    .expect_err("a new production facade dependency must fail");
    assert!(error.contains("g5-api-foundation"));
}

#[test]
fn real_workspace_admits_shared_capability_owners() {
    let workspace_root = g5d_manifest_dir()
        .parent()
        .expect("g5d should be inside the generation workspace");
    let metadata = load_workspace_metadata(&workspace_root.join("../Cargo.toml"));

    let contracts = metadata
        .packages
        .iter()
        .find(|package| package.name == "g5-contracts")
        .expect("g5-contracts must be a workspace package");
    assert!(
        metadata.workspace_members.contains(&contracts.id),
        "g5-contracts must be a workspace member"
    );
    validate_shared_capability_dependencies("g5-contracts", &contracts.dependencies)
        .expect("g5-contracts must have no internal G5 dependency");

    let auth = metadata
        .packages
        .iter()
        .find(|package| package.name == "g5-auth")
        .expect("g5-auth must be a workspace package");
    assert!(
        metadata.workspace_members.contains(&auth.id),
        "g5-auth must be a workspace member"
    );
    validate_shared_capability_dependencies("g5-auth", &auth.dependencies)
        .expect("g5-auth must depend internally only on g5-contracts");

    let storage = metadata
        .packages
        .iter()
        .find(|package| package.name == "g5-storage")
        .expect("g5-storage must be a workspace package");
    assert!(
        metadata.workspace_members.contains(&storage.id),
        "g5-storage must be a workspace member"
    );
    validate_shared_capability_dependencies("g5-storage", &storage.dependencies)
        .expect("g5-storage must have no internal G5 dependency");

    let semantic = metadata
        .packages
        .iter()
        .find(|package| package.name == "g5-semantic")
        .expect("g5-semantic must be a workspace package");
    assert!(
        metadata.workspace_members.contains(&semantic.id),
        "g5-semantic must be a workspace member"
    );
    validate_shared_capability_dependencies("g5-semantic", &semantic.dependencies)
        .expect("g5-semantic must depend internally only on g5-contracts");

    let scm = metadata
        .packages
        .iter()
        .find(|package| package.name == "g5-scm")
        .expect("g5-scm must be a workspace package");
    assert!(
        metadata.workspace_members.contains(&scm.id),
        "g5-scm must be a workspace member"
    );
    validate_shared_capability_dependencies("g5-scm", &scm.dependencies)
        .expect("g5-scm must depend internally only on g5-contracts");

    let internal_api = metadata
        .packages
        .iter()
        .find(|package| package.name == "g5-internal-api")
        .expect("g5-internal-api must be a workspace package");
    assert!(
        metadata.workspace_members.contains(&internal_api.id),
        "g5-internal-api must be a workspace member"
    );
    validate_shared_capability_dependencies("g5-internal-api", &internal_api.dependencies)
        .expect("g5-internal-api must depend internally only on g5-contracts");

    for package_name in [
        "g5-contracts",
        "g5-auth",
        "g5-storage",
        "g5-semantic",
        "g5-scm",
        "g5-internal-api",
    ] {
        let source_root = workspace_root.join(package_name).join("src");
        assert!(
            source_root.is_dir(),
            "{package_name} must have a physical source root"
        );
        let mut source_files = Vec::new();
        collect_rust_files(&source_root, &mut source_files);
        assert!(
            !source_files.is_empty(),
            "{package_name} source scans must be non-vacuous"
        );
    }

    let docker_dir = workspace_root.join("../infra/docker");
    for file_name in [
        "Dockerfile.g5d",
        "Dockerfile.g5-api",
        "Dockerfile.test-runner",
        "Dockerfile.ci-builder",
    ] {
        let contents = std::fs::read_to_string(docker_dir.join(file_name))
            .unwrap_or_else(|error| panic!("failed to read {file_name}: {error}"));
        for package_name in [
            "g5-contracts",
            "g5-auth",
            "g5-storage",
            "g5-semantic",
            "g5-scm",
            "g5-internal-api",
        ] {
            assert!(
                contents.contains(&format!(
                    "COPY backend/{package_name}/Cargo.toml backend/{package_name}/Cargo.toml"
                )),
                "{file_name} must copy the {package_name} manifest"
            );
            assert!(
                contents.contains(&format!("{package_name}/src")),
                "{file_name} must create a dummy {package_name} source root"
            );
        }
    }
    for file_name in [
        "Dockerfile.g5d",
        "Dockerfile.g5-api",
        "Dockerfile.test-runner",
    ] {
        let contents = std::fs::read_to_string(docker_dir.join(file_name))
            .unwrap_or_else(|error| panic!("failed to read {file_name}: {error}"));
        for package_name in [
            "g5-contracts",
            "g5-auth",
            "g5-storage",
            "g5-semantic",
            "g5-scm",
            "g5-internal-api",
        ] {
            assert!(
                contents.contains(&format!(
                    "COPY backend/{package_name}/ backend/{package_name}/"
                )),
                "{file_name} must copy the real {package_name} crate"
            );
        }
    }

    for file_name in [
        "../infra/ci/cloudbuild-ci-rust.yaml",
        "../infra/ci/cloudbuild-ci-e2e.yaml",
        "../scripts/test-parallel.sh",
        "../scripts/clean-baked-rust-workspace-artifacts.sh",
    ] {
        let contents = std::fs::read_to_string(workspace_root.join(file_name))
            .unwrap_or_else(|error| panic!("failed to read {file_name}: {error}"));
        for package_name in [
            "g5-contracts",
            "g5-auth",
            "g5-storage",
            "g5-semantic",
            "g5-scm",
            "g5-internal-api",
        ] {
            assert!(
                contents.contains(package_name),
                "{file_name} must include the physical {package_name} owner"
            );
        }
    }
}


#[test]
fn legacy_react_is_outer_only_and_remaining_callers_are_explicit() {
    let workspace_root = g5d_manifest_dir()
        .parent()
        .expect("g5d should be inside the generation workspace");
    let metadata = load_workspace_metadata(&workspace_root.join("../Cargo.toml"));
    let extracted_packages = [
        "g5d-foundation",
        "g5d-isolation",
        "g5d-agent",
        "g5d-host",
        "g5d-dispatch",
    ];

    for package_name in extracted_packages {
        let package = metadata
            .packages
            .iter()
            .find(|package| package.name == package_name)
            .unwrap_or_else(|| panic!("{package_name} must remain a workspace package"));
        assert!(
            package.dependencies.iter().all(|dependency| {
                dependency.kind.as_deref() == Some("dev")
                    || (dependency.name != "g5d" && dependency.name != "g5d-legacy-react")
            }),
            "{package_name} must not depend on outer g5d or a legacy ReAct package"
        );
    }

    // The outer crate owns the CLI sources now, so it is scanned with the
    // extracted packages for retired per-task surfaces.
    for package_name in extracted_packages.into_iter().chain(["g5d"]) {
        let source_root = workspace_root.join(package_name).join("src");
        for retired_module in ["agent", "llm", "prompts"] {
            assert!(
                !source_root.join(retired_module).exists(),
                "{package_name} must not physically own retired per-task {retired_module}"
            );
        }
        let mut source_files = Vec::new();
        collect_rust_files(&source_root, &mut source_files);
        for source_file in source_files {
            let source = std::fs::read_to_string(&source_file).unwrap_or_else(|error| {
                panic!("failed to read {}: {error}", source_file.display())
            });
            let source = without_legacy_tracing_target_mappings(&source);
            for forbidden in [
                "g5d::agent",
                "g5d::llm",
                "g5d::prompts",
                "g5d_legacy_react",
            ] {
                assert!(
                    !source.contains(forbidden),
                    "{} imports retired per-task surface {forbidden}",
                    source_file.display()
                );
            }
        }
    }

    let outer_source_root = workspace_root.join("g5d/src");
    assert!(
        !outer_source_root.join("agent/mod.rs").exists()
            && !outer_source_root.join("llm").exists()
            && !outer_source_root.join("prompts").exists(),
        "retired per-task agent loop, inference, and custom-question modules must stay deleted"
    );
}

#[test]
fn retired_rust_skill_registry_and_contracts_stay_deleted() {
    let backend_root = g5d_manifest_dir()
        .parent()
        .expect("g5d should be inside the backend workspace");
    for retired_path in ["g5d/src/agent/skills", "g5-contracts/src/types/skill.rs"] {
        assert!(
            !backend_root.join(retired_path).exists(),
            "disconnected Rust skill implementation must stay deleted: {retired_path}"
        );
    }
    let types = std::fs::read_to_string(backend_root.join("g5-contracts/src/types/mod.rs"))
        .expect("shared type exports should remain readable");
    for retired_export in ["pub mod skill;", "pub use skill::*;"] {
        assert!(
            !types.contains(retired_export),
            "retired skill contracts must not be exported: {retired_export}"
        );
    }
}

#[test]
fn api_foundation_is_dependency_clean_and_outer_api_preserves_compatibility_paths() {
    let workspace_root = g5d_manifest_dir()
        .parent()
        .expect("g5d should be inside the generation workspace");
    let metadata = load_workspace_metadata(&workspace_root.join("../Cargo.toml"));
    let foundation = metadata
        .packages
        .iter()
        .find(|package| package.name == "g5-api-foundation")
        .expect("g5-api-foundation must be a workspace package");
    assert!(
        metadata.workspace_members.contains(&foundation.id),
        "g5-api-foundation must remain a workspace member"
    );
    validate_internal_dependencies(
        "g5-api-foundation",
        &foundation.dependencies,
        &[],
    )
    .expect("API foundation must remain below database, service, route, and outer API layers");
    for forbidden in ["sqlx", "g5-authz"] {
        assert!(
            foundation.dependencies.iter().all(|dependency| {
                dependency.kind.as_deref() == Some("dev") || dependency.name != forbidden
            }),
            "g5-api-foundation must not depend on `{forbidden}`"
        );
    }

    let outer_api = metadata
        .packages
        .iter()
        .find(|package| package.name == "g5-api")
        .expect("g5-api must remain a workspace package");
    assert!(
        outer_api.dependencies.iter().any(|dependency| {
            dependency.name == "g5-api-foundation" && dependency.kind.as_deref() != Some("dev")
        }),
        "outer g5-api must depend on g5-api-foundation through a production edge"
    );

    let foundation_root = workspace_root.join("g5-api-foundation/src");
    assert!(foundation_root.join("config.rs").is_file());
    assert!(foundation_root.join("error_catalog.rs").is_file());
    let core_lib = std::fs::read_to_string(workspace_root.join("g5-api-core/src/lib.rs"))
        .expect("core compatibility library should be readable");
    assert!(core_lib.contains("pub use g5_api_foundation::config;"));
    let outer_lib = std::fs::read_to_string(workspace_root.join("g5-api/src/lib.rs"))
        .expect("outer compatibility library should be readable");
    assert!(outer_lib.contains("pub use g5_api_core"));
    let catalog_compat =
        std::fs::read_to_string(workspace_root.join("g5-api-core/src/errors/catalog.rs"))
            .expect("core error catalog compatibility module should be readable");
    assert!(catalog_compat.contains("pub use g5_api_foundation::error_catalog::*;"));
}

#[test]
fn isolation_downstream_edge_is_rejected() {
    let error = validate_internal_dependencies(
        "g5d-isolation",
        &[Dependency {
            name: "g5d-agent".to_owned(),
            kind: None,
        }],
        &["g5d-foundation"],
    )
    .expect_err("isolation must reject a downstream implementation edge");
    assert!(error.contains("`g5d-agent`"), "unexpected error: {error}");
}

#[test]
fn agent_sibling_edge_is_rejected() {
    let error = validate_internal_dependencies(
        "g5d-agent",
        &[Dependency {
            name: "g5d-host".to_owned(),
            kind: None,
        }],
        &["g5d-foundation"],
    )
    .expect_err("agent must reject a concrete sibling edge");
    assert!(error.contains("`g5d-host`"), "unexpected error: {error}");
}

#[test]
fn host_upward_edge_is_rejected() {
    let error = validate_internal_dependencies(
        "g5d-host",
        &[Dependency {
            name: "g5d".to_owned(),
            kind: None,
        }],
        &["g5d-foundation", "g5d-isolation"],
    )
    .expect_err("host composition must reject an upward edge to outer g5d");
    assert!(error.contains("`g5d`"), "unexpected error: {error}");
}


#[test]
fn host_owns_dependency_clean_support_with_compatibility_exports() {
    let workspace_root = g5d_manifest_dir()
        .parent()
        .expect("g5d should be inside the generation workspace");
    let metadata = load_workspace_metadata(&workspace_root.join("../Cargo.toml"));
    let host = metadata
        .packages
        .iter()
        .find(|package| package.name == "g5d-host")
        .expect("g5d-host must be a workspace package");
    assert!(
        metadata.workspace_members.contains(&host.id),
        "g5d-host must remain a workspace member"
    );
    validate_internal_dependencies(
        "g5d-host",
        &host.dependencies,
        &["g5d-foundation", "g5d-isolation"],
    )
    .expect("host composition dependencies must remain one-way");

    let host_root = workspace_root.join("g5d-host");
    let host_lib = std::fs::read_to_string(host_root.join("src/lib.rs"))
        .expect("g5d-host library root should be readable");
    for module in ["ctl", "runtime", "tunnel", "workspace_vm"] {
        assert!(
            host_root.join(format!("src/{module}.rs")).is_file()
                && host_lib.contains(&format!("pub mod {module};")),
            "g5d-host must own and export {module}"
        );
    }
    for retired in ["lifecycle", "host_observer", "supervisor", "assignment", "agent_host"] {
        assert!(
            !host_root.join(format!("src/{retired}.rs")).exists(),
            "retired per-task host module {retired} must stay deleted"
        );
    }

    let facade = std::fs::read_to_string(workspace_root.join("g5d/src/host/mod.rs"))
        .expect("outer host compatibility facade should be readable");
    for module in ["ctl", "runtime", "tunnel", "workspace_vm"] {
        assert!(
            facade.contains(&format!("pub use g5d_host::{module};")),
            "outer host facade must re-export {module}"
        );
    }
}

/// The CLI is the `g5d::cli` module of the shipped crate. It composes the
/// extracted runtime crates one way (never `g5-api`), reaches no other outer
/// `g5d` module, and the binary is a process shell around it.
#[test]
fn cli_module_depends_only_inward_and_g5d_is_the_only_shipped_entrypoint() {
    let workspace_root = g5d_manifest_dir()
        .parent()
        .expect("g5d should be inside the generation workspace");
    let metadata = load_workspace_metadata(&workspace_root.join("../Cargo.toml"));
    assert!(
        metadata.packages.iter().all(|package| package.name != "g5d-cli"),
        "the CLI is folded into g5d; a separate g5d-cli package must not return"
    );
    let g5d = metadata
        .packages
        .iter()
        .find(|package| package.name == "g5d")
        .expect("g5d must remain the shipped workspace package");
    validate_internal_dependencies(
        "g5d",
        &g5d.dependencies,
        &[
            "g5d-agent",
            "g5d-foundation",
            "g5d-isolation",
            "g5d-host",
            "g5d-dispatch",
        ],
    )
    .expect("g5d dependencies must remain one-way and never point to g5-api");

    let cli_root = workspace_root.join("g5d/src/cli");
    for owned in [
        "mod.rs",
        "config.rs",
        "process.rs",
        "dispatch.rs",
        "host.rs",
        "git/mod.rs",
    ] {
        assert!(
            cli_root.join(owned).is_file(),
            "g5d::cli must physically own {owned}"
        );
    }
    for retired in ["host_up.rs", "task.rs", "worker.rs", "ask.rs", "agent_runner.rs"] {
        assert!(
            !cli_root.join(retired).exists(),
            "retired CLI execution entrypoint {retired} must stay deleted"
        );
    }
    // Inside the crate the CLI only names itself: every `crate::` path stays
    // under `crate::cli` and no `super::` chain climbs above it, so the host
    // facade and test harness never feed back into the command layer.
    let mut cli_sources = Vec::new();
    collect_rust_files(&cli_root, &mut cli_sources);
    assert!(cli_sources.len() > 20, "CLI source scan must be non-vacuous");
    for source_file in cli_sources {
        let source = std::fs::read_to_string(&source_file).unwrap_or_else(|error| {
            panic!("failed to read {}: {error}", source_file.display())
        });
        let relative = source_file
            .strip_prefix(&cli_root)
            .expect("CLI sources live under the cli root");
        if let Some(escape) = cli_source_escape(&source, cli_module_depth(relative)) {
            panic!(
                "{} reaches outside g5d::cli: {escape}",
                source_file.display()
            );
        }
    }

    let lib = std::fs::read_to_string(workspace_root.join("g5d/src/lib.rs"))
        .expect("g5d library root should be readable");
    for export in ["pub mod cli;", "pub use cli::config;", "pub use cli::git;"] {
        assert!(
            lib.contains(export),
            "g5d must keep the `{export}` compatibility path"
        );
    }
    let main = std::fs::read_to_string(workspace_root.join("g5d/src/main.rs"))
        .expect("g5d entrypoint should be readable");
    assert!(
        main.contains("g5d::cli::run_process_entrypoint(g5d::run)")
            && !main.contains("match &cli.command")
            && !main.contains("tokio::main"),
        "the shipped g5d entrypoint must contain only process-shell wiring"
    );
}

/// Depth of a `g5d/src/cli` source file's module below `cli`: `mod.rs` is
/// the module of its directory, any other file is a module of its own.
fn cli_module_depth(relative: &Path) -> usize {
    let components = relative.components().count();
    if relative.file_name().is_some_and(|name| name == "mod.rs") {
        components - 1
    } else {
        components
    }
}

/// The first path in `source` that leaves `g5d::cli`, or `None`.
///
/// The file is parsed as Rust and every path is resolved from its first
/// segments: a `crate::` path must continue with `cli`; a `super` chain may
/// climb at most `depth` levels (the file's module depth below `cli`) plus the
/// inline `mod` blocks it sits in; `use` groups and renames are expanded, and
/// aliasing `crate`, `self`-rooted ancestors or a `super` chain is rejected
/// outright. Macro invocation tokens, which the parser leaves opaque, and any
/// file the parser rejects fall back to a whitespace-normalised text scan.
fn cli_source_escape(source: &str, depth: usize) -> Option<String> {
    let parsed = match syn::parse_file(source) {
        Ok(file) => {
            let mut visitor = CliEscapeVisitor {
                depth,
                inline: 0,
                found: None,
            };
            syn::visit::Visit::visit_file(&mut visitor, &file);
            visitor.found
        }
        Err(_) => None,
    };
    // The token backstop sees every delimiter group the parser treats as
    // opaque (macro bodies and transcribers) and ignores strings and comments.
    parsed.or_else(|| token_cli_escape(source, depth))
}

struct CliEscapeVisitor {
    depth: usize,
    inline: usize,
    found: Option<String>,
}

impl CliEscapeVisitor {
    fn check_segments(&mut self, segments: &[String], rendered: &str) {
        if self.found.is_some() {
            return;
        }
        match segments.first().map(String::as_str) {
            // A lone `crate` is a visibility (`pub(crate)`), not a path.
            Some("crate") if segments.len() > 1 => {
                if segments.get(1).map(String::as_str) != Some("cli") {
                    self.found = Some(rendered.to_owned());
                }
            }
            // The crate's external name reaches the outer facade and the test
            // harness through the self dev-dependency; the CLI never uses it.
            // A lone `g5d` is an ordinary local binding, not the crate.
            Some("g5d") if segments.len() > 1 => {
                self.found = Some(rendered.to_owned());
            }
            Some("super") => {
                let hops = segments.iter().take_while(|part| *part == "super").count();
                if hops > self.depth + self.inline {
                    self.found = Some(rendered.to_owned());
                }
            }
            _ => {}
        }
    }

    fn check_use_tree(&mut self, prefix: &mut Vec<String>, tree: &syn::UseTree) {
        match tree {
            syn::UseTree::Path(path) => {
                prefix.push(unraw(&path.ident));
                self.check_use_tree(prefix, &path.tree);
                prefix.pop();
            }
            syn::UseTree::Name(name) => {
                prefix.push(unraw(&name.ident));
                let rendered = prefix.join("::");
                if prefix.as_slice() == ["g5d"] || prefix.as_slice() == ["crate"] {
                    self.found.get_or_insert(rendered.clone());
                }
                self.check_segments(prefix, &rendered);
                prefix.pop();
            }
            syn::UseTree::Rename(rename) => {
                prefix.push(unraw(&rename.ident));
                let rendered = format!("{} as {}", prefix.join("::"), rename.rename);
                // Renaming the crate root or an ancestor hands out an alias the
                // scan could not follow, so it is an escape by itself.
                let aliases_root = (prefix.iter().all(|part| part == "super")
                    && prefix.len() > self.depth + self.inline)
                    || prefix.as_slice() == ["crate"]
                    || prefix.as_slice() == ["g5d"]
                    || (prefix.first().map(String::as_str) == Some("crate")
                        && prefix.get(1).map(String::as_str) != Some("cli"));
                if aliases_root {
                    self.found.get_or_insert(rendered.clone());
                }
                self.check_segments(prefix, &rendered);
                prefix.pop();
            }
            syn::UseTree::Glob(_) => {
                let rendered = format!("{}::*", prefix.join("::"));
                // `use crate::*;` imports the whole root, so it escapes even
                // though the bare `crate` segment is otherwise a visibility.
                if prefix.as_slice() == ["crate"] || prefix.as_slice() == ["g5d"] {
                    self.found.get_or_insert(rendered.clone());
                }
                self.check_segments(prefix, &rendered);
            }
            syn::UseTree::Group(group) => {
                for item in &group.items {
                    self.check_use_tree(prefix, item);
                }
            }
        }
    }
}

impl<'ast> syn::visit::Visit<'ast> for CliEscapeVisitor {
    fn visit_item_extern_crate(&mut self, node: &'ast syn::ItemExternCrate) {
        // `extern crate self as outer;` is a crate-root alias like
        // `use crate as outer;`, and the CLI module needs no extern crate at all.
        let ident = unraw(&node.ident);
        if ident == "self" || ident == "g5d" || node.rename.is_some() {
            let rendered = match &node.rename {
                Some((_, rename)) => format!("extern crate {} as {rename}", node.ident),
                None => format!("extern crate {}", node.ident),
            };
            self.found.get_or_insert(rendered);
        }
        syn::visit::visit_item_extern_crate(self, node);
    }

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        if node.content.is_some() {
            self.inline += 1;
            syn::visit::visit_item_mod(self, node);
            self.inline -= 1;
        } else {
            syn::visit::visit_item_mod(self, node);
        }
    }

    fn visit_item_use(&mut self, node: &'ast syn::ItemUse) {
        let mut prefix = Vec::new();
        self.check_use_tree(&mut prefix, &node.tree);
    }

    fn visit_path(&mut self, node: &'ast syn::Path) {
        let segments = node
            .segments
            .iter()
            .map(|segment| unraw(&segment.ident))
            .collect::<Vec<_>>();
        let rendered = if node.leading_colon.is_some() {
            format!("::{}", segments.join("::"))
        } else {
            segments.join("::")
        };
        self.check_segments(&segments, &rendered);
        syn::visit::visit_path(self, node);
    }

    fn visit_item_macro(&mut self, node: &'ast syn::ItemMacro) {
        // `macro_rules!` transcribers are code the module will expand into
        // itself: resolve each `=> { ... }` body with the same import rules.
        if node.mac.path.is_ident("macro_rules") {
            for transcriber in transcriber_groups(node.mac.tokens.clone()) {
                self.scan_tokens(&transcriber.to_string());
            }
        }
        syn::visit::visit_item_macro(self, node);
    }

    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        // Macro bodies are opaque tokens. When they parse as statements
        // (the shape of a macro-expanded `use` or call) resolve them like
        // ordinary code, so grouped imports do not slip through; otherwise
        // scan their text with the fallback.
        if !node.path.is_ident("macro_rules") {
            self.scan_tokens(&node.tokens.to_string());
        }
        syn::visit::visit_macro(self, node);
    }
}

impl CliEscapeVisitor {
    fn scan_tokens(&mut self, tokens: &str) {
        let parsed = syn::parse_str::<syn::Block>(&format!("{{ {tokens} }}"))
            .or_else(|_| syn::parse_str::<syn::Block>(&format!("{{ {tokens}; }}")));
        match parsed {
            Ok(block) => syn::visit::Visit::visit_block(self, &block),
            Err(_) => {
                if let Some(escape) = token_cli_escape(tokens, self.depth + self.inline) {
                    self.found.get_or_insert(escape);
                }
            }
        }
    }
}

/// The token groups that follow `=>` in a `macro_rules!` body: the
/// transcribers, whatever delimiter they use.
fn transcriber_groups(tokens: proc_macro2::TokenStream) -> Vec<proc_macro2::TokenStream> {
    let mut groups = Vec::new();
    let mut saw_eq = false;
    let mut saw_arrow = false;
    for tree in tokens {
        match tree {
            proc_macro2::TokenTree::Punct(punct) if punct.as_char() == '=' => {
                saw_eq = true;
                saw_arrow = false;
            }
            proc_macro2::TokenTree::Punct(punct) if punct.as_char() == '>' && saw_eq => {
                saw_arrow = true;
                saw_eq = false;
            }
            proc_macro2::TokenTree::Group(group) if saw_arrow => {
                groups.push(group.stream());
                saw_arrow = false;
            }
            proc_macro2::TokenTree::Group(group) => {
                groups.extend(transcriber_groups(group.stream()));
                saw_eq = false;
                saw_arrow = false;
            }
            _ => {
                saw_eq = false;
                saw_arrow = false;
            }
        }
    }
    groups
}

/// Token-level scan used as a backstop for macro bodies and unparsable
/// files: `crate::` not followed by `cli`, the crate's own name `g5d::`,
/// `extern crate`, or a `super::` chain longer than `depth` plus the inline
/// `mod` groups it sits in. Tokens ignore strings and comments by
/// construction.
fn token_cli_escape(source: &str, depth: usize) -> Option<String> {
    let tokens: proc_macro2::TokenStream = source.parse().ok()?;
    scan_token_stream(&tokens.into_iter().collect::<Vec<_>>(), depth)
}

fn scan_token_stream(trees: &[proc_macro2::TokenTree], depth: usize) -> Option<String> {
    use proc_macro2::TokenTree;
    let ident_at = |index: usize| match trees.get(index) {
        Some(TokenTree::Ident(ident)) => Some(unraw_token(ident)),
        _ => None,
    };
    let path_sep_at = |index: usize| {
        matches!((trees.get(index), trees.get(index + 1)),
            (Some(TokenTree::Punct(a)), Some(TokenTree::Punct(b)))
                if a.as_char() == ':' && b.as_char() == ':')
    };
    let mut index = 0;
    while index < trees.len() {
        match &trees[index] {
            TokenTree::Ident(ident) => {
                let name = unraw_token(ident);
                let preceded_by_sep = index >= 2 && path_sep_at(index - 2);
                if name == "extern" && ident_at(index + 1).as_deref() == Some("crate") {
                    return Some("extern crate".to_owned());
                }
                // a path root, or a leading `::g5d` (a separator with no ident before it)
                let leading_sep = preceded_by_sep && (index < 3 || ident_at(index - 3).is_none());
                if !preceded_by_sep || leading_sep {
                    if name == "g5d" && path_sep_at(index + 1) {
                        return Some(render_path(trees, index));
                    }
                    if name == "crate"
                        && path_sep_at(index + 1)
                        && ident_at(index + 3).as_deref() != Some("cli")
                    {
                        return Some(render_path(trees, index));
                    }
                    if name == "super" {
                        let mut hops = 0;
                        let mut cursor = index;
                        while ident_at(cursor).as_deref() == Some("super") && path_sep_at(cursor + 1) {
                            hops += 1;
                            cursor += 3;
                        }
                        if hops == 0 && ident_at(cursor).as_deref() == Some("super") {
                            hops = 1;
                        }
                        if hops > depth {
                            return Some(render_path(trees, index));
                        }
                    }
                }
                // an inline module: `mod name { ... }` gives its group one more hop
                if name == "mod" {
                    if let (Some(_), Some(TokenTree::Group(group))) =
                        (ident_at(index + 1), trees.get(index + 2))
                    {
                        let inner = group.stream().into_iter().collect::<Vec<_>>();
                        if let Some(found) = scan_token_stream(&inner, depth + 1) {
                            return Some(found);
                        }
                        index += 3;
                        continue;
                    }
                }
                index += 1;
            }
            TokenTree::Group(group) => {
                let inner = group.stream().into_iter().collect::<Vec<_>>();
                if let Some(found) = scan_token_stream(&inner, depth) {
                    return Some(found);
                }
                index += 1;
            }
            _ => index += 1,
        }
    }
    None
}

/// An identifier without its raw prefix: `r#g5d` names the same crate as `g5d`.
fn unraw(ident: &syn::Ident) -> String {
    syn::ext::IdentExt::unraw(ident).to_string()
}

fn unraw_token(ident: &proc_macro2::Ident) -> String {
    ident
        .to_string()
        .strip_prefix("r#")
        .map_or_else(|| ident.to_string(), str::to_owned)
}

/// Render the path starting at `trees[start]` (`a::b::c`) for a report.
fn render_path(trees: &[proc_macro2::TokenTree], start: usize) -> String {
    use proc_macro2::TokenTree;
    let mut rendered = String::new();
    let mut index = start;
    while let Some(TokenTree::Ident(ident)) = trees.get(index) {
        rendered.push_str(&unraw_token(ident));
        match (trees.get(index + 1), trees.get(index + 2)) {
            (Some(TokenTree::Punct(a)), Some(TokenTree::Punct(b)))
                if a.as_char() == ':' && b.as_char() == ':' =>
            {
                rendered.push_str("::");
                index += 3;
            }
            _ => break,
        }
    }
    rendered
}

#[test]
fn cli_inward_guard_rejects_parent_relative_and_crate_root_escapes() {
    assert_eq!(cli_module_depth(Path::new("mod.rs")), 0);
    assert_eq!(cli_module_depth(Path::new("add.rs")), 1);
    assert_eq!(cli_module_depth(Path::new("git/mod.rs")), 1);
    assert_eq!(cli_module_depth(Path::new("git/credentials.rs")), 2);

    assert_eq!(cli_source_escape("use crate::cli::config;\nuse super::config;", 1), None);
    assert_eq!(cli_source_escape("use super::super::git;", 2), None);
    assert_eq!(
        cli_source_escape(
            "fn run() {}\n#[cfg(test)]\nmod tests {\n    use super::super::DoctorArgs;\n}\n",
            1
        ),
        None
    );
    assert_eq!(
        cli_source_escape(
            "mod tests {\n    use super::super::super::host;\n}\n",
            1
        )
        .as_deref(),
        Some("super::super::super::host")
    );
    assert_eq!(
        cli_source_escape("use super::super::host as outer_host;", 1).as_deref(),
        Some("super::super::host as outer_host")
    );
    assert_eq!(
        cli_source_escape("use super::dispatch_test_support;", 0).as_deref(),
        Some("super::dispatch_test_support")
    );
    assert_eq!(
        cli_source_escape("let f = crate::dispatch_test_support::fixture();", 1).as_deref(),
        Some("crate::dispatch_test_support::fixture")
    );
    assert_eq!(cli_source_escape("let x = my_crate::thing;", 1), None);
    assert_eq!(
        cli_source_escape("use crate as outer;\nuse outer::host::ctl;", 1).as_deref(),
        Some("crate as outer")
    );
    assert_eq!(
        cli_source_escape("use super::super as outer;\nuse outer::dispatch_test_support;", 1)
            .as_deref(),
        Some("super::super as outer")
    );
    assert_eq!(
        cli_source_escape("use crate::host as outer_host;", 1).as_deref(),
        Some("crate::host as outer_host")
    );
    assert_eq!(cli_source_escape("use crate::cli::config as cfg;", 1), None);
    assert_eq!(cli_source_escape("use super as parent;", 1), None);
    assert_eq!(cli_source_escape("let name = value as usize;", 1), None);
    assert_eq!(
        cli_source_escape("use super::{super::host, Cli};", 1).as_deref(),
        Some("super::super::host")
    );
    assert_eq!(cli_source_escape("use super::{config, Cli};", 1), None);
    assert_eq!(
        cli_source_escape("use crate :: host;", 1).as_deref(),
        Some("crate::host")
    );
    assert_eq!(
        cli_source_escape("use crate::{cli::config, host};", 1).as_deref(),
        Some("crate::host")
    );
    assert_eq!(
        cli_source_escape("use crate::*;", 1).as_deref(),
        Some("crate::*")
    );
    assert_eq!(
        cli_source_escape("fn f() { let _ = crate::host::ctl(); }", 1).as_deref(),
        Some("crate::host::ctl")
    );
    assert_eq!(
        cli_source_escape("fn f() { tracing::info!(\"{}\", crate::host::NAME); }", 1).as_deref(),
        Some("crate::host::NAME")
    );
    assert_eq!(cli_source_escape("fn f() { let _ = crate::cli::config::load(); }", 1), None);
    assert_eq!(
        cli_source_escape("extern crate self as outer;\nuse self::outer::host::ctl;", 1).as_deref(),
        Some("extern crate self as outer")
    );
    assert_eq!(
        cli_source_escape("fn f() { expand!(use super::{super::host}); }", 1).as_deref(),
        Some("super::super::host")
    );
    assert_eq!(cli_source_escape("fn f() { expand!(use super::{config}); }", 1), None);
    assert_eq!(cli_source_escape("use self::config::load;", 1), None);
    assert_eq!(
        cli_source_escape("use g5d::dispatch_test_support;", 1).as_deref(),
        Some("g5d::dispatch_test_support")
    );
    assert_eq!(
        cli_source_escape("use ::g5d::host::ctl;", 1).as_deref(),
        Some("g5d::host::ctl")
    );
    assert_eq!(
        cli_source_escape("use g5d as outer;", 1).as_deref(),
        Some("g5d as outer")
    );
    assert_eq!(
        cli_source_escape("use g5d::{host, cli};", 1).as_deref(),
        Some("g5d::host")
    );
    assert_eq!(
        cli_source_escape("fn f() { let _ = ::g5d::host::ctl(); }", 1).as_deref(),
        Some("::g5d::host::ctl")
    );
    assert_eq!(
        cli_source_escape(
            "macro_rules! expose { () => { use super::{super::host}; }; }\nexpose!();",
            1
        )
        .as_deref(),
        Some("super::super::host")
    );
    assert_eq!(
        cli_source_escape("macro_rules! ok { () => { use super::config; }; }\nok!();", 1),
        None
    );
    assert_eq!(cli_source_escape("let g5d_name = g5d_value;", 1), None);
    assert_eq!(
        cli_source_escape("use g5d::*;\nconst E: u64 = dispatch_test_support::EPOCH;", 1).as_deref(),
        Some("g5d::*")
    );
    assert_eq!(
        cli_source_escape("extern crate g5d;\nuse self::g5d::dispatch_test_support;", 1).as_deref(),
        Some("extern crate g5d")
    );
    assert_eq!(
        cli_source_escape(
            "macro_rules! expose { () => (use crate::host as outer_host;); }\nexpose!();",
            1
        )
        .as_deref(),
        Some("crate::host as outer_host")
    );
    assert_eq!(
        cli_source_escape(
            "macro_rules! expose { () => [use super::{super::host};]; }\nexpose!();",
            1
        )
        .as_deref(),
        Some("super::super::host")
    );
    // the token backstop on its own: strings and comments are not paths
    assert_eq!(
        token_cli_escape("fn f() { let t = \"g5d::host\"; } // crate::host", 1),
        None
    );
    assert_eq!(
        token_cli_escape("mod tests { use super::super::host; }", 1),
        None
    );
    assert_eq!(
        token_cli_escape("mod tests { use super::super::super::host; }", 1).as_deref(),
        Some("super::super::super::host")
    );
    assert_eq!(
        token_cli_escape("fn f() { ::g5d::host::ctl() }", 1).as_deref(),
        Some("g5d::host::ctl")
    );
    // raw identifiers name the same crate
    assert_eq!(
        cli_source_escape("use r#g5d::dispatch_test_support;", 1).as_deref(),
        Some("g5d::dispatch_test_support")
    );
    assert_eq!(
        cli_source_escape("use r#g5d::host as outer_host;", 1).as_deref(),
        Some("g5d::host as outer_host")
    );
    assert_eq!(cli_source_escape("use r#g5d::*;", 1).as_deref(), Some("g5d::*"));
    assert_eq!(
        token_cli_escape("fn f() { r#g5d::host::ctl() }", 1).as_deref(),
        Some("g5d::host::ctl")
    );
}

#[test]
fn real_workspace_has_foundation_and_isolation_edges() {
    let workspace_manifest = g5d_manifest_dir()
        .parent()
        .expect("g5d should be inside the generation workspace")
        .join("../Cargo.toml");
    let metadata = load_workspace_metadata(&workspace_manifest);

    let isolation = metadata
        .packages
        .iter()
        .find(|package| package.name == "g5d-isolation")
        .expect("g5d-isolation must be a workspace package");
    assert!(
        metadata.workspace_members.contains(&isolation.id),
        "g5d-isolation must be a workspace member"
    );
    validate_internal_dependencies(
        "g5d-isolation",
        &isolation.dependencies,
        &["g5d-foundation"],
    )
    .expect("isolation dependencies must point only toward admitted lower layers");

    let g5d = metadata
        .packages
        .iter()
        .find(|package| package.name == "g5d")
        .expect("g5d must remain a workspace package");
    for dependency_name in ["g5d-foundation", "g5d-isolation"] {
        assert!(
            g5d.dependencies.iter().any(|dependency| {
                dependency.name == dependency_name && dependency.kind.as_deref() != Some("dev")
            }),
            "g5d must depend on {dependency_name} through a production edge"
        );
    }
}

/// `g5d-agent` (the merged provider runtime and `agent.turn` crate) links only
/// foundation and the shared capabilities it uses; the dispatcher and outer
/// `g5d` reach it one way, and the host never links it.
#[test]
fn real_workspace_has_one_way_agent_edge() {
    let workspace_manifest = g5d_manifest_dir()
        .parent()
        .expect("g5d should be inside the generation workspace")
        .join("../Cargo.toml");
    let metadata = load_workspace_metadata(&workspace_manifest);

    for retired in ["g5d-worker", "g5d-agent-turn"] {
        assert!(
            metadata.packages.iter().all(|package| package.name != retired),
            "{retired} merged into g5d-agent and must not return as a package"
        );
    }
    let agent = metadata
        .packages
        .iter()
        .find(|package| package.name == "g5d-agent")
        .expect("g5d-agent must be a workspace package");
    assert!(
        metadata.workspace_members.contains(&agent.id),
        "g5d-agent must be a workspace member"
    );
    validate_internal_dependencies("g5d-agent", &agent.dependencies, &["g5d-foundation"])
        .expect("agent dependencies must point only toward admitted lower layers");
    let mut internal = agent
        .dependencies
        .iter()
        .filter(|dependency| dependency.kind.as_deref() != Some("dev"))
        .filter(|dependency| {
            dependency.name.starts_with("g5-") || dependency.name.starts_with("g5d")
        })
        .map(|dependency| dependency.name.as_str())
        .collect::<Vec<_>>();
    internal.sort_unstable();
    // g5-semantic: the shared statement rule for a turn's ontology changes
    // card (`g5.chat.ontology-card`).
    assert_eq!(
        internal,
        [
            "g5-contracts",
            "g5-internal-api",
            "g5-scm",
            "g5-semantic",
            "g5d-foundation"
        ],
        "g5d-agent must link exactly the shared capabilities it uses"
    );

    for (consumer, expected) in [("g5d", true), ("g5d-dispatch", true), ("g5d-host", false)] {
        let package = metadata
            .packages
            .iter()
            .find(|package| package.name == consumer)
            .unwrap_or_else(|| panic!("{consumer} must remain a workspace package"));
        assert_eq!(
            package.dependencies.iter().any(|dependency| {
                dependency.name == "g5d-agent" && dependency.kind.as_deref() != Some("dev")
            }),
            expected,
            "{consumer} production edge to g5d-agent must be {expected}"
        );
    }
}

#[test]
fn collaborative_editing_implementation_stays_deleted() {
    let workspace_root = g5d_manifest_dir()
        .parent()
        .expect("g5d should be inside the generation workspace");
    let foundation_root = workspace_root.join("g5d-foundation");
    let g5d_root = workspace_root.join("g5d");

    assert!(
        !foundation_root.join("src/collaboration").exists()
            && !g5d_root.join("src/collaboration").exists(),
        "the removed collaborative editing implementation must stay deleted"
    );
}

#[test]
fn foundation_owns_oci_with_compatibility_export() {
    let workspace_root = g5d_manifest_dir()
        .parent()
        .expect("g5d should be inside the generation workspace");
    let foundation_root = workspace_root.join("g5d-foundation");
    let g5d_root = workspace_root.join("g5d");

    assert!(
        foundation_root.join("src/oci/mod.rs").is_file(),
        "g5d-foundation must physically own the OCI module"
    );
    assert!(
        !g5d_root.join("src/oci/mod.rs").exists(),
        "the former g5d OCI owner path must be removed"
    );

    let foundation_lib = std::fs::read_to_string(foundation_root.join("src/lib.rs"))
        .expect("g5d-foundation library root should be readable");
    assert!(
        foundation_lib.contains("pub mod oci;"),
        "g5d-foundation must export its OCI module"
    );

    let g5d_lib = std::fs::read_to_string(g5d_root.join("src/lib.rs"))
        .expect("g5d library root should be readable");
    assert!(
        g5d_lib.contains("pub use g5d_foundation::oci;"),
        "g5d must preserve its current OCI path through a compatibility export"
    );
}

#[test]
fn foundation_owns_workspace_and_write_policy_with_compatibility_exports() {
    let workspace_root = g5d_manifest_dir()
        .parent()
        .expect("g5d should be inside the generation workspace");
    let foundation_root = workspace_root.join("g5d-foundation");
    let g5d_root = workspace_root.join("g5d");

    assert!(
        foundation_root.join("src/workspace/mod.rs").is_file(),
        "g5d-foundation must physically own the workspace module"
    );
    assert!(
        !g5d_root.join("src/workspace/mod.rs").exists(),
        "the former g5d workspace owner path must be removed"
    );

    let foundation_lib = std::fs::read_to_string(foundation_root.join("src/lib.rs"))
        .expect("g5d-foundation library root should be readable");
    assert!(
        foundation_lib.contains("pub mod workspace;"),
        "g5d-foundation must export its workspace module"
    );

    let g5d_lib = std::fs::read_to_string(g5d_root.join("src/lib.rs"))
        .expect("g5d library root should be readable");
    assert!(
        g5d_lib.contains("pub use g5d_foundation::workspace;"),
        "g5d must preserve its public workspace path through a compatibility export"
    );

    let foundation_manifest = std::fs::read_to_string(foundation_root.join("Cargo.toml"))
        .expect("g5d-foundation manifest should be readable");
    assert!(
        foundation_manifest.contains("workspace-integration = []"),
        "foundation must own the moved workspace integration-test gate"
    );
    let g5d_manifest = std::fs::read_to_string(g5d_root.join("Cargo.toml"))
        .expect("g5d manifest should be readable");
    assert!(
        g5d_manifest.contains("workspace-integration = [\"g5d-foundation/workspace-integration\"]"),
        "g5d must forward its existing workspace integration feature"
    );
}

#[test]
fn isolation_owns_runtime_backend() {
    let workspace_root = g5d_manifest_dir()
        .parent()
        .expect("g5d should be inside the generation workspace");
    let isolation_root = workspace_root.join("g5d-isolation");
    let g5d_root = workspace_root.join("g5d");

    assert!(
        isolation_root.join("src/isolation/mod.rs").is_file(),
        "g5d-isolation must physically own the isolation module"
    );
    assert!(
        isolation_root.join("src/isolation/cloud_hypervisor.rs").is_file(),
        "g5d-isolation must own the Cloud Hypervisor process boundary"
    );
    assert!(
        !isolation_root.join("build.rs").exists(),
        "the isolation crate must not retain the replaced native VMM linking script"
    );
    assert!(
        !g5d_root.join("src/isolation/mod.rs").exists(),
        "the former g5d isolation owner path must be removed"
    );
    assert!(
        !g5d_root.join("build.rs").exists(),
        "the outer composition crate must not duplicate isolation native linking"
    );

    let isolation_lib = std::fs::read_to_string(isolation_root.join("src/lib.rs"))
        .expect("g5d-isolation library root should be readable");
    assert!(
        isolation_lib.contains("pub mod isolation;"),
        "g5d-isolation must export the nested isolation module"
    );

    let g5d_lib = std::fs::read_to_string(g5d_root.join("src/lib.rs"))
        .expect("g5d library root should be readable");
    assert!(
        g5d_lib.contains("pub use g5d_isolation::isolation;"),
        "g5d must preserve its isolation path through a compatibility export"
    );
}

/// The agent crate exports the provider runtime seams the dispatcher links
/// and keeps the `agent.turn` modules private behind `execute` and the
/// frozen input types.
#[test]
fn agent_owns_runtime_seams_and_private_turn_modules() {
    let workspace_root = g5d_manifest_dir()
        .parent()
        .expect("g5d should be inside the generation workspace");
    let metadata = load_workspace_metadata(&workspace_root.join("../Cargo.toml"));
    let agent = metadata
        .packages
        .iter()
        .find(|package| package.name == "g5d-agent")
        .expect("g5d-agent must be a workspace package");
    assert!(
        metadata.workspace_members.contains(&agent.id),
        "g5d-agent must remain a workspace member"
    );
    validate_internal_dependencies("g5d-agent", &agent.dependencies, &["g5d-foundation"])
        .expect("agent dependencies must point only toward admitted lower layers");
    for retired_root in ["g5d-worker", "g5d-agent-turn"] {
        assert!(
            !workspace_root.join(retired_root).exists(),
            "{retired_root} merged into g5d-agent and its tree must stay absent"
        );
    }

    let worker_root = workspace_root.join("g5d-agent");
    let worker_lib = std::fs::read_to_string(worker_root.join("src/lib.rs"))
        .expect("g5d-agent library root should be readable");
    for module in [
        "port_scan",
        "runner",
    ] {
        assert!(
            (worker_root.join(format!("src/{module}.rs")).is_file()
                || worker_root.join(format!("src/{module}/mod.rs")).is_file())
                && worker_lib.contains(&format!("pub mod {module};")),
            "g5d-agent must own and export retained provider support {module}"
        );
    }
    for module in [
        "codex_auth",
        "harness",
        "memory",
        "policy",
        "protocol",
        "provider_transport",
        "runtime",
        "types",
    ] {
        assert!(
            worker_root.join(format!("src/{module}.rs")).is_file()
                && worker_lib.contains(&format!("\nmod {module};"))
                && !worker_lib.contains(&format!("pub mod {module};")),
            "g5d-agent must keep the agent.turn module {module} private"
        );
    }
    for export in [
        "pub use policy::{resolve_policy, ResolvedPolicy};",
        "pub use protocol::{ClaudeControl, NativeRequest, NativeRequestKind};",
        "pub use runtime::execute;",
        "pub use types::*;",
    ] {
        assert!(
            worker_lib.contains(export),
            "g5d-agent must keep the agent.turn surface `{export}`"
        );
    }
    for retired in [
        "inbox",
        "poll",
        "task_env",
        "task_agent",
        "view_task_builder",
        "vsock_mux",
        "vsock_transport",
        "build_telemetry",
        "lifecycle",
        "runtime_keeper",
        "usage",
    ] {
        assert!(
            !worker_root.join(format!("src/{retired}.rs")).exists(),
            "retired per-task worker module {retired} must stay deleted"
        );
    }
    assert!(
        !workspace_root.join("g5d/src/worker").exists(),
        "outer per-task worker compatibility tree must stay deleted"
    );

    let g5d = metadata
        .packages
        .iter()
        .find(|package| package.name == "g5d")
        .expect("g5d must remain a workspace package");
    assert!(
        g5d.dependencies.iter().any(|dependency| {
            dependency.name == "g5d-agent" && dependency.kind.as_deref() != Some("dev")
        }),
        "outer g5d must depend on retained provider support through g5d-agent"
    );
}
#[test]
fn docker_build_surfaces_track_foundation_member() {
    let workspace_root = g5d_manifest_dir()
        .parent()
        .expect("g5d should be inside the generation workspace");
    let docker_dir = workspace_root.join("../infra/docker");

    for file_name in [
        "Dockerfile.g5d",
        "Dockerfile.g5-api",
        "Dockerfile.test-runner",
        "Dockerfile.ci-builder",
    ] {
        let contents = std::fs::read_to_string(docker_dir.join(file_name))
            .unwrap_or_else(|error| panic!("failed to read {file_name}: {error}"));
        assert!(
            contents.contains(
                "COPY backend/g5d-foundation/Cargo.toml backend/g5d-foundation/Cargo.toml"
            ),
            "{file_name} must copy the g5d-foundation manifest"
        );
        assert!(
            contents.contains("g5d-foundation/src"),
            "{file_name} must create a dummy g5d-foundation library root"
        );
    }

    for file_name in [
        "Dockerfile.g5d",
        "Dockerfile.g5-api",
        "Dockerfile.test-runner",
    ] {
        let contents = std::fs::read_to_string(docker_dir.join(file_name))
            .unwrap_or_else(|error| panic!("failed to read {file_name}: {error}"));
        assert!(
            contents.contains("COPY backend/g5d-foundation/ backend/g5d-foundation/"),
            "{file_name} must copy the real g5d-foundation crate"
        );
    }
}

#[test]
fn docker_and_ci_surfaces_track_api_foundation_member() {
    let workspace_root = g5d_manifest_dir()
        .parent()
        .expect("g5d should be inside the generation workspace");
    let docker_dir = workspace_root.join("../infra/docker");

    for file_name in [
        "Dockerfile.g5d",
        "Dockerfile.g5-api",
        "Dockerfile.test-runner",
        "Dockerfile.ci-builder",
    ] {
        let contents = std::fs::read_to_string(docker_dir.join(file_name))
            .unwrap_or_else(|error| panic!("failed to read {file_name}: {error}"));
        assert!(
            contents.contains(
                "COPY backend/g5-api-foundation/Cargo.toml backend/g5-api-foundation/Cargo.toml"
            ),
            "{file_name} must copy the g5-api-foundation manifest"
        );
        assert!(
            contents.contains("g5-api-foundation/src"),
            "{file_name} must create a dummy g5-api-foundation library root"
        );
    }
    for file_name in ["Dockerfile.g5-api", "Dockerfile.test-runner"] {
        let contents = std::fs::read_to_string(docker_dir.join(file_name))
            .unwrap_or_else(|error| panic!("failed to read {file_name}: {error}"));
        assert!(
            contents.contains("COPY backend/g5-api-foundation/ backend/g5-api-foundation/"),
            "{file_name} must copy the real g5-api-foundation crate"
        );
    }

    assert_ci_builder_hashes_cover(
        &[
            "g5-api-foundation",
            "g5-api-data",
            "g5-api",
        ],
        "when the API foundation manifest changes",
    );
}

#[test]
fn services_do_not_reference_routes() {
    let workspace_root = g5d_manifest_dir()
        .parent()
        .expect("g5d should be inside the generation workspace");
    let services_root = workspace_root.join("g5-api-core/src/services");
    let mut service_files = Vec::new();
    collect_rust_files(&services_root, &mut service_files);
    let mut violations = Vec::new();
    for path in service_files {
        let contents = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        if contents.contains("crate::routes") {
            violations.push(
                path.strip_prefix(workspace_root)
                    .unwrap_or(&path)
                    .display()
                    .to_string(),
            );
        }
    }
    assert!(
        violations.is_empty(),
        "services must not reference route-owned code; violations: {violations:?}"
    );
}

#[test]
fn api_data_owns_models_repository_and_database_lifecycle_without_reverse_edges() {
    let workspace_root = g5d_manifest_dir()
        .parent()
        .expect("g5d should be inside the generation workspace");
    let metadata = load_workspace_metadata(&workspace_root.join("../Cargo.toml"));
    let data = metadata
        .packages
        .iter()
        .find(|package| package.name == "g5-api-data")
        .expect("g5-api-data must be a workspace package");
    validate_internal_dependencies(
        "g5-api-data",
        &data.dependencies,
        &["g5-api-foundation"],
    )
    .expect("g5-api-data must remain below API implementation packages");

    let outer_api = metadata
        .packages
        .iter()
        .find(|package| package.name == "g5-api")
        .expect("g5-api must be a workspace package");
    assert!(
        outer_api
            .dependencies
            .iter()
            .any(|dependency| dependency.name == "g5-api-data"),
        "outer g5-api must depend on g5-api-data"
    );

    let data_root = workspace_root.join("g5-api-data/src");
    assert!(
        data_root.join("models/mod.rs").is_file(),
        "g5-api-data must physically own the model tree"
    );
    assert!(
        data_root
            .join("repositories/branch_integration_queue.rs")
            .is_file(),
        "g5-api-data must physically own the admitted branch queue repository"
    );
    assert!(
        data_root.join("db.rs").is_file(),
        "g5-api-data must physically own database lifecycle behavior"
    );

    let core_lib = std::fs::read_to_string(workspace_root.join("g5-api-core/src/lib.rs"))
        .expect("core compatibility library must exist");
    assert!(core_lib.contains("pub use g5_api_data::models;"));
    let outer_lib = std::fs::read_to_string(workspace_root.join("g5-api/src/lib.rs"))
        .expect("outer compatibility library must exist");
    assert!(outer_lib.contains("models"));
    let core_db = std::fs::read_to_string(workspace_root.join("g5-api-core/src/db/mod.rs"))
        .expect("core database compatibility facade must exist");
    assert!(core_db.contains("pub mod scoped;"));
    assert!(core_db.contains("pub use g5_api_data::db::*;"));
    assert!(
        workspace_root
            .join("g5-api-core/src/db/scoped.rs")
            .is_file(),
        "request-scoped capability wiring must remain core-owned"
    );

    let data_db = std::fs::read_to_string(data_root.join("db.rs"))
        .expect("data database lifecycle source must be readable");
    assert!(
        data_db.contains("sqlx::migrate!(\"../g5-api/src/db/migrations\")"),
        "embedded migrations must retain the established migration catalog"
    );
    let data_build = std::fs::read_to_string(workspace_root.join("g5-api-data/build.rs"))
        .expect("data build script must track the external migration catalog");
    assert!(data_build.contains("../g5-api/src/db/migrations"));

    let migrate_bin =
        std::fs::read_to_string(workspace_root.join("g5-api/src/bin/g5-db-migrate.rs"))
            .expect("migration binary source must be readable");
    assert!(migrate_bin.contains("\"g5-api/src/db/migrations\""));
    assert!(migrate_bin.contains("\"/app/migrations\""));
}

#[test]
fn docker_and_ci_surfaces_track_api_data_member() {
    let workspace_root = g5d_manifest_dir()
        .parent()
        .expect("g5d should be inside the generation workspace");
    let docker_dir = workspace_root.join("../infra/docker");

    for file_name in [
        "Dockerfile.g5d",
        "Dockerfile.g5-api",
        "Dockerfile.test-runner",
        "Dockerfile.ci-builder",
    ] {
        let contents = std::fs::read_to_string(docker_dir.join(file_name))
            .unwrap_or_else(|error| panic!("failed to read {file_name}: {error}"));
        assert!(
            contents.contains("COPY backend/g5-api-data/Cargo.toml backend/g5-api-data/Cargo.toml"),
            "{file_name} must copy the g5-api-data manifest"
        );
        assert!(
            contents.contains("g5-api-data/src"),
            "{file_name} must create a dummy g5-api-data library root"
        );
    }
    for file_name in ["Dockerfile.g5-api", "Dockerfile.test-runner"] {
        let contents = std::fs::read_to_string(docker_dir.join(file_name))
            .unwrap_or_else(|error| panic!("failed to read {file_name}: {error}"));
        assert!(
            contents.contains("COPY backend/g5-api-data/ backend/g5-api-data/"),
            "{file_name} must copy the real g5-api-data crate"
        );
    }

    assert_ci_builder_hashes_cover(
        &[
            "g5-api-foundation",
            "g5-api-data",
            "g5-api",
        ],
        "when API data changes",
    );
}

#[test]
fn api_core_and_routes_form_one_way_packages_with_thin_compatibility_shell() {
    let workspace_root = g5d_manifest_dir()
        .parent()
        .expect("g5d should be inside the generation workspace");
    let metadata = load_workspace_metadata(&workspace_root.join("../Cargo.toml"));

    let core = metadata
        .packages
        .iter()
        .find(|package| package.name == "g5-api-core")
        .expect("g5-api-core must be a workspace package");
    validate_internal_dependencies(
        "g5-api-core",
        &core.dependencies,
        &[
            "g5-authz",
            "g5-api-foundation",
            "g5-api-data",
        ],
    )
    .expect("g5-api-core must stay below routes and the outer shell");

    let routes = metadata
        .packages
        .iter()
        .find(|package| package.name == "g5-api-routes")
        .expect("g5-api-routes must be a workspace package");
    validate_internal_dependencies(
        "g5-api-routes",
        &routes.dependencies,
        &[
            "g5-authz",
            "g5-api-foundation",
            "g5-api-data",
            "g5-api-core",
        ],
    )
    .expect("g5-api-routes must depend one way on core and lower packages");
    assert!(
        routes
            .dependencies
            .iter()
            .any(|dependency| dependency.name == "g5-api-core"),
        "g5-api-routes must depend on g5-api-core"
    );

    let outer = metadata
        .packages
        .iter()
        .find(|package| package.name == "g5-api")
        .expect("g5-api must remain a workspace package");
    for dependency_name in ["g5-api-core", "g5-api-routes"] {
        assert!(
            outer
                .dependencies
                .iter()
                .any(|dependency| dependency.name == dependency_name),
            "outer g5-api must depend on {dependency_name}"
        );
    }

    let core_root = workspace_root.join("g5-api-core/src");
    let routes_root = workspace_root.join("g5-api-routes/src");
    assert!(core_root.join("services/mod.rs").is_file());
    assert!(core_root.join("db/scoped.rs").is_file());
    assert!(routes_root.join("routes/mod.rs").is_file());
    assert!(!core_root.join("routes").exists());

    let mut core_files = Vec::new();
    collect_rust_files(&core_root, &mut core_files);
    for source_file in core_files {
        let source = std::fs::read_to_string(&source_file)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", source_file.display()));
        assert!(!source.contains("crate::routes"));
        assert!(!source.contains("g5_api_routes"));
    }

    let outer_lib = std::fs::read_to_string(workspace_root.join("g5-api/src/lib.rs"))
        .expect("outer compatibility library must exist");
    assert!(outer_lib.contains("pub use g5_api_core"));
    assert!(outer_lib.contains("pub use g5_api_routes::routes"));
    for moved_tree in [
        "auth",
        "cloud",
        "errors",
        "maintenance",
        "middleware",
        "models",
        "routes",
        "scheduler",
        "services",
        "ws",
    ] {
        assert!(
            !workspace_root.join("g5-api/src").join(moved_tree).exists(),
            "outer g5-api must not retain duplicate {moved_tree} implementation"
        );
    }
}

#[test]
fn docker_and_ci_surfaces_track_api_core_and_routes_members() {
    let workspace_root = g5d_manifest_dir()
        .parent()
        .expect("g5d should be inside the generation workspace");
    let docker_dir = workspace_root.join("../infra/docker");

    for file_name in [
        "Dockerfile.g5d",
        "Dockerfile.g5-api",
        "Dockerfile.test-runner",
        "Dockerfile.ci-builder",
    ] {
        let contents = std::fs::read_to_string(docker_dir.join(file_name))
            .unwrap_or_else(|error| panic!("failed to read {file_name}: {error}"));
        for crate_name in ["g5-api-core", "g5-api-routes"] {
            assert!(
                contents.contains(&format!(
                    "COPY backend/{crate_name}/Cargo.toml backend/{crate_name}/Cargo.toml"
                )),
                "{file_name} must copy the {crate_name} manifest"
            );
            assert!(
                contents.contains(&format!("backend/{crate_name}/src")),
                "{file_name} must create a dummy {crate_name} library root"
            );
        }
    }
    for file_name in ["Dockerfile.g5-api", "Dockerfile.test-runner"] {
        let contents = std::fs::read_to_string(docker_dir.join(file_name))
            .unwrap_or_else(|error| panic!("failed to read {file_name}: {error}"));
        for crate_name in ["g5-api-core", "g5-api-routes"] {
            assert!(
                contents.contains(&format!("COPY backend/{crate_name}/ backend/{crate_name}/")),
                "{file_name} must copy the real {crate_name} crate"
            );
        }
    }
    assert_ci_builder_hashes_cover(
        &[
            "g5-api-foundation",
            "g5-api-data",
            "g5-api-core",
            "g5-api-routes",
            "g5-api",
        ],
        "for API core and routes",
    );
}

#[test]
fn docker_build_surfaces_track_runtime_and_isolation_members() {
    let workspace_root = g5d_manifest_dir()
        .parent()
        .expect("g5d should be inside the generation workspace");
    let docker_dir = workspace_root.join("../infra/docker");

    for file_name in [
        "Dockerfile.g5d",
        "Dockerfile.g5-api",
        "Dockerfile.test-runner",
        "Dockerfile.ci-builder",
    ] {
        let contents = std::fs::read_to_string(docker_dir.join(file_name))
            .unwrap_or_else(|error| panic!("failed to read {file_name}: {error}"));
        let dependency_stub_mkdir = contents
            .split("RUN mkdir -p")
            .nth(1)
            .and_then(|tail| tail.split("&& \\").next())
            .unwrap_or_else(|| panic!("{file_name} must create dependency-stub source roots"));
        for crate_name in [
            "g5-contracts",
            "g5-auth",
            "g5-storage",
            "g5-semantic",
            "g5-scm",
            "g5-internal-api",
            "g5d-isolation",
            "g5d-agent",
            "g5d-host",
        ] {
            assert!(
                contents.contains(&format!(
                    "COPY backend/{crate_name}/Cargo.toml backend/{crate_name}/Cargo.toml"
                )),
                "{file_name} must copy the {crate_name} manifest"
            );
            assert!(
                dependency_stub_mkdir
                    .split_whitespace()
                    .any(|item| item == format!("backend/{crate_name}/src")),
                "{file_name} dependency-stub mkdir must create {crate_name}/src before redirects"
            );
        }
    }

    for file_name in [
        "Dockerfile.g5d",
        "Dockerfile.g5-api",
        "Dockerfile.test-runner",
    ] {
        let contents = std::fs::read_to_string(docker_dir.join(file_name))
            .unwrap_or_else(|error| panic!("failed to read {file_name}: {error}"));
        for crate_name in [
            "g5d-isolation",
            "g5d-agent",
            "g5d-host",
        ] {
            assert!(
                contents.contains(&format!("COPY backend/{crate_name}/ backend/{crate_name}/")),
                "{file_name} must copy the real {crate_name} crate"
            );
        }
    }
}

#[test]
fn ci_builder_hashes_track_runtime_and_isolation_manifests() {
    assert_ci_builder_hashes_cover(
        &[
            "g5d-foundation",
            "g5d-isolation",
            "g5d-agent",
            "g5d-host",
            "g5d",
        ],
        "for every extracted runtime manifest",
    );
}

#[test]
fn ci_builder_hash_scan_rejects_crate_roots_that_miss_backend() {
    // The form every builder hash used until the QA infrastructure change: crate
    // directories named from the repository root, where they no longer exist
    // after the move under backend/, so the hash covered no manifests.
    let pre_relocation = r#"
  - id: build-ci-image
    dir: .
    script: |
        HASH=$$(
          {
            printf '%s\n' \
              Cargo.toml \
              Cargo.lock
            find g5-storage g5-api-foundation g5-api g5d e2e \
              -maxdepth 1 \
              \( -name Cargo.toml -o -name build.rs \) \
              -type f | sort
            find .sqlx -type f | sort
          } | while read -r file; do
            sha256sum "$${file}"
          done | sha256sum | cut -c1-12
        )
  - id: next-step
    dir: .
"#;
    let scan = builder_hash_manifest_scan(pre_relocation).expect("the stale form still parses");
    assert_eq!(
        unreachable_member_manifests(&scan, &["g5-api-foundation", "g5d"]),
        [
            "backend/g5-api-foundation/Cargo.toml",
            "backend/g5d/Cargo.toml"
        ],
        "a crate list rooted outside backend/ must be reported as hashing nothing"
    );

    let current = pre_relocation.replace(
        "find g5-storage g5-api-foundation g5-api g5d e2e \\\n              -maxdepth 1",
        "find backend tests/e2e \\\n              -maxdepth 2",
    );
    let scan = builder_hash_manifest_scan(&current).expect("the current form parses");
    assert_eq!(scan.roots, ["backend", "tests/e2e"]);
    assert!(unreachable_member_manifests(&scan, &["g5-api-foundation", "g5d"]).is_empty());

    let too_shallow = current.replace("-maxdepth 2", "-maxdepth 1");
    let scan = builder_hash_manifest_scan(&too_shallow).expect("the shallow form parses");
    assert_eq!(
        unreachable_member_manifests(&scan, &["g5d"]),
        ["backend/g5d/Cargo.toml"],
        "backend/<crate>/Cargo.toml sits at depth 2 below backend/"
    );

    let wrong_dir = current.replace("    dir: .\n    script", "    dir: backend\n    script");
    assert!(builder_hash_manifest_scan(&wrong_dir).is_err());
}

#[test]
fn ci_executes_unit_tests_for_every_extracted_package() {
    let workspace_root = g5d_manifest_dir()
        .parent()
        .expect("g5d should be inside the generation workspace");
    let api_packages = [
        "g5-api-foundation",
        "g5-api-data",
        "g5-api-core",
        "g5-api-routes",
    ];
    let shared_packages = [
        "g5-contracts",
        "g5-auth",
        "g5-storage",
        "g5-semantic",
        "g5-scm",
        "g5-internal-api",
    ];
    // `g5d` is listed because it now owns the CLI library and its unit suite.
    let g5d_packages = [
        "g5d-foundation",
        "g5d-isolation",
        "g5d-agent",
        "g5d-host",
        "g5d-dispatch",
        "g5d",
    ];

    let cloudbuild =
        std::fs::read_to_string(workspace_root.join("../infra/ci/cloudbuild-ci-rust.yaml"))
            .expect("Cloud Build Rust configuration must be readable");
    let xfs =
        std::fs::read_to_string(workspace_root.join("../scripts/run-rust-xfs-orchestrator.sh"))
            .expect("XFS orchestrator must be readable");
    let local = std::fs::read_to_string(workspace_root.join("../scripts/test-parallel.sh"))
        .expect("local parallel test runner must be readable");
    let e2e = std::fs::read_to_string(workspace_root.join("../infra/ci/cloudbuild-ci-e2e.yaml"))
        .expect("E2E Cloud Build configuration must be readable");
    let justfile = std::fs::read_to_string(workspace_root.join("../Justfile"))
        .expect("local ci-docker recipe must be readable");

    for package_name in shared_packages
        .into_iter()
        .chain(api_packages)
        .chain(g5d_packages)
    {
        assert!(
            package_flag_count(&cloudbuild, package_name) >= 4,
            "Cloud Build must compile, execute, and report unit coverage for {package_name}"
        );
        assert!(
            cloudbuild.contains(&format!("package(={package_name})")),
            "Cloud Build archive filter must execute {package_name} unit tests"
        );
        // One `-p` flag, not two: the orchestrator's superset compile
        // (UNIT_PACKAGE_ARGS) is the only place a package is named on a Cargo
        // command line now that the per-package `cargo llvm-cov report` calls
        // are gone. The UNIT_PACKAGES membership below still proves the package
        // is sliced into a lane and executed.
        assert!(
            package_flag_count(&xfs, package_name) >= 1
                && xfs.contains(&format!("\"{package_name}\",")),
            "XFS orchestration must include and select {package_name} unit binaries"
        );
        assert!(
            package_flag_count(&local, package_name) >= 1,
            "local parallel tests must execute {package_name} unit tests"
        );
    }

    assert!(
        cloudbuild.contains("package(=g5d-isolation) and kind(=lib) and test(isolation)"),
        "Cloud Build isolation-unit lane must select the physical isolation package"
    );
    // The CLI's unit tests live in the g5d library now: the local lane must
    // run `cargo test -p g5d … --lib` and the archive filter must pair
    // `package(=g5d)` with `kind(=lib)`.
    assert!(
        local
            .lines()
            .any(|line| line.contains("cargo test -p g5d") && line.contains("--lib")),
        "local parallel tests must execute the g5d library (CLI) unit tests"
    );
    let archive_filter = cloudbuild
        .lines()
        .find(|line| line.contains("package(=g5d))") && line.contains("kind(=lib)"))
        .expect("Cloud Build archive filter must select g5d library tests");
    assert!(archive_filter.contains("kind(=lib)"));
    for package_name in api_packages.into_iter().chain(g5d_packages) {
        assert!(
            package_flag_count(&e2e, package_name) >= 1,
            "E2E Rust target-prime must compile {package_name} unit harnesses"
        );
        assert!(
            package_flag_count(&justfile, package_name) >= 2,
            "local ci-docker must execute and report coverage for {package_name}"
        );
    }
    assert!(
        e2e.contains("-p g5d-isolation --lib --all-features -E \"test(isolation)\" --no-run"),
        "target-prime isolation lane must select the physical isolation owner"
    );
    assert!(
        justfile.contains("$ISOLATION_NEXTEST -p g5d-isolation --lib --all-features"),
        "local ci-docker isolation-unit lane must select the physical isolation owner"
    );
    assert!(
        justfile.contains("cargo test -p g5-api-routes --lib \"guards::tests\"")
            && justfile.contains("cargo test -p g5d-isolation --lib --all-features"),
        "local CI recipes must select physical route-guard and isolation-unit owners"
    );
    // The dedicated `ontology-v4-validation` gate is retired (G5-Lean has no
    // ontology; see docs/ci-e2e-scope.md), so Cloud Build no longer names these
    // filters on a command line. The tests themselves are untouched and still
    // execute in the orchestrator's rust-test lane, because g5d is in
    // UNIT_PACKAGES and the lane runs its `--lib` target:
    // `spec::tests::validate_specs_gates_phase1_lint_diagnostics` and
    // `spec::tests::validate_specs_reports_additive_ontology_markup_summary`
    // pass there. Asserting their physical owner is therefore the
    // UNIT_PACKAGES membership checked above.
    assert!(
        local.contains("cargo test \"${API_IMPLEMENTATION_UNIT_PACKAGE_ARGS[@]}\" --doc")
            && local.contains("cargo test \"${G5D_IMPLEMENTATION_UNIT_PACKAGE_ARGS[@]}\" --doc"),
        "local doc lane must execute the extracted API and g5d library packages"
    );
    assert!(
        xfs.contains("package_name(binary) == \"g5-contracts\"")
            && xfs.contains("binary_name(binary) == \"consumer_boundaries\""),
        "XFS unit orchestration must execute the decomposition integration guard"
    );
    let g5d_integration_selector = xfs
        .split("def is_g5d_integration(binary):")
        .nth(1)
        .and_then(|tail| tail.split("def is_isolation_backends(binary):").next())
        .expect("XFS g5d integration selector must remain present");
    assert!(
        xfs.contains("binary_name(binary) == \"dispatch\"")
            && g5d_integration_selector.contains("\"dispatch\","),
        "XFS orchestration must run the dispatch process suite only in rust-test"
    );

    let rust_test_step = cloudbuild
        .split("  - id: rust-test")
        .nth(1)
        .and_then(|tail| tail.split("  - id: rust-integration").next())
        .expect("Cloud Build rust-test step must remain present");
    assert!(
        rust_test_step.contains("      - |\n        set -euo pipefail\n"),
        "Cloud Build rust-test must fail closed in both archive and non-archive modes"
    );
}

#[test]
fn retired_execution_test_binaries_are_absent_from_nextest_and_xfs_orchestration() {
    let workspace_root = g5d_manifest_dir()
        .parent()
        .expect("g5d should be inside the generation workspace");
    let repo_root = workspace_root
        .parent()
        .expect("generation workspace should be inside the repository");
    let nextest = std::fs::read_to_string(repo_root.join(".config/nextest.toml"))
        .expect("nextest configuration must be readable");
    let xfs = std::fs::read_to_string(workspace_root.join("../scripts/run-rust-xfs-orchestrator.sh"))
        .expect("XFS orchestrator must be readable");

    for retired_binary in [
        "draft_session_scope_contract",
        "agent_pipeline",
        "pipeline_tools",
        "synth_lifecycle",
    ] {
        assert!(
            !nextest.contains(retired_binary),
            "nextest must not select retired binary {retired_binary}"
        );
        assert!(
            !xfs.contains(retired_binary),
            "XFS orchestration must not select retired binary {retired_binary}"
        );
    }
}

#[test]
fn local_parallel_unit_lane_propagates_background_failures() {
    let workspace_root = g5d_manifest_dir()
        .parent()
        .expect("g5d should be inside the generation workspace");
    let fixture = tempfile::tempdir().expect("temporary command fixture should be created");
    let fake_cargo = fixture.path().join("cargo");
    std::fs::write(
        &fake_cargo,
        "#!/usr/bin/env bash\nif [[ \" $* \" == *\" -p g5-api-core \"* ]]; then exit 41; fi\nexit 0\n",
    )
    .expect("fake cargo fixture should be written");
    let mut permissions = std::fs::metadata(&fake_cargo)
        .expect("fake cargo fixture metadata should be readable")
        .permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&fake_cargo, permissions)
        .expect("fake cargo fixture should be executable");

    let mut path = fixture.path().as_os_str().to_os_string();
    path.push(":");
    path.push(std::env::var_os("PATH").unwrap_or_default());
    let output = Command::new("bash")
        .arg("scripts/test-parallel.sh")
        .arg("unit")
        .current_dir(workspace_root.join(".."))
        .env("PATH", path)
        .env("G5_PORT_POSTGRES", "15432")
        .output()
        .expect("local parallel test runner should start");

    assert!(
        !output.status.success(),
        "a failed background package family must fail the local unit lane"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Failed: 1"),
        "failure summary was: {stdout}; stderr was: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("unit g5-api family"),
        "failed package family was absent from summary: {stdout}"
    );
}

#[test]
fn demo_seed_copies_every_extracted_implementation_crate() {
    let workspace_root = g5d_manifest_dir()
        .parent()
        .expect("g5d should be inside the generation workspace");
    let seed = std::fs::read_to_string(workspace_root.join("../scripts/seed-demo.sh"))
        .expect("demo seed script must be readable");
    let crate_list = seed
        .split("RUST_CRATES=(")
        .nth(1)
        .and_then(|tail| tail.split(')').next())
        .expect("demo seed script must declare its Rust crate copy set");

    for package_name in [
        "g5-contracts",
        "g5-auth",
        "g5-storage",
        "g5-semantic",
        "g5-scm",
        "g5-internal-api",
        "g5-api-foundation",
        "g5-api-data",
        "g5-api-core",
        "g5-api-routes",
        "g5d-foundation",
        "g5d-isolation",
        "g5d-agent",
        "g5d-host",
    ] {
        assert!(
            crate_list
                .split_whitespace()
                .any(|item| item == package_name),
            "demo seed must copy physical implementation owner {package_name}"
        );
    }
}

#[test]
fn decomposition_support_surfaces_track_every_extracted_shared_crate() {
    let workspace_root = g5d_manifest_dir()
        .parent()
        .expect("g5d should be inside the generation workspace");
    let worker_hash = std::fs::read_to_string(workspace_root.join("../scripts/worker-host-env.sh"))
        .expect("worker-host freshness script must be readable");
    let changed_gate =
        std::fs::read_to_string(workspace_root.join("../scripts/g5_verify_changed.py"))
            .expect("changed-file production gate must be readable");

    for package_name in [
        "g5-contracts",
        "g5-auth",
        "g5-storage",
        "g5-semantic",
        "g5-scm",
        "g5-internal-api",
    ] {
        assert!(
            worker_hash.contains(&format!("\"backend/{package_name}/Cargo.toml\""))
                && worker_hash.contains(&format!("\"backend/{package_name}/src\"")),
            "worker-host freshness must hash the physical {package_name} inputs"
        );
        assert!(
            changed_gate.contains(&format!("\"backend/{package_name}/src/\"")),
            "changed-file production scan must include {package_name}"
        );
    }
}

#[test]
fn active_guidance_references_physical_implementation_owners() {
    let workspace_root = g5d_manifest_dir()
        .parent()
        .expect("g5d should be inside the generation workspace");
    let entrypoint = std::fs::read_to_string(
        workspace_root.join("../scripts/ci-host-xfs-entrypoint.sh"),
    )
    .expect("CI workspace-host entrypoint must be readable");
    assert!(entrypoint.contains("g5d host"));
    assert!(!entrypoint.contains("g5d worker"));
    let expectations = [
        (
            "../scripts/ci-host-xfs-entrypoint.sh",
            "g5d/src/workspace/materialize.rs",
            "g5d-foundation/src/workspace/materialize.rs",
        ),
        (
            "../infra/packer/g5-worker/files/g5d-host.env",
            "g5d/src/oci/cache.rs",
            "g5d-foundation/src/oci/cache.rs",
        ),
        (
            "../scripts/reset-dev-environment.sh",
            "g5-api/src/routes/platform.rs",
            "g5-api-routes/src/routes/platform.rs",
        ),
        (
            "../frontend/src/lib/routes.ts",
            "g5-api/src/routes/mod.rs",
            "g5-api-routes/src/routes/mod.rs",
        ),
        (
            "../frontend/src/lib/api-schemas.ts",
            "g5-api/src/routes/*.rs",
            "g5-api-routes/src/routes/*.rs",
        ),
        (
            "../frontend/src/lib/types.ts",
            "g5-api/src/routes/worker_hosts.rs",
            "g5-api-routes/src/routes/worker_hosts.rs",
        ),
        (
            "../frontend/src/lib/types.ts",
            "g5-api/src/routes/workspaces.rs",
            "g5-api-routes/src/routes/workspaces.rs",
        ),
        (
            "g5-api-core/src/cloud/startup_script.rs",
            "g5d/src/isolation/microvm.rs",
            "g5d-isolation/src/isolation/microvm.rs",
        ),
    ];

    for (file_name, legacy_path, physical_path) in expectations {
        let contents =
            std::fs::read_to_string(workspace_root.join(file_name)).unwrap_or_else(|error| {
                panic!("active guidance {file_name} must be readable: {error}")
            });
        assert!(
            !contents.contains(legacy_path),
            "active guidance {file_name} must not cite retired owner {legacy_path}"
        );
        assert!(
            contents.contains(physical_path),
            "active guidance {file_name} must cite physical owner {physical_path}"
        );
    }

}

#[test]
fn ci_cleanup_surfaces_clean_dummy_artifacts_for_every_extracted_package() {
    let workspace_root = g5d_manifest_dir()
        .parent()
        .expect("g5d should be inside the generation workspace");
    let contents =
        std::fs::read_to_string(workspace_root.join("../infra/docker/Dockerfile.ci-builder"))
            .expect("CI builder Dockerfile must be readable");
    let helper = std::fs::read_to_string(
        workspace_root.join("../scripts/clean-baked-rust-workspace-artifacts.sh"),
    )
    .expect("runtime baked-artifact cleanup helper must be readable");
    assert!(
        contents.contains("cargo test --locked -p g5-api-routes --lib \"guards::tests\""),
        "CI builder must execute route guards from their physical owner"
    );
    let clean_sections = contents
        .split("cargo clean \\")
        .skip(1)
        .map(|tail| tail.split("&& \\").next().unwrap_or(tail))
        .collect::<Vec<_>>();
    let extracted_packages = [
        "g5-contracts",
        "g5-auth",
        "g5-storage",
        "g5-semantic",
        "g5-scm",
        "g5-internal-api",
        "g5-api-foundation",
        "g5-api-data",
        "g5-api-core",
        "g5-api-routes",
        "g5d-foundation",
        "g5d-isolation",
        "g5d-agent",
        "g5d-host",
    ];

    assert_eq!(
        clean_sections.len(),
        2,
        "CI builder must retain separate dev/test and release cleanup stages"
    );
    for (index, section) in clean_sections.iter().enumerate() {
        for package_name in extracted_packages {
            assert!(
                package_flag_count(section, package_name) == 1,
                "CI builder clean stage {} must remove dummy {package_name} artifacts",
                index + 1
            );
        }
    }
    for package_name in extracted_packages {
        assert!(
            helper.lines().any(|line| line.trim() == package_name),
            "runtime cleanup helper must remove dummy {package_name} artifacts"
        );
    }
}


#[test]
fn cli_bootstrap_port_executes_the_validated_repository_snapshot_without_reload() {
    let workspace_root = g5d_manifest_dir()
        .parent()
        .expect("g5d should be inside the generation workspace");
    let cli = std::fs::read_to_string(workspace_root.join("g5d/src/cli/mod.rs"))
        .expect("CLI implementation must be readable");
    assert!(
        cli.contains("Host(HostArgs)"),
        "the retained CLI must expose the workspace host entrypoint"
    );
    for retired in ["task.rs", "host_up.rs", "worker.rs"] {
        assert!(
            !workspace_root.join("g5d/src/cli").join(retired).exists(),
            "retired per-task CLI module {retired} must stay deleted"
        );
    }
}

/// g5d publishes through the API and native Git only; it never links SQL or
/// the API crate's finalization code.
#[test]
fn g5d_remains_sql_blind() {
    let manifest: toml::Value = std::fs::read_to_string(g5d_manifest_dir().join("Cargo.toml"))
        .expect("g5d Cargo.toml must be readable")
        .parse()
        .expect("g5d Cargo.toml must parse");

    for table_name in ["dependencies", "dev-dependencies", "build-dependencies"] {
        let Some(table) = manifest.get(table_name).and_then(toml::Value::as_table) else {
            continue;
        };
        assert!(
            !table.contains_key("sqlx"),
            "g5d must not acquire SQL access through [{table_name}]"
        );
        assert!(
            !table.contains_key("g5-api"),
            "g5d must not link API-owned finalization code through [{table_name}]"
        );
    }
}

#[test]
fn guest_execution_inventory_has_only_retained_runtime_producers() {
    let workspace_root = g5d_manifest_dir()
        .parent()
        .expect("g5d should be inside the generation workspace");
    let metadata = load_workspace_metadata(&workspace_root.join("../Cargo.toml"));
    let shipped_g5d = metadata
        .packages
        .iter()
        .find(|package| package.name == "g5d")
        .expect("shipped g5d package must exist");
    let binaries = shipped_g5d
        .targets
        .iter()
        .filter(|target| target.kind.iter().any(|kind| kind == "bin"))
        .map(|target| target.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(binaries, ["g5d"], "retired guest binaries must stay absent");

    let cli = std::fs::read_to_string(workspace_root.join("g5d/src/cli/mod.rs"))
        .expect("CLI source must be readable");
    let dispatch = std::fs::read_to_string(workspace_root.join("g5d/src/cli/dispatch.rs"))
        .expect("CLI dispatcher source must be readable");
    let command_enum = cli
        .split_once("pub enum Command {")
        .and_then(|(_, rest)| rest.split_once("\n}"))
        .map(|(body, _)| body)
        .expect("g5d Command enum must be present");
    let variants = command_enum
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("//") && !line.starts_with('#'))
        .filter_map(|line| line.split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_').next())
        .collect::<BTreeSet<_>>();
    for retained in ["Dispatch", "Bootstrap", "AgentTurn", "PreviewServe"] {
        assert!(variants.contains(retained), "retained entrypoint {retained} is missing");
        assert!(
            dispatch.contains(&format!("Command::{retained}")),
            "retained entrypoint {retained} is not dispatched"
        );
    }
    for retired in ["Task", "Worker", "HostUp", "Push"] {
        assert!(
            !variants.contains(retired) && !dispatch.contains(&format!("Command::{retired}")),
            "retired guest producer {retired} must stay absent"
        );
    }
    // These were the per-task queue consumers and launchers. Keep their
    // physical absence in the same inventory as the shipped CLI and catalog.
    for retired_path in [
        "g5d-agent/src/inbox.rs",
        "g5d-agent/src/poll.rs",
        "g5d-agent/src/task_agent.rs",
        "g5d-agent/src/task_env.rs",
        "g5d-agent/src/runtime_keeper.rs",
        "g5d-host/src/assignment.rs",
        "g5d-host/src/agent_host.rs",
        "g5d-host/src/lifecycle.rs",
        "g5d/src/cli/task.rs",
        "g5d/src/cli/worker.rs",
        "g5d/src/cli/host_up.rs",
        "g5d/src/worker",
    ] {
        assert!(
            !workspace_root.join(retired_path).exists(),
            "retired queue consumer or launcher {retired_path} must stay absent"
        );
    }
    let api_vm = std::fs::read_to_string(
        workspace_root.join("g5-api-core/src/services/workspace/workspace_vm_service.rs"),
    )
    .expect("workspace VM producer must be readable");
    assert!(
        api_vm.contains("entrypoint: vec![\"/opt/g5/bin/g5d\".into(), \"dispatch\".into()]"),
        "workspace VM producer must launch the retained dispatcher"
    );
    let host_vm = std::fs::read_to_string(workspace_root.join("g5d-host/src/workspace_vm.rs"))
        .expect("workspace VM host launcher must be readable");
    assert!(
        host_vm.contains("spec.entrypoint != [\"/opt/g5/bin/g5d\", \"dispatch\"]"),
        "host launcher must reject retired guest runtime entrypoints"
    );

    let catalog: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(workspace_root.join("g5d-dispatch/modules/catalog.json"))
            .expect("module catalog must be readable"),
    )
    .expect("module catalog must remain valid JSON");
    let mut process_entries = catalog["modules"]
        .as_array()
        .expect("catalog modules")
        .iter()
        .filter_map(|module| {
            let entrypoint = module["entrypoint"].as_array()?;
            (entrypoint.first()?.as_str()? == "g5d").then(|| {
                (
                    module["module_id"].as_str().unwrap().to_owned(),
                    entrypoint
                        .iter()
                        .map(|value| value.as_str().unwrap().to_owned())
                        .collect::<Vec<_>>(),
                )
            })
        })
        .collect::<Vec<_>>();
    // Several pinned versions of one module are one execution producer.
    process_entries.dedup();
    assert_eq!(
        process_entries,
        [
            (
                "workspace.bootstrap".to_owned(),
                vec!["g5d".to_owned(), "bootstrap".to_owned()],
            ),
            (
                "agent.turn".to_owned(),
                vec!["g5d".to_owned(), "agent-turn".to_owned()],
            ),
            (
                "workspace.preview".to_owned(),
                vec!["g5d".to_owned(), "preview-serve".to_owned()],
            ),
        ],
        "catalog must not regain per-task worker or task-VM launchers"
    );

    let routes = std::fs::read_to_string(workspace_root.join("g5-api-routes/src/routes/mod.rs"))
        .expect("API route registry must be readable");
    assert!(routes.contains("pub mod workspace_preview;"));
    assert!(routes.contains("/api/v1/workspaces/:id/terminal"));
    assert!(!routes.contains("pub mod workers;"));
    assert!(!workspace_root.join("g5-api-routes/src/routes/workers.rs").exists());
}
