use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{DateTime, Utc};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

use crate::config::SelfHostedConfig;
use crate::errors::G5Error;

const DEFAULT_PRODUCT: &str = "g5-self-hosted";
const DEFAULT_VERSION: &str = "latest";
const DOCKER_ACCESS_TOKEN_USERNAME: &str = "oauth2accesstoken";
const LICENSE_KEY_PREFIX: &str = "g5_live_";
const LICENSE_KEY_RANDOM_BYTES: usize = 32;
const MAX_LICENSE_GENERATION_ATTEMPTS: usize = 5;

#[derive(Debug, Clone)]
pub struct MintedRegistryToken {
    pub access_token: String,
    pub expires_in: u64,
}

#[async_trait]
pub trait RegistryTokenIssuer: Send + Sync {
    async fn mint_access_token(
        &self,
        service_account_email: &str,
        lifetime: Duration,
    ) -> Result<MintedRegistryToken, G5Error>;
}

#[derive(Clone)]
pub struct SelfHostedLicenseService {
    db: PgPool,
    config: SelfHostedConfig,
    token_issuer: Option<Arc<dyn RegistryTokenIssuer>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct RegistryCoordinatesInput {
    pub location: Option<String>,
    pub project: Option<String>,
    pub repository: Option<String>,
    pub puller_service_account: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RegistryCoordinates {
    pub location: String,
    pub project: String,
    pub repository: String,
    pub puller_service_account: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AdminLicenseRequest {
    pub customer_id: String,
    pub customer_name: String,
    pub contact_email: String,
    #[serde(default = "default_product")]
    pub product: String,
    #[serde(default = "default_version")]
    pub version: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub requested_by: Option<String>,
    #[serde(default)]
    pub registry: RegistryCoordinatesInput,
}

#[derive(Debug, Clone, Serialize)]
pub struct AdminLicenseResponse {
    pub license_key: String,
    pub license_key_prefix: String,
    pub customer_id: String,
    pub customer_name: String,
    pub contact_email: String,
    pub product: String,
    pub version: String,
    pub registry: RegistryCoordinates,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RegistryTokenRequest {
    #[serde(default = "default_product")]
    pub product: String,
    #[serde(default = "default_version")]
    pub version: String,
    pub platform: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RegistryTokenResponse {
    pub registry: String,
    pub username: String,
    pub password: String,
    pub expires_in: u64,
    pub product: String,
    pub release: RegistryRelease,
}

#[derive(Debug, Clone, Serialize)]
pub struct RegistryRelease {
    pub version: String,
    pub image_refs: RegistryImageRefs,
}

#[derive(Debug, Clone, Serialize)]
pub struct RegistryImageRefs {
    pub api: String,
    pub edge: String,
    pub worker: String,
    pub migration: String,
}

#[derive(sqlx::FromRow)]
struct CreatedLicenseRow {
    license_key_prefix: String,
    customer_id: String,
    customer_name: String,
    contact_email: String,
    product: String,
    version: String,
    expires_at: Option<DateTime<Utc>>,
    registry_location: String,
    registry_project: String,
    registry_repository: String,
    registry_puller_service_account: String,
    created_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct LicenseRow {
    id: Uuid,
    customer_id: String,
    product: String,
    version: String,
    status: String,
    expires_at: Option<DateTime<Utc>>,
    registry_location: String,
    registry_project: String,
    registry_repository: String,
    registry_puller_service_account: String,
}

impl SelfHostedLicenseService {
    pub fn new_with_issuer(
        db: PgPool,
        config: SelfHostedConfig,
        token_issuer: Option<Arc<dyn RegistryTokenIssuer>>,
    ) -> Self {
        Self {
            db,
            config,
            token_issuer,
        }
    }

    pub async fn provision_license(
        &self,
        admin_token: &str,
        request: AdminLicenseRequest,
    ) -> Result<AdminLicenseResponse, G5Error> {
        self.validate_admin_token(admin_token)?;

        let customer_id = required_string(request.customer_id, "customer_id")?;
        let customer_name = required_string(request.customer_name, "customer_name")?;
        let contact_email = required_string(request.contact_email, "contact_email")?;
        let product = required_string(request.product, "product")?;
        let version = required_string(request.version, "version")?;
        let registry = self.resolve_registry(request.registry)?;
        let created_by = normalize_optional(request.requested_by);

        for attempt in 0..MAX_LICENSE_GENERATION_ATTEMPTS {
            let license_key = generate_license_key();
            let license_key_hash = sha256_hex(&license_key);
            let license_key_prefix = license_prefix(&license_key);

            let result = sqlx::query_as::<_, CreatedLicenseRow>(
                r#"
                INSERT INTO self_hosted_licenses (
                    license_key_hash,
                    license_key_prefix,
                    customer_id,
                    customer_name,
                    contact_email,
                    product,
                    version,
                    expires_at,
                    registry_location,
                    registry_project,
                    registry_repository,
                    registry_puller_service_account,
                    created_by
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
                RETURNING
                    license_key_prefix,
                    customer_id,
                    customer_name,
                    contact_email,
                    product,
                    version,
                    expires_at,
                    registry_location,
                    registry_project,
                    registry_repository,
                    registry_puller_service_account,
                    created_at
                "#,
            )
            .bind(&license_key_hash)
            .bind(&license_key_prefix)
            .bind(&customer_id)
            .bind(&customer_name)
            .bind(&contact_email)
            .bind(&product)
            .bind(&version)
            .bind(request.expires_at)
            .bind(&registry.location)
            .bind(&registry.project)
            .bind(&registry.repository)
            .bind(&registry.puller_service_account)
            .bind(&created_by)
            .fetch_one(&self.db)
            .await;

            match result {
                Ok(row) => {
                    return Ok(AdminLicenseResponse {
                        license_key,
                        license_key_prefix: row.license_key_prefix,
                        customer_id: row.customer_id,
                        customer_name: row.customer_name,
                        contact_email: row.contact_email,
                        product: row.product,
                        version: row.version,
                        registry: RegistryCoordinates {
                            location: row.registry_location,
                            project: row.registry_project,
                            repository: row.registry_repository,
                            puller_service_account: row.registry_puller_service_account,
                        },
                        created_at: row.created_at,
                        expires_at: row.expires_at,
                    });
                }
                Err(error)
                    if license_key_hash_collision(&error)
                        && attempt + 1 < MAX_LICENSE_GENERATION_ATTEMPTS =>
                {
                    continue;
                }
                Err(error) => return Err(G5Error::from(error)),
            }
        }

        Err(G5Error::Conflict {
            reason: "license key generation collision".to_string(),
        })
    }

    pub async fn issue_registry_token(
        &self,
        license_key: &str,
        request: RegistryTokenRequest,
    ) -> Result<RegistryTokenResponse, G5Error> {
        let license_key = required_borrowed_string(license_key, "license_key")?;
        let requested_product = required_string(request.product, "product")?;
        let requested_version = required_string(request.version, "version")?;
        let _platform = normalize_optional(request.platform);

        let license_key_hash = sha256_hex(license_key);
        let row = sqlx::query_as::<_, LicenseRow>(
            r#"
            SELECT
                id,
                customer_id,
                product,
                version,
                status,
                expires_at,
                registry_location,
                registry_project,
                registry_repository,
                registry_puller_service_account
            FROM self_hosted_licenses
            WHERE license_key_hash = $1
            "#,
        )
        .bind(&license_key_hash)
        .fetch_optional(&self.db)
        .await?
        .ok_or(G5Error::Unauthorized)?;

        if row.product != requested_product {
            return Err(G5Error::Forbidden {
                missing_scopes: vec![format!("self_hosted_product:{}", requested_product)],
            });
        }
        if row.status != "active" {
            return Err(G5Error::Forbidden {
                missing_scopes: vec!["self_hosted_license_active".to_string()],
            });
        }
        if row
            .expires_at
            .map(|expires_at| expires_at <= Utc::now())
            .unwrap_or(false)
        {
            return Err(G5Error::Forbidden {
                missing_scopes: vec!["self_hosted_license_unexpired".to_string()],
            });
        }

        let release_version = resolve_release_version(&row.version, &requested_version)?;
        let token_issuer =
            self.token_issuer
                .as_ref()
                .ok_or_else(|| G5Error::ServiceUnavailable {
                    message: "self-hosted registry token issuer is not configured".to_string(),
                })?;
        let minted = token_issuer
            .mint_access_token(
                &row.registry_puller_service_account,
                self.config.registry_token_lifetime,
            )
            .await?;

        sqlx::query(
            r#"
            UPDATE self_hosted_licenses
            SET last_issued_at = now(),
                updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(row.id)
        .execute(&self.db)
        .await?;

        tracing::info!(
            customer_id = %row.customer_id,
            product = %row.product,
            release_version = %release_version,
            registry_project = %row.registry_project,
            registry_repository = %row.registry_repository,
            "Issued self-hosted registry token"
        );

        let registry = format!("{}-docker.pkg.dev", row.registry_location);
        let image_refs = image_refs_for_release(
            &registry,
            &row.registry_project,
            &row.registry_repository,
            &release_version,
        );

        Ok(RegistryTokenResponse {
            registry,
            username: DOCKER_ACCESS_TOKEN_USERNAME.to_string(),
            password: minted.access_token,
            expires_in: minted.expires_in,
            product: row.product,
            release: RegistryRelease {
                version: manifest_version(&release_version),
                image_refs,
            },
        })
    }

    fn validate_admin_token(&self, presented_token: &str) -> Result<(), G5Error> {
        let expected_hash = self
            .config
            .license_admin_token_sha256
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| G5Error::ServiceUnavailable {
                message: "license admin token hash is not configured".to_string(),
            })?;

        if expected_hash.len() != 64 || !expected_hash.chars().all(|ch| ch.is_ascii_hexdigit()) {
            return Err(G5Error::ServiceUnavailable {
                message: "license admin token hash is malformed".to_string(),
            });
        }

        let actual_hash = sha256_hex(presented_token);
        if !constant_time_eq(actual_hash.as_bytes(), expected_hash.as_bytes()) {
            return Err(G5Error::Unauthorized);
        }

        Ok(())
    }

    fn resolve_registry(
        &self,
        registry: RegistryCoordinatesInput,
    ) -> Result<RegistryCoordinates, G5Error> {
        Ok(RegistryCoordinates {
            location: first_nonempty(
                registry.location,
                Some(self.config.registry_location.clone()),
            )
            .ok_or_else(|| required_field("registry.location"))?,
            project: first_nonempty(registry.project, self.config.registry_project.clone())
                .ok_or_else(|| required_field("registry.project"))?,
            repository: first_nonempty(
                registry.repository,
                Some(self.config.registry_repository.clone()),
            )
            .ok_or_else(|| required_field("registry.repository"))?,
            puller_service_account: first_nonempty(
                registry.puller_service_account,
                self.config.registry_puller_service_account.clone(),
            )
            .ok_or_else(|| required_field("registry.puller_service_account"))?,
        })
    }
}

pub fn sha256_hex(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    hex::encode(digest)
}

fn default_product() -> String {
    DEFAULT_PRODUCT.to_string()
}

fn default_version() -> String {
    DEFAULT_VERSION.to_string()
}

fn generate_license_key() -> String {
    let mut random = [0u8; LICENSE_KEY_RANDOM_BYTES];
    OsRng.fill_bytes(&mut random);
    format!("{LICENSE_KEY_PREFIX}{}", URL_SAFE_NO_PAD.encode(random))
}

fn license_prefix(license_key: &str) -> String {
    license_key.chars().take(16).collect()
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|raw| raw.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn first_nonempty(primary: Option<String>, fallback: Option<String>) -> Option<String> {
    normalize_optional(primary).or_else(|| normalize_optional(fallback))
}

fn required_string(value: String, field: &str) -> Result<String, G5Error> {
    required_borrowed_string(&value, field).map(ToOwned::to_owned)
}

fn required_borrowed_string<'a>(value: &'a str, field: &str) -> Result<&'a str, G5Error> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err(required_field(field))
    } else {
        Ok(trimmed)
    }
}

fn required_field(field: &str) -> G5Error {
    G5Error::ValidationError {
        details: json!({ "field": field, "error": "required" }),
    }
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right.iter())
        .fold(0u8, |acc, (left, right)| acc | (left ^ right))
        == 0
}

fn license_key_hash_collision(error: &sqlx::Error) -> bool {
    match error {
        sqlx::Error::Database(database_error) => database_error
            .constraint()
            .map(|constraint| constraint == "self_hosted_licenses_license_key_hash_key")
            .unwrap_or(false),
        _ => false,
    }
}

fn resolve_release_version(
    entitled_version: &str,
    requested_version: &str,
) -> Result<String, G5Error> {
    let requested = requested_version.trim();
    let entitled = entitled_version.trim();
    let resolved = if requested == DEFAULT_VERSION {
        entitled
    } else {
        requested
    };

    if entitled != DEFAULT_VERSION && resolved != entitled {
        return Err(G5Error::Forbidden {
            missing_scopes: vec![format!("self_hosted_version:{resolved}")],
        });
    }

    Ok(resolved.to_string())
}

fn manifest_version(version: &str) -> String {
    if version == DEFAULT_VERSION {
        DEFAULT_VERSION.to_string()
    } else {
        version.trim_start_matches('v').to_string()
    }
}

fn tag_for_version(version: &str) -> String {
    if version == DEFAULT_VERSION || version.starts_with('v') {
        version.to_string()
    } else {
        format!("v{version}")
    }
}

fn image_refs_for_release(
    registry: &str,
    project: &str,
    repository: &str,
    version: &str,
) -> RegistryImageRefs {
    let tag = tag_for_version(version);
    let base = format!("{registry}/{project}/{repository}");
    let api = format!("{base}/api:{tag}");
    RegistryImageRefs {
        api: api.clone(),
        edge: format!("{base}/edge:{tag}"),
        worker: format!("{base}/g5d:{tag}"),
        migration: api,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_refs_use_release_semver_tags() {
        let refs = image_refs_for_release("us-central1-docker.pkg.dev", "g5-dev", "g5", "1.0.7");

        assert_eq!(refs.api, "us-central1-docker.pkg.dev/g5-dev/g5/api:v1.0.7");
        assert_eq!(
            refs.edge,
            "us-central1-docker.pkg.dev/g5-dev/g5/edge:v1.0.7"
        );
        assert_eq!(
            refs.worker,
            "us-central1-docker.pkg.dev/g5-dev/g5/g5d:v1.0.7"
        );
        assert_eq!(refs.migration, refs.api);
    }

    #[test]
    fn latest_image_refs_stay_latest() {
        let refs = image_refs_for_release("us-central1-docker.pkg.dev", "g5-dev", "g5", "latest");

        assert_eq!(refs.api, "us-central1-docker.pkg.dev/g5-dev/g5/api:latest");
        assert_eq!(
            refs.worker,
            "us-central1-docker.pkg.dev/g5-dev/g5/g5d:latest"
        );
    }

    #[test]
    fn version_entitlement_allows_specific_request_only_when_compatible() {
        assert_eq!(resolve_release_version("latest", "1.0.7").unwrap(), "1.0.7");
        assert_eq!(resolve_release_version("1.0.7", "latest").unwrap(), "1.0.7");
        assert_eq!(resolve_release_version("1.0.7", "1.0.7").unwrap(), "1.0.7");
        assert!(matches!(
            resolve_release_version("1.0.7", "1.0.8"),
            Err(G5Error::Forbidden { .. })
        ));
    }

    #[test]
    fn admin_token_hash_compare_uses_sha256_hex() {
        assert_eq!(
            sha256_hex("admin-secret"),
            "16175223c8ddce5ace0493c948569c211b03c4c6bb3d3e484434999448cffe01"
        );
    }
}
