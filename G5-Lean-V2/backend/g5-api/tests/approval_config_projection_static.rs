//! Static architecture guards for the workspace-approvals projection.
//!
//! Scenario ownership: `test_contract/workspace_approvals_management` D16.

const APPROVAL_CONFIG_SERVICE: &str =
    include_str!("../../g5-api-core/src/services/workspace/approval_config_service.rs");

#[test]
fn outgoing_history_metadata_resolution_is_set_based() {
    let history_start = APPROVAL_CONFIG_SERVICE
        .find("// History is request/status complete")
        .expect("history projection marker");
    let requirements_start = APPROVAL_CONFIG_SERVICE[history_start..]
        .find("let mut requirements = Vec::new();")
        .map(|offset| history_start + offset)
        .expect("requirements projection marker");
    let history_projection = &APPROVAL_CONFIG_SERVICE[history_start..requirements_start];

    assert_eq!(
        history_projection
            .matches("FROM approval_definitions")
            .count(),
        1,
        "history definition metadata must be resolved by one SQL statement"
    );
    assert!(
        history_projection.contains("id = ANY($2)"),
        "history definition metadata must bind the complete id set"
    );

    let assembly_loop = history_projection
        .find("for definition_id in history_definition_ids")
        .map(|offset| &history_projection[offset..])
        .expect("history assembly loop");
    assert!(
        !assembly_loop.contains("sqlx::"),
        "per-definition history assembly must not execute SQL"
    );
}
