//! Cloud Build e2e graph guards: the mirrored microVM guest base (cicd D15)
//! and the org/host isolation of the value-loop, rust-e2e and provider-agent
//! lanes (cicd D24/D30/D31/D32). No stack needed. Moved from the deleted
//! `stable_task_publication.rs` (P1.4); they guard retained CI configuration.

use g5d::oci::{detect_registry, parse_image_ref, Registry};

/// CI guest-base guard for worker_host_provisioning D900 and cicd D15: every
/// independent microVM host uses the supported Google Docker Hub cache route.
#[test]
fn ci_microvm_guest_base_uses_google_dockerhub_mirror() {
    const GUEST_BASE: &str = "mirror.gcr.io/library/buildpack-deps:resolute";

    let parsed = parse_image_ref(GUEST_BASE)
        .expect("the CI Docker Hub cache reference must be accepted by g5d OCI parsing");
    assert_eq!(parsed.registry, "mirror.gcr.io");
    assert_eq!(parsed.repository, "library/buildpack-deps");
    assert_eq!(detect_registry(GUEST_BASE), Registry::DockerHub);

    let cloudbuild = include_str!("../../../../infra/ci/cloudbuild-ci-e2e.yaml");
    let step = |start: &str, end: &str, label: &str| -> &str {
        cloudbuild
            .split_once(start)
            .and_then(|(_, tail)| tail.split_once(end).map(|(body, _)| body))
            .unwrap_or_else(|| panic!("cloud CI must retain the {label} step"))
    };

    let main_host = step(
        "  - id: start-workers",
        "  - id: wait-workers",
        "start-workers",
    );
    assert!(
        main_host.contains(&format!("G5_WORKER_BASE_IMAGE={GUEST_BASE}"))
            && main_host.contains("G5D_HOST_BASE_IMAGE=\"$$G5_WORKER_BASE_IMAGE\""),
        "the main host must verify and pass the mirrored guest base"
    );

    for (start, end, label) in [
        (
            "  - id: start-value-loop-worker",
            "  - id: wait-value-loop-workers",
            "start-value-loop-worker",
        ),
        (
            "  - id: start-rust-e2e-worker",
            "  - id: wait-rust-e2e-workers",
            "start-rust-e2e-worker",
        ),
        (
            "  - id: start-pa-worker",
            "  - id: wait-pa-workers",
            "start-pa-worker",
        ),
    ] {
        assert!(
            step(start, end, label).contains(&format!("G5D_HOST_BASE_IMAGE={GUEST_BASE}")),
            "{label} must use the same mirrored guest base as the main host"
        );
    }

    assert!(
        !cloudbuild.lines().any(|line| line
            .trim_start()
            .starts_with("-e G5D_HOST_BASE_IMAGE=buildpack-deps:")),
        "no CI microVM host may bypass the mirror with a direct Docker Hub short reference"
    );
}

/// CI-graph guard for cicd D30: the value-loop receipt lane runs concurrently
/// with rust-e2e ONLY under org+host isolation,
/// and the rollback substitution re-serializes it behind rust-e2e's marker.
#[test]
fn value_loop_lane_is_org_isolated_off_the_critical_path() {
    let cloudbuild = include_str!("../../../../infra/ci/cloudbuild-ci-e2e.yaml");
    let value_loop_step = cloudbuild
        .split_once("  - id: value-loop-rust-api-profiles")
        .and_then(|(_, tail)| tail.split_once("  - id: api-smoke").map(|(step, _)| step))
        .expect("cloud CI must retain the value-loop receipt step");
    let wait_for = value_loop_step
        .lines()
        .find_map(|line| line.trim().strip_prefix("waitFor: "))
        .expect("the value-loop step must declare waitFor");
    assert_eq!(
        wait_for,
        "['generate-stack-secrets', 'rust-e2e', 'e2e-rust-prime', 'stage-rust-e2e-node', 'wait-value-loop-workers', 'setup-kvm-cloud-hypervisor']",
        "the isolated value-loop lane waitFor rust-e2e to drop g5-api-re dual-load while keeping compile, Node-staging, dedicated-worker, and KVM dependencies (cicd D35)"
    );
    assert!(
        value_loop_step.contains("/workspace/value-loop-org-id")
            && value_loop_step.contains("G5_E2E_LIVE_HOST_CONTAINER=g5d-host-vl-1")
            && !value_loop_step.contains("G5_E2E_WORKER_CONTAINERS"),
        "the isolated lane must pin its dedicated org and keep the bound workspace host running"
    );
    assert!(
        value_loop_step.contains("/workspace/step-results-rust-e2e.txt"),
        "the rollback lane must re-serialize behind rust-e2e's completion marker"
    );

    let step_wait_for = |step: &str, label: &str| -> String {
        step.lines()
            .find_map(|line| line.trim().strip_prefix("waitFor: "))
            .unwrap_or_else(|| panic!("the {label} step must declare waitFor"))
            .to_string()
    };

    let seed_step = cloudbuild
        .split_once("  - id: seed-value-loop-org")
        .and_then(|(_, tail)| {
            tail.split_once("  - id: start-value-loop-worker")
                .map(|(step, _)| step)
        })
        .expect("cloud CI must retain the dedicated value-loop org seed");
    assert!(
        seed_step.contains("G5_E2E_HOST_WORKSPACE_VM_LIMIT=3")
            && seed_step.contains("G5_E2E_TEST_ORG_SLUG=g5labs-vl"),
        "the dedicated org seed must advertise main-host-parity capacity under its own slug — selectors fan out internally (concurrent conversations need two task authorities)"
    );
    assert_eq!(
        step_wait_for(seed_step, "seed-value-loop-org"),
        "['seed-ci-org']",
        "the dedicated org seed must run once the API-backed org seeding surface is proven"
    );

    let host_step = cloudbuild
        .split_once("  - id: start-value-loop-worker")
        .and_then(|(_, tail)| {
            tail.split_once("  - id: wait-value-loop-workers")
                .map(|(step, _)| step)
        })
        .expect("cloud CI must retain the dedicated value-loop host step");
    assert!(
        host_step.contains("--name=\"g5d-host-vl-1\"")
            && host_step.contains("G5D_HOST_MAX_MICROVMS=3")
            && host_step.contains("g5-e2e-host-runtime:/opt/g5-runtime:ro"),
        "the dedicated host must be g5d-host-vl-1 at main-host slot parity reusing the staged runtime volume"
    );
    assert_eq!(
        step_wait_for(host_step, "start-value-loop-worker"),
        "['seed-value-loop-org', 'start-workers', 'start-rust-e2e-api']",
        "the dedicated host must consume the seeded host credentials, the staged runtime volume, and the lane API replica it registers through when rust-e2e isolation is active (cicd D31)"
    );

    let wait_step = cloudbuild
        .split_once("  - id: wait-value-loop-workers")
        .and_then(|(_, tail)| {
            tail.split_once("  - id: seed-e2e-fixtures")
                .map(|(step, _)| step)
        })
        .expect("cloud CI must retain the value-loop readiness wait");
    assert!(
        wait_step
            .contains("WHERE org_id = '$$ORG_ID' AND host_id = '$$HOST_ID' AND status = 'ready' AND last_heartbeat IS NOT NULL"),
        "value-loop readiness must require the exact bound host to be ready"
    );
    assert_eq!(
        step_wait_for(wait_step, "wait-value-loop-workers"),
        "['start-value-loop-worker']",
        "value-loop readiness must gate on its own host start"
    );

    let rust_e2e_step = cloudbuild
        .split_once("  - id: rust-e2e\n")
        .and_then(|(_, tail)| {
            tail.split_once("  - id: value-loop-rust-api-profiles")
                .map(|(step, _)| step)
        })
        .expect("cloud CI must retain the rust-e2e step");
    assert!(
        !rust_e2e_step.contains("G5_E2E_WORKER_CONTAINERS")
            && !rust_e2e_step.contains("docker pause"),
        "the Rust lane must leave every bound workspace host running"
    );
    assert!(
        rust_e2e_step.contains("ci-stage-status.sh report \"$$rc\" /workspace/step-results-rust-e2e.txt RUST_E2E")
            && rust_e2e_step.contains("' EXIT"),
        "rust-e2e must report failures and guarantee its completion marker on hard aborts — the rollback value-loop lane serializes by polling it"
    );
}

/// CI-graph guard for cicd D31: the browser chain runs concurrently with the
/// Rust lanes ONLY when rust-e2e moves to its own org/host/API replica, the
/// receipt outputs are split per lane, the rollback substitution re-serializes
/// via completion-marker polls, and the resource sampler measures the RAM
/// envelope the overlap must prove before the rollback is dropped.
#[test]
fn rust_e2e_lane_is_org_isolated_off_the_browser_critical_path() {
    let cloudbuild = include_str!("../../../../infra/ci/cloudbuild-ci-e2e.yaml");
    let step = |start: &str, end: &str, label: &str| -> String {
        cloudbuild
            .split_once(start)
            .and_then(|(_, tail)| tail.split_once(end).map(|(step, _)| step.to_string()))
            .unwrap_or_else(|| panic!("cloud CI must retain the {label} step"))
    };
    let step_wait_for = |step: &str, label: &str| -> String {
        step.lines()
            .find_map(|line| line.trim().strip_prefix("waitFor: "))
            .unwrap_or_else(|| panic!("the {label} step must declare waitFor"))
            .to_string()
    };

    assert!(
        cloudbuild.contains("_G5_CI_RUST_E2E_ISOLATED: 'true'"),
        "rust-e2e isolation must default on with the rollback substitution retained until the canary confirms the RAM envelope (cicd D31)"
    );

    let seed_step = step(
        "  - id: seed-rust-e2e-org",
        "  - id: start-rust-e2e-api",
        "seed-rust-e2e-org",
    );
    assert!(
        seed_step.contains("G5_E2E_TEST_ORG_SLUG=g5labs-re")
            && seed_step.contains("G5_E2E_HOST_WORKSPACE_VM_LIMIT=3"),
        "the dedicated rust-e2e org seed must advertise main-host-parity capacity under its own slug"
    );
    assert_eq!(
        step_wait_for(&seed_step, "seed-rust-e2e-org"),
        "['seed-value-loop-org']",
        "the three seed-ci-org.sh runs must never interleave their shared-user upserts"
    );

    let api_step = step(
        "  - id: start-rust-e2e-api",
        "  - id: start-rust-e2e-worker",
        "start-rust-e2e-api",
    );
    assert!(
        api_step.contains("--name=g5-api-re")
            && api_step.contains("G5_GIT_EXTERNAL_URL=http://g5-api-re:7500"),
        "the lane API replica must name itself in the git URLs it mints"
    );
    assert!(
        api_step.contains("G5_DATABASE_MAX_CONNECTIONS=60")
            && api_step.contains("G5_AUTHZ_DATABASE_MAX_CONNECTIONS=24")
            && api_step.contains("G5_TENANT_DATABASE_MAX_CONNECTIONS=40"),
        "g5-api-re pools must cover 16 rust-e2e threads plus workers/projection/indexer (cicd D35 / #2475)"
    );
    assert!(
        api_step.contains("storage initialized (Filestore")
            && api_step.contains("/api/v1/instance/health")
            && api_step.contains("\"database\""),
        "g5-api-re readiness must match wait-e2e-stack depth — filestore storage-initialized log line and database health, never /healthz alone"
    );
    let main_api_step = step(
        "  - id: start-g5-api",
        "  - id: start-client",
        "start-g5-api",
    );
    for (label, step_body) in [
        ("start-g5-api", &main_api_step),
        ("start-rust-e2e-api", &api_step),
    ] {
        assert!(
            step_body.contains("source /g5-ci-secrets/env")
                && step_body.contains("-e G5_JWT_PRIVATE_KEY")
                && step_body.contains("-e G5_JWT_PUBLIC_KEY"),
            "{label} must source its JWT key material from the shared job-generated bundle — the two API replicas must hold identical keys (host JWTs mint via g5-api, present to g5-api-re)"
        );
    }

    let host_step = step(
        "  - id: start-rust-e2e-worker",
        "  - id: wait-rust-e2e-workers",
        "start-rust-e2e-worker",
    );
    assert!(
        host_step.contains("--name=\"g5d-host-re-1\"")
            && host_step.contains("G5D_HOST_MAX_MICROVMS=3")
            && host_step.contains("G5D_API_URL=http://g5-api-re:7500")
            && host_step.contains("g5-e2e-host-runtime:/opt/g5-runtime:ro"),
        "the dedicated rust-e2e host must be g5d-host-re-1 at main-host slot parity, registered through the lane API replica, reusing the staged runtime volume"
    );
    assert_eq!(
        step_wait_for(&host_step, "start-rust-e2e-worker"),
        "['seed-rust-e2e-org', 'start-workers', 'start-rust-e2e-api']",
        "the dedicated rust-e2e host must consume the seeded credentials, the staged runtime volume, and its lane API replica"
    );

    let wait_step = step(
        "  - id: wait-rust-e2e-workers",
        "  - id: seed-e2e-fixtures",
        "wait-rust-e2e-workers",
    );
    assert!(
        wait_step.contains("FROM worker_hosts")
            && wait_step.contains("WHERE org_id = '$$ORG_ID' AND host_id = '$$HOST_ID'")
            && wait_step.contains("status = 'ready'")
            && wait_step.contains("last_heartbeat IS NOT NULL"),
        "rust-e2e readiness must require the dedicated org's exact workspace host to be ready and live"
    );

    let fixtures_step = step(
        "  - id: seed-e2e-fixtures",
        "  - id: rust-e2e\n",
        "seed-e2e-fixtures",
    );
    assert!(
        fixtures_step.contains("/workspace/rust-e2e-org-id")
            && fixtures_step.contains("g5-api-re"),
        "the shared Rust fixture seed must target the dedicated org and lane API replica when isolation is active"
    );
    assert_eq!(
        step_wait_for(&fixtures_step, "seed-e2e-fixtures"),
        "['wait-workers', 'wait-rust-e2e-workers']",
        "the Rust fixture seed must gate on the substrate that will claim its work in both modes"
    );

    let rust_e2e_step = step(
        "  - id: rust-e2e\n",
        "  - id: value-loop-rust-api-profiles",
        "rust-e2e",
    );
    assert!(
        rust_e2e_step.contains("G5_E2E_LIVE_HOST_CONTAINER=g5d-host-re-1")
            && rust_e2e_step.contains("http://g5-api-re:7500")
            && rust_e2e_step.contains("G5_E2E_HTTP_TIMEOUT_SECS=90")
            && !rust_e2e_step.contains("G5_E2E_WORKER_CONTAINERS"),
        "the isolated rust-e2e lane must pin its dedicated org API and live host without pausing it"
    );
    assert_eq!(
        step_wait_for(&rust_e2e_step, "rust-e2e"),
        "['generate-stack-secrets', 'wait-workers', 'wait-rust-e2e-workers', 'e2e-rust-prime', 'stage-rust-e2e-node']",
        "rust-e2e must additionally gate on its dedicated substrate readiness"
    );

    let value_loop_step = step(
        "  - id: value-loop-rust-api-profiles",
        "  - id: api-smoke",
        "value-loop-rust-api-profiles",
    );
    assert!(
        value_loop_step.contains("selector-results.jsonl")
            && !value_loop_step.contains("selector-results-playwright.jsonl"),
        "the value-loop lane must own its own receipt file and never write the browser chain's"
    );
    assert!(
        value_loop_step.contains("http://g5-api-re:7500"),
        "the value-loop lane must leave the main API when isolation is active — the browser chain cold-restarts g5-api mid-overlap (cicd D31)"
    );
    assert!(
        value_loop_step
            .contains("http://g5-api-re:7500) export G5_E2E_API_RESTART_CONTAINER=g5-api-re")
            && value_loop_step
                .contains("http://g5-api:7500) export G5_E2E_API_RESTART_CONTAINER=g5-api"),
        "recovery API restarts must target only the API serving the selected value-loop lane"
    );
    assert!(
        value_loop_step.contains(
            "$$selector\" == test_failed_live_flush_then_api_restart_forks_one_complete_child"
        ) && value_loop_step.contains(
            "$$selector\" == test_oversized_stop_kills_the_vm_and_start_restores_the_last_receipt"
        ) && value_loop_step.contains("/workspace/step-results-api-smoke.txt"),
        "recovery restarts of the replica must wait for API smoke to finish using it"
    );
    assert!(
        value_loop_step.contains(
            "unsupported substitution combo: _G5_CI_RUST_E2E_ISOLATED='true' requires _G5_CI_VALUE_LOOP_ISOLATED='true'"
        ),
        "the fail-closed substitution-combo guard must reject a rollback value-loop lane under rust-e2e isolation — that combo collides orgs with the browser chain and eats the mid-overlap g5-api restart"
    );

    let pw_workspace_step = step(
        "  - id: seed-playwright-workspace",
        "  - id: seed-pa-workspace",
        "seed-playwright-workspace",
    );
    assert_eq!(
        step_wait_for(&pw_workspace_step, "seed-playwright-workspace"),
        "['wait-workers', 'seed-rust-e2e-org']",
        "the browser workspace seed must run off the rust-e2e critical path while staying serialized behind the shared-user seed chain (its test-token mint upserts e2e-test@g5.dev)"
    );
    assert!(
        pw_workspace_step.contains("/workspace/step-results-rust-e2e.txt"),
        "the rollback browser workspace seed must re-serialize behind rust-e2e's completion marker"
    );

    let pw_fixtures_step = step(
        "  - id: seed-playwright-fixtures",
        "  - id: playwright-e2e-shard-1",
        "seed-playwright-fixtures",
    );
    assert_eq!(
        step_wait_for(&pw_fixtures_step, "seed-playwright-fixtures"),
        "['seed-playwright-workspace', 'frontend-test', 'seed-rust-e2e-org', 'e2e-rust-prime']",
        "the browser fixture seed must keep its node_modules gate, the org-seed serialization edge, and the lane-image build it runs from — but no Rust-lane TEST edges"
    );
    assert!(
        pw_fixtures_step.contains("/workspace/step-results-value-loop-rust-api.txt")
            && !pw_fixtures_step.contains("G5_E2E_WORKER_CONTAINERS")
            && !pw_fixtures_step.contains("docker pause"),
        "the browser fixture seed must poll the value-loop marker in rollback while leaving workspace hosts running"
    );
    // Single writer per receipt file (cicd D31). P1.4 retired the persistent
    // selector loop that made this step the browser chain's recorder, so the
    // file now has exactly one writer: the provider-agent lane, which
    // provider_agent_lane_is_org_isolated_off_the_browser_tail pins.
    assert!(
        !pw_fixtures_step.contains("selector-results-playwright.jsonl"),
        "the retired persistent selector loop must not leave the browser fixture seed writing the browser-lane receipt file"
    );

    let provider_step = step(
        "  - id: provider-agent-playwright-e2e",
        "  - id: finalize-value-loop-receipt",
        "provider-agent-playwright-e2e",
    );
    assert_eq!(
        step_wait_for(&provider_step, "provider-agent-playwright-e2e"),
        "['seed-playwright-fixtures', 'seed-pa-workspace', 'start-pa-client', 'plan-playwright-shards', 'wait-pa-workers']",
        "provider-agent must remain off the Rust-lane critical path and overlap the broad shards on its own stack (deployment g5.quality.delivery.ci.provider-agent-overlap)"
    );
    assert!(
        provider_step.contains("selector-results-playwright.jsonl")
            && provider_step.contains("/workspace/step-results-rust-e2e.txt")
            && provider_step.contains("/workspace/step-results-value-loop-rust-api.txt"),
        "provider-agent must append to the browser-lane receipt file and re-serialize behind both Rust-lane markers in rollback"
    );

    let finalize_step = step(
        "  - id: finalize-value-loop-receipt",
        "  - id: stop-e2e-stack",
        "finalize-value-loop-receipt",
    );
    assert!(
        finalize_step.contains("selector-results-playwright.jsonl"),
        "receipt finalization must merge the per-lane receipt files (single-writer-per-file replaces the retired graph edge)"
    );

    let sampler_step = step(
        "  - id: resource-sampler",
        "  - id: start-postgres",
        "resource-sampler",
    );
    assert_eq!(
        step_wait_for(&sampler_step, "resource-sampler"),
        "['-']",
        "the resource sampler must observe the whole build"
    );
    assert!(
        sampler_step.contains("/workspace/resource-sampler-stop")
            && sampler_step.contains("MemAvailable"),
        "the sampler must terminate on the teardown marker and record the memory envelope the canary acceptance reads"
    );
    let stop_step = step(
        "  - id: stop-e2e-stack",
        "  - id: export-e2e-artifacts",
        "stop-e2e-stack",
    );
    assert!(
        stop_step.contains("/workspace/resource-sampler-stop")
            && stop_step.contains("g5-api-re")
            && stop_step.contains("g5d-host-re-1"),
        "teardown must release the sampler and remove/log the dedicated rust-e2e lane containers"
    );
}

/// CI-graph guard for cicd D24/D32 and deployment
/// g5.quality.delivery.ci.provider-agent-overlap: the release compile keeps
/// its waitFor barrier off the proven image-build memory peak, and the
/// isolated provider-agent lane retains its complete workload and dedicated
/// capacity while overlapping the broad shards on its own org/host/API
/// replica/edge (the serialized layout's resource-sampler summary showed
/// 34 GB of 44 GB free, so only its receipt tail waits for the shards). The
/// complete Cloud Build graph must also keep all dependency ids resolvable,
/// declared before their consumers, and acyclic.
#[test]
fn provider_agent_lane_is_org_isolated_off_the_browser_tail() {
    let cloudbuild = include_str!("../../../../infra/ci/cloudbuild-ci-e2e.yaml");
    let workspace_agent = include_str!("../../../../frontend/e2e/flows/chat.spec.ts");
    let step = |start: &str, end: &str, label: &str| -> String {
        cloudbuild
            .split_once(start)
            .and_then(|(_, tail)| tail.split_once(end).map(|(step, _)| step.to_string()))
            .unwrap_or_else(|| panic!("cloud CI must retain the {label} step"))
    };
    let step_wait_for = |step: &str, label: &str| -> String {
        step.lines()
            .find_map(|line| line.trim().strip_prefix("waitFor: "))
            .unwrap_or_else(|| panic!("the {label} step must declare waitFor"))
            .to_string()
    };

    assert!(
        cloudbuild.contains("_G5_CI_PROVIDER_AGENT_ISOLATED: 'true'"),
        "provider-agent isolation must default on with the rollback substitution retained until the canary confirms the enlarged RAM envelope (cicd D32)"
    );
    assert!(
        workspace_agent.contains("captureChatTurn"),
        "the isolated workspace-agent suite must exercise a real retained chat turn"
    );
    assert!(
        cloudbuild.contains("_G5_CI_RELEASE_BUILD_JOBS: '4'"),
        "the release compile must retain a bounded job ceiling: the barrier below \
         keeps it off the frontend and runtime-image preparation peaks, and a single job starved \
         rustc's codegen jobserver for ~120 non-cacheable link units (5m34s at a \
         99.5% sccache hit rate, build bb7bcffa)"
    );

    let release_step = step(
        "  - id: rust-release-artifacts",
        "  - id: e2e-rust-prime",
        "rust-release-artifacts",
    );
    assert_eq!(
        step_wait_for(&release_step, "rust-release-artifacts"),
        "['rust-target-prime', 'frontend-test', 'build-playwright-runner-image', 'build-api-runtime-shell', 'build-worker-runtime-shell', 'build-edge-image']",
        "the release compile starts after the frontend and runtime-image preparation \
         peaks; overlapping frontend-test gained nothing under CPU contention \
         (ontology g5.quality.delivery.ci.release-overlap)"
    );
    // frontend-test also gates the browser fixtures that consume its
    // node_modules and the final CI status, independently of the release edge.
    for (consumer, end) in [
        (
            "  - id: seed-playwright-fixtures",
            "  - id: plan-playwright-shards",
        ),
        ("  - id: assert-ci-status", "\nartifacts:"),
    ] {
        let consumer_step = step(consumer, end, consumer.trim());
        assert!(
            step_wait_for(&consumer_step, consumer.trim()).contains("'frontend-test'"),
            "{consumer} must keep waiting for frontend-test"
        );
    }

    let mut graph = std::collections::BTreeMap::<String, Vec<String>>::new();
    let mut declared = std::collections::BTreeSet::<String>::new();
    for chunk in cloudbuild.split("\n  - id: ").skip(1) {
        let id = chunk
            .lines()
            .next()
            .expect("a Cloud Build step id must have a value")
            .trim()
            .to_string();
        let wait_for = chunk
            .lines()
            .find_map(|line| line.trim().strip_prefix("waitFor: "))
            .unwrap_or("[]")
            .trim_start_matches('[')
            .trim_end_matches(']')
            .split(',')
            .map(|dep| dep.trim().trim_matches('\''))
            .filter(|dep| !dep.is_empty() && *dep != "-")
            .map(str::to_string)
            .collect::<Vec<_>>();
        for dependency in &wait_for {
            assert!(
                declared.contains(dependency),
                "Cloud Build step {id} waits for dependency {dependency}, which must be declared earlier"
            );
        }
        assert!(
            declared.insert(id.clone()),
            "Cloud Build step id {id} must be unique"
        );
        assert!(
            graph.insert(id.clone(), wait_for).is_none(),
            "Cloud Build step id {id} must be unique"
        );
    }
    // Required steps and dependencies are checked below. An unrelated guard
    // (for example migration checksums) may be added without invalidating
    // lane isolation; the old fixed total rejected such additions.
    for (step_id, dependencies) in &graph {
        for dependency in dependencies {
            assert!(
                graph.contains_key(dependency),
                "Cloud Build step {step_id} waits for missing dependency {dependency}"
            );
        }
    }
    fn visit(
        step_id: &str,
        graph: &std::collections::BTreeMap<String, Vec<String>>,
        visiting: &mut std::collections::BTreeSet<String>,
        visited: &mut std::collections::BTreeSet<String>,
    ) {
        if visited.contains(step_id) {
            return;
        }
        assert!(
            visiting.insert(step_id.to_string()),
            "Cloud Build dependency cycle reaches {step_id}"
        );
        for dependency in &graph[step_id] {
            visit(dependency, graph, visiting, visited);
        }
        visiting.remove(step_id);
        visited.insert(step_id.to_string());
    }
    let mut visited = std::collections::BTreeSet::new();
    for step_id in graph.keys() {
        visit(
            step_id,
            &graph,
            &mut std::collections::BTreeSet::new(),
            &mut visited,
        );
    }

    let seed_step = step(
        "  - id: seed-pa-org",
        "  - id: start-rust-e2e-api",
        "seed-pa-org",
    );
    assert!(
        seed_step.contains("G5_E2E_TEST_ORG_SLUG=g5labs-pa")
            && seed_step.contains("G5_E2E_HOST_WORKSPACE_VM_LIMIT=5"),
        "the dedicated provider-agent org seed must advertise the five-slot capacity (running seed + two lane workers + lifecycle overlap; deployment g5.quality.delivery.ci.provider-agent-parallel) under its own slug"
    );
    assert_eq!(
        step_wait_for(&seed_step, "seed-pa-org"),
        "['seed-rust-e2e-org']",
        "the four seed-ci-org.sh runs must never interleave their shared-user upserts"
    );

    let workspace_seed_step = step(
        "  - id: seed-pa-workspace",
        "  - id: seed-playwright-fixtures",
        "seed-pa-workspace",
    );
    assert!(
        workspace_seed_step.contains("/workspace/pa-org-id"),
        "the PA workspace seed must target the dedicated org so the lane's createWorkspaceClone source exists there"
    );
    assert!(
        workspace_seed_step.contains("G5_E2E_TEST_ID_SCOPE=\"e2e-playwright-pa\""),
        "the PA workspace seed must salt the seeder's deterministic UUIDv5 fixture ids with its own scope — the scope defaults to the workspace name, and two same-name seeds collide on the global ontology_nodes_pkey (canary 47702b1f)"
    );
    assert_eq!(
        step_wait_for(&workspace_seed_step, "seed-pa-workspace"),
        "['wait-pa-workers', 'seed-playwright-workspace']",
        "the PA clone-source seed must wait for its dedicated host and the short browser workspace seed, which inspects the same base-image namespace; the test lanes still overlap"
    );

    let host_step = step(
        "  - id: start-pa-worker",
        "  - id: wait-pa-workers",
        "start-pa-worker",
    );
    assert!(
        host_step.contains("--name=\"g5d-host-pa-1\"")
            && host_step.contains("G5D_HOST_MAX_MICROVMS=5")
            && host_step.contains("G5D_API_URL=http://g5-api-pa:7500")
            && host_step.contains("G5D_GIT_HTTP_ALLOW_HOSTS=g5-api-pa:7500")
            && host_step.contains("g5-e2e-host-runtime:/opt/g5-runtime:ro"),
        "the dedicated provider-agent host must be g5d-host-pa-1 with five slots (running seed + two lane workers + lifecycle overlap, a superset of the former g5d-host-1 parity; deployment g5.quality.delivery.ci.provider-agent-parallel), registered through the lane's own API replica g5-api-pa, reusing the staged runtime volume"
    );
    assert_eq!(
        step_wait_for(&host_step, "start-pa-worker"),
        "['seed-pa-org', 'start-workers', 'start-pa-api']",
        "the dedicated provider-agent host must consume the seeded credentials, the staged runtime volume and its own API replica"
    );

    // Dedicated API replica + edge (deployment
    // g5.quality.delivery.ci.provider-agent-overlap): chat-restart-continuity
    // `docker restart`s E2E_API_CONTAINER, so the lane may only overlap the
    // broad shards when that container is not their g5-api.
    let pa_api_step = step(
        "  - id: start-pa-api",
        "  - id: start-pa-client",
        "start-pa-api",
    );
    assert!(
        pa_api_step.contains("--name=g5-api-pa")
            && pa_api_step.contains("G5_EXTERNAL_URL=https://app-pa.g5-e2e.test")
            && pa_api_step.contains("G5_PREVIEW_HOST_SUFFIX=preview-pa.g5-e2e.test")
            && pa_api_step.contains("G5_GIT_EXTERNAL_URL=http://g5-api-pa:7500")
            && pa_api_step.contains("G5_STORAGE_WORKTREE_DIR=/tmp/g5-api-pa-storage-worktrees")
            && pa_api_step.contains("/api/v1/instance/health"),
        "the provider-agent API replica must be g5-api-pa with lane-local external/git URLs and the same storage+database readiness depth as the main API"
    );
    assert_eq!(
        step_wait_for(&pa_api_step, "start-pa-api"),
        "['generate-stack-secrets', 'build-api-image', 'start-oauth-stub', 'db-bootstrap-runtime-users-e2e']",
        "the provider-agent API replica must start from the same prerequisites as the main API"
    );
    let pa_client_step = step(
        "  - id: start-pa-client",
        "  - id: start-pa-worker",
        "start-pa-client",
    );
    assert!(
        pa_client_step.contains("--name=g5-test-client-pa")
            && pa_client_step.contains("--network-alias=app-pa.g5-e2e.test")
            && pa_client_step.contains("G5_API_URL=http://g5-api-pa:7500")
            && pa_client_step.contains("G5_EDGE_PREVIEW_SUFFIX=preview-pa.g5-e2e.test"),
        "the provider-agent edge must be g5-test-client-pa proxying to g5-api-pa"
    );
    assert_eq!(
        step_wait_for(&pa_client_step, "start-pa-client"),
        "['build-edge-image', 'start-pa-api']",
        "the provider-agent edge must wait for its image and its API replica"
    );
    let pa_workspace_seed_step = step(
        "  - id: seed-pa-workspace",
        "  - id: seed-playwright-fixtures",
        "seed-pa-workspace",
    );
    assert!(
        pa_workspace_seed_step.contains("API_CONTAINER=\"g5-api-pa\"")
            && pa_workspace_seed_step.contains("\"http://g5-api-pa:7500\""),
        "the provider-agent workspace seed must target the lane's API replica"
    );
    assert!(
        host_step.contains("G5_CLAUDE_CODE_FAKE=1")
            && host_step.contains("G5_CODEX_FAKE=1")
            && host_step.contains("G5_FAKE_STRICT_RESUME=1"),
        "the provider-agent host must run both fake providers and reject missing native files on resume"
    );
    // Explicit PA-host coverage of the mirrored guest-base rule guarded
    // fleet-wide by `ci_microvm_guest_base_uses_google_dockerhub_mirror`
    // (#2010): every independent microVM host cache must pull through
    // Google's Docker Hub cache, never the direct short reference.
    assert!(
        host_step.contains("G5D_HOST_BASE_IMAGE=mirror.gcr.io/library/buildpack-deps:resolute"),
        "g5d-host-pa-1 must use the mirrored guest base like every other CI microVM host"
    );
    assert!(
        !host_step.contains("G5D_HOST_BASE_IMAGE=buildpack-deps:"),
        "g5d-host-pa-1 must not bypass the mirror with a direct Docker Hub short reference"
    );

    let wait_step = step(
        "  - id: wait-pa-workers",
        "  - id: seed-e2e-fixtures",
        "wait-pa-workers",
    );
    assert!(
        wait_step.contains("FROM worker_hosts")
            && wait_step.contains("WHERE org_id = '$$ORG_ID' AND host_id = '$$HOST_ID'")
            && wait_step.contains("status = 'ready'")
            && wait_step.contains("last_heartbeat IS NOT NULL"),
        "provider-agent readiness must require the dedicated org's exact workspace host to be ready and live"
    );
    assert_eq!(
        step_wait_for(&wait_step, "wait-pa-workers"),
        "['start-pa-worker']",
        "the PA readiness wait gates only on its own host start"
    );

    let provider_step = step(
        "  - id: provider-agent-playwright-e2e",
        "  - id: finalize-value-loop-receipt",
        "provider-agent-playwright-e2e",
    );
    let shared_selectors = include_str!("../../playwright_shard_value_loops.tsv");
    assert!(
        provider_step.contains("selector-results-playwright.jsonl")
            && provider_step.contains("--profile playwright-workspace-vm")
            && provider_step
                .contains("python3 /workspace/scripts/record_playwright_shard_results.py")
            && provider_step
                .contains("--selectors /workspace/tests/e2e/playwright_shard_value_loops.tsv")
            && [
                "e2e/flows/association-mismatch.spec.ts",
                "e2e/flows/explicit-save.spec.ts",
                "e2e/flows/workspace-vm.spec.ts",
                "e2e/flows/workspace-fork-outcomes.spec.ts",
                "e2e/flows/workspace-fork-isolation.spec.ts",
                "e2e/flows/sharing-matrix.spec.ts",
                "e2e/flows/agent-value-outcome.spec.ts",
                "e2e/flows/changes-and-push.spec.ts",
                "e2e/flows/terminal.spec.ts",
                "e2e/flows/workspace-preview.spec.ts",
                "e2e/flows/source-control-selection.spec.ts",
                "e2e/flows/source-control-comparisons.spec.ts",
                "e2e/flows/source-control-history.spec.ts",
                "e2e/flows/auto-commit-review.spec.ts",
            ]
            .iter()
            .all(|selector| {
                shared_selectors
                    .lines()
                    .any(|row| row == format!("{selector}|frontend/{selector}"))
            }),
        "the dedicated workspace lane must consume the manifest retaining its browser selectors"
    );
    assert!(
        provider_step.contains("VITE_E2E_ORG=g5labs-pa")
            && provider_step.contains("VITE_E2E_SELECTED_ORG=g5labs-pa")
            && !provider_step.contains("G5_E2E_WORKER_CONTAINERS"),
        "the isolated provider-agent lane must export both browser org env vars and keep its bound workspace host running"
    );
    assert!(
        provider_step.contains("e2e/flows/chat.spec.ts")
            && provider_step.contains("e2e/flows/codex-settings.spec.ts")
            && provider_step.contains("LLM_ENABLED=1"),
        "the dedicated provider-agent lane must execute workspace chat instead of skipping it and retain provider settings coverage"
    );
    assert!(
        provider_step.contains("/workspace/step-results-playwright-e2e-shard-1.txt")
            && provider_step.contains("/workspace/step-results-playwright-e2e-shard-2.txt")
            && provider_step.contains("/workspace/step-results-playwright-e2e-shard-3.txt"),
        "the rollback provider-agent lane must re-serialize behind every shard completion marker, and the isolated lane's receipt tail must poll them before reading the shard reports (waitFor cannot branch on substitutions)"
    );
    assert!(
        provider_step.contains("export E2E_API_CONTAINER=g5-api-pa")
            && provider_step.contains("export E2E_API_URL=http://g5-api-pa:7500")
            && provider_step.contains("export E2E_BASE_URL=https://app-pa.g5-e2e.test"),
        "the isolated provider-agent lane must point its browser, API client and API restarts at its own replica/edge so its restarts never hit the broad shards' g5-api"
    );
    assert_eq!(
        step_wait_for(&provider_step, "provider-agent-playwright-e2e"),
        "['seed-playwright-fixtures', 'seed-pa-workspace', 'start-pa-client', 'plan-playwright-shards', 'wait-pa-workers']",
        "provider-agent execution must overlap the broad shards on its own stack; only its receipt tail waits for the shard markers in-step (deployment g5.quality.delivery.ci.provider-agent-overlap)"
    );

    for (start, end, label) in [
        (
            "  - id: playwright-e2e-shard-1",
            "  - id: playwright-e2e-shard-2",
            "shard 1",
        ),
        (
            "  - id: playwright-e2e-shard-2",
            "  - id: playwright-e2e-shard-3",
            "shard 2",
        ),
        (
            "  - id: playwright-e2e-shard-3",
            "  - id: provider-agent-playwright-e2e",
            "shard 3",
        ),
    ] {
        let shard_step = step(start, end, label);
        assert!(
            shard_step.contains("Claude Code routing from UI actions"),
            "{label} must exclude the Runtime-Config-writing Claude routing serial suite"
        );
    }

    assert!(
        !cloudbuild.contains("coordinated-base-pull-playwright-e2e")
            && !cloudbuild.contains("coordinated-repo-sync.spec.ts"),
        "the deleted coordinated-repo-sync spec must have no remaining Cloud Build lane"
    );

    let stop_step = step(
        "  - id: stop-e2e-stack",
        "  - id: export-e2e-artifacts",
        "stop-e2e-stack",
    );
    assert!(
        stop_step.contains("g5d-host-pa-1"),
        "teardown must remove/log the dedicated provider-agent lane host"
    );
}
