//! Image attachments of native issues (`g5.issue.native.attachments`).
//!
//! An attachment belongs to its issue. Text shows one by its address under
//! the issue page; where each appears is derived from the description and
//! comments when listed, never stored, so an attachment no text shows is
//! listed at the issue. Uploads are exactly-once per uploader and key, and
//! never change the issue's edit version, so a description edited right
//! after an upload is not refused as changed.
//!
//! The bytes live in the attachment store
//! (`g5.issue.native.attachment-store`), which only this module holds: it
//! names an object only for an organization the caller's tenant transaction
//! is scoped to, writes it before its record and deletes it only after the
//! record's removal committed ([`RemovedAttachment`]).

use std::sync::Arc;

use bytes::Bytes;
use chrono::{DateTime, Utc};
use g5_authz::ScopedDb;
use g5_contracts::attachments::image_type;
use g5_contracts::error::G5Error;
use g5_storage::{AttachmentKey, AttachmentStore, StorageError, StoredObject};
use serde::Serialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::{
    conflict, db_error, invalid, load_profiles, number_of, profile, row_by_id, wire_time,
    NativeReader, NativeSite,
};
use crate::services::git::external_issues::ExternalIssueUserDTO;

/// Largest attachment: 10 MiB, GitHub's own limit for images.
pub const MAX_ATTACHMENT_BYTES: usize = 10 * 1024 * 1024;
/// Most attachments one issue holds.
pub const MAX_ATTACHMENTS_PER_ISSUE: i64 = 50;
const MAX_FILE_NAME_CHARS: usize = 255;

const ATTACHMENT_COLUMNS: &str = "id, org_id, issue_id, storage_id, uploader_user_id, origin, \
     file_name, content_type, size_bytes, sha256, created_at";

/// The attachment store as this module alone reaches it.
#[derive(Clone, Default)]
pub struct IssueAttachmentStore {
    backend: Option<Arc<dyn AttachmentStore>>,
}

impl IssueAttachmentStore {
    /// `None` when the deployment configured no store: attachment
    /// operations then answer unavailable.
    #[must_use]
    pub fn new(backend: Option<Arc<dyn AttachmentStore>>) -> Self {
        Self { backend }
    }

    /// Refuse, as unavailable, an operation that will need the bytes when
    /// no store is configured, before it changes or sends anything.
    pub fn ensure_available(&self) -> Result<(), G5Error> {
        self.backend().map(|_| ())
    }

    fn backend(&self) -> Result<&dyn AttachmentStore, G5Error> {
        self.backend
            .as_deref()
            .ok_or_else(|| G5Error::ServiceUnavailable {
                message: "G5 issue attachments are not available: no attachment store is \
                          configured"
                    .to_string(),
            })
    }

    /// The name of `org_id`'s object `id`, refused unless `db` is scoped to
    /// that organization: the path guard, independent of row-level security.
    fn key(db: &ScopedDb, org_id: Uuid, id: Uuid) -> Result<AttachmentKey, G5Error> {
        Self::key_within(db.org_ids(), org_id, id)
    }

    fn key_within(
        scope: &[g5_contracts::types::ids::OrgId],
        org_id: Uuid,
        id: Uuid,
    ) -> Result<AttachmentKey, G5Error> {
        if scope.iter().any(|org| *org.as_uuid() == org_id) {
            Ok(AttachmentKey { org_id, id })
        } else {
            tracing::error!(%org_id, "attachment of an organization outside the caller's scope");
            Err(G5Error::NotFound {
                resource: "the attachment".to_string(),
            })
        }
    }

    async fn open(&self, db: &ScopedDb, org_id: Uuid, id: Uuid) -> Result<StoredObject, G5Error> {
        let key = Self::key(db, org_id, id)?;
        self.backend()?
            .get(&key)
            .await
            .map_err(|error| match error {
                // A record never names missing bytes unless the store was
                // damaged or restored apart from the database: logged, and the
                // reader sees a missing attachment.
                StorageError::NotFound(_) => {
                    tracing::error!(key = %key.object_name(), "attachment bytes are missing");
                    G5Error::NotFound {
                        resource: "the attachment's bytes".to_string(),
                    }
                }
                other => store_error("read", &key, &other),
            })
    }

    /// Delete a removed attachment's bytes, once its removal has committed.
    /// A failure leaves bytes no record names, which nothing can read.
    pub async fn discard(&self, removed: RemovedAttachment) {
        let Ok(backend) = self.backend() else {
            return;
        };
        if let Err(error) = backend.delete(&removed.key).await {
            tracing::warn!(key = %removed.key.object_name(), %error, "removed attachment's bytes were kept");
        }
    }
}

fn store_error(action: &str, key: &AttachmentKey, error: &StorageError) -> G5Error {
    tracing::error!(key = %key.object_name(), %error, "attachment store {action} failed");
    G5Error::Internal {
        message: format!("the attachment's bytes could not be {action}"),
    }
}

/// A removed attachment whose bytes [`IssueAttachmentStore::discard`]
/// deletes after the removal commits; only a removal makes one.
#[must_use = "discard the bytes after the removal commits"]
pub struct RemovedAttachment {
    key: AttachmentKey,
}

#[derive(sqlx::FromRow)]
struct AttachmentRow {
    id: i64,
    org_id: Uuid,
    issue_id: i64,
    storage_id: Uuid,
    uploader_user_id: Uuid,
    origin: String,
    file_name: String,
    content_type: String,
    size_bytes: i32,
    sha256: String,
    created_at: DateTime<Utc>,
}

/// One attachment as the issue page and the attachment adapter show it.
#[derive(Debug, Clone)]
pub struct NativeAttachment {
    pub id: i64,
    pub issue_number: u64,
    pub uploader: ExternalIssueUserDTO,
    pub uploader_user_id: Uuid,
    pub origin: String,
    pub file_name: String,
    pub content_type: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub created_at: DateTime<Utc>,
    /// Its address under the issue page, which text uses to show it.
    pub url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeAttachmentDTO {
    pub id: String,
    pub file_name: String,
    pub content_type: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub uploader: ExternalIssueUserDTO,
    pub origin: String,
    pub created_at: String,
    pub url: String,
    /// The Markdown that shows it, for a composer to insert.
    pub markdown: String,
}

impl NativeAttachment {
    #[must_use]
    pub fn to_dto(&self) -> NativeAttachmentDTO {
        NativeAttachmentDTO {
            id: self.id.to_string(),
            file_name: self.file_name.clone(),
            content_type: self.content_type.clone(),
            size_bytes: self.size_bytes,
            sha256: self.sha256.clone(),
            uploader: self.uploader.clone(),
            origin: self.origin.clone(),
            created_at: wire_time(self.created_at),
            url: self.url.clone(),
            markdown: format!("![{}]({})", markdown_alt(&self.file_name), self.url),
        }
    }
}

/// Alt text that cannot close the image syntax early.
fn markdown_alt(name: &str) -> String {
    name.chars()
        .filter(|c| !matches!(c, '[' | ']' | '\n' | '\r'))
        .collect()
}

impl NativeSite {
    /// `/<org>/issues/g5/<number>/attachments/<id>`, under the issue page.
    #[must_use]
    pub fn attachment_url(&self, number: u64, id: i64) -> String {
        format!("{}/attachments/{id}", self.issue_url(number))
    }

    /// The `(issue number, attachment id)` of each attachment address of
    /// this organization `text` contains, absolute or as a path, in order.
    #[must_use]
    pub fn attachment_references(&self, text: &str) -> Vec<(u64, i64)> {
        let marker = format!("/{}/issues/g5/", self.org_slug);
        let mut found = Vec::new();
        let mut rest = text;
        while let Some(at) = rest.find(&marker) {
            rest = &rest[at + marker.len()..];
            let digits = |value: &str| {
                let end = value
                    .find(|c: char| !c.is_ascii_digit())
                    .unwrap_or(value.len());
                (end, value[..end].parse::<u64>().ok())
            };
            let (end, Some(number)) = digits(rest) else {
                continue;
            };
            let Some(after) = rest[end..].strip_prefix("/attachments/") else {
                continue;
            };
            let (_, Some(id)) = digits(after) else {
                continue;
            };
            if let Ok(id) = i64::try_from(id) {
                found.push((number, id));
            }
        }
        found
    }
}

/// An upload's file name: its last path component, trimmed, without
/// control characters, at most 255 characters.
pub fn normalize_file_name(raw: &str) -> Result<String, G5Error> {
    let name: String = raw
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .chars()
        .filter(|c| !c.is_control())
        .collect();
    let name = name.trim();
    if name.is_empty() || name == "." || name == ".." {
        return Err(invalid("fileName", "name the file"));
    }
    if name.chars().count() > MAX_FILE_NAME_CHARS {
        return Err(invalid(
            "fileName",
            format!("must be at most {MAX_FILE_NAME_CHARS} characters"),
        ));
    }
    Ok(name.to_string())
}

pub struct NewNativeAttachment<'a> {
    pub org_id: Uuid,
    pub issue_id: i64,
    pub uploader: Uuid,
    pub origin: &'static str,
    pub idempotency_key: &'a str,
    pub file_name: &'a str,
    pub bytes: &'a [u8],
}

/// Add an image to an issue, or return the one an earlier upload with the
/// same key added (`false`). The same key with another issue, name or
/// content is refused. The issue's row is locked so that two uploads cannot
/// pass its limit together.
pub async fn add_attachment(
    db: &mut ScopedDb,
    reader: &NativeReader<'_>,
    store: &IssueAttachmentStore,
    new: &NewNativeAttachment<'_>,
) -> Result<(NativeAttachment, bool), G5Error> {
    let file_name = normalize_file_name(new.file_name)?;
    if new.bytes.is_empty() {
        return Err(invalid("content", "the file is empty"));
    }
    if new.bytes.len() > MAX_ATTACHMENT_BYTES {
        return Err(invalid(
            "content",
            format!("an image may be at most {MAX_ATTACHMENT_BYTES} bytes"),
        ));
    }
    let content_type = image_type(new.bytes)
        .ok_or_else(|| invalid("content", "must be a PNG, JPEG, GIF or WebP image"))?;
    let sha256 = hex::encode(Sha256::digest(new.bytes));
    let issue = row_by_id(db, reader.domain.id, new.issue_id)
        .await?
        .ok_or_else(|| G5Error::NotFound {
            resource: "the issue".to_string(),
        })?;
    let replay = |row: AttachmentRow| {
        if row.issue_id == new.issue_id && row.file_name == file_name && row.sha256 == sha256 {
            Ok(row)
        } else {
            Err(invalid(
                "idempotencyKey",
                "already used for a different attachment",
            ))
        }
    };
    if let Some(row) = row_by_key(db, new).await? {
        return Ok((
            attachment_of(reader, number_of(&issue), replay(row)?).await?,
            false,
        ));
    }
    sqlx::query("SELECT id FROM native_issues WHERE id = $1 FOR NO KEY UPDATE")
        .bind(new.issue_id)
        .execute(db.conn())
        .await
        .map_err(|error| db_error("issue lock", &error))?;
    // A retry that waited on the lock finds the upload it repeats.
    if let Some(row) = row_by_key(db, new).await? {
        return Ok((
            attachment_of(reader, number_of(&issue), replay(row)?).await?,
            false,
        ));
    }
    let held: i64 =
        sqlx::query_scalar("SELECT count(*) FROM native_issue_attachments WHERE issue_id = $1")
            .bind(new.issue_id)
            .fetch_one(db.conn())
            .await
            .map_err(|error| db_error("attachment count", &error))?;
    if held >= MAX_ATTACHMENTS_PER_ISSUE {
        return Err(conflict(
            "attachment_limit",
            format!("an issue holds at most {MAX_ATTACHMENTS_PER_ISSUE} attachments"),
        ));
    }
    let size = i32::try_from(new.bytes.len()).map_err(|_| invalid("content", "too large"))?;
    // The bytes first, so that a record never names missing bytes.
    let backend = store.backend()?;
    let key = IssueAttachmentStore::key(db, new.org_id, Uuid::new_v4())?;
    backend
        .put(&key, Bytes::copy_from_slice(new.bytes))
        .await
        .map_err(|error| store_error("stored", &key, &error))?;
    let inserted: Result<Option<AttachmentRow>, G5Error> = sqlx::query_as(&format!(
        "INSERT INTO native_issue_attachments \
             (org_id, issue_id, storage_id, uploader_user_id, origin, idempotency_key, \
              file_name, content_type, size_bytes, sha256) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) \
         ON CONFLICT ON CONSTRAINT native_issue_attachments_idempotency_key DO NOTHING \
         RETURNING {ATTACHMENT_COLUMNS}"
    ))
    .bind(new.org_id)
    .bind(new.issue_id)
    .bind(key.id)
    .bind(new.uploader)
    .bind(new.origin)
    .bind(new.idempotency_key)
    .bind(&file_name)
    .bind(content_type)
    .bind(size)
    .bind(&sha256)
    .fetch_optional(db.conn())
    .await
    .map_err(|error| db_error("attachment insert", &error));
    if !matches!(inserted, Ok(Some(_))) {
        // No record names these bytes.
        store.discard(RemovedAttachment { key }).await;
    }
    let (row, fresh) = match inserted? {
        Some(row) => (row, true),
        // A concurrent retry won the insert; return what it stored.
        None => {
            let row = row_by_key(db, new)
                .await?
                .ok_or_else(|| G5Error::Internal {
                    message: "the conflicting attachment was not readable".to_string(),
                })?;
            (replay(row)?, false)
        }
    };
    Ok((attachment_of(reader, number_of(&issue), row).await?, fresh))
}

async fn row_by_key(
    db: &mut ScopedDb,
    new: &NewNativeAttachment<'_>,
) -> Result<Option<AttachmentRow>, G5Error> {
    sqlx::query_as(&format!(
        "SELECT {ATTACHMENT_COLUMNS} FROM native_issue_attachments \
         WHERE org_id = $1 AND uploader_user_id = $2 AND idempotency_key = $3"
    ))
    .bind(new.org_id)
    .bind(new.uploader)
    .bind(new.idempotency_key)
    .fetch_optional(db.conn())
    .await
    .map_err(|error| db_error("attachment key lookup", &error))
}

async fn attachment_of(
    reader: &NativeReader<'_>,
    number: u64,
    row: AttachmentRow,
) -> Result<NativeAttachment, G5Error> {
    let profiles = load_profiles(reader.system_pool, vec![row.uploader_user_id]).await?;
    Ok(attachment_with(reader, number, row, &profiles))
}

fn attachment_with(
    reader: &NativeReader<'_>,
    number: u64,
    row: AttachmentRow,
    profiles: &std::collections::HashMap<Uuid, ExternalIssueUserDTO>,
) -> NativeAttachment {
    NativeAttachment {
        id: row.id,
        issue_number: number,
        uploader: profile(profiles, row.uploader_user_id),
        uploader_user_id: row.uploader_user_id,
        origin: row.origin,
        url: reader.site.attachment_url(number, row.id),
        file_name: row.file_name,
        content_type: row.content_type,
        size_bytes: u64::try_from(row.size_bytes).unwrap_or_default(),
        sha256: row.sha256,
        created_at: row.created_at,
    }
}

/// The issue's attachments, oldest first.
pub async fn list_attachments(
    db: &mut ScopedDb,
    reader: &NativeReader<'_>,
    issue_id: i64,
) -> Result<Vec<NativeAttachment>, G5Error> {
    let issue = row_by_id(db, reader.domain.id, issue_id)
        .await?
        .ok_or_else(|| G5Error::NotFound {
            resource: "the issue".to_string(),
        })?;
    let rows: Vec<AttachmentRow> = sqlx::query_as(&format!(
        "SELECT {ATTACHMENT_COLUMNS} FROM native_issue_attachments \
         WHERE issue_id = $1 ORDER BY created_at, id"
    ))
    .bind(issue_id)
    .fetch_all(db.conn())
    .await
    .map_err(|error| db_error("attachment list", &error))?;
    let profiles = load_profiles(
        reader.system_pool,
        rows.iter().map(|row| row.uploader_user_id).collect(),
    )
    .await?;
    let number = number_of(&issue);
    Ok(rows
        .into_iter()
        .map(|row| attachment_with(reader, number, row, &profiles))
        .collect())
}

/// One attachment of the issue and its bytes, opened before the caller's
/// transaction ends.
pub async fn attachment_content(
    db: &mut ScopedDb,
    reader: &NativeReader<'_>,
    store: &IssueAttachmentStore,
    issue_id: i64,
    id: i64,
) -> Result<(NativeAttachment, StoredObject), G5Error> {
    let issue = row_by_id(db, reader.domain.id, issue_id)
        .await?
        .ok_or_else(|| G5Error::NotFound {
            resource: "the issue".to_string(),
        })?;
    let found: AttachmentRow = sqlx::query_as(&format!(
        "SELECT {ATTACHMENT_COLUMNS} FROM native_issue_attachments \
         WHERE issue_id = $1 AND id = $2"
    ))
    .bind(issue_id)
    .bind(id)
    .fetch_optional(db.conn())
    .await
    .map_err(|error| db_error("attachment read", &error))?
    .ok_or_else(|| G5Error::NotFound {
        resource: "the attachment".to_string(),
    })?;
    let content = store.open(db, found.org_id, found.storage_id).await?;
    let attachment = attachment_of(reader, number_of(&issue), found).await?;
    Ok((attachment, content))
}

/// Remove an attachment: the uploader's own, or any when `may_remove_any`
/// (an organization administrator). Its bytes go once the caller commits and
/// hands the result to [`IssueAttachmentStore::discard`].
pub async fn remove_attachment(
    db: &mut ScopedDb,
    issue_id: i64,
    id: i64,
    actor: Uuid,
    may_remove_any: bool,
) -> Result<RemovedAttachment, G5Error> {
    let (uploader, org_id, storage_id): (Uuid, Uuid, Uuid) = sqlx::query_as(
        "SELECT uploader_user_id, org_id, storage_id FROM native_issue_attachments \
         WHERE issue_id = $1 AND id = $2",
    )
    .bind(issue_id)
    .bind(id)
    .fetch_optional(db.conn())
    .await
    .map_err(|error| db_error("attachment lookup", &error))?
    .ok_or_else(|| G5Error::NotFound {
        resource: "the attachment".to_string(),
    })?;
    if uploader != actor && !may_remove_any {
        return Err(G5Error::Forbidden {
            missing_scopes: vec!["org:admin".to_string()],
        });
    }
    // Attachments are never edited, so the uploader read above cannot have
    // changed; a concurrent removal leaves nothing to delete, which is the
    // same outcome.
    sqlx::query("DELETE FROM native_issue_attachments WHERE issue_id = $1 AND id = $2")
        .bind(issue_id)
        .bind(id)
        .execute(db.conn())
        .await
        .map_err(|error| db_error("attachment delete", &error))?;
    Ok(RemovedAttachment {
        key: IssueAttachmentStore::key(db, org_id, storage_id)?,
    })
}

/// Where an attachment appears.
#[derive(Debug, Clone)]
pub enum NativeAttachmentPlace {
    Description,
    Comment {
        id: i64,
        author: ExternalIssueUserDTO,
        created_at: DateTime<Utc>,
    },
    /// No text of the issue shows it.
    Issue,
}

/// The first place each of `attachments` appears: the description, else the
/// oldest comment that shows it, else the issue itself.
pub async fn attachment_places(
    db: &mut ScopedDb,
    reader: &NativeReader<'_>,
    issue_id: i64,
    attachments: &[NativeAttachment],
) -> Result<Vec<NativeAttachmentPlace>, G5Error> {
    let issue = row_by_id(db, reader.domain.id, issue_id)
        .await?
        .ok_or_else(|| G5Error::NotFound {
            resource: "the issue".to_string(),
        })?;
    let number = number_of(&issue);
    #[derive(sqlx::FromRow)]
    struct CommentText {
        id: i64,
        author_user_id: Uuid,
        created_at: DateTime<Utc>,
        body: String,
    }
    let comments: Vec<CommentText> = sqlx::query_as(
        "SELECT id, author_user_id, created_at, body FROM native_issue_comments \
         WHERE issue_id = $1 AND strpos(body, '/attachments/') > 0 ORDER BY created_at, id",
    )
    .bind(issue_id)
    .fetch_all(db.conn())
    .await
    .map_err(|error| db_error("comment references", &error))?;
    let profiles = load_profiles(
        reader.system_pool,
        comments
            .iter()
            .map(|comment| comment.author_user_id)
            .collect(),
    )
    .await?;
    let shows = |text: &str, id: i64| {
        reader
            .site
            .attachment_references(text)
            .contains(&(number, id))
    };
    Ok(attachments
        .iter()
        .map(|attachment| {
            if shows(&issue.body, attachment.id) {
                return NativeAttachmentPlace::Description;
            }
            comments
                .iter()
                .find(|comment| shows(&comment.body, attachment.id))
                .map_or(NativeAttachmentPlace::Issue, |comment| {
                    NativeAttachmentPlace::Comment {
                        id: comment.id,
                        author: profile(&profiles, comment.author_user_id),
                        created_at: comment.created_at,
                    }
                })
        })
        .collect())
}

/// Every attachment of the domain's issues, without its bytes, for export;
/// [`export_content`] reads them one at a time.
pub(super) async fn export_rows(
    db: &mut ScopedDb,
    domain_id: Uuid,
) -> Result<Vec<ExportedAttachment>, G5Error> {
    sqlx::query_as(
        "SELECT a.id, a.org_id, a.issue_id, a.storage_id, a.uploader_user_id, a.origin, a.file_name, \
                a.content_type, a.size_bytes, a.sha256, a.created_at \
         FROM native_issue_attachments a JOIN native_issues i ON i.id = a.issue_id \
         WHERE i.domain_id = $1 ORDER BY a.created_at, a.id",
    )
    .bind(domain_id)
    .fetch_all(db.conn())
    .await
    .map_err(|error| db_error("export attachments", &error))
}

/// Where an exported attachment's bytes are.
#[derive(Debug, Clone, Copy)]
pub(super) struct StoredBytes {
    org_id: Uuid,
    storage_id: Uuid,
}

/// One exported attachment's bytes, read under the export's transaction.
pub(super) async fn export_content(
    db: &ScopedDb,
    store: &IssueAttachmentStore,
    bytes: StoredBytes,
) -> Result<Vec<u8>, G5Error> {
    use futures_util::TryStreamExt as _;
    let object = store.open(db, bytes.org_id, bytes.storage_id).await?;
    let mut content = Vec::with_capacity(usize::try_from(object.length).unwrap_or_default());
    let mut body = object.body;
    while let Some(chunk) = body.try_next().await.map_err(|error| G5Error::Internal {
        message: format!("reading an exported attachment: {error}"),
    })? {
        content.extend_from_slice(&chunk);
    }
    Ok(content)
}

#[derive(sqlx::FromRow)]
pub(super) struct ExportedAttachment {
    pub(super) id: i64,
    org_id: Uuid,
    pub(super) issue_id: i64,
    storage_id: Uuid,
    pub(super) uploader_user_id: Uuid,
    pub(super) origin: String,
    pub(super) file_name: String,
    pub(super) content_type: String,
    pub(super) size_bytes: i32,
    pub(super) sha256: String,
    pub(super) created_at: DateTime<Utc>,
}

impl ExportedAttachment {
    pub(super) fn bytes(&self) -> StoredBytes {
        StoredBytes {
            org_id: self.org_id,
            storage_id: self.storage_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attachment_addresses_are_found_for_this_organization_only() {
        let site = NativeSite::new("https://g5.example/", "acme");
        assert_eq!(
            site.attachment_url(12, 7),
            "https://g5.example/acme/issues/g5/12/attachments/7"
        );
        let text = "see ![a](https://g5.example/acme/issues/g5/12/attachments/7) and \
                    ![b](/acme/issues/g5/12/attachments/8), not /other/issues/g5/12/attachments/9 \
                    nor /acme/issues/g5/12/comments/3 or /acme/issues/g5/x/attachments/4";
        assert_eq!(site.attachment_references(text), vec![(12, 7), (12, 8)]);
        assert!(site.attachment_references("no images").is_empty());
    }

    #[test]
    fn file_names_keep_only_their_last_component() {
        assert_eq!(normalize_file_name(" mock.png ").unwrap(), "mock.png");
        assert_eq!(
            normalize_file_name("C:\\Users\\me\\shot.png").unwrap(),
            "shot.png"
        );
        assert_eq!(normalize_file_name("../../etc/a\nb.png").unwrap(), "ab.png");
        assert!(normalize_file_name("dir/").is_err());
        assert!(normalize_file_name("..").is_err());
        assert!(normalize_file_name(&"x".repeat(256)).is_err());
    }

    #[test]
    fn bytes_are_named_only_within_the_callers_organizations() {
        use g5_contracts::types::ids::OrgId;
        let (acme, beta, id) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        let scope = [OrgId::from_uuid(acme)];
        let key = IssueAttachmentStore::key_within(&scope, acme, id).unwrap();
        assert_eq!(key, AttachmentKey { org_id: acme, id });
        assert!(key.object_name().starts_with(&format!("{acme}/")));
        assert!(matches!(
            IssueAttachmentStore::key_within(&scope, beta, id),
            Err(G5Error::NotFound { .. })
        ));
        assert!(IssueAttachmentStore::key_within(&[], acme, id).is_err());
    }

    #[test]
    fn without_a_store_attachments_are_unavailable() {
        let store = IssueAttachmentStore::default();
        assert!(matches!(
            store.backend(),
            Err(G5Error::ServiceUnavailable { .. })
        ));
    }

    #[test]
    fn alt_text_cannot_break_the_image_syntax() {
        assert_eq!(markdown_alt("a]b[c\nd.png"), "abcd.png");
    }
}
