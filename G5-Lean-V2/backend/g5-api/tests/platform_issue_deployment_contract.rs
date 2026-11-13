//! Deployment-contract source test for the split-role platform-issue delivery fix.
//!
//! The hosted deployment renders two workloads from the same g5-api binary: the
//! `g5-api` Deployment (`G5_ROLE=api`, HTTP intake + durable enqueue) and the
//! `g5-daemon` Deployment (`G5_ROLE=daemon`, which owns the
//! `MaintenanceRunner` and therefore the platform-issue outbox delivery loop).
//! Both processes must receive the dedicated optional
//! `g5-platform-issue-reporting-secrets` Secret; otherwise the API accepts a
//! report (HTTP 202) while the daemon resolves delivery as disabled and never
//! spawns the outbox loop, so the report is enqueued but never delivered.
//!
//! This is a fast, dependency-free STATIC source contract: it reads the raw Helm
//! chart templates (no Helm render, no cluster) and asserts that a single shared
//! helper in `_helpers.tpl` owns the common g5-api-binary secret projection and
//! that BOTH Deployments consume it under `envFrom`. It fails on the current
//! daemon template — which projects only the core secret — and reports exactly
//! which workload contract is incomplete.
//!
//! Ontology: software_architecture/infra/helm [D7]/[I10]/[D25];
//! software_architecture/infra/terraform_secrets [D11].

use std::fs;
use std::path::{Path, PathBuf};

/// Named helper that owns the common g5-api-binary process secret sources.
const HELPER_NAME: &str = "g5.apiSecretEnvFrom";
/// The dedicated, isolated, optional platform-issue-reporting Secret.
const DEDICATED_SECRET: &str = "g5-platform-issue-reporting-secrets";
/// The values path for the core g5-api Secret name (`g5-api-secrets`).
const CORE_SECRET_REF: &str = ".Values.api.secrets.secretName";

fn templates_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../infra/helm/g5/templates")
        .canonicalize()
        .expect("chart templates directory should exist relative to g5-api crate")
}

fn read_template(name: &str) -> String {
    let path = templates_dir().join(name);
    fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()))
}

/// Extract the body of a named `{{- define "<name>" -}}` ... `{{- end -}}` block
/// from a Helm template, so the shared secret projection can be asserted in one
/// place rather than duplicated per workload.
fn helper_body(source: &str, name: &str) -> Option<String> {
    let needle = format!("define \"{name}\"");
    let start = source.find(&needle)?;
    // Body begins after the line that opens the define.
    let after_define = source[start..].find('\n')? + start + 1;
    let end_rel = source[after_define..].find("{{- end")?;
    Some(source[after_define..after_define + end_rel].to_string())
}

/// True when `haystack` uses the shared helper via `include "<HELPER_NAME>"`
/// after an `envFrom:` line (i.e. the container consumes the shared projection
/// as environment, not somewhere unrelated).
fn includes_helper_under_envfrom(haystack: &str) -> bool {
    let include = format!("include \"{HELPER_NAME}\"");
    match haystack.find("envFrom:") {
        Some(envfrom_at) => match haystack.find(&include) {
            Some(include_at) => include_at > envfrom_at,
            None => false,
        },
        None => false,
    }
}

/// Parse the helper body's `- secretRef:` entries into `(name, optional)` pairs.
/// `name` is the raw right-hand side of the `name:` key (a template expression
/// like `{{ .Values.api.secrets.secretName }}` or a literal secret name), and
/// `optional` records whether that entry sets `optional: true`. This lets the
/// test bind the optional flag to a specific secret rather than accepting it
/// anywhere in the body.
fn secret_ref_entries(body: &str) -> Vec<(String, bool)> {
    let mut entries: Vec<(String, bool)> = Vec::new();
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("- secretRef:") {
            entries.push((String::new(), false));
        } else if let Some(rest) = trimmed.strip_prefix("name:") {
            if let Some(last) = entries.last_mut() {
                last.0 = rest.trim().to_string();
            }
        } else if trimmed == "optional: true" {
            if let Some(last) = entries.last_mut() {
                last.1 = true;
            }
        }
    }
    entries
}

/// Whether the single secretRef entry whose name contains `name_needle` sets
/// `optional: true`. `None` means no such entry exists.
fn entry_optional(body: &str, name_needle: &str) -> Option<bool> {
    secret_ref_entries(body)
        .into_iter()
        .find(|(name, _)| name.contains(name_needle))
        .map(|(_, optional)| optional)
}

#[test]
fn shared_secret_projection_is_consumed_by_both_api_and_daemon() {
    let helpers = read_template("_helpers.tpl");
    let api = read_template("deployment-api.yaml");
    let daemon = read_template("deployment-daemon.yaml");

    let mut violations: Vec<String> = Vec::new();

    // 1. The shared helper must exist and own BOTH secret sources.
    match helper_body(&helpers, HELPER_NAME) {
        None => violations.push(format!(
            "_helpers.tpl must define a single shared helper \"{HELPER_NAME}\" \
             that owns the common g5-api-binary secret envFrom sources"
        )),
        Some(body) => {
            if !body.contains(CORE_SECRET_REF) {
                violations.push(format!(
                    "helper \"{HELPER_NAME}\" must project the core secret via {CORE_SECRET_REF}"
                ));
            }
            // The optional flag must belong to the dedicated secret specifically,
            // not merely appear somewhere in the body: a swapped helper where the
            // core secret is optional and the dedicated one is required must fail.
            match entry_optional(&body, DEDICATED_SECRET) {
                None => violations.push(format!(
                    "helper \"{HELPER_NAME}\" must project the dedicated {DEDICATED_SECRET}"
                )),
                Some(false) => violations.push(format!(
                    "helper \"{HELPER_NAME}\" must mark the dedicated {DEDICATED_SECRET} entry \
                     as optional: true so an absent Secret keeps delivery fail-closed without \
                     blocking startup"
                )),
                Some(true) => {}
            }
            // The core secret must NOT be optional; making it optional would let
            // either role start without its primary configuration.
            if entry_optional(&body, "secretName") == Some(true) {
                violations.push(format!(
                    "helper \"{HELPER_NAME}\" must not mark the core secret ({CORE_SECRET_REF}) \
                     optional; only {DEDICATED_SECRET} is optional"
                ));
            }
        }
    }

    // 2. The API Deployment must consume the shared helper under envFrom.
    if !includes_helper_under_envfrom(&api) {
        violations.push(format!(
            "deployment-api.yaml must consume the shared secret helper \
             \"{HELPER_NAME}\" under its container envFrom"
        ));
    }

    // 3. The daemon Deployment must consume the SAME shared helper under envFrom.
    //    This is the split-role delivery gap: the daemon owns the outbox worker,
    //    so without this projection a submission enqueues on the API but is never
    //    delivered.
    if !includes_helper_under_envfrom(&daemon) {
        violations.push(format!(
            "deployment-daemon.yaml does NOT project {DEDICATED_SECRET}: the daemon \
             (G5_ROLE=daemon) owns the MaintenanceRunner and platform-issue outbox \
             delivery loop, so it must consume the shared secret helper \"{HELPER_NAME}\" \
             under its container envFrom — otherwise reports enqueue on the API but are \
             never delivered"
        ));
    }

    // 4. Neither workload may carry a second, ad-hoc copy of the dedicated
    //    reference outside the shared helper (single source of truth, so a future
    //    secret-bundle change cannot silently reach only one role).
    if api.contains(DEDICATED_SECRET) {
        violations.push(format!(
            "deployment-api.yaml must not reference {DEDICATED_SECRET} directly; \
             the reference belongs only in the shared helper \"{HELPER_NAME}\""
        ));
    }
    if daemon.contains(DEDICATED_SECRET) {
        violations.push(format!(
            "deployment-daemon.yaml must not reference {DEDICATED_SECRET} directly; \
             the reference belongs only in the shared helper \"{HELPER_NAME}\""
        ));
    }

    assert!(
        violations.is_empty(),
        "split-role platform-issue secret projection contract is incomplete:\n  - {}",
        violations.join("\n  - ")
    );
}
