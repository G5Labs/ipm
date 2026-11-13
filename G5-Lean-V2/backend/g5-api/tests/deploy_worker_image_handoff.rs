//! QA deploy validates a provided worker image and joins authentication and
//! image readiness before handing the concrete provisioning pin to Helm.

const DEPLOY_PIPELINE: &str = include_str!("../../../infra/ci/cloudbuild-deploy.yaml");
const WORKER_IMAGE_RESOLVER: &str = include_str!("../../../scripts/resolve_worker_image.py");

#[test]
fn deploy_only_worker_image_handoff_is_validated_and_passed_to_helm() {
    for required in [
        "_WORKER_IMAGE_REF: \"\"",
        "--provided-ref \"${_WORKER_IMAGE_REF}\"",
        "source /workspace/worker-image.env",
        "api.env.G5_WORKER_IMAGE=\"$${WORKER_IMAGE_REF}\"",
        "waitFor: ['gke-auth', 'resolve-worker-image']",
    ] {
        assert!(
            DEPLOY_PIPELINE.contains(required),
            "deploy-only worker-image handoff must retain `{required}`"
        );
    }
    for required in [
        "--provided-ref",
        "validated promoted worker image",
        "status",
        "family",
    ] {
        assert!(
            WORKER_IMAGE_RESOLVER.contains(required),
            "worker-image validation must retain `{required}`"
        );
    }
}

#[test]
fn helm_deploy_waits_for_authentication_and_worker_image_validation() {
    let resolver = DEPLOY_PIPELINE
        .find("- id: resolve-worker-image")
        .expect("deploy pipeline must resolve or validate the worker image");
    let helm = DEPLOY_PIPELINE
        .find("- id: helm-deploy")
        .expect("deploy pipeline must contain the Helm step");
    assert!(resolver < helm, "worker-image validation must precede Helm");
    let auth = DEPLOY_PIPELINE
        .find("- id: gke-auth")
        .expect("deploy pipeline must authenticate to the cluster");
    assert!(auth < helm, "authentication must precede Helm");
    let helm_wait = DEPLOY_PIPELINE[helm..]
        .lines()
        .find_map(|line| line.trim().strip_prefix("waitFor: "))
        .expect("the Helm step must declare waitFor");
    assert!(
        helm_wait.contains("'resolve-worker-image'"),
        "Helm must wait for worker-image validation, got waitFor: {helm_wait}"
    );
    assert!(
        helm_wait.contains("'gke-auth'"),
        "Helm must wait for authentication, got waitFor: {helm_wait}"
    );
}
