//! Structural guard: a shared row shape has one reader, held by its owner.
//!
//! TaskService alone decodes the attempt policy snapshot
//! (`task_attempts.policy_snapshot_ref`, `g5.architecture.task_owner`), and
//! ChatService alone reads platform notice artifacts
//! (`g5.architecture.conversations`). Other owners receive decoded values.

use std::{
    fs,
    path::{Path, PathBuf},
};

use walkdir::WalkDir;

/// Each shape's marker and the only files that may name it, relative to the
/// backend directory.
const SHAPE_OWNERS: &[(&str, &[&str])] = &[
    (
        "policy_snapshot_ref",
        &[
            "g5-api-core/src/services/agents/agent_dispatch.rs",
            "g5-api-core/src/services/agents/agent_steering.rs",
            "g5-api-core/src/services/agents/attempt_snapshot.rs",
            "g5-api-core/src/services/agents/codex_auth.rs",
            "g5-api-core/src/services/agents/subscription_usage.rs",
            "g5-api-core/src/services/agents/task_lifecycle.rs",
            "g5-api-core/src/services/agents/task_service.rs",
            // The row model declares the column and decodes nothing.
            "g5-api-data/src/models/runtime.rs",
        ],
    ),
    (
        "platform_notice",
        &["g5-api-core/src/services/agents/chat_service.rs"],
    ),
];

fn backend_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("g5-api crate must be beneath the backend directory")
        .to_path_buf()
}

#[test]
fn shared_shapes_are_read_only_by_their_owners() {
    let backend = backend_dir();
    let mut violations = Vec::new();
    for crate_name in [
        "g5-api",
        "g5-api-core",
        "g5-api-data",
        "g5-api-foundation",
        "g5-api-routes",
    ] {
        for entry in WalkDir::new(backend.join(crate_name).join("src")) {
            let entry = entry.expect("walk API source");
            let path = entry.path();
            if !entry.file_type().is_file()
                || path.extension().and_then(|extension| extension.to_str()) != Some("rs")
            {
                continue;
            }
            let relative = path
                .strip_prefix(&backend)
                .expect("API source is beneath the backend directory")
                .to_string_lossy()
                .replace('\\', "/");
            let source = fs::read_to_string(path).expect("read API Rust source");
            for (marker, owners) in SHAPE_OWNERS {
                if source.contains(marker) && !owners.contains(&relative.as_str()) {
                    violations.push(format!("{relative} names `{marker}`"));
                }
            }
        }
    }
    assert!(
        violations.is_empty(),
        "a shared shape is read only by its owner; pass decoded values instead:\n{}",
        violations.join("\n")
    );
}

#[test]
fn every_named_owner_still_reads_its_shape() {
    let backend = backend_dir();
    for (marker, owners) in SHAPE_OWNERS {
        for owner in *owners {
            let source = fs::read_to_string(backend.join(owner))
                .unwrap_or_else(|error| panic!("read {owner}: {error}"));
            assert!(
                source.contains(marker),
                "{owner} no longer names `{marker}`; remove it from the guard"
            );
        }
    }
}
