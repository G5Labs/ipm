//! JWT validation logic for G5 authentication.
//!
//! Provides [`JwtClaims`] (parsed JWT payload), [`JwtValidator`] (ES256
//! token validation), and the [`has_scopes`] utility function.
//!
//! This module validates tokens but does NOT mint them. Token minting is
//! exclusively `g5-api`'s responsibility.

use super::Scope;
use crate::error::G5Error;
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// JwtClaims — parsed JWT payload
// ---------------------------------------------------------------------------

/// Represents the parsed JWT payload.
///
/// Matches the JWT payload structure defined in `API_AND_STORAGE.md` §4A.
/// Field names use the exact JWT claim names (no rename needed — JWT
/// payloads use these exact claim names).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtClaims {
    /// User ID (UUID string).
    pub sub: String,
    /// Current org ID (UUID string).
    pub org: String,
    /// Effective OAuth scopes (e.g., `[Scope::WorkspacesRead, Scope::TasksExecute]`).
    pub scopes: Vec<Scope>,
    /// Issued-at Unix timestamp.
    pub iat: i64,
    /// Expires-at Unix timestamp.
    pub exp: i64,
    /// Token issuer (e.g., `"g5-api"`). Validated by [`JwtValidator`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iss: Option<String>,
    /// Intended audience (e.g., `"g5"`). Validated by [`JwtValidator`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aud: Option<String>,
    /// Unique token ID (UUID v4). Used for request tracing and future
    /// replay-detection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jti: Option<String>,
    /// Token use discriminator. User/worker JWTs must NOT have this field.
    /// Capability tokens set `token_use: "capability"`. The JWT validator
    /// rejects any token that carries a `token_use` claim, preventing
    /// capability → auth token confusion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_use: Option<String>,
    /// How the person session behind this access token was issued:
    /// [`SESSION_CLIENT_BROWSER`] or [`SESSION_CLIENT_CLI`]
    /// (g5.security.browser-session-actions). Absent on every other token and
    /// on sessions issued before sessions recorded their kind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client: Option<String>,
}

/// [`JwtClaims::client`] of a session issued by the web sign-in.
pub const SESSION_CLIENT_BROWSER: &str = "browser";
/// [`JwtClaims::client`] of a session issued by CLI device login.
pub const SESSION_CLIENT_CLI: &str = "cli";

/// Signed authority for one current workspace VM incarnation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceIncarnationClaims {
    pub org_id: uuid::Uuid,
    pub workspace_id: uuid::Uuid,
    pub owner_user_id: uuid::Uuid,
    pub vm_id: uuid::Uuid,
    pub epoch: u64,
    pub iat: i64,
    pub exp: i64,
    pub iss: Option<String>,
    pub aud: String,
    pub jti: String,
    pub token_use: String,
}

/// A workspace agent's credential for HTTP requests to one managed preview
/// task (`g5.preview.agent-access`). Its VM credential mints it under agent
/// scope; the preview relay accepts it only from [`PREVIEW_AGENT_HEADER`] on
/// that task's preview origin and rechecks every fact it names on each
/// request. Neither a person session, a browser ticket nor a VM credential.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreviewAgentClaims {
    pub org_id: uuid::Uuid,
    pub owner_user_id: uuid::Uuid,
    /// The minting incarnation: the agent's own workspace, VM and epoch.
    pub workspace_id: uuid::Uuid,
    pub vm_id: uuid::Uuid,
    pub epoch: u64,
    /// The workspace whose preview the credential reaches: the caller's own
    /// or a direct child.
    pub target_workspace_id: uuid::Uuid,
    pub task_id: uuid::Uuid,
    pub iat: i64,
    pub exp: i64,
    pub iss: Option<String>,
    pub aud: String,
    pub jti: String,
    pub token_use: String,
}

impl PreviewAgentClaims {
    /// The minting incarnation in the shape the agent-scope checks take. Only
    /// its identity fields are meaningful; it is never a usable VM credential.
    #[must_use]
    pub fn caller_incarnation(&self) -> WorkspaceIncarnationClaims {
        WorkspaceIncarnationClaims {
            org_id: self.org_id,
            workspace_id: self.workspace_id,
            owner_user_id: self.owner_user_id,
            vm_id: self.vm_id,
            epoch: self.epoch,
            iat: self.iat,
            exp: self.exp,
            iss: self.iss.clone(),
            aud: WORKSPACE_INCARNATION_AUDIENCE.to_string(),
            jti: self.jti.clone(),
            token_use: WORKSPACE_INCARNATION_TOKEN_USE.to_string(),
        }
    }
}

/// `token_use` of [`PreviewAgentClaims`].
pub const PREVIEW_AGENT_TOKEN_USE: &str = "preview_agent";
/// The longest a preview agent credential lives. Every request rechecks its
/// authority, so expiry is a backstop rather than the revocation path.
pub const PREVIEW_AGENT_TTL_SECS: i64 = 600;
/// The only place a preview agent credential is accepted: a request header,
/// never a cookie or URL parameter, so browsers cannot carry it.
pub const PREVIEW_AGENT_HEADER: &str = "x-g5-preview-credential";

/// Bind a preview agent credential to its exact preview task.
#[must_use]
pub fn preview_agent_audience(task_id: uuid::Uuid) -> String {
    format!("preview-agent:{task_id}")
}

/// Short-lived read authority for one host's explicitly selected source vector.
/// This is neither a user token nor a workspace incarnation credential.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostSourceClaims {
    pub org_id: uuid::Uuid,
    pub host_id: uuid::Uuid,
    pub source_workspace_id: uuid::Uuid,
    pub actor_user_id: uuid::Uuid,
    pub repo_ids: Vec<uuid::Uuid>,
    pub membership_digest: String,
    pub iat: i64,
    pub exp: i64,
    pub iss: Option<String>,
    pub aud: String,
    pub jti: String,
    pub token_use: String,
}
pub const HOST_SOURCE_TOKEN_USE: &str = "host_source_read";
pub const HOST_SOURCE_AUDIENCE: &str = "g5-host-source-read";
pub const HOST_SOURCE_TTL_SECS: i64 = 900;

// ---------------------------------------------------------------------------
// JwtValidator — ES256 token validation
// ---------------------------------------------------------------------------

/// `token_use` discriminator carried by preview tickets. Tokens with this
/// claim are rejected by [`JwtValidator::validate`] (its blanket `token_use`
/// rejection) and accepted ONLY by [`JwtValidator::validate_preview_ticket`],
/// so a preview cookie replayed as a Bearer credential is dead by construction.
pub const PREVIEW_TOKEN_USE: &str = "preview";

/// `token_use` discriminator for short-lived browser WebSocket tickets.
/// Access tokens and all other capability tokens are rejected by the
/// WebSocket upgrade boundary.
pub const WEBSOCKET_TOKEN_USE: &str = "websocket";
pub const WEBSOCKET_AUDIENCE: &str = "g5-websocket";

/// `token_use` discriminator carried by capability tokens.
pub const CAPABILITY_TOKEN_USE: &str = "capability";
pub const WORKSPACE_INCARNATION_TOKEN_USE: &str = "workspace_incarnation";
pub const WORKSPACE_INCARNATION_AUDIENCE: &str = "g5-workspace-incarnation";

/// `token_use` discriminator carried by the single-use preview handoff
/// ticket that enters the per-preview subdomain origin as the
/// `__g5_preview_ticket` URL parameter (issue G5Labs/G5#935;
/// routes/preview [D15]). Rejected by [`JwtValidator::validate`] and
/// [`JwtValidator::validate_preview_ticket`] alike, and accepted ONLY by
/// [`JwtValidator::validate_preview_handoff_ticket`] — a URL-leaked handoff
/// ticket can never be replayed as a session cookie or Bearer credential.
pub const PREVIEW_HANDOFF_TOKEN_USE: &str = "preview_handoff";

/// The G5-reserved `token_use` discriminator values. Membership here is the
/// unsigned provenance signal used by [`JwtValidator::is_platform_credential`]:
/// only these platform-minted values count, NOT mere presence of a
/// `token_use` claim — customer identity providers routinely set their own
/// `token_use: "access"` / `"id"` on app-owned tokens, which must pass
/// through the preview relay (routes/preview [D9]).
pub const RESERVED_TOKEN_USE_VALUES: &[&str] = &[
    PREVIEW_TOKEN_USE,
    PREVIEW_HANDOFF_TOKEN_USE,
    PREVIEW_AGENT_TOKEN_USE,
    WEBSOCKET_TOKEN_USE,
    CAPABILITY_TOKEN_USE,
    WORKSPACE_INCARNATION_TOKEN_USE,
    HOST_SOURCE_TOKEN_USE,
];

/// Prefix of every platform API key (`g5k_…`). API keys are opaque bearer
/// credentials resolved against the database, so they never parse as JWTs;
/// the prefix is the provenance signal that keeps them from being forwarded
/// to a previewed application like an app's own bearer would be.
pub const API_KEY_PREFIX: &str = "g5k_";

/// Builds the workspace-bound `aud` claim value for a preview ticket
/// (`preview:<workspaceId>`). Shared by the g5-api minter and
/// [`JwtValidator::validate_preview_ticket`] so the two sides cannot drift.
#[must_use]
pub fn preview_ticket_audience(workspace_id: uuid::Uuid) -> String {
    format!("preview:{workspace_id}")
}

/// Bind a workspace terminal ticket to its exact workspace. The platform
/// event socket uses the general audience because it has no workspace route.
#[must_use]
pub fn websocket_ticket_audience(workspace_id: Option<uuid::Uuid>) -> String {
    workspace_id.map_or_else(
        || WEBSOCKET_AUDIENCE.to_string(),
        |workspace_id| format!("{WEBSOCKET_AUDIENCE}:{workspace_id}"),
    )
}

/// Validates JWT tokens using ES256 (ECDSA P-256) public key cryptography.
///
/// Holds the pre-parsed decoding key and validation configuration so that
/// PEM parsing happens only once at construction time.
pub struct JwtValidator {
    decoding_key: Option<DecodingKey>,
    validation: Validation,
    /// Validation config for preview tickets: identical to `validation`
    /// except the standard `aud` enforcement is disabled — a preview ticket
    /// carries `aud: "preview:<workspaceId>"`, which is checked explicitly against
    /// the request's workspace id instead.
    preview_validation: Validation,
}

impl std::fmt::Debug for JwtValidator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JwtValidator")
            .field("validation", &self.validation)
            .finish_non_exhaustive()
    }
}

impl JwtValidator {
    /// Reject every JWT when a deployment has no configured verifier.
    /// No fallback identity is trusted. Outbound preview classification fails
    /// closed for every bearer credential when no verifying key is available.
    pub fn reject_all() -> Self {
        let validation = Validation::new(Algorithm::ES256);
        Self {
            decoding_key: None,
            preview_validation: validation.clone(),
            validation,
        }
    }

    fn decoding_key(&self) -> Result<&DecodingKey, G5Error> {
        self.decoding_key.as_ref().ok_or(G5Error::Unauthorized)
    }

    /// Creates a new `JwtValidator` from a PEM-encoded ES256 public key
    /// and the expected issuer / audience values.
    ///
    /// When `expected_issuer` or `expected_audience` is `Some`, the
    /// validator will reject tokens whose `iss` / `aud` claim does not
    /// match. When `None`, that claim is not checked (backward-compat /
    /// dev mode).
    ///
    /// Returns `Err(G5Error::Internal)` if the PEM key cannot be parsed
    /// as a valid ECDSA P-256 public key.
    pub fn new(
        public_key_pem: &str,
        expected_issuer: Option<&str>,
        expected_audience: Option<&str>,
    ) -> Result<Self, G5Error> {
        let decoding_key =
            DecodingKey::from_ec_pem(public_key_pem.as_bytes()).map_err(|e| G5Error::Internal {
                message: format!("Failed to parse ES256 public key: {}", e),
            })?;

        let mut validation = Validation::new(Algorithm::ES256);
        validation.leeway = 0; // Zero clock skew tolerance
        validation.validate_exp = true; // Enforce expiration

        // Configure issuer validation
        if let Some(iss) = expected_issuer {
            validation.set_issuer(&[iss]);
            validation.required_spec_claims.insert("iss".to_string());
        }

        // Configure audience validation
        if let Some(aud) = expected_audience {
            validation.set_audience(&[aud]);
            validation.required_spec_claims.insert("aud".to_string());
        } else {
            // jsonwebtoken v9 defaults validate_aud to true.  When no
            // expected audience is provided we must explicitly disable it,
            // otherwise tokens that carry an `aud` claim are rejected.
            validation.validate_aud = false;
        }

        // When neither iss nor aud is validated, clear required_spec_claims
        // so the validator doesn't reject tokens missing those standard fields.
        if expected_issuer.is_none() && expected_audience.is_none() {
            validation.required_spec_claims = Default::default();
        }

        // Preview tickets carry a workspace-bound `aud` ("preview:<workspaceId>") that
        // the standard audience check would reject; disable it here and check
        // the audience explicitly in `validate_preview_ticket`.
        let mut preview_validation = validation.clone();
        preview_validation.validate_aud = false;
        preview_validation.required_spec_claims.remove("aud");

        Ok(Self {
            decoding_key: Some(decoding_key),
            validation,
            preview_validation,
        })
    }

    /// Validates a JWT token string and returns the parsed claims.
    ///
    /// Checks:
    /// 1. Token signature is valid against the configured public key
    /// 2. Token uses the ES256 algorithm
    /// 3. Token is not expired (exp > current system time, zero leeway)
    ///
    /// Returns `Err(G5Error::Unauthorized)` for any validation failure:
    /// - Invalid signature
    /// - Wrong algorithm
    /// - Expired token
    /// - Malformed token (bad base64, missing claims, etc.)
    pub fn validate(&self, token: &str) -> Result<JwtClaims, G5Error> {
        let token_data =
            jsonwebtoken::decode::<JwtClaims>(token, self.decoding_key()?, &self.validation)?;
        // Reject capability tokens presented as user/worker JWTs.
        // Capability tokens carry `token_use: "capability"` and must
        // never be accepted by the JWT auth middleware.  This prevents
        // token-confusion attacks (§3.2).
        if token_data.claims.token_use.is_some() {
            return Err(G5Error::Unauthorized);
        }
        Ok(token_data.claims)
    }

    /// Validates a preview ticket JWT and returns the parsed claims.
    ///
    /// Preview tickets are the short-lived credentials carried in the
    /// workspace-path-scoped `g5_preview` cookie (minted by g5-api for issue
    /// G5Labs/G5#736). In addition to signature / algorithm / expiry (same
    /// key, ES256, zero leeway), this checks:
    /// 1. `token_use == "preview"` — a normal access/ws JWT (`token_use:
    ///    None`) or a capability token is rejected here, mirroring how
    ///    [`validate()`](Self::validate) rejects preview tickets.
    /// 2. `aud == "preview:<expected_workspace_id>"` — the ticket is bound to
    ///    exactly one workspace; a ticket minted for another workspace fails closed.
    ///
    /// Returns `Err(G5Error::Unauthorized)` for every failure mode, matching
    /// the `validate()` non-disclosure posture.
    pub fn validate_preview_ticket(
        &self,
        token: &str,
        expected_workspace_id: uuid::Uuid,
    ) -> Result<JwtClaims, G5Error> {
        let token_data = jsonwebtoken::decode::<JwtClaims>(
            token,
            self.decoding_key()?,
            &self.preview_validation,
        )?;
        let claims = token_data.claims;
        if claims.token_use.as_deref() != Some(PREVIEW_TOKEN_USE) {
            return Err(G5Error::Unauthorized);
        }
        if claims.aud.as_deref() != Some(preview_ticket_audience(expected_workspace_id).as_str()) {
            return Err(G5Error::Unauthorized);
        }
        Ok(claims)
    }

    /// Validates a single-use preview HANDOFF ticket and returns the parsed
    /// claims (auth [D12]; issue G5Labs/G5#935).
    ///
    /// Handoff tickets enter the per-preview subdomain origin as the
    /// `__g5_preview_ticket` URL parameter and are exchanged exactly once for
    /// the session cookie (routes/preview [D15]). Identical posture to
    /// [`validate_preview_ticket`](Self::validate_preview_ticket) — same key,
    /// ES256, zero leeway, explicit workspace-bound `aud` — but requires
    /// `token_use == "preview_handoff"`, so the three validators accept
    /// pairwise-disjoint token populations. Single-use (`jti`) bookkeeping is
    /// the caller's responsibility: this module stays stateless.
    ///
    /// Returns `Err(G5Error::Unauthorized)` for every failure mode, matching
    /// the `validate()` non-disclosure posture.
    pub fn validate_preview_handoff_ticket(
        &self,
        token: &str,
        expected_workspace_id: uuid::Uuid,
    ) -> Result<JwtClaims, G5Error> {
        let token_data = jsonwebtoken::decode::<JwtClaims>(
            token,
            self.decoding_key()?,
            &self.preview_validation,
        )?;
        let claims = token_data.claims;
        if claims.token_use.as_deref() != Some(PREVIEW_HANDOFF_TOKEN_USE) {
            return Err(G5Error::Unauthorized);
        }
        if claims.aud.as_deref() != Some(preview_ticket_audience(expected_workspace_id).as_str()) {
            return Err(G5Error::Unauthorized);
        }
        Ok(claims)
    }

    /// Validate a short-lived WebSocket ticket and, for terminal streams,
    /// require that the ticket was minted for the exact workspace route.
    pub fn validate_websocket_ticket(
        &self,
        token: &str,
        expected_workspace_id: Option<uuid::Uuid>,
    ) -> Result<JwtClaims, G5Error> {
        let token_data = jsonwebtoken::decode::<JwtClaims>(
            token,
            self.decoding_key()?,
            &self.preview_validation,
        )?;
        let claims = token_data.claims;
        if claims.token_use.as_deref() != Some(WEBSOCKET_TOKEN_USE)
            || claims.aud.as_deref()
                != Some(websocket_ticket_audience(expected_workspace_id).as_str())
        {
            return Err(G5Error::Unauthorized);
        }
        Ok(claims)
    }

    /// Validate a token for one workspace VM incarnation. Callers must still
    /// compare these claims with the current database row before acting.
    pub fn validate_workspace_incarnation(
        &self,
        token: &str,
    ) -> Result<WorkspaceIncarnationClaims, G5Error> {
        let token_data = jsonwebtoken::decode::<WorkspaceIncarnationClaims>(
            token,
            self.decoding_key()?,
            &self.preview_validation,
        )?;
        let claims = token_data.claims;
        if claims.token_use != WORKSPACE_INCARNATION_TOKEN_USE
            || claims.aud != WORKSPACE_INCARNATION_AUDIENCE
            || claims.epoch == 0
        {
            return Err(G5Error::Unauthorized);
        }
        Ok(claims)
    }

    /// Validate a workspace agent's preview credential for the preview task
    /// the request addresses. The relay must still recheck the caller
    /// incarnation, agent scope and the task's readiness before relaying.
    pub fn validate_preview_agent(
        &self,
        token: &str,
        expected_task_id: uuid::Uuid,
    ) -> Result<PreviewAgentClaims, G5Error> {
        let claims = jsonwebtoken::decode::<PreviewAgentClaims>(
            token,
            self.decoding_key()?,
            &self.preview_validation,
        )?
        .claims;
        if claims.token_use != PREVIEW_AGENT_TOKEN_USE
            || claims.task_id != expected_task_id
            || claims.aud != preview_agent_audience(expected_task_id)
            || claims.epoch == 0
            || [
                claims.org_id,
                claims.owner_user_id,
                claims.workspace_id,
                claims.vm_id,
                claims.target_workspace_id,
            ]
            .iter()
            .any(uuid::Uuid::is_nil)
            || claims.exp <= claims.iat
            || claims.exp.saturating_sub(claims.iat) > PREVIEW_AGENT_TTL_SECS
        {
            return Err(G5Error::Unauthorized);
        }
        Ok(claims)
    }

    /// Signature validation is only the first step. Git reads also recheck host
    /// liveness, actor access and the complete current membership vector.
    pub fn validate_host_source(&self, token: &str) -> Result<HostSourceClaims, G5Error> {
        let claims = jsonwebtoken::decode::<HostSourceClaims>(
            token,
            self.decoding_key()?,
            &self.preview_validation,
        )?
        .claims;
        if claims.token_use != HOST_SOURCE_TOKEN_USE
            || claims.aud != HOST_SOURCE_AUDIENCE
            || claims.host_id.is_nil()
            || claims.org_id.is_nil()
            || claims.source_workspace_id.is_nil()
            || claims.actor_user_id.is_nil()
            || claims.repo_ids.is_empty()
            || claims.repo_ids.len() > 64
            || claims.repo_ids.iter().any(uuid::Uuid::is_nil)
            || claims
                .repo_ids
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != claims.repo_ids.len()
            || claims.membership_digest.len() != 64
            || !claims
                .membership_digest
                .bytes()
                .all(|b| b.is_ascii_hexdigit())
            || claims.exp <= claims.iat
            || claims.exp.saturating_sub(claims.iat) > HOST_SOURCE_TTL_SECS
        {
            return Err(G5Error::Unauthorized);
        }
        Ok(claims)
    }

    /// Classifies whether a bearer token could EVER authenticate against the
    /// platform (auth [D11]; consumed by the preview relay's selective
    /// `authorization` stripping, routes/preview [D9]).
    ///
    /// Returns `true` when either the signature verifies against the
    /// platform decoding key with expiry validation disabled (an expired
    /// platform token is still platform-issued and must not reach untrusted
    /// app code), or — regardless of signature — the payload segment parses
    /// as JSON whose `token_use` claim is a G5-reserved value
    /// ([`RESERVED_TOKEN_USE_VALUES`]: `preview` / `capability`; defense in
    /// depth across signing-key rotation).
    ///
    /// Without a decoding key, returns `true` for every value: the preview
    /// boundary must fail closed rather than forward unclassifiable bearers.
    /// With a key, returns `false` for JWTs signed by foreign keys (including app tokens
    /// carrying a NON-reserved `token_use` such as an IdP's `access` / `id`),
    /// opaque non-JWT bearers other than platform API keys
    /// ([`API_KEY_PREFIX`]), and malformed values: those cannot
    /// authenticate to G5 and are the previewed app's own login. Classification
    /// only — a `true` here must never be used to ACCEPT a token; acceptance
    /// stays with the strict validators above.
    pub fn is_platform_credential(&self, token: &str) -> bool {
        // An opaque platform API key never parses as a JWT, so the signature
        // arm below cannot classify it; its prefix does.
        if token.starts_with(API_KEY_PREFIX) {
            return true;
        }
        let Some(key) = self.decoding_key.as_ref() else {
            return true;
        };
        // Provenance check: same key + algorithm as `validate`, but expiry
        // ignored and issuer/audience irrelevant. `token_use`-carrying
        // tokens pass this decode — they are platform credentials even
        // though `validate` rejects them as Bearer logins.
        let mut sig_only = self.preview_validation.clone();
        sig_only.validate_exp = false;
        sig_only.validate_aud = false;
        sig_only.iss = None;
        sig_only.required_spec_claims = Default::default();
        // Decoded as free-form JSON, deliberately not as `JwtClaims`: the
        // question is who signed the token, not whether its claims still
        // parse. A token minted before a scope or field was retired is still
        // platform-issued and must keep classifying as ours.
        if jsonwebtoken::decode::<serde_json::Value>(token, key, &sig_only).is_ok() {
            return true;
        }
        payload_carries_reserved_token_use(token)
    }

    /// Validates a JWT token AND checks that all required scopes are present.
    ///
    /// First calls [`validate()`](Self::validate) to verify the token. If valid,
    /// checks that every scope in `required_scopes` is present in the claims'
    /// `scopes` field.
    ///
    /// Returns `Err(G5Error::Unauthorized)` if the token itself is invalid.
    /// Returns `Err(G5Error::Forbidden { missing_scopes })` if the token is
    /// valid but the claims don't contain all required scopes.
    pub fn validate_with_scopes(
        &self,
        token: &str,
        required_scopes: &[Scope],
    ) -> Result<JwtClaims, G5Error> {
        let claims = self.validate(token)?;

        if !has_scopes(&claims, required_scopes) {
            let missing_scopes: Vec<String> = required_scopes
                .iter()
                .filter(|s| !claims.scopes.contains(s))
                .map(|s| s.to_string())
                .collect();
            return Err(G5Error::Forbidden { missing_scopes });
        }

        Ok(claims)
    }
}

/// Whether a JWT-shaped token's payload segment carries a G5-**reserved**
/// `token_use` value ([`RESERVED_TOKEN_USE_VALUES`]) without verifying the
/// signature ([D11] defense-in-depth arm — see
/// [`JwtValidator::is_platform_credential`]). Mere presence of a `token_use`
/// claim is NOT sufficient: a foreign app token with `token_use: "access"`
/// returns `false` so it passes through the preview relay. Malformed input
/// returns `false`.
fn payload_carries_reserved_token_use(token: &str) -> bool {
    let mut parts = token.split('.');
    let (Some(_header), Some(payload), Some(_sig), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    use base64::Engine as _;
    let Ok(bytes) = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(payload) else {
        return false;
    };
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return false;
    };
    value
        .get("token_use")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|tu| RESERVED_TOKEN_USE_VALUES.contains(&tu))
}

// ---------------------------------------------------------------------------
// has_scopes — scope subset check
// ---------------------------------------------------------------------------

/// Checks whether the given claims contain ALL required scopes.
///
/// Returns `true` if every scope in `required` is present in
/// `claims.scopes`. Returns `true` if `required` is empty.
#[must_use]
pub fn has_scopes(claims: &JwtClaims, required: &[Scope]) -> bool {
    required.iter().all(|scope| claims.scopes.contains(scope))
}

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

#[cfg(test)]
impl JwtClaims {
    /// Creates test claims with sensible defaults.
    ///
    /// - `sub` = `"test-user-id"`
    /// - `org` = `"test-org-id"`
    /// - `scopes` = `["workspaces:read", "workspaces:write"]`
    /// - `iat` = current time
    /// - `exp` = current time + 3600
    #[must_use]
    pub fn test_default() -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        Self {
            sub: "test-user-id".to_string(),
            org: "test-org-id".to_string(),
            scopes: vec![Scope::WorkspacesRead, Scope::WorkspacesWrite],
            iat: now,
            exp: now + 3600,
            iss: Some("g5-api".to_string()),
            aud: Some("g5".to_string()),
            jti: Some("test-jti-00000000".to_string()),
            token_use: None,
            client: None,
        }
    }

    /// Creates test claims with custom scopes.
    #[must_use]
    pub fn test_with_scopes(scopes: Vec<Scope>) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        Self {
            sub: "test-user-id".to_string(),
            org: "test-org-id".to_string(),
            scopes,
            iat: now,
            exp: now + 3600,
            iss: Some("g5-api".to_string()),
            aud: Some("g5".to_string()),
            jti: Some("test-jti-00000001".to_string()),
            token_use: None,
            client: None,
        }
    }

    /// Creates expired test claims (exp = current time - 3600).
    #[must_use]
    pub fn test_expired() -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        Self {
            sub: "test-user-id".to_string(),
            org: "test-org-id".to_string(),
            scopes: vec![Scope::WorkspacesRead, Scope::WorkspacesWrite],
            iat: now - 7200,
            exp: now - 3600,
            iss: Some("g5-api".to_string()),
            aud: Some("g5".to_string()),
            jti: Some("test-jti-expired".to_string()),
            token_use: None,
            client: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::super::Scope;
    use super::*;
    use jsonwebtoken::{encode, EncodingKey, Header};

    // -- Test ES256 key pair (NIST P-256 / prime256v1) --
    //
    // Pre-generated via:
    //   openssl ecparam -name prime256v1 -genkey -noout > ec_private.pem
    //   openssl pkcs8 -topk8 -nocrypt -in ec_private.pem -out ec_private_pkcs8.pem
    //   openssl ec -in ec_private.pem -pubout -out ec_public.pem
    //
    // These are test-only keys — NOT for production use.

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

    /// Creates a signed JWT token from the given claims using the test private key.
    fn sign_token(claims: &JwtClaims) -> String {
        sign_token_with(claims, TEST_PRIVATE_KEY_PEM)
    }

    /// A second, unrelated ES256 key pair standing in for a previewed
    /// customer app's own signing key ([D11] classifier tests). Test-only.
    const FOREIGN_PRIVATE_KEY_PEM: &str = "\
-----BEGIN PRIVATE KEY-----
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQg1lBfhwF4cOv3cVYf
UMxcNdboDWbksUlDdFo6gY8XypmhRANCAARvTDNBIMd35GR7ONUMYUCaVrx4Ndlg
vxcv9KQzML9LIqcza/CdTbO8XLgcwRrO/B8Vyc8z4RcsMbsdOAD1y5ad
-----END PRIVATE KEY-----";

    /// Signs claims with an arbitrary ES256 private key.
    fn sign_token_with(claims: &JwtClaims, private_pem: &str) -> String {
        sign_value_with(
            &serde_json::to_value(claims).expect("claims serialize"),
            private_pem,
        )
    }

    /// Signs an arbitrary JSON payload, for tokens whose claim shape the
    /// current `JwtClaims` no longer accepts.
    fn sign_value_with(payload: &serde_json::Value, private_pem: &str) -> String {
        let key = EncodingKey::from_ec_pem(private_pem.as_bytes())
            .expect("test private key should be valid");
        let header = Header::new(Algorithm::ES256);
        encode(&header, payload, &key).expect("encoding should succeed")
    }

    #[test]
    fn absent_verifier_rejects_every_token_kind_and_retains_reserved_classification() {
        let rejecting = JwtValidator::reject_all();
        let configured = JwtValidator::new(TEST_PUBLIC_KEY_PEM, None, None).unwrap();
        let claims = JwtClaims::test_default();
        let token = sign_token(&claims);
        assert!(configured.validate(&token).is_ok());
        assert!(matches!(
            rejecting.validate(&token),
            Err(G5Error::Unauthorized)
        ));
        assert!(matches!(
            rejecting.validate_with_scopes(&token, &[]),
            Err(G5Error::Unauthorized)
        ));
        assert!(rejecting.is_platform_credential(&token));
        assert!(rejecting.is_platform_credential("opaque-app-bearer"));

        let workspace = uuid::Uuid::new_v4();
        for token_use in [
            PREVIEW_TOKEN_USE,
            PREVIEW_HANDOFF_TOKEN_USE,
            WEBSOCKET_TOKEN_USE,
        ] {
            let mut ticket = claims.clone();
            ticket.token_use = Some(token_use.into());
            ticket.aud = Some(if token_use == WEBSOCKET_TOKEN_USE {
                websocket_ticket_audience(Some(workspace))
            } else {
                preview_ticket_audience(workspace)
            });
            let token = sign_token(&ticket);
            let validate = |validator: &JwtValidator| match token_use {
                PREVIEW_TOKEN_USE => validator.validate_preview_ticket(&token, workspace),
                PREVIEW_HANDOFF_TOKEN_USE => {
                    validator.validate_preview_handoff_ticket(&token, workspace)
                }
                _ => validator.validate_websocket_ticket(&token, Some(workspace)),
            };
            assert!(validate(&configured).is_ok(), "{token_use}");
            assert!(
                matches!(validate(&rejecting), Err(G5Error::Unauthorized)),
                "{token_use}"
            );
            assert!(
                rejecting.is_platform_credential(&token),
                "reserved tokens must still be stripped"
            );
        }
        let minter = crate::JwtMinter::new(TEST_PRIVATE_KEY_PEM, "g5-api", "g5").unwrap();
        let incarnation = minter
            .mint_workspace_incarnation(workspace, workspace, workspace, workspace, 1, 60)
            .unwrap();
        assert!(configured
            .validate_workspace_incarnation(&incarnation)
            .is_ok());
        assert!(matches!(
            rejecting.validate_workspace_incarnation(&incarnation),
            Err(G5Error::Unauthorized)
        ));
        assert!(rejecting.is_platform_credential(&incarnation));
        assert!(rejecting.validate("malformed").is_err());
    }

    // -- is_platform_credential tests ([D11]) --

    /// A platform token minted before a scope was retired no longer parses as
    /// `JwtClaims`, but it is still platform-issued: the relay must keep
    /// stripping it (the `blobs:read` retirement, G5Labs/G5-Lean#457).
    #[test]
    fn platform_credential_classification_does_not_depend_on_claim_shape() {
        let validator =
            JwtValidator::new(TEST_PUBLIC_KEY_PEM, None, None).expect("validator should build");
        let mut legacy = serde_json::to_value(JwtClaims::test_default()).unwrap();
        legacy["scopes"] = serde_json::json!(["workspaces:read", "blobs:read"]);
        let live = sign_value_with(&legacy, TEST_PRIVATE_KEY_PEM);
        // Strict validation still refuses the retired scope string...
        assert!(matches!(
            validator.validate(&live),
            Err(G5Error::Unauthorized)
        ));
        // ...but the token must never reach a previewed application.
        assert!(validator.is_platform_credential(&live));

        legacy["exp"] = serde_json::json!(legacy["iat"].as_i64().unwrap() - 7200);
        assert!(validator.is_platform_credential(&sign_value_with(&legacy, TEST_PRIVATE_KEY_PEM)));

        // The same shape under a foreign key is an app's own token.
        assert!(
            !validator.is_platform_credential(&sign_value_with(&legacy, FOREIGN_PRIVATE_KEY_PEM))
        );
    }

    #[test]
    fn platform_credential_classifier_strips_ours_and_forwards_foreign() {
        let validator =
            JwtValidator::new(TEST_PUBLIC_KEY_PEM, None, None).expect("validator should build");

        // Live platform access token → platform credential.
        let live = sign_token(&JwtClaims::test_default());
        assert!(validator.is_platform_credential(&live));

        // EXPIRED platform token: still platform-issued, must classify true.
        let mut expired_claims = JwtClaims::test_default();
        expired_claims.exp = expired_claims.iat - 7200;
        let expired = sign_token(&expired_claims);
        assert!(validator.is_platform_credential(&expired));

        // Preview-ticket-style token (token_use) signed by the platform key.
        let mut ticket_claims = JwtClaims::test_default();
        ticket_claims.token_use = Some("preview".to_string());
        assert!(validator.is_platform_credential(&sign_token(&ticket_claims)));

        // RESERVED token_use ("preview") under a FOREIGN signature:
        // defense-in-depth arm still classifies it as platform → stripped.
        assert!(validator
            .is_platform_credential(&sign_token_with(&ticket_claims, FOREIGN_PRIVATE_KEY_PEM)));

        // A customer app's own JWT (foreign key, no token_use): NOT ours —
        // this is the header-auth app login that must pass through.
        let app_token = sign_token_with(&JwtClaims::test_default(), FOREIGN_PRIVATE_KEY_PEM);
        assert!(!validator.is_platform_credential(&app_token));

        // A foreign app token carrying a NON-reserved IdP `token_use`
        // (e.g. Cognito `access` / `id`): merely having a token_use claim is
        // NOT a G5 provenance signal — it must pass through, or we recreate
        // the login-bounce bug for that app class (PR #889 review PG-1/BC-1).
        for idp_value in ["access", "id", "refresh"] {
            let mut idp_claims = JwtClaims::test_default();
            idp_claims.token_use = Some(idp_value.to_string());
            assert!(
                !validator
                    .is_platform_credential(&sign_token_with(&idp_claims, FOREIGN_PRIVATE_KEY_PEM)),
                "foreign token_use={idp_value} must pass through"
            );
        }

        // A foreign token carrying a RESERVED value ("capability") IS
        // platform-shaped → stripped (defense in depth across key rotation).
        let mut cap_claims = JwtClaims::test_default();
        cap_claims.token_use = Some(CAPABILITY_TOKEN_USE.to_string());
        assert!(validator
            .is_platform_credential(&sign_token_with(&cap_claims, FOREIGN_PRIVATE_KEY_PEM)));

        // Opaque platform API keys are platform credentials by prefix; other
        // opaque / malformed bearers cannot authenticate to G5.
        assert!(validator.is_platform_credential("g5k_0123456789abcdef0123456789abcdef"));
        assert!(!validator.is_platform_credential("g5kx-not-a-platform-key"));
        assert!(!validator.is_platform_credential("opaque-session-token-123"));
        assert!(!validator.is_platform_credential(""));
        assert!(!validator.is_platform_credential("a.b"));
    }

    #[test]
    fn platform_credential_classifier_never_accepts() {
        // Classification is not acceptance: the same expired token that
        // classifies true must still FAIL strict validation.
        let validator =
            JwtValidator::new(TEST_PUBLIC_KEY_PEM, None, None).expect("validator should build");
        let mut expired_claims = JwtClaims::test_default();
        expired_claims.exp = expired_claims.iat - 7200;
        let expired = sign_token(&expired_claims);
        assert!(validator.is_platform_credential(&expired));
        assert!(validator.validate(&expired).is_err());
    }

    // -- has_scopes tests --

    #[test]
    fn test_has_scopes_empty_required() {
        let claims = JwtClaims::test_default();
        assert!(has_scopes(&claims, &[]));
    }

    #[test]
    fn test_has_scopes_all_present() {
        let claims = JwtClaims::test_default();
        assert!(has_scopes(&claims, &[Scope::WorkspacesRead]));
        assert!(has_scopes(
            &claims,
            &[Scope::WorkspacesRead, Scope::WorkspacesWrite]
        ));
    }

    #[test]
    fn test_has_scopes_missing() {
        let claims = JwtClaims::test_default();
        assert!(!has_scopes(&claims, &[Scope::OrgAdmin]));
        assert!(!has_scopes(
            &claims,
            &[Scope::WorkspacesRead, Scope::OrgAdmin]
        ));
    }

    #[test]
    fn test_has_scopes_empty_claims() {
        let claims = JwtClaims::test_with_scopes(vec![]);
        assert!(has_scopes(&claims, &[]));
        assert!(!has_scopes(&claims, &[Scope::WorkspacesRead]));
    }

    // -- JwtClaims test helpers --

    #[test]
    fn test_claims_test_default() {
        let claims = JwtClaims::test_default();
        assert_eq!(claims.sub, "test-user-id");
        assert_eq!(claims.org, "test-org-id");
        assert_eq!(claims.scopes.len(), 2);
        assert!(claims.exp > claims.iat);
    }

    #[test]
    fn test_claims_test_with_scopes() {
        let claims = JwtClaims::test_with_scopes(vec![Scope::OrgAdmin]);
        assert_eq!(claims.scopes, vec![Scope::OrgAdmin]);
    }

    #[test]
    fn test_claims_test_expired() {
        let claims = JwtClaims::test_expired();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        assert!(claims.exp < now);
    }

    // -- JwtClaims serde tests --

    #[test]
    fn test_claims_serde_roundtrip() {
        let claims = JwtClaims::test_default();
        let json = serde_json::to_string(&claims).unwrap();
        let parsed: JwtClaims = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.sub, claims.sub);
        assert_eq!(parsed.org, claims.org);
        assert_eq!(
            parsed.scopes,
            vec![Scope::WorkspacesRead, Scope::WorkspacesWrite]
        );
        assert_eq!(parsed.iat, claims.iat);
        assert_eq!(parsed.exp, claims.exp);
        assert_eq!(parsed.iss, claims.iss);
        assert_eq!(parsed.aud, claims.aud);
        assert_eq!(parsed.jti, claims.jti);
    }

    #[test]
    fn test_claims_field_names_match_jwt_standard() {
        let claims = JwtClaims::test_default();
        let json = serde_json::to_value(&claims).unwrap();
        // Verify exact field names — no rename, matches JWT standard
        assert!(json.get("sub").is_some());
        assert!(json.get("org").is_some());
        assert!(json.get("scopes").is_some());
        assert!(json.get("iat").is_some());
        assert!(json.get("exp").is_some());
        assert!(json.get("iss").is_some());
        assert!(json.get("aud").is_some());
        assert!(json.get("jti").is_some());
    }

    #[test]
    fn test_claims_backward_compat_missing_iss_aud_jti() {
        // Old tokens without iss/aud/jti should still deserialize
        let json = r#"{"sub":"u","org":"o","scopes":[],"iat":0,"exp":9999999999}"#;
        let claims: JwtClaims = serde_json::from_str(json).unwrap();
        assert!(claims.iss.is_none());
        assert!(claims.aud.is_none());
        assert!(claims.jti.is_none());
    }

    #[test]
    fn test_claims_skip_serializing_none_fields() {
        let mut claims = JwtClaims::test_default();
        claims.iss = None;
        claims.aud = None;
        claims.jti = None;
        let json = serde_json::to_value(&claims).unwrap();
        assert!(json.get("iss").is_none(), "None iss should be omitted");
        assert!(json.get("aud").is_none(), "None aud should be omitted");
        assert!(json.get("jti").is_none(), "None jti should be omitted");
    }

    // -- JwtValidator construction tests --

    #[test]
    fn test_validator_new_invalid_pem() {
        let result = JwtValidator::new("not a valid PEM key", None, None);
        assert!(result.is_err());
        match result.unwrap_err() {
            G5Error::Internal { message } => {
                assert!(message.contains("Failed to parse ES256 public key"));
            }
            other => panic!("Expected G5Error::Internal, got {:?}", other),
        }
    }

    #[test]
    fn test_validator_new_empty_pem() {
        let result = JwtValidator::new("", None, None);
        assert!(result.is_err());
        match result.unwrap_err() {
            G5Error::Internal { .. } => {}
            other => panic!("Expected G5Error::Internal, got {:?}", other),
        }
    }

    #[test]
    fn test_validator_new_valid_pem() {
        let result = JwtValidator::new(TEST_PUBLIC_KEY_PEM, None, None);
        assert!(result.is_ok(), "Valid PEM should parse successfully");
    }

    // -- JwtValidator::validate tests --

    #[test]
    fn test_validate_valid_token() {
        let validator = JwtValidator::new(TEST_PUBLIC_KEY_PEM, None, None).unwrap();

        let claims = JwtClaims::test_default();
        let token = sign_token(&claims);

        let result = validator.validate(&token);
        assert!(result.is_ok(), "Valid token should validate: {:?}", result);
        let parsed = result.unwrap();
        assert_eq!(parsed.sub, "test-user-id");
        assert_eq!(parsed.org, "test-org-id");
        assert_eq!(
            parsed.scopes,
            vec![Scope::WorkspacesRead, Scope::WorkspacesWrite]
        );
    }

    #[test]
    fn test_validate_expired_token() {
        let validator = JwtValidator::new(TEST_PUBLIC_KEY_PEM, None, None).unwrap();

        let claims = JwtClaims::test_expired();
        let token = sign_token(&claims);

        let result = validator.validate(&token);
        assert!(result.is_err());
        match result.unwrap_err() {
            G5Error::Unauthorized => {} // expected
            other => panic!("Expected Unauthorized, got {:?}", other),
        }
    }

    /// §3.2: A JWT carrying `token_use` must be rejected by the JWT validator
    /// to prevent capability-token confusion attacks.
    #[test]
    fn rejects_jwt_with_token_use_claim() {
        let validator = JwtValidator::new(TEST_PUBLIC_KEY_PEM, None, None).unwrap();

        let mut claims = JwtClaims::test_default();
        claims.token_use = Some("capability".to_string());
        let token = sign_token(&claims);

        let result = validator.validate(&token);
        assert!(result.is_err(), "JWT with token_use should be rejected");
        match result.unwrap_err() {
            G5Error::Unauthorized => {}
            other => panic!("Expected Unauthorized, got {:?}", other),
        }
    }

    #[test]
    fn test_validate_malformed_token() {
        let validator = JwtValidator::new(TEST_PUBLIC_KEY_PEM, None, None).unwrap();

        let result = validator.validate("not.a.jwt.token");
        assert!(result.is_err());
        match result.unwrap_err() {
            G5Error::Unauthorized => {} // expected
            other => panic!("Expected Unauthorized, got {:?}", other),
        }
    }

    #[test]
    fn test_validate_empty_token() {
        let validator = JwtValidator::new(TEST_PUBLIC_KEY_PEM, None, None).unwrap();

        let result = validator.validate("");
        assert!(result.is_err());
        match result.unwrap_err() {
            G5Error::Unauthorized => {}
            other => panic!("Expected Unauthorized, got {:?}", other),
        }
    }

    // -- JwtValidator::validate_preview_ticket tests --

    fn test_workspace_id() -> uuid::Uuid {
        uuid::Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap()
    }

    fn preview_claims(workspace_id: uuid::Uuid) -> JwtClaims {
        let mut claims = JwtClaims::test_with_scopes(vec![Scope::TasksRead, Scope::WorkspacesRead]);
        claims.aud = Some(preview_ticket_audience(workspace_id));
        claims.token_use = Some(PREVIEW_TOKEN_USE.to_string());
        claims
    }

    #[test]
    fn preview_ticket_validates_for_matching_job() {
        let validator = JwtValidator::new(TEST_PUBLIC_KEY_PEM, None, None).unwrap();
        let token = sign_token(&preview_claims(test_workspace_id()));

        let claims = validator
            .validate_preview_ticket(&token, test_workspace_id())
            .expect("matching-workspace preview ticket should validate");
        assert_eq!(claims.sub, "test-user-id");
        assert_eq!(claims.scopes, vec![Scope::TasksRead, Scope::WorkspacesRead]);
    }

    #[test]
    fn preview_ticket_rejected_for_other_job() {
        let validator = JwtValidator::new(TEST_PUBLIC_KEY_PEM, None, None).unwrap();
        let token = sign_token(&preview_claims(test_workspace_id()));

        let other_job = uuid::Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap();
        match validator.validate_preview_ticket(&token, other_job) {
            Err(G5Error::Unauthorized) => {}
            other => panic!("Expected Unauthorized for foreign-workspace ticket, got {other:?}"),
        }
    }

    #[test]
    fn preview_ticket_rejected_when_expired() {
        let validator = JwtValidator::new(TEST_PUBLIC_KEY_PEM, None, None).unwrap();
        let mut claims = preview_claims(test_workspace_id());
        claims.iat -= 7200;
        claims.exp = claims.iat + 60;
        let token = sign_token(&claims);

        match validator.validate_preview_ticket(&token, test_workspace_id()) {
            Err(G5Error::Unauthorized) => {}
            other => panic!("Expected Unauthorized for expired ticket, got {other:?}"),
        }
    }

    #[test]
    fn preview_ticket_rejected_by_standard_validate() {
        // A preview ticket replayed as a Bearer credential must be dead:
        // `validate()` rejects any token carrying a `token_use` claim.
        let validator = JwtValidator::new(TEST_PUBLIC_KEY_PEM, None, None).unwrap();
        let token = sign_token(&preview_claims(test_workspace_id()));

        match validator.validate(&token) {
            Err(G5Error::Unauthorized) => {}
            other => panic!("Expected Unauthorized for preview ticket as Bearer, got {other:?}"),
        }
    }

    #[test]
    fn normal_jwt_rejected_by_preview_validator() {
        // A full-privilege access JWT smuggled into the preview cookie must
        // be equally dead: it has no `token_use: "preview"` claim.
        let validator = JwtValidator::new(TEST_PUBLIC_KEY_PEM, None, None).unwrap();
        let token = sign_token(&JwtClaims::test_default());

        match validator.validate_preview_ticket(&token, test_workspace_id()) {
            Err(G5Error::Unauthorized) => {}
            other => panic!("Expected Unauthorized for access JWT in cookie, got {other:?}"),
        }
    }

    #[test]
    fn preview_ticket_validates_when_issuer_and_audience_configured() {
        // With a configured expected audience ("g5"), the standard validation
        // path would reject the preview `aud` — the dedicated preview
        // Validation disables that check and binds the audience explicitly.
        let validator = JwtValidator::new(TEST_PUBLIC_KEY_PEM, Some("g5-api"), Some("g5")).unwrap();
        let token = sign_token(&preview_claims(test_workspace_id()));

        let result = validator.validate_preview_ticket(&token, test_workspace_id());
        assert!(
            result.is_ok(),
            "preview ticket should validate under configured iss/aud: {result:?}"
        );
    }

    #[test]
    fn websocket_ticket_is_disjoint_and_workspace_bound() {
        let validator = JwtValidator::new(TEST_PUBLIC_KEY_PEM, None, None).unwrap();
        let workspace_id = uuid::Uuid::new_v4();
        let other_workspace_id = uuid::Uuid::new_v4();
        let mut claims = JwtClaims::test_with_scopes(vec![Scope::WorkspacesWrite]);
        claims.aud = Some(websocket_ticket_audience(Some(workspace_id)));
        claims.token_use = Some(WEBSOCKET_TOKEN_USE.to_string());
        let token = sign_token(&claims);

        assert!(validator
            .validate_websocket_ticket(&token, Some(workspace_id))
            .is_ok());
        assert!(matches!(
            validator.validate_websocket_ticket(&token, Some(other_workspace_id)),
            Err(G5Error::Unauthorized)
        ));
        assert!(matches!(
            validator.validate_websocket_ticket(&token, None),
            Err(G5Error::Unauthorized)
        ));
        assert!(matches!(
            validator.validate(&token),
            Err(G5Error::Unauthorized)
        ));
    }

    #[test]
    fn access_jwt_cannot_open_a_websocket() {
        let validator = JwtValidator::new(TEST_PUBLIC_KEY_PEM, None, None).unwrap();
        let token = sign_token(&JwtClaims::test_default());
        assert!(matches!(
            validator.validate_websocket_ticket(&token, None),
            Err(G5Error::Unauthorized)
        ));
    }

    // -- JwtValidator::validate_preview_handoff_ticket tests --

    fn handoff_claims(workspace_id: uuid::Uuid) -> JwtClaims {
        let mut claims = JwtClaims::test_with_scopes(vec![Scope::TasksRead, Scope::WorkspacesRead]);
        claims.aud = Some(preview_ticket_audience(workspace_id));
        claims.token_use = Some(PREVIEW_HANDOFF_TOKEN_USE.to_string());
        claims.jti = Some("handoff-jti-1".to_string());
        claims
    }

    #[test]
    fn handoff_ticket_validates_for_matching_job() {
        let validator = JwtValidator::new(TEST_PUBLIC_KEY_PEM, None, None).unwrap();
        let token = sign_token(&handoff_claims(test_workspace_id()));
        let claims = validator
            .validate_preview_handoff_ticket(&token, test_workspace_id())
            .expect("matching-workspace handoff ticket should validate");
        assert_eq!(claims.jti.as_deref(), Some("handoff-jti-1"));
    }

    #[test]
    fn handoff_ticket_rejected_for_other_job() {
        let validator = JwtValidator::new(TEST_PUBLIC_KEY_PEM, None, None).unwrap();
        let token = sign_token(&handoff_claims(test_workspace_id()));
        let other_job = uuid::Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap();
        match validator.validate_preview_handoff_ticket(&token, other_job) {
            Err(G5Error::Unauthorized) => {}
            other => panic!("Expected Unauthorized for foreign-workspace handoff, got {other:?}"),
        }
    }

    #[test]
    fn handoff_and_session_ticket_populations_are_disjoint() {
        // The three validators accept pairwise-disjoint token_use values: a
        // handoff ticket must not authenticate as a session ticket or Bearer,
        // and a session ticket must not authenticate as a handoff.
        let validator = JwtValidator::new(TEST_PUBLIC_KEY_PEM, None, None).unwrap();
        let handoff = sign_token(&handoff_claims(test_workspace_id()));
        let session = sign_token(&preview_claims(test_workspace_id()));

        assert!(matches!(
            validator.validate_preview_ticket(&handoff, test_workspace_id()),
            Err(G5Error::Unauthorized)
        ));
        assert!(matches!(
            validator.validate(&handoff),
            Err(G5Error::Unauthorized)
        ));
        assert!(matches!(
            validator.validate_preview_handoff_ticket(&session, test_workspace_id()),
            Err(G5Error::Unauthorized)
        ));
    }

    #[test]
    fn handoff_ticket_is_platform_credential() {
        // The relay must strip a handoff ticket presented as a Bearer, since
        // `preview_handoff` is a G5-reserved token_use value.
        let validator = JwtValidator::new(TEST_PUBLIC_KEY_PEM, None, None).unwrap();
        let token = sign_token(&handoff_claims(test_workspace_id()));
        assert!(validator.is_platform_credential(&token));
    }

    // -- JwtValidator::validate_with_scopes tests --

    #[test]
    fn test_validate_with_scopes_all_present() {
        let validator = JwtValidator::new(TEST_PUBLIC_KEY_PEM, None, None).unwrap();

        let claims = JwtClaims::test_default();
        let token = sign_token(&claims);

        let result = validator.validate_with_scopes(&token, &[Scope::WorkspacesRead]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_with_scopes_empty_required() {
        let validator = JwtValidator::new(TEST_PUBLIC_KEY_PEM, None, None).unwrap();

        let claims = JwtClaims::test_default();
        let token = sign_token(&claims);

        let result = validator.validate_with_scopes(&token, &[]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_with_scopes_missing() {
        let validator = JwtValidator::new(TEST_PUBLIC_KEY_PEM, None, None).unwrap();

        let claims = JwtClaims::test_default();
        let token = sign_token(&claims);

        let result =
            validator.validate_with_scopes(&token, &[Scope::WorkspacesRead, Scope::OrgAdmin]);
        assert!(result.is_err());
        match result.unwrap_err() {
            G5Error::Forbidden { missing_scopes } => {
                assert_eq!(missing_scopes, vec!["org:admin"]);
            }
            other => panic!("Expected Forbidden, got {:?}", other),
        }
    }

    #[test]
    fn test_validate_with_scopes_invalid_token() {
        let validator = JwtValidator::new(TEST_PUBLIC_KEY_PEM, None, None).unwrap();

        let result = validator.validate_with_scopes("bad.token", &[Scope::WorkspacesRead]);
        assert!(result.is_err());
        match result.unwrap_err() {
            G5Error::Unauthorized => {} // invalid token → Unauthorized, not Forbidden
            other => panic!("Expected Unauthorized, got {:?}", other),
        }
    }

    #[test]
    fn test_validate_with_scopes_expired_token() {
        let validator = JwtValidator::new(TEST_PUBLIC_KEY_PEM, None, None).unwrap();

        let claims = JwtClaims::test_expired();
        let token = sign_token(&claims);

        let result = validator.validate_with_scopes(&token, &[Scope::WorkspacesRead]);
        assert!(result.is_err());
        match result.unwrap_err() {
            G5Error::Unauthorized => {} // expired → Unauthorized
            other => panic!("Expected Unauthorized, got {:?}", other),
        }
    }

    // -- From<jsonwebtoken::errors::Error> test --

    #[test]
    fn test_jwt_error_maps_to_unauthorized() {
        let err: G5Error =
            jsonwebtoken::errors::Error::from(jsonwebtoken::errors::ErrorKind::InvalidToken).into();
        match err {
            G5Error::Unauthorized => {} // expected
            other => panic!("Expected Unauthorized, got {:?}", other),
        }
    }

    // -- iss/aud validation tests --

    #[test]
    fn test_validate_with_correct_iss_aud() {
        let validator = JwtValidator::new(TEST_PUBLIC_KEY_PEM, Some("g5-api"), Some("g5")).unwrap();

        let claims = JwtClaims::test_default();
        let token = sign_token(&claims);

        let result = validator.validate(&token);
        assert!(
            result.is_ok(),
            "Token with correct iss/aud should validate: {:?}",
            result
        );
    }

    #[test]
    fn test_validate_rejects_wrong_issuer() {
        let validator =
            JwtValidator::new(TEST_PUBLIC_KEY_PEM, Some("wrong-issuer"), Some("g5")).unwrap();

        let claims = JwtClaims::test_default();
        let token = sign_token(&claims);

        let result = validator.validate(&token);
        assert!(result.is_err());
        match result.unwrap_err() {
            G5Error::Unauthorized => {}
            other => panic!("Expected Unauthorized for wrong issuer, got {:?}", other),
        }
    }

    #[test]
    fn test_validate_rejects_wrong_audience() {
        let validator =
            JwtValidator::new(TEST_PUBLIC_KEY_PEM, Some("g5-api"), Some("wrong-audience")).unwrap();

        let claims = JwtClaims::test_default();
        let token = sign_token(&claims);

        let result = validator.validate(&token);
        assert!(result.is_err());
        match result.unwrap_err() {
            G5Error::Unauthorized => {}
            other => panic!("Expected Unauthorized for wrong audience, got {:?}", other),
        }
    }

    #[test]
    fn test_validate_rejects_missing_iss_when_required() {
        let validator = JwtValidator::new(TEST_PUBLIC_KEY_PEM, Some("g5-api"), None).unwrap();

        let mut claims = JwtClaims::test_default();
        claims.iss = None; // Missing issuer
        let token = sign_token(&claims);

        let result = validator.validate(&token);
        assert!(
            result.is_err(),
            "Token without iss should be rejected when iss validation is enabled"
        );
    }

    #[test]
    fn test_validate_rejects_missing_aud_when_required() {
        let validator = JwtValidator::new(TEST_PUBLIC_KEY_PEM, None, Some("g5")).unwrap();

        let mut claims = JwtClaims::test_default();
        claims.aud = None; // Missing audience
        let token = sign_token(&claims);

        let result = validator.validate(&token);
        assert!(
            result.is_err(),
            "Token without aud should be rejected when aud validation is enabled"
        );
    }

    #[test]
    fn test_validate_no_iss_aud_validation_accepts_old_tokens() {
        let validator = JwtValidator::new(TEST_PUBLIC_KEY_PEM, None, None).unwrap();

        // Token without iss/aud (old-style)
        let mut claims = JwtClaims::test_default();
        claims.iss = None;
        claims.aud = None;
        claims.jti = None;
        let token = sign_token(&claims);

        let result = validator.validate(&token);
        assert!(
            result.is_ok(),
            "Old tokens should still validate with no iss/aud enforcement: {:?}",
            result
        );
    }
}
