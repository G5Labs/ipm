//! Authentication primitives for G5.
//!
//! Provides shared JWT validation, timing-safe comparison, and SCM
//! credential scrubbing — consumed by both `g5-api` and `g5d`.
//!
//! - [`JwtClaims`] — parsed JWT payload struct
//! - [`JwtValidator`] — ES256 (ECDSA P-256) token validation
//! - [`has_scopes`] — utility function for scope subset checking
//! - [`constant_time_eq`] — timing-safe byte comparison
//! - [`credential_scrub::scrub`] — removes SCM token material from
//!   free-text strings before persistence (see
//!   `auth_identity_access.security_view.md` D16)

pub use g5_contracts::error;

pub use g5_contracts::credential_scrub;
mod deployment_keys;
mod jwt;
mod minter;
mod scope;
mod timing;

pub use deployment_keys::validate_deployment_keys;
pub use jwt::{
    has_scopes, preview_agent_audience, preview_ticket_audience, websocket_ticket_audience,
    HostSourceClaims, JwtClaims, JwtValidator, PreviewAgentClaims, WorkspaceIncarnationClaims,
    API_KEY_PREFIX, CAPABILITY_TOKEN_USE, HOST_SOURCE_AUDIENCE, HOST_SOURCE_TOKEN_USE,
    HOST_SOURCE_TTL_SECS, PREVIEW_AGENT_HEADER, PREVIEW_AGENT_TOKEN_USE, PREVIEW_AGENT_TTL_SECS,
    PREVIEW_HANDOFF_TOKEN_USE, PREVIEW_TOKEN_USE, SESSION_CLIENT_BROWSER, SESSION_CLIENT_CLI,
    WEBSOCKET_TOKEN_USE, WORKSPACE_INCARNATION_AUDIENCE, WORKSPACE_INCARNATION_TOKEN_USE,
};
pub use minter::JwtMinter;
pub use scope::Scope;
pub use timing::constant_time_eq;
