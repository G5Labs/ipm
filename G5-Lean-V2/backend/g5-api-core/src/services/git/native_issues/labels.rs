//! The native tracker's labels (`g5.issue.labels.native`): a domain's label
//! catalog and the labels on its issues, in the `native_issue_labels` and
//! `native_issue_label_assignments` tables of migration 154.
//!
//! Every statement runs in the caller's tenant transaction. Locks are taken
//! labels first, then issues: a change locks the labels it names `FOR SHARE`
//! and then its issue; a catalog delete locks its label `FOR UPDATE` and then
//! the issues carrying it. A change therefore never sees a label half
//! deleted, and the two cannot deadlock.

use std::collections::HashSet;

use g5_authz::ScopedDb;
use g5_contracts::error::G5Error;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use super::{db_error, row_by_id, touch, NativeReader};
use crate::services::git::external_issues::{
    ExternalIssueLabelDTO, ExternalIssueSummaryDTO, IssueDomainRecord,
};
use crate::services::git::issue_labels::{
    label_exists, matches_new, unknown_labels, LabelChange, LabelPageDTO, LabelPatch,
    LabelSelector, NewLabel, LABEL_PAGE_SIZE,
};

/// Most labels one native issue carries, as on GitHub.
pub const MAX_LABELS_PER_ISSUE: usize = 100;

/// A label as issue reads carry it (JSON from [`super::ISSUE_SELECT`]).
#[derive(Debug, Clone, Deserialize, sqlx::FromRow)]
pub(super) struct LabelRow {
    pub id: i64,
    pub name: String,
    pub color: Option<String>,
    pub description: String,
}

impl LabelRow {
    pub(super) fn to_dto(&self) -> ExternalIssueLabelDTO {
        ExternalIssueLabelDTO {
            id: Some(self.id.to_string()),
            name: self.name.clone(),
            color: self.color.clone(),
            description: Some(self.description.clone()).filter(|text| !text.is_empty()),
        }
    }
}

const LABEL_COLUMNS: &str = "id, name, color, description";

fn not_found(selector: &LabelSelector, domain: &IssueDomainRecord) -> G5Error {
    G5Error::NotFound {
        resource: format!("{} in {}", selector.describe(), domain.display_key),
    }
}

/// The selector's label in the domain, locked as `lock` says (`FOR UPDATE`
/// or empty).
async fn select(
    db: &mut ScopedDb,
    domain_id: Uuid,
    selector: &LabelSelector,
    lock: &str,
) -> Result<Option<LabelRow>, G5Error> {
    let found = match selector {
        LabelSelector::Id(id) => {
            let Ok(id) = id.parse::<i64>() else {
                return Ok(None);
            };
            sqlx::query_as::<_, LabelRow>(&format!(
                "SELECT {LABEL_COLUMNS} FROM native_issue_labels \
                 WHERE domain_id = $1 AND id = $2 {lock}"
            ))
            .bind(domain_id)
            .bind(id)
            .fetch_optional(db.conn())
            .await
        }
        LabelSelector::Name(name) => {
            sqlx::query_as::<_, LabelRow>(&format!(
                "SELECT {LABEL_COLUMNS} FROM native_issue_labels \
                 WHERE domain_id = $1 AND lower(name) = lower($2) {lock}"
            ))
            .bind(domain_id)
            .bind(name)
            .fetch_optional(db.conn())
            .await
        }
    };
    found.map_err(|error| db_error("label lookup", &error))
}

/// One page (1-based) of the domain's catalog, ordered by name.
pub async fn list(
    db: &mut ScopedDb,
    domain: &IssueDomainRecord,
    page: u32,
) -> Result<LabelPageDTO, G5Error> {
    let page = page.max(1);
    let size = i64::try_from(LABEL_PAGE_SIZE).unwrap_or(100);
    let mut rows: Vec<LabelRow> = sqlx::query_as(&format!(
        "SELECT {LABEL_COLUMNS} FROM native_issue_labels WHERE domain_id = $1 \
         ORDER BY lower(name), id LIMIT $2 OFFSET $3"
    ))
    .bind(domain.id)
    .bind(size + 1)
    .bind(i64::from(page - 1) * size)
    .fetch_all(db.conn())
    .await
    .map_err(|error| db_error("label list", &error))?;
    let more = rows.len() > LABEL_PAGE_SIZE;
    rows.truncate(LABEL_PAGE_SIZE);
    Ok(LabelPageDTO {
        domain_id: domain.id,
        display_key: domain.display_key.clone(),
        labels: rows.iter().map(LabelRow::to_dto).collect(),
        next_page: more.then_some(page + 1),
    })
}

/// The domain's label `selector` names, if any.
pub async fn find(
    db: &mut ScopedDb,
    domain: &IssueDomainRecord,
    selector: &LabelSelector,
) -> Result<Option<ExternalIssueLabelDTO>, G5Error> {
    Ok(select(db, domain.id, selector, "")
        .await?
        .map(|row| row.to_dto()))
}

/// Add a label to the catalog. A name the catalog already has (without
/// regard to case) returns that label when it is what was asked for, so a
/// retried create is harmless, and is otherwise refused with the existing
/// label (`label_exists`); nothing is overwritten.
pub async fn create(
    db: &mut ScopedDb,
    org_id: Uuid,
    domain: &IssueDomainRecord,
    new: &NewLabel,
) -> Result<ExternalIssueLabelDTO, G5Error> {
    let inserted: Option<LabelRow> = sqlx::query_as(&format!(
        "INSERT INTO native_issue_labels (org_id, domain_id, name, color, description) \
         VALUES ($1, $2, $3, $4, $5) \
         ON CONFLICT (domain_id, lower(name)) DO NOTHING RETURNING {LABEL_COLUMNS}"
    ))
    .bind(org_id)
    .bind(domain.id)
    .bind(&new.name)
    .bind(&new.color)
    .bind(&new.description)
    .fetch_optional(db.conn())
    .await
    .map_err(|error| db_error("label insert", &error))?;
    if let Some(row) = inserted {
        return Ok(row.to_dto());
    }
    let existing = select(db, domain.id, &LabelSelector::Name(new.name.clone()), "")
        .await?
        .ok_or_else(|| G5Error::DatabaseRetryable {
            message: "a label with that name was removed while it was being created".to_string(),
        })?
        .to_dto();
    if matches_new(&existing, new) {
        Ok(existing)
    } else {
        Err(label_exists(&existing))
    }
}

/// Rename, recolour or redescribe a catalog label. Issues keep it: they
/// refer to its id. Taking another label's name is refused with that label.
pub async fn edit(
    db: &mut ScopedDb,
    domain: &IssueDomainRecord,
    selector: &LabelSelector,
    patch: &LabelPatch,
) -> Result<ExternalIssueLabelDTO, G5Error> {
    let current = select(db, domain.id, selector, "FOR UPDATE")
        .await?
        .ok_or_else(|| not_found(selector, domain))?;
    if let Some(name) = &patch.name {
        if let Some(other) = select(db, domain.id, &LabelSelector::Name(name.clone()), "")
            .await?
            .filter(|other| other.id != current.id)
        {
            return Err(label_exists(&other.to_dto()));
        }
    }
    let row: LabelRow = sqlx::query_as(&format!(
        "UPDATE native_issue_labels SET name = COALESCE($2, name), \
         color = COALESCE($3, color), description = COALESCE($4, description), \
         updated_at = now() WHERE id = $1 RETURNING {LABEL_COLUMNS}"
    ))
    .bind(current.id)
    .bind(&patch.name)
    .bind(&patch.color)
    .bind(&patch.description)
    .fetch_one(db.conn())
    .await
    .map_err(|error| match &error {
        sqlx::Error::Database(database) if database.is_unique_violation() => {
            G5Error::ConflictDetails {
                reason: "another label took that name meanwhile; reload the labels and try again"
                    .to_string(),
                details: json!({"code": "label_exists"}),
            }
        }
        _ => db_error("label update", &error),
    })?;
    Ok(row.to_dto())
}

/// Delete a catalog label and remove it from every issue, recording the
/// removal on each, in this transaction. Returns the label as it was.
pub async fn delete(
    db: &mut ScopedDb,
    org_id: Uuid,
    domain: &IssueDomainRecord,
    selector: &LabelSelector,
    actor: Uuid,
) -> Result<ExternalIssueLabelDTO, G5Error> {
    let label = select(db, domain.id, selector, "FOR UPDATE")
        .await?
        .ok_or_else(|| not_found(selector, domain))?;
    // No assignment can be added while the label is locked; lock the
    // issues carrying it, in id order, before changing them.
    let issues: Vec<i64> = sqlx::query_scalar(
        "SELECT i.id FROM native_issues i \
         WHERE i.id IN (SELECT issue_id FROM native_issue_label_assignments WHERE label_id = $1) \
         ORDER BY i.id FOR UPDATE OF i",
    )
    .bind(label.id)
    .fetch_all(db.conn())
    .await
    .map_err(|error| db_error("labelled issues lock", &error))?;
    for issue_id in &issues {
        record(db, org_id, *issue_id, actor, "unlabeled", &label.name).await?;
        touch(db, *issue_id).await?;
    }
    sqlx::query("DELETE FROM native_issue_labels WHERE id = $1")
        .bind(label.id)
        .execute(db.conn())
        .await
        .map_err(|error| db_error("label delete", &error))?;
    Ok(label.to_dto())
}

async fn record(
    db: &mut ScopedDb,
    org_id: Uuid,
    issue_id: i64,
    actor: Uuid,
    kind: &str,
    label_name: &str,
) -> Result<(), G5Error> {
    sqlx::query(
        "INSERT INTO native_issue_events (org_id, issue_id, actor_user_id, kind, label_name) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(org_id)
    .bind(issue_id)
    .bind(actor)
    .bind(kind)
    .bind(label_name)
    .execute(db.conn())
    .await
    .map_err(|error| db_error("label event insert", &error))?;
    Ok(())
}

/// Lock the catalog labels `names` name (without regard to case) `FOR SHARE`,
/// in id order, so none is renamed or deleted before this transaction ends.
/// A name the catalog lacks is refused, never created.
pub(super) async fn lock_named(
    db: &mut ScopedDb,
    domain: &IssueDomainRecord,
    field: &str,
    names: &[String],
) -> Result<Vec<LabelRow>, G5Error> {
    if names.is_empty() {
        return Ok(Vec::new());
    }
    let lowered: Vec<String> = names.iter().map(|name| name.to_lowercase()).collect();
    let rows: Vec<LabelRow> = sqlx::query_as(&format!(
        "SELECT {LABEL_COLUMNS} FROM native_issue_labels \
         WHERE domain_id = $1 AND lower(name) = ANY($2) ORDER BY id FOR SHARE"
    ))
    .bind(domain.id)
    .bind(&lowered)
    .fetch_all(db.conn())
    .await
    .map_err(|error| db_error("label lock", &error))?;
    let found: HashSet<String> = rows.iter().map(|row| row.name.to_lowercase()).collect();
    let missing: Vec<String> = names
        .iter()
        .filter(|name| !found.contains(&name.to_lowercase()))
        .cloned()
        .collect();
    if !missing.is_empty() {
        return Err(unknown_labels(field, &missing, &domain.display_key));
    }
    Ok(rows)
}

/// Attach locked catalog labels to a newly created issue (no events, as
/// creation records none for its assignees).
pub(super) async fn attach_new(
    db: &mut ScopedDb,
    org_id: Uuid,
    domain_id: Uuid,
    issue_id: i64,
    labels: &[LabelRow],
) -> Result<(), G5Error> {
    for label in labels {
        sqlx::query(
            "INSERT INTO native_issue_label_assignments (org_id, domain_id, issue_id, label_id) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(org_id)
        .bind(domain_id)
        .bind(issue_id)
        .bind(label.id)
        .execute(db.conn())
        .await
        .map_err(|error| db_error("label attach", &error))?;
    }
    Ok(())
}

/// Add and remove labels on an issue (`g5.issue.labels.native`): the named
/// labels, then the issue, are locked; each actual change appends an event,
/// and the issue's update time moves only when something changed. Adding a
/// label the issue has, or removing one it lacks, changes nothing.
pub async fn change_issue_labels(
    db: &mut ScopedDb,
    reader: &NativeReader<'_>,
    org_id: Uuid,
    issue_id: i64,
    actor: Uuid,
    change: &LabelChange,
) -> Result<ExternalIssueSummaryDTO, G5Error> {
    let domain = reader.domain;
    // One statement locks every named label, in id order.
    let named: Vec<String> = change.add.iter().chain(&change.remove).cloned().collect();
    let locked = lock_named(db, domain, "labels", &named).await?;
    let of = |names: &[String]| -> Vec<LabelRow> {
        locked
            .iter()
            .filter(|label| {
                names
                    .iter()
                    .any(|name| name.to_lowercase() == label.name.to_lowercase())
            })
            .cloned()
            .collect()
    };
    let (added, removed) = (of(&change.add), of(&change.remove));
    sqlx::query_scalar::<_, i64>(
        "SELECT id FROM native_issues WHERE org_id = $1 AND domain_id = $2 AND id = $3 FOR UPDATE",
    )
    .bind(org_id)
    .bind(domain.id)
    .bind(issue_id)
    .fetch_optional(db.conn())
    .await
    .map_err(|error| db_error("issue lock", &error))?
    .ok_or_else(|| G5Error::NotFound {
        resource: "the issue".to_string(),
    })?;
    let mut changed = false;
    for label in &removed {
        let gone = sqlx::query(
            "DELETE FROM native_issue_label_assignments WHERE issue_id = $1 AND label_id = $2",
        )
        .bind(issue_id)
        .bind(label.id)
        .execute(db.conn())
        .await
        .map_err(|error| db_error("label removal", &error))?
        .rows_affected();
        if gone > 0 {
            record(db, org_id, issue_id, actor, "unlabeled", &label.name).await?;
            changed = true;
        }
    }
    for label in &added {
        let fresh = sqlx::query(
            "INSERT INTO native_issue_label_assignments (org_id, domain_id, issue_id, label_id) \
             VALUES ($1, $2, $3, $4) ON CONFLICT (issue_id, label_id) DO NOTHING",
        )
        .bind(org_id)
        .bind(domain.id)
        .bind(issue_id)
        .bind(label.id)
        .execute(db.conn())
        .await
        .map_err(|error| db_error("label assignment", &error))?
        .rows_affected();
        if fresh > 0 {
            record(db, org_id, issue_id, actor, "labeled", &label.name).await?;
            changed = true;
        }
    }
    if !added.is_empty() {
        let carried: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM native_issue_label_assignments WHERE issue_id = $1",
        )
        .bind(issue_id)
        .fetch_one(db.conn())
        .await
        .map_err(|error| db_error("label count", &error))?;
        if usize::try_from(carried).unwrap_or(usize::MAX) > MAX_LABELS_PER_ISSUE {
            return Err(G5Error::ValidationError {
                details: json!({
                    "field": "addLabels",
                    "code": "too_many_labels",
                    "error": format!("an issue carries at most {MAX_LABELS_PER_ISSUE} labels"),
                }),
            });
        }
    }
    if changed {
        touch(db, issue_id).await?;
    }
    let row = row_by_id(db, domain.id, issue_id)
        .await?
        .ok_or_else(|| G5Error::NotFound {
            resource: "the issue".to_string(),
        })?;
    reader.one_summary(&row).await
}
