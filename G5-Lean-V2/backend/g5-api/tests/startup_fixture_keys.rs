//! Run the deployment binary: fixture rejection must precede any DB access.

use std::process::{Command, Output};

const FIXTURE_PRIVATE: &str = include_str!("../../../scripts/ci/e2e-ci-jwt-private.pem");
const FIXTURE_PUBLIC: &str = include_str!("../../../scripts/ci/e2e-ci-jwt-public.pem");
const FIXTURE_SCM: &str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";

fn boot(role: &str, e2e: &str, credentials: &[(&str, &str)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_g5-api"));
    for (name, _) in std::env::vars().filter(|(name, _)| name.starts_with("G5_")) {
        command.env_remove(name);
    }
    command
        .env("G5_ROLE", role)
        .env("G5_E2E_AUTH", e2e)
        .env("G5_DATABASE_URL", "invalid-url-for-startup-boundary")
        .env("G5_EXTERNAL_URL", "http://localhost:3000")
        .env("SELF_HOSTED", "false")
        .envs(credentials.iter().copied())
        .output()
        .expect("run the actual API/daemon startup binary")
}

#[test]
fn public_fixtures_fail_before_database_access_in_both_roles_and_e2e_modes() {
    for role in ["api", "daemon"] {
        for e2e in ["true", "false"] {
            for (field, value) in [
                ("G5_JWT_SECRET", FIXTURE_PRIVATE),
                ("G5_JWT_PRIVATE_KEY", FIXTURE_PRIVATE),
                ("G5_JWT_PUBLIC_KEY", FIXTURE_PUBLIC),
                ("G5_SCM_ENCRYPTION_KEY", FIXTURE_SCM),
            ] {
                let output = boot(role, e2e, &[(field, value)]);
                let stderr = String::from_utf8_lossy(&output.stderr);
                assert!(!output.status.success());
                assert!(
                    stderr.contains(field) && stderr.contains("fixture"),
                    "{stderr}"
                );
                assert!(!stderr.contains("connecting to database"), "{stderr}");
                assert!(!stderr.contains(value), "startup leaked {field}");
            }
        }
    }
}

#[tokio::test]
async fn generated_job_credentials_survive_reload_and_pass_real_startup_validation() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("bundle.json");
    let script = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../scripts/stack_secrets.py"
    );
    let load = || {
        let output = Command::new("python3")
            .arg(script)
            .arg("job")
            .arg(&path)
            .env_remove("G5_CI_STACK_SECRETS_DIGEST")
            .output()
            .expect("run actual job secret bootstrap");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        std::fs::read(&path).unwrap()
    };
    let first = load();
    let bundle: serde_json::Value = serde_json::from_slice(&first).unwrap();
    let private = bundle["G5_JWT_PRIVATE_KEY"].as_str().unwrap();
    let public = bundle["G5_JWT_PUBLIC_KEY"].as_str().unwrap();
    let scm = bundle["G5_SCM_ENCRYPTION_KEY"].as_str().unwrap();
    let manager = |key: &str| {
        g5_api_core::auth::ScmTokenManager::new(
            hex::decode(key).unwrap().try_into().unwrap(),
            g5_scm::providers::ProviderClients::new(),
        )
    };
    let ciphertext = manager(scm).encrypt_token("retained-scm-token").unwrap();
    let restarted = load();
    assert_eq!(restarted, first, "restart must retain the complete bundle");
    let reloaded: serde_json::Value = serde_json::from_slice(&restarted).unwrap();
    assert_eq!(
        manager(reloaded["G5_SCM_ENCRYPTION_KEY"].as_str().unwrap())
            .decrypt_token(&ciphertext)
            .unwrap(),
        "retained-scm-token",
        "a fresh process's key must still decrypt previously persisted SCM ciphertext"
    );
    let credentials = [
        ("G5_JWT_SECRET", private),
        ("G5_JWT_PRIVATE_KEY", private),
        ("G5_JWT_PUBLIC_KEY", public),
        ("G5_SCM_ENCRYPTION_KEY", scm),
    ];
    for role in ["api", "daemon"] {
        let output = boot(role, "true", &credentials);
        let stderr = String::from_utf8_lossy(&output.stderr);
        // The deliberately invalid URL fails only after startup validation.
        assert!(!output.status.success());
        assert!(stderr.contains("connecting to database"), "{stderr}");
        for (_, value) in credentials {
            assert!(!stderr.contains(value), "startup leaked secret material");
        }
    }
}
