use tower_http::compression::{
    predicate::{DefaultPredicate, NotForContentType, Predicate},
    CompressionLayer,
};

#[derive(Clone)]
pub struct GitAwareCompressionPredicate {
    inner: tower_http::compression::predicate::And<DefaultPredicate, NotForContentType>,
}

impl GitAwareCompressionPredicate {
    fn new() -> Self {
        Self {
            inner: DefaultPredicate::new().and(NotForContentType::const_new("application/x-git-")),
        }
    }
}

impl Predicate for GitAwareCompressionPredicate {
    fn should_compress<B>(&self, response: &http::Response<B>) -> bool
    where
        B: axum::body::HttpBody,
    {
        self.inner.should_compress(response)
    }
}

/// Builds the response compression layer for the g5-api middleware stack.
///
/// Returns a [`CompressionLayer`] configured with gzip, brotli, and deflate
/// support. The layer automatically selects the best encoding based on
/// the client's `Accept-Encoding` header.
///
/// # Compression Algorithms
///
/// - **gzip** — The most widely supported compression encoding.
/// - **brotli** — Modern algorithm with better compression ratios than gzip.
/// - **deflate** — Legacy encoding for compatibility with older clients.
///
/// The layer reads the `Accept-Encoding` request header and selects the best
/// encoding based on client preference (quality values). If no quality values
/// are specified, `tower-http` uses its internal preference order (typically
/// brotli > gzip > deflate based on compression ratio).
///
/// # Usage
///
/// Applied as part of the middleware stack in `mod.rs` at position 3
/// (after CORS, before Rate Limit):
///
/// ```rust,no_run,ignore
/// router.layer(build_compression_layer())
/// ```
#[must_use]
pub fn build_compression_layer() -> CompressionLayer<GitAwareCompressionPredicate> {
    CompressionLayer::new().compress_when(GitAwareCompressionPredicate::new())
}

#[cfg(test)]
mod tests {
    use axum::http::{header::CONTENT_TYPE, Response};
    use tower_http::compression::predicate::Predicate;

    use super::*;

    #[test]
    fn predicate_skips_git_smart_http_payloads() {
        let predicate = GitAwareCompressionPredicate::new();
        let response = Response::builder()
            .header(CONTENT_TYPE, "application/x-git-upload-pack-result")
            .body(axum::body::Body::from("git-payload"))
            .expect("response should build");

        assert!(!predicate.should_compress(&response));
    }

    #[test]
    fn predicate_still_compresses_normal_text_payloads() {
        let predicate = GitAwareCompressionPredicate::new();
        let response = Response::builder()
            .header(CONTENT_TYPE, "text/plain")
            .body(axum::body::Body::from(
                "this body is intentionally long enough to exceed the size predicate",
            ))
            .expect("response should build");

        assert!(predicate.should_compress(&response));
    }
}
