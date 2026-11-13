//! Organization admin settings (`g5.data.configuration.org-admin-settings`).
//!
//! The settings an organization's administrators edit on the Admin page are
//! registered org keys in the typed `configs` store, validated again by
//! `g5_config_value_is_registered`. A missing key means the platform default.
//! Callers hold the administrator authority (`g5.security.org-admin-settings`)
//! before reading or writing through this module.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::models::{
    DEFAULT_IDLE_HOST_RETIRE_SECS, DEFAULT_IDLE_TIMEOUT_SECS, DEFAULT_MAX_RUNNING_VMS,
    DEFAULT_MAX_RUNNING_VMS_PER_OWNER, DEFAULT_MAX_WORKSPACES_PER_OWNER, DEFAULT_MIN_FREE_SLOTS,
    IDLE_HOST_RETIRE_SECS_RANGE, IDLE_TIMEOUT_SECS_RANGE, MAX_RUNNING_VMS_PER_OWNER_RANGE,
    MAX_RUNNING_VMS_RANGE, MAX_WORKSPACES_PER_OWNER_RANGE, MIN_FREE_SLOTS_RANGE,
};
use g5_contracts::error::G5Error;
use g5_contracts::types::worker_host::{
    default_workspace_vm_limit, parse_vcpus_from_instance_type, WorkerHostBootDiskSizing,
    DEFAULT_WORKER_HOST_INSTANCE_TYPE, MAX_WORKER_HOST_BOOT_DISK_GB, MIN_HOST_DISK_ALLOWANCE_GIB,
    MIN_VM_DISK_ALLOWANCE_GIB, WORKER_HOST_INSTANCE_TYPE_ALLOWLIST,
};

/// Idle timeout every new workspace starts with (`g5.lifecycle.idle.settings`).
pub const DEFAULT_IDLE_TIMEOUT_KEY: &str = "default_idle_timeout_secs";
/// Idle timeout every new personal workspace starts with
/// (`g5.personal.capacity`).
pub const DEFAULT_PERSONAL_IDLE_TIMEOUT_KEY: &str = "default_personal_idle_timeout_secs";
/// Platform default for [`DEFAULT_PERSONAL_IDLE_TIMEOUT_KEY`]: 30 minutes.
pub const DEFAULT_PERSONAL_IDLE_TIMEOUT_SECS: i32 = 1800;
/// Most starting, running and stopping VMs the org may hold
/// (`g5.runtime.placement.capacity`).
pub const MAX_RUNNING_VMS_KEY: &str = "max_running_vms";
/// Most VMs and earlier waiting Starts one owner may hold
/// (`g5.runtime.placement.owner-limit`).
pub const MAX_RUNNING_VMS_PER_OWNER_KEY: &str = "max_running_vms_per_owner";
/// Most live workspaces, pending forks and transfers one owner may hold
/// (`g5.workspace.owner-limit`).
pub const MAX_WORKSPACES_PER_OWNER_KEY: &str = "max_workspaces_per_owner";
/// Whether managed capacity adds hosts for waiting Starts
/// (`g5.architecture.host_demand_capacity`).
pub const AUTO_ADD_HOSTS_KEY: &str = "auto_add_hosts";
/// Machine type of hosts managed capacity adds.
pub const AUTO_HOST_INSTANCE_TYPE_KEY: &str = "auto_host_instance_type";
/// Whether managed capacity removes empty hosts
/// (`g5.architecture.host_idle_retirement`).
pub const AUTO_RETIRE_IDLE_HOSTS_KEY: &str = "auto_retire_idle_hosts";
/// How long a host stays empty before removal.
pub const IDLE_HOST_RETIRE_SECS_KEY: &str = "idle_host_retire_secs";
/// Free current slots managed capacity keeps ready in the demand region
/// (`g5.architecture.host_demand_capacity`); removal never goes below it.
pub const MIN_FREE_SLOTS_KEY: &str = "min_free_slots";
/// Whether a workspace whose chat turn a host drain or a lost VM interrupted
/// is started again for its owner (`g5.lifecycle.start.automatic-continuation`).
pub const AUTOMATIC_CONTINUATION_KEY: &str = "automatic_continuation_enabled";
pub const DEFAULT_AUTOMATIC_CONTINUATION: bool = true;

pub const HOST_DISK_ALLOWANCE_KEY: &str = "host_disk_allowance_gib";
pub const VM_DISK_ALLOWANCE_KEY: &str = "vm_disk_allowance_gib";

const ALL_KEYS: [&str; 13] = [
    DEFAULT_IDLE_TIMEOUT_KEY,
    DEFAULT_PERSONAL_IDLE_TIMEOUT_KEY,
    MAX_RUNNING_VMS_KEY,
    MAX_RUNNING_VMS_PER_OWNER_KEY,
    MAX_WORKSPACES_PER_OWNER_KEY,
    AUTO_ADD_HOSTS_KEY,
    AUTO_HOST_INSTANCE_TYPE_KEY,
    AUTO_RETIRE_IDLE_HOSTS_KEY,
    IDLE_HOST_RETIRE_SECS_KEY,
    MIN_FREE_SLOTS_KEY,
    AUTOMATIC_CONTINUATION_KEY,
    HOST_DISK_ALLOWANCE_KEY,
    VM_DISK_ALLOWANCE_KEY,
];

/// Managed capacity's per-organization settings, read once per capacity
/// sweep under the admission lock so the sweep decides under one snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedCapacitySettings {
    pub add_hosts: bool,
    pub instance_type: String,
    pub retire_idle_hosts: bool,
    pub idle_host_retire_secs: i32,
    pub min_free_slots: i32,
}

impl Default for ManagedCapacitySettings {
    fn default() -> Self {
        Self {
            add_hosts: true,
            instance_type: DEFAULT_WORKER_HOST_INSTANCE_TYPE.to_string(),
            retire_idle_hosts: true,
            idle_host_retire_secs: DEFAULT_IDLE_HOST_RETIRE_SECS,
            min_free_slots: DEFAULT_MIN_FREE_SLOTS,
        }
    }
}

/// Effective values: the stored setting, or the platform default when unset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrgAdminSettings {
    pub host_disk_allowance_gib: u32,
    pub vm_disk_allowance_gib: u32,
    pub default_idle_timeout_secs: i32,
    pub default_personal_idle_timeout_secs: i32,
    pub max_running_vms: i64,
    pub max_running_vms_per_owner: i64,
    pub max_workspaces_per_owner: i64,
    pub auto_add_hosts: bool,
    pub auto_host_instance_type: String,
    pub auto_retire_idle_hosts: bool,
    pub idle_host_retire_secs: i32,
    pub min_free_slots: i32,
    pub automatic_continuation_enabled: bool,
}

/// A partial update; absent fields keep their current value.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateOrgAdminSettings {
    pub host_disk_allowance_gib: Option<i64>,
    pub vm_disk_allowance_gib: Option<i64>,
    pub default_idle_timeout_secs: Option<i64>,
    pub default_personal_idle_timeout_secs: Option<i64>,
    pub max_running_vms: Option<i64>,
    pub max_running_vms_per_owner: Option<i64>,
    pub max_workspaces_per_owner: Option<i64>,
    pub auto_add_hosts: Option<bool>,
    pub auto_host_instance_type: Option<String>,
    pub auto_retire_idle_hosts: Option<bool>,
    pub idle_host_retire_secs: Option<i64>,
    pub min_free_slots: Option<i64>,
    pub automatic_continuation_enabled: Option<bool>,
}

async fn read_values(
    conn: &mut PgConnection,
    org_id: Uuid,
    keys: &[&str],
) -> Result<HashMap<String, Value>, G5Error> {
    let rows = sqlx::query_as::<_, (String, Value)>(
        "SELECT key, value
         FROM configs
         WHERE org_id = $1 AND scope = 'org' AND scope_id = $1 AND key = ANY($2)",
    )
    .bind(org_id)
    .bind(keys)
    .fetch_all(conn)
    .await
    .map_err(|e| G5Error::Internal {
        message: format!("Failed to read org settings: {e}"),
    })?;
    Ok(rows.into_iter().collect())
}

fn int_or(values: &HashMap<String, Value>, key: &str, default: i64) -> i64 {
    values.get(key).and_then(Value::as_i64).unwrap_or(default)
}

fn bool_or(values: &HashMap<String, Value>, key: &str, default: bool) -> bool {
    values.get(key).and_then(Value::as_bool).unwrap_or(default)
}

fn managed_capacity_from(values: &HashMap<String, Value>) -> ManagedCapacitySettings {
    let defaults = ManagedCapacitySettings::default();
    ManagedCapacitySettings {
        add_hosts: bool_or(values, AUTO_ADD_HOSTS_KEY, defaults.add_hosts),
        instance_type: values
            .get(AUTO_HOST_INSTANCE_TYPE_KEY)
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or(defaults.instance_type),
        retire_idle_hosts: bool_or(
            values,
            AUTO_RETIRE_IDLE_HOSTS_KEY,
            defaults.retire_idle_hosts,
        ),
        idle_host_retire_secs: i32::try_from(int_or(
            values,
            IDLE_HOST_RETIRE_SECS_KEY,
            i64::from(defaults.idle_host_retire_secs),
        ))
        .unwrap_or(defaults.idle_host_retire_secs),
        min_free_slots: i32::try_from(int_or(
            values,
            MIN_FREE_SLOTS_KEY,
            i64::from(defaults.min_free_slots),
        ))
        .unwrap_or(defaults.min_free_slots),
    }
}

fn disk_policy_from(values: &HashMap<String, Value>) -> WorkerHostBootDiskSizing {
    let defaults = WorkerHostBootDiskSizing::default();
    WorkerHostBootDiskSizing::new(
        int_or(values, HOST_DISK_ALLOWANCE_KEY, i64::from(defaults.base_gb)) as u32,
        int_or(
            values,
            VM_DISK_ALLOWANCE_KEY,
            i64::from(defaults.per_slot_gb),
        ) as u32,
    )
}

/// A single read observes H and V together. Creation holds lock_org_vm_quota,
/// as do settings writers, until its complete host snapshot is committed.
pub async fn disk_policy(
    conn: &mut PgConnection,
    org_id: Uuid,
) -> Result<WorkerHostBootDiskSizing, G5Error> {
    Ok(disk_policy_from(
        &read_values(
            conn,
            org_id,
            &[HOST_DISK_ALLOWANCE_KEY, VM_DISK_ALLOWANCE_KEY],
        )
        .await?,
    ))
}

/// The idle timeout a personal workspace created now in `org_id` starts with.
/// Read it in the transaction that inserts the workspace's settings row.
pub async fn default_personal_idle_timeout_secs(
    conn: &mut PgConnection,
    org_id: Uuid,
) -> Result<i32, G5Error> {
    let values = read_values(conn, org_id, &[DEFAULT_PERSONAL_IDLE_TIMEOUT_KEY]).await?;
    Ok(i32::try_from(int_or(
        &values,
        DEFAULT_PERSONAL_IDLE_TIMEOUT_KEY,
        i64::from(DEFAULT_PERSONAL_IDLE_TIMEOUT_SECS),
    ))
    .unwrap_or(DEFAULT_PERSONAL_IDLE_TIMEOUT_SECS))
}

/// The idle timeout a workspace created now in `org_id` starts with. Read it
/// in the transaction that inserts the workspace's settings row.
pub async fn default_idle_timeout_secs(
    conn: &mut PgConnection,
    org_id: Uuid,
) -> Result<i32, G5Error> {
    let values = read_values(conn, org_id, &[DEFAULT_IDLE_TIMEOUT_KEY]).await?;
    Ok(i32::try_from(int_or(
        &values,
        DEFAULT_IDLE_TIMEOUT_KEY,
        i64::from(DEFAULT_IDLE_TIMEOUT_SECS),
    ))
    .unwrap_or(DEFAULT_IDLE_TIMEOUT_SECS))
}

/// The per-owner VM limit (`g5.runtime.placement.owner-limit`). Placement
/// reads it after [`lock_org_vm_quota`], which every writer also takes.
pub async fn max_running_vms_per_owner(
    conn: &mut PgConnection,
    org_id: Uuid,
) -> Result<i64, G5Error> {
    let values = read_values(conn, org_id, &[MAX_RUNNING_VMS_PER_OWNER_KEY]).await?;
    Ok(int_or(
        &values,
        MAX_RUNNING_VMS_PER_OWNER_KEY,
        DEFAULT_MAX_RUNNING_VMS_PER_OWNER,
    ))
}

/// The per-owner workspace limit (`g5.workspace.owner-limit`).
pub async fn max_workspaces_per_owner(
    conn: &mut PgConnection,
    org_id: Uuid,
) -> Result<i64, G5Error> {
    let values = read_values(conn, org_id, &[MAX_WORKSPACES_PER_OWNER_KEY]).await?;
    Ok(int_or(
        &values,
        MAX_WORKSPACES_PER_OWNER_KEY,
        DEFAULT_MAX_WORKSPACES_PER_OWNER,
    ))
}

/// The org's managed-capacity settings. Capacity sweeps read them after
/// [`lock_org_vm_quota`], which every writer of these keys also takes.
pub async fn managed_capacity(
    conn: &mut PgConnection,
    org_id: Uuid,
) -> Result<ManagedCapacitySettings, G5Error> {
    let values = read_values(
        conn,
        org_id,
        &[
            AUTO_ADD_HOSTS_KEY,
            AUTO_HOST_INSTANCE_TYPE_KEY,
            AUTO_RETIRE_IDLE_HOSTS_KEY,
            IDLE_HOST_RETIRE_SECS_KEY,
            MIN_FREE_SLOTS_KEY,
        ],
    )
    .await?;
    Ok(managed_capacity_from(&values))
}

/// Serialize VM quota and capacity decisions for one org. Placement and
/// capacity sweeps take this lock before reading the quota, occupancy or the
/// capacity settings, and a writer of those settings takes it before writing,
/// so they are strictly ordered even when no settings row exists yet.
pub async fn lock_org_vm_quota(conn: &mut PgConnection, org_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1::text, 0))")
        .bind(org_id.to_string())
        .execute(conn)
        .await
        .map(|_| ())
}

pub async fn read(conn: &mut PgConnection, org_id: Uuid) -> Result<OrgAdminSettings, G5Error> {
    let values = read_values(conn, org_id, &ALL_KEYS).await?;
    let capacity = managed_capacity_from(&values);
    let disk = disk_policy_from(&values);
    Ok(OrgAdminSettings {
        host_disk_allowance_gib: disk.base_gb,
        vm_disk_allowance_gib: disk.per_slot_gb,
        default_idle_timeout_secs: i32::try_from(int_or(
            &values,
            DEFAULT_IDLE_TIMEOUT_KEY,
            i64::from(DEFAULT_IDLE_TIMEOUT_SECS),
        ))
        .unwrap_or(DEFAULT_IDLE_TIMEOUT_SECS),
        default_personal_idle_timeout_secs: i32::try_from(int_or(
            &values,
            DEFAULT_PERSONAL_IDLE_TIMEOUT_KEY,
            i64::from(DEFAULT_PERSONAL_IDLE_TIMEOUT_SECS),
        ))
        .unwrap_or(DEFAULT_PERSONAL_IDLE_TIMEOUT_SECS),
        max_running_vms: int_or(&values, MAX_RUNNING_VMS_KEY, DEFAULT_MAX_RUNNING_VMS),
        max_running_vms_per_owner: int_or(
            &values,
            MAX_RUNNING_VMS_PER_OWNER_KEY,
            DEFAULT_MAX_RUNNING_VMS_PER_OWNER,
        ),
        max_workspaces_per_owner: int_or(
            &values,
            MAX_WORKSPACES_PER_OWNER_KEY,
            DEFAULT_MAX_WORKSPACES_PER_OWNER,
        ),
        auto_add_hosts: capacity.add_hosts,
        auto_host_instance_type: capacity.instance_type,
        auto_retire_idle_hosts: capacity.retire_idle_hosts,
        idle_host_retire_secs: capacity.idle_host_retire_secs,
        min_free_slots: capacity.min_free_slots,
        automatic_continuation_enabled: bool_or(
            &values,
            AUTOMATIC_CONTINUATION_KEY,
            DEFAULT_AUTOMATIC_CONTINUATION,
        ),
    })
}

fn range_error(range: &std::ops::RangeInclusive<i64>) -> Value {
    format!("Must be between {} and {}", range.start(), range.end()).into()
}

fn validate(req: &UpdateOrgAdminSettings) -> Result<(), G5Error> {
    let mut errors = serde_json::Map::new();
    for (field, value, min) in [
        (
            "hostDiskAllowanceGib",
            req.host_disk_allowance_gib,
            MIN_HOST_DISK_ALLOWANCE_GIB,
        ),
        (
            "vmDiskAllowanceGib",
            req.vm_disk_allowance_gib,
            MIN_VM_DISK_ALLOWANCE_GIB,
        ),
    ] {
        if value.is_some_and(|v| {
            !(i64::from(min)..=i64::from(MAX_WORKER_HOST_BOOT_DISK_GB)).contains(&v)
        }) {
            errors.insert(
                field.into(),
                range_error(&(i64::from(min)..=i64::from(MAX_WORKER_HOST_BOOT_DISK_GB))),
            );
        }
    }
    if let Some(value) = req.default_idle_timeout_secs {
        if !IDLE_TIMEOUT_SECS_RANGE.contains(&value) {
            errors.insert(
                "defaultIdleTimeoutSecs".into(),
                range_error(&IDLE_TIMEOUT_SECS_RANGE),
            );
        }
    }
    if let Some(value) = req.default_personal_idle_timeout_secs {
        if !IDLE_TIMEOUT_SECS_RANGE.contains(&value) {
            errors.insert(
                "defaultPersonalIdleTimeoutSecs".into(),
                range_error(&IDLE_TIMEOUT_SECS_RANGE),
            );
        }
    }
    if let Some(value) = req.max_running_vms {
        if !MAX_RUNNING_VMS_RANGE.contains(&value) {
            errors.insert("maxRunningVms".into(), range_error(&MAX_RUNNING_VMS_RANGE));
        }
    }
    if let Some(value) = req.max_running_vms_per_owner {
        if !MAX_RUNNING_VMS_PER_OWNER_RANGE.contains(&value) {
            errors.insert(
                "maxRunningVmsPerOwner".into(),
                range_error(&MAX_RUNNING_VMS_PER_OWNER_RANGE),
            );
        }
    }
    if let Some(value) = req.max_workspaces_per_owner {
        if !MAX_WORKSPACES_PER_OWNER_RANGE.contains(&value) {
            errors.insert(
                "maxWorkspacesPerOwner".into(),
                range_error(&MAX_WORKSPACES_PER_OWNER_RANGE),
            );
        }
    }
    if let Some(value) = &req.auto_host_instance_type {
        // The same allowlist Add Host enforces: the value reaches the cloud
        // API path and decides host cost.
        if !WORKER_HOST_INSTANCE_TYPE_ALLOWLIST.contains(&value.as_str()) {
            errors.insert(
                "autoHostInstanceType".into(),
                format!(
                    "Must be one of {}",
                    WORKER_HOST_INSTANCE_TYPE_ALLOWLIST.join(", ")
                )
                .into(),
            );
        }
    }
    if let Some(value) = req.idle_host_retire_secs {
        if !IDLE_HOST_RETIRE_SECS_RANGE.contains(&value) {
            errors.insert(
                "idleHostRetireSecs".into(),
                range_error(&IDLE_HOST_RETIRE_SECS_RANGE),
            );
        }
    }
    if let Some(value) = req.min_free_slots {
        if !MIN_FREE_SLOTS_RANGE.contains(&value) {
            errors.insert("minFreeSlots".into(), range_error(&MIN_FREE_SLOTS_RANGE));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(G5Error::ValidationError {
            details: Value::Object(errors),
        })
    }
}

async fn write_key(
    conn: &mut PgConnection,
    org_id: Uuid,
    actor_id: Uuid,
    key: &str,
    value: Value,
) -> Result<(), G5Error> {
    sqlx::query(
        "INSERT INTO configs (org_id, scope, scope_id, key, value, updated_by, updated_at)
         VALUES ($1, 'org', $1, $2, $3, $4, now())
         ON CONFLICT (org_id, scope, scope_id, key)
         DO UPDATE SET value = EXCLUDED.value,
                       updated_by = EXCLUDED.updated_by,
                       updated_at = now()",
    )
    .bind(org_id)
    .bind(key)
    .bind(value)
    .bind(actor_id)
    .execute(conn)
    .await
    .map(|_| ())
    .map_err(|e| G5Error::Internal {
        message: format!("Failed to write org setting {key}: {e}"),
    })
}

/// Validate the whole request, then write the fields present. Existing
/// workspaces and running VMs are unaffected: a new idle default reaches only
/// workspaces created afterwards, a new quota or owner limit only later
/// placements and admissions, and new capacity settings only later capacity
/// sweeps.
pub async fn update(
    conn: &mut PgConnection,
    org_id: Uuid,
    actor_id: Uuid,
    req: &UpdateOrgAdminSettings,
) -> Result<OrgAdminSettings, G5Error> {
    validate(req)?;
    let writes: Vec<(&str, Value)> = [
        (
            HOST_DISK_ALLOWANCE_KEY,
            req.host_disk_allowance_gib.map(|v| json!(v)),
        ),
        (
            VM_DISK_ALLOWANCE_KEY,
            req.vm_disk_allowance_gib.map(|v| json!(v)),
        ),
        (
            DEFAULT_IDLE_TIMEOUT_KEY,
            req.default_idle_timeout_secs.map(|v| json!(v)),
        ),
        (
            DEFAULT_PERSONAL_IDLE_TIMEOUT_KEY,
            req.default_personal_idle_timeout_secs.map(|v| json!(v)),
        ),
        (MAX_RUNNING_VMS_KEY, req.max_running_vms.map(|v| json!(v))),
        (
            MAX_RUNNING_VMS_PER_OWNER_KEY,
            req.max_running_vms_per_owner.map(|v| json!(v)),
        ),
        (
            MAX_WORKSPACES_PER_OWNER_KEY,
            req.max_workspaces_per_owner.map(|v| json!(v)),
        ),
        (AUTO_ADD_HOSTS_KEY, req.auto_add_hosts.map(|v| json!(v))),
        (
            AUTO_HOST_INSTANCE_TYPE_KEY,
            req.auto_host_instance_type.as_ref().map(|v| json!(v)),
        ),
        (
            AUTO_RETIRE_IDLE_HOSTS_KEY,
            req.auto_retire_idle_hosts.map(|v| json!(v)),
        ),
        (
            IDLE_HOST_RETIRE_SECS_KEY,
            req.idle_host_retire_secs.map(|v| json!(v)),
        ),
        (MIN_FREE_SLOTS_KEY, req.min_free_slots.map(|v| json!(v))),
        (
            AUTOMATIC_CONTINUATION_KEY,
            req.automatic_continuation_enabled.map(|v| json!(v)),
        ),
    ]
    .into_iter()
    .filter_map(|(key, value)| value.map(|value| (key, value)))
    .collect();
    // Placement and capacity sweeps decide under the org lock; take it before
    // changing anything they read. The idle default is read only at workspace
    // creation and needs no lock.
    if writes
        .iter()
        .any(|(key, _)| *key != DEFAULT_IDLE_TIMEOUT_KEY)
    {
        lock_org_vm_quota(conn, org_id)
            .await
            .map_err(|e| G5Error::Internal {
                message: format!("Failed to lock org capacity settings: {e}"),
            })?;
    }
    if req.host_disk_allowance_gib.is_some()
        || req.vm_disk_allowance_gib.is_some()
        || req.auto_host_instance_type.is_some()
        || req.auto_add_hosts.is_some()
    {
        let mut values = read_values(conn, org_id, &ALL_KEYS).await?;
        values.extend(
            writes
                .iter()
                .map(|(key, value)| ((*key).to_owned(), value.clone())),
        );
        let capacity = managed_capacity_from(&values);
        let slots = if capacity.add_hosts {
            default_workspace_vm_limit(
                parse_vcpus_from_instance_type(&capacity.instance_type).unwrap_or(4),
            )
        } else {
            1
        };
        let policy = disk_policy_from(&values);
        if policy.resolve(None, slots).is_err() {
            return Err(G5Error::ValidationError {
                details: json!({
                    "hostDiskAllowanceGib": format!("H + V × {slots} slots requires {} GiB; the supported maximum is {} GiB", policy.default_gb(slots), policy.max_gb),
                    "vmDiskAllowanceGib": "Reduce the disk allowances or the automatic host slot count"
                }),
            });
        }
    }
    for (key, value) in writes {
        write_key(conn, org_id, actor_id, key, value).await?;
    }
    read(conn, org_id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(idle: Option<i64>, quota: Option<i64>) -> UpdateOrgAdminSettings {
        UpdateOrgAdminSettings {
            default_idle_timeout_secs: idle,
            max_running_vms: quota,
            ..Default::default()
        }
    }

    #[test]
    fn validation_accepts_the_inclusive_bounds() {
        for (idle, quota) in [(60, 0), (86_400, 100), (28_800, 10)] {
            assert!(validate(&request(Some(idle), Some(quota))).is_ok());
        }
        assert!(validate(&request(None, None)).is_ok());
        for secs in [900, 86_400] {
            assert!(validate(&UpdateOrgAdminSettings {
                idle_host_retire_secs: Some(secs),
                auto_host_instance_type: Some("c3-standard-44".into()),
                ..Default::default()
            })
            .is_ok());
        }
        for floor in [0, 100] {
            assert!(validate(&UpdateOrgAdminSettings {
                min_free_slots: Some(floor),
                ..Default::default()
            })
            .is_ok());
        }
        for (vms, workspaces) in [(0, 0), (100, 1_000)] {
            assert!(validate(&UpdateOrgAdminSettings {
                max_running_vms_per_owner: Some(vms),
                max_workspaces_per_owner: Some(workspaces),
                ..Default::default()
            })
            .is_ok());
        }
    }

    #[test]
    fn validation_rejects_each_out_of_range_field() {
        for (idle, quota, field) in [
            (Some(59), None, "defaultIdleTimeoutSecs"),
            (Some(86_401), None, "defaultIdleTimeoutSecs"),
            (None, Some(-1), "maxRunningVms"),
            (None, Some(101), "maxRunningVms"),
        ] {
            let Err(G5Error::ValidationError { details }) = validate(&request(idle, quota)) else {
                panic!("expected a validation error for {idle:?}/{quota:?}");
            };
            assert!(details.get(field).is_some(), "{details}");
        }
        for (req, field) in [
            (
                UpdateOrgAdminSettings {
                    idle_host_retire_secs: Some(899),
                    ..Default::default()
                },
                "idleHostRetireSecs",
            ),
            (
                UpdateOrgAdminSettings {
                    idle_host_retire_secs: Some(86_401),
                    ..Default::default()
                },
                "idleHostRetireSecs",
            ),
            (
                UpdateOrgAdminSettings {
                    auto_host_instance_type: Some("c3-standard-88".into()),
                    ..Default::default()
                },
                "autoHostInstanceType",
            ),
            (
                UpdateOrgAdminSettings {
                    auto_host_instance_type: Some("n2-standard-8/global/networks/x".into()),
                    ..Default::default()
                },
                "autoHostInstanceType",
            ),
            (
                UpdateOrgAdminSettings {
                    min_free_slots: Some(-1),
                    ..Default::default()
                },
                "minFreeSlots",
            ),
            (
                UpdateOrgAdminSettings {
                    min_free_slots: Some(101),
                    ..Default::default()
                },
                "minFreeSlots",
            ),
            (
                UpdateOrgAdminSettings {
                    max_running_vms_per_owner: Some(-1),
                    ..Default::default()
                },
                "maxRunningVmsPerOwner",
            ),
            (
                UpdateOrgAdminSettings {
                    max_running_vms_per_owner: Some(101),
                    ..Default::default()
                },
                "maxRunningVmsPerOwner",
            ),
            (
                UpdateOrgAdminSettings {
                    max_workspaces_per_owner: Some(-1),
                    ..Default::default()
                },
                "maxWorkspacesPerOwner",
            ),
            (
                UpdateOrgAdminSettings {
                    max_workspaces_per_owner: Some(1_001),
                    ..Default::default()
                },
                "maxWorkspacesPerOwner",
            ),
        ] {
            let Err(G5Error::ValidationError { details }) = validate(&req) else {
                panic!("expected a validation error for {req:?}");
            };
            assert!(details.get(field).is_some(), "{details}");
        }
    }

    #[test]
    fn missing_or_malformed_values_fall_back_to_platform_defaults() {
        assert_eq!(
            managed_capacity_from(&HashMap::new()),
            ManagedCapacitySettings::default()
        );
        let stored = HashMap::from([
            (AUTO_ADD_HOSTS_KEY.to_string(), json!(false)),
            (
                AUTO_HOST_INSTANCE_TYPE_KEY.to_string(),
                json!("c3-standard-8"),
            ),
            (AUTO_RETIRE_IDLE_HOSTS_KEY.to_string(), json!("no")),
            (IDLE_HOST_RETIRE_SECS_KEY.to_string(), json!(1_800)),
            (MIN_FREE_SLOTS_KEY.to_string(), json!(2)),
        ]);
        assert_eq!(
            managed_capacity_from(&stored),
            ManagedCapacitySettings {
                add_hosts: false,
                instance_type: "c3-standard-8".into(),
                retire_idle_hosts: true,
                idle_host_retire_secs: 1_800,
                min_free_slots: 2,
            }
        );
    }

    #[test]
    fn update_body_rejects_unknown_fields() {
        let parsed: Result<UpdateOrgAdminSettings, _> =
            serde_json::from_str(r#"{"maxRunningVms": 5, "region": "x"}"#);
        assert!(parsed.is_err());
    }
}
