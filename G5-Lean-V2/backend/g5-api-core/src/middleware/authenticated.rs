//! Type-state authentication wrappers — compile-time auth enforcement.
//!
//! Provides [`AuthenticatedUser`] and [`AuthenticatedActor`], which are
//! **axum extractors** with private fields and a single construction path
//! through the verified [`AuthContext`]. By accepting these types instead
//! of raw `AuthContext` + `AuthContext::require_user_id()`, handlers make it impossible
//! to forget authentication at compile time.
//!
//! # Why this exists
//!
//! Before:
//! ```ignore
//! async fn create_workspace(auth: AuthContext, ...) -> ... {
//!     let user_id = auth.require_user_id()?;  // rejects Host contexts
//!     let org_id = auth.org_id().ok_or(…)?;    // org identity still needs parsing
//!     state.workspace_service.create_workspace(&state.system_pool, org_id, user_id, req).await
//! }
//! ```
//!
//! After:
//! ```ignore
//! async fn create_workspace(user: AuthenticatedUser, ...) -> ... {
//!     // user.user_id() and user.org_id() are infallible — the extractor
//!     // already verified the auth context is User and parsed the UUIDs.
//!     state.workspace_service.create_workspace(&state.system_pool, user.org_id(), user.user_id(), req).await
//! }
//! ```
//!
//! Handlers that keep `AuthContext` use its shared `require_user_id()` method.
//! These extractors additionally validate identity fields before handler entry.

use async_trait::async_trait;
use axum::extract::FromRequestParts;
use http::request::Parts;
use uuid::Uuid;

use g5_auth::{JwtClaims, Scope};
use g5_contracts::error::G5Error;
use g5_contracts::types::ids::{OrgId, UserId};

use crate::errors::AppError;
use crate::middleware::AuthContext;

// ---------------------------------------------------------------------------
// AuthenticatedHost — worker-host extractor (rev-7 §5.7)
// ---------------------------------------------------------------------------

/// Proof that the request was made by an authenticated **worker host** (a row
/// in the `worker_hosts` table), authenticated via a `host_jwt`.
///
/// Construction goes through the [`FromRequestParts`] implementation, which
/// rejects non-Host auth contexts with `Forbidden`. All accessors are
/// infallible — UUID parsing happens during extraction so handlers receive a
/// typed `Uuid` rather than a string they have to re-parse.
///
/// # Usage
/// ```ignore
/// async fn host_status_update(
///     host: AuthenticatedHost,
///     Json(req): Json<StatusUpdateReq>,
/// ) -> ApiResult<()> {
///     // host.host_id() and host.org_id() are infallible Uuids.
///     persist_host_status(host.host_id(), host.org_id(), req).await
/// }
/// ```
///
/// See `g5/g5-api/worker_hosts` [D4] for the routing contract.
#[derive(Debug, Clone)]
pub struct AuthenticatedHost {
    host_id: Uuid,
    org_id: Uuid,
    scopes: Vec<Scope>,
}

impl AuthenticatedHost {
    /// The authenticated host's UUID (`worker_hosts.host_id`).
    #[must_use]
    pub fn host_id(&self) -> Uuid {
        self.host_id
    }

    /// The owning organisation's UUID (`worker_hosts.org_id`).
    #[must_use]
    pub fn org_id(&self) -> Uuid {
        self.org_id
    }

    /// The host's effective scopes. Always contains [`Scope::WorkerHost`]
    /// (the discriminant the middleware used to route here).
    #[must_use]
    pub fn scopes(&self) -> &[Scope] {
        &self.scopes
    }

    /// Check whether the host has a specific scope.
    #[must_use]
    pub fn has_scope(&self, scope: &Scope) -> bool {
        self.scopes.contains(scope)
    }
}

#[async_trait]
impl<S> FromRequestParts<S> for AuthenticatedHost
where
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let auth = parts
            .extensions
            .get::<AuthContext>()
            .cloned()
            .ok_or_else(|| AppError::from_error(G5Error::Unauthorized))?;

        match auth {
            AuthContext::Host {
                host_id,
                org_id,
                scopes,
            } => {
                // AuthContext::Host stores typed Uuids — no re-parse needed.
                // Defensive: the middleware routes here only when the scope is
                // present, but the typed extractor's contract is that the
                // scope is on the row — enforce on construction so a future
                // refactor that wrongly populates AuthContext::Host without
                // the scope fails loudly here rather than silently bypassing.
                if !scopes.contains(&Scope::WorkerHost) {
                    return Err(AppError::from_error(G5Error::Forbidden {
                        missing_scopes: vec![Scope::WorkerHost.to_string()],
                    }));
                }
                Ok(Self {
                    host_id,
                    org_id,
                    scopes,
                })
            }
            _ => Err(AppError::from_error(G5Error::Forbidden {
                missing_scopes: vec![Scope::WorkerHost.to_string()],
            })),
        }
    }
}

// ---------------------------------------------------------------------------
// AuthenticatedUser — user-only extractor
// ---------------------------------------------------------------------------

/// Proof that the request was made by an authenticated **user** (not a
/// worker or service). All fields are private — the only construction
/// path is through the [`FromRequestParts`] implementation, which
/// rejects non-User auth contexts at extraction time.
///
/// # Infallible accessors
///
/// Because the extractor validates everything up-front, all accessors
/// are infallible (no `Result`, no `Option`).
///
/// # Usage
///
/// Replace:
/// ```ignore
/// async fn handler(auth: AuthContext) -> Result<..., AppError> {
///     let user_id = auth.require_user_id()?;
///     // ...
/// }
/// ```
/// With:
/// ```ignore
/// async fn handler(user: AuthenticatedUser) -> Result<..., AppError> {
///     let user_id = user.user_id();
///     // ...
/// }
/// ```
#[derive(Debug, Clone)]
pub struct AuthenticatedUser {
    user_id: UserId,
    org_id: OrgId,
    claims: JwtClaims,
}

impl AuthenticatedUser {
    /// The authenticated user's ID (parsed from `JwtClaims.sub`).
    #[must_use]
    pub fn user_id(&self) -> UserId {
        self.user_id
    }

    /// The user's current org (parsed from `JwtClaims.org`).
    #[must_use]
    pub fn org_id(&self) -> OrgId {
        self.org_id
    }

    /// The user's UUID (convenience for APIs that still use raw `Uuid`).
    #[must_use]
    pub fn user_uuid(&self) -> Uuid {
        self.user_id.into_uuid()
    }

    /// The org's UUID (convenience for APIs that still use raw `Uuid`).
    #[must_use]
    pub fn org_uuid(&self) -> Uuid {
        self.org_id.into_uuid()
    }

    /// The full JWT claims (for cases where scopes or other fields are needed).
    #[must_use]
    pub fn claims(&self) -> &JwtClaims {
        &self.claims
    }

    /// The user's effective scopes.
    #[must_use]
    pub fn scopes(&self) -> &[Scope] {
        &self.claims.scopes
    }

    /// Check whether the user has a specific scope.
    #[must_use]
    pub fn has_scope(&self, scope: &Scope) -> bool {
        self.claims.scopes.contains(scope)
    }
}

#[async_trait]
impl<S> FromRequestParts<S> for AuthenticatedUser
where
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let auth = parts
            .extensions
            .get::<AuthContext>()
            .cloned()
            .ok_or_else(|| AppError::from_error(G5Error::Unauthorized))?;

        match auth {
            AuthContext::User(claims) => {
                let user_id = claims.sub.parse::<Uuid>().map_err(|_| {
                    AppError::from_error(G5Error::Internal {
                        message: "Invalid user ID in JWT claims".into(),
                    })
                })?;
                let org_id = claims.org.parse::<Uuid>().map_err(|_| {
                    AppError::from_error(G5Error::Internal {
                        message: "Invalid org ID in JWT claims".into(),
                    })
                })?;

                Ok(Self {
                    user_id: UserId::from_uuid(user_id),
                    org_id: OrgId::from_uuid(org_id),
                    claims,
                })
            }
            _ => Err(AppError::from_error(G5Error::Forbidden {
                missing_scopes: vec!["user_auth_required".into()],
            })),
        }
    }
}

// ---------------------------------------------------------------------------
// AuthenticatedActor — any non-anonymous identity (user, worker, or service)
// ---------------------------------------------------------------------------

/// Proof that the request was made by *some* authenticated identity.
///
/// Accepts User, Worker, and Service contexts. The `actor_id` is always
/// available (for Service, it falls back to a nil UUID).
///
/// Use this for endpoints that workers and services can also call (e.g.,
/// task status updates, worker registration).
#[derive(Debug, Clone)]
pub struct AuthenticatedActor {
    actor_id: UserId,
    org_id: Option<OrgId>,
    context: AuthContext,
}

impl AuthenticatedActor {
    /// The actor's identity (user ID, worker ID, or nil for service).
    #[must_use]
    pub fn actor_id(&self) -> UserId {
        self.actor_id
    }

    /// The org context, if available.
    #[must_use]
    pub fn org_id(&self) -> Option<OrgId> {
        self.org_id
    }

    /// The underlying `AuthContext` for cases requiring full discrimination.
    #[must_use]
    pub fn context(&self) -> &AuthContext {
        &self.context
    }

    /// Returns `true` if this is a user context.
    #[must_use]
    pub fn is_user(&self) -> bool {
        self.context.is_user()
    }
}

#[async_trait]
impl<S> FromRequestParts<S> for AuthenticatedActor
where
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let auth = parts
            .extensions
            .get::<AuthContext>()
            .cloned()
            .ok_or_else(|| AppError::from_error(G5Error::Unauthorized))?;

        let actor_id_str = auth
            .actor_id()
            .map(std::borrow::Cow::into_owned)
            .unwrap_or_else(|| "00000000-0000-0000-0000-000000000000".to_string());
        let actor_id = actor_id_str.parse::<Uuid>().map_err(|_| {
            AppError::from_error(G5Error::Internal {
                message: "Invalid actor ID in auth context".into(),
            })
        })?;
        let org_id = auth
            .org_id()
            .and_then(|s| s.as_ref().parse::<Uuid>().ok())
            .map(OrgId::from_uuid);

        Ok(Self {
            actor_id: UserId::from_uuid(actor_id),
            org_id,
            context: auth,
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    use http::Request;

    fn parts_with_extension<T: Clone + Send + Sync + 'static>(ext: T) -> http::request::Parts {
        let mut req = Request::builder().body(()).unwrap();
        req.extensions_mut().insert(ext);
        let (parts, _) = req.into_parts();
        parts
    }

    fn parts_no_extension() -> http::request::Parts {
        let req = Request::builder().body(()).unwrap();
        let (parts, _) = req.into_parts();
        parts
    }

    #[tokio::test]
    async fn authenticated_host_extracts_from_host_context() {
        let host_id = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
        let org_id = Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap();
        let mut parts = parts_with_extension(AuthContext::Host {
            host_id,
            org_id,
            scopes: vec![Scope::WorkerHost],
        });

        let extracted = AuthenticatedHost::from_request_parts(&mut parts, &())
            .await
            .expect("Host extraction should succeed");
        assert_eq!(extracted.host_id(), host_id);
        assert_eq!(extracted.org_id(), org_id);
        assert!(extracted.has_scope(&Scope::WorkerHost));
    }

    #[tokio::test]
    async fn authenticated_host_rejects_user_context() {
        let claims = JwtClaims {
            sub: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            org: "660e8400-e29b-41d4-a716-446655440000".to_string(),
            scopes: vec![Scope::WorkspacesRead],
            iat: 0,
            exp: 0,
            iss: None,
            aud: None,
            jti: None,
            token_use: None,
            client: None,
        };
        let mut parts = parts_with_extension(AuthContext::User(claims));

        let err = AuthenticatedHost::from_request_parts(&mut parts, &())
            .await
            .expect_err("User context must not satisfy AuthenticatedHost");
        match err.inner {
            G5Error::Forbidden { missing_scopes } => {
                assert!(missing_scopes.contains(&Scope::WorkerHost.to_string()));
            }
            other => panic!("Expected Forbidden, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn authenticated_host_rejects_missing_context() {
        let mut parts = parts_no_extension();

        let err = AuthenticatedHost::from_request_parts(&mut parts, &())
            .await
            .expect_err("Missing AuthContext must yield Unauthorized");
        match err.inner {
            G5Error::Unauthorized => {}
            other => panic!("Expected Unauthorized, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn authenticated_host_rejects_host_context_missing_worker_host_scope() {
        // Defensive case: AuthContext::Host populated without Scope::WorkerHost.
        // The routing layer never does this, but a future bug might — the
        // extractor must fail closed.
        let host_id = Uuid::new_v4();
        let org_id = Uuid::new_v4();
        let mut parts = parts_with_extension(AuthContext::Host {
            host_id,
            org_id,
            scopes: vec![], // intentionally empty
        });

        let err = AuthenticatedHost::from_request_parts(&mut parts, &())
            .await
            .expect_err("Host context without Scope::WorkerHost must be rejected");
        match err.inner {
            G5Error::Forbidden { missing_scopes } => {
                assert!(missing_scopes.contains(&Scope::WorkerHost.to_string()));
            }
            other => panic!("Expected Forbidden, got {:?}", other),
        }
    }

    #[test]
    fn authenticated_user_accessors_are_consistent() {
        let claims = JwtClaims {
            sub: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            org: "660e8400-e29b-41d4-a716-446655440000".to_string(),
            scopes: vec![Scope::WorkspacesRead, Scope::WorkspacesWrite],
            iat: 0,
            exp: 0,
            iss: None,
            aud: None,
            jti: None,
            token_use: None,
            client: None,
        };

        let user = AuthenticatedUser {
            user_id: UserId::from_uuid("550e8400-e29b-41d4-a716-446655440000".parse().unwrap()),
            org_id: OrgId::from_uuid("660e8400-e29b-41d4-a716-446655440000".parse().unwrap()),
            claims,
        };

        assert_eq!(
            user.user_uuid(),
            "550e8400-e29b-41d4-a716-446655440000"
                .parse::<Uuid>()
                .unwrap()
        );
        assert_eq!(
            user.org_uuid(),
            "660e8400-e29b-41d4-a716-446655440000"
                .parse::<Uuid>()
                .unwrap()
        );
        assert!(user.has_scope(&Scope::WorkspacesRead));
        assert!(!user.has_scope(&Scope::BillingAdmin));
    }
}
