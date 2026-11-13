//! Executable coverage catalog for the workspace VM MVP receipt.
//!
//! The catalog maps each delivered package to at least one retained executable
//! boundary. `scripts/value_loop_receipt.py` uses the same file to drive the
//! exact-candidate Cloud Build receipt.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

const CATALOG: &str = include_str!("../../value_loop_matrix.tsv");
const EXPECTED_PACKAGES: [&str; 66] = [
    "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "P2.1", "P2.2", "P2.4", "P4.3a",
    "P4.3b", "P4.4a", "P4.4b", "P4.5", "P4.6", "P4.7", "P4.8", "P4.9a", "P4.9b", "P4.10", "P4.11",
    "P4.13", "P5.1", "P5.2", "M1", "M2", "M3", "M4", "M5", "M6", "M7", "M8", "M9", "M10", "M11",
    "M12", "M13", "M14", "RV1", "RV2", "RV3", "RV4", "RV5", "RV6", "RV7", "RV8", "RV9", "RV10",
    // Core shipping, first-run, issue and alignment paths (G5Labs/G5-Lean#976).
    "SH1", "SH2", "FR1", "IS1", "ORG1", "AP1", "DC1", "DC2", "DC3", "ON1", "ON2", "ON3", "M15",
    "M16",
];

#[derive(Debug)]
struct CatalogRow<'a> {
    package: &'a str,
    lane: &'a str,
    profile: &'a str,
    selector: &'a str,
    source: &'a str,
    proof: &'a str,
}

fn catalog_rows() -> Vec<CatalogRow<'static>> {
    CATALOG
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            if index == 0 || line.trim().is_empty() {
                return None;
            }
            let fields = line.split('\t').collect::<Vec<_>>();
            assert_eq!(
                fields.len(),
                6,
                "workspace MVP catalog line {} must have six tab-separated fields: {line:?}",
                index + 1
            );
            Some(CatalogRow {
                package: fields[0],
                lane: fields[1],
                profile: fields[2],
                selector: fields[3],
                source: fields[4],
                proof: fields[5],
            })
        })
        .collect()
}

#[test]
fn workspace_mvp_catalog_maps_every_package_to_retained_executable_boundaries() {
    let rows = catalog_rows();
    assert!(!rows.is_empty(), "workspace MVP catalog must not be empty");

    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("e2e crate must live under tests/e2e");
    let mut by_package = BTreeMap::<&str, usize>::new();
    let mut seen_rows = BTreeSet::new();
    let mut selector_profiles = BTreeMap::<(&str, &str), &str>::new();
    let mut observed_profiles = BTreeSet::new();

    for row in &rows {
        for (field_name, field) in [
            ("package", row.package),
            ("lane", row.lane),
            ("profile", row.profile),
            ("selector", row.selector),
            ("source", row.source),
            ("proof", row.proof),
        ] {
            assert!(!field.is_empty(), "{} has empty {field_name}", row.package);
            assert_eq!(
                field,
                field.trim(),
                "{} has surrounding whitespace in {field_name}",
                row.package
            );
        }
        assert!(
            matches!(row.lane, "api" | "e2e" | "playwright"),
            "{} has unknown lane {:?}",
            row.package,
            row.lane
        );
        let expected_lane = match row.profile {
            "api-integration" => "api",
            "rust-e2e-broad" => "e2e",
            "playwright-workspace-vm" => "playwright",
            profile => panic!("{} has unknown execution profile {profile:?}", row.package),
        };
        assert_eq!(
            row.lane, expected_lane,
            "{} selector {} uses profile {} in the wrong lane",
            row.package, row.selector, row.profile
        );
        observed_profiles.insert(row.profile);
        assert!(
            seen_rows.insert((row.package, row.lane, row.profile, row.selector)),
            "duplicate workspace MVP catalog row: {} {} {} {}",
            row.package,
            row.lane,
            row.profile,
            row.selector
        );
        if let Some(existing) = selector_profiles.insert((row.lane, row.selector), row.profile) {
            assert_eq!(
                existing, row.profile,
                "selector/profile ambiguity for {} {}: {existing} versus {}",
                row.lane, row.selector, row.profile
            );
        }

        let source_path = repo_root.join(row.source);
        let source = std::fs::read_to_string(&source_path).unwrap_or_else(|error| {
            panic!(
                "{} selector {} names unreadable source {}: {error}",
                row.package,
                row.selector,
                source_path.display()
            )
        });
        if row.lane != "playwright" {
            assert!(
                source.contains(row.selector),
                "{} selector {} is absent from {}",
                row.package,
                row.selector,
                source_path.display()
            );
        }
        *by_package.entry(row.package).or_default() += 1;
    }

    let expected = EXPECTED_PACKAGES.into_iter().collect::<BTreeSet<_>>();
    let actual = by_package.keys().copied().collect::<BTreeSet<_>>();
    assert_eq!(
        actual, expected,
        "catalog must cover all required MVP loops"
    );
    assert_eq!(
        observed_profiles,
        BTreeSet::from([
            "api-integration",
            "playwright-workspace-vm",
            "rust-e2e-broad",
        ]),
        "catalog must exercise every retained execution profile"
    );

    let runner = include_str!("../../../../scripts/run-rust-e2e-lanes.sh");
    assert!(
        runner.contains("\n  r2_guest_lifecycle\n"),
        "the regular Rust E2E runner must select the public terminal lifecycle scenarios"
    );
    assert!(
        runner.contains("\n  shared_vm_chats\n"),
        "the regular Rust E2E runner must select the concurrent chat scenario"
    );
}
