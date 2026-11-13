const API_DEPLOYMENT: &str = include_str!("../../../infra/helm/g5/templates/deployment-api.yaml");
const BASE_VALUES: &str = include_str!("../../../infra/helm/g5/values.yaml");
const QA_VALUES: &str = include_str!("../../../infra/helm/g5/values-qa.yaml");

const REMOVED_RECOVERY_WRITE_FLAG: &str = "G5_TASK_WORKSPACE_RECOVERY_STATUS_WRITES_ENABLED";
const STRICT_IDENTITY_ENV: &str = "G5_EXECUTION_LIFECYCLE_STRICT_IDENTITY";

fn api_values(values: &str) -> &str {
    values
        .split_once("\napi:\n")
        .map(|(_, rest)| rest)
        .expect("values file must define the api stanza")
        .split_once("\n# ===")
        .map(|(api, _)| api)
        .unwrap_or(values)
}

#[test]
fn hosted_recovery_states_use_singleton_recreate_without_a_write_flag() {
    assert!(
        API_DEPLOYMENT.contains("  strategy:\n    type: Recreate")
            && !API_DEPLOYMENT.contains("type: RollingUpdate"),
        "durable recovery and review schemas require non-overlapping API revisions for every replica count"
    );

    for (environment, values) in [("base", BASE_VALUES), ("qa", QA_VALUES)] {
        assert!(
            api_values(values).contains("  replicaCount: 1"),
            "{environment} must keep the audited singleton API topology"
        );
        assert!(
            !values.contains(REMOVED_RECOVERY_WRITE_FLAG),
            "{environment} must not retain the removed recovery-status write flag"
        );
    }
}

#[test]
fn deployment_configs_do_not_gate_execution_lifecycle_identity() {
    for (environment, values) in [("base", BASE_VALUES), ("qa", QA_VALUES)] {
        assert!(
            !values.contains(STRICT_IDENTITY_ENV),
            "{environment} must leave {STRICT_IDENTITY_ENV} unset"
        );
    }
}
