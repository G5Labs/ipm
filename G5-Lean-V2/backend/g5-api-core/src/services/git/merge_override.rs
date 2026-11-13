//! Merge with override: an organization administrator's waiver of G5's own
//! merge requirements (g5.workspace.merge.admin-override) and a person's
//! GitHub rule bypass under their own GitHub identity
//! (g5.git.provider-override), sharing one confirmation, re-check and record
//! (g5.workspace.merge.admin-override.formal).
//!
//! [`merge_override_authority`] is the only place that decides who may make
//! each override. The merge, the override preview and, through the preview,
//! the UI's offer ask it; none of them tests roles itself.

use chrono::{DateTime, Utc};
use g5_contracts::types::task_context::MergeSourceHead;
use g5_contracts::G5Error;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::workspace_merge_service::MergeAdmissionEvidence;

// ---------------------------------------------------------------------------
// Authority
// ---------------------------------------------------------------------------

/// The facts [`merge_override_authority`] decides on. Callers establish them
/// with the ordinary authority checks and never decide override authority
/// from them directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MergeOverrideSubject {
    /// The caller passed the ordinary merge admission for this pull request
    /// (including the source-owner path when the target has a requirement).
    pub may_merge: bool,
    /// The caller holds organization administration (`org:admin`).
    pub org_admin: bool,
    /// The request is a browser session from a trusted browser
    /// (`require_browser_session`, g5.security.browser-session-actions).
    pub browser_session: bool,
}

/// Whether the subject may waive one group of blockers, and if not, why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverrideAllowance {
    Allowed,
    Denied(&'static str),
}

impl OverrideAllowance {
    pub fn is_allowed(self) -> bool {
        matches!(self, Self::Allowed)
    }
}

impl Serialize for OverrideAllowance {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Allowed => json!({"allowed": true}),
            Self::Denied(reason) => json!({"allowed": false, "reason": reason}),
        }
        .serialize(serializer)
    }
}

/// Who may waive each group of override blockers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeOverrideAuthority {
    /// G5's own requirements: approvals and the external-checks policy.
    pub g5: OverrideAllowance,
    /// GitHub's rules on repositories GitHub reports blocked. GitHub then
    /// decides whether the person's own account may bypass them.
    pub github: OverrideAllowance,
}

pub const OVERRIDE_REQUIRES_BROWSER: &str =
    "Merge with override is available only from the G5 web app.";
pub const OVERRIDE_REQUIRES_MERGE_AUTHORITY: &str =
    "Only someone who may merge this pull request can merge it with an override.";
pub const OVERRIDE_REQUIRES_ORG_ADMIN: &str =
    "Only an organization administrator can waive G5's merge requirements.";

/// Who may make each override (g5.workspace.merge.admin-override,
/// g5.git.provider-override). Changing who may override changes this function
/// and its tests alone.
pub fn merge_override_authority(subject: MergeOverrideSubject) -> MergeOverrideAuthority {
    let person = if !subject.browser_session {
        OverrideAllowance::Denied(OVERRIDE_REQUIRES_BROWSER)
    } else if !subject.may_merge {
        OverrideAllowance::Denied(OVERRIDE_REQUIRES_MERGE_AUTHORITY)
    } else {
        OverrideAllowance::Allowed
    };
    let g5 = match person {
        OverrideAllowance::Allowed if !subject.org_admin => {
            OverrideAllowance::Denied(OVERRIDE_REQUIRES_ORG_ADMIN)
        }
        other => other,
    };
    MergeOverrideAuthority { g5, github: person }
}

/// Recheck ordinary merge authority, then apply the override policy. Preview
/// uses this momentary decision; execution also retains the requirement-set
/// lock through `require_current_override_authority`.
pub async fn current_merge_override_authority(
    db: &mut g5_authz::ScopedDb,
    source_workspace_id: Uuid,
    target_workspace_id: Uuid,
    browser_session: bool,
) -> Result<MergeOverrideAuthority, G5Error> {
    use crate::db::scoped::{workspace_access_for_role_with, AccessHold, WorkspaceAccessResult};
    use crate::services::{workspace_mutation_policy, PermissionService};

    let actor_id = db.actor_id().into_uuid();
    let org_id = db
        .primary_org_id()
        .map_err(|error| crate::errors::AppError::from(error).inner)?
        .into_uuid();
    let role = PermissionService::require_active_member_on_conn(db.conn(), actor_id, org_id, false)
        .await?;
    let org_admin = current_scope(db, org_id, None, "org:admin").await?;
    let accessible = workspace_access_for_role_with(
        db.conn(),
        &role.to_string(),
        actor_id,
        org_id,
        target_workspace_id,
        AccessHold::UntilCommit,
    )
    .await?
        == WorkspaceAccessResult::Allowed;
    let may_merge =
        if !accessible {
            false
        } else if org_admin
            && workspace_mutation_policy::base_admin_subject_org(db, target_workspace_id, None)
                .await?
                .is_some()
        {
            true
        } else {
            let target_owned =
                match workspace_mutation_policy::ensure_workspace_owner(db, target_workspace_id)
                    .await
                {
                    Ok(()) => true,
                    Err(G5Error::Forbidden { .. }) => false,
                    Err(error) => return Err(error),
                };
            if target_owned
                && current_scope(db, org_id, Some(target_workspace_id), "workspaces:write").await?
            {
                true
            } else {
                current_scope(db, org_id, None, "workspaces:write").await?
                    && workspace_mutation_policy::source_workspace_owner_may_merge(
                        db,
                        org_id,
                        target_workspace_id,
                        source_workspace_id,
                        actor_id,
                    )
                    .await?
            }
        };
    Ok(merge_override_authority(MergeOverrideSubject {
        may_merge,
        org_admin,
        browser_session,
    }))
}

async fn current_scope(
    db: &mut g5_authz::ScopedDb,
    org_id: Uuid,
    workspace_id: Option<Uuid>,
    scope: &str,
) -> Result<bool, G5Error> {
    let actor_id = db.actor_id().into_uuid();
    match crate::services::PermissionService::require_user_scopes_on_conn(
        db.conn(),
        actor_id,
        org_id,
        workspace_id,
        &[scope],
    )
    .await
    {
        Ok(()) => Ok(true),
        Err(G5Error::Forbidden { .. }) => Ok(false),
        Err(error) => Err(error),
    }
}

/// Re-admit a confirmed override in each transaction that records or consumes
/// it. Hold current membership, permissions, audience, ownership and (for the
/// source-owner path) the attached requirement until the effect commits.
/// Provider-accepted recovery completes its recorded admission instead.
pub(super) async fn require_current_override_authority(
    admission: &dyn super::workspace_merge_service::MergeAdmissionGate,
    db: &mut g5_authz::ScopedDb,
    source_workspace_id: Uuid,
    target_workspace_id: Uuid,
    record: &MergeOverrideRecord,
) -> Result<(), G5Error> {
    let org_id = db
        .primary_org_id()
        .map_err(|error| crate::errors::AppError::from(error).inner)?
        .into_uuid();
    admission
        .lock_requirement_set(db, org_id, target_workspace_id)
        .await?;
    let authority =
        current_merge_override_authority(db, source_workspace_id, target_workspace_id, true)
            .await?;
    let allowance = if record.actor_id != db.actor_id().into_uuid() {
        OverrideAllowance::Denied("Only the person who confirmed this override may execute it.")
    } else if record.waives_g5() {
        authority.g5
    } else {
        authority.github
    };
    if let OverrideAllowance::Denied(reason) = allowance {
        return Err(G5Error::ConflictDetails {
            reason: reason.to_string(),
            details: json!({"code": OVERRIDE_NOT_AUTHORIZED}),
        });
    }
    Ok(())
}

impl MergeOverrideAuthority {
    /// Refuses unless the subject may waive every group `blockers` holds.
    pub fn require_for(&self, blockers: &OverrideBlockers) -> Result<(), G5Error> {
        for (present, allowance) in [
            (blockers.waives_g5(), self.g5),
            (blockers.waives_github(), self.github),
        ] {
            if let (true, OverrideAllowance::Denied(reason)) = (present, allowance) {
                return Err(G5Error::ConflictDetails {
                    reason: reason.to_string(),
                    details: json!({"code": OVERRIDE_NOT_AUTHORIZED}),
                });
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Blockers
// ---------------------------------------------------------------------------

/// `details.code` of a refused override whose blockers changed since they
/// were shown; `details` carries the current set.
pub const OVERRIDE_CHANGED: &str = "merge_override_changed";
/// `details.code` of an override with nothing left to waive.
pub const OVERRIDE_NOT_NEEDED: &str = "merge_override_not_needed";
/// `details.code` of an override the caller may not make.
pub const OVERRIDE_NOT_AUTHORIZED: &str = "merge_override_not_authorized";

/// An attached approval requirement the merge admission found unmet.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WaivedApproval {
    pub definition_id: Uuid,
    pub definition_name: String,
    pub definition_version: i32,
    /// `pending` or `rejected`, as the admission evaluated it.
    pub state: String,
}

/// One check row that blocks under the organization's external-checks
/// policy.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WaivedCheck {
    pub name: String,
    /// GitHub's bucket: `fail`, `cancel` or `pending`.
    pub bucket: String,
    pub required: bool,
}

/// A repository whose checks block under the organization's policy.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WaivedCheckRepository {
    pub repository: String,
    #[serde(default)]
    pub head_ref_oid: Option<String>,
    /// `ready`, or `pending`/`unavailable` when the checks could not be read.
    pub availability: String,
    pub checks: Vec<WaivedCheck>,
}

/// A repository GitHub reports blocked by its branch protection or rulesets
/// (g5.git.provider-rules).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockedProviderRepository {
    pub repo_id: Uuid,
    pub repository: String,
    pub head_sha: String,
    /// GitHub's `mergeable_state`.
    pub merge_state: String,
}

/// Everything an override would waive, grouped by the authority that may
/// waive it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OverrideBlockers {
    pub approvals: Vec<WaivedApproval>,
    pub checks: Vec<WaivedCheckRepository>,
    pub github: Vec<BlockedProviderRepository>,
}

impl OverrideBlockers {
    pub fn new(
        mut approvals: Vec<WaivedApproval>,
        mut checks: Vec<WaivedCheckRepository>,
        mut github: Vec<BlockedProviderRepository>,
    ) -> Self {
        approvals.sort();
        for repository in &mut checks {
            repository.checks.sort();
        }
        checks.sort();
        github.sort();
        Self {
            approvals,
            checks,
            github,
        }
    }

    pub fn is_empty(&self) -> bool {
        !self.waives_g5() && !self.waives_github()
    }

    pub fn waives_g5(&self) -> bool {
        !self.approvals.is_empty() || !self.checks.is_empty()
    }

    pub fn waives_github(&self) -> bool {
        !self.github.is_empty()
    }

    /// Identity of the shown set: these blockers for this source version. A
    /// push after the set was shown changes it even when the blockers read
    /// the same, because check results belong to a head.
    pub fn digest(&self, source_heads: &[MergeSourceHead]) -> String {
        let mut heads: Vec<(&str, &str)> = source_heads
            .iter()
            .map(|head| (head.repo_id.as_str(), head.head_commit.as_str()))
            .collect();
        heads.sort();
        let canonical = json!({
            "sourceHeads": heads,
            "approvals": self.approvals.iter().map(|approval| json!([
                approval.definition_id,
                approval.definition_version,
                approval.state,
            ])).collect::<Vec<_>>(),
            "checks": self.checks.iter().map(|repository| json!([
                repository.repository.to_ascii_lowercase(),
                repository.head_ref_oid,
                repository.availability,
                repository.checks.iter().map(|check| json!([check.name, check.bucket])).collect::<Vec<_>>(),
            ])).collect::<Vec<_>>(),
            "github": self.github.iter().map(|repository| json!([
                repository.repo_id,
                repository.head_sha,
                repository.merge_state,
            ])).collect::<Vec<_>>(),
        });
        hex::encode(Sha256::digest(canonical.to_string().as_bytes()))
    }
}

/// The unmet requirements of a merge admission evaluation.
pub fn waived_approvals(admission: &MergeAdmissionEvidence) -> Vec<WaivedApproval> {
    admission
        .requirements
        .iter()
        .filter(|requirement| requirement.state != "approved")
        .map(|requirement| WaivedApproval {
            definition_id: requirement.definition_id,
            definition_name: requirement.definition_name.clone(),
            definition_version: requirement.definition_version,
            state: requirement.state.clone(),
        })
        .collect()
}

/// The check rows that block under the organization's external-checks policy,
/// by the same rules as `external_checks_verdict`: any failure, a required
/// check still pending or expected, and a repository whose checks cannot be
/// read. A malformed snapshot is one unreadable entry, never a pass.
pub fn blocking_checks(snapshot: &Value) -> Vec<WaivedCheckRepository> {
    let Some(repositories) = snapshot.get("repositories").and_then(Value::as_array) else {
        return vec![WaivedCheckRepository {
            repository: String::new(),
            head_ref_oid: None,
            availability: "unavailable".to_string(),
            checks: Vec::new(),
        }];
    };
    let text = |value: &Value, key: &str| value.get(key).and_then(Value::as_str).map(str::to_owned);
    repositories
        .iter()
        .filter_map(|repository| {
            let availability = text(repository, "availability").unwrap_or_default();
            let checks: Vec<WaivedCheck> = repository
                .get("checks")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter(|check| {
                    let required = check["required"] == true;
                    match check.get("bucket").and_then(Value::as_str) {
                        Some("fail") | Some("cancel") => true,
                        Some("pending") => required,
                        _ => false,
                    }
                })
                .map(|check| WaivedCheck {
                    name: text(check, "name").unwrap_or_default(),
                    bucket: text(check, "bucket").unwrap_or_default(),
                    required: check["required"] == true,
                })
                .collect();
            (availability != "ready" || !checks.is_empty()).then(|| WaivedCheckRepository {
                repository: text(repository, "repository")
                    .or_else(|| text(repository, "nameWithOwner"))
                    .unwrap_or_default(),
                head_ref_oid: text(repository, "headRefOid"),
                availability,
                checks,
            })
        })
        .collect()
}

/// Whether `actor_id` owns the pull request's source workspace: a
/// self-override, flagged in the record and notifications.
pub async fn owns_source_workspace(
    db: &mut g5_authz::ScopedDb,
    source_workspace_id: Uuid,
    actor_id: Uuid,
) -> Result<bool, G5Error> {
    let owner: Option<Uuid> =
        sqlx::query_scalar("SELECT owner_user_id FROM workspaces WHERE id = $1")
            .bind(source_workspace_id)
            .fetch_optional(db.conn())
            .await?
            .flatten();
    Ok(owner == Some(actor_id))
}

/// The overriding person's name for the record. Names live on the system
/// pool; the tenant scope carries no grant on `users`.
pub async fn actor_display_name(
    system_pool: &sqlx::PgPool,
    actor_id: Uuid,
) -> Result<Option<String>, G5Error> {
    Ok(sqlx::query_scalar(
        "SELECT COALESCE(NULLIF(display_name, ''), email) FROM users WHERE id = $1",
    )
    .bind(actor_id)
    .fetch_optional(system_pool)
    .await?)
}

// ---------------------------------------------------------------------------
// Intent and record
// ---------------------------------------------------------------------------

/// What one override confirmation asks the merge to do: the shown set's
/// digest, the reason, and the blockers observed fresh at execution outside
/// the approval admission (checks, GitHub's merge state).
#[derive(Debug, Clone)]
pub struct MergeOverrideIntent {
    pub actor_id: Uuid,
    pub actor_name: Option<String>,
    pub reason: String,
    pub self_override: bool,
    pub expected_digest: String,
    pub authority: MergeOverrideAuthority,
    pub checks: Vec<WaivedCheckRepository>,
    pub github: Vec<BlockedProviderRepository>,
}

/// The durable record of an override, part of the merge admission evidence
/// for the exact cut it admitted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeOverrideRecord {
    pub actor_id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor_name: Option<String>,
    pub at: DateTime<Utc>,
    pub reason: String,
    pub self_override: bool,
    #[serde(default)]
    pub waived_approvals: Vec<WaivedApproval>,
    #[serde(default)]
    pub waived_checks: Vec<WaivedCheckRepository>,
    #[serde(default)]
    pub github_repositories: Vec<BlockedProviderRepository>,
}

impl MergeOverrideRecord {
    pub fn waives_g5(&self) -> bool {
        !self.waived_approvals.is_empty() || !self.waived_checks.is_empty()
    }

    /// Whether the record lets `actor_id` merge `repo_id` past GitHub's rules.
    /// Only the person who made the override, under their own identity.
    pub fn bypasses_github(&self, actor_id: Uuid, repo_id: Uuid) -> bool {
        self.actor_id == actor_id
            && self
                .github_repositories
                .iter()
                .any(|repository| repository.repo_id == repo_id)
    }
}

/// Turn a fresh admission evaluation into an admitted override, or refuse
/// with no effect: the blockers (with this cut's source heads) must equal the
/// shown set, something must be left to waive, and the intent's authority
/// must cover every group present.
pub fn apply_override(
    admission: &mut MergeAdmissionEvidence,
    intent: &MergeOverrideIntent,
) -> Result<(), G5Error> {
    let approvals = waived_approvals(admission);
    if !admission.admitted && approvals.is_empty() {
        // Refused for a reason that is not an unmet requirement, such as a
        // destination other than the configured target: not overridable.
        return Err(G5Error::Conflict {
            reason: admission
                .reason
                .clone()
                .unwrap_or_else(|| "Merge admission refused".to_string()),
        });
    }
    let blockers = OverrideBlockers::new(approvals, intent.checks.clone(), intent.github.clone());
    let digest = blockers.digest(&admission.source_heads);
    if digest != intent.expected_digest {
        return Err(G5Error::ConflictDetails {
            reason: "What this override would waive has changed. Review the current list and confirm again.".to_string(),
            details: json!({
                "code": OVERRIDE_CHANGED,
                "blockers": blockers,
                "sourceHeads": admission.source_heads,
                "digest": digest,
            }),
        });
    }
    if blockers.is_empty() {
        return Err(G5Error::ConflictDetails {
            reason: "Nothing blocks this merge any more; merge it without an override.".to_string(),
            details: json!({"code": OVERRIDE_NOT_NEEDED}),
        });
    }
    intent.authority.require_for(&blockers)?;
    admission.admitted = true;
    admission.reason = None;
    admission.merge_override = Some(MergeOverrideRecord {
        actor_id: intent.actor_id,
        actor_name: intent.actor_name.clone(),
        at: Utc::now(),
        reason: intent.reason.clone(),
        self_override: intent.self_override,
        waived_approvals: blockers.approvals,
        waived_checks: blockers.checks,
        github_repositories: blockers.github,
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subject(may_merge: bool, org_admin: bool, browser_session: bool) -> MergeOverrideSubject {
        MergeOverrideSubject {
            may_merge,
            org_admin,
            browser_session,
        }
    }

    #[test]
    fn authority_needs_a_browser_session_and_merge_authority_and_admin_for_g5() {
        let admin = merge_override_authority(subject(true, true, true));
        assert_eq!(admin.g5, OverrideAllowance::Allowed);
        assert_eq!(admin.github, OverrideAllowance::Allowed);

        let member = merge_override_authority(subject(true, false, true));
        assert_eq!(
            member.g5,
            OverrideAllowance::Denied(OVERRIDE_REQUIRES_ORG_ADMIN)
        );
        assert_eq!(member.github, OverrideAllowance::Allowed);

        let cli_admin = merge_override_authority(subject(true, true, false));
        assert_eq!(
            cli_admin.g5,
            OverrideAllowance::Denied(OVERRIDE_REQUIRES_BROWSER)
        );
        assert_eq!(
            cli_admin.github,
            OverrideAllowance::Denied(OVERRIDE_REQUIRES_BROWSER)
        );

        let outsider_admin = merge_override_authority(subject(false, true, true));
        assert_eq!(
            outsider_admin.g5,
            OverrideAllowance::Denied(OVERRIDE_REQUIRES_MERGE_AUTHORITY)
        );
        assert_eq!(
            outsider_admin.github,
            OverrideAllowance::Denied(OVERRIDE_REQUIRES_MERGE_AUTHORITY)
        );
    }

    fn head(repo: &str, commit: &str) -> MergeSourceHead {
        MergeSourceHead {
            repo_id: repo.to_string(),
            branch_name: "main".to_string(),
            workspace_id: Uuid::nil().to_string(),
            head_commit: commit.to_string(),
        }
    }

    fn admission(states: &[&str]) -> MergeAdmissionEvidence {
        MergeAdmissionEvidence {
            schema_version: super::super::workspace_merge_service::MERGE_ADMISSION_SCHEMA_VERSION,
            evaluated_at: Utc::now(),
            source_workspace_id: Uuid::nil(),
            target_workspace_id: Uuid::nil(),
            source_heads: vec![head("r1", "a")],
            admitted: states.iter().all(|state| *state == "approved"),
            reason: Some("Approval required".to_string()),
            requirements: states
                .iter()
                .enumerate()
                .map(|(index, state)| {
                    super::super::workspace_merge_service::MergeAdmissionRequirement {
                        definition_id: Uuid::from_u128(index as u128 + 1),
                        definition_name: format!("Req {index}"),
                        definition_version: 1,
                        state: state.to_string(),
                    }
                })
                .collect(),
            merge_override: None,
        }
    }

    fn intent(digest: String, authority: MergeOverrideAuthority) -> MergeOverrideIntent {
        MergeOverrideIntent {
            actor_id: Uuid::from_u128(9),
            actor_name: Some("Ada".to_string()),
            reason: "Approver away".to_string(),
            self_override: false,
            expected_digest: digest,
            authority,
            checks: Vec::new(),
            github: Vec::new(),
        }
    }

    #[test]
    fn apply_override_admits_exactly_the_shown_set() {
        let admin = merge_override_authority(subject(true, true, true));
        let mut evaluated = admission(&["approved", "pending", "rejected"]);
        let shown = OverrideBlockers::new(waived_approvals(&evaluated), Vec::new(), Vec::new());
        let digest = shown.digest(&evaluated.source_heads);
        apply_override(&mut evaluated, &intent(digest, admin)).unwrap();
        assert!(evaluated.admitted);
        assert_eq!(evaluated.reason, None);
        let record = evaluated.merge_override.unwrap();
        assert_eq!(record.waived_approvals.len(), 2);
        assert!(record.waives_g5());
        assert_eq!(record.reason, "Approver away");
    }

    #[test]
    fn apply_override_refuses_a_changed_set_or_source_version_without_effect() {
        let admin = merge_override_authority(subject(true, true, true));
        let shown_admission = admission(&["pending"]);
        let digest =
            OverrideBlockers::new(waived_approvals(&shown_admission), Vec::new(), Vec::new())
                .digest(&shown_admission.source_heads);

        let mut now_rejected = admission(&["rejected"]);
        let error = apply_override(&mut now_rejected, &intent(digest.clone(), admin)).unwrap_err();
        assert!(
            matches!(&error, G5Error::ConflictDetails { details, .. } if details["code"] == OVERRIDE_CHANGED)
        );
        assert!(!now_rejected.admitted);
        assert!(now_rejected.merge_override.is_none());

        let mut pushed = admission(&["pending"]);
        pushed.source_heads = vec![head("r1", "b")];
        assert!(apply_override(&mut pushed, &intent(digest, admin)).is_err());
        assert!(!pushed.admitted);
    }

    #[test]
    fn apply_override_refuses_nothing_to_waive_and_missing_authority() {
        let admin = merge_override_authority(subject(true, true, true));
        let mut clear = admission(&["approved"]);
        let digest = OverrideBlockers::default().digest(&clear.source_heads);
        let error = apply_override(&mut clear, &intent(digest, admin)).unwrap_err();
        assert!(
            matches!(&error, G5Error::ConflictDetails { details, .. } if details["code"] == OVERRIDE_NOT_NEEDED)
        );

        let member = merge_override_authority(subject(true, false, true));
        let mut pending = admission(&["pending"]);
        let digest = OverrideBlockers::new(waived_approvals(&pending), Vec::new(), Vec::new())
            .digest(&pending.source_heads);
        let error = apply_override(&mut pending, &intent(digest, member)).unwrap_err();
        assert!(
            matches!(&error, G5Error::ConflictDetails { details, .. } if details["code"] == OVERRIDE_NOT_AUTHORIZED)
        );
        assert!(!pending.admitted);

        // A refusal that is not an unmet requirement is not overridable.
        let mut elsewhere = admission(&[]);
        elsewhere.admitted = false;
        elsewhere.reason = Some("Merge admission evaluates the configured merge target".into());
        let digest = OverrideBlockers::default().digest(&elsewhere.source_heads);
        let error = apply_override(&mut elsewhere, &intent(digest, admin)).unwrap_err();
        assert!(
            matches!(&error, G5Error::Conflict { reason } if reason.contains("configured merge target"))
        );
        assert!(!elsewhere.admitted);
    }

    #[test]
    fn blocking_checks_follow_the_policy_verdict_rules() {
        let snapshot = json!({"repositories": [
            {"repository": "o/a", "availability": "ready", "headRefOid": "h", "checks": [
                {"name": "lint", "bucket": "fail", "required": false},
                {"name": "build", "bucket": "pending", "required": true},
                {"name": "docs", "bucket": "pending", "required": false},
                {"name": "unit", "bucket": "pass", "required": true}
            ]},
            {"repository": "o/b", "availability": "ready", "checks": [
                {"name": "unit", "bucket": "pass", "required": true}
            ]},
            {"repository": "o/c", "availability": "unavailable", "checks": []}
        ]});
        let blocking = blocking_checks(&snapshot);
        assert_eq!(blocking.len(), 2);
        assert_eq!(blocking[0].repository, "o/a");
        let names: Vec<_> = blocking[0].checks.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["lint", "build"]);
        assert_eq!(blocking[1].availability, "unavailable");
        assert_eq!(blocking_checks(&json!({})).len(), 1);
    }

    #[test]
    fn github_bypass_is_only_for_the_overriding_person_and_waived_repositories() {
        let repo = Uuid::from_u128(5);
        let record = MergeOverrideRecord {
            actor_id: Uuid::from_u128(9),
            actor_name: None,
            at: Utc::now(),
            reason: "r".to_string(),
            self_override: false,
            waived_approvals: Vec::new(),
            waived_checks: Vec::new(),
            github_repositories: vec![BlockedProviderRepository {
                repo_id: repo,
                repository: "o/a".to_string(),
                head_sha: "h".to_string(),
                merge_state: "blocked".to_string(),
            }],
        };
        assert!(record.bypasses_github(Uuid::from_u128(9), repo));
        assert!(!record.bypasses_github(Uuid::from_u128(8), repo));
        assert!(!record.bypasses_github(Uuid::from_u128(9), Uuid::from_u128(6)));
        assert!(!record.waives_g5());
    }
}
