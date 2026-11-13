//! The services and maintenance jobs moved into context modules (P8.5 tranche
//! 2) keep their historical tracing targets: a `RUST_LOG` directive written
//! against `g5_api::services::<name>` or `g5_api::maintenance::<job>` still
//! selects the module, and rendered events still carry the historical target.

use std::io;
use std::sync::{Arc, Mutex};

use g5_contracts::tracing_compat::{legacy_compatible_env_filter, LegacyTargetFormatter};
use tracing_subscriber::fmt::MakeWriter;

#[derive(Clone)]
struct Capture(Arc<Mutex<Vec<u8>>>);

impl io::Write for Capture {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for Capture {
    type Writer = Capture;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

/// Expand `RUST_LOG` through the API crate's legacy families (the one test
/// below owns the process-wide variable).
fn expand(filter: &str) -> String {
    std::env::set_var("RUST_LOG", filter);
    let expanded = legacy_compatible_env_filter("info", g5_api::LEGACY_TRACING_TARGET_FAMILIES);
    std::env::remove_var("RUST_LOG");
    expanded.to_string()
}

fn broad_historical_filters_still_select_the_composition_roots() {
    // `g5_api::maintenance` now has descendant mappings (the moved jobs), so
    // its expansion goes through them; the runner's own events under
    // `g5_api_core::maintenance` must stay selected, and so must the moved jobs.
    // The runner's own events carry `g5_api::maintenance` literally, so the
    // directive selects them as written; the jobs come through the table.
    let directives = expand("off,g5_api::maintenance=info");
    assert!(
        directives.contains("g5_api::maintenance=info"),
        "{directives}"
    );
    assert!(
        directives.contains("g5_api_core::services::git::repo_gc=info"),
        "{directives}"
    );
    let directives = expand("off,g5_api::services=warn");
    assert!(directives.contains("g5_api::services=warn"), "{directives}");
    assert!(
        directives.contains("g5_api_core::services::identity::org_service=warn"),
        "{directives}"
    );
    // A broad service directive reaches the services only: the maintenance
    // jobs, physically under the same contexts, keep the default level.
    let directives = expand("info,g5_api::services=off");
    assert!(
        directives.contains("g5_api_core::services::identity::org_service=off"),
        "{directives}"
    );
    assert!(!directives.contains("repo_gc"), "{directives}");
    assert!(
        !directives.contains("g5_api_core::services=off"),
        "{directives}"
    );
    // An abbreviated historical target selects every module it prefixes,
    // across contexts, as `EnvFilter` would have matched the flat modules.
    let directives = expand("off,g5_api::services::workspace_=debug");
    assert!(
        directives.contains("g5_api_core::services::workspace::workspace_service=debug"),
        "{directives}"
    );
    assert!(
        directives.contains("g5_api_core::services::git::workspace_ops=debug"),
        "{directives}"
    );
    // A field selector is its own matcher next to the plain one.
    let directives = expand(
        "off,g5_api::maintenance::repo_gc[{repo_id}]=debug,g5_api::maintenance::repo_gc=warn",
    );
    assert!(
        directives.contains("g5_api_core::services::git::repo_gc[{repo_id}]=debug"),
        "{directives}"
    );
    assert!(
        directives.contains("g5_api_core::services::git::repo_gc=warn"),
        "{directives}"
    );
    // A specific historical directive is not demoted by a broad one.
    let directives = expand("g5_api::services::org_service=trace,g5_api::services=warn");
    assert!(
        directives.contains("g5_api_core::services::identity::org_service=trace"),
        "{directives}"
    );
}

#[test]
fn historical_targets_of_moved_modules_select_and_label_them() {
    // Both checks share the process-wide RUST_LOG, so they run in sequence.
    broad_historical_filters_still_select_the_composition_roots();
    std::env::set_var(
        "RUST_LOG",
        "warn,g5_api::maintenance=debug,g5_api::services::org_service=trace",
    );
    let filter = legacy_compatible_env_filter("info", g5_api::LEGACY_TRACING_TARGET_FAMILIES);
    std::env::remove_var("RUST_LOG");
    let directives = filter.to_string();
    assert!(
        directives.contains("g5_api_core::services::git::repo_gc=debug"),
        "{directives}"
    );
    assert!(
        directives.contains("g5_api_core::services::identity::org_service=trace"),
        "{directives}"
    );

    let bytes = Arc::new(Mutex::new(Vec::new()));
    let subscriber = tracing_subscriber::fmt()
        .json()
        .event_format(LegacyTargetFormatter::new(
            tracing_subscriber::fmt::format().json(),
            g5_api::LEGACY_TRACING_TARGET_FAMILIES,
        ))
        .with_env_filter(filter)
        .with_writer(Capture(Arc::clone(&bytes)))
        .finish();
    tracing::subscriber::with_default(subscriber, || {
        tracing::debug!(target: "g5_api_core::services::git::repo_gc::sweep", "selected");
        tracing::trace!(target: "g5_api_core::services::identity::org_service", "selected");
        tracing::info!(target: "g5_api::maintenance", "the runner's own event");
        tracing::debug!(target: "g5_api_core::services::git::branch_commit", "not selected");
    });
    let rendered = String::from_utf8(bytes.lock().unwrap().clone()).unwrap();
    let targets: Vec<String> = rendered
        .lines()
        .map(|line| {
            let value: serde_json::Value = serde_json::from_str(line).unwrap();
            value["target"].as_str().unwrap().to_string()
        })
        .collect();
    assert_eq!(
        targets,
        vec![
            "g5_api::maintenance::repo_gc::sweep".to_string(),
            "g5_api::services::org_service".to_string(),
            "g5_api::maintenance".to_string(),
        ]
    );
}
