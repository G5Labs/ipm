//! Platform-path preview guard. Previews are served only from isolated
//! preview hosts (`g5.security.mvp-exposure.preview`); on the platform
//! origin the `/api/v1/preview` namespace only redirects to a preview host,
//! refuses, or reports that no preview origin is configured. Host
//! translation marks isolated preview requests before this stack runs; a
//! client header cannot opt out.

use axum::{
    extract::{OriginalUri, Request},
    middleware::Next,
    response::{IntoResponse, Response},
};
use http::{header, HeaderValue, StatusCode};

use super::PreviewHostOrigin;

pub(super) async fn platform_preview_path_guard(req: Request, next: Next) -> Response {
    let isolated = req.extensions().get::<PreviewHostOrigin>().is_some();
    let path = req
        .extensions()
        .get::<OriginalUri>()
        .map(|uri| uri.0.path())
        .unwrap_or_else(|| req.uri().path());
    // Cover the namespace rather than parsing the raw UUID: Axum decodes
    // path parameters later, and encoded IDs must not evade this boundary.
    let platform_path =
        !isolated && (path == "/api/v1/preview" || path.starts_with("/api/v1/preview/"));
    if !platform_path {
        return next.run(req).await;
    }

    // A sibling preview is same-site, but must never drive the platform-path
    // redirect or the re-auth flow with the owner's cookies. Missing metadata
    // permits non-browser clients; unknown, duplicate or malformed values
    // fail closed.
    let mut sites = req.headers().get_all("sec-fetch-site").iter();
    let allowed = match sites.next() {
        None => true,
        Some(value) => {
            matches!(value.to_str(), Ok("same-origin" | "none")) && sites.next().is_none()
        }
    };
    let mut response = if allowed {
        next.run(req).await
    } else {
        (
            StatusCode::FORBIDDEN,
            "Cross-origin platform-path preview requests are forbidden",
        )
            .into_response()
    };
    // Redirects carry single-use handoff tickets and refusals carry nothing
    // worth keeping: never let a browser or intermediary cache them.
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, routing::any, Router};
    use tower::ServiceExt;
    use uuid::Uuid;

    fn router() -> Router {
        Router::new()
            .route("/api/v1/me", any(|| async { "platform" }))
            .nest(
                "/api/v1/preview",
                Router::new().route(
                    "/:task/*path",
                    any(|| async {
                        (
                            StatusCode::FOUND,
                            [
                                ("location", "https://task.preview.g5.example/index.html"),
                                ("cache-control", "public, max-age=600"),
                            ],
                            "",
                        )
                    }),
                ),
            )
            .layer(axum::middleware::from_fn(platform_preview_path_guard))
    }

    #[tokio::test]
    async fn platform_path_rejects_sibling_site_and_cross_site_before_handler() {
        for method in ["GET", "POST"] {
            for site in [
                "same-site",
                "cross-site",
                "unknown",
                "same-origin, same-site",
            ] {
                let response = router()
                    .oneshot(
                        Request::builder()
                            .method(method)
                            .uri(format!("/api/v1/preview/{}/index.html", Uuid::new_v4()))
                            .header("sec-fetch-site", site)
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(response.status(), StatusCode::FORBIDDEN, "{method} {site}");
                assert_eq!(
                    response.headers()[header::CACHE_CONTROL],
                    "private, no-store"
                );
            }
        }
        let response = router()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/preview/{}/index.html", Uuid::new_v4()))
                    .header("sec-fetch-site", "same-origin")
                    .header("sec-fetch-site", "same-site")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn platform_path_allows_direct_and_same_origin_navigation_without_caching() {
        for site in [Some("none"), Some("same-origin"), None] {
            let mut request =
                Request::builder().uri(format!("/api/v1/preview/{}/index.html", Uuid::new_v4()));
            if let Some(site) = site {
                request = request.header("sec-fetch-site", site);
            }
            let response = router()
                .oneshot(request.body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::FOUND, "{site:?}");
            assert_eq!(
                response.headers()[header::CACHE_CONTROL],
                "private, no-store"
            );
        }
    }

    #[tokio::test]
    async fn encoded_ids_stay_inside_the_guarded_namespace() {
        let response = router()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/preview/%61aaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa/index.html")
                    .header("sec-fetch-site", "cross-site")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn translated_preview_and_platform_api_are_untouched() {
        let workspace_id = Uuid::new_v4();
        let response = router()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/preview/{workspace_id}/login"))
                    .header("sec-fetch-site", "same-site")
                    .extension(PreviewHostOrigin {
                        workspace_id,
                        host: format!("{workspace_id}.preview.g5.example"),
                    })
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FOUND);
        assert_eq!(
            response.headers()[header::CACHE_CONTROL],
            "public, max-age=600"
        );

        let response = router()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/me")
                    .header("sec-fetch-site", "cross-site")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(!response.headers().contains_key(header::CACHE_CONTROL));
    }
}
