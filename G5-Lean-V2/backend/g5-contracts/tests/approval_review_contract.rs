use g5_contracts::types::*;
use serde_json::json;
use uuid::Uuid;

#[test]
fn v3_request_context_contains_only_stable_request_identity() {
    let context = ApprovalReviewRequestContextV3 {
        schema_version: APPROVAL_REVIEW_REQUEST_CONTEXT_SCHEMA_VERSION_V3,
        approval_request_id: Uuid::new_v4(),
        request_key: "a".repeat(64),
        definition_id: Uuid::new_v4(),
        definition_version: 3,
        phase: Some("ie".to_string()),
        stage: None,
    };
    context.validate().expect("valid request-only context");
    assert_eq!(context.settled_phase(), "ie");

    let value = serde_json::to_value(&context).expect("serialize context");
    assert_eq!(
        value
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>(),
        [
            "approvalRequestId",
            "definitionId",
            "definitionVersion",
            "phase",
            "requestKey",
            "schemaVersion",
        ]
        .into_iter()
        .map(str::to_string)
        .collect()
    );
    for mutable_field in [
        "targetWorkspaceId",
        "sourceHeads",
        "targetHeads",
        "outputDocPath",
        "agentLaunch",
        "callerIdempotencyKeyHash",
    ] {
        assert!(value.get(mutable_field).is_none());
    }

    let mut redirected = value;
    redirected["targetWorkspaceId"] = json!(Uuid::new_v4());
    assert!(serde_json::from_value::<ApprovalReviewRequestContextV3>(redirected).is_err());
}

#[test]
fn human_request_settlement_fingerprint_excludes_mutable_review_cuts() {
    let settlement = ApprovalRequestSettlementV2 {
        schema_version: APPROVAL_REQUEST_SETTLEMENT_SCHEMA_VERSION,
        request_id: Uuid::new_v4(),
        org_id: Uuid::new_v4(),
        source_workspace_id: Uuid::new_v4(),
        approval_definition_id: Uuid::new_v4(),
        approval_definition_version: 2,
        phase: "ie".to_string(),
        actor_slot: format!("human:{}", Uuid::new_v4()),
        outcome: "approved".to_string(),
        comment: Some("reviewed at action time".to_string()),
    };
    let fingerprint = settlement.fingerprint().expect("valid human settlement");
    // The settled phase is part of every fingerprint (g5.workbench.gates).
    let mut other_phase = settlement.clone();
    other_phase.phase = "be".to_string();
    assert_ne!(
        fingerprint,
        other_phase.fingerprint().expect("valid BE settlement")
    );
    assert_eq!(fingerprint.len(), 64);
    assert!(fingerprint
        .chars()
        .all(|character| character.is_ascii_hexdigit() && !character.is_ascii_uppercase()));
    assert_eq!(
        fingerprint,
        settlement.fingerprint().expect("deterministic fingerprint")
    );

    let value = serde_json::to_value(&settlement).expect("settlement JSON");
    for mutable_or_relational_field in [
        "targetWorkspaceId",
        "currentHeads",
        "baseHeads",
        "approvalStatusId",
    ] {
        assert!(value.get(mutable_or_relational_field).is_none());
    }

    let mut conflicting = settlement;
    conflicting.comment = Some("different response".to_string());
    assert_ne!(
        fingerprint,
        conflicting
            .fingerprint()
            .expect("valid conflicting settlement")
    );
}
