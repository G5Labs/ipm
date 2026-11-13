//! Request ID middleware for the g5-api HTTP processing pipeline.
//!
//! This is the **outermost** Tower middleware layer in the stack:
//! Request ID → CORS → Compression → Rate Limit → Auth → Handler.
//!
//! On every incoming HTTP request, this middleware:
//! 1. Generates a unique ID in the format `req_<nanoid>` (21-char nanoid suffix).
//! 2. Stores the ID in axum request extensions as [`RequestId`].
//! 3. Creates a `tracing::info_span!("request", request_id = %id)` that covers
//!    the entire async request/response lifecycle.
//! 4. Sets the `x-request-id` response header on the outgoing response.
//!
//! Being outermost ensures that ALL subsequent middleware and handlers can
//! reference the request ID in logs and error responses — including errors
//! from the CORS, rate limit, and auth layers.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use axum::body::Body;
use http::{HeaderValue, Request, Response};
use tower::{Layer, Service};
use tracing::Instrument;

// ---------------------------------------------------------------------------
// RequestId newtype
// ---------------------------------------------------------------------------

/// Newtype wrapper for the request ID string.
///
/// Stored in axum request extensions for downstream extraction by route
/// handlers and other middleware. The inner `String` is `pub` so that
/// downstream code can access the raw ID string directly (e.g., `req_id.0`).
///
/// # Examples
///
/// Extracting in an axum handler:
/// ```ignore
/// use axum::Extension;
/// use crate::middleware::request_id::RequestId;
///
/// async fn handler(Extension(req_id): Extension<RequestId>) -> String {
///     format!("Your request ID is: {}", req_id.0)
/// }
/// ```
#[derive(Debug, Clone)]
pub struct RequestId(pub String);

// ---------------------------------------------------------------------------
// RequestIdLayer
// ---------------------------------------------------------------------------

/// Tower [`Layer`] that wraps services with request ID generation.
///
/// Takes no configuration — the ID format (`req_<nanoid>`) and generation
/// strategy are fixed. Apply this as the outermost layer in the middleware
/// stack so that all subsequent layers and handlers have access to the
/// request ID.
///
/// # Usage
///
/// ```ignore
/// use crate::middleware::request_id::RequestIdLayer;
///
/// let app = Router::new()
///     .route("/", get(handler))
///     .layer(RequestIdLayer);  // outermost — applied last, runs first
/// ```
#[derive(Debug, Clone)]
pub struct RequestIdLayer;

impl<S> Layer<S> for RequestIdLayer {
    type Service = RequestIdMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RequestIdMiddleware { inner }
    }
}

// ---------------------------------------------------------------------------
// RequestIdMiddleware
// ---------------------------------------------------------------------------

/// Tower [`Service`] that generates request IDs, injects them into request
/// extensions and response headers, and creates tracing spans.
///
/// This middleware:
/// - Generates a unique `req_<nanoid>` ID for each request.
/// - Inserts [`RequestId`] into request extensions for downstream access.
/// - Creates a `tracing::info_span!("request", request_id = %id)` that spans
///   the entire async request/response lifecycle.
/// - Sets the `x-request-id` response header on every response (success and
///   error alike).
#[derive(Debug, Clone)]
pub struct RequestIdMiddleware<S> {
    inner: S,
}

impl<S> Service<Request<Body>> for RequestIdMiddleware<S>
where
    S: Service<Request<Body>, Response = Response<Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: Request<Body>) -> Self::Future {
        // 1. Generate request ID: `req_` prefix + 21-character nanoid.
        //    Nanoid uses URL-safe characters (A-Za-z0-9_-), so no encoding
        //    is needed in headers. 21 chars ≈ 126 bits of entropy.
        let request_id = format!("req_{}", nanoid::nanoid!());

        // 2. Store in request extensions so downstream middleware and route
        //    handlers can extract it via `Extension<RequestId>`.
        req.extensions_mut().insert(RequestId(request_id.clone()));

        // 3. Create a tracing span at INFO level with the request_id field.
        //    This span covers the entire request/response lifecycle so that
        //    all log entries automatically include the request ID.
        let span = tracing::info_span!("request", request_id = %request_id);

        // 4. Clone inner service — required by Tower's Service contract when
        //    `call()` takes `&mut self`. The clone ensures each concurrent
        //    request gets its own service instance.
        let mut inner = self.inner.clone();

        // 5. Capture request_id for response header injection (after the
        //    inner service produces a response).
        let req_id_for_header = request_id;

        // 6. Call inner service within the span, then inject response header.
        let future = async move {
            let mut response = inner.call(req).await?;

            // Set x-request-id response header (lowercase for HTTP/2 compat).
            // Nanoid characters are always valid header value characters, so
            // HeaderValue::from_str will always succeed in practice.
            if let Ok(val) = HeaderValue::from_str(&req_id_for_header) {
                response.headers_mut().insert("x-request-id", val);
            }

            Ok(response)
        }
        .instrument(span);

        Box::pin(future)
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use http::{Request, Response, StatusCode};
    use tower::{ServiceBuilder, ServiceExt};

    /// Helper service that echoes back a 200 OK with an empty body.
    /// Used to test the middleware in isolation.
    #[derive(Clone)]
    struct EchoService;

    impl Service<Request<Body>> for EchoService {
        type Response = Response<Body>;
        type Error = std::convert::Infallible;
        type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, _req: Request<Body>) -> Self::Future {
            Box::pin(async {
                Ok(Response::builder()
                    .status(StatusCode::OK)
                    .body(Body::empty())
                    .unwrap())
            })
        }
    }

    /// Helper service that extracts the RequestId from extensions and returns
    /// it in the response body.
    #[derive(Clone)]
    struct ExtractIdService;

    impl Service<Request<Body>> for ExtractIdService {
        type Response = Response<Body>;
        type Error = std::convert::Infallible;
        type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, req: Request<Body>) -> Self::Future {
            let id = req
                .extensions()
                .get::<RequestId>()
                .map(|r| r.0.clone())
                .unwrap_or_else(|| "MISSING".to_string());

            Box::pin(async move {
                Ok(Response::builder()
                    .status(StatusCode::OK)
                    .body(Body::from(id))
                    .unwrap())
            })
        }
    }

    #[tokio::test]
    async fn test_request_id_format() {
        let svc = ServiceBuilder::new()
            .layer(RequestIdLayer)
            .service(EchoService);

        let req = Request::builder().uri("/test").body(Body::empty()).unwrap();

        let response = svc.oneshot(req).await.unwrap();

        // Verify x-request-id header is present
        let header = response
            .headers()
            .get("x-request-id")
            .expect("x-request-id header must be present");

        let id = header.to_str().unwrap();

        // Verify format: req_ prefix + 21-char nanoid
        assert!(
            id.starts_with("req_"),
            "ID must start with 'req_', got: {id}"
        );
        let suffix = &id[4..]; // strip "req_"
        assert_eq!(
            suffix.len(),
            21,
            "Nanoid suffix must be 21 chars, got {} chars: {suffix}",
            suffix.len()
        );

        // Verify URL-safe characters only (A-Za-z0-9_-)
        assert!(
            suffix
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
            "Nanoid must use URL-safe characters only, got: {suffix}"
        );
    }

    #[tokio::test]
    async fn test_unique_ids_per_request() {
        let layer = RequestIdLayer;

        // Make two requests and verify they get different IDs
        let svc1 = layer.layer(EchoService);
        let req1 = Request::builder().uri("/a").body(Body::empty()).unwrap();
        let resp1 = svc1.oneshot(req1).await.unwrap();
        let id1 = resp1
            .headers()
            .get("x-request-id")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();

        let svc2 = layer.layer(EchoService);
        let req2 = Request::builder().uri("/b").body(Body::empty()).unwrap();
        let resp2 = svc2.oneshot(req2).await.unwrap();
        let id2 = resp2
            .headers()
            .get("x-request-id")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();

        assert_ne!(id1, id2, "Each request must get a unique ID");
    }

    #[tokio::test]
    async fn test_request_id_in_extensions() {
        let svc = ServiceBuilder::new()
            .layer(RequestIdLayer)
            .service(ExtractIdService);

        let req = Request::builder().uri("/test").body(Body::empty()).unwrap();

        let response = svc.oneshot(req).await.unwrap();

        // The body contains the extracted request ID
        let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body_id = String::from_utf8(body_bytes.to_vec()).unwrap();

        assert!(
            body_id.starts_with("req_"),
            "RequestId must be available in extensions, got: {body_id}"
        );
        assert_ne!(body_id, "MISSING", "RequestId must be stored in extensions");
    }

    #[tokio::test]
    async fn test_header_set_on_success_response() {
        let svc = ServiceBuilder::new()
            .layer(RequestIdLayer)
            .service(EchoService);

        let req = Request::builder().uri("/ok").body(Body::empty()).unwrap();

        let response = svc.oneshot(req).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            response.headers().contains_key("x-request-id"),
            "x-request-id header must be set on success responses"
        );
    }

    #[tokio::test]
    async fn test_header_set_on_error_response() {
        /// Helper service that returns a 500 error.
        #[derive(Clone)]
        struct ErrorService;

        impl Service<Request<Body>> for ErrorService {
            type Response = Response<Body>;
            type Error = std::convert::Infallible;
            type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

            fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
                Poll::Ready(Ok(()))
            }

            fn call(&mut self, _req: Request<Body>) -> Self::Future {
                Box::pin(async {
                    Ok(Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(Body::empty())
                        .unwrap())
                })
            }
        }

        let svc = ServiceBuilder::new()
            .layer(RequestIdLayer)
            .service(ErrorService);

        let req = Request::builder().uri("/fail").body(Body::empty()).unwrap();

        let response = svc.oneshot(req).await.unwrap();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert!(
            response.headers().contains_key("x-request-id"),
            "x-request-id header must be set on error responses too"
        );
    }

    #[tokio::test]
    async fn test_header_and_extension_ids_match() {
        let svc = ServiceBuilder::new()
            .layer(RequestIdLayer)
            .service(ExtractIdService);

        let req = Request::builder().uri("/test").body(Body::empty()).unwrap();

        let response = svc.oneshot(req).await.unwrap();

        let header_id = response
            .headers()
            .get("x-request-id")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();

        let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let extension_id = String::from_utf8(body_bytes.to_vec()).unwrap();

        assert_eq!(
            header_id, extension_id,
            "Response header ID and extension ID must match"
        );
    }

    #[test]
    fn test_request_id_layer_is_clone() {
        let layer = RequestIdLayer;
        let _cloned = layer.clone();
    }

    #[test]
    fn test_request_id_layer_is_debug() {
        let layer = RequestIdLayer;
        let debug_str = format!("{:?}", layer);
        assert!(debug_str.contains("RequestIdLayer"));
    }

    #[test]
    fn test_request_id_newtype_clone_and_debug() {
        let id = RequestId("req_test123".to_string());
        let cloned = id.clone();
        assert_eq!(cloned.0, "req_test123");

        let debug_str = format!("{:?}", id);
        assert!(debug_str.contains("req_test123"));
    }

    #[test]
    fn test_request_id_pub_field_access() {
        let id = RequestId("req_abc".to_string());
        // The inner String is pub — verify direct access works
        let raw: &str = &id.0;
        assert_eq!(raw, "req_abc");
    }
}
