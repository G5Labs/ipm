//! Reject public test identities at the deployment startup boundary.

use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};

const FIXTURE_PUBLIC_KEY: &str = "-----BEGIN PUBLIC KEY-----
MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAEFfuUC0c1DWgQGckWlSDlhbX2aXtp
cuxr4lvW79HKEitLhjhZP/DeVIkq6cA47hDewyCHQD8Lw2NulR2vaobFvg==
-----END PUBLIC KEY-----";

// A public signature over a probe, not a user/host token. It lets a verifier-only
// configuration be checked without embedding the fixture's private key here.
const FIXTURE_PROBE: &str = "eyJhbGciOiJFUzI1NiIsInR5cCI6IkpXVCJ9.eyJnNV9maXh0dXJlX3Byb2JlIjp0cnVlfQ.69r-8GBjEtcYXxhS4KRF8TOLHJJ_tucs2AopnuV-cQHSY1FfqwA0cIQD2mpUmfCnmEEiqJw5Zmiuccafzpup0A";

fn verifies_probe(token: &str, key: &DecodingKey) -> bool {
    let mut validation = Validation::new(Algorithm::ES256);
    validation.required_spec_claims.clear();
    validation.validate_exp = false;
    validation.validate_aud = false;
    decode::<serde_json::Value>(token, key, &validation).is_ok()
}

/// Validate configured deployment keys before opening the database.
///
/// There is no E2E-login exception: that flag cannot make a public key secret.
/// Isolated test harnesses may construct auth services with fixture keys without
/// calling this deployment-only boundary. Errors never include key material.
pub fn validate_deployment_keys(
    user_private: Option<&str>,
    worker_private: Option<&str>,
    public: Option<&str>,
    scm: Option<&str>,
) -> Result<(), String> {
    let fixture_verifier = DecodingKey::from_ec_pem(FIXTURE_PUBLIC_KEY.as_bytes())
        .expect("the public fixture verifier is valid ES256 PEM");
    for (name, configured) in [
        ("G5_JWT_SECRET", user_private),
        ("G5_JWT_PRIVATE_KEY", worker_private),
    ] {
        if let Some(pem) = configured {
            let signer = EncodingKey::from_ec_pem(pem.as_bytes())
                .map_err(|_| format!("{name} must be a valid ES256 private key"))?;
            let probe = encode(
                &Header::new(Algorithm::ES256),
                &serde_json::json!({"g5_fixture_probe": true}),
                &signer,
            )
            .map_err(|_| format!("{name} must be a valid ES256 private key"))?;
            if verifies_probe(&probe, &fixture_verifier) {
                return Err(format!("{name} uses the public JWT fixture key; provision deployment-owned credentials"));
            }
        }
    }
    if let Some(pem) = public {
        let verifier = DecodingKey::from_ec_pem(pem.as_bytes())
            .map_err(|_| "G5_JWT_PUBLIC_KEY must be a valid ES256 public key".to_owned())?;
        if verifies_probe(FIXTURE_PROBE, &verifier) {
            return Err("G5_JWT_PUBLIC_KEY trusts the public JWT fixture key; provision deployment-owned credentials".into());
        }
    }
    if let Some(key) = scm {
        if key.eq_ignore_ascii_case(
            "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f",
        ) {
            return Err("G5_SCM_ENCRYPTION_KEY uses the public fixture key; existing encrypted data needs deliberate migration or reprovisioning, not automatic key replacement".into());
        }
        if key.len() != 64 || !key.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("G5_SCM_ENCRYPTION_KEY must contain 64 hex characters".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE_PRIVATE: &str = include_str!("../../../scripts/ci/e2e-ci-jwt-private.pem");

    #[test]
    fn rejects_each_fixture_signer_and_verifier_independently() {
        for pem in [
            FIXTURE_PRIVATE.to_owned(),
            FIXTURE_PRIVATE.replace('\n', "\r\n"),
        ] {
            for (user, worker, field) in [
                (Some(pem.as_str()), None, "G5_JWT_SECRET"),
                (None, Some(pem.as_str()), "G5_JWT_PRIVATE_KEY"),
            ] {
                let error = validate_deployment_keys(user, worker, None, None).unwrap_err();
                assert!(error.contains(field) && error.contains("fixture"));
                assert!(!error.contains(&pem));
            }
        }
        let reformatted = FIXTURE_PUBLIC_KEY.replace('\n', "\r\n");
        for pem in [FIXTURE_PUBLIC_KEY, reformatted.as_str()] {
            let error = validate_deployment_keys(None, None, Some(pem), None).unwrap_err();
            assert!(error.contains("G5_JWT_PUBLIC_KEY") && error.contains("fixture"));
        }
    }

    #[test]
    fn rejects_fixture_scm_in_either_hex_case() {
        let key = (0u8..32)
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        for candidate in [key.clone(), key.to_uppercase()] {
            let error = validate_deployment_keys(None, None, None, Some(&candidate)).unwrap_err();
            assert!(error.contains("fixture"));
            assert!(!error.contains(&candidate));
        }
    }

    #[test]
    fn rejects_empty_configured_fields_without_revealing_them() {
        for (user, worker, public, scm) in [
            (Some(""), None, None, None),
            (None, Some(""), None, None),
            (None, None, Some(""), None),
            (None, None, None, Some("")),
        ] {
            assert!(validate_deployment_keys(user, worker, public, scm).is_err());
        }
        assert!(validate_deployment_keys(None, None, None, None).is_ok());
    }
}
