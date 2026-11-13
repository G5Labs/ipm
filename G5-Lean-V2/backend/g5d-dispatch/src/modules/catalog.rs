//! The hash-pinned declared-module catalog.
//!
//! The catalog ships in the **host runtime bundle** mounted read-only at
//! `/opt/g5` (design v9 §6.3), never in the repository-selected guest image: a
//! workspace may select or configure an installed module, but cannot add code
//! or privilege. The dispatcher loads `/opt/g5/modules/catalog.json`, verifies
//! its content hash against the pin it was started with, and thereafter
//! resolves only immutable entries from it.
//!
//! A manifest *requests* capabilities; it never grants them. The grant arrives
//! separately as a control-plane `grant_id`, and the runtime intersects the two
//! (see [`ModuleEntry::intersect`]).

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Lifecycle class. The distinction is load-bearing for limits: a `service`
/// legitimately has no wall-clock bound and consumes no bounded-execution slot,
/// while a `bounded_run` must declare one.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    BoundedRun,
    Service,
}

impl Lifecycle {
    pub fn is_bounded(self) -> bool {
        matches!(self, Self::BoundedRun)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    /// Required for `bounded_run`; forbidden for `service`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wall_clock_ms: Option<u64>,
    pub cpu_cores: u32,
    pub memory_mib: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disk_mib: Option<u64>,
    /// Cap on the module's stdout. Required for every module except
    /// `agent.turn`, which must not declare one: an agent turn's output has
    /// no cumulative limit (`g5.agents.output-bounds`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_bytes: Option<u64>,
    pub max_log_bytes: u64,
    /// Concurrent executions of this module permitted on one incarnation.
    pub concurrency: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct OperationSchemas {
    pub input_schema: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_schema: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Observability {
    pub logs: String,
    pub events: String,
    #[serde(default)]
    pub redaction: String,
    #[serde(default)]
    pub health_probes: Vec<String>,
    #[serde(default)]
    pub diagnostic_profile: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ModuleEntry {
    pub module_id: String,
    pub module_version: String,
    pub manifest_hash: String,
    pub lifecycle: Lifecycle,
    /// Pinned entrypoint. A caller never supplies argv, environment or cwd;
    /// the runner derives the whole command line from this entry.
    pub entrypoint: Vec<String>,
    pub limits: Limits,
    pub operations: BTreeMap<String, OperationSchemas>,
    #[serde(default)]
    pub requested_capabilities: Vec<String>,
    pub observability: Observability,
    /// Built-in modules are served by in-process handlers rather than the
    /// process runner. The entrypoint still names them for audit and for the
    /// catalog-integrity probe.
    #[serde(default)]
    pub builtin: bool,
}

impl ModuleEntry {
    /// Intersect the capabilities this manifest requests with the capabilities
    /// the control plane actually granted. The result can only ever shrink:
    /// a manifest cannot widen a grant, and a grant cannot confer a capability
    /// the module never declared.
    pub fn intersect(&self, granted: &[String]) -> Vec<String> {
        self.requested_capabilities
            .iter()
            .filter(|capability| granted.iter().any(|g| g == *capability))
            .cloned()
            .collect()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub catalog_version: String,
    pub modules: Vec<ModuleEntry>,
    /// Maximum bounded executions in flight on one incarnation, shared by
    /// agent, command and task-less bounded operations. Services are exempt.
    pub bounded_execution_slots: u32,
    #[serde(skip)]
    pub content_hash: String,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum CatalogError {
    #[error("catalog unreadable: {0}")]
    Unreadable(String),
    #[error("catalog malformed: {0}")]
    Malformed(String),
    #[error("catalog hash {actual} does not match the pinned {expected}")]
    HashMismatch { expected: String, actual: String },
    #[error("module {module_id}: {reason}")]
    Invalid { module_id: String, reason: String },
}

impl Catalog {
    /// Load and verify the catalog. `expected_hash` is the pin the dispatcher
    /// was started with; when absent (test fixtures) the computed hash is
    /// still recorded so the integrity probe and `runtime.inspect.summary`
    /// report a real value.
    pub fn load(path: &Path, expected_hash: Option<&str>) -> Result<Self, CatalogError> {
        let bytes = std::fs::read(path).map_err(|e| CatalogError::Unreadable(e.to_string()))?;
        Self::from_bytes(&bytes, expected_hash)
    }

    pub fn from_bytes(bytes: &[u8], expected_hash: Option<&str>) -> Result<Self, CatalogError> {
        let content_hash = format!("{:x}", Sha256::digest(bytes));
        if let Some(expected) = expected_hash {
            if !expected.eq_ignore_ascii_case(&content_hash) {
                return Err(CatalogError::HashMismatch {
                    expected: expected.to_owned(),
                    actual: content_hash,
                });
            }
        }
        let mut catalog: Self =
            serde_json::from_slice(bytes).map_err(|e| CatalogError::Malformed(e.to_string()))?;
        catalog.content_hash = content_hash;
        catalog.validate()?;
        Ok(catalog)
    }

    fn validate(&self) -> Result<(), CatalogError> {
        if self.bounded_execution_slots == 0 {
            return Err(CatalogError::Malformed(
                "bounded_execution_slots must be positive".into(),
            ));
        }
        let mut seen = BTreeMap::new();
        for module in &self.modules {
            let invalid = |reason: &str| CatalogError::Invalid {
                module_id: module.module_id.clone(),
                reason: reason.to_owned(),
            };
            if module.module_id.is_empty() || module.manifest_hash.is_empty() {
                return Err(invalid("module_id and manifest_hash are required"));
            }
            if seen
                .insert(
                    (module.module_id.clone(), module.module_version.clone()),
                    (),
                )
                .is_some()
            {
                return Err(invalid("duplicate module_id and version"));
            }
            if module.operations.is_empty() {
                return Err(invalid("at least one operation must be declared"));
            }
            if module.entrypoint.is_empty() {
                return Err(invalid("a pinned entrypoint is required"));
            }
            // An entrypoint is the dispatcher binary itself, a path contained
            // in the read-only runtime mount, or — for a built-in only — the
            // name of its in-process handler; nothing resolves through PATH
            // or outside the mount.
            let program = module.entrypoint[0].as_str();
            let allowed = program == "g5d"
                || contained_in_runtime_mount(program)
                || (module.builtin && program.starts_with("builtin:"));
            if !allowed {
                return Err(invalid(
                    "an entrypoint must be `g5d`, an absolute path inside /opt/g5, or (built-ins only) builtin:<module_id>",
                ));
            }
            // The load-bearing lifecycle/limits rule: a bounded run must be
            // bounded, and a service must not pretend to be.
            match module.lifecycle {
                Lifecycle::BoundedRun => {
                    // Finite runs report an outcome; they need not have a lifetime cap.
                }
                Lifecycle::Service => {
                    if module.limits.wall_clock_ms.is_some() {
                        return Err(invalid("service modules must not declare wall_clock_ms"));
                    }
                }
            }
            if module.limits.cpu_cores == 0
                || module.limits.memory_mib == 0
                || module.limits.max_log_bytes == 0
                || module.limits.concurrency == 0
            {
                return Err(invalid(
                    "cpu, memory, log and concurrency limits are required",
                ));
            }
            // An agent turn's output is bounded per event, never in total
            // (`g5.agents.output-bounds`). Refusing a declared cap keeps a
            // stale entry from quietly restoring the limit; every other
            // module still declares one.
            match (
                module.module_id == "agent.turn",
                module.limits.max_output_bytes,
            ) {
                (true, None) => {}
                (true, Some(_)) => {
                    return Err(invalid("agent.turn must not declare max_output_bytes"));
                }
                (false, Some(bytes)) if bytes > 0 => {}
                (false, _) => return Err(invalid("an output limit is required")),
            }
        }
        Ok(())
    }

    pub fn find(&self, module_id: &str, version: Option<&str>) -> Option<&ModuleEntry> {
        self.modules.iter().find(|module| {
            module.module_id == module_id
                && version.is_none_or(|version| module.module_version == version)
        })
    }
}

/// The read-only runtime mount every non-`g5d` process entrypoint must live
/// in, checked lexically: absolute, below the mount, only normal components
/// (no `.`/`..`), and not the mount root itself.
pub const RUNTIME_MOUNT: &str = "/opt/g5";

fn contained_in_runtime_mount(program: &str) -> bool {
    use std::path::{Component, Path};
    let path = Path::new(program);
    let mount = Path::new(RUNTIME_MOUNT);
    path.is_absolute()
        && path.starts_with(mount)
        && path != mount
        && path
            .components()
            .all(|component| matches!(component, Component::RootDir | Component::Normal(_)))
}

#[cfg(test)]
mod shipped {
    use super::*;

    /// The catalog exported into the host runtime bundle must load under its
    /// own pin, declare the MVP modules, and refuse a wrong pin.
    #[test]
    fn shipped_catalog_is_valid_and_hash_file_matches() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("modules");
        let bytes = std::fs::read(dir.join("catalog.json")).unwrap();
        let pinned = std::fs::read_to_string(dir.join("catalog.sha256")).unwrap();
        let catalog = Catalog::from_bytes(&bytes, Some(pinned.trim())).unwrap();
        assert_eq!(
            catalog
                .modules
                .iter()
                .map(|m| m.module_id.as_str())
                .collect::<Vec<_>>(),
            [
                "workspace.changes",
                "workspace.bootstrap",
                "agent.turn",
                "agent.turn",
                "agent.turn",
                "agent.turn",
                "agent.turn",
                "agent.turn",
                "agent.turn",
                "agent.turn",
                "agent.turn",
                "agent.turn",
                "workspace.preview",
                "workspace.diagnostics"
            ]
        );
        assert!(catalog
            .modules
            .iter()
            .all(|module| match module.module_id.as_str() {
                "agent.turn" if matches!(module.module_version.as_str(), "1.0.5" | "1.0.8") =>
                    module.lifecycle == Lifecycle::BoundedRun
                        && module.limits.wall_clock_ms.is_none(),
                "workspace.preview" => {
                    module.lifecycle == Lifecycle::Service && module.limits.wall_clock_ms.is_none()
                }
                _ => {
                    module.lifecycle == Lifecycle::BoundedRun
                        && module.limits.wall_clock_ms.is_some()
                }
            }));
        // No agent turn has an output cap; every other module declares one
        // (`g5.agents.output-bounds`).
        assert!(catalog.modules.iter().all(|module| {
            (module.module_id == "agent.turn") == module.limits.max_output_bytes.is_none()
        }));
        assert!(!catalog.find("agent.turn", None).unwrap().builtin);
        assert_eq!(
            catalog.find("agent.turn", None).unwrap().module_version,
            "1.0.5"
        );
        assert_eq!(
            catalog
                .find("agent.turn", Some("1.0.6"))
                .unwrap()
                .limits
                .wall_clock_ms,
            Some(3_000_000),
        );
        assert_eq!(
            catalog
                .find("workspace.bootstrap", None)
                .unwrap()
                .entrypoint,
            ["g5d", "bootstrap"]
        );
        assert!(catalog.bounded_execution_slots > 0);
        assert!(matches!(
            Catalog::from_bytes(&bytes, Some(&"0".repeat(64))),
            Err(CatalogError::HashMismatch { .. })
        ));
    }

    /// The output cap is optional in the schema only for `agent.turn`: a
    /// catalog that gives an agent turn one, or leaves any other module
    /// without one (absent or zero), does not load.
    #[test]
    fn only_agent_turns_run_without_an_output_cap() {
        let bytes = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("modules/catalog.json"),
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let load = |edit: &dyn Fn(&mut serde_json::Value)| {
            let mut value = value.clone();
            for module in value["modules"].as_array_mut().unwrap() {
                edit(module);
            }
            Catalog::from_bytes(&serde_json::to_vec(&value).unwrap(), None)
        };
        assert!(load(&|_| {}).is_ok());
        assert_eq!(
            load(&|module| {
                if module["module_id"] == "agent.turn" && module["module_version"] == "1.0.8" {
                    module["limits"]["max_output_bytes"] = serde_json::json!(67_108_864u64);
                }
            })
            .unwrap_err(),
            CatalogError::Invalid {
                module_id: "agent.turn".into(),
                reason: "agent.turn must not declare max_output_bytes".into(),
            }
        );
        for missing in [None, Some(0)] {
            for module_id in [
                "workspace.changes",
                "workspace.bootstrap",
                "workspace.preview",
                "workspace.diagnostics",
            ] {
                assert_eq!(
                    load(&|module| {
                        if module["module_id"] == module_id {
                            let limits = module["limits"].as_object_mut().unwrap();
                            match missing {
                                Some(zero) => {
                                    limits.insert("max_output_bytes".into(), zero.into());
                                }
                                None => {
                                    limits.remove("max_output_bytes");
                                }
                            }
                        }
                    })
                    .unwrap_err(),
                    CatalogError::Invalid {
                        module_id: module_id.into(),
                        reason: "an output limit is required".into(),
                    },
                    "{module_id} {missing:?}"
                );
            }
        }
    }

    #[test]
    fn process_entrypoints_are_g5d_or_contained_in_the_runtime_mount() {
        let bytes = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("modules/catalog.json"),
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let bootstrap = value["modules"]
            .as_array()
            .unwrap()
            .iter()
            .position(|m| m["module_id"] == "workspace.bootstrap")
            .unwrap();
        let with_entrypoint = |entrypoint: serde_json::Value, builtin: bool| {
            let mut value = value.clone();
            value["modules"][bootstrap]["entrypoint"] = entrypoint;
            value["modules"][bootstrap]["builtin"] = serde_json::Value::Bool(builtin);
            Catalog::from_bytes(&serde_json::to_vec(&value).unwrap(), None)
        };
        for (entrypoint, builtin, ok) in [
            (serde_json::json!(["g5d", "bootstrap"]), false, true),
            (serde_json::json!(["/opt/g5/bin/tool"]), false, true),
            (serde_json::json!(["git-helper"]), false, false),
            (serde_json::json!(["/bin/sh", "-c", "x"]), false, false),
            (serde_json::json!(["/opt/g5/../etc/x"]), false, false),
            (serde_json::json!(["/opt/g5"]), false, false),
            (serde_json::json!(["/opt/g5x/tool"]), false, false),
            (
                serde_json::json!(["builtin:workspace.bootstrap"]),
                false,
                false,
            ),
            (
                serde_json::json!(["builtin:workspace.bootstrap"]),
                true,
                true,
            ),
            (serde_json::json!(["g5d", "bootstrap"]), true, true),
            (serde_json::json!(["bootstrap"]), true, false),
        ] {
            assert_eq!(
                with_entrypoint(entrypoint.clone(), builtin).is_ok(),
                ok,
                "{entrypoint} builtin={builtin}"
            );
        }
    }
}
