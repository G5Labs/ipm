//! ECR (Amazon Elastic Container Registry, `*.dkr.ecr.*.amazonaws.com`)
//! credential provider (D24).
//!
//! ECR is the one registry that does NOT use the OCI Distribution bearer-token
//! flow: its `/v2/` API authenticates with an HTTP **Basic** credential
//! obtained from the SigV4-signed AWS API call `ecr:GetAuthorizationToken`.
//! `EcrCredentials` therefore sets `scheme() = Basic` and
//! `pre_authenticates() = true`, and the shared core mints + applies the Basic
//! credential on the first `/v2/` request (refreshing on a 401).
//!
//! The SigV4 signer is implemented by hand (`hmac` + `sha2` + `hex`, timestamp
//! via `chrono`) rather than pulling `aws-sigv4` / `aws-sdk-ecr`: the signed
//! worker binary is a rev-7 §5.0 keystone and the `GetAuthorizationToken` call
//! is the only AWS surface this module needs.

use std::time::Duration;

use async_trait::async_trait;
use base64::Engine as _;
use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::auth::{AuthChallenge, AuthScheme, CredentialProvider};
use super::error::OciError;
use super::registry::parse_ecr_region;

type HmacSha256 = Hmac<Sha256>;

/// AWS region service for ECR SigV4.
const ECR_SERVICE: &str = "ecr";
/// `GetAuthorizationToken` API target (X-Amz-Target).
const ECR_TARGET: &str = "AmazonEC2ContainerRegistry_V20150921.GetAuthorizationToken";
const AWS_JSON_CONTENT_TYPE: &str = "application/x-amz-json-1.1";
/// IMDSv2 base (D24 tier 3); overridable for tests.
const IMDS_BASE: &str = "http://169.254.169.254";
/// Bounded timeout for each IMDSv2 probe call. Off-EC2 hosts whose link-local
/// 169.254.169.254 is blackholed (Docker bridge networks, GCE, on-prem) would
/// otherwise stall every TCP connect for the OS default (~60-120s) under the
/// async refresh lock — wedging all concurrent ECR pulls.
const IMDS_TIMEOUT: Duration = Duration::from_secs(2);
/// Default Basic-token lifetime when the API omits `expiresAt` (ECR tokens
/// last ~12h).
const DEFAULT_TOKEN_TTL_SECS: u64 = 12 * 60 * 60;

/// Injected / resolved AWS credentials for the `GetAuthorizationToken` call.
#[derive(Clone, Debug)]
pub struct AwsCredentials {
    pub access_key_id: String,
    pub secret_access_key: String,
    /// STS session token (`AWS_SESSION_TOKEN` / IMDS / SSO); signed as
    /// `X-Amz-Security-Token` when present.
    pub session_token: Option<String>,
    /// Region override; when `None` the region is taken from the registry host
    /// (authoritative) and only then from `AWS_REGION`.
    pub region: Option<String>,
}

/// Internal resolved credential triple (no region — region is resolved
/// separately from the registry host).
struct ResolvedAws {
    access_key_id: String,
    secret_access_key: String,
    session_token: Option<String>,
}

/// ECR `GetAuthorizationToken` response (only the fields we consume).
#[derive(Deserialize)]
struct GetAuthTokenResponse {
    #[serde(rename = "authorizationData", default)]
    authorization_data: Vec<AuthorizationData>,
}

#[derive(Deserialize)]
struct AuthorizationData {
    #[serde(rename = "authorizationToken")]
    authorization_token: String,
    /// Epoch seconds (AWS serializes this as a JSON number, possibly
    /// fractional).
    #[serde(rename = "expiresAt", default)]
    expires_at: Option<f64>,
}

/// IMDSv2 instance-profile credential response.
#[derive(Deserialize)]
struct ImdsCredentials {
    #[serde(rename = "AccessKeyId")]
    access_key_id: String,
    #[serde(rename = "SecretAccessKey")]
    secret_access_key: String,
    #[serde(rename = "Token")]
    token: Option<String>,
}

/// ECR credential source (D24). One `EcrClient` serves every ECR
/// host/region; the region is parsed per-request from `registry_host`.
pub struct EcrCredentials {
    /// Tier 1: injected AWS credentials from the daemon config layer.
    injected: Option<AwsCredentials>,
    /// AWS API endpoint base override (tests). Production builds
    /// `https://ecr.<region>.amazonaws.com`.
    api_base: Option<String>,
    /// IMDS base override (tests).
    imds_base: String,
    /// Whether to consult ambient host sources (env vars, IMDS, shared
    /// credentials file). `false` under test so the suite never reads real AWS
    /// environment / instance metadata.
    ambient: bool,
}

impl EcrCredentials {
    /// Resolve AWS credentials from the runtime cascade (env → IMDS → shared
    /// file) at `mint` time.
    pub fn from_env() -> Self {
        Self {
            injected: None,
            api_base: None,
            imds_base: IMDS_BASE.to_string(),
            ambient: true,
        }
    }

    /// Inject AWS credentials (tier 1).
    pub fn with_credentials(creds: AwsCredentials) -> Self {
        Self {
            injected: Some(creds),
            api_base: None,
            imds_base: IMDS_BASE.to_string(),
            ambient: true,
        }
    }

    /// Test-only: overridable AWS-API + IMDS base URLs and ambient sources
    /// disabled so `wiremock` can stand in for `GetAuthorizationToken` without
    /// touching real AWS.
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn for_tests(
        creds: Option<AwsCredentials>,
        api_base: Option<String>,
        imds_base: Option<String>,
    ) -> Self {
        Self {
            injected: creds,
            api_base,
            imds_base: imds_base.unwrap_or_else(|| IMDS_BASE.to_string()),
            ambient: false,
        }
    }

    /// Region for the SigV4 signature: the registry host is authoritative
    /// (`<acct>.dkr.ecr.<region>.amazonaws.com`), then any injected
    /// `region`, then `AWS_REGION`.
    fn resolve_region(&self, registry_host: &str) -> Option<String> {
        if let Some(region) = parse_ecr_region(registry_host) {
            return Some(region.to_string());
        }
        if let Some(creds) = &self.injected {
            if let Some(region) = &creds.region {
                return Some(region.clone());
            }
        }
        std::env::var("AWS_REGION").ok().filter(|s| !s.is_empty())
    }

    /// Resolve the AWS credential triple (injected → env → IMDS → shared file),
    /// or `None` (anonymous, which private ECR rejects).
    async fn resolve_aws_creds(&self, http: &reqwest::Client) -> Option<ResolvedAws> {
        if let Some(c) = &self.injected {
            return Some(ResolvedAws {
                access_key_id: c.access_key_id.clone(),
                secret_access_key: c.secret_access_key.clone(),
                session_token: c.session_token.clone(),
            });
        }
        if self.ambient {
            if let Some(r) = creds_from_env() {
                return Some(r);
            }
            if let Some(r) = self.creds_from_imds(http).await {
                return Some(r);
            }
            if let Some(r) = creds_from_shared_file().await {
                return Some(r);
            }
        }
        None
    }

    /// Tier 3: IMDSv2 instance-profile credentials (EC2-hosted G5). Each leg
    /// of the three-call IMDSv2 dance is bounded by [`IMDS_TIMEOUT`] so an
    /// off-EC2 host with a blackholed link-local address cannot stall the
    /// refresh lock waiting on the OS TCP-connect timeout.
    async fn creds_from_imds(&self, http: &reqwest::Client) -> Option<ResolvedAws> {
        let base = self.imds_base.trim_end_matches('/');
        // IMDSv2: obtain a session token first.
        let token = http
            .put(format!("{base}/latest/api/token"))
            .header("X-aws-ec2-metadata-token-ttl-seconds", "21600")
            .timeout(IMDS_TIMEOUT)
            .send()
            .await
            .ok()?
            .text()
            .await
            .ok()?;
        let role = http
            .get(format!("{base}/latest/meta-data/iam/security-credentials/"))
            .header("X-aws-ec2-metadata-token", &token)
            .timeout(IMDS_TIMEOUT)
            .send()
            .await
            .ok()?;
        if !role.status().is_success() {
            return None;
        }
        let role = role.text().await.ok()?;
        let role = role.lines().next()?.trim();
        if role.is_empty() {
            return None;
        }
        let resp = http
            .get(format!(
                "{base}/latest/meta-data/iam/security-credentials/{role}"
            ))
            .header("X-aws-ec2-metadata-token", &token)
            .timeout(IMDS_TIMEOUT)
            .send()
            .await
            .ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let creds: ImdsCredentials = resp.json().await.ok()?;
        Some(ResolvedAws {
            access_key_id: creds.access_key_id,
            secret_access_key: creds.secret_access_key,
            session_token: creds.token,
        })
    }

    /// Call `ecr:GetAuthorizationToken` and return the base64 `AWS:<password>`
    /// token (applied verbatim as `Authorization: Basic <token>`) plus its TTL.
    async fn get_authorization_token(
        &self,
        http: &reqwest::Client,
        region: &str,
        resolved: &ResolvedAws,
    ) -> Result<(String, u64), OciError> {
        let base = self
            .api_base
            .clone()
            .unwrap_or_else(|| format!("https://ecr.{region}.amazonaws.com"));
        let url = format!("{}/", base.trim_end_matches('/'));
        // GetAuthorizationToken takes no required parameters.
        let payload = b"{}";

        let parsed = reqwest::Url::parse(&url).map_err(|e| {
            OciError::AuthFailed(ecr_label(region), format!("bad ECR API URL: {e}"))
        })?;
        let host = host_header(&parsed);

        let now = chrono::Utc::now();
        let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
        let date_stamp = now.format("%Y%m%d").to_string();

        let authorization = sigv4_authorization(
            resolved,
            region,
            ECR_SERVICE,
            "POST",
            &host,
            "/",
            AWS_JSON_CONTENT_TYPE,
            ECR_TARGET,
            payload,
            &amz_date,
            &date_stamp,
        );

        let mut req = http
            .post(&url)
            .header(reqwest::header::CONTENT_TYPE, AWS_JSON_CONTENT_TYPE)
            .header("X-Amz-Target", ECR_TARGET)
            .header("X-Amz-Date", &amz_date)
            .header(reqwest::header::AUTHORIZATION, authorization)
            .body(payload.to_vec());
        if let Some(tok) = &resolved.session_token {
            req = req.header("X-Amz-Security-Token", tok);
        }

        let resp = req.send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(OciError::AuthFailed(
                ecr_label(region),
                format!(
                    "GetAuthorizationToken returned {}: {}",
                    status,
                    super::error::truncate_body(&body)
                ),
            ));
        }

        let parsed: GetAuthTokenResponse = resp.json().await.map_err(|e| {
            OciError::AuthFailed(
                ecr_label(region),
                format!("GetAuthorizationToken response decode failed: {e}"),
            )
        })?;
        let data = parsed
            .authorization_data
            .into_iter()
            .next()
            .ok_or_else(|| {
                OciError::AuthFailed(
                    ecr_label(region),
                    "GetAuthorizationToken returned no authorizationData".to_string(),
                )
            })?;

        // Validate the token base64-decodes to a `user:password` pair (the
        // decode-error path). The base64 token itself is the Basic credential.
        validate_basic_token(&data.authorization_token, region)?;

        let expires_in = match data.expires_at {
            Some(epoch) => {
                let now_secs = now.timestamp() as f64;
                ((epoch - now_secs).max(60.0)) as u64
            }
            None => DEFAULT_TOKEN_TTL_SECS,
        };
        Ok((data.authorization_token, expires_in))
    }
}

#[async_trait]
impl CredentialProvider for EcrCredentials {
    fn registry(&self) -> &str {
        "ECR (*.dkr.ecr.*.amazonaws.com)"
    }

    fn scheme(&self) -> AuthScheme {
        AuthScheme::Basic
    }

    fn pre_authenticates(&self) -> bool {
        true
    }

    async fn mint(
        &self,
        http: &reqwest::Client,
        _challenge: Option<&AuthChallenge>,
        registry_host: &str,
        repository: &str,
        scope: &str,
    ) -> Result<(String, u64), OciError> {
        let region = self.resolve_region(registry_host).ok_or_else(|| {
            OciError::AuthFailed(
                self.registry().to_string(),
                format!("could not determine AWS region for ECR host '{registry_host}'"),
            )
        })?;
        let resolved =
            self.resolve_aws_creds(http)
                .await
                .ok_or_else(|| OciError::AuthRequired {
                    registry: registry_host.to_string(),
                    image: repository.to_string(),
                    scope: scope.to_string(),
                })?;
        self.get_authorization_token(http, &region, &resolved).await
    }

    fn on_denied(&self, registry_host: &str, repository: &str, scope: &str) -> OciError {
        OciError::AuthRequired {
            registry: registry_host.to_string(),
            image: repository.to_string(),
            scope: scope.to_string(),
        }
    }
}

/// Tier 2: static credentials from the standard AWS environment variables.
fn creds_from_env() -> Option<ResolvedAws> {
    let access_key_id = std::env::var("AWS_ACCESS_KEY_ID")
        .ok()
        .filter(|s| !s.is_empty())?;
    let secret_access_key = std::env::var("AWS_SECRET_ACCESS_KEY")
        .ok()
        .filter(|s| !s.is_empty())?;
    let session_token = std::env::var("AWS_SESSION_TOKEN")
        .ok()
        .filter(|s| !s.is_empty());
    Some(ResolvedAws {
        access_key_id,
        secret_access_key,
        session_token,
    })
}

/// Tier 4: the `[<profile>]` section of the shared credentials file
/// (`AWS_SHARED_CREDENTIALS_FILE` or `~/.aws/credentials`). Uses `tokio::fs`
/// because the call is reached from `mint`, which already runs under the
/// async refresh lock — a blocking `std::fs::read_to_string` here would
/// stall the reactor for the duration of the disk I/O.
async fn creds_from_shared_file() -> Option<ResolvedAws> {
    let path = std::env::var("AWS_SHARED_CREDENTIALS_FILE")
        .ok()
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| std::path::Path::new(&h).join(".aws/credentials"))
        })?;
    let contents = tokio::fs::read_to_string(path).await.ok()?;
    let profile = std::env::var("AWS_PROFILE").unwrap_or_else(|_| "default".to_string());
    parse_aws_ini(&contents, &profile)
}

/// Minimal INI parse of the shared credentials file: find `[<profile>]` and
/// read `aws_access_key_id` / `aws_secret_access_key` / optional
/// `aws_session_token` until the next section header.
fn parse_aws_ini(contents: &str, profile: &str) -> Option<ResolvedAws> {
    let mut in_section = false;
    let mut access_key_id: Option<String> = None;
    let mut secret_access_key: Option<String> = None;
    let mut session_token: Option<String> = None;
    for raw in contents.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            in_section = name.trim() == profile;
            continue;
        }
        if !in_section {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let (k, v) = (k.trim(), v.trim().to_string());
            match k {
                "aws_access_key_id" => access_key_id = Some(v),
                "aws_secret_access_key" => secret_access_key = Some(v),
                "aws_session_token" => session_token = Some(v),
                _ => {}
            }
        }
    }
    Some(ResolvedAws {
        access_key_id: access_key_id?,
        secret_access_key: secret_access_key?,
        session_token,
    })
}

/// Validate that an ECR `authorizationToken` base64-decodes to a
/// `<user>:<password>` pair. Returns the decoded password on success; maps a
/// malformed token to `AuthFailed` (the decode-error path, D25).
fn validate_basic_token(token: &str, region: &str) -> Result<(), OciError> {
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(token)
        .map_err(|e| {
            OciError::AuthFailed(
                ecr_label(region),
                format!("ECR authorizationToken is not valid base64: {e}"),
            )
        })?;
    let decoded = String::from_utf8(decoded).map_err(|_| {
        OciError::AuthFailed(
            ecr_label(region),
            "ECR authorizationToken did not base64-decode to UTF-8".to_string(),
        )
    })?;
    if !decoded.contains(':') {
        return Err(OciError::AuthFailed(
            ecr_label(region),
            "ECR authorizationToken did not decode to a 'user:password' pair".to_string(),
        ));
    }
    Ok(())
}

fn ecr_label(region: &str) -> String {
    format!("ecr.{region}.amazonaws.com")
}

/// The `Host` header value reqwest will send for `url` (host + non-default
/// port), which is what the SigV4 signature must cover.
fn host_header(url: &reqwest::Url) -> String {
    match (url.host_str(), url.port()) {
        (Some(h), Some(p)) => format!("{h}:{p}"),
        (Some(h), None) => h.to_string(),
        (None, _) => String::new(),
    }
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC-SHA256 accepts a key of any length");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

/// Derive the SigV4 signing key: `HMAC(HMAC(HMAC(HMAC("AWS4"+secret, date),
/// region), service), "aws4_request")`.
fn derive_signing_key(secret: &str, date_stamp: &str, region: &str, service: &str) -> Vec<u8> {
    let k_date = hmac_sha256(format!("AWS4{secret}").as_bytes(), date_stamp.as_bytes());
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, service.as_bytes());
    hmac_sha256(&k_service, b"aws4_request")
}

/// Build the `Authorization: AWS4-HMAC-SHA256 ...` header for the
/// `GetAuthorizationToken` POST. Signs the fixed header set (content-type,
/// host, x-amz-date, x-amz-target, and x-amz-security-token when present).
#[allow(clippy::too_many_arguments)]
fn sigv4_authorization(
    resolved: &ResolvedAws,
    region: &str,
    service: &str,
    method: &str,
    host: &str,
    canonical_uri: &str,
    content_type: &str,
    amz_target: &str,
    payload: &[u8],
    amz_date: &str,
    date_stamp: &str,
) -> String {
    let mut headers: Vec<(&str, &str)> = vec![
        ("content-type", content_type),
        ("host", host),
        ("x-amz-date", amz_date),
    ];
    if let Some(tok) = &resolved.session_token {
        headers.push(("x-amz-security-token", tok.as_str()));
    }
    headers.push(("x-amz-target", amz_target));
    headers.sort_by(|a, b| a.0.cmp(b.0));

    let canonical_headers: String = headers.iter().map(|(n, v)| format!("{n}:{v}\n")).collect();
    let signed_headers = headers
        .iter()
        .map(|(n, _)| *n)
        .collect::<Vec<_>>()
        .join(";");
    let payload_hash = sha256_hex(payload);

    // CanonicalQueryString is empty (the `\n\n` after the URI).
    let canonical_request = format!(
        "{method}\n{canonical_uri}\n\n{canonical_headers}\n{signed_headers}\n{payload_hash}"
    );

    let credential_scope = format!("{date_stamp}/{region}/{service}/aws4_request");
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{amz_date}\n{credential_scope}\n{}",
        sha256_hex(canonical_request.as_bytes())
    );

    let signing_key = derive_signing_key(&resolved.secret_access_key, date_stamp, region, service);
    let signature = hex::encode(hmac_sha256(&signing_key, string_to_sign.as_bytes()));

    format!(
        "AWS4-HMAC-SHA256 Credential={}/{credential_scope}, SignedHeaders={signed_headers}, Signature={signature}",
        resolved.access_key_id
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signing_key_matches_aws_published_vector() {
        // AWS Signature V4 documented "derive the signing key" example:
        // secret `wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY`, date 20120215,
        // region us-east-1, service iam → a fixed signing key. Guards the
        // hand-rolled HMAC chain against regression.
        let key = derive_signing_key(
            "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY",
            "20120215",
            "us-east-1",
            "iam",
        );
        assert_eq!(
            hex::encode(key),
            "f4780e2d9f65fa895f9c67b32ce1baf0b0d8a43505a000a1a9e090d414db404d"
        );
    }

    #[test]
    fn authorization_header_is_well_formed() {
        let resolved = ResolvedAws {
            access_key_id: "AKIDEXAMPLE".to_string(),
            secret_access_key: "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY".to_string(),
            session_token: None,
        };
        let auth = sigv4_authorization(
            &resolved,
            "us-east-1",
            ECR_SERVICE,
            "POST",
            "ecr.us-east-1.amazonaws.com",
            "/",
            AWS_JSON_CONTENT_TYPE,
            ECR_TARGET,
            b"{}",
            "20120215T000000Z",
            "20120215",
        );
        assert!(auth.starts_with(
            "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20120215/us-east-1/ecr/aws4_request"
        ));
        assert!(auth.contains("SignedHeaders=content-type;host;x-amz-date;x-amz-target"));
        assert!(auth.contains("Signature="));
    }

    #[test]
    fn session_token_is_added_to_signed_headers() {
        let resolved = ResolvedAws {
            access_key_id: "AKIDEXAMPLE".to_string(),
            secret_access_key: "secret".to_string(),
            session_token: Some("session-tok".to_string()),
        };
        let auth = sigv4_authorization(
            &resolved,
            "eu-west-1",
            ECR_SERVICE,
            "POST",
            "ecr.eu-west-1.amazonaws.com",
            "/",
            AWS_JSON_CONTENT_TYPE,
            ECR_TARGET,
            b"{}",
            "20240101T000000Z",
            "20240101",
        );
        assert!(auth.contains(
            "SignedHeaders=content-type;host;x-amz-date;x-amz-security-token;x-amz-target"
        ));
    }

    #[test]
    fn validate_basic_token_accepts_aws_pair() {
        let token = base64::engine::general_purpose::STANDARD.encode("AWS:hunter2");
        assert!(validate_basic_token(&token, "us-east-1").is_ok());
    }

    #[test]
    fn validate_basic_token_rejects_non_base64() {
        let err = validate_basic_token("!!!not base64!!!", "us-east-1").unwrap_err();
        assert!(matches!(err, OciError::AuthFailed(_, _)));
    }

    #[test]
    fn validate_basic_token_rejects_missing_colon() {
        let token = base64::engine::general_purpose::STANDARD.encode("no-colon-here");
        let err = validate_basic_token(&token, "us-east-1").unwrap_err();
        assert!(matches!(err, OciError::AuthFailed(_, _)));
        assert!(err.to_string().contains("user:password"));
    }

    #[test]
    fn parse_aws_ini_reads_named_profile() {
        let ini = "\
[default]
aws_access_key_id = DEFAULTKEY
aws_secret_access_key = defaultsecret

[work]
aws_access_key_id = WORKKEY
aws_secret_access_key = worksecret
aws_session_token = worktoken
";
        let work = parse_aws_ini(ini, "work").expect("work profile resolves");
        assert_eq!(work.access_key_id, "WORKKEY");
        assert_eq!(work.secret_access_key, "worksecret");
        assert_eq!(work.session_token.as_deref(), Some("worktoken"));

        let default = parse_aws_ini(ini, "default").expect("default profile resolves");
        assert_eq!(default.access_key_id, "DEFAULTKEY");
        assert!(default.session_token.is_none());

        assert!(parse_aws_ini(ini, "missing").is_none());
    }

    #[test]
    fn host_header_includes_nondefault_port() {
        let url = reqwest::Url::parse("http://127.0.0.1:8080/").unwrap();
        assert_eq!(host_header(&url), "127.0.0.1:8080");
        let url = reqwest::Url::parse("https://ecr.us-east-1.amazonaws.com/").unwrap();
        assert_eq!(host_header(&url), "ecr.us-east-1.amazonaws.com");
    }
}
