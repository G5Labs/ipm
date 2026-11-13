//! Deterministic coverage for `scripts/cancel-stale-ci-builds.sh` (cicd D18).
//!
//! Issue #1328: live GitHub PR events were observed to deliver an empty
//! `_PR_NUMBER`, so the old `_PR_NUMBER`-gated cancellation step always
//! skipped and stale branch builds ran to completion. The shared helper now
//! identifies the build stream by PR number when present, else by head repo +
//! head branch. These tests execute the real script against a stub `gcloud`
//! on `PATH`, asserting the selection behavior mode by mode; a final guard
//! test pins both Cloud Build CI files to the shared helper so the two copies
//! cannot drift.

#![cfg(test)]

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn script_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/cancel-stale-ci-builds.sh")
}

/// Outcome of one scripted run against the stub `gcloud`.
struct RunOutcome {
    status_code: i32,
    stdout: String,
    /// Every stub `gcloud` invocation, one line per call (full argv).
    gcloud_calls: Vec<String>,
    /// argv lines of `builds cancel` invocations only.
    cancel_calls: Vec<String>,
}

/// Baseline env for a valid current build; individual tests override fields.
fn base_env() -> HashMap<&'static str, String> {
    HashMap::from([
        ("G5_CI_EXPECTED_TRIGGER", "g5-ci-rust".to_string()),
        ("G5_CI_TRIGGER_NAME", "g5-ci-rust".to_string()),
        ("G5_CI_BUILD_ID", "current-build".to_string()),
        ("G5_CI_COMMIT_SHA", "aaaaaaa".to_string()),
        ("G5_CI_PR_NUMBER", "371".to_string()),
        ("G5_CI_HEAD_BRANCH", "feat/thing".to_string()),
        (
            "G5_CI_HEAD_REPO_URL",
            "https://github.com/G5Labs/G5".to_string(),
        ),
        ("G5_CI_REGION", "us-central1".to_string()),
        ("G5_CI_PROJECT", "g5-test-project".to_string()),
    ])
}

const CURRENT_CREATE: &str = "2026-07-21T12:00:00.000000000+00:00";

/// Candidate rows for `builds list` in `value(id,substitutions.COMMIT_SHA,createTime)`
/// form (tab-separated), exercising every preservation rule at once.
fn standard_candidates() -> String {
    [
        // Older, different SHA: the one genuinely stale build.
        "stale-old\tbbbbbbb\t2026-07-21T11:00:00.000000000+00:00",
        // Same SHA as the current build (e.g. sibling retry): preserved.
        "same-sha\taaaaaaa\t2026-07-21T10:00:00.000000000+00:00",
        // Strictly newer than the current build: preserved.
        "newer\tccccccc\t2026-07-21T13:00:00.000000000+00:00",
        // Equal createTime: not strictly older, preserved.
        "tie\tddddddd\t2026-07-21T12:00:00.000000000+00:00",
        // The current build itself: always preserved.
        "current-build\taaaaaaa\t2026-07-21T12:00:00.000000000+00:00",
    ]
    .join("\n")
}

fn run_script(
    env: &HashMap<&'static str, String>,
    list_output: &str,
    cancel_exit: i32,
) -> RunOutcome {
    let dir = tempfile::tempdir().expect("tempdir");
    let stub_dir = dir.path();

    fs::write(stub_dir.join("describe.out"), format!("{CURRENT_CREATE}\n")).unwrap();
    fs::write(stub_dir.join("list.out"), list_output).unwrap();
    fs::write(stub_dir.join("cancel_exit"), cancel_exit.to_string()).unwrap();

    // Stub gcloud: log every argv, serve fixtures for describe/list, record
    // cancel argv, and honor the configured cancel exit code.
    let stub = r#"#!/usr/bin/env bash
STUB_DIR="$(cd "$(dirname "$0")" && pwd)"
printf '%s\n' "$*" >> "$STUB_DIR/calls.log"
if [[ "$1" == "builds" && "$2" == "describe" ]]; then
  cat "$STUB_DIR/describe.out"
elif [[ "$1" == "builds" && "$2" == "list" ]]; then
  cat "$STUB_DIR/list.out"
elif [[ "$1" == "builds" && "$2" == "cancel" ]]; then
  printf '%s\n' "$*" >> "$STUB_DIR/cancel.log"
  exit "$(cat "$STUB_DIR/cancel_exit")"
fi
"#
    .to_string();
    let stub_path = stub_dir.join("gcloud");
    fs::write(&stub_path, stub).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&stub_path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    let path_env = format!(
        "{}:{}",
        stub_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let mut cmd = Command::new("bash");
    cmd.arg(script_path()).env("PATH", path_env);
    for (k, v) in env {
        cmd.env(k, v);
    }
    let output = cmd.output().expect("run cancel-stale-ci-builds.sh");

    let read_log = |name: &str| -> Vec<String> {
        fs::read_to_string(stub_dir.join(name))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    };

    RunOutcome {
        status_code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        gcloud_calls: read_log("calls.log"),
        cancel_calls: read_log("cancel.log"),
    }
}

fn list_call(outcome: &RunOutcome) -> Option<&String> {
    outcome
        .gcloud_calls
        .iter()
        .find(|line| line.starts_with("builds list"))
}

#[test]
fn pr_mode_cancels_only_strictly_older_different_sha_builds() {
    let outcome = run_script(&base_env(), &standard_candidates(), 0);

    assert_eq!(outcome.status_code, 0, "stdout: {}", outcome.stdout);
    let list = list_call(&outcome).expect("builds list must be queried in PR mode");
    assert!(
        list.contains(r#"substitutions._PR_NUMBER="371""#)
            && list.contains(r#"substitutions.TRIGGER_NAME="g5-ci-rust""#)
            && list.contains("--ongoing"),
        "PR mode must filter by PR number + current trigger: {list}"
    );
    assert!(
        !list.contains("_HEAD_BRANCH") && !list.contains("_HEAD_REPO_URL"),
        "PR mode must not mix in the branch fallback identity: {list}"
    );

    assert_eq!(
        outcome.cancel_calls.len(),
        1,
        "exactly one cancel request expected: {:?}",
        outcome.cancel_calls
    );
    let cancel = &outcome.cancel_calls[0];
    assert!(
        cancel.contains("stale-old"),
        "the older different-SHA build must be cancelled: {cancel}"
    );
    for preserved in ["same-sha", "newer", "tie", "current-build"] {
        assert!(
            !cancel.contains(preserved),
            "{preserved} must be preserved, got: {cancel}"
        );
    }
}

#[test]
fn branch_fallback_cancels_when_pr_number_is_empty() {
    let mut env = base_env();
    env.insert("G5_CI_PR_NUMBER", String::new());
    let outcome = run_script(&env, &standard_candidates(), 0);

    assert_eq!(outcome.status_code, 0, "stdout: {}", outcome.stdout);
    let list = list_call(&outcome).expect("builds list must be queried in branch-fallback mode");
    assert!(
        list.contains(r#"substitutions._HEAD_REPO_URL="https://github.com/G5Labs/G5""#)
            && list.contains(r#"substitutions._HEAD_BRANCH="feat/thing""#)
            && list.contains(r#"substitutions.TRIGGER_NAME="g5-ci-rust""#),
        "fallback mode must filter by head repo + head branch + trigger: {list}"
    );
    assert!(
        !list.contains("_PR_NUMBER"),
        "fallback mode must not filter on an empty PR number: {list}"
    );
    assert_eq!(outcome.cancel_calls.len(), 1, "{:?}", outcome.cancel_calls);
    assert!(outcome.cancel_calls[0].contains("stale-old"));
}

#[test]
fn skips_without_any_stream_identity() {
    let mut env = base_env();
    env.insert("G5_CI_PR_NUMBER", String::new());
    env.insert("G5_CI_HEAD_BRANCH", String::new());
    env.insert("G5_CI_HEAD_REPO_URL", String::new());
    let outcome = run_script(&env, &standard_candidates(), 0);

    assert_eq!(outcome.status_code, 0, "stdout: {}", outcome.stdout);
    assert!(
        outcome.stdout.contains("skipping stale build cancellation"),
        "must announce the skip: {}",
        outcome.stdout
    );
    assert!(
        outcome.gcloud_calls.is_empty(),
        "no gcloud call may happen without a stream identity: {:?}",
        outcome.gcloud_calls
    );
}

#[test]
fn trigger_mismatch_skips_entirely() {
    // Cross-trigger isolation, manual runs, and misconfigured invocations all
    // reduce to TRIGGER_NAME != G5_CI_EXPECTED_TRIGGER.
    let mut env = base_env();
    env.insert("G5_CI_TRIGGER_NAME", "g5-ci-e2e".to_string());
    let outcome = run_script(&env, &standard_candidates(), 0);

    assert_eq!(outcome.status_code, 0, "stdout: {}", outcome.stdout);
    assert!(
        outcome.gcloud_calls.is_empty() && outcome.cancel_calls.is_empty(),
        "a build of another trigger must never query or cancel: {:?}",
        outcome.gcloud_calls
    );
}

#[test]
fn filter_metacharacters_in_fallback_identity_refuse_to_query() {
    let mut env = base_env();
    env.insert("G5_CI_PR_NUMBER", String::new());
    env.insert("G5_CI_HEAD_BRANCH", "feat/\"quoted\"".to_string());
    let outcome = run_script(&env, &standard_candidates(), 0);

    assert_eq!(outcome.status_code, 0, "stdout: {}", outcome.stdout);
    assert!(
        outcome.stdout.contains("filter metacharacters"),
        "must announce the metacharacter refusal: {}",
        outcome.stdout
    );
    assert!(
        outcome.gcloud_calls.is_empty(),
        "a metacharacter identity must never reach the list filter: {:?}",
        outcome.gcloud_calls
    );
}

#[test]
fn cancel_failure_never_fails_the_gate() {
    let outcome = run_script(&base_env(), &standard_candidates(), 1);

    assert_eq!(
        outcome.status_code, 0,
        "a failed cancel request must not fail CI: {}",
        outcome.stdout
    );
    assert!(
        outcome
            .stdout
            .contains("WARN: stale build cancellation request failed"),
        "the failure must be visible in the log: {}",
        outcome.stdout
    );
}

#[test]
fn no_candidates_is_a_clean_noop() {
    let outcome = run_script(&base_env(), "", 0);

    assert_eq!(outcome.status_code, 0, "stdout: {}", outcome.stdout);
    assert!(
        outcome.cancel_calls.is_empty(),
        "nothing to cancel: {:?}",
        outcome.cancel_calls
    );
    assert!(
        outcome.stdout.contains("No stale in-progress CI builds"),
        "stdout: {}",
        outcome.stdout
    );
}

#[test]
fn both_ci_files_delegate_to_the_shared_helper_without_drift() {
    let rust_yaml = include_str!("../../../../infra/ci/cloudbuild-ci-rust.yaml");
    let e2e_yaml = include_str!("../../../../infra/ci/cloudbuild-ci-e2e.yaml");

    let step_body = |yaml: &str, label: &str| -> String {
        let start = yaml
            .find("- id: cancel-stale-pr-builds")
            .unwrap_or_else(|| panic!("{label} must keep the cancel-stale-pr-builds step"));
        let rest = &yaml[start..];
        let end = rest
            .find("waitFor: ['-']")
            .unwrap_or_else(|| panic!("{label} cancel step must stay waitFor: ['-']"));
        rest[..end].to_string()
    };

    let rust_step = step_body(rust_yaml, "cloudbuild-ci-rust.yaml");
    let e2e_step = step_body(e2e_yaml, "cloudbuild-ci-e2e.yaml");

    assert!(
        rust_step.contains("bash scripts/cancel-stale-ci-builds.sh")
            && e2e_step.contains("bash scripts/cancel-stale-ci-builds.sh"),
        "both CI files must delegate to the shared helper"
    );
    assert!(
        rust_step.contains(r#"G5_CI_EXPECTED_TRIGGER="g5-ci-rust""#),
        "Rust file must pass its own trigger: {rust_step}"
    );
    assert!(
        e2e_step.contains(r#"G5_CI_EXPECTED_TRIGGER="g5-ci-e2e""#),
        "E2E file must pass its own trigger: {e2e_step}"
    );
    // cicd D23: the expected-trigger env line is the ONLY sanctioned delta
    // between the two step bodies.
    assert_eq!(
        rust_step.replace(r#"G5_CI_EXPECTED_TRIGGER="g5-ci-rust""#, "TRIGGER_DELTA"),
        e2e_step.replace(r#"G5_CI_EXPECTED_TRIGGER="g5-ci-e2e""#, "TRIGGER_DELTA"),
        "cancel-stale-pr-builds step bodies drifted beyond the expected-trigger delta"
    );

    // #1328 root cause: a key declared in the config `substitutions:` block
    // MASKS the trigger event's derived value (a `_PR_NUMBER: ''` default
    // added 2026-05-27 silently killed cancellation and the PR comment). The
    // event-derived keys must never be declared; ALLOW_LOOSE keeps manual
    // submissions rendering absent keys empty instead of erroring (cicd D18).
    for (yaml, label) in [(rust_yaml, "rust"), (e2e_yaml, "e2e")] {
        for key in ["_PR_NUMBER:", "_HEAD_BRANCH:", "_HEAD_REPO_URL:"] {
            let declared = yaml.lines().any(|l| l.trim_start().starts_with(key));
            assert!(
                !declared,
                "{label} CI file must NOT declare {key} — a config declaration masks the trigger event's value (#1328)"
            );
        }
        assert!(
            yaml.contains("substitution_option: 'ALLOW_LOOSE'"),
            "{label} CI file must set ALLOW_LOOSE so undeclared event keys render empty on manual submissions"
        );
    }
}

/// Occurrences of the single-`$` bash-default substitution spelling
/// (`${_VAR:-...}`) in a Cloud Build config. Cloud Build does not substitute
/// the `:-` form even when the substitution exists, so the literal reaches
/// bash, which reads an unset SHELL variable — the expression is always
/// empty. Deliberate shell expressions use `$${VAR:-...}` and are skipped, as
/// are yaml comment lines (which may cite the forbidden spelling).
fn single_dollar_default_spellings(yaml: &str) -> Vec<String> {
    let mut hits = Vec::new();
    for (idx, line) in yaml.lines().enumerate() {
        if line.trim_start().starts_with('#') {
            continue;
        }
        let bytes = line.as_bytes();
        let mut search = 0;
        while let Some(pos) = line[search..].find("${_") {
            let start = search + pos;
            search = start + 3;
            if start > 0 && bytes[start - 1] == b'$' {
                continue;
            }
            let rest = &line[start + 3..];
            let name_len = rest
                .find(|c: char| !(c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_'))
                .unwrap_or(rest.len());
            if rest[name_len..].starts_with(":-") {
                hits.push(format!("line {}: {}", idx + 1, line.trim()));
            }
        }
    }
    hits
}

#[test]
fn no_unsubstituted_bash_default_spelling_in_cloud_build_configs() {
    // #1328 follow-up: the former post-ci-comment (both CI files) and
    // terraform post-pr-comment steps gated on `${_PR_NUMBER:-}` and
    // silently never ran — even after the unmasking, because Cloud Build
    // leaves the `:-` form unsubstituted (cicd I7). The comment steps are
    // gone (2026-07-31), but the spelling rule binds every
    // substitution-gated step, so keep the pattern pinned out of the CI
    // configs and the QA terraform pipeline.
    let configs = [
        (
            "cloudbuild-ci-rust.yaml",
            include_str!("../../../../infra/ci/cloudbuild-ci-rust.yaml"),
        ),
        (
            "cloudbuild-ci-e2e.yaml",
            include_str!("../../../../infra/ci/cloudbuild-ci-e2e.yaml"),
        ),
        (
            "cloudbuild-terraform-qa.yaml",
            include_str!("../../../../infra/ci/cloudbuild-terraform-qa.yaml"),
        ),
    ];
    for (label, yaml) in configs {
        let hits = single_dollar_default_spellings(yaml);
        assert!(
            hits.is_empty(),
            "{label} contains single-$ `${{_VAR:-}}` spellings Cloud Build never substitutes (write `${{_VAR}}` + ALLOW_LOOSE instead): {hits:?}"
        );
    }
}

#[test]
fn terraform_plan_and_apply_use_bounded_state_lock_wait() {
    let terraform = include_str!("../../../../infra/ci/cloudbuild-terraform-qa.yaml");
    let roots = terraform
        .split_once("  - id: tf-roots\n")
        .map(|(_, body)| body)
        .expect("QA terraform pipeline must retain the tf-roots step");

    assert_eq!(
        roots.matches("-lock-timeout=10m").count(),
        2,
        "plan and apply must each retry same-workspace backend lock acquisition for exactly the bounded ten-minute policy"
    );
    assert!(
        !roots.contains("-lock=false"),
        "tf-roots must never disable Terraform state locking"
    );
    assert!(
        roots.contains("-detailed-exitcode") && roots.contains("plan_exit=$${PIPESTATUS[0]}"),
        "plan must retain detailed-exitcode and the real piped Terraform status"
    );
    assert!(
        roots.contains("-out=\"/workspace/tfplan-$$root\"")
            && roots.contains("\"/workspace/tfplan-$$root\"\n"),
        "apply must retain immutable saved-plan application"
    );
}

#[test]
fn self_hosted_install_uses_one_pinned_checksum_verified_just_action() {
    let workflow = include_str!("../../../../.github/workflows/self-hosted-install.yml");
    let local_action = "uses: ./.github/actions/setup-just";
    assert_eq!(
        workflow.matches(local_action).count(),
        2,
        "contract and smoke must use the same repo-local just installer action"
    );
    assert!(
        !workflow.contains("just.systems") && !workflow.contains("curl | sudo bash"),
        "the workflow must never execute the mutable just.systems installer"
    );

    let action_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.github/actions/setup-just/action.yml");
    let action = fs::read_to_string(&action_path)
        .unwrap_or_else(|error| panic!("read {}: {error}", action_path.display()));

    for required in [
        "JUST_VERSION: \"1.55.1\"",
        "just-1.55.1-x86_64-unknown-linux-musl.tar.gz",
        "b0ef600f0df20d5ae91ae931627c499fc52b477ffe5f5ea7b7b3ec616b16c778",
        "actions/cache@0057852bfaa89a56745cba8c7296529d2fc39830",
        "${{ runner.os }}",
        "${{ runner.arch }}",
        "${{ steps.release.outputs.version }}",
        "${{ steps.release.outputs.sha256 }}",
        "sha256sum --check --status",
        "https://github.com/casey/just/releases/download/",
        "5|6|7|18|28|35|52|55|56|92",
        "408|429|5??",
        "just --version",
    ] {
        assert!(
            action.contains(required),
            "the shared just action must retain `{required}`"
        );
    }
    assert!(
        !action.contains("just.systems") && !action.contains("SHA256SUMS"),
        "the action must use the reviewed checksum without another mutable runtime fetch"
    );
}

#[test]
fn ci_and_terraform_pipelines_post_no_pr_comments() {
    // Registered decision (chat 2b73ccb9, 2026-07-31): the CI pipelines post
    // no PR comments — the lane-scoped post-ci-comment steps were removed
    // because even under the 2026-07-22 post-on-failure policy (chat
    // cddc123c) the stage table duplicated what the PR's status checks and
    // build logs already convey. The terraform pipeline had already dropped
    // its post-pr-comment step (chat cddc123c: a failed plan aborts the
    // build before any comment step could run, so a plan comment could only
    // ever report success — nine comments on one PR). Pin all three pipeline
    // configs comment-free (cicd I7, cloudbuild_pipelines D14).
    let configs = [
        (
            "cloudbuild-ci-rust.yaml",
            include_str!("../../../../infra/ci/cloudbuild-ci-rust.yaml"),
        ),
        (
            "cloudbuild-ci-e2e.yaml",
            include_str!("../../../../infra/ci/cloudbuild-ci-e2e.yaml"),
        ),
        (
            "cloudbuild-terraform-qa.yaml",
            include_str!("../../../../infra/ci/cloudbuild-terraform-qa.yaml"),
        ),
    ];
    for (label, yaml) in configs {
        for (idx, line) in yaml.lines().enumerate() {
            let trimmed = line.trim_start();
            // Skip YAML/inline-script comment lines: an explanatory comment
            // may cite the forbidden tokens (same self-trip hazard the `:-`
            // spelling and jq scanners skip).
            if trimmed.starts_with('#') {
                continue;
            }
            for forbidden in [
                "post-ci-comment",
                "post-pr-comment",
                "api.github.com",
                "_GITHUB_COMMENT_TOKEN_SECRET_ID",
            ] {
                assert!(
                    !trimmed.contains(forbidden),
                    "{label} line {}: must stay comment-free (found `{forbidden}`): the CI and terraform pipelines post no PR comments (cicd I7, cloudbuild_pipelines D14): {trimmed}",
                    idx + 1
                );
            }
        }
    }
}
