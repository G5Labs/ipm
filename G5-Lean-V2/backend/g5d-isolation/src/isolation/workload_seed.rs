//! Clean committed repository baselines. Never read or promote a guest disk.
//! Build outside the publication lock; Start either pins a complete seed or misses.
// Portable validation builds without the Linux preparation caller.
#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use super::{
    workload_root::{self, failure, helper, BackingGuard},
    workload_template::{self, Identity, Staging},
    WorkloadRepoScope,
};
use g5_contracts::error::G5Error;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Component, Path, PathBuf},
};

const MIB: u64 = 1024 * 1024;
const MAX_MANIFEST: u64 = 1024 * 1024;
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SeedInput {
    scope: WorkloadRepoScope,
    repositories: Vec<SeedRepository>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SeedRepository {
    repo_id: uuid::Uuid,
    /// A local committed Git bundle supplied by trusted delivery, never a VM root.
    bundle: PathBuf,
    commit: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct SeedIdentity {
    version: u32,
    image: Identity,
    scope: WorkloadRepoScope,
    commits: Vec<(uuid::Uuid, String)>,
}

fn normalized_scope(mut scope: WorkloadRepoScope) -> Result<WorkloadRepoScope, G5Error> {
    if scope.org_id.is_nil() || scope.repos.is_empty() || scope.repos.len() > 64 {
        return Err(failure(
            "repository seed requires a nonempty authorized scope",
        ));
    }
    scope.repos.sort_by_key(|repo| repo.repo_id);
    let mut ids = BTreeSet::new();
    let mut roots: Vec<&Path> = Vec::new();
    for repo in &scope.repos {
        let path = Path::new(&repo.root_path);
        if repo.repo_id.is_nil()
            || !ids.insert(repo.repo_id)
            || repo.root_path.is_empty()
            || !path
                .components()
                .all(|part| matches!(part, Component::Normal(_)))
            || path.starts_with(".g5")
            || roots
                .iter()
                .any(|root| path.starts_with(root) || root.starts_with(path))
            || repo.git_url.is_empty()
            || repo.git_url.contains(['\0', '\n', '\r'])
            || repo.git_url.contains('@')
        {
            return Err(failure("invalid or overlapping repository seed membership"));
        }
        roots.push(path);
    }
    Ok(scope)
}

fn scope_allows(seed: &WorkloadRepoScope, actual: &WorkloadRepoScope) -> bool {
    seed.org_id == actual.org_id
        && !seed.repos.is_empty()
        && seed.repos.iter().all(|repo| actual.repos.contains(repo))
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, G5Error> {
    let meta = std::fs::symlink_metadata(path)
        .map_err(|e| failure(format!("inspect seed manifest: {e}")))?;
    if !meta.is_file() || meta.len() > MAX_MANIFEST {
        return Err(failure("seed manifest must be a bounded regular file"));
    }
    serde_json::from_slice(
        &std::fs::read(path).map_err(|e| workload_root::io_failure(e, "repository_seed"))?,
    )
    .map_err(|e| failure(format!("invalid seed manifest: {e}")))
}

fn checked(command: &mut std::process::Command, operation: &str) -> Result<(), G5Error> {
    let output = command
        .output()
        .map_err(|e| failure(format!("{operation}: {e}")))?;
    if !output.status.success() {
        return Err(failure(format!(
            "{operation}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(())
}

#[cfg(all(test, target_os = "linux"))]
fn git(root: &Path) -> std::process::Command {
    let mut command = std::process::Command::new("git");
    // Do not inherit credential helpers, filters, hooks or per-user Git config.
    command
        .env_clear()
        .env("PATH", "/usr/local/bin:/usr/bin:/bin")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_LFS_SKIP_SMUDGE", "1")
        // A freshly materialized workspace belongs to the guest uid. Trust
        // only this host-selected path, never a global safe.directory wildcard.
        .arg("-c")
        .arg(format!("safe.directory={}", root.display()))
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "protocol.file.allow=always",
            "-C",
        ])
        .arg(root);
    command
}

fn seed_git() -> Result<g5_scm::native::NativeGit, G5Error> {
    use g5_scm::native::{HostGitEnvironment, NativeGit};
    NativeGit::host(
        HostGitEnvironment {
            search_path: std::env::var_os("PATH")
                .unwrap_or_else(|| "/usr/local/bin:/usr/bin:/bin".into()),
            home: None,
            xdg_config_home: None,
            global_config: Some("/dev/null".into()),
            system_config_path: None,
            system_config: false,
            transport: Default::default(),
            ssh_agent_socket: None,
            library_path: std::env::var_os("LD_LIBRARY_PATH"),
            dyld_library_path: None,
            ca_bundle: None,
            ca_directory: None,
        },
        std::time::Duration::from_secs(300),
    )
    .map_err(Into::into)
}

fn populate_work(input: &SeedInput, base: &Path, work: &Path) -> Result<(), G5Error> {
    std::fs::create_dir(work).map_err(|e| workload_root::io_failure(e, "repository_seed"))?;
    let git = seed_git()?;
    for member in &input.scope.repos {
        let source = input
            .repositories
            .iter()
            .find(|source| source.repo_id == member.repo_id)
            .ok_or_else(|| failure("seed input does not cover the member vector"))?;
        if !matches!(source.commit.len(), 40 | 64)
            || !source.commit.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(failure("seed commit must be an immutable Git object id"));
        }
        let bundle = base
            .join(&source.bundle)
            .canonicalize()
            .map_err(|e| failure(format!("resolve seed bundle: {e}")))?;
        if !std::fs::symlink_metadata(&bundle).is_ok_and(|m| m.is_file()) {
            return Err(failure("seed bundle must be a regular file"));
        }
        let repo = workload_root::ensure_directory(work, &member.root_path)?;
        git.prepare_bundle_checkout_blocking(&repo, &bundle, &source.commit)?;
        workload_template::remove_owned(&repo.join(".git/hooks"))?;
    }
    Ok(())
}

/// Invoked only by the explicit delivery preparation operation.
pub(super) fn prepare(
    template: &Path,
    size_mib: u32,
    reserve_mib: u64,
    manifest: &Path,
    mkfs: &Path,
    stage_only: bool,
) -> Result<(), G5Error> {
    let mut input: SeedInput = read_json(manifest)?;
    input.scope = normalized_scope(input.scope)?;
    let input_ids: BTreeSet<_> = input.repositories.iter().map(|repo| repo.repo_id).collect();
    if input_ids.len() != input.repositories.len()
        || input_ids != input.scope.repos.iter().map(|repo| repo.repo_id).collect()
    {
        return Err(failure(
            "seed bundles must cover exactly the authorized member vector",
        ));
    }
    let (cache, image) = workload_template::location(template, size_mib)?;
    let _generic = workload_template::lock(&cache, false)?;
    let _builder = workload_template::lock_named(&cache, "seed-build.lock", true, false)?;
    workload_template::inherit_builder_lock(&_generic)?;
    if let Some(builder) = _builder.as_ref() {
        workload_template::inherit_builder_lock(builder)?;
    }
    if !workload_template::complete(&cache, &image) {
        return Err(failure(
            "prepare the matching generic template before the repository seed",
        ));
    }
    let _ = reserve_mib; // Compatibility flag: preparation reserves no slot capacity.
    workload_template::remove_owned(&cache.join("repo-building"))?;
    workload_template::remove_owned(&cache.join("repo-prepared"))?;
    let stage = Staging(cache.join("repo-building"));
    std::fs::create_dir(&stage.0).map_err(|e| workload_root::io_failure(e, "repository_seed"))?;
    let work = stage.0.join("work");
    populate_work(&input, manifest.parent().unwrap_or(Path::new(".")), &work)?;
    let root = stage.0.join("rootfs");
    checked(
        helper("cp")
            .args(["-a", "--reflink=always", "--"])
            .arg(template.join("rootfs"))
            .arg(&root),
        "stage repository environment",
    )?;
    workload_root::stage_workspace(&root, &work)?;
    workload_template::remove_owned(&root.join(".g5"))?;
    workload_root::ensure_directory(&root, ".g5/workload-root")?;
    workload_root::ensure_directory(&root, "etc")?;
    for name in ["hosts", "resolv.conf"] {
        workload_template::remove_owned(&root.join("etc").join(name))?;
    }
    let guard = workload_root::prepare_disk(&root, size_mib, mkfs)?;
    let disk = stage.0.join("image.ext4");
    std::fs::rename(&guard.0, &disk)
        .map_err(|e| workload_root::io_failure(e, "repository_seed"))?;
    drop(guard);
    checked(
        helper("e2fsck").args(["-f", "-n"]).arg(&disk),
        "verify repository seed ext4",
    )?;
    std::fs::set_permissions(&disk, std::fs::Permissions::from_mode(0o400))
        .map_err(|e| workload_root::io_failure(e, "repository_seed"))?;
    let mut commits: Vec<_> = input
        .repositories
        .into_iter()
        .map(|repo| (repo.repo_id, repo.commit))
        .collect();
    commits.sort();
    let identity = SeedIdentity {
        version: 2,
        image,
        scope: input.scope,
        commits,
    };
    let identity_path = stage.0.join("identity.json");
    std::fs::write(
        &identity_path,
        serde_json::to_vec(&identity).map_err(|e| failure(e.to_string()))?,
    )
    .map_err(|e| workload_root::io_failure(e, "repository_seed"))?;
    workload_template::remove_owned(&root)?;
    workload_template::remove_owned(&work)?;
    for path in [&disk, &identity_path, &stage.0] {
        File::open(path)
            .and_then(|f| f.sync_all())
            .map_err(|e| workload_root::io_failure(e, "repository_seed"))?;
    }
    if stage_only {
        std::fs::rename(&stage.0, cache.join("repo-prepared"))
            .map_err(|e| workload_root::io_failure(e, "repository_seed"))?;
        File::open(&cache)
            .and_then(|f| f.sync_all())
            .map_err(|e| workload_root::io_failure(e, "repository_seed"))?;
        return Ok(());
    }
    publish_stage(&cache, &stage.0)?;
    File::open(&cache)
        .and_then(|f| f.sync_all())
        .map_err(|e| workload_root::io_failure(e, "repository_seed"))?;
    tracing::info!(size_mib, "prepared scoped repository block template");
    Ok(())
}

fn publish_stage(cache: &Path, stage: &Path) -> Result<(), G5Error> {
    let _publication = workload_template::lock_named(cache, "seed-publication.lock", true, false)?;
    workload_template::remove_owned(&cache.join("repo-current"))?;
    std::fs::rename(stage, cache.join("repo-current"))
        .map_err(|e| workload_root::io_failure(e, "repository_seed"))?;
    File::open(cache)
        .and_then(|f| f.sync_all())
        .map_err(|e| workload_root::io_failure(e, "repository_seed"))
}

/// The daemon has just refreshed the entire read grant. Match the exact staged
/// generation under the builder lock; a different builder cannot be promoted.
pub(super) fn publish_prepared(
    template: &Path,
    size_mib: u32,
    scope: WorkloadRepoScope,
    mut commits: Vec<(uuid::Uuid, String)>,
) -> Result<(), G5Error> {
    let (cache, image) = workload_template::location(template, size_mib)?;
    let _generic = workload_template::lock(&cache, false)?;
    let _builder = workload_template::lock_named(&cache, "seed-build.lock", true, false)?;
    let stage = cache.join("repo-prepared");
    let identity: SeedIdentity = read_json(&stage.join("identity.json"))?;
    commits.sort();
    if identity
        != (SeedIdentity {
            version: 2,
            image,
            scope: normalized_scope(scope)?,
            commits,
        })
    {
        return Err(failure(
            "prepared seed generation changed before publication",
        ));
    }
    publish_stage(&cache, &stage)
}

pub(super) fn eligible(
    template: &Path,
    size_mib: u32,
    scope: &WorkloadRepoScope,
) -> Result<bool, G5Error> {
    let (cache, image) = workload_template::location(template, size_mib)?;
    if !cache.exists() {
        return Ok(false);
    }
    let Some(_publication) =
        workload_template::lock_named(&cache, "seed-publication.lock", false, true)?
    else {
        return Ok(false);
    };
    Ok(
        read_json::<SeedIdentity>(&cache.join("repo-current/identity.json")).is_ok_and(
            |identity| {
                identity.version == 2
                    && identity.image == image
                    && scope_allows(&identity.scope, scope)
            },
        ),
    )
}

/// An optimization hint only: callers must refresh authorization before reuse.
/// Compare the exact assignment, including commits, and the completed disk pair.
pub(super) fn current_generation(
    template: &Path,
    size_mib: u32,
    scope: WorkloadRepoScope,
    mut commits: Vec<(uuid::Uuid, String)>,
) -> Result<bool, G5Error> {
    let (cache, image) = workload_template::location(template, size_mib)?;
    if !cache.exists() {
        return Ok(false);
    }
    let Some(_generic) = workload_template::lock_named(&cache, "lock", false, true)? else {
        return Ok(false);
    };
    let Some(_publication) =
        workload_template::lock_named(&cache, "seed-publication.lock", false, true)?
    else {
        return Ok(false);
    };
    commits.sort();
    let expected = SeedIdentity {
        version: 2,
        image,
        scope: normalized_scope(scope)?,
        commits,
    };
    let current = cache.join("repo-current");
    Ok(read_json::<SeedIdentity>(&current.join("identity.json"))
        .is_ok_and(|identity| identity == expected)
        && std::fs::symlink_metadata(current.join("image.ext4")).is_ok_and(|m| {
            m.is_file() && m.len() == u64::from(size_mib) * MIB && m.mode() & 0o222 == 0
        }))
}

/// Scope is supplied by the validated host spec; no guest files select seeds.
pub(super) fn try_clone(
    template: &Path,
    root: &Path,
    size_mib: u32,
    scope: WorkloadRepoScope,
    authorized_stores: &BTreeMap<uuid::Uuid, PathBuf>,
) -> Result<Option<BackingGuard>, G5Error> {
    let scope = match normalized_scope(scope) {
        Ok(scope) => scope,
        Err(_) => return Ok(None), // No compatible seed; preserve ordinary Git support.
    };
    let (cache, image) = workload_template::location(template, size_mib)?;
    if !cache.exists() {
        return Ok(None);
    }
    let _generic = workload_template::lock(&cache, false)?;
    let Some(_publication) =
        workload_template::lock_named(&cache, "seed-publication.lock", false, true)?
    else {
        return Ok(None);
    };
    let current = cache.join("repo-current");
    if !std::fs::symlink_metadata(&current).is_ok_and(|m| m.is_dir()) {
        return Ok(None);
    }
    let Ok(identity) = read_json::<SeedIdentity>(&current.join("identity.json")) else {
        return Ok(None);
    };
    if identity.version != 2 || identity.image != image || !scope_allows(&identity.scope, &scope) {
        tracing::info!("repository template scope or image mismatch; using generic template");
        return Ok(None);
    }
    // Delivery labels alone cannot establish that the bundled source belongs
    // to an authorized repository. Require its pinned commit in the current
    // independently authorized bare store before attaching any seeded blocks.
    if identity.commits.len() != identity.scope.repos.len() {
        return Err(failure("seed commit vector does not match its scope"));
    }
    let mut commit_ids = BTreeSet::new();
    let git = seed_git()?;
    for (repo_id, commit) in &identity.commits {
        let member = identity
            .scope
            .repos
            .iter()
            .find(|repo| repo.repo_id == *repo_id)
            .ok_or_else(|| failure("seed commit names a nonmember"))?;
        if !commit_ids.insert(*repo_id)
            || !matches!(commit.len(), 40 | 64)
            || !commit.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(failure("invalid seed commit identity"));
        }
        let Some(store) = authorized_stores.get(&member.repo_id) else {
            return Ok(None);
        };
        if git.object_kind_blocking(store, commit)?
            != Some(g5_scm::native::NativeObjectKind::Commit)
        {
            tracing::info!(
                "repository baseline absent from authorized Git objects; using generic template"
            );
            return Ok(None);
        }
    }
    let disk = current.join("image.ext4");
    if !std::fs::symlink_metadata(&disk).is_ok_and(|m| {
        m.is_file()
            && m.len() == u64::from(size_mib) * MIB
            && m.uid() == unsafe { libc::geteuid() }
            && m.mode() & 0o222 == 0
    }) {
        return Err(failure("incomplete repository template pair"));
    }
    let guard = BackingGuard(root.join(workload_root::BACKING_REL));
    if !workload_template::reflink(&disk, &guard.0)? {
        return Ok(None);
    }
    tracing::info!("pinned repository block template for trusted guest Git restore");
    Ok(Some(guard))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_is_exact_canonical_and_rejects_ambiguous_membership() {
        let member = super::super::WorkloadRepoMember {
            repo_id: uuid::Uuid::new_v4(),
            root_path: "repo".into(),
            git_url: "https://api/git/one".into(),
        };
        let scope = WorkloadRepoScope {
            org_id: uuid::Uuid::new_v4(),
            repos: vec![member.clone()],
        };
        assert_eq!(normalized_scope(scope.clone()).unwrap(), scope);
        for root in ["", "/work", "../escape", ".g5/seed"] {
            let mut invalid = scope.clone();
            invalid.repos[0].root_path = root.into();
            assert!(normalized_scope(invalid).is_err());
        }
        let mut overlap = scope.clone();
        let mut child = member;
        child.repo_id = uuid::Uuid::new_v4();
        child.root_path = "repo/child".into();
        overlap.repos.push(child);
        assert!(normalized_scope(overlap).is_err());
        let mut other = scope.clone();
        other.org_id = uuid::Uuid::new_v4();
        assert_ne!(normalized_scope(other).unwrap(), scope);
        let mut extra = scope.clone();
        extra.repos.push(super::super::WorkloadRepoMember {
            repo_id: uuid::Uuid::new_v4(),
            root_path: "docs".into(),
            git_url: "https://api/git/docs".into(),
        });
        assert!(scope_allows(&scope, &extra));
        assert!(!scope_allows(&extra, &scope));
    }

    #[test]
    fn completed_generation_reuse_requires_exact_scope_commits_and_disk() {
        let temp = tempfile::tempdir().unwrap();
        let template = temp.path().join("templates/sha256").join("a".repeat(64));
        std::fs::create_dir_all(template.join("rootfs")).unwrap();
        std::fs::write(template.join(".complete"), b"ok").unwrap();
        let (cache, image) = workload_template::location(&template, 32).unwrap();
        let repo_id = uuid::Uuid::new_v4();
        let scope = WorkloadRepoScope {
            org_id: uuid::Uuid::new_v4(),
            repos: vec![super::super::WorkloadRepoMember {
                repo_id,
                root_path: "code".into(),
                git_url: "https://api/git/fixture".into(),
            }],
        };
        let commits = vec![(repo_id, "a".repeat(40))];
        assert!(!current_generation(&template, 32, scope.clone(), commits.clone()).unwrap());
        let lock = workload_template::lock(&cache, true).unwrap();
        let current = cache.join("repo-current");
        std::fs::create_dir(&current).unwrap();
        let identity = SeedIdentity {
            version: 2,
            image,
            scope: scope.clone(),
            commits: commits.clone(),
        };
        std::fs::write(
            current.join("identity.json"),
            serde_json::to_vec(&identity).unwrap(),
        )
        .unwrap();
        let disk = File::create(current.join("image.ext4")).unwrap();
        disk.set_len(32 * MIB).unwrap();
        std::fs::set_permissions(
            current.join("image.ext4"),
            std::fs::Permissions::from_mode(0o400),
        )
        .unwrap();
        drop(lock);
        // This is a nonblocking hint. Another parallel test can fork while the
        // fixture lock is open; CLOEXEC releases its copy when exec completes.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while !current_generation(&template, 32, scope.clone(), commits.clone()).unwrap() {
            assert!(
                std::time::Instant::now() < deadline,
                "completed generation stayed ineligible"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(!current_generation(
            &template,
            32,
            scope.clone(),
            vec![(repo_id, "b".repeat(40))]
        )
        .unwrap());
        let mut other = scope.clone();
        other.org_id = uuid::Uuid::new_v4();
        assert!(!current_generation(&template, 32, other, commits.clone()).unwrap());
        disk.set_len(1).unwrap();
        assert!(!current_generation(&template, 32, scope, commits).unwrap());
    }

    #[test]
    #[cfg(target_os = "linux")]
    #[ignore = "requires Linux e2fsprogs and G5_TEST_REFLINK_ROOT on XFS"]
    fn real_seed_scope_proof_and_private_clones() {
        let base = std::env::var_os("G5_TEST_REFLINK_ROOT").expect("XFS test root");
        let temp = tempfile::tempdir_in(base).unwrap();
        let base = temp.path();
        let template = base.join("templates/sha256").join("a".repeat(64));
        std::fs::create_dir_all(template.join("rootfs/usr/bin")).unwrap();
        for name in ["rsync", "sha256sum"] {
            std::fs::write(template.join("rootfs/usr/bin").join(name), b"fixture").unwrap();
        }
        std::fs::write(template.join(".complete"), b"ok").unwrap();
        let source = base.join("source");
        std::fs::create_dir(&source).unwrap();
        checked(git(&source).args(["init", "-q"]), "init fixture").unwrap();
        checked(
            git(&source).args(["config", "user.name", "fixture"]),
            "identity",
        )
        .unwrap();
        checked(
            git(&source).args(["config", "user.email", "fixture@example.test"]),
            "identity",
        )
        .unwrap();
        std::fs::write(source.join("large-unchanged"), vec![b'x'; 2 * 1024 * 1024]).unwrap();
        std::fs::write(source.join("deleted"), b"remove this").unwrap();
        std::fs::write(source.join("same-size"), b"before").unwrap();
        std::fs::write(source.join("file-to-dir"), b"old file").unwrap();
        std::fs::create_dir(source.join("dir-to-link")).unwrap();
        std::fs::write(source.join("dir-to-link/child"), b"old child").unwrap();
        checked(git(&source).args(["add", "."]), "add fixture").unwrap();
        checked(
            git(&source).args(["commit", "-qm", "baseline"]),
            "commit fixture",
        )
        .unwrap();
        let output = git(&source).args(["rev-parse", "HEAD"]).output().unwrap();
        let commit = String::from_utf8(output.stdout).unwrap().trim().to_owned();
        let bundle = base.join("seed.bundle");
        checked(
            git(&source)
                .args(["bundle", "create"])
                .arg(&bundle)
                .arg("HEAD"),
            "bundle fixture",
        )
        .unwrap();
        let repo_id = uuid::Uuid::new_v4();
        let scope = WorkloadRepoScope {
            org_id: uuid::Uuid::new_v4(),
            repos: vec![super::super::WorkloadRepoMember {
                repo_id,
                root_path: "code".into(),
                git_url: "https://api/git/fixture".into(),
            }],
        };
        let input = SeedInput {
            scope: scope.clone(),
            repositories: vec![SeedRepository {
                repo_id,
                bundle,
                commit: commit.clone(),
            }],
        };
        let manifest = base.join("seed.json");
        std::fs::write(&manifest, serde_json::to_vec(&input).unwrap()).unwrap();
        let mkfs = workload_root::find_mkfs().unwrap();
        workload_template::prewarm(&template, 64, 0, mkfs).unwrap();
        prepare(&template, 64, 0, &manifest, mkfs, false).unwrap();
        let (cache, _) = workload_template::location(&template, 64).unwrap();
        let current = cache.join("repo-current");
        assert!(
            !current.join("work").exists(),
            "no duplicate comparison tree"
        );
        let root = base.join("private");
        std::fs::create_dir_all(root.join(".g5")).unwrap();
        let stores = BTreeMap::from([(repo_id, source.clone())]);
        for case in 0..4 {
            let mut invalid = scope.clone();
            match case {
                0 => invalid.org_id = uuid::Uuid::new_v4(),
                1 => invalid.repos[0].repo_id = uuid::Uuid::new_v4(),
                2 => invalid.repos[0].root_path = "other".into(),
                _ => invalid.repos[0].git_url = "https://other/git/fixture".into(),
            }
            assert!(try_clone(&template, &root, 64, invalid, &stores)
                .unwrap()
                .is_none());
            assert!(!root.join(workload_root::BACKING_REL).exists());
        }
        assert!(
            try_clone(&template, &root, 64, scope.clone(), &BTreeMap::new())
                .unwrap()
                .is_none()
        );
        let identity_path = current.join("identity.json");
        let original = std::fs::read(&identity_path).unwrap();
        let mut wrong: SeedIdentity = serde_json::from_slice(&original).unwrap();
        wrong.commits[0].1 = "0".repeat(40);
        std::fs::write(&identity_path, serde_json::to_vec(&wrong).unwrap()).unwrap();
        assert!(try_clone(&template, &root, 64, scope.clone(), &stores)
            .unwrap()
            .is_none());
        std::fs::write(&identity_path, original).unwrap();
        let guard = try_clone(&template, &root, 64, scope.clone(), &stores)
            .unwrap()
            .unwrap();
        assert!(
            !root.join("work").exists(),
            "seed hit needs no host checkout"
        );
        let before = std::fs::read(current.join("image.ext4")).unwrap();
        use std::os::unix::fs::FileExt;
        std::fs::OpenOptions::new()
            .write(true)
            .open(&guard.0)
            .unwrap()
            .write_all_at(b"private", 4096)
            .unwrap();
        assert_eq!(std::fs::read(current.join("image.ext4")).unwrap(), before);
        drop(guard);
        let _publication =
            workload_template::lock_named(&cache, "seed-publication.lock", true, false).unwrap();
        let start = std::time::Instant::now();
        assert!(try_clone(&template, &root, 64, scope, &stores)
            .unwrap()
            .is_none());
        assert!(
            start.elapsed() < std::time::Duration::from_secs(1),
            "Start cannot wait for seed publication"
        );
    }
}
