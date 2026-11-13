use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

const MIGRATED_CLI_COMPONENT: &str = "software_architecture/g5d/cli/implementation_map";
const MIGRATED_CLI_SPEC_PATH: &str =
    ".semlift/specs/software_architecture/g5d/cli/implementation_map.semlift.md";

fn repo_root_from_manifest() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("g5d crate should live under backend/g5d")
        .to_path_buf()
}

fn write_spec(workspace: &Path, component_id: &str, content: &str) {
    let path = workspace
        .join(".semlift")
        .join("specs")
        .join(format!("{component_id}.semlift.md"));
    fs::create_dir_all(path.parent().expect("spec path should have parent")).unwrap();
    fs::write(path, content).unwrap();
}

fn write_legacy_spec(workspace: &Path, component_id: &str, content: &str) {
    let path = workspace
        .join(".g5")
        .join("specs")
        .join(format!("{component_id}.md"));
    fs::create_dir_all(path.parent().expect("legacy spec path should have parent")).unwrap();
    fs::write(path, content).unwrap();
}

fn g5d_json(workspace: &Path, args: &[&str]) -> Value {
    let output = cargo_bin_cmd!("g5d")
        .arg("--workspace")
        .arg(workspace)
        .arg("--json")
        .args(args)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&output).expect("g5d should print JSON")
}

fn g5d_stdout(workspace: &Path, args: &[&str]) -> String {
    let output = cargo_bin_cmd!("g5d")
        .arg("--workspace")
        .arg(workspace)
        .args(args)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    String::from_utf8(output).expect("g5d stdout should be utf8")
}

fn g5d_failure(workspace: &Path, args: &[&str]) -> String {
    let output = cargo_bin_cmd!("g5d")
        .arg("--workspace")
        .arg(workspace)
        .arg("--json")
        .args(args)
        .assert()
        .failure()
        .get_output()
        .clone();
    let mut combined = output.stdout.clone();
    combined.extend_from_slice(&output.stderr);
    String::from_utf8(combined).expect("g5d failure output should be utf8")
}

fn g5d_json_failure(workspace: &Path, args: &[&str]) -> Value {
    let output = cargo_bin_cmd!("g5d")
        .arg("--workspace")
        .arg(workspace)
        .arg("--json")
        .args(args)
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();
    let mut stream = serde_json::Deserializer::from_slice(&output).into_iter::<Value>();
    stream
        .next()
        .expect("g5d should print failure JSON report to stdout")
        .expect("first stdout value should be JSON")
}

#[test]
fn read_ontology_preserves_old_format_raw_behavior() {
    let dir = tempfile::tempdir().unwrap();
    let content = r#"---
component_id: legacy/orders
---

# Legacy Orders

[I1. Old behavior]
This spec has no v4 references.
"#;
    write_spec(dir.path(), "legacy/orders", content);

    let bundle = g5d_json(
        dir.path(),
        &["spec", "read-ontology", "legacy/orders", "--mode", "both"],
    );

    assert_eq!(bundle["rootComponentId"].as_str(), Some("legacy/orders"));
    assert_eq!(bundle["rawSource"].as_str(), Some(content));
    assert_eq!(bundle["resolvedBlocks"].as_array().unwrap().len(), 0);
    assert_eq!(bundle["omittedBlocks"].as_array().unwrap().len(), 0);

    let human_output = g5d_stdout(
        dir.path(),
        &["spec", "read-ontology", "legacy/orders", "--mode", "both"],
    );
    assert_eq!(human_output, content);
}

#[test]
fn read_ontology_scans_legacy_specs_when_root_is_legacy() {
    let dir = tempfile::tempdir().unwrap();
    write_spec(
        dir.path(),
        "unrelated/new_format",
        "---\ncomponent_id: unrelated/new_format\n---\n# New Format\n",
    );
    let root = r#"---
component_id: legacy/orders
---

# Legacy Orders

[D1. Orders flow]
Use @ref legacy/company#D2 for tenant ownership.
"#;
    let concept = r#"---
component_id: legacy/company
---

# Company

[D2. Company persistence]
Legacy company rows own tenant-visible identity.
"#;
    write_legacy_spec(dir.path(), "legacy/orders", root);
    write_legacy_spec(dir.path(), "legacy/company", concept);

    let bundle = g5d_json(
        dir.path(),
        &[
            "spec",
            "read-ontology",
            "legacy/orders",
            "--mode",
            "resolved",
        ],
    );

    let resolved = bundle["resolvedBlocks"].as_array().unwrap();
    assert_eq!(resolved.len(), 1, "{bundle:#}");
    assert_eq!(
        resolved[0]["targetComponent"].as_str(),
        Some("legacy/company")
    );
    assert_eq!(resolved[0]["targetBlock"].as_str(), Some("D2"));
    assert!(resolved[0]["sourceText"]
        .as_str()
        .unwrap()
        .contains("Legacy company rows"));
}

#[test]
fn read_ontology_fails_closed_on_duplicate_component_ids() {
    let dir = tempfile::tempdir().unwrap();
    let root = r#"---
component_id: software_architecture/orders
---

# Orders

[D1. Orders flow]
Use @ref data_model/company#D2 because orders persist a company pointer.
"#;
    let concept = r#"---
component_id: data_model/company
---

# Company

[D2. Company persistence]
Canonical company shape.
"#;
    let duplicate = r#"---
component_id: data_model/company
---

# Company Duplicate

[D2. Company persistence]
Conflicting company shape from a different file.
"#;
    write_spec(dir.path(), "software_architecture/orders", root);
    write_spec(dir.path(), "data_model/company", concept);
    write_spec(dir.path(), "data_model/company_duplicate", duplicate);

    let stderr = g5d_failure(
        dir.path(),
        &["spec", "read-ontology", "software_architecture/orders"],
    );

    assert!(stderr.contains("duplicate_component_id"), "{stderr}");
    assert!(stderr.contains("data_model/company"), "{stderr}");
    assert!(stderr.contains("company_duplicate.semlift.md"), "{stderr}");
}

#[test]
fn measure_v4_reports_refs_tokens_and_preserved_ref_sets() {
    let dir = tempfile::tempdir().unwrap();
    let concept = r#"---
component_id: data_model/company
type: file
---
# Company

[I1. Company concept]
Company owns the business entity.

[D5. DBML shape]
The DBML shape is normative.
"#;
    let architecture = r#"---
component_id: software_architecture/models/company
type: file
---
# Company Model

[D1. Code-level surface]
The SQLAlchemy model realizes @ref data_model/company#D5 for concrete columns.
**Binding:** maps every concept field into a typed ORM column without schema additions.

## References
- @ref data_model/company#D5
"#;
    write_spec(dir.path(), "data_model/company", concept);
    write_spec(
        dir.path(),
        "software_architecture/models/company",
        architecture,
    );

    let report = g5d_json(
        dir.path(),
        &["spec", "measure-v4", "software_architecture/models/company"],
    );

    assert_eq!(report["valid"].as_bool(), Some(true));
    assert_eq!(report["checked"].as_u64(), Some(1));
    assert_eq!(report["totals"]["documentCount"].as_u64(), Some(1));
    assert!(report["totals"]["rawTokenEstimate"].as_u64().unwrap() > 0);
    assert!(report["totals"]["resolvedTokenEstimate"].as_u64().unwrap() > 0);
    assert!(report["totals"]["tokenDelta"].is_i64());
    assert_eq!(
        report["documents"][0]["inlineRefTargets"][0].as_str(),
        Some("data_model/company#D5")
    );
    assert_eq!(
        report["documents"][0]["derivedRefTargets"][0].as_str(),
        Some("data_model/company#D5")
    );
    assert_eq!(
        report["documents"][0]["refSetPreserved"].as_bool(),
        Some(true)
    );
}

#[test]
fn measure_v4_rejects_weak_whole_node_refs() {
    let dir = tempfile::tempdir().unwrap();
    let concept = r#"---
component_id: data_model/company
type: file
---
# Company

[I1. Company concept]
Company owns the business entity.
"#;
    let weak_architecture = r#"---
component_id: software_architecture/models/company
type: file
---
# Company Model

[D1. Weak concept surface]
The model mentions @ref data_model/company because it stores company data.
"#;
    write_spec(dir.path(), "data_model/company", concept);
    write_spec(
        dir.path(),
        "software_architecture/models/company",
        weak_architecture,
    );

    let report = g5d_json_failure(
        dir.path(),
        &["spec", "measure-v4", "software_architecture/models/company"],
    );

    assert_eq!(report["valid"].as_bool(), Some(false));
    assert_eq!(
        report["documents"][0]["weakWholeNodeRefTargets"][0].as_str(),
        Some("data_model/company")
    );
    assert_eq!(
        report["ontologyLint"]["diagnostics"][0]["ruleId"].as_str(),
        Some("ref_quality_lint")
    );
}

#[test]
fn migrate_v4_dry_run_reports_zero_writes_and_rejects_scaffolding() {
    let dir = tempfile::tempdir().unwrap();
    let scaffold = r#"---
component_id: software_architecture/models/scaffold
type: file
---
# Scaffold

## Legacy intent inventory

[D1. Converted content]
Mechanical migration content must not ship.
"#;
    write_spec(
        dir.path(),
        "software_architecture/models/scaffold",
        scaffold,
    );
    let spec_path = dir
        .path()
        .join(".semlift/specs/software_architecture/models/scaffold.semlift.md");
    let before = fs::read_to_string(&spec_path).unwrap();

    let report = g5d_json_failure(
        dir.path(),
        &[
            "spec",
            "migrate-v4",
            "software_architecture/models/scaffold",
            "--dry-run",
        ],
    );
    let after = fs::read_to_string(&spec_path).unwrap();

    assert_eq!(before, after);
    assert_eq!(report["dryRun"].as_bool(), Some(true));
    assert_eq!(report["valid"].as_bool(), Some(false));
    assert_eq!(report["writes"]["sourceFiles"].as_u64(), Some(0));
    assert_eq!(report["writes"]["gitCommits"].as_u64(), Some(0));
    assert_eq!(report["writes"]["intentRecords"].as_u64(), Some(0));
    assert_eq!(report["writes"]["groundingRecords"].as_u64(), Some(0));
    assert_eq!(
        report["measurement"]["ontologyLint"]["diagnostics"][0]["ruleId"].as_str(),
        Some("anti_scaffolding_lint")
    );
}

#[test]
fn measure_and_migrate_v4_dry_run_accept_real_checked_in_ontology_file() {
    // Keep this real, historical document as test data; the Lean checkout no longer
    // carries the original platform ontology as live workspace content.
    let fixture = repo_root_from_manifest()
        .join("tests/fixtures/ontology-v4/cli-implementation-map.semlift.md");
    let workspace = tempfile::tempdir().unwrap();
    let repo = workspace.path();
    write_spec(
        repo,
        MIGRATED_CLI_COMPONENT,
        &fs::read_to_string(fixture).expect("checked-in CLI ontology fixture should exist"),
    );
    // The historical document has two inline reference targets. Supply their
    // blocks explicitly so reference validation remains active in this fixture.
    write_spec(
        repo,
        "wire_contract/error_taxonomy",
        "---\ncomponent_id: wire_contract/error_taxonomy\n---\n# Error taxonomy\n\n[D1. Closed vocabulary]\nError codes form a shared catalog.\n\n[D2. Status mapping]\nEach code maps to one HTTP status.\n",
    );
    let cli_spec_path = repo.join(MIGRATED_CLI_SPEC_PATH);
    let before = fs::read_to_string(&cli_spec_path).expect("real CLI ontology file should exist");

    let measurement = g5d_json(repo, &["spec", "measure-v4", MIGRATED_CLI_COMPONENT]);

    assert_eq!(measurement["valid"].as_bool(), Some(true));
    assert_eq!(measurement["checked"].as_u64(), Some(1));
    assert_eq!(
        measurement["documents"][0]["componentId"].as_str(),
        Some(MIGRATED_CLI_COMPONENT)
    );
    assert!(
        measurement["documents"][0]["path"]
            .as_str()
            .unwrap()
            .ends_with(MIGRATED_CLI_SPEC_PATH),
        "{measurement:#}"
    );
    assert!(
        measurement["documents"][0]["idBlockCount"]
            .as_u64()
            .unwrap()
            >= 15,
        "{measurement:#}"
    );
    assert!(
        measurement["documents"][0]["rawTokenEstimate"]
            .as_u64()
            .unwrap()
            > 1_000,
        "{measurement:#}"
    );
    assert_eq!(
        measurement["ontologyLint"]["diagnostics"]
            .as_array()
            .unwrap()
            .len(),
        0,
        "{measurement:#}"
    );

    let dry_run = g5d_json(
        repo,
        &["spec", "migrate-v4", MIGRATED_CLI_COMPONENT, "--dry-run"],
    );

    assert_eq!(dry_run["dryRun"].as_bool(), Some(true));
    assert_eq!(dry_run["valid"].as_bool(), Some(true));
    assert_eq!(
        dry_run["measurement"]["documents"][0]["refSetPreserved"].as_bool(),
        Some(true)
    );
    assert_eq!(dry_run["writes"]["sourceFiles"].as_u64(), Some(0));
    assert_eq!(dry_run["writes"]["gitCommits"].as_u64(), Some(0));
    assert_eq!(dry_run["writes"]["intentRecords"].as_u64(), Some(0));
    assert_eq!(dry_run["writes"]["groundingRecords"].as_u64(), Some(0));
    assert_eq!(
        fs::read_to_string(&cli_spec_path).expect("real CLI ontology file should remain readable"),
        before,
        "dry-run migration must not rewrite checked-in ontology"
    );
}

#[test]
fn read_ontology_resolves_inline_bindings_and_transitive_refs() {
    let dir = tempfile::tempdir().unwrap();
    let root = r#"---
component_id: software_architecture/orders
---

# Orders

[D1. Orders flow]
Use @ref data_model/company#D2 because orders persist a company pointer.
**Binding:** maps company_id onto orders.company_id.

## References
- @ref data_model/company #D1 #D2
"#;
    let concept = r#"---
component_id: data_model/company
---

# Company

[D1. Company identity]
Company is the tenant boundary.

[D2. Company persistence]
Company rows include id, name, and tax identifier, and require @ref security_policy/tenant#D1.
"#;
    let security = r#"---
component_id: security_policy/tenant
---

# Tenant

[D1. Tenant guard]
Every row is scoped by company id.
"#;
    write_spec(dir.path(), "software_architecture/orders", root);
    write_spec(dir.path(), "data_model/company", concept);
    write_spec(dir.path(), "security_policy/tenant", security);

    let bundle = g5d_json(
        dir.path(),
        &[
            "spec",
            "read-ontology",
            "software_architecture/orders",
            "--mode",
            "both",
            "--include-transitive",
            "--start-block-id",
            "D1",
        ],
    );

    let resolved = bundle["resolvedBlocks"].as_array().unwrap();
    assert_eq!(resolved.len(), 2, "{bundle:#}");
    assert_eq!(
        resolved[0]["targetComponent"].as_str(),
        Some("data_model/company")
    );
    assert_eq!(resolved[0]["targetBlock"].as_str(), Some("D2"));
    assert!(resolved[0]["bindingOverlays"][0]
        .as_str()
        .unwrap()
        .contains("orders.company_id"));
    assert_eq!(
        resolved[1]["targetComponent"].as_str(),
        Some("security_policy/tenant")
    );
    assert_eq!(resolved[1]["transitiveDepth"].as_u64(), Some(1));
    assert!(bundle["resolvedMarkdown"]
        .as_str()
        .unwrap()
        .contains("g5-resolved-ontology-block"));
}

#[test]
fn read_ontology_ignores_derived_refs_and_omits_over_budget_blocks() {
    let dir = tempfile::tempdir().unwrap();
    let root = r#"---
component_id: software_architecture/orders
---

# Orders

[D1. Orders flow]
This block has no authoritative inline reference.

## References
- @ref data_model/company #D2
"#;
    let concept = r#"---
component_id: data_model/company
---

# Company

[D2. Company persistence]
Company rows include id, name, and tax identifier.
"#;
    write_spec(dir.path(), "software_architecture/orders", root);
    write_spec(dir.path(), "data_model/company", concept);

    let derived_only = g5d_json(
        dir.path(),
        &[
            "spec",
            "read-ontology",
            "software_architecture/orders",
            "--mode",
            "resolved",
        ],
    );
    assert_eq!(
        derived_only["resolvedBlocks"].as_array().unwrap().len(),
        0,
        "{derived_only:#}"
    );

    let root_with_inline_ref = root.replace(
        "This block has no authoritative inline reference.",
        "Use @ref data_model/company#D2 because orders persist company ids.",
    );
    write_spec(
        dir.path(),
        "software_architecture/orders",
        &root_with_inline_ref,
    );

    let over_budget = g5d_json(
        dir.path(),
        &[
            "spec",
            "read-ontology",
            "software_architecture/orders",
            "--mode",
            "resolved",
            "--token-budget",
            "1",
        ],
    );
    assert_eq!(over_budget["resolvedBlocks"].as_array().unwrap().len(), 0);
    let omitted = over_budget["omittedBlocks"].as_array().unwrap();
    assert_eq!(omitted.len(), 1, "{over_budget:#}");
    assert_eq!(omitted[0]["reason"].as_str(), Some("token_budget_exceeded"));
    assert!(over_budget["resolvedMarkdown"]
        .as_str()
        .unwrap()
        .contains("g5-resolved-ontology-omissions"));
}

#[test]
fn read_ontology_deduplicates_refs_and_reports_cycle_cuts_without_writing_files() {
    let dir = tempfile::tempdir().unwrap();
    let root = r#"---
component_id: software_architecture/orders
---

# Orders

[D1. First use]
Use @ref data_model/company#D2 because the order row stores company_id.

[D2. Second use]
Use @ref data_model/company#D2 because audit rows also store company_id.
"#;
    let concept = r#"---
component_id: data_model/company
---

# Company

[D2. Company persistence]
Company rows require @ref security_policy/tenant#D1.
"#;
    let security = r#"---
component_id: security_policy/tenant
---

# Tenant

[D1. Tenant guard]
Tenant checks use @ref data_model/company#D2.
"#;
    write_spec(dir.path(), "software_architecture/orders", root);
    write_spec(dir.path(), "data_model/company", concept);
    write_spec(dir.path(), "security_policy/tenant", security);

    let bundle = g5d_json(
        dir.path(),
        &[
            "spec",
            "read-ontology",
            "software_architecture/orders",
            "--mode",
            "both",
            "--include-transitive",
        ],
    );

    let resolved = bundle["resolvedBlocks"].as_array().unwrap();
    assert_eq!(resolved.len(), 2, "{bundle:#}");
    let company = resolved
        .iter()
        .find(|block| block["targetComponent"].as_str() == Some("data_model/company"))
        .expect("company block should be resolved");
    assert_eq!(company["targetBlock"].as_str(), Some("D2"));
    assert_eq!(company["neededBy"].as_array().unwrap().len(), 3);
    assert_eq!(company["cycleCut"].as_bool(), Some(true));
    assert!(bundle["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|diagnostic| diagnostic["code"].as_str() == Some("cycle_cut")));

    let root_path = dir
        .path()
        .join(".semlift/specs/software_architecture/orders.semlift.md");
    assert_eq!(
        fs::read_to_string(root_path).unwrap(),
        root,
        "read-ontology must not persist resolved projections back to source"
    );
}
