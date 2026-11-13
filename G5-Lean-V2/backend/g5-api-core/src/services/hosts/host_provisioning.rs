//! Shared Add Host preparation and cloud launch. Both manual provisioning and
//! demand capacity use this boundary; callers commit before launching cloud work.
use super::worker_host_service::{parse_promoted_base_images, ProvisionHostRecordParams};
use crate::cloud::bringup::{
    bring_up_cloud_resources, parse_vcpus_from_instance_type, BringUpCloudResourcesParams,
};
use crate::errors::AppError;
use crate::services::{AppState, WorkerHostService};
use g5_authz::ScopedDb;
use g5_contracts::error::G5Error;
use g5_contracts::types::worker_host::*;
use std::sync::Arc;
use uuid::Uuid;

impl WorkerHostService {
    pub async fn prepare_host(
        &self,
        state: &AppState,
        db: &mut ScopedDb,
        org_id: Uuid,
        created_by: Option<Uuid>,
        body: ProvisionWorkerHostRequest,
    ) -> Result<PreparedHost, AppError> {
        // Validate region (required, non-empty, allowlisted). The value is
        // interpolated into `format!`-built GCP REST URLs
        // (`regions/{region}/...`, `zones/{region}-{suffix}/...`), so off-list
        // values must be rejected at the boundary — same URL-path-injection vector
        // the `instance_type` allowlist closes for the machine-type segment, and
        // the same hardening B-1 cross-review (opus48) flagged for `region`.
        // Allowlist is the single source of truth in `g5_contracts`
        // ([WORKER_HOST_REGION_ALLOWLIST]); the per-region subnet CIDR table in
        // `cloud::gcp` keys off the same set (ontology [D18]).
        let region = body.region.trim();
        if region.is_empty() {
            return Err(AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!({
                    "field": "region",
                    "error": "region must be a non-empty cloud region",
                }),
            }));
        }
        if !WORKER_HOST_REGION_ALLOWLIST.contains(&region) {
            return Err(AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!({
                    "field": "region",
                    "error": "region is not in the supported region allowlist",
                    "value": region,
                    "allowed": WORKER_HOST_REGION_ALLOWLIST,
                }),
            }));
        }
        let region = region.to_string();

        // Apply the instance-type default when omitted or blank.
        let instance_type = body
            .instance_type
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(DEFAULT_WORKER_HOST_INSTANCE_TYPE)
            .to_string();

        // Allowlist the instance type against the rev-7 §5.2 capability matrix.
        // The handler interpolates this value into the GCP Compute path
        // (`zones/{zone}/machineTypes/{instance_type}`); rejecting unknown values
        // at the boundary closes both the cost vector (an admin can't ask for a
        // 96-core machine) and the path-injection vector (a value like
        // `n2-standard-8/global/networks/x` would otherwise reshape the URL).
        if !WORKER_HOST_INSTANCE_TYPE_ALLOWLIST
            .iter()
            .any(|allowed| *allowed == instance_type)
        {
            return Err(AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!({
                    "field": "instanceType",
                    "error": "instance_type is not in the supported capability matrix",
                    "value": instance_type,
                    "allowed": WORKER_HOST_INSTANCE_TYPE_ALLOWLIST,
                }),
            }));
        }

        // Validate the explicit slot limit before creation. The host-record
        // owner resolves and saves the default together with H, V and disk size.
        let instance_vcpus = parse_vcpus_from_instance_type(&instance_type).unwrap_or(4);
        if let Some(bad) = body
            .workspace_vm_limit
            .filter(|n| *n == 0 || *n > instance_vcpus)
        {
            return Err(AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!({
                    "field": "workspaceVmLimit",
                    "error": "workspaceVmLimit must be between 1 and the instance type's vCPU count",
                    "value": bad,
                    "min": 1,
                    "max": instance_vcpus,
                    "instanceType": instance_type,
                }),
            }));
        }
        let slot_cpus = DEFAULT_WORKER_HOST_SLOT_CPUS;
        let slot_mem_mib = DEFAULT_WORKER_HOST_SLOT_MEM_MIB;
        let requested_pin = body
            .base_image_ref
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(ToOwned::to_owned);
        let requested_track = body
            .image_track
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(ToOwned::to_owned);
        if requested_pin.is_some() && requested_track.is_some() {
            return Err(AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!({
                    "field": "imageSource",
                    "error": "imageTrack and baseImageRef are mutually exclusive",
                }),
            }));
        }
        if let Some(pin) = requested_pin.as_deref() {
            validate_base_image_ref(pin).map_err(|e| {
                AppError::from_error(G5Error::ValidationError {
                    details: serde_json::json!({
                        "field": "baseImageRef", "error": e.as_str(), "value": pin,
                    }),
                })
            })?;
        }

        // worker_profile: omitted/blank → fat-default; allowlist-gated.
        let worker_profile = body
            .worker_profile
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .unwrap_or(DEFAULT_WORKER_PROFILE)
            .to_string();
        if !WORKER_PROFILE_ALLOWLIST.contains(&worker_profile.as_str()) {
            return Err(AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!({
                    "field": "workerProfile",
                    "error": "worker_profile is not in the supported profile allowlist",
                    "value": worker_profile, "allowed": WORKER_PROFILE_ALLOWLIST,
                }),
            }));
        }

        // Resolve source policy completely before inserting the host or minting a
        // credential. A selected track requires the current validated catalog;
        // no source means a 422, never an implicit buildpack-deps fallback.
        let (org_track, org_pin) = state
            .worker_host_service
            .org_image_policy(&state.system_pool, org_id)
            .await?;
        let catalog = std::env::var("G5_PROMOTED_BASE_IMAGES")
            .ok()
            .map(|raw| parse_promoted_base_images(&raw))
            .unwrap_or_default();
        let selected_track = requested_track.or(org_track.filter(|_| requested_pin.is_none()));
        let (base_image_ref, base_image_digest, image_source_kind, image_source_value) =
            if let Some(pin) = requested_pin {
                let digest = validate_base_image_ref(&pin).map_err(|e| {
                    AppError::from_error(G5Error::ValidationError {
                        details: serde_json::json!({
                            "field": "baseImageRef", "error": e.as_str(), "value": pin,
                        }),
                    })
                })?;
                (pin.clone(), digest, "pin".to_string(), pin)
            } else if let Some(track) = selected_track {
                let image_ref = catalog
            .iter()
            .find_map(|(name, image_ref)| (name == &track).then(|| image_ref.clone()))
            .ok_or_else(|| {
                AppError::from_error(G5Error::ValidationError {
                    details: serde_json::json!({
                        "field": "imageTrack",
                        "error": "selected promoted image track is absent from the current catalog",
                        "value": track,
                    }),
                })
            })?;
                let digest = validate_base_image_ref(&image_ref).map_err(|e| {
                    AppError::from_error(G5Error::Internal {
                        message: format!("validated promoted catalog drifted: {}", e.as_str()),
                    })
                })?;
                (image_ref, digest, "track".to_string(), track)
            } else if let Some(pin) = org_pin {
                let digest = validate_base_image_ref(&pin).map_err(|e| {
                    AppError::from_error(G5Error::Internal {
                        message: format!("org image pin failed validation: {}", e.as_str()),
                    })
                })?;
                (pin.clone(), digest, "pin".to_string(), pin)
            } else {
                return Err(AppError::from_error(G5Error::ValidationError {
                    details: serde_json::json!({
                        "field": "imageSource",
                        "error": "select imageTrack or baseImageRef, or configure an org image policy",
                        "code": "worker_image_source_required",
                    }),
                }));
            };

        // A host with no credential is useless; fail closed if no minter is wired.
        let minter = state.jwt_minter.as_ref().ok_or_else(|| {
            AppError::from_error(G5Error::Internal {
                message: "JWT minter not configured; cannot mint host_jwt".into(),
            })
        })?;

        // host_id, status, vpc_id, cloud_instance_id, cache_state, attestation_state
        // all come from DB defaults (status='provisioning', the cloud columns NULL).
        let (host, allocation) = state
            .worker_host_service
            .provision_host_record(
                db,
                ProvisionHostRecordParams {
                    org_id,
                    region: &region,
                    instance_type: &instance_type,
                    base_image_ref: Some(base_image_ref.as_str()),
                    base_image_digest: Some(base_image_digest.as_str()),
                    base_image_source_kind: &image_source_kind,
                    base_image_source_value: &image_source_value,
                    worker_profile: &worker_profile,
                    slot_cpus,
                    slot_mem_mib,
                    // Explicit request value only — never the resolved default
                    // ([D54]; omitted ⇒ NULL ⇒ the host tracks the default).
                    workspace_vm_limit: body.workspace_vm_limit.map(|n| n as i32),
                    // Same rule for the disk: explicit override only, so an
                    // omitted size re-derives on every replacement
                    // (g5.runtime.placement.host-disk).
                    boot_disk_gb: body.boot_disk_gb.map(|gb| gb as i32),
                    created_by_user_id: created_by,
                },
            )
            .await?;

        // Mint inside the transaction so a (vanishingly rare) mint failure rolls
        // back the INSERT rather than orphaning a credential-less host record.
        // `state.host_jwt_ttl` is resolved once at AppState construction (see
        // `services::resolve_host_jwt_ttl`); the handler reads the cached value.
        let host_jwt = minter.mint_host_jwt(host.host_id, host.org_id, state.host_jwt_ttl)?;

        Ok(PreparedHost {
            host,
            host_jwt,
            allocation,
        })
    }
}

pub struct PreparedHost {
    pub host: WorkerHost,
    pub host_jwt: String,
    pub allocation: HostDiskAllocation,
}

impl PreparedHost {
    /// The caller must commit the host record first. Manual HTTP response cancellation
    /// cannot interrupt bring-up; durable automatic claims supply restart recovery.
    pub fn launch(&self, state: Arc<AppState>, actor: Uuid) -> Result<(), AppError> {
        match (state.cloud_features_enabled, state.cloud_provider.clone()) {
            (true, Some(cloud)) => {
                let host_id = self.host.host_id;
                let org_id = self.host.org_id;
                let instance_type = self.host.instance_type.clone();
                let region = self.host.region.clone();
                let host_jwt = self.host_jwt.clone();
                let base_image_ref = self.host.base_image_ref.clone();
                let scratch_disk_mib = self.host.scratch_disk_mib;
                let max_microvms = self.allocation.slots;
                let boot_disk_gb = self.allocation.disk_gib;
                let host_disk_gib = self.allocation.host_disk_gib;
                let _bringup = tokio::spawn(async move {
                    bring_up_cloud_resources(BringUpCloudResourcesParams {
                        state: &state, cloud: cloud.as_ref(), host_id, org_id,
                        instance_type: &instance_type, region: &region, host_jwt: &host_jwt,
                        base_image_ref: base_image_ref.as_deref(), created_by: actor,
                        max_microvms, scratch_disk_mib, boot_disk_gb, host_disk_gib,
                    }).await;
                });
            }
            (true, None) => return Err(AppError::from_error(G5Error::ServiceUnavailable {
                message: "cloud_features_enabled but no cloud provider configured (set GCP_PROJECT_ID + auth)".into(),
            })),
            (false, _) => tracing::info!(target: "g5_audit", event = "worker_host.cloud_bringup_skipped",
                host_id = %self.host.host_id, reason = "cloud_features_disabled"),
        }
        Ok(())
    }

    pub fn into_response(self) -> Result<ProvisionWorkerHostResponse, AppError> {
        let host = self.host;
        let missing = || {
            AppError::from_error(G5Error::Internal {
                message: "new worker host did not persist its image source".into(),
            })
        };
        Ok(ProvisionWorkerHostResponse {
            host_id: host.host_id,
            host_jwt: self.host_jwt,
            disk_allocation: self.allocation,
            status: host.status,
            region: host.region,
            instance_type: host.instance_type,
            created_at: host.created_at,
            base_image_ref: host.base_image_ref,
            base_image_digest: host.base_image_digest,
            base_image_source_kind: host.base_image_source_kind.ok_or_else(missing)?,
            base_image_source_value: host.base_image_source_value.ok_or_else(missing)?,
            runtime_schema: host.runtime_schema,
            runtime_labels: host.runtime_labels,
            runtime_labels_refreshed_at: host.runtime_labels_refreshed_at,
            worker_profile: host.worker_profile,
        })
    }
}
