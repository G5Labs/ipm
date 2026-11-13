// ---------------------------------------------------------------------------
// g5-common/src/auth/minter.rs — JWT minting for worker session tokens
//
// Provides [`JwtMinter`] for signing short-lived JWT tokens. Used by g5-api
// to mint worker session tokens during registration and heartbeat refresh.
//
// Implements §4.2 Change 2 of SPEC_CHANGE_PROPOSAL.md.
// ---------------------------------------------------------------------------

use jsonwebtoken::{Algorithm, EncodingKey, Header};
use uuid::Uuid;

use super::jwt::{
    WorkspaceIncarnationClaims, WORKSPACE_INCARNATION_AUDIENCE, WORKSPACE_INCARNATION_TOKEN_USE,
};
use super::{JwtClaims, Scope};
use crate::error::G5Error;

// ---------------------------------------------------------------------------
// JwtMinter
// ---------------------------------------------------------------------------

/// Signs JWTs using ES256 (ECDSA P-256).
///
/// Holds the pre-parsed encoding (private) key. Constructed once at startup
/// in `g5-api` and shared via `Arc<JwtMinter>`.
pub struct JwtMinter {
    encoding_key: EncodingKey,
    /// Issuer value stamped on all minted tokens.
    issuer: String,
    /// Audience value stamped on all minted tokens.
    audience: String,
}

impl std::fmt::Debug for JwtMinter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JwtMinter")
            .field("issuer", &self.issuer)
            .field("audience", &self.audience)
            .finish_non_exhaustive()
    }
}

impl JwtMinter {
    /// Create a new `JwtMinter` from a PEM-encoded ES256 private key.
    ///
    /// # Errors
    ///
    /// Returns `G5Error::Internal` if the PEM cannot be parsed as a
    /// valid ECDSA P-256 private key.
    pub fn new(private_key_pem: &str, issuer: &str, audience: &str) -> Result<Self, G5Error> {
        let encoding_key = EncodingKey::from_ec_pem(private_key_pem.as_bytes()).map_err(|e| {
            G5Error::Internal {
                message: format!("Failed to parse ES256 private key PEM: {e}"),
            }
        })?;

        Ok(Self {
            encoding_key,
            issuer: issuer.to_string(),
            audience: audience.to_string(),
        })
    }

    /// Mint a typed token for one current workspace VM incarnation.
    #[allow(clippy::too_many_arguments)]
    pub fn mint_workspace_incarnation(
        &self,
        org_id: Uuid,
        workspace_id: Uuid,
        owner_user_id: Uuid,
        vm_id: Uuid,
        epoch: u64,
        ttl_secs: i64,
    ) -> Result<String, G5Error> {
        if epoch == 0 || ttl_secs <= 0 {
            return Err(G5Error::Internal {
                message: "workspace incarnation epoch and lifetime must be positive".to_string(),
            });
        }
        let now = chrono::Utc::now().timestamp();
        let claims = WorkspaceIncarnationClaims {
            org_id,
            workspace_id,
            owner_user_id,
            vm_id,
            epoch,
            iat: now,
            exp: now.saturating_add(ttl_secs),
            iss: Some(self.issuer.clone()),
            aud: WORKSPACE_INCARNATION_AUDIENCE.to_string(),
            jti: Uuid::new_v4().to_string(),
            token_use: WORKSPACE_INCARNATION_TOKEN_USE.to_string(),
        };
        let header = Header::new(Algorithm::ES256);
        jsonwebtoken::encode(&header, &claims, &self.encoding_key).map_err(|error| {
            G5Error::Internal {
                message: format!("Failed to encode workspace incarnation JWT: {error}"),
            }
        })
    }

    /// Mint a workspace agent's credential for one preview task
    /// (`g5.preview.agent-access`). The caller must first admit `caller` as a
    /// current incarnation and the target under agent scope, and resolve
    /// `task_id` as the target's current ready preview.
    pub fn mint_preview_agent(
        &self,
        caller: &WorkspaceIncarnationClaims,
        target_workspace_id: Uuid,
        task_id: Uuid,
    ) -> Result<String, G5Error> {
        use super::jwt::{
            preview_agent_audience, PreviewAgentClaims, PREVIEW_AGENT_TOKEN_USE,
            PREVIEW_AGENT_TTL_SECS,
        };
        if caller.epoch == 0 || target_workspace_id.is_nil() || task_id.is_nil() {
            return Err(G5Error::Internal {
                message: "preview agent credential needs a caller epoch, target and task"
                    .to_string(),
            });
        }
        let now = chrono::Utc::now().timestamp();
        let claims = PreviewAgentClaims {
            org_id: caller.org_id,
            owner_user_id: caller.owner_user_id,
            workspace_id: caller.workspace_id,
            vm_id: caller.vm_id,
            epoch: caller.epoch,
            target_workspace_id,
            task_id,
            iat: now,
            exp: now + PREVIEW_AGENT_TTL_SECS,
            iss: Some(self.issuer.clone()),
            aud: preview_agent_audience(task_id),
            jti: Uuid::new_v4().to_string(),
            token_use: PREVIEW_AGENT_TOKEN_USE.to_string(),
        };
        jsonwebtoken::encode(&Header::new(Algorithm::ES256), &claims, &self.encoding_key).map_err(
            |error| G5Error::Internal {
                message: format!("Failed to encode preview agent credential: {error}"),
            },
        )
    }

    /// The caller must first authorize the source workspace and bind the current
    /// vector to this live host. No provider credential is included in the grant.
    pub fn mint_host_source(
        &self,
        org_id: Uuid,
        host_id: Uuid,
        source_workspace_id: Uuid,
        actor_user_id: Uuid,
        repo_ids: Vec<Uuid>,
        membership_digest: String,
    ) -> Result<String, G5Error> {
        use super::{
            HostSourceClaims, HOST_SOURCE_AUDIENCE, HOST_SOURCE_TOKEN_USE, HOST_SOURCE_TTL_SECS,
        };
        if [org_id, host_id, source_workspace_id, actor_user_id]
            .iter()
            .any(Uuid::is_nil)
            || repo_ids.is_empty()
            || repo_ids.len() > 64
            || repo_ids.iter().any(Uuid::is_nil)
            || repo_ids
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != repo_ids.len()
            || membership_digest.len() != 64
            || !membership_digest.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(G5Error::Config("invalid host source grant scope".into()));
        }
        let now = chrono::Utc::now().timestamp();
        let claims = HostSourceClaims {
            org_id,
            host_id,
            source_workspace_id,
            actor_user_id,
            repo_ids,
            membership_digest,
            iat: now,
            exp: now + HOST_SOURCE_TTL_SECS,
            iss: Some(self.issuer.clone()),
            aud: HOST_SOURCE_AUDIENCE.into(),
            jti: Uuid::new_v4().to_string(),
            token_use: HOST_SOURCE_TOKEN_USE.into(),
        };
        jsonwebtoken::encode(&Header::new(Algorithm::ES256), &claims, &self.encoding_key).map_err(
            |e| G5Error::Internal {
                message: format!("encode host source grant: {e}"),
            },
        )
    }

    /// Mint a per-host bearer token (`host_jwt`) for a worker host.
    ///
    /// Per the rev-7 architecture (`worker_hosts.semlift.md` [D3]), each bare
    /// cloud VM host authenticates to the control plane with a JWT that names a
    /// row in `worker_hosts`. The claim shape reuses [`JwtClaims`]:
    ///
    /// - `sub` = `host_id` (UUID as string)
    /// - `org` = `org_id` (UUID as string)
    /// - `scopes` = `[Scope::WorkerHost]` — the single-element scope vec is the
    ///   discriminant that distinguishes host JWTs from user JWTs at the
    ///   auth middleware (see `g5-api-core/src/middleware/auth_middleware.rs`).
    /// - `iat`/`exp` = standard issued-at / expires-at Unix timestamps.
    /// - `token_use` = `None` (NOT a capability token).
    ///
    /// This is the production minting surface for the PR-rev-7-9b provisioning
    /// endpoint, which mints through the `JwtMinter` already wired into
    /// `AppState.jwt_minter`.
    ///
    /// # Errors
    ///
    /// - `G5Error::Internal` with message `"host_jwt lifetime below minimum"`
    ///   when `lifetime` is shorter than [`MIN_HOST_JWT_LIFETIME_SECS`](Self::MIN_HOST_JWT_LIFETIME_SECS).
    ///   A lifetime floor defends against replay-via-clock-skew with absurdly
    ///   short tokens (see `worker_hosts.semlift.md` [I4]).
    /// - `G5Error::Internal` if JWT encoding fails.
    pub fn mint_host_jwt(
        &self,
        host_id: Uuid,
        org_id: Uuid,
        lifetime: std::time::Duration,
    ) -> Result<String, G5Error> {
        // Saturate to i64::MAX rather than reject: a Duration above i64::MAX
        // seconds (~292B years) is upstream programmer error, not a token-issuance
        // failure. The 60s floor below still applies, so the floor check below
        // is the only legitimate rejection path.
        let lifetime_secs = i64::try_from(lifetime.as_secs()).unwrap_or(i64::MAX);
        if lifetime_secs < Self::MIN_HOST_JWT_LIFETIME_SECS {
            return Err(G5Error::Internal {
                message: "host_jwt lifetime below minimum".into(),
            });
        }

        let now = chrono::Utc::now().timestamp();

        let claims = JwtClaims {
            sub: host_id.to_string(),
            org: org_id.to_string(),
            scopes: vec![Scope::WorkerHost],
            iat: now,
            exp: now + lifetime_secs,
            iss: Some(self.issuer.clone()),
            aud: Some(self.audience.clone()),
            jti: Some(Uuid::new_v4().to_string()),
            token_use: None,
            client: None,
        };

        let header = Header::new(Algorithm::ES256);
        jsonwebtoken::encode(&header, &claims, &self.encoding_key).map_err(|e| G5Error::Internal {
            message: format!("Failed to encode host JWT: {e}"),
        })
    }

    /// Minimum accepted `host_jwt` lifetime, in seconds.
    ///
    /// Mirrors the floor enforced by the g5-api `JwtMinter` and documented in
    /// `worker_hosts.semlift.md` [I4]. 60s is well above realistic NTP skew and
    /// well below any legitimate host-token lifetime (V1 mints hours-long
    /// tokens and rotates before expiry).
    pub const MIN_HOST_JWT_LIFETIME_SECS: i64 = 60;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::super::Scope;
    use super::*;
    use jsonwebtoken::{Algorithm, DecodingKey, Validation};

    // Test ES256 key pair (NIST P-256 / prime256v1, PKCS#8 format).
    // Same key pair as jwt.rs tests — NOT for production use.
    const TEST_PRIVATE_KEY_PEM: &str = "\
-----BEGIN PRIVATE KEY-----
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQg0J63p4NbSY225Nco
6sEKek+VGL+z0Bt+/o3rkfIBFAihRANCAAQV+5QLRzUNaBAZyRaVIOWFtfZpe2ly
7GviW9bv0coSK0uGOFk/8N5UiSrpwDjuEN7DIIdAPwvDY26VHa9qhsW+
-----END PRIVATE KEY-----";

    const TEST_PUBLIC_KEY_PEM: &str = "\
-----BEGIN PUBLIC KEY-----
MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAEFfuUC0c1DWgQGckWlSDlhbX2aXtp
cuxr4lvW79HKEitLhjhZP/DeVIkq6cA47hDewyCHQD8Lw2NulR2vaobFvg==
-----END PUBLIC KEY-----";

    fn make_minter() -> JwtMinter {
        JwtMinter::new(TEST_PRIVATE_KEY_PEM, "g5-api", "g5").unwrap()
    }

    #[test]
    fn invalid_pem_returns_error() {
        let result = JwtMinter::new("not a pem", "g5-api", "g5");
        assert!(result.is_err());
    }

    #[test]
    fn mint_host_jwt_round_trips_with_worker_host_scope() {
        let minter = make_minter();
        let host_id = Uuid::new_v4();
        let org_id = Uuid::new_v4();
        let token = minter
            .mint_host_jwt(host_id, org_id, std::time::Duration::from_secs(3600))
            .expect("mint_host_jwt should succeed for a 1h lifetime");

        let decoding_key = DecodingKey::from_ec_pem(TEST_PUBLIC_KEY_PEM.as_bytes()).unwrap();
        let mut validation = Validation::new(Algorithm::ES256);
        validation.set_issuer(&["g5-api"]);
        validation.set_audience(&["g5"]);
        validation.validate_exp = true;
        validation.leeway = 5;
        let data = jsonwebtoken::decode::<JwtClaims>(&token, &decoding_key, &validation).unwrap();

        assert_eq!(data.claims.sub, host_id.to_string());
        assert_eq!(data.claims.org, org_id.to_string());
        assert_eq!(data.claims.scopes, vec![Scope::WorkerHost]);
        assert_eq!(data.claims.exp - data.claims.iat, 3600);
        assert!(data.claims.jti.is_some());
        assert!(
            data.claims.token_use.is_none(),
            "host_jwt must not set token_use"
        );
    }

    #[test]
    fn workspace_incarnation_token_is_typed_and_bound() {
        let minter = make_minter();
        let org_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        let owner_user_id = Uuid::new_v4();
        let vm_id = Uuid::new_v4();
        let token = minter
            .mint_workspace_incarnation(org_id, workspace_id, owner_user_id, vm_id, 7, 86_400)
            .expect("mint workspace incarnation");
        let validator = crate::JwtValidator::new(TEST_PUBLIC_KEY_PEM, Some("g5-api"), Some("g5"))
            .expect("test validator");

        let claims = validator
            .validate_workspace_incarnation(&token)
            .expect("validate workspace incarnation");
        assert_eq!(claims.org_id, org_id);
        assert_eq!(claims.workspace_id, workspace_id);
        assert_eq!(claims.owner_user_id, owner_user_id);
        assert_eq!(claims.vm_id, vm_id);
        assert_eq!(claims.epoch, 7);
        assert_eq!(claims.exp - claims.iat, 86_400);
        assert!(validator.validate(&token).is_err());
        assert!(validator.is_platform_credential(&token));
    }

    #[test]
    fn preview_agent_credential_is_task_bound_and_disjoint_from_other_credentials() {
        let minter = make_minter();
        let validator = crate::JwtValidator::new(TEST_PUBLIC_KEY_PEM, Some("g5-api"), Some("g5"))
            .expect("test validator");
        let (org_id, parent, owner, vm_id) = (
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
        );
        let incarnation = minter
            .mint_workspace_incarnation(org_id, parent, owner, vm_id, 3, 3600)
            .unwrap();
        let caller = validator
            .validate_workspace_incarnation(&incarnation)
            .unwrap();
        let (child, task) = (Uuid::new_v4(), Uuid::new_v4());
        let token = minter.mint_preview_agent(&caller, child, task).unwrap();

        let claims = validator.validate_preview_agent(&token, task).unwrap();
        assert_eq!(
            (claims.org_id, claims.owner_user_id, claims.workspace_id),
            (org_id, owner, parent)
        );
        assert_eq!((claims.vm_id, claims.epoch), (vm_id, 3));
        assert_eq!((claims.target_workspace_id, claims.task_id), (child, task));
        assert_eq!(claims.exp - claims.iat, crate::PREVIEW_AGENT_TTL_SECS);
        let derived = claims.caller_incarnation();
        assert_eq!(
            (derived.workspace_id, derived.vm_id, derived.epoch),
            (parent, vm_id, 3)
        );

        // Bound to its task, and no other validator accepts it.
        assert!(validator
            .validate_preview_agent(&token, Uuid::new_v4())
            .is_err());
        assert!(validator.validate(&token).is_err());
        assert!(validator.validate_preview_ticket(&token, task).is_err());
        assert!(validator
            .validate_preview_handoff_ticket(&token, task)
            .is_err());
        assert!(validator.validate_workspace_incarnation(&token).is_err());
        // Never forwarded to a previewed application as an app bearer.
        assert!(validator.is_platform_credential(&token));
        // A VM credential is not a preview agent credential.
        assert!(validator
            .validate_preview_agent(&incarnation, task)
            .is_err());
        assert!(minter
            .mint_preview_agent(&caller, child, Uuid::nil())
            .is_err());
    }

    #[test]
    fn host_source_grant_is_bounded_and_separate_from_other_credentials() {
        let minter = make_minter();
        let validator =
            crate::JwtValidator::new(TEST_PUBLIC_KEY_PEM, Some("g5-api"), Some("g5")).unwrap();
        let host = Uuid::new_v4();
        let org = Uuid::new_v4();
        let repo = Uuid::new_v4();
        let token = minter
            .mint_host_source(
                org,
                host,
                Uuid::new_v4(),
                Uuid::new_v4(),
                vec![repo],
                "a".repeat(64),
            )
            .unwrap();
        let grant = validator.validate_host_source(&token).unwrap();
        assert_eq!(grant.host_id, host);
        assert_eq!(grant.repo_ids, vec![repo]);
        assert_eq!(grant.exp - grant.iat, crate::HOST_SOURCE_TTL_SECS);
        assert!(validator.validate(&token).is_err());
        assert!(validator.validate_workspace_incarnation(&token).is_err());
        assert!(validator
            .validate_preview_ticket(&token, Uuid::new_v4())
            .is_err());
        assert!(validator.is_platform_credential(&token));
        let host_token = minter
            .mint_host_jwt(host, org, std::time::Duration::from_secs(3600))
            .unwrap();
        assert!(validator.validate_host_source(&host_token).is_err());
        assert!(minter
            .mint_host_source(
                org,
                host,
                Uuid::new_v4(),
                Uuid::new_v4(),
                vec![repo, repo],
                "a".repeat(64)
            )
            .is_err());
    }

    #[test]
    fn mint_host_jwt_rejects_lifetime_below_minimum() {
        let minter = make_minter();
        let too_short = std::time::Duration::from_secs(
            (JwtMinter::MIN_HOST_JWT_LIFETIME_SECS as u64).saturating_sub(1),
        );
        let result = minter.mint_host_jwt(Uuid::new_v4(), Uuid::new_v4(), too_short);
        match result {
            Err(G5Error::Internal { message }) => {
                assert_eq!(message, "host_jwt lifetime below minimum");
            }
            other => panic!("expected below-minimum rejection, got {other:?}"),
        }
    }

    #[test]
    fn mint_host_jwt_accepts_lifetime_at_minimum() {
        let minter = make_minter();
        let exactly_min =
            std::time::Duration::from_secs(JwtMinter::MIN_HOST_JWT_LIFETIME_SECS as u64);
        let token = minter
            .mint_host_jwt(Uuid::new_v4(), Uuid::new_v4(), exactly_min)
            .expect("lifetime == MIN must be accepted");

        let decoding_key = DecodingKey::from_ec_pem(TEST_PUBLIC_KEY_PEM.as_bytes()).unwrap();
        let mut validation = Validation::new(Algorithm::ES256);
        validation.set_audience(&["g5"]);
        validation.validate_exp = true;
        validation.leeway = 5;
        let data = jsonwebtoken::decode::<JwtClaims>(&token, &decoding_key, &validation).unwrap();
        assert_eq!(
            data.claims.exp - data.claims.iat,
            JwtMinter::MIN_HOST_JWT_LIFETIME_SECS
        );
    }
}
