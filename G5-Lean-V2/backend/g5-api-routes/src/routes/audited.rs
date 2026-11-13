//! Audited route builder — compile-time enforcement of scope requirements.
//!
//! Provides [`AuditedRouter`], a wrapper around [`axum::Router`] that makes
//! it **impossible** to register a handler without specifying required scopes.
//!
//! # Problem
//!
//! Using raw `Router::new().route("/foo", get(handler))`, it's easy to forget
//! `.layer(RequireScopes::new(...))`. The code compiles, tests may pass, and
//! the endpoint is silently unprotected.
//!
//! # Solution
//!
//! `AuditedRouter` has no `route()` method. Instead, it exposes
//! [`audited_route()`] and [`exempt_route()`]:
//!
//! ```ignore
//! AuditedRouter::new()
//!     // Compile error if you don't specify scopes:
//!     .audited_route("/", get(list_workspaces), &[Scope::WorkspacesRead])
//!     .audited_route("/", post(create_workspace), &[Scope::WorkspacesWrite])
//!     // Explicitly marking a route as exempt (health check, OAuth):
//!     .exempt_route("/health", get(health_check), "load balancer probe")
//!     .into_router()
//! ```
//!
//! The `exempt_route()` method requires a `reason` string — forcing the
//! developer to document WHY the route is unauthenticated.

use std::sync::Arc;

use axum::routing::MethodRouter;
use axum::Router;

use g5_auth::Scope;

use crate::middleware::RequireScopes;
use crate::services::AppState;

// ---------------------------------------------------------------------------
// AuditedRouter
// ---------------------------------------------------------------------------

/// A route builder that enforces scope requirements at the API level.
///
/// Every route MUST be registered through either [`audited_route()`]
/// (which attaches `RequireScopes`) or [`exempt_route()`] (which requires
/// an explicit justification string). There is no way to add an unguarded
/// route without being explicit about it.
///
/// # Usage
///
/// ```ignore
/// pub fn router() -> Router<Arc<AppState>> {
///     AuditedRouter::new()
///         .audited_route("/:id", get(get_workspace), &[Scope::WorkspacesRead])
///         .audited_route("/:id", patch(update_workspace), &[Scope::WorkspacesWrite])
///         .into_router()
/// }
/// ```
pub struct AuditedRouter {
    inner: Router<Arc<AppState>>,
    /// Track registered routes for test-time introspection.
    routes: Vec<RouteRegistration>,
}

/// Record of a registered route for audit/test purposes.
#[derive(Debug, Clone)]
pub struct RouteRegistration {
    pub path: String,
    pub scopes: Vec<Scope>,
    pub exempt: bool,
    pub exempt_reason: Option<String>,
}

impl AuditedRouter {
    /// Create a new empty audited router.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Router::new(),
            routes: Vec::new(),
        }
    }

    /// Register a route with mandatory scope requirements.
    ///
    /// The handler will be wrapped in `RequireScopes::new(scopes)`.
    /// This is the primary route registration method — use it for all
    /// authenticated endpoints.
    #[must_use]
    pub fn audited_route(
        mut self,
        path: &str,
        method_router: MethodRouter<Arc<AppState>>,
        scopes: &'static [Scope],
    ) -> Self {
        self.routes.push(RouteRegistration {
            path: path.to_string(),
            scopes: scopes.to_vec(),
            exempt: false,
            exempt_reason: None,
        });
        self.inner = self
            .inner
            .route(path, method_router.layer(RequireScopes::new(scopes)));
        self
    }

    /// Register a route that is explicitly exempt from scope checking.
    ///
    /// Requires a `reason` string documenting WHY this route is exempt.
    /// This forces developers to think about and justify unauthenticated
    /// access — there's no "just forget to add scopes" path.
    ///
    /// # Examples of valid reasons
    ///
    /// - `"OAuth login initiation — no token exists yet"`
    /// - `"Health check — consumed by load balancer, no user context"`
    /// - `"Blob read — auth via Ed25519 capability token, not JWT"`
    #[must_use]
    pub fn exempt_route(
        mut self,
        path: &str,
        method_router: MethodRouter<Arc<AppState>>,
        reason: &str,
    ) -> Self {
        self.routes.push(RouteRegistration {
            path: path.to_string(),
            scopes: vec![],
            exempt: true,
            exempt_reason: Some(reason.to_string()),
        });
        self.inner = self.inner.route(path, method_router);
        self
    }

    /// Apply a layer to ALL routes in the router.
    #[must_use]
    pub fn route_layer<L>(mut self, layer: L) -> Self
    where
        L: tower::Layer<axum::routing::Route> + Clone + Send + 'static,
        L::Service: tower::Service<
                axum::http::Request<axum::body::Body>,
                Response = axum::response::Response,
                Error = std::convert::Infallible,
            > + Clone
            + Send
            + 'static,
        <L::Service as tower::Service<axum::http::Request<axum::body::Body>>>::Future:
            Send + 'static,
    {
        self.inner = self.inner.route_layer(layer);
        self
    }

    /// Merge another `AuditedRouter` into this one.
    #[must_use]
    pub fn merge(mut self, other: AuditedRouter) -> Self {
        self.inner = self.inner.merge(other.inner);
        self.routes.extend(other.routes);
        self
    }

    /// Consume the builder and return the underlying axum `Router`.
    #[must_use = "the Router should be mounted or served"]
    pub fn into_router(self) -> Router<Arc<AppState>> {
        self.inner
    }

    /// Returns the audit trail — all registered routes and their scope
    /// requirements. Useful in tests to verify coverage.
    #[must_use]
    pub fn registrations(&self) -> &[RouteRegistration] {
        &self.routes
    }
}

impl Default for AuditedRouter {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audited_router_tracks_registrations() {
        let router = AuditedRouter::new()
            .audited_route(
                "/test",
                axum::routing::get(|| async { "ok" }),
                &[Scope::WorkspacesRead],
            )
            .exempt_route(
                "/health",
                axum::routing::get(|| async { "ok" }),
                "load balancer probe",
            );

        assert_eq!(router.registrations().len(), 2);

        // First route: audited
        assert!(!router.registrations()[0].exempt);
        assert_eq!(
            router.registrations()[0].scopes,
            vec![Scope::WorkspacesRead]
        );

        // Second route: exempt with reason
        assert!(router.registrations()[1].exempt);
        assert_eq!(
            router.registrations()[1].exempt_reason.as_deref(),
            Some("load balancer probe")
        );
    }

    #[test]
    fn no_unaudited_route_method_exists() {
        // This is a compile-time guarantee: AuditedRouter has no `route()`
        // method. Any attempt to call `.route()` on it will fail to compile.
        // This test documents the intent.
        //
        // If someone adds a `route()` method to AuditedRouter, this comment
        // should trigger a review conversation about why.
        let _router = AuditedRouter::new();
        // _router.route("/foo", get(handler)); // ← would not compile
    }
}
