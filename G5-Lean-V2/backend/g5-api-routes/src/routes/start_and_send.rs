//! Start and send (`g5.personal.start-and-send`): one owner request that
//! records Start consent and queues a chat input bound to that consent.
//!
//! The consent is recorded first, through the ordinary Resume admission
//! (`g5.lifecycle.start`), with the clearly labelled `compound_start` kind. The
//! input is then queued in the chat request's transaction and bound to the
//! consent under a share lock on the VM row, so the VM-ready transition cannot
//! interleave. From then on the `workspace_vms` trigger settles the binding in
//! the same transition that ends the consent: ready releases the input to
//! ordinary queueing; cancellation, expiry, failure or supersession withdraws
//! it. If the request fails after the consent was recorded, a consent that is
//! still only waiting is cancelled.

use std::sync::Arc;

use serde::Serialize;
use uuid::Uuid;

use crate::errors::AppError;
use crate::middleware::AuthContext;
use crate::services::workspace_vm_service::{ResumeOutcome, StartKind};
use crate::services::AppState;
use g5_authz::ScopedDb;
use g5_contracts::error::G5Error;

/// A recorded compound Start: its intent, the status Resume reported, and
/// whether the queued input must be bound to it.
#[derive(Debug, Clone)]
pub struct CompoundStart {
    pub workspace_id: Uuid,
    pub intent_id: Uuid,
    pub status: String,
    pub bind: bool,
    /// This request recorded the consent (false: it joined one under way).
    pub recorded: bool,
}

/// Reported with the queued message.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartComputeDTO {
    pub intent_id: String,
    pub status: String,
}

impl From<&CompoundStart> for StartComputeDTO {
    fn from(start: &CompoundStart) -> Self {
        Self {
            intent_id: start.intent_id.to_string(),
            status: start.status.clone(),
        }
    }
}

/// Record the owner's Start consent for `workspace_id`. Runs before the chat
/// request opens its transaction, because Resume locks the workspace row.
pub async fn begin(
    state: &Arc<AppState>,
    auth: &AuthContext,
    workspace_id: Uuid,
) -> Result<CompoundStart, AppError> {
    let user_id = auth.require_user_id()?;
    // The same authority as recording a Start: a deleted, transferring or
    // Base workspace refuses before anything is joined or recorded.
    let scope =
        crate::routes::workspaces::authorize_workspace_action(state, auth, workspace_id).await?;
    // A compound action while the owner's own Start is still under way joins
    // that Start: recording another would supersede it and cancel the work
    // bound to it (a queued message, a pending preview). Resume admission
    // applies that rule under the VM row lock, marks the joined consent and
    // retries placement for one still waiting, so no shortcut is taken here.
    let lifecycle = crate::routes::workspaces::workspace_vm_service(state, scope)?;
    let intent_id = Uuid::new_v4();
    // Admission selects the consent under its lock: the one it recorded, or
    // the owner's Start it joined or attached to. The work binds to that id
    // and never to one read from the VM row afterwards, which another Start
    // may have replaced meanwhile.
    let (outcome, effective) = lifecycle
        .resume_for_binding(workspace_id, user_id, intent_id, StartKind::CompoundStart)
        .await?;
    let (status, bind) = match outcome {
        ResumeOutcome::Attached { .. } => ("running", false),
        ResumeOutcome::Starting { .. } => ("starting", true),
        ResumeOutcome::QueuedForCapacity { .. } => ("queued_for_capacity", true),
    };
    Ok(CompoundStart {
        workspace_id,
        intent_id: effective,
        status: status.to_string(),
        bind,
        recorded: effective == intent_id,
    })
}

/// Bind the queued input to the consent inside the chat request's transaction.
/// A consent that already reached ready needs no binding; one that already
/// ended refuses the request, so nothing stays queued.
pub async fn bind(
    db: &mut ScopedDb,
    start: &CompoundStart,
    input_id: Uuid,
) -> Result<(), AppError> {
    if !start.bind {
        return Ok(());
    }
    use crate::services::agent_dispatch::{bind_input_to_start_consent, StartConsentBinding};
    match bind_input_to_start_consent(db.conn(), start.workspace_id, start.intent_id, input_id)
        .await?
    {
        StartConsentBinding::Bound | StartConsentBinding::AlreadyReady => Ok(()),
        StartConsentBinding::Ended => Err(AppError::from_error(G5Error::Conflict {
            reason: "The Start ended before the message was queued; nothing was sent".to_string(),
        })),
    }
}

/// After a failed request, cancel the consent if it still only waits. A
/// consent joined rather than recorded is left to the action that recorded it.
pub async fn abandon(state: &Arc<AppState>, auth: &AuthContext, start: &CompoundStart) {
    if !start.bind || !start.recorded {
        return;
    }
    let result = async {
        // A consent another compound action joined meanwhile keeps its
        // bound work: the cancel itself refuses that case.
        let scope =
            crate::routes::workspaces::authorize_workspace_action(state, auth, start.workspace_id)
                .await?;
        crate::routes::workspaces::workspace_vm_service(state, scope)?
            .cancel_waiting_start(start.workspace_id, start.intent_id)
            .await
            .map_err(AppError::from)
    }
    .await;
    if let Err(error) = result {
        tracing::warn!(
            workspace_id = %start.workspace_id,
            intent_id = %start.intent_id,
            error = %error.inner,
            "abandoned Start-and-send consent was not cancelled"
        );
    }
}
