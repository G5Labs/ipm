//! Issue labels through the one issue service (`g5.issue.labels.contract`):
//! a domain's label catalog and the labels on an issue, for every tracker.
//!
//! The `*_as` operations are shared by people (the routes here and the issue
//! page) and by a workspace's agent acting as its owner
//! (`workspace_agent.rs`). Each validates the request centrally, applies the
//! write rules of the caller (a review copy changes nothing), and sends the
//! operation to the domain's adapter: G5's own store in the caller's tenant
//! transaction (`g5.issue.labels.native`), or GitHub under the person's
//! connected account after the comment-publication admission
//! (`g5.issue.labels.github`). No transaction is held across GitHub I/O.

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, patch, post},
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::db::scoped::scoped_db_for_org;
use crate::errors::AppError;
use crate::middleware::AuthContext;
use crate::routes::audited::AuditedRouter;
use crate::routes::external_issues::{
    admit_person_write, comment_token_manager, ensure_same_issue, ensure_writable_issues,
    github_client, github_page_target, issue_error, native_page_db, target_location, CommentTarget,
    IssueChange, IssueEditDTO, IssueWriter, PersonWrite,
};
use crate::routes::github_app::mask_foreign_org_access;
use crate::routes::ResponseEnvelope;
use crate::services::external_issues::{
    domain_issue, domain_label, domain_label_by_id, domain_label_page, domain_labels_named,
    ensure_external_issues_enabled, label_dto, list_domain_access, summary, ExternalIssueLabelDTO,
    IssueDomainAccess, LabelScan, LinkedLocation,
};
use crate::services::github_issues::{
    self, GithubIssueLabel, IssueError, LabelEdit, LabelRemoveOutcome, LabelWriteOutcome,
    LabelsAddOutcome,
};
use crate::services::issue_creations::screen;
use crate::services::issue_labels::{
    carries, label_exists, matches_new, unknown_labels, validate_change, validate_new_label,
    validate_patch, IssueLabelChangeDTO, LabelChange, LabelChangeOutcomeDTO, LabelPageDTO,
    LabelPatch, LabelSelector, LabelWriteDTO, NewLabel, MAX_LABEL_SCAN_PAGES,
};
use crate::services::native_issues::{self, NativeIssueChange, NativeReader};
use crate::services::AppState;
use g5_auth::Scope;
use g5_contracts::error::{ExternalIssueErrorKind, G5Error};

const MEMBER_READ_SCOPE: &[&str] = &["workspaces:read"];
const MEMBER_WRITE_SCOPE: &[&str] = &["workspaces:write"];

/// Who changes a domain's catalog, from which workspace if any.
pub(crate) struct LabelWriter<'a> {
    pub org_id: Uuid,
    pub workspace_id: Option<Uuid>,
    pub actor: Uuid,
    pub access: &'a IssueDomainAccess,
}

fn native_reader<'a>(
    state: &'a AppState,
    access: &'a IssueDomainAccess,
    site: &'a native_issues::NativeSite,
) -> NativeReader<'a> {
    NativeReader {
        domain: &access.domain,
        site,
        system_pool: &state.system_pool,
    }
}

fn not_found(selector: &LabelSelector, access: &IssueDomainAccess) -> AppError {
    AppError::from_error(G5Error::NotFound {
        resource: format!("{} in {}", selector.describe(), access.domain.display_key),
    })
}

/// The GitHub label a selector names, read with the installation token: by
/// name directly, by id through at most [`MAX_LABEL_SCAN_PAGES`] catalog
/// pages.
async fn github_label(
    state: &AppState,
    access: &IssueDomainAccess,
    selector: &LabelSelector,
) -> Result<GithubIssueLabel, AppError> {
    let client = github_client(state)?;
    match selector {
        LabelSelector::Name(name) => domain_label(client, access, name)
            .await
            .map_err(issue_error)?
            .ok_or_else(|| not_found(selector, access)),
        LabelSelector::Id(id) => {
            let Ok(id) = id.parse::<u64>() else {
                return Err(not_found(selector, access));
            };
            match domain_label_by_id(client, access, id, MAX_LABEL_SCAN_PAGES)
                .await
                .map_err(issue_error)?
            {
                LabelScan::Found(label) => Ok(label),
                LabelScan::Missing => Err(not_found(selector, access)),
                LabelScan::TooLarge => Err(AppError::from_error(G5Error::ValidationError {
                    details: serde_json::json!({
                        "field": "label",
                        "code": "label_scan_limit",
                        "error": format!(
                            "{} has more than {} labels; name the label instead of its id",
                            access.domain.display_key,
                            u64::from(MAX_LABEL_SCAN_PAGES) * 100
                        ),
                    }),
                })),
            }
        }
    }
}

/// One page (1-based) of a domain's catalog. `db` is a read scope for the
/// organization; it is committed before GitHub is read.
pub(crate) async fn list_labels_as(
    state: &AppState,
    mut db: g5_authz::ScopedDb,
    access: &IssueDomainAccess,
    page: u32,
) -> Result<LabelPageDTO, AppError> {
    let page = page.max(1);
    if access.is_native() {
        let listed = native_issues::labels::list(&mut db, &access.domain, page).await?;
        db.commit().await.map_err(AppError::from)?;
        return Ok(listed);
    }
    db.commit().await.map_err(AppError::from)?;
    let (labels, more) = domain_label_page(github_client(state)?, access, page)
        .await
        .map_err(issue_error)?;
    Ok(LabelPageDTO {
        domain_id: access.domain.id,
        display_key: access.domain.display_key.clone(),
        labels: labels.iter().map(label_dto).collect(),
        next_page: more.then_some(page + 1),
    })
}

/// The GitHub admission for a catalog write: the comment switch, a
/// non-public repository and the actor's connected account, with supplied
/// text screened for secrets.
async fn admit_github_catalog_write(
    state: &AppState,
    access: &IssueDomainAccess,
    actor: Uuid,
    name: Option<&str>,
    description: Option<&str>,
) -> Result<(String, String), AppError> {
    const ACTION: &str = "saving the label";
    if let Some(name) = name {
        screen("name", name, ACTION)?;
    }
    if let Some(description) = description {
        screen("description", description, ACTION)?;
    }
    let acting = admit_person_write(
        github_client(state)?,
        comment_token_manager(state)?,
        &state.system_pool,
        access,
        actor,
        PersonWrite::Labels,
    )
    .await?;
    Ok((acting.token, acting.github_login))
}

fn written(label: ExternalIssueLabelDTO, github_login: Option<String>) -> LabelWriteDTO {
    LabelWriteDTO {
        outcome: "applied",
        label: Some(label),
        github_login,
        message: None,
    }
}

fn unknown(github_login: String, message: String) -> LabelWriteDTO {
    LabelWriteDTO {
        outcome: "unknown",
        label: None,
        github_login: Some(github_login),
        message: Some(message),
    }
}

/// Add a label to a domain's catalog (`g5.issue.labels.contract`). A name
/// already taken returns that label when it matches the request, so a
/// retried create is harmless; otherwise the conflict carries it.
pub(crate) async fn create_label_as(
    state: &AppState,
    mut db: g5_authz::ScopedDb,
    writer: &LabelWriter<'_>,
    new: &NewLabel,
) -> Result<LabelWriteDTO, AppError> {
    let access = writer.access;
    ensure_writable_issues(&mut db, writer.org_id, writer.workspace_id, "Create label").await?;
    if access.is_native() {
        let label =
            native_issues::labels::create(&mut db, writer.org_id, &access.domain, new).await?;
        db.commit().await.map_err(AppError::from)?;
        return Ok(written(label, None));
    }
    db.commit().await.map_err(AppError::from)?;
    let (token, login) = admit_github_catalog_write(
        state,
        access,
        writer.actor,
        Some(&new.name),
        Some(&new.description),
    )
    .await?;
    let client = github_client(state)?;
    let repository_id = access.github_repository_id().map_err(issue_error)?;
    let outcome = github_issues::create_label(
        client,
        &token,
        repository_id,
        &new.name,
        new.color.as_deref(),
        &new.description,
    )
    .await;
    let result = match outcome {
        LabelWriteOutcome::Written(label) => written(label_dto(&label), Some(login)),
        // GitHub refuses a taken name (422): reconcile by reading it.
        LabelWriteOutcome::Rejected(error)
            if error.kind == ExternalIssueErrorKind::InvalidReference =>
        {
            match domain_label(client, access, &new.name)
                .await
                .map_err(issue_error)?
            {
                Some(existing) if matches_new(&label_dto(&existing), new) => {
                    written(label_dto(&existing), Some(login))
                }
                Some(existing) => {
                    return Err(AppError::from_error(label_exists(&label_dto(&existing))))
                }
                None => return Err(issue_error(error)),
            }
        }
        LabelWriteOutcome::Rejected(error) => return Err(issue_error(error)),
        LabelWriteOutcome::Unknown(message) => unknown(login, message),
    };
    tracing::info!(
        org_id = %writer.org_id,
        workspace_id = ?writer.workspace_id,
        domain = %access.domain.display_key,
        actor = %writer.actor,
        outcome = result.outcome,
        "issue label created"
    );
    Ok(result)
}

/// Rename, recolour or redescribe a catalog label: last writer wins.
pub(crate) async fn edit_label_as(
    state: &AppState,
    mut db: g5_authz::ScopedDb,
    writer: &LabelWriter<'_>,
    selector: &LabelSelector,
    patch: &LabelPatch,
) -> Result<LabelWriteDTO, AppError> {
    let access = writer.access;
    ensure_writable_issues(&mut db, writer.org_id, writer.workspace_id, "Edit label").await?;
    if access.is_native() {
        let label = native_issues::labels::edit(&mut db, &access.domain, selector, patch).await?;
        db.commit().await.map_err(AppError::from)?;
        return Ok(written(label, None));
    }
    db.commit().await.map_err(AppError::from)?;
    let (token, login) = admit_github_catalog_write(
        state,
        access,
        writer.actor,
        patch.name.as_deref(),
        patch.description.as_deref(),
    )
    .await?;
    let current = github_label(state, access, selector).await?;
    let client = github_client(state)?;
    if let Some(name) = &patch.name {
        if !name.eq_ignore_ascii_case(&current.name) {
            if let Some(other) = domain_label(client, access, name)
                .await
                .map_err(issue_error)?
            {
                return Err(AppError::from_error(label_exists(&label_dto(&other))));
            }
        }
    }
    let repository_id = access.github_repository_id().map_err(issue_error)?;
    let outcome = github_issues::update_label(
        client,
        &token,
        repository_id,
        &current.name,
        &LabelEdit {
            name: patch.name.clone(),
            color: patch.color.clone(),
            description: patch.description.clone(),
        },
    )
    .await;
    let result = match outcome {
        LabelWriteOutcome::Written(label) => written(label_dto(&label), Some(login)),
        LabelWriteOutcome::Rejected(error) => return Err(issue_error(error)),
        LabelWriteOutcome::Unknown(message) => unknown(login, message),
    };
    tracing::info!(
        org_id = %writer.org_id,
        workspace_id = ?writer.workspace_id,
        domain = %access.domain.display_key,
        actor = %writer.actor,
        outcome = result.outcome,
        "issue label edited"
    );
    Ok(result)
}

/// Delete a catalog label, which leaves every issue too. Only an explicit
/// delete does this; no issue edit deletes a catalog label.
pub(crate) async fn delete_label_as(
    state: &AppState,
    mut db: g5_authz::ScopedDb,
    writer: &LabelWriter<'_>,
    selector: &LabelSelector,
) -> Result<LabelWriteDTO, AppError> {
    let access = writer.access;
    ensure_writable_issues(&mut db, writer.org_id, writer.workspace_id, "Delete label").await?;
    if access.is_native() {
        let label = native_issues::labels::delete(
            &mut db,
            writer.org_id,
            &access.domain,
            selector,
            writer.actor,
        )
        .await?;
        db.commit().await.map_err(AppError::from)?;
        return Ok(written(label, None));
    }
    db.commit().await.map_err(AppError::from)?;
    let (token, login) =
        admit_github_catalog_write(state, access, writer.actor, None, None).await?;
    let current = github_label(state, access, selector).await?;
    let repository_id = access.github_repository_id().map_err(issue_error)?;
    let outcome =
        github_issues::delete_label(github_client(state)?, &token, repository_id, &current.name)
            .await;
    let result = match outcome {
        // Gone either way: someone may have deleted it first.
        LabelRemoveOutcome::Removed(_) | LabelRemoveOutcome::Absent => {
            written(label_dto(&current), Some(login))
        }
        LabelRemoveOutcome::Rejected(error) => return Err(issue_error(error)),
        LabelRemoveOutcome::Unknown(message) => unknown(login, message),
    };
    tracing::info!(
        org_id = %writer.org_id,
        workspace_id = ?writer.workspace_id,
        domain = %access.domain.display_key,
        actor = %writer.actor,
        outcome = result.outcome,
        "issue label deleted"
    );
    Ok(result)
}

fn outcome(label: &str, change: &'static str, outcome: &'static str) -> LabelChangeOutcomeDTO {
    LabelChangeOutcomeDTO {
        label: label.to_string(),
        change,
        outcome,
        message: None,
    }
}

/// Add and remove labels on `target` as the writer's actor
/// (`g5.issue.labels.contract`). Natively one transaction applies the
/// change whole. On GitHub every named label must be in the catalog, then
/// one request adds labels and one request removes each; the first refusal
/// or unclear answer stops the sequence, leaving the rest not attempted. A
/// refusal before anything applied is returned as the error. The issue is
/// read again for the labels it then has; a failed read keeps the known
/// outcomes.
pub(crate) async fn change_issue_labels_as(
    state: &Arc<AppState>,
    mut db: g5_authz::ScopedDb,
    writer: &IssueWriter<'_>,
    target: &CommentTarget,
    change: &LabelChange,
) -> Result<IssueLabelChangeDTO, AppError> {
    let IssueWriter {
        org_id,
        workspace_id,
        actor,
        domains,
    } = *writer;
    ensure_writable_issues(&mut db, org_id, workspace_id, "Change labels").await?;
    let (access, location) = target_location(domains, target)?;
    let repository_id = match location {
        LinkedLocation::Native { issue_id } => {
            let site =
                native_issues::native_site(&state.system_pool, &state.external_url, org_id).await?;
            let reader = native_reader(state, access, &site);
            let issue = native_issues::labels::change_issue_labels(
                &mut db, &reader, org_id, issue_id, actor, change,
            )
            .await?;
            db.commit().await.map_err(AppError::from)?;
            tracing::info!(
                workspace_id = ?workspace_id,
                issue = %target.display_key,
                actor = %actor,
                "native issue labels changed"
            );
            return Ok(IssueLabelChangeDTO::applied(change, issue));
        }
        LinkedLocation::Github { repository_id } => repository_id,
    };
    db.commit().await.map_err(AppError::from)?;
    let client = github_client(state)?;
    let acting = admit_person_write(
        client,
        comment_token_manager(state)?,
        &state.system_pool,
        access,
        actor,
        PersonWrite::Labels,
    )
    .await?;
    // GitHub's add creates a label it does not know, so every named label
    // must be in the catalog first; names are sent as the catalog has them.
    let named: Vec<String> = change.add.iter().chain(&change.remove).cloned().collect();
    let mut known = Vec::new();
    let mut missing = Vec::new();
    for (name, found) in named.iter().zip(
        domain_labels_named(client, access, &named)
            .await
            .map_err(issue_error)?,
    ) {
        match found {
            Some(label) => known.push(label.name),
            None => missing.push(name.clone()),
        }
    }
    if !missing.is_empty() {
        return Err(AppError::from_error(unknown_labels(
            "labels",
            &missing,
            &access.domain.display_key,
        )));
    }
    let (add, remove) = known.split_at(change.add.len());
    let current = domain_issue(client, access, target.number)
        .await
        .map_err(issue_error)?;
    ensure_same_issue(target, &current)?;

    let mut changes: Vec<LabelChangeOutcomeDTO> = add
        .iter()
        .map(|name| outcome(name, "add", "not_attempted"))
        .chain(
            remove
                .iter()
                .map(|name| outcome(name, "remove", "not_attempted")),
        )
        .collect();
    let mut stopped: Option<IssueError> = None;
    let mut unclear = None;
    if !add.is_empty() {
        match github_issues::add_issue_labels(
            client,
            &acting.token,
            repository_id,
            target.number,
            add,
        )
        .await
        {
            LabelsAddOutcome::Added(labels) => {
                let shown: Vec<ExternalIssueLabelDTO> = labels.iter().map(label_dto).collect();
                for entry in changes.iter_mut().filter(|entry| entry.change == "add") {
                    if carries(&shown, &entry.label) {
                        entry.outcome = "applied";
                    } else {
                        entry.outcome = "refused";
                        entry.message = Some("GitHub's answer does not show the label".to_string());
                    }
                }
            }
            LabelsAddOutcome::Rejected(error) => stopped = Some(error),
            LabelsAddOutcome::Unknown(message) => {
                for entry in changes.iter_mut().filter(|entry| entry.change == "add") {
                    entry.outcome = "unknown";
                }
                unclear = Some(message);
            }
        }
    }
    if stopped.is_none() && unclear.is_none() {
        for (index, name) in remove.iter().enumerate() {
            let entry = add.len() + index;
            match github_issues::remove_issue_label(
                client,
                &acting.token,
                repository_id,
                target.number,
                name,
            )
            .await
            {
                LabelRemoveOutcome::Removed(_) | LabelRemoveOutcome::Absent => {
                    changes[entry].outcome = "applied";
                }
                LabelRemoveOutcome::Rejected(error) => {
                    stopped = Some(error);
                    break;
                }
                LabelRemoveOutcome::Unknown(message) => {
                    changes[entry].outcome = "unknown";
                    unclear = Some(message);
                    break;
                }
            }
        }
    }
    if let Some(error) = stopped {
        if changes.iter().all(|entry| entry.outcome == "not_attempted") {
            return Err(issue_error(error));
        }
        if let Some(entry) = changes
            .iter_mut()
            .find(|entry| entry.outcome == "not_attempted")
        {
            entry.outcome = "refused";
            entry.message = Some(error.message.clone());
        }
    }
    let overall = IssueLabelChangeDTO::overall(&changes);
    let (issue, read_failure) = match domain_issue(client, access, target.number).await {
        Ok(issue) => (Some(summary(access, repository_id, &issue)), None),
        Err(error) => (None, Some(error.message)),
    };
    let message = match (overall, unclear, read_failure) {
        (_, Some(message), _) => Some(format!(
            "GitHub did not confirm every change ({message}); read the issue before trying again"
        )),
        ("partial", None, _) => Some(
            "GitHub refused part of the change; the owner's account may not be allowed to change labels here"
                .to_string(),
        ),
        (_, None, Some(failure)) => Some(format!(
            "the change was made, but the issue could not be read again: {failure}"
        )),
        _ => None,
    };
    tracing::info!(
        workspace_id = ?workspace_id,
        issue = %target.display_key,
        actor = %actor,
        outcome = overall,
        "issue labels changed"
    );
    Ok(IssueLabelChangeDTO {
        outcome: overall,
        changes,
        issue,
        github_login: Some(acting.github_login),
        message,
    })
}

/// Refuse a change naming a label `target`'s GitHub domain lacks, before
/// anything is written: the check an edit that also changes fields runs
/// first, so an unknown label refuses it whole. Natively the one
/// transaction refuses it instead.
pub(crate) async fn ensure_labels_exist(
    state: &AppState,
    writer: &IssueWriter<'_>,
    target: &CommentTarget,
    change: &LabelChange,
) -> Result<(), AppError> {
    let (access, location) = target_location(writer.domains, target)?;
    if matches!(location, LinkedLocation::Native { .. }) {
        return Ok(());
    }
    let named: Vec<String> = change.add.iter().chain(&change.remove).cloned().collect();
    let found = domain_labels_named(github_client(state)?, access, &named)
        .await
        .map_err(issue_error)?;
    let missing: Vec<String> = named
        .into_iter()
        .zip(found)
        .filter_map(|(name, found)| found.is_none().then_some(name))
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(AppError::from_error(unknown_labels(
            "labels",
            &missing,
            &access.domain.display_key,
        )))
    }
}

/// Edit a G5 issue's fields and labels in one transaction, so the request
/// applies or is refused whole (`g5.external-issues.edit`). Labels change
/// first, keeping the labels-before-issue lock order; the field edit then
/// applies to the issue as this transaction left it.
pub(crate) async fn edit_native_issue_with_labels(
    state: &Arc<AppState>,
    mut db: g5_authz::ScopedDb,
    writer: &IssueWriter<'_>,
    target: &CommentTarget,
    change: &IssueChange<'_>,
    labels: &LabelChange,
) -> Result<IssueEditDTO, AppError> {
    let IssueWriter {
        org_id,
        workspace_id,
        actor,
        domains,
    } = *writer;
    ensure_writable_issues(&mut db, org_id, workspace_id, "Edit issue").await?;
    let (access, location) = target_location(domains, target)?;
    let LinkedLocation::Native { issue_id } = location else {
        return Err(AppError::from_error(G5Error::Internal {
            message: "a G5 issue edit was sent a GitHub issue".to_string(),
        }));
    };
    let site = native_issues::native_site(&state.system_pool, &state.external_url, org_id).await?;
    let reader = native_reader(state, access, &site);
    native_issues::labels::change_issue_labels(&mut db, &reader, org_id, issue_id, actor, labels)
        .await?;
    let issue = native_issues::change_issue(
        &mut db,
        &reader,
        org_id,
        issue_id,
        actor,
        &NativeIssueChange {
            title: change.title,
            body: change.body,
            add: change.add,
            remove: change.remove,
            expected_body: change.expected_body,
        },
    )
    .await?;
    db.commit().await.map_err(AppError::from)?;
    tracing::info!(
        workspace_id = ?workspace_id,
        issue = %target.display_key,
        actor = %actor,
        "native issue edit with labels"
    );
    Ok(IssueEditDTO {
        outcome: "applied",
        issue: Some(issue.clone()),
        github_login: None,
        not_assigned: Vec::new(),
        not_unassigned: Vec::new(),
        message: None,
        labels: Some(IssueLabelChangeDTO::applied(labels, issue)),
    })
}

// ─── People's routes ─────────────────────────────────────────────────────────

/// Merged into `/api/v1/orgs/:orgId/issue-domains`: a domain's label catalog.
/// Members read it; members with write permission change it.
pub fn domain_label_router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .audited_route(
            "/:domainId/labels",
            get(list_domain_labels),
            &[Scope::WorkspacesRead],
        )
        .audited_route(
            "/:domainId/labels",
            post(create_domain_label),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/:domainId/labels/:labelId",
            patch(edit_domain_label).delete(delete_domain_label),
            &[Scope::WorkspacesWrite],
        )
        .into_router()
}

/// Merged into `/api/v1/orgs/:orgId/issues`: an issue's labels, changed
/// from its page.
pub fn issue_label_router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .audited_route(
            "/github/:repositoryId/:number/labels",
            post(change_github_page_labels),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/g5/:number/labels",
            post(change_native_page_labels),
            &[Scope::WorkspacesWrite],
        )
        .into_router()
}

#[derive(Debug, Deserialize)]
pub struct LabelDomainPathParams {
    #[serde(rename = "orgId")]
    pub org_id: Uuid,
    #[serde(rename = "domainId")]
    pub domain_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct LabelPathParams {
    #[serde(rename = "orgId")]
    pub org_id: Uuid,
    #[serde(rename = "domainId")]
    pub domain_id: Uuid,
    #[serde(rename = "labelId")]
    pub label_id: String,
}

#[derive(Debug, Deserialize)]
pub struct LabelPageQuery {
    #[serde(default)]
    pub page: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateLabelRequest {
    pub name: String,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditLabelRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChangeIssueLabelsRequest {
    #[serde(default)]
    pub add_labels: Vec<String>,
    #[serde(default)]
    pub remove_labels: Vec<String>,
}

/// The organization's domain `domain_id` and a scope admitting the caller
/// with `scopes`.
async fn domain_db(
    state: &Arc<AppState>,
    auth: &AuthContext,
    org_id: Uuid,
    domain_id: Uuid,
    scopes: &[&str],
) -> Result<(g5_authz::ScopedDb, IssueDomainAccess), AppError> {
    let mut db = scoped_db_for_org(state, auth, org_id, scopes)
        .await
        .map_err(mask_foreign_org_access)?;
    ensure_external_issues_enabled(&mut db, org_id).await?;
    let access = list_domain_access(&mut db, org_id)
        .await?
        .into_iter()
        .find(|access| access.domain.id == domain_id)
        .ok_or_else(|| {
            AppError::from_error(G5Error::NotFound {
                resource: format!("issue domain {domain_id}"),
            })
        })?;
    Ok((db, access))
}

/// `GET /api/v1/orgs/:orgId/issue-domains/:domainId/labels?page=N`
pub(crate) async fn list_domain_labels(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<LabelDomainPathParams>,
    Query(query): Query<LabelPageQuery>,
) -> Result<Json<ResponseEnvelope<LabelPageDTO>>, AppError> {
    let (db, access) = domain_db(
        &state,
        &auth,
        params.org_id,
        params.domain_id,
        MEMBER_READ_SCOPE,
    )
    .await?;
    let page = list_labels_as(&state, db, &access, query.page.unwrap_or(1)).await?;
    Ok(Json(ResponseEnvelope::ok(page)))
}

/// `POST /api/v1/orgs/:orgId/issue-domains/:domainId/labels`
pub(crate) async fn create_domain_label(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<LabelDomainPathParams>,
    Json(request): Json<CreateLabelRequest>,
) -> Result<(StatusCode, Json<ResponseEnvelope<LabelWriteDTO>>), AppError> {
    let actor = auth.require_user_id().map_err(AppError::from_error)?;
    let new = validate_new_label(
        &request.name,
        request.color.as_deref(),
        request.description.as_deref(),
    )?;
    let (db, access) = domain_db(
        &state,
        &auth,
        params.org_id,
        params.domain_id,
        MEMBER_WRITE_SCOPE,
    )
    .await?;
    let created = create_label_as(
        &state,
        db,
        &LabelWriter {
            org_id: params.org_id,
            workspace_id: None,
            actor,
            access: &access,
        },
        &new,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(ResponseEnvelope::ok(created))))
}

/// `PATCH /api/v1/orgs/:orgId/issue-domains/:domainId/labels/:labelId`
pub(crate) async fn edit_domain_label(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<LabelPathParams>,
    Json(request): Json<EditLabelRequest>,
) -> Result<Json<ResponseEnvelope<LabelWriteDTO>>, AppError> {
    let actor = auth.require_user_id().map_err(AppError::from_error)?;
    let patch = validate_patch(
        request.name.as_deref(),
        request.color.as_deref(),
        request.description.as_deref(),
    )?;
    let (db, access) = domain_db(
        &state,
        &auth,
        params.org_id,
        params.domain_id,
        MEMBER_WRITE_SCOPE,
    )
    .await?;
    let edited = edit_label_as(
        &state,
        db,
        &LabelWriter {
            org_id: params.org_id,
            workspace_id: None,
            actor,
            access: &access,
        },
        &LabelSelector::Id(params.label_id),
        &patch,
    )
    .await?;
    Ok(Json(ResponseEnvelope::ok(edited)))
}

/// `DELETE /api/v1/orgs/:orgId/issue-domains/:domainId/labels/:labelId`
pub(crate) async fn delete_domain_label(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<LabelPathParams>,
) -> Result<Json<ResponseEnvelope<LabelWriteDTO>>, AppError> {
    let actor = auth.require_user_id().map_err(AppError::from_error)?;
    let (db, access) = domain_db(
        &state,
        &auth,
        params.org_id,
        params.domain_id,
        MEMBER_WRITE_SCOPE,
    )
    .await?;
    let deleted = delete_label_as(
        &state,
        db,
        &LabelWriter {
            org_id: params.org_id,
            workspace_id: None,
            actor,
            access: &access,
        },
        &LabelSelector::Id(params.label_id),
    )
    .await?;
    Ok(Json(ResponseEnvelope::ok(deleted)))
}

#[derive(Debug, Deserialize)]
pub struct GithubLabelIssuePathParams {
    #[serde(rename = "orgId")]
    pub org_id: Uuid,
    #[serde(rename = "repositoryId")]
    pub repository_id: String,
    pub number: String,
}

#[derive(Debug, Deserialize)]
pub struct NativeLabelIssuePathParams {
    #[serde(rename = "orgId")]
    pub org_id: Uuid,
    pub number: String,
}

/// `POST /api/v1/orgs/:orgId/issues/github/:repositoryId/:number/labels`
///
/// Add and remove labels on a GitHub issue as the caller's GitHub account.
pub(crate) async fn change_github_page_labels(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<GithubLabelIssuePathParams>,
    Json(request): Json<ChangeIssueLabelsRequest>,
) -> Result<Json<ResponseEnvelope<IssueLabelChangeDTO>>, AppError> {
    let actor = auth.require_user_id().map_err(AppError::from_error)?;
    let change = validate_change(&request.add_labels, &request.remove_labels, false)?;
    let (db, domains, target) = github_page_target(
        &state,
        &auth,
        params.org_id,
        &params.repository_id,
        &params.number,
    )
    .await?;
    let changed = change_issue_labels_as(
        &state,
        db,
        &IssueWriter {
            org_id: params.org_id,
            workspace_id: None,
            actor,
            domains: &domains,
        },
        &target,
        &change,
    )
    .await?;
    Ok(Json(ResponseEnvelope::ok(changed)))
}

/// `POST /api/v1/orgs/:orgId/issues/g5/:number/labels`
///
/// Add and remove labels on a G5 issue as the caller.
pub(crate) async fn change_native_page_labels(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<NativeLabelIssuePathParams>,
    Json(request): Json<ChangeIssueLabelsRequest>,
) -> Result<Json<ResponseEnvelope<IssueLabelChangeDTO>>, AppError> {
    let actor = auth.require_user_id().map_err(AppError::from_error)?;
    let change = validate_change(&request.add_labels, &request.remove_labels, false)?;
    let number = params
        .number
        .parse::<u64>()
        .ok()
        .filter(|number| *number > 0)
        .ok_or_else(|| {
            issue_error(IssueError::invalid_reference(
                "a G5 issue number is a positive integer",
            ))
        })?;
    let (mut db, access, site) = native_page_db(&state, &auth, params.org_id).await?;
    let reader = native_reader(&state, &access, &site);
    let issue_id = native_issues::issue_id_by_number(&mut db, &reader, number).await?;
    let issue = native_issues::labels::change_issue_labels(
        &mut db,
        &reader,
        params.org_id,
        issue_id,
        actor,
        &change,
    )
    .await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(IssueLabelChangeDTO::applied(
        &change, issue,
    ))))
}
