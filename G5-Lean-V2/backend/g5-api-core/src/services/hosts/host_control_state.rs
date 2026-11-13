//! PostgreSQL authority for the worker-host control stream.
//!
//! P4.1 owns the additive tables and columns used here. Event insertion and
//! lifecycle transition share one transaction, and an acknowledgement is
//! returned only after that transaction commits.

use crate::services::git::repo_store::TenantRepoStorage;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Timelike, Utc};
use g5_auth::JwtMinter;
use g5_authz::{AuthzService, ScopedDb};
use g5_internal_api::host_ctl::{
    CurrentIncarnation, HeartbeatAck, HostAccept, HostCtlConfig, HostDrained, HostHeartbeat,
    HostHello, HostVmState, LeaseGrant, LeaseRenew, LeaseRenewResult, StaleLease, VmEvent,
    VmEventAck, VmEventKind, VmFileChanged, PROTOCOL_VERSION,
};
use sqlx::{PgConnection, PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::services::workspace_vm_release::AgentWorkSettlement;
use crate::services::EventBus;
use crate::ws::host_ctl::{ControlPlaneHostState, HostConnection, HostCtlError};
use crate::ws::{Channel, WsEvent};

const MVP_LEASE_TTL_SECS: i64 = 300;
const MVP_INCARNATION_TOKEN_TTL_SECS: i64 = 24 * 60 * 60;

pub struct PostgresHostControlState {
    pool: PgPool,
    tenant_pool: PgPool,
    minter: Arc<JwtMinter>,
    event_bus: EventBus,
    authz: Arc<dyn AuthzService>,
    agent_work: Arc<dyn AgentWorkSettlement>,
    source_preparation: Option<super::source_preparation::SourcePreparationContext>,
    live_hints: Arc<super::live_hints::LiveHints>,
}

impl PostgresHostControlState {
    /// Crate-private: only the host control stream applies a host's facts
    /// (`AppState::install_host_control_backend`). Another crate holding this
    /// backend could release a live VM with a made-up exit, or mint a VM
    /// token through lease renewal.
    #[must_use]
    pub(crate) fn new(
        pool: PgPool,
        tenant_pool: PgPool,
        minter: Arc<JwtMinter>,
        event_bus: EventBus,
        authz: Arc<dyn AuthzService>,
        agent_work: Arc<dyn AgentWorkSettlement>,
    ) -> Self {
        Self {
            pool,
            tenant_pool,
            minter,
            event_bus,
            authz,
            agent_work,
            source_preparation: None,
            live_hints: Arc::default(),
        }
    }

    /// Route hosts' module-event hints to this process's attempt followers.
    pub(crate) fn with_live_hints(mut self, live_hints: Arc<super::live_hints::LiveHints>) -> Self {
        self.live_hints = live_hints;
        self
    }

    pub(crate) fn with_source_preparation(
        mut self,
        storage: TenantRepoStorage,
        git_url: String,
    ) -> Self {
        self.source_preparation =
            Some(super::source_preparation::SourcePreparationContext { storage, git_url });
        self
    }

    /// For integration tests that drive host facts directly.
    #[cfg(feature = "test-helpers")]
    #[must_use]
    pub fn new_for_tests(
        pool: PgPool,
        tenant_pool: PgPool,
        minter: Arc<JwtMinter>,
        event_bus: EventBus,
        authz: Arc<dyn AuthzService>,
        agent_work: Arc<dyn AgentWorkSettlement>,
    ) -> Self {
        Self::new(pool, tenant_pool, minter, event_bus, authz, agent_work)
    }

    #[cfg(feature = "test-helpers")]
    pub fn with_source_preparation_for_tests(
        self,
        storage: TenantRepoStorage,
        git_url: String,
    ) -> Self {
        self.with_source_preparation(storage, git_url)
    }

    /// The live host's organization, read without a lock: a host never
    /// changes organization.
    async fn host_org(&self, host_id: Uuid) -> Result<Uuid, HostCtlError> {
        sqlx::query_scalar::<_, Uuid>(
            "SELECT org_id FROM worker_hosts WHERE host_id = $1 AND status <> 'terminated'",
        )
        .bind(host_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(database_error)?
        .ok_or_else(|| backend_error("host is absent or terminated"))
    }

    /// A transaction scoped to the host's organization, as org-scoped
    /// maintenance opens it (`g5.runtime.events.org-scope`): a host fact's
    /// owner effects then run under the same row rules and lock order as any
    /// other tenant caller, and the scope ends with the transaction.
    async fn org_scope(&self, org_id: Uuid) -> Result<ScopedDb, HostCtlError> {
        crate::db::maintenance::maintenance_org_scope(
            self.authz.as_ref(),
            &self.tenant_pool,
            org_id,
        )
        .await
        .map_err(database_error)
    }
}

/// A host-control transaction: on the system pool, or scoped to the host's
/// organization for a fact whose owner effects settle Agents work.
enum HostTx {
    System(Transaction<'static, Postgres>),
    Org(ScopedDb),
}

impl HostTx {
    fn conn(&mut self) -> &mut PgConnection {
        match self {
            Self::System(tx) => tx,
            Self::Org(db) => db.conn(),
        }
    }

    async fn commit(self) -> Result<(), HostCtlError> {
        match self {
            Self::System(tx) => tx.commit().await.map_err(database_error),
            Self::Org(db) => db.commit().await.map_err(database_error),
        }
    }
}

#[async_trait]
impl ControlPlaneHostState for PostgresHostControlState {
    async fn accept_hello(
        &self,
        host_id: Uuid,
        hello: &HostHello,
        connection_active: &HostConnection,
    ) -> Result<HostAccept, HostCtlError> {
        if hello.host_id != host_id
            || !g5_internal_api::host_ctl::SUPPORTED_PROTOCOL_VERSIONS
                .contains(&hello.protocol_version)
        {
            return Err(backend_error("host identity or protocol version mismatch"));
        }
        let slots_total = i32::try_from(hello.slots_total)
            .map_err(|_| backend_error("host slots_total exceeds database range"))?;
        let size = if hello.protocol_version == PROTOCOL_VERSION {
            let size = hello
                .slot_size
                .as_ref()
                .ok_or_else(|| backend_error("host hello must report its resolved slot size"))?;
            if slots_total <= 0
                || size.cpus == 0
                || !(256..=1_048_576).contains(&size.mem_mib)
                || !(1024..=1_073_741_824).contains(&size.scratch_disk_mib)
            {
                return Err(backend_error("invalid host slot resource contract"));
            }
            Some(size)
        } else {
            None
        };
        // Inventory reconciliation may release VMs and settle their Agents work.
        let org_id = self.host_org(host_id).await?;
        let mut db = self.org_scope(org_id).await?;
        // Match placement's lock order: org admission lock, then the host row.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1::text, 0))")
            .bind(org_id.to_string())
            .execute(db.conn())
            .await
            .map_err(database_error)?;
        let previous = sqlx::query(
            "SELECT slots_total, slot_cpus, slot_mem_mib, scratch_disk_mib, provisioned_slots,
                CASE WHEN status = 'ready' AND slot_size_reported
                    AND last_heartbeat >= now() - interval '30 seconds'
                    THEN idle_since END AS prior_idle
             FROM worker_hosts WHERE host_id = $1 AND status <> 'terminated' FOR UPDATE",
        )
        .bind(host_id)
        .fetch_optional(db.conn())
        .await
        .map_err(database_error)?
        .ok_or_else(|| backend_error("host is absent or terminated"))?;
        // A replaced tunnel may have waited for admission/row locks. It must
        // never publish after the currently authenticated tunnel took over.
        let _publication_guard = connection_active.publication_guard().await;
        if !connection_active.is_active() {
            return Err(HostCtlError::Closed);
        }
        // As with heartbeat, the liveness UPDATE temporarily invalidates
        // inventory and idle evidence. A fresh empty reconnect can restore the
        // locked timer, but readiness promotion or a new sizing contract cannot.
        let mut prior_idle = previous
            .try_get::<Option<DateTime<Utc>>, _>("prior_idle")
            .map_err(database_error)?;
        // The host's published runtime identity decides placement eligibility
        // (`g5.runtime.placement.runtime-identity`); a hello without one
        // stores an empty object, which no control plane matches.
        let runtime_identity = hello
            .runtime
            .as_ref()
            .map(serde_json::to_value)
            .transpose()
            .map_err(|e| backend_error(format!("encode host runtime identity: {e}")))?
            .unwrap_or_else(|| serde_json::json!({}));
        if let Some(size) = size {
            if let Some(recorded_slots) = previous
                .try_get::<Option<i32>, _>("provisioned_slots")
                .map_err(database_error)?
            {
                if recorded_slots != slots_total
                    || previous
                        .try_get::<i32, _>("scratch_disk_mib")
                        .map_err(database_error)?
                        != size.scratch_disk_mib as i32
                {
                    return Err(backend_error(
                        "host registration differs from its recorded disk allocation (V or slots)",
                    ));
                }
            }
            let changed = previous
                .try_get::<i32, _>("slots_total")
                .map_err(database_error)?
                != slots_total
                || previous
                    .try_get::<i32, _>("slot_cpus")
                    .map_err(database_error)?
                    != i32::from(size.cpus)
                || previous
                    .try_get::<i32, _>("slot_mem_mib")
                    .map_err(database_error)?
                    != size.mem_mib as i32
                || previous
                    .try_get::<i32, _>("scratch_disk_mib")
                    .map_err(database_error)?
                    != size.scratch_disk_mib as i32;
            if changed {
                prior_idle = None;
                let occupied = sqlx::query_scalar::<_, bool>(
                    "SELECT EXISTS(SELECT 1 FROM workspace_vms WHERE host_id = $1
                 AND status IN ('starting', 'running', 'stopping'))",
                )
                .bind(host_id)
                .fetch_one(db.conn())
                .await
                .map_err(database_error)?;
                if occupied
                    || hello
                        .inventory
                        .iter()
                        .any(|vm| vm.state != g5_internal_api::host_ctl::HostVmState::Exited)
                {
                    return Err(backend_error(
                        "cannot resize host slots while VM custody is active",
                    ));
                }
            }
            sqlx::query(
            "UPDATE worker_hosts SET last_heartbeat = now(), capabilities = $2,
                 slots_total = $3, slot_cpus = $4,
                 slot_mem_mib = $5, scratch_disk_mib = $6, slot_size_reported = true,
                 runtime_identity = $7,
                 status = CASE WHEN status IN ('provisioning', 'unreachable') THEN 'ready' ELSE status END
             WHERE host_id = $1",
        ).bind(host_id).bind(&hello.capabilities).bind(slots_total)
            .bind(i32::from(size.cpus)).bind(size.mem_mib as i32).bind(size.scratch_disk_mib as i32)
            .bind(&runtime_identity)
            .execute(db.conn()).await.map_err(database_error)?;
        } else {
            prior_idle = None;
            // Legacy peers retain custody/control, but cannot qualify a sizing
            // contract or receive new placement during control-plane-first rollout.
            // Restore control liveness for existing rotation/recovery consumers;
            // both placement and replacement readiness independently require publication.
            sqlx::query(
                "UPDATE worker_hosts SET last_heartbeat = now(), capabilities = $2,
                slot_size_reported = false, runtime_identity = $3,
                status = CASE WHEN status = 'unreachable' THEN 'ready' ELSE status END
                WHERE host_id = $1",
            )
            .bind(host_id)
            .bind(&hello.capabilities)
            .bind(&runtime_identity)
            .execute(db.conn())
            .await
            .map_err(database_error)?;
        }
        reconcile_hello_inventory(
            self.agent_work.as_ref(),
            db.conn(),
            host_id,
            hello,
            prior_idle,
        )
        .await?;
        let current_incarnations = current_incarnations(db.conn(), host_id).await?;
        let incarnation_generation = transaction_generation(db.conn()).await?;
        // COMMIT may already be dispatched when the carrier is cancelled.
        // Its owned guard must survive that cancellation until PostgreSQL has
        // completed the commit, so takeover cannot precede publication.
        tokio::spawn(async move {
            let _publication_guard = _publication_guard;
            db.commit().await
        })
        .await
        .map_err(|e| backend_error(format!("host publication commit task failed: {e}")))?
        .map_err(database_error)?;
        // Negotiate only; optional Git/DB work runs after registration, on a
        // requested heartbeat, so source preparation cannot delay host hello.
        let config = HostCtlConfig {
            source_preparation_enabled: self.source_preparation.is_some()
                && hello
                    .capabilities
                    .iter()
                    .any(|c| c == g5_internal_api::host_ctl::CAPABILITY_SOURCE_PREPARATION),
            // Enabled only for a host that offered it, so an older host never
            // sees the field and a newer one never sends hints to an older
            // control plane (`g5.runtime.module-event-hints`).
            module_event_hints_enabled: hello
                .capabilities
                .iter()
                .any(|c| c == g5_internal_api::host_ctl::CAPABILITY_MODULE_EVENT_HINTS),
            ..HostCtlConfig::default()
        };
        Ok(HostAccept {
            accepted: true,
            protocol_version: hello.protocol_version,
            incarnation_generation,
            current_incarnations,
            config,
        })
    }

    async fn heartbeat(
        &self,
        host_id: Uuid,
        heartbeat: &HostHeartbeat,
    ) -> Result<HeartbeatAck, HostCtlError> {
        let mut tx = self.pool.begin().await.map_err(database_error)?;
        // Liveness writes invalidate inventory so old API replicas cannot leave
        // stale known-empty evidence. Preserve the prior projection under the
        // same row lock, then restore this partial report before committing.
        let (prior_inventory, prior_idle) = sqlx::query_as::<
            _,
            (Option<Vec<Uuid>>, Option<chrono::DateTime<chrono::Utc>>),
        >(
            "SELECT reported_live_vm_ids, CASE WHEN last_heartbeat >= now() - interval '30 seconds'
                THEN idle_since END FROM worker_hosts
             WHERE host_id = $1 AND status <> 'terminated' FOR UPDATE",
        )
        .bind(host_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(database_error)?
        .ok_or_else(|| backend_error("heartbeat host is absent or terminated"))?;
        // Authenticated control proves liveness, not resource qualification.
        // Never promote provisioning here; admission retains its publication gate.
        // A heartbeat may omit inventory, so absence cannot clear custody from
        // hello. Merge live reports and remove only explicit exit evidence.
        let live = heartbeat
            .vms
            .iter()
            .filter(|vm| vm.state != HostVmState::Exited)
            .map(|vm| vm.vm_id)
            .collect::<Vec<_>>();
        let exited = heartbeat
            .vms
            .iter()
            .filter(|vm| vm.state == HostVmState::Exited)
            .map(|vm| vm.vm_id)
            .collect::<Vec<_>>();
        let updated = sqlx::query_scalar::<_, Uuid>(
            "UPDATE worker_hosts
             SET last_heartbeat = now(),
                 status = CASE WHEN status = 'unreachable' THEN 'ready' ELSE status END
             WHERE host_id = $1 AND status <> 'terminated'
             RETURNING host_id",
        )
        .bind(host_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(database_error)?;
        if updated.is_none() {
            return Err(backend_error("heartbeat host is absent or terminated"));
        }
        sqlx::query(
            "UPDATE worker_hosts
             SET reported_live_vm_ids = CASE WHEN $2::uuid[] IS NULL THEN NULL ELSE ARRAY(
                 (SELECT unnest($2::uuid[]) EXCEPT SELECT unnest($4::uuid[]))
                 UNION SELECT unnest($3::uuid[])) END,
                 idle_since = $5
             WHERE host_id = $1",
        )
        .bind(host_id)
        .bind(prior_inventory)
        .bind(live)
        .bind(exited)
        .bind(prior_idle)
        .execute(&mut *tx)
        .await
        .map_err(database_error)?;
        let current_incarnations = current_incarnations(&mut tx, host_id).await?;
        let incarnation_generation = transaction_generation(&mut tx).await?;
        tx.commit().await.map_err(database_error)?;
        let source_preparation = if heartbeat.refresh_source_preparation {
            if let Some(source) = &self.source_preparation {
                let org = self.host_org(host_id).await?;
                tokio::time::timeout(
                    std::time::Duration::from_secs(2),
                    source.manifest(
                        &self.pool,
                        &self.tenant_pool,
                        self.authz.as_ref(),
                        &self.minter,
                        host_id,
                        org,
                    ),
                )
                .await
                .ok()
                .and_then(Result::ok)
            } else {
                None
            }
        } else {
            None
        };
        Ok(HeartbeatAck {
            source_preparation,
            incarnation_generation,
            current_incarnations,
        })
    }

    async fn ingest_vm_event(
        &self,
        host_id: Uuid,
        event: &VmEvent,
    ) -> Result<VmEventAck, HostCtlError> {
        let seq = i64::try_from(event.seq)
            .map_err(|_| backend_error("vm event sequence exceeds database range"))?;
        let payload = event_payload(event)?;
        let kind = event_kind_name(&event.kind);
        // PostgreSQL timestamptz stores microseconds. Canonicalize before the
        // insert and replay comparison so Linux nanosecond clocks do not make
        // an identical journal replay look like a conflicting sequence.
        let event_at = postgres_timestamp(event.at)?;
        let mut tx = if settles_agents_work(&event.kind) {
            HostTx::Org(self.org_scope(self.host_org(host_id).await?).await?)
        } else {
            HostTx::System(self.pool.begin().await.map_err(database_error)?)
        };
        let org_id = sqlx::query_scalar::<_, Uuid>(
            "SELECT org_id FROM worker_hosts WHERE host_id = $1 AND status <> 'terminated' FOR UPDATE",
        )
        .bind(host_id)
        .fetch_optional(tx.conn())
        .await
        .map_err(database_error)?
        .ok_or_else(|| backend_error("event host is absent or terminated"))?;

        let inserted = sqlx::query(
            "INSERT INTO worker_vm_events
                 (org_id, workspace_id, host_id, vm_id, seq, kind, payload, at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT (vm_id, seq) DO NOTHING",
        )
        .bind(org_id)
        .bind(event.workspace_id)
        .bind(host_id)
        .bind(event.vm_id.to_string())
        .bind(seq)
        .bind(kind)
        .bind(&payload)
        .bind(event_at)
        .execute(tx.conn())
        .await
        .map_err(database_error)?
        .rows_affected()
            == 1;

        let stored = sqlx::query(
            "SELECT org_id, workspace_id, host_id, kind, payload, at
             FROM worker_vm_events WHERE vm_id = $1 AND seq = $2",
        )
        .bind(event.vm_id.to_string())
        .bind(seq)
        .fetch_one(tx.conn())
        .await
        .map_err(database_error)?;
        if stored
            .try_get::<Uuid, _>("org_id")
            .map_err(database_error)?
            != org_id
            || stored
                .try_get::<Uuid, _>("workspace_id")
                .map_err(database_error)?
                != event.workspace_id
            || stored
                .try_get::<Uuid, _>("host_id")
                .map_err(database_error)?
                != host_id
            || stored
                .try_get::<String, _>("kind")
                .map_err(database_error)?
                != kind
            || stored
                .try_get::<serde_json::Value, _>("payload")
                .map_err(database_error)?
                != payload
            || stored
                .try_get::<DateTime<Utc>, _>("at")
                .map_err(database_error)?
                != event_at
        {
            return Err(backend_error(
                "vm event sequence replay conflicts with the durable event",
            ));
        }

        // Recording an event and applying it commit together, so a replay of a
        // recorded event is only acknowledged again. Applying it again would
        // take the release's workspace lock while this transaction holds the
        // host row.
        let activity_change = if inserted {
            apply_current_event(
                self.agent_work.as_ref(),
                tx.conn(),
                org_id,
                host_id,
                event,
                &payload,
            )
            .await?
        } else {
            None
        };
        // Every host-reported event that PROVES the child dead retires the
        // incarnation from the host's reported inventory, not just a clean
        // exit (issue #627 S5, `g5.runtime.placement.capacity`): a failed boot
        // emits BootFailed for diagnostics, then Exited only after proven
        // cleanup. Keeping those facts separate preserves durable payloads
        // through older APIs. BootFailed alone leaves inventory counting;
        // StaleKilled is journaled only after a confirmed kill.
        // vm_ids are unique per incarnation, so the keyed removal can never
        // touch a newer VM; array_remove keeps replays idempotent.
        if matches!(
            event.kind,
            VmEventKind::Exited { .. } | VmEventKind::StaleKilled
        ) {
            sqlx::query(
                "UPDATE worker_hosts
                 SET reported_live_vm_ids = array_remove(reported_live_vm_ids, $2::uuid)
                 WHERE host_id = $1",
            )
            .bind(host_id)
            .bind(event.vm_id)
            .execute(tx.conn())
            .await
            .map_err(database_error)?;
        }
        let through_seq = contiguous_event_sequence(tx.conn(), org_id, event.vm_id).await?;
        sqlx::query(
            "UPDATE worker_vm_events SET acknowledged_at = COALESCE(acknowledged_at, now())
             WHERE org_id = $1 AND vm_id = $2 AND seq <= $3",
        )
        .bind(org_id)
        .bind(event.vm_id.to_string())
        .bind(through_seq)
        .execute(tx.conn())
        .await
        .map_err(database_error)?;
        // Host control has no tenant session. Use the narrow runtime authority
        // lookup, as lease renewal does, rather than a workspace query hidden by RLS.
        let file_changed_owner =
            if inserted && matches!(event.kind, VmEventKind::FileChanged { .. }) {
                sqlx::query_scalar::<_, Uuid>(
                    "SELECT workspace.owner_user_id
                 FROM authz.lookup_workspace_runtime_authority($1) AS workspace
                 JOIN workspace_vms AS vm
                   ON vm.workspace_id = workspace.workspace_id AND vm.org_id = workspace.org_id
                 WHERE workspace.org_id = $2
                   AND workspace.deleted_at IS NULL
                   AND vm.host_id = $3
                   AND vm.vm_id = $4
                   AND vm.status = 'running'",
                )
                .bind(event.workspace_id)
                .bind(org_id)
                .bind(host_id)
                .bind(event.vm_id.to_string())
                .fetch_optional(tx.conn())
                .await
                .map_err(database_error)?
            } else {
                None
            };
        tx.commit().await?;
        // A VM becoming ready releases held turns; one that exits or fails
        // settles them. Either changes chat activity across the workspace
        // (chat g5.chat.activity.refresh).
        if activity_change.is_some()
            || matches!(
                event.kind,
                VmEventKind::Exited { .. }
                    | VmEventKind::Oom
                    | VmEventKind::StaleKilled
                    | VmEventKind::BootFailed { .. }
            )
        {
            self.event_bus.publish(
                &Channel::Workspace(event.workspace_id),
                &WsEvent::ChatActivity {
                    workspace_id: event.workspace_id,
                    session_id: None,
                },
            );
        }
        if let Some((epoch, previous, new)) = activity_change {
            g5_api_foundation::activity::log_activity_change(
                event.workspace_id,
                event.vm_id,
                epoch,
                "initialization",
                "vm_ready",
                &format!("host-event:{}:{}", event.vm_id, event.seq),
                previous,
                Some(new),
            );
        }
        if let (
            Some(owner_user_id),
            VmEventKind::FileChanged {
                repo_id,
                path,
                rescan_required,
            },
        ) = (file_changed_owner, &event.kind)
        {
            self.event_bus.publish_to_user(
                &Channel::Workspace(event.workspace_id),
                owner_user_id,
                &WsEvent::WorkspaceFileChanged {
                    workspace_id: event.workspace_id,
                    repo_id: *repo_id,
                    path: path.clone(),
                    rescan_required: *rescan_required,
                },
            );
        }
        Ok(VmEventAck {
            vm_id: event.vm_id,
            through_seq: u64::try_from(through_seq)
                .map_err(|_| backend_error("negative durable event acknowledgement"))?,
        })
    }

    async fn module_events(
        &self,
        _host_id: Uuid,
        hint: &g5_internal_api::host_ctl::VmModuleEvents,
    ) -> Result<(), HostCtlError> {
        // The hint only wakes followers; authority, incarnation and ordering
        // are checked when the follower fetches the journal through the
        // runtime facade, so a stale or forged hint can at most cause one
        // fenced fetch.
        for exec_id in &hint.exec_ids {
            self.live_hints.hint(exec_id);
        }
        Ok(())
    }

    async fn file_changed(&self, host_id: Uuid, event: &VmFileChanged) -> Result<(), HostCtlError> {
        let owner_user_id = sqlx::query_scalar::<_, Uuid>(
            "SELECT workspace.owner_user_id
             FROM authz.lookup_workspace_runtime_authority($1) AS workspace
             JOIN workspace_vms AS vm
               ON vm.workspace_id = workspace.workspace_id AND vm.org_id = workspace.org_id
             JOIN worker_hosts AS host ON host.host_id = vm.host_id AND host.org_id = vm.org_id
             WHERE workspace.deleted_at IS NULL
               AND vm.host_id = $2
               AND vm.vm_id = $3
               AND vm.status = 'running'
               AND host.status <> 'terminated'",
        )
        .bind(event.workspace_id)
        .bind(host_id)
        .bind(event.vm_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(database_error)?;
        if let Some(owner_user_id) = owner_user_id {
            self.event_bus.publish_to_user(
                &Channel::Workspace(event.workspace_id),
                owner_user_id,
                &WsEvent::WorkspaceFileChanged {
                    workspace_id: event.workspace_id,
                    repo_id: event.repo_id,
                    path: event.path.clone(),
                    rescan_required: event.rescan_required,
                },
            );
        }
        Ok(())
    }

    async fn renew_lease(
        &self,
        host_id: Uuid,
        renewal: &LeaseRenew,
    ) -> Result<LeaseRenewResult, HostCtlError> {
        let epoch = i64::try_from(renewal.epoch)
            .map_err(|_| backend_error("lease epoch exceeds database range"))?;
        let sequence = i64::try_from(renewal.seq)
            .map_err(|_| backend_error("lease sequence exceeds database range"))?;
        let mut tx = self.pool.begin().await.map_err(database_error)?;
        let row = sqlx::query(
            "SELECT vm.org_id, workspace.owner_user_id, vm.lease_sequence, vm.lease_expires_at
             FROM workspace_vms AS vm
             JOIN authz.lookup_workspace_runtime_authority($1) AS workspace
               ON workspace.org_id = vm.org_id
              AND workspace.workspace_id = vm.workspace_id
             WHERE vm.workspace_id = $1 AND vm.host_id = $2 AND vm.vm_id = $3
               AND vm.epoch = $4 AND vm.status IN ('starting', 'running', 'stopping')
               AND workspace.deleted_at IS NULL
               AND vm.lease_expires_at > now()
             FOR UPDATE OF vm",
        )
        .bind(renewal.workspace_id)
        .bind(host_id)
        .bind(renewal.vm_id.to_string())
        .bind(epoch)
        .fetch_optional(&mut *tx)
        .await
        .map_err(database_error)?;
        let Some(row) = row else {
            tx.rollback().await.map_err(database_error)?;
            return Ok(LeaseRenewResult::Stale(StaleLease { stale: true }));
        };
        let previous_sequence = row
            .try_get::<i64, _>("lease_sequence")
            .map_err(database_error)?;
        if sequence < previous_sequence {
            tx.rollback().await.map_err(database_error)?;
            return Ok(LeaseRenewResult::Stale(StaleLease { stale: true }));
        }
        let lease_expires_at = if sequence > previous_sequence {
            sqlx::query_scalar::<_, DateTime<Utc>>(
                "UPDATE workspace_vms
                 SET lease_sequence = $2,
                     lease_expires_at = now() + make_interval(secs => $3),
                     updated_at = now()
                 WHERE workspace_id = $1
                 RETURNING lease_expires_at",
            )
            .bind(renewal.workspace_id)
            .bind(sequence)
            .bind(i32::try_from(MVP_LEASE_TTL_SECS).unwrap_or(i32::MAX))
            .fetch_one(&mut *tx)
            .await
            .map_err(database_error)?
        } else {
            row.try_get::<DateTime<Utc>, _>("lease_expires_at")
                .map_err(database_error)?
        };
        let org_id = row.try_get::<Uuid, _>("org_id").map_err(database_error)?;
        let owner_user_id = row
            .try_get::<Uuid, _>("owner_user_id")
            .map_err(database_error)?;
        let token = self
            .minter
            .mint_workspace_incarnation(
                org_id,
                renewal.workspace_id,
                owner_user_id,
                renewal.vm_id,
                renewal.epoch,
                MVP_INCARNATION_TOKEN_TTL_SECS,
            )
            .map_err(|error| backend_error(error.to_string()))?;
        let incarnation_generation = transaction_generation(&mut tx).await?;
        let remaining = lease_expires_at
            .signed_duration_since(Utc::now())
            .num_seconds()
            .max(1) as u64;
        tx.commit().await.map_err(database_error)?;
        Ok(LeaseRenewResult::Granted(LeaseGrant {
            stale: false,
            seq: renewal.seq,
            token,
            lease_ttl_secs: remaining,
            incarnation_generation,
        }))
    }

    async fn host_drained(&self, host_id: Uuid, drained: &HostDrained) -> Result<(), HostCtlError> {
        let active = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM workspace_vms
             WHERE host_id = $1 AND status IN ('starting', 'running', 'stopping')",
        )
        .bind(host_id)
        .fetch_one(&self.pool)
        .await
        .map_err(database_error)?;
        if active != 0 {
            return Err(backend_error(
                "host reported drained while durable VM custody remains",
            ));
        }
        let updated = sqlx::query(
            "UPDATE worker_hosts
             SET status = 'terminated', last_heartbeat = NULL
             WHERE host_id = $1 AND status = 'draining' AND drain_id = $2",
        )
        .bind(host_id)
        .bind(drained.drain_id)
        .execute(&self.pool)
        .await
        .map_err(database_error)?;
        if updated.rows_affected() != 1 {
            return Err(backend_error(
                "host.drained does not match the active durable drain",
            ));
        }
        Ok(())
    }
}

async fn reconcile_hello_inventory(
    agents: &dyn AgentWorkSettlement,
    conn: &mut PgConnection,
    host_id: Uuid,
    hello: &HostHello,
    prior_idle: Option<DateTime<Utc>>,
) -> Result<(), HostCtlError> {
    let live = hello
        .inventory
        .iter()
        .filter(|vm| vm.state != HostVmState::Exited)
        .map(|vm| vm.vm_id)
        .collect::<Vec<_>>();
    // The authenticated complete snapshot is published under the host-row
    // lock and connection guard, alongside sizing and durable reconciliation.
    // The capacity guard rejects the restored timer if this inventory is live.
    sqlx::query(
        "UPDATE worker_hosts SET reported_live_vm_ids = $2, idle_since = $3 WHERE host_id = $1",
    )
    .bind(host_id)
    .bind(live)
    .bind(prior_idle)
    .execute(&mut *conn)
    .await
    .map_err(database_error)?;
    let present = hello
        .inventory
        .iter()
        .map(|vm| vm.vm_id.to_string())
        .collect::<Vec<_>>();
    crate::services::hosts::workspace_vm_release::release_missing_from_inventory(
        agents, conn, host_id, &present,
    )
    .await
    .map_err(database_error)?;
    Ok(())
}

async fn current_incarnations(
    conn: &mut PgConnection,
    host_id: Uuid,
) -> Result<Vec<CurrentIncarnation>, HostCtlError> {
    let rows = sqlx::query(
        "SELECT workspace_id, vm_id, epoch FROM workspace_vms
         WHERE host_id = $1 AND status IN ('starting', 'running', 'stopping')
         ORDER BY workspace_id",
    )
    .bind(host_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(database_error)?;
    rows.into_iter()
        .map(|row| {
            let vm_id = row
                .try_get::<String, _>("vm_id")
                .map_err(database_error)?
                .parse::<Uuid>()
                .map_err(|_| backend_error("durable VM id is not a UUID"))?;
            let epoch = u64::try_from(row.try_get::<i64, _>("epoch").map_err(database_error)?)
                .map_err(|_| backend_error("durable VM epoch is negative"))?;
            Ok(CurrentIncarnation {
                workspace_id: row.try_get("workspace_id").map_err(database_error)?,
                vm_id,
                epoch,
            })
        })
        .collect()
}

async fn transaction_generation(conn: &mut PgConnection) -> Result<u64, HostCtlError> {
    let generation = sqlx::query_scalar::<_, i64>("SELECT txid_current()::bigint")
        .fetch_one(&mut *conn)
        .await
        .map_err(database_error)?;
    u64::try_from(generation).map_err(|_| backend_error("negative transaction generation"))
}

/// Whether a VM fact changes Agents work, which needs the organization's task
/// and message rows (`g5.runtime.events.org-scope`): a release settles it, and
/// Ready returns work held by a stop to the queue (`g5.lifecycle.start.held-work`).
fn settles_agents_work(kind: &VmEventKind) -> bool {
    match kind {
        VmEventKind::Ready { .. }
        | VmEventKind::Exited { .. }
        | VmEventKind::Oom
        | VmEventKind::StaleKilled
        | VmEventKind::BootFailed { .. } => true,
        VmEventKind::Booted | VmEventKind::FileChanged { .. } => false,
    }
}

async fn apply_current_event(
    agents: &dyn AgentWorkSettlement,
    conn: &mut PgConnection,
    org_id: Uuid,
    host_id: Uuid,
    event: &VmEvent,
    payload: &serde_json::Value,
) -> Result<Option<(u64, Option<DateTime<Utc>>, DateTime<Utc>)>, HostCtlError> {
    match &event.kind {
        VmEventKind::Ready {
            epoch,
            module_catalog_version,
            dispatcher_contract_version,
        } => {
            let epoch = i64::try_from(*epoch)
                .map_err(|_| backend_error("ready epoch exceeds database range"))?;
            // Take custody of the exact incarnation before testing wall-clock
            // expiry. Ready can arrive after slow image preparation, or wait
            // behind another row owner until its original lease has expired.
            // A predicate evaluated before that wait cannot authorize release.
            let previous = sqlx::query_scalar::<_, Option<DateTime<Utc>>>(
                "SELECT last_activity_at FROM workspace_vms
                 WHERE org_id = $1 AND workspace_id = $2 AND host_id = $3 AND vm_id = $4
                   AND epoch = $5 AND status = 'starting' FOR UPDATE",
            )
            .bind(org_id)
            .bind(event.workspace_id)
            .bind(host_id)
            .bind(event.vm_id.to_string())
            .bind(epoch)
            .fetch_optional(&mut *conn)
            .await
            .map_err(database_error)?;
            let updated = sqlx::query(
                "UPDATE workspace_vms
                 SET status = 'running', started_at = $7,
                     last_activity_at = $7,
                     idle_retry_after = NULL,
                     start_intent = CASE WHEN start_intent IS NULL THEN NULL ELSE
                         jsonb_set(
                             jsonb_set(start_intent, '{status}', '\"consumed\"'),
                             '{consumed_vm_id}', to_jsonb($4::text)
                         ) END,
                     updated_at = now()
                 WHERE org_id = $1 AND workspace_id = $2 AND host_id = $3 AND vm_id = $4
                   AND epoch = $5 AND status = 'starting'
                   AND lease_expires_at > clock_timestamp()
                   AND module_catalog_version = $6
                   AND (boot_spec ->> 'dispatcher_contract_version')::bigint = $8",
            )
            .bind(org_id)
            .bind(event.workspace_id)
            .bind(host_id)
            .bind(event.vm_id.to_string())
            .bind(epoch)
            .bind(module_catalog_version)
            .bind(event.at)
            .bind(i64::from(*dispatcher_contract_version))
            .execute(&mut *conn)
            .await
            .map_err(database_error)?;
            if updated.rows_affected() == 1 {
                agents
                    .release_tasks_after_resume(conn, event.workspace_id)
                    .await
                    .map_err(database_error)?;
                return Ok(Some((
                    epoch as u64,
                    previous.flatten(),
                    postgres_timestamp(event.at)?,
                )));
            }
        }
        VmEventKind::Exited { .. } => {
            if let Some(diagnostic) = event
                .log_tail
                .as_deref()
                .filter(|tail| g5_contracts::error::HostDiskPressure::parse(tail).is_some())
            {
                if crate::services::hosts::workspace_vm_release::release_disk_capacity(
                    conn,
                    org_id,
                    event.workspace_id,
                    host_id,
                    event.vm_id,
                    diagnostic,
                )
                .await
                .map_err(database_error)?
                {
                    return Ok(None);
                }
            }
            crate::services::hosts::workspace_vm_release::release_exited(
                agents,
                conn,
                org_id,
                event.workspace_id,
                host_id,
                event.vm_id,
                payload,
                event.at,
            )
            .await
            .map_err(database_error)?;
        }
        VmEventKind::BootFailed { reason }
            if g5_contracts::error::HostDiskPressure::parse(reason).is_some() =>
        {
            // Diagnostics do not prove exit. Preserve Start until the separate
            // Exited fact (or a definite capacity reply) settles this incarnation.
            sqlx::query("UPDATE workspace_vms SET last_error = $5 WHERE org_id = $1 AND workspace_id = $2 AND host_id = $3 AND vm_id = $4 AND status = 'starting'")
                .bind(org_id).bind(event.workspace_id).bind(host_id).bind(event.vm_id.to_string())
                .bind(serde_json::json!({"reason": "host_capacity_refused", "host_reason": "disk", "diagnostic": reason}))
                .execute(&mut *conn).await.map_err(database_error)?;
        }
        VmEventKind::Oom | VmEventKind::StaleKilled | VmEventKind::BootFailed { .. } => {
            crate::services::hosts::workspace_vm_release::release_failed(
                agents,
                conn,
                org_id,
                event.workspace_id,
                host_id,
                event.vm_id,
                payload,
                event.at,
            )
            .await
            .map_err(database_error)?;
        }
        VmEventKind::Booted | VmEventKind::FileChanged { .. } => {}
    }
    Ok(None)
}

async fn contiguous_event_sequence(
    conn: &mut PgConnection,
    org_id: Uuid,
    vm_id: Uuid,
) -> Result<i64, HostCtlError> {
    sqlx::query_scalar::<_, i64>(
        "WITH ordered AS (
             SELECT seq, lag(seq, 1, 0) OVER (ORDER BY seq) AS previous
             FROM worker_vm_events WHERE org_id = $1 AND vm_id = $2
         ), first_gap AS (
             SELECT min(seq) AS seq FROM ordered WHERE seq <> previous + 1
         ), maximum AS (
             SELECT COALESCE(max(seq), 0) AS seq FROM ordered
         )
         SELECT CASE
             WHEN first_gap.seq IS NULL THEN maximum.seq
             ELSE first_gap.seq - 1
         END
         FROM first_gap CROSS JOIN maximum",
    )
    .bind(org_id)
    .bind(vm_id.to_string())
    .fetch_one(&mut *conn)
    .await
    .map_err(database_error)
}

fn event_payload(event: &VmEvent) -> Result<serde_json::Value, HostCtlError> {
    let mut payload = serde_json::to_value(&event.kind)
        .map_err(|error| backend_error(format!("encode VM event payload: {error}")))?;
    let object = payload
        .as_object_mut()
        .ok_or_else(|| backend_error("VM event payload is not an object"))?;
    if let Some(log_tail) = &event.log_tail {
        object.insert(
            "log_tail".to_string(),
            serde_json::Value::String(log_tail.clone()),
        );
    }
    Ok(payload)
}

fn event_kind_name(kind: &VmEventKind) -> &'static str {
    match kind {
        VmEventKind::Booted => "booted",
        VmEventKind::Ready { .. } => "ready",
        VmEventKind::Exited { .. } => "exited",
        VmEventKind::Oom => "oom",
        VmEventKind::StaleKilled => "stale_killed",
        VmEventKind::BootFailed { .. } => "boot_failed",
        VmEventKind::FileChanged { .. } => "file_changed",
    }
}

fn postgres_timestamp(timestamp: DateTime<Utc>) -> Result<DateTime<Utc>, HostCtlError> {
    timestamp
        .with_nanosecond(timestamp.nanosecond() / 1_000 * 1_000)
        .ok_or_else(|| backend_error("vm event timestamp is invalid"))
}

fn database_error(error: impl std::fmt::Display) -> HostCtlError {
    backend_error(format!("database operation failed: {error}"))
}

fn backend_error(message: impl Into<String>) -> HostCtlError {
    HostCtlError::Backend(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postgres_timestamp_matches_timestamptz_precision() {
        let timestamp = DateTime::parse_from_rfc3339("2026-09-21T12:34:56.123456789Z")
            .unwrap()
            .with_timezone(&Utc);

        assert_eq!(
            postgres_timestamp(timestamp).unwrap().to_rfc3339(),
            "2026-09-21T12:34:56.123456+00:00"
        );
    }
}
