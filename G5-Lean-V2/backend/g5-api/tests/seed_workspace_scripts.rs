const TINYDB_SOURCE_SEED: &str = include_str!("../../../scripts/seed-tinydb-workspace.sh");
const TINYDB_COMMIT0_SEED: &str = include_str!("../../../scripts/seed-tinydb-commit0-workspace.sh");
const DEV_ORG_SEED: &str = include_str!("../../../scripts/seed-dev-org.sh");
const CI_ORG_SEED: &str = include_str!("../../../scripts/seed-ci-org.sh");
const PUBLISH_SEED_REFS: &str = include_str!("../../../scripts/publish-seed-workspace-refs.sh");
const TEST_WORKSPACE_SEED: &str = include_str!("../../../scripts/seed-test-workspace.sh");
const ONTOLOGY_EXAMPLES_SEED: &str = include_str!("../../../scripts/seed-ontology-examples.sh");
const CODEX_SEED: &str = include_str!("../../../scripts/seed-codex.sh");
const COMPLIANCE_SEED: &str = include_str!("../../../scripts/seed-compliance-workspace.sh");
const PROD_ORG_SEED: &str = include_str!("../../../scripts/seed-prod-org.sh");
const DEMO_MULTI_SEED: &str = include_str!("../../../scripts/seed-demo-multi.sh");
const DEMO_SEED: &str = include_str!("../../../scripts/seed-demo.sh");
const G5_WORKSPACE_SEED: &str = include_str!("../../../scripts/seed-g5-workspace.sh");
const NATIVE_PUBLISH_LIB: &str = include_str!("../../../scripts/lib/native-publish.sh");
const SELF_HOSTED_SEED: &str = include_str!("../../../infra/self-hosted/scripts/g5-selfhost.sh");
const CODEX_CONTROL_FRAMES: &str =
    include_str!("../../../tests/e2e/src/scenarios/codex_control_frames.rs");

fn assert_chat_session_insert_columns_are_current(source_name: &str, source: &str) {
    let mut remaining = source;
    while let Some(offset) = remaining.find("INSERT INTO chat_sessions") {
        let insert = &remaining[offset..];
        let header_end = insert
            .find("VALUES")
            .or_else(|| insert.find("SELECT"))
            .unwrap_or_else(|| panic!("{source_name} has an unrecognized chat_sessions insert"));
        let header = &insert[..header_end];
        for removed_column in ["session_type", "status"] {
            assert!(
                !header.split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
                    .any(|word| word == removed_column),
                "{source_name} still writes migration-008 removed chat_sessions column {removed_column}"
            );
        }
        remaining = &insert[header_end..];
    }
}

#[test]
fn test_workspace_seed_publishes_the_selected_workspace_ref() {
    assert!(TEST_WORKSPACE_SEED.contains("publish-seed-workspace-refs.sh"));
    assert!(PUBLISH_SEED_REFS.contains("HEAD:refs/heads/$branch"));
    assert!(TEST_WORKSPACE_SEED.contains("refs/heads/${WORKSPACE_NAME}"));
    assert!(!TEST_WORKSPACE_SEED.contains("docs_branch_name ="));
}

#[test]
fn seed_scripts_do_not_write_columns_removed_by_the_mvp_cutover() {
    let scripts_dir = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../scripts"));
    for entry in std::fs::read_dir(scripts_dir).expect("list scripts directory") {
        let path = entry.expect("scripts directory entry").path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !name.starts_with("seed-") || !name.ends_with(".sh") {
            continue;
        }
        let script = std::fs::read_to_string(&path).expect("read seed script");
        for removed_column in ["authority_scope_kind", "authority_scope_id", "session_type"] {
            assert!(
                !script.contains(removed_column),
                "{name} still writes migration-008 removed column {removed_column}"
            );
        }
        assert_chat_session_insert_columns_are_current(name, &script);
    }
    assert_chat_session_insert_columns_are_current("codex_control_frames.rs", CODEX_CONTROL_FRAMES);
}

#[test]
fn dedicated_seeders_create_exact_membership_and_git_parent_cuts() {
    for (name, script) in [
        ("TinyDB source", TINYDB_SOURCE_SEED),
        ("TinyDB commit0", TINYDB_COMMIT0_SEED),
    ] {
        assert!(script.contains("seed_create_fixture_workspace"), "{name}");
        assert!(script.contains("seed_capture_fixture_parent"), "{name}");
        assert!(script.contains("INSERT INTO workspace_repos"), "{name}");
        for removed in [
            "workspace_branches",
            "workspace_branch_repos",
            "head_sha_cache",
        ] {
            assert!(!script.contains(removed), "{name} still uses {removed}");
        }
    }
    let helper = include_str!("../../../scripts/seed-workspace-model.sh");
    assert!(helper.contains("git -c safe.directory=\"*\" -C \"$repo\" update-ref"));
    assert!(helper.contains("SET cloned_from_id"));
    assert!(helper.contains("INSERT INTO workspace_repos"));
    assert!(helper.contains("INSERT INTO workspace_settings (workspace_id, org_id)"));
    assert!(helper.contains("\" >/dev/null || return"));
}

#[test]
fn checked_fork_reseed_deletes_git_refs_before_discarding_membership() {
    let delete_refs = TEST_WORKSPACE_SEED
        .find("api_authed_post \"/api/v1/workspaces/${OLD_WS_ID}/delete\"")
        .expect("reseed must use the lifecycle service to remove owned Git refs");
    let delete_members = TEST_WORKSPACE_SEED
        .find("DELETE FROM workspace_repos WHERE workspace_id = '${OLD_WS_ID}'")
        .unwrap();
    assert!(delete_refs < delete_members);
}

#[test]
fn seed_scripts_do_not_reference_the_deleted_registry() {
    // The Lean baseline has no head-operation registry: a seed script that
    // still reads or deletes its rows fails against the current schema.
    let forbidden = [
        concat!("workspace_head_", "operation"),
        concat!("authority_advance", "_seq"),
        concat!("pr1884", "_"),
        concat!("refs/g5/", "staged"),
        concat!("refs/g5/", "committed"),
    ];
    let scripts_dir = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../scripts"));
    let mut seed_scripts = std::fs::read_dir(scripts_dir)
        .expect("list scripts directory")
        .map(|entry| entry.expect("scripts directory entry").path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("seed-") && name.ends_with(".sh"))
        })
        .collect::<Vec<_>>();
    seed_scripts.sort();
    assert!(
        seed_scripts
            .iter()
            .any(|path| path.ends_with("seed-test-workspace.sh")),
        "the seed script inventory must include seed-test-workspace.sh"
    );
    for path in seed_scripts {
        let script = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        for literal in forbidden {
            assert!(
                !script.contains(literal),
                "{} must not reference {literal}; the head-operation registry and its refs were deleted",
                path.display()
            );
        }
    }
}

#[test]
fn seed_scripts_do_not_write_the_deleted_workspace_status_column() {
    let mut scripts = vec![
        ("seed-prod-org.sh".to_owned(), PROD_ORG_SEED.to_owned()),
        ("seed-demo-multi.sh".to_owned(), DEMO_MULTI_SEED.to_owned()),
        ("g5-selfhost.sh".to_owned(), SELF_HOSTED_SEED.to_owned()),
    ];
    let scripts_dir = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../scripts"));
    scripts.extend(
        std::fs::read_dir(scripts_dir)
            .expect("list scripts directory")
            .map(|entry| entry.expect("scripts directory entry").path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("seed-") && name.ends_with(".sh"))
            })
            .map(|path| {
                let name = path
                    .file_name()
                    .expect("seed script file name")
                    .to_string_lossy()
                    .into_owned();
                let script = std::fs::read_to_string(&path)
                    .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
                (name, script)
            }),
    );

    for (name, script) in scripts {
        for statement in script.split(';') {
            let normalized = statement.split_whitespace().collect::<Vec<_>>().join(" ");
            if normalized.contains("INSERT INTO workspaces")
                || normalized.contains("INSERT INTO public.workspaces")
                || normalized.contains("UPDATE workspaces")
                || normalized.contains("UPDATE public.workspaces")
            {
                let mentions_status = normalized
                    .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
                    .any(|token| token == "status");
                assert!(
                    !mentions_status,
                    "{name} must not write workspaces.status; migration 008 removes that column: {normalized}"
                );
            }
        }
    }
}

#[test]
fn destructive_workspace_reseeds_clear_authoritative_dependencies_and_remove_children() {
    let scripts = [
        ("seed-tinydb-workspace.sh", TINYDB_SOURCE_SEED),
        ("seed-tinydb-commit0-workspace.sh", TINYDB_COMMIT0_SEED),
        ("seed-test-workspace.sh", TEST_WORKSPACE_SEED),
        ("seed-ontology-examples.sh", ONTOLOGY_EXAMPLES_SEED),
        ("seed-compliance-workspace.sh", COMPLIANCE_SEED),
    ];
    let p4_children = [
        "chat_sessions",
        "session_inputs",
        "provider_requests",
        "task_attempts",
        "workspace_forks",
        "workspace_ownership_transfers",
        "worker_vm_events",
        "workspace_vms",
        "workspace_flushes",
    ];

    for (name, script) in scripts {
        let parent_delete = script
            .find("DELETE FROM workspaces")
            .unwrap_or_else(|| panic!("{name} must delete its fixture workspace"));
        let task_delete = script
            .find("DELETE FROM tasks")
            .unwrap_or_else(|| panic!("{name} must delete its fixture tasks"));
        let dependency_clear = script
            .find("DELETE FROM task_dependencies")
            .unwrap_or_else(|| panic!("{name} must clear authoritative task dependency rows"));
        assert!(
            dependency_clear < task_delete && dependency_clear < parent_delete,
            "{name} must clear authoritative task dependencies before deleting tasks or workspace"
        );
        assert!(
            !script.contains("UPDATE tasks SET depends_on"),
            "{name} must not treat the retired dependency array as authoritative"
        );
        for table in p4_children {
            let child_delete = script
                .find(&format!("DELETE FROM {table}"))
                .unwrap_or_else(|| panic!("{name} must delete P4.1 child table {table}"));
            assert!(
                child_delete < parent_delete,
                "{name} must delete {table} before its workspace"
            );
        }
        assert!(
            !script.contains("psql_exec_quiet \"DELETE FROM workspaces"),
            "{name} must fail closed if P4.1 children still block workspace cleanup"
        );
    }
}

#[test]
fn commit0_reseed_storage_cleanup_targets_only_fixture_owned_repos() {
    let old_repo_query_start = TINYDB_COMMIT0_SEED
        .find("OLD_REPO_IDS=$(psql_exec")
        .expect("commit0 seed must collect old fixture repo ids");
    let old_repo_query = &TINYDB_COMMIT0_SEED[old_repo_query_start..];

    assert!(
        old_repo_query.contains("JOIN repos repo ON repo.id = branch.repo_id")
            && old_repo_query
                .contains("AND repo.slug IN ('${CODE_REPO_SLUG}', '${DOCS_REPO_SLUG}')"),
        "commit0 reseed must not include the organization shared-docs repo in volume cleanup"
    );
}

#[test]
fn test_workspace_reseed_refuses_to_delete_other_workspace_history() {
    let owned_repo_query_start = TEST_WORKSPACE_SEED
        .find("OLD_REPO_IDS=$(psql_exec")
        .expect("test workspace seed must identify fixture-owned repositories");
    let owned_repo_query_end = TEST_WORKSPACE_SEED[owned_repo_query_start..]
        .find("# ── Cancel running tasks")
        .map(|offset| owned_repo_query_start + offset)
        .expect("fixture-owned repository query must end before task cleanup");
    let owned_repo_query = &TEST_WORKSPACE_SEED[owned_repo_query_start..owned_repo_query_end];
    let cleanup_start = TEST_WORKSPACE_SEED
        .find("for ORPHAN_SLUG in")
        .expect("test workspace seed must clean orphaned fixture repos");
    let cleanup = &TEST_WORKSPACE_SEED[cleanup_start..];
    assert!(
        owned_repo_query.contains("NOT EXISTS (\n        SELECT 1 FROM workspace_repos other")
            && owned_repo_query.contains("other.workspace_id <> '${OLD_WS_ID}'")
            && owned_repo_query.contains("NOT EXISTS (\n        SELECT 1 FROM workspaces other")
            && owned_repo_query.contains("other.id <> '${OLD_WS_ID}'")
            && cleanup.contains(
            "if [ \"$ORPHAN_WS_COUNT\" != \"0\" ] || [ \"$ORPHAN_DOCS_REFERENCE_COUNT\" != \"0\" ]; then",
        ),
        "reseed must exclude shared repositories from cleanup and refuse orphan cleanup while any workspace branch or shared-docs reference remains"
    );
    assert!(
        cleanup.contains("refusing destructive cleanup")
            && cleanup.contains("G5_E2E_TEST_WORKSPACE_NAME")
            && !cleanup.contains("DELETE FROM workspaces"),
        "orphan cleanup must fail closed and direct callers to a fresh scope instead of deleting workspace history"
    );
}

#[test]
fn commit0_seed_publishes_a_current_workspace_project_doc_launch_target() {
    assert!(
        TINYDB_COMMIT0_SEED.contains("PROJECT_DOC_PATH=\"docs/${WS_ID}/tinydb.md\"")
            && TINYDB_COMMIT0_SEED
                .contains("cp /seed/docs/designs/tinydb.md '${DOCS_BASE}/${PROJECT_DOC_PATH}'",),
        "commit0 design must be committed under the current workspace Project Docs prefix"
    );
    assert!(
        !TINYDB_COMMIT0_SEED.contains("cp -R /seed/docs/. '${DOCS_BASE}/'"),
        "commit0 docs repo must not retain the legacy root-level designs path"
    );
    assert!(
        TINYDB_COMMIT0_SEED.contains("resolve_commit0_project_doc_id()")
            && TINYDB_COMMIT0_SEED.contains(".repoId == $repo and .filePath == $path")
            && TINYDB_COMMIT0_SEED.contains("/docs/by-path?path=${encoded_path}")
            && TINYDB_COMMIT0_SEED.contains(
                "resolve_commit0_project_doc_id \"$WS_ID\" \"$DOCS_REPO_ID\" \"$PROJECT_DOC_PATH\"",
            )
            && TINYDB_COMMIT0_SEED.contains("expected a readable TinyDB Project Doc"),
        "commit0 seed must fail closed unless the document file reader resolves the repository-qualified launch target"
    );
    assert!(
        TINYDB_COMMIT0_SEED.contains("G5_TINYDB_COMMIT0_DOC_ID=${DOC_ID}")
            && TINYDB_COMMIT0_SEED.contains("G5_TINYDB_COMMIT0_DOC_PATH=${PROJECT_DOC_PATH}"),
        "commit0 seed must export the current Project Doc launch handles"
    );
}

#[test]
fn dev_org_seed_verifies_canonical_docs_authority_without_repairing_it() {
    assert!(
        DEV_ORG_SEED.contains("/api/v1/workspaces/${WS_ID}/branches")
            && DEV_ORG_SEED.contains(".repoId == $repo")
            && DEV_ORG_SEED.contains(".materializationStatus == \"materialized\"")
            && DEV_ORG_SEED.contains(".headCommit"),
        "dev org seed must read the selected docs Git head from the member API"
    );
    assert!(
        !DEV_ORG_SEED.contains("UPDATE workspace_branches")
            && !DEV_ORG_SEED.contains("UPDATE workspace_branch_repos")
            && !DEV_ORG_SEED.contains("git init")
            && !DEV_ORG_SEED.contains("git push"),
        "dev org seed must not repair repository authority or refs outside the canonical finalizer"
    );
}

#[test]
fn ci_org_seed_provisions_db_only_hosts_with_an_explicit_immutable_image_pin() {
    assert!(
        CI_ORG_SEED.contains("G5_E2E_HOST_BASE_IMAGE_REF")
            && CI_ORG_SEED.contains("g5d-worker-ci@sha256:")
            && CI_ORG_SEED.contains("\\\"baseImageRef\\\":\\\"${HOST_BASE_IMAGE_REF}\\\""),
        "DB-only CI host registration must satisfy the fail-closed image-source contract with an explicit immutable pin"
    );
}

#[test]
fn ci_org_seed_pins_the_workspace_guest_image_before_resume() {
    assert!(
        CI_ORG_SEED.contains("G5_E2E_WORKSPACE_BASE_IMAGE")
            && CI_ORG_SEED.contains("docker image inspect")
            && CI_ORG_SEED.contains("/worker-hosts/image-policy")
            && CI_ORG_SEED.contains("resolvedImageRef"),
        "seed-ci-org.sh must install the digest-pinned org default consumed by workspace Resume"
    );
}

#[test]
fn ci_org_seed_creates_shared_demo_through_checked_fork_api() {
    let base_lookup = CI_ORG_SEED
        .find("BASE_WS_ID=$(psql_exec")
        .expect("seed-ci-org.sh must resolve the API-created Base workspace");
    let fixture_name = CI_ORG_SEED
        .find("FIXTURE_NAME=\"demo\"")
        .expect("seed-ci-org.sh must create the shared demo workspace");
    let clone_call = CI_ORG_SEED
        .find("/api/v1/workspaces/${BASE_WS_ID}/clone")
        .expect("the shared demo must use the public checked-fork endpoint");
    assert!(
        base_lookup < fixture_name && fixture_name < clone_call,
        "the shared demo fork must be created from the resolved Base workspace"
    );
    assert!(
        CI_ORG_SEED.contains("ci-seed:${BASE_WS_ID}:${FIXTURE_NAME}")
            && CI_ORG_SEED.contains(".data.workspace.id // empty")
            && CI_ORG_SEED.contains("FORK_HTTP_CODE\" != \"201"),
        "shared demo creation must be idempotent and fail closed on an invalid clone response"
    );
    assert!(
        !CI_ORG_SEED.contains("test-token provisioning did not create the checked demo fork"),
        "lane-specific orgs are created after the first token mint and cannot rely on token provisioning for their demo fork"
    );
    assert!(
        !CI_ORG_SEED.contains("for FIXTURE_NAME in demo e2e-test")
            && TEST_WORKSPACE_SEED
                .contains("WORKSPACE_NAME=\"${G5_E2E_TEST_WORKSPACE_NAME:-e2e-test}\""),
        "seed-test-workspace.sh must retain sole ownership of the disposable e2e-test fork"
    );
}

#[test]
fn disposable_workspace_seed_refuses_to_delete_an_active_vm() {
    let guard = TEST_WORKSPACE_SEED
        .find("status IN ('starting', 'running', 'stopping')")
        .expect("seed-test-workspace.sh must detect an active workspace VM");
    let refusal = TEST_WORKSPACE_SEED[guard..]
        .find("Refusing to reset workspace")
        .map(|offset| guard + offset)
        .expect("seed-test-workspace.sh must fail closed when a VM is active");
    let vm_delete = TEST_WORKSPACE_SEED
        .find("DELETE FROM workspace_vms")
        .expect("seed-test-workspace.sh must clean an inactive VM row");

    assert!(
        guard < refusal && refusal < vm_delete,
        "the active-VM guard must run before fixture teardown deletes runtime authority"
    );
}

#[test]
fn ci_org_seed_reuses_existing_users_by_unique_email() {
    for email_variable in [
        "CI_WORKER_EMAIL",
        "E2E_MEMBER_EMAIL",
        "E2E_REAUTH_EMAIL",
        "E2E_NEW_USER_EMAIL",
    ] {
        assert!(
            CI_ORG_SEED.contains(&format!(
                "SELECT id FROM users WHERE email = '${{{email_variable}}}';"
            )),
            "seed-ci-org.sh must reuse {email_variable} by unique email before inserting its OAuth tuple"
        );
    }
}

#[test]
fn codex_seed_stages_tracked_upstream_set_with_tree_hash_gate() {
    // CODEX_SRC is an already-git-tracked upstream tree (seed_scripts D31).
    // A plain `git add -A` over a filesystem copy silently drops any
    // upstream file that is tracked in codex-rs's own history but also
    // matches a `.gitignore` pattern carried over with a copied subtree, so
    // the seed must force-add the copy and gate on tree-hash equality
    // instead.
    assert!(
        CODEX_SEED.contains("git add -f -A"),
        "codex seed must force-add the copied codex-rs subtrees so a copied \
         nested .gitignore cannot silently drop upstream-tracked files"
    );
    assert!(
        !CODEX_SEED.contains("git add -A"),
        "codex seed must not fall back to a plain git add -A for the copied-tree init"
    );

    // Fail closed unless CODEX_SRC is a verified clean, git-tracked clone.
    assert!(
        CODEX_SEED.contains("$CODEX_SRC/.git")
            && CODEX_SEED.contains("git -C \"$CODEX_SRC\" status --porcelain")
            && CODEX_SEED.contains("uncommitted changes")
            && CODEX_SEED.contains("CODEX_SRC_HEAD=\"$(git -C \"$CODEX_SRC\" rev-parse HEAD)\""),
        "codex seed must refuse to stage from a dirty or non-git codex-rs source"
    );

    // Tree-hash fidelity gate: every subtree copied from codex-rs must stage
    // to the exact tree hash the upstream commit records for that subtree.
    let verify_start = CODEX_SEED
        .find("verify_codex_subtree_hash()")
        .expect("codex seed must define a subtree tree-hash verification helper");
    let verify_helper = &CODEX_SEED[verify_start..];
    assert!(
        verify_helper.contains("git rev-parse \"HEAD:${rel_path}\"")
            && verify_helper
                .contains("git -C \"$CODEX_SRC\" rev-parse \"${CODEX_SRC_HEAD}:${rel_path}\"")
            && verify_helper.contains("Tree-hash mismatch")
            && verify_helper.contains("exit 1"),
        "codex seed tree-hash helper must compare staged vs upstream subtree hashes and fail loudly on drift"
    );
    assert!(
        CODEX_SEED.contains("verify_codex_subtree_hash \".semlift/specs\""),
        "codex seed must verify the ontology subtree hash matches upstream"
    );
    assert!(
        verify_start < CODEX_SEED.len()
            && CODEX_SEED.contains("verify_codex_subtree_hash \"${crate_name}/src\"")
            && CODEX_SEED.contains("verify_codex_subtree_hash \"${crate_name}/tests\""),
        "codex seed must verify per-crate src/tests subtree hashes match upstream"
    );

    // D31 explicitly rejects gitignore negation as a fidelity workaround.
    assert!(
        !CODEX_SEED.contains("echo '!'") && !CODEX_SEED.contains("echo \"!"),
        "codex seed must not add gitignore negation rules as a fidelity workaround"
    );
}

#[test]
fn compliance_reseed_creates_members_before_publishing_the_git_cut() {
    let member = COMPLIANCE_SEED
        .find("CODE_REPO_ID=$(seed_add_fixture_repo")
        .unwrap();
    let publish = COMPLIANCE_SEED
        .find("seed_install_fixture_repo \"$CODE_REPO_ID\"")
        .unwrap();
    let parent = COMPLIANCE_SEED
        .find("seed_capture_fixture_parent \"$WS_ID\"")
        .unwrap();
    assert!(member < publish && publish < parent);
    assert!(!COMPLIANCE_SEED.contains("workspace_branches"));
    assert!(!COMPLIANCE_SEED.contains("workspace_branch_repos"));
    assert!(COMPLIANCE_SEED.contains("owner_user_id"));
}

#[test]
fn compliance_seed_wires_tracking_aware_staging_and_tree_hash_gate() {
    // The real G5-on-G5 restaging path is seed-compliance-workspace.sh's
    // `init_local_seed_repo`, invoked by the self-hosting wrapper
    // seed-g5-workspace.sh (NOT seed-codex.sh, which seeds the external
    // codex-rs demo). It must wire the tracking-aware staging + tree-hash
    // equality gate (seed_scripts D31/D11) that the execution-backed fixture
    // below proves out, so the canonical `.claude/**` and
    // `provided_agent_assets/**` tracked-but-ignored assets survive reseed.
    // D31: stage "the exact set of paths tracked in the upstream source commit",
    // NOT a whole-worktree `git add -f -A`. Whole-worktree force-add drags
    // ignored UNTRACKED artifacts carried over by the rsync copy into the seed;
    // the tracked-set staging keeps the seed faithful to the upstream tree while
    // still force-adding (`-f`) canonical tracked-but-ignored assets.
    assert!(
        COMPLIANCE_SEED.contains("stage_upstream_tracked_set")
            && COMPLIANCE_SEED.contains("ls-files -z")
            && COMPLIANCE_SEED.contains("git add -f --"),
        "compliance seed must stage the upstream tracked set (ls-files) with a forced add, not a whole-worktree add"
    );
    // No executable (non-comment) line may whole-worktree force-add; comments
    // that explain why the OLD `git add -f -A` was wrong are fine. A line-based
    // check keeps this structural rather than a brittle exact-string match.
    let whole_worktree_force_add_command = COMPLIANCE_SEED.lines().any(|line| {
        let trimmed = line.trim_start();
        !trimmed.starts_with('#') && trimmed.contains("git add -f -A")
    });
    assert!(
        !whole_worktree_force_add_command,
        "compliance seed must NOT whole-worktree force-add as a command; that pulls ignored untracked artifacts into the seed"
    );
    // The verify-source-tree flag still exists, but only to ESCALATE the
    // git-tracked precondition — it must not gate staging (see the structural
    // selection assertion below and the execution-backed proof in
    // default_seed_flow_is_tracking_aware_and_canonical_verified).
    assert!(
        COMPLIANCE_SEED.contains("G5_COMPLIANCE_VERIFY_SOURCE_TREE"),
        "compliance seed must retain the verify-source-tree escalation flag"
    );
    // A tracking-aware seed stages the source's committed tracked set and gates
    // on equality against its HEAD tree, so a dirty source must be rejected
    // early (D31 "from a verified clean clone").
    assert!(
        COMPLIANCE_SEED.contains("require_clean_source_repo")
            && COMPLIANCE_SEED.contains("diff --quiet HEAD")
            && COMPLIANCE_SEED.contains("uncommitted changes to tracked files"),
        "compliance seed must reject a dirty source repo early with an actionable error"
    );
    // Restore canonical assets only (issue #2189 Q2/D6): tracked-set staging must
    // be paired with dropping the Group A disposable set, and the tree-hash gate
    // must compare the canonical source tree (Group A excluded), not a naive
    // whole-tree equality that would force-restore Group A.
    assert!(
        COMPLIANCE_SEED.contains("remove_seed_disposable_paths")
            && COMPLIANCE_SEED.contains("list_seed_disposable_paths")
            && COMPLIANCE_SEED.contains("outputs/ci-logs")
            && COMPLIANCE_SEED.contains("\\.env"),
        "compliance seed must drop the Group A disposable set (outputs/ci-logs/**, .env*) from the canonical restore"
    );
    assert!(
        COMPLIANCE_SEED.contains("canonical_source_tree_hash")
            && COMPLIANCE_SEED
                .contains("Staged seed trees do not match their selected source commits"),
        "compliance seed must gate on canonical-scope tree-hash equality (Group A out of scope) and fail closed on drift"
    );
    assert!(
        COMPLIANCE_SEED.contains("staged_docs_tree")
            && COMPLIANCE_SEED.contains("managed_docs_tree")
            && COMPLIANCE_SEED.contains("rev-parse \"${DOCS_HEAD}^{tree}\"")
            && COMPLIANCE_SEED.contains("CODE_HEAD\" != \"$code_head"),
        "post-push verification must compare the reparented docs tree while retaining exact code-head identity (seed_scripts D11)"
    );
    assert!(
        !COMPLIANCE_SEED.contains("DOCS_HEAD\" != \"$docs_head"),
        "the canonical initial docs bootstrap reparents the staged tree, so post-push verification must not require docs commit-id equality"
    );
    assert!(
        COMPLIANCE_SEED.contains("repair_unresolved_managed_repo_head")
            && COMPLIANCE_SEED.contains("rev-parse --verify HEAD^{commit}")
            && COMPLIANCE_SEED.contains("symbolic-ref HEAD \"refs/heads/${expected_branch}\"")
            && COMPLIANCE_SEED.contains("repair_unresolved_managed_repo_head \"$CODE_REPO_ID\" \"code\" \"$CODE_BRANCH_NAME\""),
        "the compliance seed must repair only an unresolved proposed code repo HEAD before verifying it (seed_scripts D12)"
    );
    // The Group A drop runs after the tracked set is force-staged, so those paths
    // have changes staged in the index and an UNFORCED `git rm` refuses. Without
    // `-f` — especially with the error swallowed — the removal silently no-ops
    // and Group A stays in the staged tree while the canonical hash excludes it.
    assert!(
        COMPLIANCE_SEED.contains("rm -q -f --ignore-unmatch")
            && COMPLIANCE_SEED.contains("rm --cached -q -f --ignore-unmatch"),
        "the Group A drop must force-remove; an unforced `git rm` refuses on force-staged paths and silently no-ops"
    );
    assert!(
        !COMPLIANCE_SEED.contains("--ignore-unmatch -- \"$path\" >/dev/null 2>&1 || true"),
        "the Group A drop must not swallow removal failures; a skipped removal must fail loudly (seed_scripts D31)"
    );
    // D31 explicitly rejects gitignore negation as a fidelity workaround.
    assert!(
        !COMPLIANCE_SEED.contains("echo '!'") && !COMPLIANCE_SEED.contains("echo \"!"),
        "compliance seed must not add gitignore negation rules as a fidelity workaround"
    );

    // D31 applies to the DEFAULT seed flow, not only under
    // G5_COMPLIANCE_VERIFY_SOURCE_TREE=true. Tracking-aware staging must be
    // selected by whether the SOURCE is already-git-tracked (D31's own
    // precondition), and the canonical-scope gate must run in that default path.
    // This structural pair — selection keyed on source_repo_is_git_tracked and
    // the gate wired unconditionally — replaces the previous brittle exact
    // multi-line "not gated on the verify flag" string match; the behavioral
    // proof lives in default_seed_flow_is_tracking_aware_and_canonical_verified.
    assert!(
        COMPLIANCE_SEED.contains("source_repo_is_git_tracked")
            && COMPLIANCE_SEED.contains("tracking_aware"),
        "tracking-aware staging must be selected per git-tracked source, not by the verify flag"
    );
    assert!(
        COMPLIANCE_SEED.contains("verify_canonical_seed_tree"),
        "the canonical-scope tree-hash gate must run outside the verify-flag branch"
    );
}

#[test]
fn default_seed_flow_is_tracking_aware_and_canonical_verified() {
    // Execution-backed proof for the DEFAULT path (issue #2189, seed_scripts
    // D31): with G5_COMPLIANCE_VERIFY_SOURCE_TREE unset, a git-tracked source
    // must be staged tracking-aware — only the upstream tracked set enters the
    // seed (canonical tracked-but-ignored assets preserved, Group A dropped,
    // ignored UNTRACKED artifacts left out) — and be canonical-scope verified.
    // A source with no git history falls back to plain staging without failing,
    // and a dirty source is rejected early. Drives the SHIPPED script helpers
    // (stage_upstream_tracked_set, require_clean_source_repo, ...) via `bash -c`
    // so this tests behavior, not text.
    fn run_snippet(script_body: &str) -> std::process::Output {
        std::process::Command::new("bash")
            .arg("-c")
            .arg(script_body)
            .output()
            .expect("bash runs")
    }

    let seed_script = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../scripts/seed-compliance-workspace.sh"
    );
    let base = tempfile::tempdir().expect("tempdir");
    let root = base.path().to_string_lossy().to_string();

    // NOTE: the real seeder copies with `rsync`; this harness uses `cp -a` plus an
    // explicit `.git` removal because rsync is not guaranteed present in CI
    // containers. The copy mechanism is incidental — what is under test is the
    // staging + canonical-verification logic applied to the copied tree.
    // Extract just the helper functions under test from the real script, so the
    // assertions exercise shipped code without running the full seeder (which
    // needs docker/psql/API).
    let harness = format!(
        r#"
set -euo pipefail
# Pull the shipped helpers under test out of the real script.
eval "$(sed -n '/^list_seed_disposable_paths()/,/^}}/p;/^remove_seed_disposable_paths()/,/^}}/p;/^canonical_source_tree_hash()/,/^}}/p;/^source_repo_is_git_tracked()/,/^}}/p;/^stage_upstream_tracked_set()/,/^}}/p;/^require_clean_source_repo()/,/^}}/p' {script})"

ROOT="{root}"
SRC="$ROOT/src"
mkdir -p "$SRC/.claude/skills" "$SRC/outputs/ci-logs"
printf '.claude/\noutputs/\n.env*\n!.env.example\n' > "$SRC/.gitignore"
printf 'root\n' > "$SRC/README.md"
printf '# canonical skill\n' > "$SRC/.claude/skills/skill.md"
printf 'disposable dump\n' > "$SRC/outputs/ci-logs/run.log"
# A committed .env* that IS ignored (Group A disposable) alongside a committed
# `!`-negated .env.example that is NOT ignored (canonical template; MUST survive
# reseed). This distinguishes a negation-respecting disposable set from a naive
# `.env*` name glob (issue #2189 Group A wording correction).
printf 'SECRET=disposable\n' > "$SRC/.env.tinydb.test"
printf 'SECRET=example\n' > "$SRC/.env.example"
git -C "$SRC" init -q
git -C "$SRC" config user.email t@example.com
git -C "$SRC" config user.name Test
git -C "$SRC" add -f .gitignore README.md .claude/skills/skill.md outputs/ci-logs/run.log .env.tinydb.test .env.example
git -C "$SRC" commit -q -m canonical

# IGNORED, UNTRACKED local artifacts in the source worktree, created AFTER the
# commit so they are not part of the upstream tracked set. A whole-worktree
# `git add -f -A` would drag these into the seed; tracking-aware staging
# (git ls-files) must leave them out. They also mean the source carries
# untracked files WITHOUT being dirty (diff --quiet HEAD ignores untracked).
printf 'local junk\n' > "$SRC/outputs/ci-logs/untracked-local.log"
printf 'scratch\n' > "$SRC/.claude/skills/untracked-note.md"

# Tracked-source detection must be true here and false for a plain directory.
source_repo_is_git_tracked "$SRC" || {{ echo "FAIL: tracked source not detected"; exit 1; }}
PLAIN="$ROOT/plain"; mkdir -p "$PLAIN"
if source_repo_is_git_tracked "$PLAIN"; then echo "FAIL: plain dir reported tracked"; exit 1; fi

# A source carrying only untracked (ignored) files is NOT dirty and is accepted.
require_clean_source_repo "Clean" "$SRC" || {{ echo "FAIL: clean source wrongly rejected"; exit 1; }}

# DEFAULT-path staging: tracking_aware=true (because the source is tracked),
# with no G5_COMPLIANCE_VERIFY_SOURCE_TREE set at all. Drive the SHIPPED
# stage_upstream_tracked_set against a filesystem copy of the source worktree.
SEED="$ROOT/seed"
mkdir -p "$SEED"
cp -a "$SRC"/. "$SEED"/ && rm -rf "$SEED/.git"
git -C "$SEED" init -q
git -C "$SEED" config user.email t@example.com
git -C "$SEED" config user.name Test
( cd "$SEED" && stage_upstream_tracked_set "$SRC" )
remove_seed_disposable_paths "$SEED"
git -C "$SEED" commit -q -m reseed

CANON="$(canonical_source_tree_hash "$SRC")"
STAGED="$(git -C "$SEED" rev-parse 'HEAD^{{tree}}')"
[ "$CANON" = "$STAGED" ] || {{ echo "FAIL: default path not canonical-equal ($CANON vs $STAGED)"; exit 1; }}

LIST="$(git -C "$SEED" ls-tree -r --name-only HEAD)"
echo "$LIST" | grep -q '.claude/skills/skill.md' || {{ echo "FAIL: canonical asset lost"; exit 1; }}
if echo "$LIST" | grep -q 'outputs/ci-logs/run.log'; then echo "FAIL: Group A restored"; exit 1; fi
# The committed `!`-negated .env.example is canonical and MUST survive reseed;
# the ignored .env.tinydb.test is Group A disposable and MUST be dropped. A
# naive `.env*` glob would wrongly drop BOTH.
echo "$LIST" | grep -qxF '.env.example' || {{ echo "FAIL: negated canonical .env.example dropped from seed"; exit 1; }}
if echo "$LIST" | grep -qxF '.env.tinydb.test'; then echo "FAIL: ignored .env.tinydb.test restored into seed"; exit 1; fi
# The ignored UNTRACKED artifacts must NOT have entered the seed.
if echo "$LIST" | grep -q 'untracked-local.log'; then echo "FAIL: ignored untracked artifact entered seed"; exit 1; fi
if echo "$LIST" | grep -q 'untracked-note.md'; then echo "FAIL: ignored untracked note entered seed"; exit 1; fi

# Plain `git add -A` (the OLD default) must NOT satisfy the canonical gate: it
# drops the canonical tracked-but-ignored asset.
BAD="$ROOT/bad"
mkdir -p "$BAD"
cp -a "$SRC"/. "$BAD"/ && rm -rf "$BAD/.git"
git -C "$BAD" init -q
git -C "$BAD" config user.email t@example.com
git -C "$BAD" config user.name Test
git -C "$BAD" add -A
remove_seed_disposable_paths "$BAD"
git -C "$BAD" commit -q -m plain
BADTREE="$(git -C "$BAD" rev-parse 'HEAD^{{tree}}')"
[ "$BADTREE" != "$CANON" ] || {{ echo "FAIL: plain add -A wrongly matched canonical"; exit 1; }}

# A DIRTY source (uncommitted change to a TRACKED file) must be rejected early
# with an actionable message, before any staging happens.
DIRTY="$ROOT/dirty"
mkdir -p "$DIRTY"
cp -a "$SRC"/. "$DIRTY"/
printf 'MUTATED\n' >> "$DIRTY/README.md"
if ERROUT="$( ( require_clean_source_repo "Dirty" "$DIRTY" ) 2>&1 1>/dev/null )"; then
  echo "FAIL: dirty source not rejected"; exit 1
fi
echo "$ERROUT" | grep -q 'uncommitted changes to tracked files' || {{ echo "FAIL: dirty rejection lacks actionable message: $ERROUT"; exit 1; }}

echo OK
"#,
        script = seed_script,
        root = root
    );

    let out = run_snippet(&harness);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success() && stdout.contains("OK"),
        "default-path seed staging proof failed\nstdout: {stdout}\nstderr: {stderr}"
    );
}

#[test]
fn tracking_aware_restage_restores_canonical_only_and_fails_closed_on_canonical_drift() {
    // Execution-backed proof of the seed_scripts D31 "restore canonical assets
    // only" mechanism the compliance seeder uses (issue #2189 Q2/D6). It mirrors
    // the seeder's canonical-restore math: force-add the tracked-but-ignored set
    // + drop the Group A disposable set + `HEAD^{tree}` equality against the
    // CANONICAL source tree (source commit minus Group A). This fixture's source
    // has NO untracked files, so `git add -f -A` here coincides with the
    // seeder's tracked-set staging (stage_upstream_tracked_set); the
    // ignored-untracked exclusion is proven separately in
    // default_seed_flow_is_tracking_aware_and_canonical_verified. The fixture carries
    // BOTH tracked-but-ignored classes so the test distinguishes them:
    //   * CANONICAL tracked-but-ignored asset (`.claude/**`) — MUST be restored;
    //   * DISPOSABLE Group A tracked-but-ignored artifact (`outputs/ci-logs/**`,
    //     tracked despite a committed ignore rule) — MUST NOT be force-restored
    //     and MUST be outside the tree-hash-equality scope.
    // Requires a real `git`.
    fn git(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .expect("git runs");
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        out
    }
    fn tree_hash(dir: &std::path::Path) -> String {
        String::from_utf8(git(dir, &["rev-parse", "HEAD^{tree}"]).stdout)
            .unwrap()
            .trim()
            .to_string()
    }
    fn configure(dir: &std::path::Path) {
        git(dir, &["init", "--quiet"]);
        git(dir, &["config", "user.email", "t@example.com"]);
        git(dir, &["config", "user.name", "Test"]);
    }
    // Copy the source WORKING tree (excluding .git) into `dst`, mimicking the
    // seeder's `rsync -a --exclude '.git/'` copy.
    fn copy_tree(src: &std::path::Path, dst: &std::path::Path) {
        std::fs::create_dir_all(dst).unwrap();
        for entry in std::fs::read_dir(src).unwrap() {
            let entry = entry.unwrap();
            if entry.file_name() == ".git" {
                continue;
            }
            let from = entry.path();
            let to = dst.join(entry.file_name());
            if from.is_dir() {
                copy_tree(&from, &to);
            } else {
                std::fs::copy(&from, &to).unwrap();
            }
        }
    }
    // Group A disposable paths tracked in a repo — mirrors the seeder's
    // `list_seed_disposable_paths` (outputs/ci-logs/** + .env* files).
    fn disposable_paths(dir: &std::path::Path) -> Vec<String> {
        let all = String::from_utf8(git(dir, &["ls-files"]).stdout).unwrap();
        all.lines()
            .filter(|p| {
                p.starts_with("outputs/ci-logs/")
                    || p.rsplit('/')
                        .next()
                        .map(|b| b.starts_with(".env"))
                        .unwrap_or(false)
            })
            .map(|p| p.to_string())
            .collect()
    }
    // Mirror of the seeder's `remove_seed_disposable_paths`: drop Group A from a
    // just-staged seed repo's index and worktree.
    fn remove_disposable(dir: &std::path::Path) {
        for p in disposable_paths(dir) {
            // `-f` mirrors the seeder: these paths were just force-staged by
            // `git add -f -A`, so an unforced `git rm` refuses with "has changes
            // staged in the index".
            git(dir, &["rm", "-q", "-f", "--ignore-unmatch", "--", &p]);
        }
    }
    // Mirror of the seeder's `canonical_source_tree_hash`: source commit tree
    // with Group A removed from a scratch index.
    fn canonical_source_tree(source: &std::path::Path, scratch_index: &std::path::Path) -> String {
        let run = |args: &[&str]| -> std::process::Output {
            let out = std::process::Command::new("git")
                .arg("-C")
                .arg(source)
                .env("GIT_INDEX_FILE", scratch_index)
                .args(args)
                .output()
                .expect("git runs");
            assert!(
                out.status.success(),
                "git {args:?}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            out
        };
        run(&["read-tree", "HEAD"]);
        for p in disposable_paths(source) {
            run(&["rm", "--cached", "-q", "-f", "--ignore-unmatch", "--", &p]);
        }
        String::from_utf8(run(&["write-tree"]).stdout)
            .unwrap()
            .trim()
            .to_string()
    }

    let base = tempfile::tempdir().expect("tempdir");

    // Source: clean, committed. Committed ignore rules match `.claude/` and
    // `outputs/`. The canonical asset under `.claude/` AND a disposable Group A
    // dump under `outputs/ci-logs/` are BOTH force-tracked despite the ignore
    // rules (both are "tracked-but-ignored"; only the first is canonical).
    let source = base.path().join("source");
    std::fs::create_dir_all(source.join(".claude/skills")).unwrap();
    std::fs::create_dir_all(source.join("outputs/ci-logs")).unwrap();
    std::fs::write(source.join(".gitignore"), ".claude/\noutputs/\n").unwrap();
    std::fs::write(source.join("README.md"), "root\n").unwrap();
    std::fs::write(
        source.join(".claude/skills/skill.md"),
        "# canonical skill\n",
    )
    .unwrap();
    std::fs::write(
        source.join("outputs/ci-logs/run.log"),
        "disposable ci dump\n",
    )
    .unwrap();
    configure(&source);
    git(
        &source,
        &[
            "add",
            "-f",
            ".gitignore",
            "README.md",
            ".claude/skills/skill.md",
            "outputs/ci-logs/run.log",
        ],
    );
    git(
        &source,
        &["commit", "--quiet", "-m", "canonical+disposable"],
    );

    // Canonical source tree = source commit minus Group A.
    let canonical_tree = canonical_source_tree(&source, &base.path().join("scratch-index-src"));

    // (A) Canonical-only restage: force-add everything, then drop Group A.
    let seed = base.path().join("seed");
    copy_tree(&source, &seed);
    configure(&seed);
    git(&seed, &["add", "-f", "-A"]);
    remove_disposable(&seed);
    git(&seed, &["commit", "--quiet", "-m", "reseed-canonical"]);
    let seed_tree = tree_hash(&seed);
    assert_eq!(
        seed_tree, canonical_tree,
        "canonical-only restage must equal the canonical source tree (Group A excluded from both sides)"
    );
    let listing =
        String::from_utf8(git(&seed, &["ls-tree", "-r", "--name-only", "HEAD"]).stdout).unwrap();
    assert!(
        listing.contains(".claude/skills/skill.md"),
        "the canonical tracked-but-ignored asset MUST be restored: {listing}"
    );
    assert!(
        !listing.contains("outputs/ci-logs/run.log"),
        "the disposable Group A artifact MUST NOT be force-restored into the canonical seed: {listing}"
    );

    // (B) The naive whole-tree `git add -f -A` (the rejected "restore all") does
    // NOT satisfy the contract: it force-restores Group A, so its tree does not
    // equal the canonical source tree.
    let restore_all = base.path().join("restore_all");
    copy_tree(&source, &restore_all);
    configure(&restore_all);
    git(&restore_all, &["add", "-f", "-A"]);
    git(&restore_all, &["commit", "--quiet", "-m", "restore-all"]);
    assert_ne!(
        tree_hash(&restore_all),
        canonical_tree,
        "a whole-tree force-add restore (Group A included) must NOT equal the canonical source tree — that is the rejected 'restore all' behavior"
    );
    let restore_all_listing =
        String::from_utf8(git(&restore_all, &["ls-tree", "-r", "--name-only", "HEAD"]).stdout)
            .unwrap();
    assert!(
        restore_all_listing.contains("outputs/ci-logs/run.log"),
        "sanity: the naive whole-tree restore does carry Group A (which the canonical restore excludes)"
    );

    // (C) A plain `git add -A` (no -f) drops the canonical tracked-but-ignored
    // asset, so even after removing Group A the seed tree diverges from the
    // canonical source tree — the force-add is load-bearing.
    let dropped = base.path().join("dropped");
    copy_tree(&source, &dropped);
    configure(&dropped);
    git(&dropped, &["add", "-A"]);
    remove_disposable(&dropped);
    git(&dropped, &["commit", "--quiet", "-m", "reseed-plain"]);
    assert_ne!(
        tree_hash(&dropped),
        canonical_tree,
        "a plain `git add -A` reseed drops the canonical tracked-but-ignored asset; the canonical-scope gate fails closed"
    );

    // (D) Drift WITHIN the canonical scope (a mutated canonical asset) must fail
    // the canonical-scope gate.
    let drift = base.path().join("drift");
    copy_tree(&source, &drift);
    std::fs::write(
        drift.join(".claude/skills/skill.md"),
        "# TAMPERED canonical skill\n",
    )
    .unwrap();
    configure(&drift);
    git(&drift, &["add", "-f", "-A"]);
    remove_disposable(&drift);
    git(&drift, &["commit", "--quiet", "-m", "reseed-drift"]);
    assert_ne!(
        tree_hash(&drift),
        canonical_tree,
        "drift within the canonical scope must break canonical-scope tree-hash equality; the gate fails closed"
    );
}

#[test]
fn mini_calc_seed_publishes_both_mirrors_after_sql_and_before_reindex() {
    let s = TEST_WORKSPACE_SEED;
    assert!(s.contains("BASE=\"/data/repos/${ORG_ID}/.seed-ready-${CODE_REPO_ID}\""));
    assert!(s.contains("DOCS_BASE=\"/data/repos/${ORG_ID}/.seed-ready-${DOCS_REPO_ID}\""));
    let docs = s.find("echo \"$VIEW_SQL\" | docker exec").unwrap();
    let grounding = s
        .find("JSONL grounding files committed, heads updated")
        .unwrap();
    let publish = s
        .find("< \"$SCRIPT_DIR/publish-seed-workspace-refs.sh\"")
        .unwrap();
    let reindex = s.find("# ─── Step 9:").unwrap();
    assert!(docs < publish && grounding < publish && publish < reindex);
    assert!(
        PUBLISH_SEED_REFS
            .contains("--receive-pack='git -c safe.directory=* receive-pack'"),
        "file-transport publication must authorize the receive-pack side of checked repositories owned by the API runtime user"
    );
}

fn position(source: &str, needle: &str, name: &str) -> usize {
    source
        .find(needle)
        .unwrap_or_else(|| panic!("{name} must contain {needle:?}"))
}

#[test]
fn native_publish_helper_pushes_the_pinned_sha_with_the_pinned_lease() {
    // The apply phase must publish the SHA the plan phase validated (the one
    // ancestry was checked against) leased to the mirror head it read then,
    // never a re-read HEAD.
    assert!(NATIVE_PUBLISH_LIB.contains("\"${new}:refs/heads/${branch}\""));
    assert!(NATIVE_PUBLISH_LIB.contains("--force-with-lease=\"refs/heads/${branch}:${old}\""));
    assert!(NATIVE_PUBLISH_LIB.contains("--no-follow-tags --recurse-submodules=no"));
    assert!(!NATIVE_PUBLISH_LIB.contains("\"HEAD:refs/heads/"));
    assert!(!NATIVE_PUBLISH_LIB.contains("native_publish_fast_forward"));
    let plan = position(NATIVE_PUBLISH_LIB, "native_publish_plan() {", "helper");
    let apply = position(NATIVE_PUBLISH_LIB, "native_publish_apply() {", "helper");
    assert!(plan < apply);
    // The token is scoped to the API origin and a URL rewrite of a member's
    // mirror URL refuses its plan.
    assert!(NATIVE_PUBLISH_LIB.contains("http.${NATIVE_PUBLISH_API_URL}/.extraHeader="));
    assert!(!NATIVE_PUBLISH_LIB.contains("\"http.extraHeader="));
    let rewrites = position(
        NATIVE_PUBLISH_LIB,
        "native_publish_refuse_url_rewrites \"$repo_dir\"",
        "helper",
    );
    assert!(plan < rewrites && rewrites < apply);
    // Manifest reads still validate every member before publication.
    assert!(NATIVE_PUBLISH_LIB.contains("/api/v1/workspaces/${workspace_id}/manifest"));
    assert!(NATIVE_PUBLISH_LIB.contains("manifest member without repo_id and branch"));
}

#[test]
fn seed_scripts_plan_every_member_before_the_first_native_push() {
    for (name, script) in [
        ("seed-compliance-workspace.sh", COMPLIANCE_SEED),
        ("seed-g5-workspace.sh", G5_WORKSPACE_SEED),
        ("seed-demo.sh", DEMO_SEED),
    ] {
        assert!(
            !script.contains("native_publish_fast_forward"),
            "{name} must use the plan/apply helper"
        );
    }

    // Compliance: members exist, repos are installed and the parent cut is
    // captured, then both members are planned before the single apply.
    let parent = position(
        COMPLIANCE_SEED,
        "seed_capture_fixture_parent \"$WS_ID\"",
        "compliance",
    );
    let docs = position(
        COMPLIANCE_SEED,
        "native_publish_plan \"docs\"",
        "compliance",
    );
    let code = position(
        COMPLIANCE_SEED,
        "native_publish_plan \"code\"",
        "compliance",
    );
    let apply = position(COMPLIANCE_SEED, "native_publish_apply", "compliance");
    assert!(parent < docs && docs < code && code < apply);

    // Remote seed: plan both members, apply, and recover in order — a settled
    // mirror establishes successful publication; the dev-only
    // storage repair is the second step and re-plans before pushing again.
    let plan_fn = position(G5_WORKSPACE_SEED, "plan_remote_members() {", "g5 seed");
    let plan_code = position(G5_WORKSPACE_SEED, "native_publish_plan \"code\"", "g5 seed");
    let plan_docs = position(G5_WORKSPACE_SEED, "native_publish_plan \"docs\"", "g5 seed");
    let members = position(
        G5_WORKSPACE_SEED,
        "native_publish_manifest_members_exist \"$RESOLVED_WORKSPACE_ID\"",
        "g5 seed",
    );
    let publish_fn = position(
        G5_WORKSPACE_SEED,
        "publish_remote_members_with_recovery() {",
        "g5 seed",
    );
    assert!(plan_fn < plan_code && plan_code < plan_docs && plan_docs < members);
    assert!(members < publish_fn);
    let body = &G5_WORKSPACE_SEED[publish_fn..];
    let first_apply = position(body, "native_publish_apply", "g5 publish");
    let settled = position(body, "native_publish_plan_settled", "g5 publish");
    let repair = position(
        body,
        "repair_dev_server_repos_if_requested \"publish-failure\"",
        "g5 publish",
    );
    let replan = position(
        body,
        "plan_remote_members && native_publish_apply",
        "g5 publish",
    );
    assert!(position(body, "plan_remote_members", "g5 publish") < first_apply);
    assert!(first_apply < settled && settled < repair && repair < replan);
    assert!(!G5_WORKSPACE_SEED.contains("X-G5-Force-Push: true"));

    // Demo: no branch is pushed. Base's default branch is protected and the
    // test-token cascade publishes the demo and e2e-test workspaces stopped,
    // so every docs cut is installed through the repo volume with a
    // compare-and-swapped ref (Base first, and only Base moves the mirror's
    // HEAD), then each workspace's published head is checked over the API.
    let install_fn = position(DEMO_SEED, "seed_demo_install_docs_branch() {", "demo seed");
    let install_body = &DEMO_SEED[install_fn..];
    assert!(install_body.contains("update-ref \"refs/heads/$2\" \"$3\" \"$4\""));
    assert!(install_body.contains("if [ \"$5\" = \"1\" ]; then"));
    assert!(install_body.contains("symbolic-ref HEAD \"refs/heads/$2\""));
    assert!(install_body.contains("stopped seed workspaces"));
    assert!(DEMO_SEED.contains("running incarnation"));
    let publish_fn = position(DEMO_SEED, "seed_publish_native() {", "demo seed");
    let publish_body = &DEMO_SEED[publish_fn..];
    let base_install = position(
        publish_body,
        "seed_demo_install_docs_branch \"Base\"",
        "demo publish",
    );
    let child_install = position(
        publish_body,
        "seed_demo_install_docs_branch \"workspace ${ws_id}\"",
        "demo publish",
    );
    let wait = position(
        publish_body,
        "wait_for_materialized_branch_head \"$ws_id\"",
        "demo publish",
    );
    assert!(base_install < child_install && child_install < wait);
    assert!(!publish_body.contains("native_publish_plan"));
    assert!(!publish_body.contains("native_publish_apply"));
    assert!(publish_body.contains("ORDER BY w.is_base DESC"));
}

#[test]
fn seed_clients_do_not_call_retired_projection_contracts() {
    for (name, source) in [
        ("native-publish", NATIVE_PUBLISH_LIB),
        ("g5", G5_WORKSPACE_SEED),
        ("demo", DEMO_SEED),
        ("multi-demo", DEMO_MULTI_SEED),
        ("compliance", COMPLIANCE_SEED),
        ("tinydb commit0", TINYDB_COMMIT0_SEED),
        (
            "Azure canary",
            include_str!("../../../scripts/self-hosted-azure/github-login-smoke.js"),
        ),
    ] {
        for retired in [
            "native_publish_sync_",
            "/ontology/sync-specs",
            "/docs/sync-committed",
            "linkedNodeIds",
            "nodeIds",
        ] {
            assert!(!source.contains(retired), "{name} still calls {retired}");
        }
    }
    assert!(!COMPLIANCE_SEED.contains("FROM projection_state"));
}

#[test]
fn seed_clients_use_the_contracted_schema() {
    for (name, source) in [
        ("mini-calc", TEST_WORKSPACE_SEED),
        ("tinydb", TINYDB_SOURCE_SEED),
        ("tinydb commit0", TINYDB_COMMIT0_SEED),
        ("codex", CODEX_SEED),
        ("demo", DEMO_SEED),
        ("multi-demo", DEMO_MULTI_SEED),
        ("compliance", COMPLIANCE_SEED),
        ("ontology examples", ONTOLOGY_EXAMPLES_SEED),
        (
            "BankWorkflow",
            include_str!("../../../scripts/seed-bankworkflow-new-ontology-workspace.sh"),
        ),
        (
            "self-hosted starter",
            include_str!("../../../infra/self-hosted/scripts/g5-selfhost.sh"),
        ),
    ] {
        for retired in [
            "ontology_nodes",
            "doc_node_links",
            "projection_state",
            "scope_node_id",
            "scope_mode",
        ] {
            assert!(
                !source.contains(retired),
                "{name} still uses retired storage {retired}"
            );
        }
    }
}
