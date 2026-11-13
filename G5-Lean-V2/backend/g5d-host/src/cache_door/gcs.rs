//! Read-only access to the organization's fleet sccache prefix in Google
//! Cloud Storage, with the host's own identity (the GCE metadata token).
//! Guests never see this credential: the door reads on their behalf and only
//! within the configured prefix (`g5.runtime.cache.fleet`).

use std::time::{Duration, Instant};

use tokio::sync::Mutex;

pub const METADATA_TOKEN_URL: &str =
    "http://169.254.169.254/computeMetadata/v1/instance/service-accounts/default/token";
pub const STORAGE_BASE_URL: &str = "https://storage.googleapis.com";

pub struct GcsReader {
    http: reqwest::Client,
    bucket: String,
    prefix: String,
    storage_base: String,
    metadata_url: String,
    token: Mutex<Option<(String, Instant)>>,
}

#[derive(serde::Deserialize)]
struct MetadataToken {
    access_token: String,
    #[serde(default)]
    expires_in: u64,
}

impl GcsReader {
    pub fn new(bucket: &str, prefix: &str) -> Self {
        Self::with_endpoints(bucket, prefix, STORAGE_BASE_URL, METADATA_TOKEN_URL)
    }

    /// Endpoints are injectable so tests can stand a fake object store and a
    /// fake metadata server behind the same code path.
    pub fn with_endpoints(
        bucket: &str,
        prefix: &str,
        storage_base: &str,
        metadata_url: &str,
    ) -> Self {
        Self {
            http: reqwest::Client::builder()
                .user_agent("g5d-cache-door/0.1.0")
                .timeout(Duration::from_secs(120))
                .build()
                .expect("reqwest client builds with default config"),
            bucket: bucket.to_string(),
            prefix: prefix.trim_matches('/').to_string(),
            storage_base: storage_base.trim_end_matches('/').to_string(),
            metadata_url: metadata_url.to_string(),
            token: Mutex::new(None),
        }
    }

    pub fn location(&self) -> String {
        format!("gs://{}/{}", self.bucket, self.prefix)
    }

    async fn token(&self) -> Result<String, String> {
        let mut slot = self.token.lock().await;
        if let Some((token, until)) = slot.as_ref() {
            if Instant::now() < *until {
                return Ok(token.clone());
            }
        }
        let resp = self
            .http
            .get(&self.metadata_url)
            .header("Metadata-Flavor", "Google")
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .map_err(|e| format!("metadata token request failed: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("metadata token request returned {}", resp.status()));
        }
        let token: MetadataToken = resp
            .json()
            .await
            .map_err(|e| format!("metadata token body undecodable: {e}"))?;
        // Cache the token for its lifetime minus a five-minute margin, never
        // longer: the metadata server may hand out a cached token with only
        // minutes left, and such a token is used once and not kept.
        let ttl = token.expires_in.saturating_sub(300);
        *slot = if ttl > 0 {
            Some((
                token.access_token.clone(),
                Instant::now() + Duration::from_secs(ttl),
            ))
        } else {
            None
        };
        Ok(token.access_token)
    }

    fn object_url(&self, key: &str) -> String {
        let name = if self.prefix.is_empty() {
            key.to_string()
        } else {
            format!("{}/{}", self.prefix, key)
        };
        format!(
            "{}/storage/v1/b/{}/o/{}?alt=media",
            self.storage_base,
            self.bucket,
            percent_encode(&name)
        )
    }

    /// Open `key` under the prefix for streaming. `Ok(None)` is a plain miss.
    pub async fn get(&self, key: &str) -> Result<Option<reqwest::Response>, String> {
        let token = self.token().await?;
        let resp = self
            .http
            .get(self.object_url(key))
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| format!("object read failed: {e}"))?;
        match resp.status().as_u16() {
            200 => Ok(Some(resp)),
            404 => Ok(None),
            status => Err(format!("object read returned {status}")),
        }
    }
}

/// RFC 3986 percent-encoding of an object name as one path segment: `/`
/// inside the name must be encoded, or the JSON API reads it as a path.
fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len() * 3);
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_names_are_encoded_as_one_segment() {
        let reader =
            GcsReader::with_endpoints("b", "/guest-sccache/orgs/x/", "https://s", "http://m");
        assert_eq!(
            reader.object_url("a/b/c/abc"),
            "https://s/storage/v1/b/b/o/guest-sccache%2Forgs%2Fx%2Fa%2Fb%2Fc%2Fabc?alt=media"
        );
        assert_eq!(percent_encode("ok-._~"), "ok-._~");
        assert_eq!(percent_encode("a b/c"), "a%20b%2Fc");
    }
}
