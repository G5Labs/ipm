//! Provider session files (design v6 §8.4): harvested after every turn,
//! restored before a resumed turn.
//!
//! A provider CLI keeps what it needs to resume its own session in files
//! under its home directory:
//!
//! * Claude, under `CLAUDE_CONFIG_DIR`: the transcript
//!   `projects/<project key>/<session id>.jsonl` and every regular file under
//!   `projects/<project key>/<session id>/`, where the project key
//!   is the escaped working directory ([`super::claude::project_key_for_cwd`]).
//! * Codex, under `CODEX_HOME`: the rollout
//!   `sessions/YYYY/MM/DD/rollout-<timestamp>-<session id>.jsonl`.
//!
//! Postgres (`chat_session_files`) is the only durable copy. Production
//! [`harvest_complete`] publishes one verified generation, and
//! [`restore_verified`] replaces local session files from that generation.
//! Both use a [`SessionFileStore`] and the provider home.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use g5_contracts::error::G5Error;
use g5_internal_api::dtos::{
    ChatSessionFileManifestItem, PublishedSessionFile, SessionFileBundlePublication,
};
use g5_internal_api::session_files::{
    sha256_hex, validate_provider_session_id, validate_session_file_path, ChatSessionFileContent,
    SessionFileProvider, CHAT_SESSION_FILE_MAX_BYTES,
};
use g5_internal_api::InternalApiClient;

use super::Provider;

/// Depth of the dated `YYYY/MM/DD` directories under Codex's `sessions/`.
const CODEX_SESSION_DIR_DEPTH: usize = 3;

impl From<Provider> for SessionFileProvider {
    fn from(provider: Provider) -> Self {
        match provider {
            Provider::ClaudeCode => SessionFileProvider::ClaudeCode,
            Provider::Codex => SessionFileProvider::Codex,
        }
    }
}

/// Where one provider keeps its session files on this host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionFileLayout {
    /// `config_dir` is `CLAUDE_CONFIG_DIR` as this process sees it; `cwd` is
    /// the working directory the CLI runs in, as the CLI sees it, which
    /// selects the project key.
    Claude { config_dir: PathBuf, cwd: PathBuf },
    /// `CODEX_HOME` as this process sees it.
    Codex { codex_home: PathBuf },
}

impl SessionFileLayout {
    /// Construct the layout for the working directory of the fenced attempt.
    /// Both turn start and workspace flush use this mapping.
    pub fn for_provider(
        provider: SessionFileProvider,
        home: &Path,
        workspace_root: &Path,
        cwd_relative_path: &Path,
    ) -> Self {
        match provider {
            SessionFileProvider::ClaudeCode => Self::Claude {
                config_dir: home.join(".claude"),
                // Claude keys its project directory from the CLI's normalized
                // cwd. A root target joined with "" or "." must not introduce
                // a trailing slash or dot into that key.
                cwd: workspace_root
                    .join(cwd_relative_path)
                    .components()
                    .collect(),
            },
            SessionFileProvider::Codex => Self::Codex {
                codex_home: home.join(".codex"),
            },
        }
    }

    pub fn provider(&self) -> SessionFileProvider {
        match self {
            SessionFileLayout::Claude { .. } => SessionFileProvider::ClaudeCode,
            SessionFileLayout::Codex { .. } => SessionFileProvider::Codex,
        }
    }

    /// The provider home every relative path is joined onto.
    pub fn home(&self) -> &Path {
        match self {
            SessionFileLayout::Claude { config_dir, .. } => config_dir,
            SessionFileLayout::Codex { codex_home } => codex_home,
        }
    }

    fn claude_project_rel(cwd: &Path) -> String {
        format!("projects/{}", super::claude::project_key_for_cwd(cwd))
    }
}

/// One session file read from the host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalSessionFile {
    pub relative_path: String,
    pub sha256: String,
    pub bytes: Vec<u8>,
}

/// Why a file under the session's paths was not harvested.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// Larger than the 128 MiB cap.
    TooLarge,
    /// A symlink; the scan never follows one.
    Symlink,
    /// A file name cannot be represented by the bounded UTF-8 API path.
    InvalidPath,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedSessionFile {
    pub relative_path: String,
    pub reason: SkipReason,
}

/// The session files found under one layout.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionFileScan {
    pub files: Vec<LocalSessionFile>,
    pub skipped: Vec<SkippedSessionFile>,
}

/// One file write within an already fenced publication generation.
pub struct FencedSessionFileUpload<'a> {
    pub provider: SessionFileProvider,
    pub provider_session_id: &'a str,
    pub relative_path: &'a str,
    pub bytes: &'a [u8],
    pub full_sha256: &'a str,
    pub base: Option<(i64, &'a str)>,
    pub generation: uuid::Uuid,
}

/// The durable copy of the session files: the internal API in production, an
/// in-memory map in tests.
#[async_trait]
pub trait SessionFileStore: Send + Sync {
    async fn published(
        &self,
        _chat_session_id: &str,
        _provider: SessionFileProvider,
        _provider_session_id: &str,
    ) -> Result<Option<SessionFileBundlePublication>, G5Error> {
        Ok(None)
    }

    async fn begin(&self, _chat_session_id: &str, _generation: uuid::Uuid) -> Result<(), G5Error> {
        Err(G5Error::Internal {
            message: "bundle publication is unsupported".into(),
        })
    }

    async fn publish(
        &self,
        _chat_session_id: &str,
        _bundle: &SessionFileBundlePublication,
    ) -> Result<(), G5Error> {
        Err(G5Error::Internal {
            message: "bundle publication is unsupported".into(),
        })
    }

    async fn put_fenced(
        &self,
        _chat_session_id: &str,
        _upload: FencedSessionFileUpload<'_>,
    ) -> Result<bool, G5Error> {
        Err(G5Error::Internal {
            message: "fenced upload is unsupported".into(),
        })
    }

    async fn manifest(
        &self,
        chat_session_id: &str,
        provider_session_id: &str,
    ) -> Result<Vec<ChatSessionFileManifestItem>, G5Error>;

    async fn content(
        &self,
        chat_session_id: &str,
        provider_session_id: &str,
        relative_path: &str,
    ) -> Result<ChatSessionFileContent, G5Error>;

    /// Upsert one file; returns whether the stored bytes changed.
    async fn put(
        &self,
        chat_session_id: &str,
        provider: SessionFileProvider,
        provider_session_id: &str,
        relative_path: &str,
        bytes: &[u8],
    ) -> Result<bool, G5Error>;
}

#[async_trait]
impl SessionFileStore for InternalApiClient {
    async fn published(
        &self,
        chat_session_id: &str,
        provider: SessionFileProvider,
        provider_session_id: &str,
    ) -> Result<Option<SessionFileBundlePublication>, G5Error> {
        self.published_session_file_bundle(chat_session_id, provider, provider_session_id)
            .await
    }

    async fn publish(
        &self,
        chat_session_id: &str,
        bundle: &SessionFileBundlePublication,
    ) -> Result<(), G5Error> {
        self.publish_session_file_bundle(chat_session_id, bundle)
            .await
    }

    async fn put_fenced(
        &self,
        chat_session_id: &str,
        upload: FencedSessionFileUpload<'_>,
    ) -> Result<bool, G5Error> {
        Ok(if let Some((base_size, base_sha256)) = upload.base {
            self.append_chat_session_file(
                chat_session_id,
                upload.provider,
                upload.provider_session_id,
                upload.relative_path,
                g5_internal_api::client::ChatSessionFileAppend {
                    suffix: upload.bytes,
                    full_sha256: upload.full_sha256,
                    base_size,
                    base_sha256,
                    generation: upload.generation,
                },
            )
            .await?
            .changed
        } else {
            self.put_chat_session_file(
                chat_session_id,
                upload.provider,
                upload.provider_session_id,
                upload.relative_path,
                upload.bytes,
                Some(upload.generation),
            )
            .await?
            .changed
        })
    }
    async fn manifest(
        &self,
        chat_session_id: &str,
        provider_session_id: &str,
    ) -> Result<Vec<ChatSessionFileManifestItem>, G5Error> {
        Ok(self
            .chat_session_files_manifest(chat_session_id, provider_session_id)
            .await?
            .files)
    }

    async fn content(
        &self,
        chat_session_id: &str,
        provider_session_id: &str,
        relative_path: &str,
    ) -> Result<ChatSessionFileContent, G5Error> {
        self.chat_session_file_content(chat_session_id, provider_session_id, relative_path)
            .await
    }

    async fn put(
        &self,
        chat_session_id: &str,
        provider: SessionFileProvider,
        provider_session_id: &str,
        relative_path: &str,
        bytes: &[u8],
    ) -> Result<bool, G5Error> {
        Ok(self
            .put_chat_session_file(
                chat_session_id,
                provider,
                provider_session_id,
                relative_path,
                bytes,
                None,
            )
            .await?
            .changed)
    }
}

/// An in-memory [`SessionFileStore`] keyed like the table's primary key.
#[derive(Debug, Default)]
pub struct MemorySessionFileStore {
    files: Mutex<BTreeMap<(String, String, String), StoredFile>>,
    bundles: Mutex<BTreeMap<String, (uuid::Uuid, Option<SessionFileBundlePublication>)>>,
    clock: Mutex<i64>,
    content_reads: AtomicUsize,
    transferred_bytes: AtomicUsize,
}

#[derive(Debug, Clone)]
struct StoredFile {
    provider: SessionFileProvider,
    bytes: Vec<u8>,
    updated_at: DateTime<Utc>,
}

impl MemorySessionFileStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn content_reads(&self) -> usize {
        self.content_reads.load(Ordering::Relaxed)
    }

    pub fn transferred_bytes(&self) -> usize {
        self.transferred_bytes.load(Ordering::Relaxed)
    }

    /// Every stored `(provider session id, relative path) -> bytes` of one
    /// chat session.
    pub fn files_of(&self, chat_session_id: &str) -> BTreeMap<(String, String), Vec<u8>> {
        self.files
            .lock()
            .expect("store lock")
            .iter()
            .filter(|((chat, _, _), _)| chat == chat_session_id)
            .map(|((_, provider_session_id, path), file)| {
                (
                    (provider_session_id.clone(), path.clone()),
                    file.bytes.clone(),
                )
            })
            .collect()
    }

    /// Seed a row directly, as an earlier incarnation's harvest would have.
    pub fn insert(
        &self,
        chat_session_id: &str,
        provider: SessionFileProvider,
        provider_session_id: &str,
        relative_path: &str,
        bytes: &[u8],
    ) {
        let updated_at = self.tick();
        self.files.lock().expect("store lock").insert(
            (
                chat_session_id.to_string(),
                provider_session_id.to_string(),
                relative_path.to_string(),
            ),
            StoredFile {
                provider,
                bytes: bytes.to_vec(),
                updated_at,
            },
        );
    }

    fn tick(&self) -> DateTime<Utc> {
        let mut clock = self.clock.lock().expect("clock lock");
        *clock += 1;
        DateTime::<Utc>::from_timestamp(1_700_000_000 + *clock, 0).expect("valid timestamp")
    }
}

#[async_trait]
impl SessionFileStore for MemorySessionFileStore {
    async fn published(
        &self,
        chat_session_id: &str,
        provider: SessionFileProvider,
        provider_session_id: &str,
    ) -> Result<Option<SessionFileBundlePublication>, G5Error> {
        Ok(self
            .bundles
            .lock()
            .expect("bundle lock")
            .get(chat_session_id)
            .and_then(|(_, bundle)| bundle.as_ref())
            .filter(|bundle| {
                bundle.provider == provider && bundle.provider_session_id == provider_session_id
            })
            .cloned())
    }

    async fn begin(&self, chat_session_id: &str, generation: uuid::Uuid) -> Result<(), G5Error> {
        self.bundles
            .lock()
            .expect("bundle lock")
            .insert(chat_session_id.into(), (generation, None));
        Ok(())
    }

    async fn publish(
        &self,
        chat_session_id: &str,
        bundle: &SessionFileBundlePublication,
    ) -> Result<(), G5Error> {
        let mut bundles = self.bundles.lock().expect("bundle lock");
        let Some((generation, publication)) = bundles.get_mut(chat_session_id) else {
            return Err(G5Error::Conflict {
                reason: "bundle not begun".into(),
            });
        };
        if *generation == bundle.generation && publication.as_ref() == Some(bundle) {
            return Ok(());
        }
        if *generation != bundle.generation || publication.is_some() || bundle.files.is_empty() {
            return Err(G5Error::Conflict {
                reason: "bundle generation is not pending".into(),
            });
        }
        let files = self.files.lock().expect("store lock");
        if bundle.files.iter().any(|file| {
            files
                .get(&(
                    chat_session_id.into(),
                    bundle.provider_session_id.clone(),
                    file.relative_path.clone(),
                ))
                .is_none_or(|stored| {
                    stored.provider != bundle.provider
                        || stored.bytes.len() as i64 != file.size
                        || sha256_hex(&stored.bytes) != file.sha256
                })
        }) {
            return Err(G5Error::Conflict {
                reason: "bundle file missing or changed".into(),
            });
        }
        *publication = Some(bundle.clone());
        Ok(())
    }

    async fn put_fenced(
        &self,
        chat_session_id: &str,
        upload: FencedSessionFileUpload<'_>,
    ) -> Result<bool, G5Error> {
        let bundles = self.bundles.lock().expect("bundle lock");
        if !bundles
            .get(chat_session_id)
            .is_some_and(|(current, publication)| {
                *current == upload.generation && publication.is_none()
            })
        {
            return Err(G5Error::Conflict {
                reason: "bundle generation is not pending".into(),
            });
        }
        let key = (
            chat_session_id.to_string(),
            upload.provider_session_id.to_string(),
            upload.relative_path.to_string(),
        );
        let full_bytes = if let Some((base_size, base_sha256)) = upload.base {
            let files = self.files.lock().expect("store lock");
            let Some(stored) = files.get(&key) else {
                return Err(G5Error::Conflict {
                    reason: "session file append base is missing".into(),
                });
            };
            if stored.bytes.len() as i64 != base_size || sha256_hex(&stored.bytes) != base_sha256 {
                return Err(G5Error::Conflict {
                    reason: "session file append base changed".into(),
                });
            }
            let mut full = stored.bytes.clone();
            full.extend_from_slice(upload.bytes);
            full
        } else {
            upload.bytes.to_vec()
        };
        if sha256_hex(&full_bytes) != upload.full_sha256
            || full_bytes.len() as u64 > CHAT_SESSION_FILE_MAX_BYTES
        {
            return Err(G5Error::Conflict {
                reason: "session file append digest or size is invalid".into(),
            });
        }
        self.transferred_bytes
            .fetch_add(upload.bytes.len(), Ordering::Relaxed);
        let unchanged = self
            .files
            .lock()
            .expect("store lock")
            .get(&key)
            .is_some_and(|file| file.bytes == full_bytes);
        if !unchanged {
            self.insert(
                chat_session_id,
                upload.provider,
                upload.provider_session_id,
                upload.relative_path,
                &full_bytes,
            );
        }
        Ok(!unchanged)
    }
    async fn manifest(
        &self,
        chat_session_id: &str,
        provider_session_id: &str,
    ) -> Result<Vec<ChatSessionFileManifestItem>, G5Error> {
        Ok(self
            .files
            .lock()
            .expect("store lock")
            .iter()
            .filter(|((chat, id, _), _)| chat == chat_session_id && id == provider_session_id)
            .map(|((_, id, path), file)| ChatSessionFileManifestItem {
                provider: file.provider,
                provider_session_id: id.clone(),
                relative_path: path.clone(),
                sha256: sha256_hex(&file.bytes),
                size: file.bytes.len() as i64,
                updated_at: file.updated_at,
            })
            .collect())
    }

    async fn content(
        &self,
        chat_session_id: &str,
        provider_session_id: &str,
        relative_path: &str,
    ) -> Result<ChatSessionFileContent, G5Error> {
        self.content_reads.fetch_add(1, Ordering::Relaxed);
        let files = self.files.lock().expect("store lock");
        let file = files
            .get(&(
                chat_session_id.to_string(),
                provider_session_id.to_string(),
                relative_path.to_string(),
            ))
            .ok_or_else(|| G5Error::NotFound {
                resource: "chat_session_file".to_string(),
            })?;
        Ok(ChatSessionFileContent {
            provider: file.provider,
            sha256: sha256_hex(&file.bytes),
            bytes: file.bytes.clone(),
        })
    }

    async fn put(
        &self,
        chat_session_id: &str,
        provider: SessionFileProvider,
        provider_session_id: &str,
        relative_path: &str,
        bytes: &[u8],
    ) -> Result<bool, G5Error> {
        if self
            .bundles
            .lock()
            .expect("bundle lock")
            .get(chat_session_id)
            .and_then(|(_, publication)| publication.as_ref())
            .is_some_and(|publication| {
                publication.provider == provider
                    && publication.provider_session_id == provider_session_id
            })
        {
            return Err(G5Error::Conflict {
                reason: "legacy upload cannot replace a published session bundle".into(),
            });
        }
        let key = (
            chat_session_id.to_string(),
            provider_session_id.to_string(),
            relative_path.to_string(),
        );
        let unchanged = self
            .files
            .lock()
            .expect("store lock")
            .get(&key)
            .is_some_and(|file| file.bytes == bytes);
        if unchanged {
            return Ok(false);
        }
        self.insert(
            chat_session_id,
            provider,
            provider_session_id,
            relative_path,
            bytes,
        );
        Ok(true)
    }
}

/// Scan the session files of `provider_session_id` under `layout`. Missing
/// directories are not an error: a session that wrote nothing has no files.
pub fn scan(
    layout: &SessionFileLayout,
    provider_session_id: &str,
) -> Result<SessionFileScan, G5Error> {
    validate_provider_session_id(provider_session_id).map_err(|error| G5Error::Internal {
        message: format!("session file scan: {error}"),
    })?;
    let mut scan = SessionFileScan::default();
    match layout {
        SessionFileLayout::Claude { config_dir, cwd } => {
            let project_rel = SessionFileLayout::claude_project_rel(cwd);
            if !is_real_dir(&config_dir.join("projects"))
                || !is_real_dir(&config_dir.join(&project_rel))
            {
                return Ok(scan);
            }
            collect_file(
                config_dir,
                &format!("{project_rel}/{provider_session_id}.jsonl"),
                &mut scan,
            )?;
            collect_claude_session_dir(
                config_dir,
                &format!("{project_rel}/{provider_session_id}"),
                &mut scan,
            )?;
        }
        SessionFileLayout::Codex { codex_home } => {
            for relative_path in codex_rollouts(codex_home, provider_session_id)? {
                collect_file(codex_home, &relative_path, &mut scan)?;
            }
        }
    }
    scan.files
        .sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    Ok(scan)
}

/// Whether the file the provider resumes from is on this host: the Claude
/// transcript or a Codex rollout of the session.
pub fn resume_file_present(layout: &SessionFileLayout, provider_session_id: &str) -> bool {
    if validate_provider_session_id(provider_session_id).is_err() {
        return false;
    }
    match layout {
        SessionFileLayout::Claude { config_dir, cwd } => is_file_under(
            config_dir,
            &format!(
                "{}/{provider_session_id}.jsonl",
                SessionFileLayout::claude_project_rel(cwd)
            ),
        ),
        SessionFileLayout::Codex { codex_home } => codex_rollouts(codex_home, provider_session_id)
            .map(|rollouts| {
                rollouts
                    .iter()
                    .any(|relative_path| is_regular_file(&codex_home.join(relative_path)))
            })
            .unwrap_or(false),
    }
}

/// What [`harvest`] did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HarvestReport {
    /// Files found under the session's paths.
    pub found: usize,
    /// Files whose stored copy changed.
    pub uploaded: usize,
    /// Files already stored with the same bytes.
    pub unchanged: usize,
    pub skipped: Vec<SkippedSessionFile>,
}

/// Upload every session file whose sha256 differs from the stored copy.
/// Stops at the first failed upload; the files uploaded before it stay
/// stored, and the next harvest compares against them.
pub async fn harvest(
    store: &dyn SessionFileStore,
    chat_session_id: &str,
    layout: &SessionFileLayout,
    provider_session_id: &str,
) -> Result<HarvestReport, G5Error> {
    let scan = {
        let layout = layout.clone();
        let provider_session_id = provider_session_id.to_string();
        tokio::task::spawn_blocking(move || scan(&layout, &provider_session_id))
            .await
            .map_err(|error| G5Error::Internal {
                message: format!("session file scan task failed: {error}"),
            })??
    };
    let mut report = HarvestReport {
        found: scan.files.len(),
        skipped: scan.skipped,
        ..HarvestReport::default()
    };
    if scan.files.is_empty() {
        return Ok(report);
    }
    let stored: BTreeMap<String, String> = store
        .manifest(chat_session_id, provider_session_id)
        .await?
        .into_iter()
        .map(|item| (item.relative_path, item.sha256))
        .collect();
    let provider = layout.provider();
    for file in &scan.files {
        if stored.get(&file.relative_path) == Some(&file.sha256) {
            report.unchanged += 1;
            continue;
        }
        let changed = store
            .put(
                chat_session_id,
                provider,
                provider_session_id,
                &file.relative_path,
                &file.bytes,
            )
            .await?;
        if changed {
            report.uploaded += 1;
        } else {
            report.unchanged += 1;
        }
    }
    Ok(report)
}

/// What [`restore_missing`] found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreOutcome {
    /// Every stored file of the session, and the resume file, is already on
    /// this host; nothing was fetched.
    Present,
    /// The stored files this host lacked were written; the session can be
    /// resumed.
    Restored { files: usize },
    /// Postgres holds no file for this provider session and this host lacks
    /// the resume file: start a new one.
    NothingStored,
    /// Postgres holds files, but they cannot resume the session (no resume
    /// file among them, or they could not be written): start a new one.
    Unusable { reason: String },
}

/// Restore only a committed complete generation. Existing local bytes are
/// never treated as proof and are replaced after checking every stored file.
pub async fn restore_verified(
    store: &dyn SessionFileStore,
    chat_session_id: &str,
    layout: &SessionFileLayout,
    provider_session_id: &str,
) -> Result<RestoreOutcome, G5Error> {
    if validate_provider_session_id(provider_session_id).is_err() {
        return Ok(RestoreOutcome::Unusable {
            reason: "invalid provider session id".into(),
        });
    }
    let provider = layout.provider();
    let bundle = match store
        .published(chat_session_id, provider, provider_session_id)
        .await
    {
        Ok(bundle) => bundle,
        Err(error) => return classify_restore_store_error(error),
    };
    let Some(bundle) = bundle else {
        return Ok(RestoreOutcome::NothingStored);
    };
    if bundle.provider != provider
        || bundle.provider_session_id != provider_session_id
        || bundle.files.is_empty()
    {
        return Ok(RestoreOutcome::Unusable {
            reason: "published bundle identity is invalid".into(),
        });
    }
    let mut seen = std::collections::HashSet::new();
    let mut local_seen = std::collections::HashSet::new();
    let mut planned = Vec::new();
    for file in &bundle.files {
        if validate_session_file_path(provider, &file.relative_path).is_err()
            || !seen.insert(&file.relative_path)
        {
            return Ok(RestoreOutcome::Unusable {
                reason: "published bundle path is invalid".into(),
            });
        }
        let local_path = match layout {
            SessionFileLayout::Claude { cwd, .. } => {
                let expected = SessionFileLayout::claude_project_rel(cwd);
                if !file.relative_path.starts_with(&format!("{expected}/"))
                    || claude_session_rest(&file.relative_path, provider_session_id).is_none()
                {
                    return Ok(RestoreOutcome::Unusable {
                        reason: "published Claude session belongs to another working directory"
                            .into(),
                    });
                }
                file.relative_path.clone()
            }
            SessionFileLayout::Codex { .. } => {
                if !codex_rollout_path_matches(&file.relative_path, provider_session_id) {
                    return Ok(RestoreOutcome::Unusable {
                        reason: "published file belongs to another session".into(),
                    });
                }
                file.relative_path.clone()
            }
        };
        if !local_seen.insert(local_path.clone()) {
            return Ok(RestoreOutcome::Unusable {
                reason: "published bundle maps multiple files to one provider path".into(),
            });
        }
        planned.push((file.clone(), local_path));
    }
    if !planned
        .iter()
        .any(|(_, path)| resumes_from(layout, provider_session_id, path))
    {
        return Ok(RestoreOutcome::Unusable {
            reason: "published bundle lacks the resume file".into(),
        });
    }
    let cleanup_layout = layout.clone();
    let cleanup_id = provider_session_id.to_string();
    let expected: std::collections::HashSet<_> =
        planned.iter().map(|(_, path)| path.clone()).collect();
    let cleaned =
        tokio::task::spawn_blocking(move || -> Result<BTreeMap<String, (String, i64)>, String> {
            let local = scan(&cleanup_layout, &cleanup_id).map_err(|error| error.to_string())?;
            if !local.skipped.is_empty() {
                return Err("local session path cannot be safely replaced".into());
            }
            let mut matching = BTreeMap::new();
            for file in &local.files {
                if !expected.contains(&file.relative_path) {
                    std::fs::remove_file(cleanup_layout.home().join(&file.relative_path))
                        .map_err(|error| format!("could not remove stale session file: {error}"))?;
                } else {
                    matching.insert(
                        file.relative_path.clone(),
                        (file.sha256.clone(), file.bytes.len() as i64),
                    );
                }
            }
            Ok(matching)
        })
        .await
        .map_err(|error| G5Error::Internal {
            message: format!("session restore task failed: {error}"),
        })?;
    let matching = cleaned.map_err(|reason| G5Error::SessionFileStoreUnavailable {
        message: format!("session restore local cleanup failed: {reason}"),
    })?;
    let count = planned.len();
    for (file, local_path) in planned {
        if matching.get(&local_path) == Some(&(file.sha256.clone(), file.size)) {
            continue;
        }
        let content = match store
            .content(chat_session_id, provider_session_id, &file.relative_path)
            .await
        {
            Ok(content) => content,
            Err(error) => return classify_restore_store_error(error),
        };
        if content.provider != provider
            || content.bytes.len() as i64 != file.size
            || content.sha256 != file.sha256
            || sha256_hex(&content.bytes) != file.sha256
        {
            return Ok(RestoreOutcome::Unusable {
                reason: "published file integrity failed".into(),
            });
        }
        let home = layout.home().to_path_buf();
        let written = tokio::task::spawn_blocking(move || {
            write_session_file(&home, provider, &local_path, &content.bytes)
        })
        .await
        .map_err(|error| G5Error::Internal {
            message: format!("session restore task failed: {error}"),
        })?;
        written.map_err(|reason| G5Error::SessionFileStoreUnavailable {
            message: format!("session restore local write failed: {reason}"),
        })?;
    }
    Ok(RestoreOutcome::Restored { files: count })
}

fn classify_restore_store_error(error: G5Error) -> Result<RestoreOutcome, G5Error> {
    match error {
        G5Error::NotFound { .. }
        | G5Error::Conflict { .. }
        | G5Error::ValidationError { .. }
        | G5Error::SessionFileIntegrity { .. } => Ok(RestoreOutcome::Unusable {
            reason: "published session content is missing or invalid".into(),
        }),
        error => Err(G5Error::SessionFileStoreUnavailable {
            message: error.to_string(),
        }),
    }
}

/// Harvest and publish one complete file set under the turn's fence.
pub async fn harvest_complete(
    store: &dyn SessionFileStore,
    chat_session_id: &str,
    layout: &SessionFileLayout,
    provider_session_id: &str,
    generation: uuid::Uuid,
) -> Result<HarvestReport, G5Error> {
    let scan = {
        let layout = layout.clone();
        let id = provider_session_id.to_string();
        tokio::task::spawn_blocking(move || scan(&layout, &id))
            .await
            .map_err(|error| G5Error::SessionFileIntegrity {
                message: format!("session scan task failed: {error}"),
            })?
            .map_err(|error| G5Error::SessionFileIntegrity {
                message: format!("session scan failed: {error}"),
            })?
    };
    let mut report = HarvestReport {
        found: scan.files.len(),
        skipped: scan.skipped,
        ..HarvestReport::default()
    };
    if !report.skipped.is_empty() {
        return Ok(report);
    }
    if !scan
        .files
        .iter()
        .any(|file| resumes_from(layout, provider_session_id, &file.relative_path))
    {
        return Err(G5Error::SessionFileIntegrity {
            message: "session harvest lacks the provider resume file".into(),
        });
    }
    let stored: BTreeMap<String, (String, i64)> = store
        .manifest(chat_session_id, provider_session_id)
        .await?
        .into_iter()
        .filter(|item| {
            item.provider == layout.provider() && item.provider_session_id == provider_session_id
        })
        .map(|item| (item.relative_path, (item.sha256, item.size)))
        .collect();
    let mut files = Vec::new();
    for file in scan.files {
        if stored.get(&file.relative_path).map(|(sha256, _)| sha256) == Some(&file.sha256) {
            report.unchanged += 1;
        } else {
            let base = stored.get(&file.relative_path).and_then(|(sha256, size)| {
                let size = usize::try_from(*size).ok()?;
                (size < file.bytes.len() && sha256_hex(&file.bytes[..size]) == *sha256)
                    .then_some((size as i64, sha256.as_str()))
            });
            let bytes = base.map_or(file.bytes.as_slice(), |(size, _)| {
                &file.bytes[size as usize..]
            });
            let uploaded = store
                .put_fenced(
                    chat_session_id,
                    FencedSessionFileUpload {
                        provider: layout.provider(),
                        provider_session_id,
                        relative_path: &file.relative_path,
                        bytes,
                        full_sha256: &file.sha256,
                        base,
                        generation,
                    },
                )
                .await;
            let changed = match uploaded {
                Err(G5Error::Conflict { .. }) if base.is_some() => {
                    store
                        .put_fenced(
                            chat_session_id,
                            FencedSessionFileUpload {
                                provider: layout.provider(),
                                provider_session_id,
                                relative_path: &file.relative_path,
                                bytes: &file.bytes,
                                full_sha256: &file.sha256,
                                base: None,
                                generation,
                            },
                        )
                        .await?
                }
                result => result?,
            };
            if changed {
                report.uploaded += 1;
            } else {
                report.unchanged += 1;
            }
        }
        files.push(PublishedSessionFile {
            relative_path: file.relative_path,
            sha256: file.sha256,
            size: file.bytes.len() as i64,
        });
    }
    store
        .publish(
            chat_session_id,
            &SessionFileBundlePublication {
                generation,
                provider: layout.provider(),
                provider_session_id: provider_session_id.into(),
                files,
            },
        )
        .await?;
    Ok(report)
}

/// Before a turn that resumes `provider_session_id`: write back every stored
/// file of the session that this host lacks, so a subagent transcript comes
/// back even when the main transcript is still here, and a retry after a
/// partial restore finishes it. A file already on this host is kept: it is
/// the copy the provider appends to, and it is newer than the stored copy
/// when an earlier harvest failed. The resume file is written last. A Claude
/// transcript stored under another working directory's project key is
/// restored under the current one, where the CLI looks for it.
///
/// Errors are reserved for the store being unreachable or serving content
/// that fails its integrity check (that file is not written), so a caller
/// can fail the turn (retryable) instead of silently starting over.
pub async fn restore_missing(
    store: &dyn SessionFileStore,
    chat_session_id: &str,
    layout: &SessionFileLayout,
    provider_session_id: &str,
) -> Result<RestoreOutcome, G5Error> {
    if validate_provider_session_id(provider_session_id).is_err() {
        return Ok(RestoreOutcome::Unusable {
            reason: "the stored provider session id is not a valid id".to_string(),
        });
    }
    let provider = layout.provider();
    let manifest: Vec<ChatSessionFileManifestItem> = store
        .manifest(chat_session_id, provider_session_id)
        .await?
        .into_iter()
        .filter(|item| item.provider == provider)
        .collect();
    let plan = restore_plan(layout, provider_session_id, &manifest);
    let planned = plan.len();
    let mut missing: Vec<(String, String)> = plan
        .into_iter()
        .filter(|(_, local_path)| !is_file_under(layout.home(), local_path))
        .collect();
    // Stable: the other files keep their order and the resume file goes last.
    missing.sort_by_key(|(_, local_path)| resumes_from(layout, provider_session_id, local_path));
    let resume_file_stored = missing
        .iter()
        .any(|(_, local_path)| resumes_from(layout, provider_session_id, local_path));
    if !resume_file_stored && !resume_file_present(layout, provider_session_id) {
        return Ok(if manifest.is_empty() {
            RestoreOutcome::NothingStored
        } else if planned == 0 {
            RestoreOutcome::Unusable {
                reason: "no stored file is a session file of this provider session".to_string(),
            }
        } else {
            RestoreOutcome::Unusable {
                reason: "the stored files do not include the file the provider resumes from"
                    .to_string(),
            }
        });
    }
    if missing.is_empty() {
        return Ok(RestoreOutcome::Present);
    }
    let mut files = 0;
    for (stored_path, local_path) in missing {
        let content = store
            .content(chat_session_id, provider_session_id, &stored_path)
            .await?;
        if content.provider != provider {
            return Ok(RestoreOutcome::Unusable {
                reason: format!("stored file {stored_path:?} belongs to another provider"),
            });
        }
        let home = layout.home().to_path_buf();
        let written = tokio::task::spawn_blocking(move || {
            write_session_file(&home, provider, &local_path, &content.bytes)
        })
        .await
        .map_err(|error| G5Error::Internal {
            message: format!("session file restore task failed: {error}"),
        })?;
        if let Err(reason) = written {
            return Ok(RestoreOutcome::Unusable { reason });
        }
        files += 1;
    }
    if !resume_file_present(layout, provider_session_id) {
        return Ok(RestoreOutcome::Unusable {
            reason: "the stored files do not include the file the provider resumes from"
                .to_string(),
        });
    }
    Ok(RestoreOutcome::Restored { files })
}

/// Whether the provider resumes the session from `local_path`: the Claude
/// transcript, or a Codex rollout.
fn resumes_from(layout: &SessionFileLayout, provider_session_id: &str, local_path: &str) -> bool {
    match layout {
        SessionFileLayout::Claude { cwd, .. } => {
            local_path
                == format!(
                    "{}/{provider_session_id}.jsonl",
                    SessionFileLayout::claude_project_rel(cwd)
                )
        }
        SessionFileLayout::Codex { .. } => {
            codex_rollout_path_matches(local_path, provider_session_id)
        }
    }
}

/// `(stored relative path, local relative path)` for every stored file that
/// belongs to this provider session in this working directory.
fn restore_plan(
    layout: &SessionFileLayout,
    provider_session_id: &str,
    manifest: &[ChatSessionFileManifestItem],
) -> Vec<(String, String)> {
    let provider = layout.provider();
    match layout {
        SessionFileLayout::Claude { cwd, .. } => {
            let project_rel = SessionFileLayout::claude_project_rel(cwd);
            manifest
                .iter()
                .filter(|item| validate_session_file_path(provider, &item.relative_path).is_ok())
                .filter(|item| item.relative_path.starts_with(&format!("{project_rel}/")))
                .filter(|item| {
                    claude_session_rest(&item.relative_path, provider_session_id).is_some()
                })
                .map(|item| (item.relative_path.clone(), item.relative_path.clone()))
                .collect()
        }
        SessionFileLayout::Codex { .. } => manifest
            .iter()
            .filter(|item| validate_session_file_path(provider, &item.relative_path).is_ok())
            .filter(|item| codex_rollout_path_matches(&item.relative_path, provider_session_id))
            .map(|item| (item.relative_path.clone(), item.relative_path.clone()))
            .collect(),
    }
}

/// The part of a stored Claude path after `projects/<project key>/` when it
/// is this session's transcript or a file under its session directory.
fn claude_session_rest<'a>(relative_path: &'a str, provider_session_id: &str) -> Option<&'a str> {
    let rest = relative_path.strip_prefix("projects/")?;
    let (_project_key, rest) = rest.split_once('/')?;
    if rest == format!("{provider_session_id}.jsonl") {
        return Some(rest);
    }
    let name = rest.strip_prefix(&format!("{provider_session_id}/"))?;
    (!name.is_empty() && name.split('/').all(|segment| !segment.is_empty())).then_some(rest)
}

fn codex_rollout_name_matches(name: &str, provider_session_id: &str) -> bool {
    name.starts_with("rollout-")
        && (name.ends_with(&format!("-{provider_session_id}.jsonl"))
            || name.ends_with(&format!("-{provider_session_id}.jsonl.zst")))
}

fn codex_rollout_path_matches(relative_path: &str, provider_session_id: &str) -> bool {
    let segments: Vec<&str> = relative_path.split('/').collect();
    segments.len() >= 2
        && segments.len() <= CODEX_SESSION_DIR_DEPTH + 2
        && segments[0] == "sessions"
        && segments
            .last()
            .is_some_and(|name| codex_rollout_name_matches(name, provider_session_id))
}

/// Relative paths of the session's rollouts under `codex_home/sessions`,
/// walking at most the dated directories and never following a symlink.
fn codex_rollouts(codex_home: &Path, provider_session_id: &str) -> Result<Vec<String>, G5Error> {
    let mut found = Vec::new();
    if !is_real_dir(&codex_home.join("sessions")) {
        return Ok(found);
    }
    let mut stack = vec![("sessions".to_string(), 0usize)];
    while let Some((dir_rel, depth)) = stack.pop() {
        let entries = match std::fs::read_dir(codex_home.join(&dir_rel)) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(G5Error::Internal {
                    message: format!("failed to list Codex session directory: {error}"),
                })
            }
        };
        for entry in entries {
            let entry = entry.map_err(|error| G5Error::Internal {
                message: format!("failed to enumerate Codex session directory: {error}"),
            })?;
            let file_type = match entry.file_type() {
                Ok(file_type) => file_type,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => {
                    return Err(G5Error::Internal {
                        message: format!("failed to inspect Codex session entry: {error}"),
                    })
                }
            };
            let Some(name) = entry.file_name().to_str().map(str::to_string) else {
                // A non-UTF-8 name cannot be a valid provider rollout path.
                continue;
            };
            let relative_path = format!("{dir_rel}/{name}");
            if file_type.is_dir() {
                if depth < CODEX_SESSION_DIR_DEPTH {
                    stack.push((relative_path, depth + 1));
                }
            } else if codex_rollout_name_matches(&name, provider_session_id) {
                found.push(relative_path);
            }
        }
    }
    found.sort();
    Ok(found)
}

/// Read `relative_path` under `home` into the scan unless it is absent, a
/// symlink, or over the cap.
fn collect_file(
    home: &Path,
    relative_path: &str,
    scan: &mut SessionFileScan,
) -> Result<(), G5Error> {
    let path = home.join(relative_path);
    let metadata = match std::fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(G5Error::Internal {
                message: format!("failed to stat session file {relative_path:?}: {error}"),
            })
        }
    };
    if metadata.file_type().is_symlink() {
        scan.skipped.push(SkippedSessionFile {
            relative_path: relative_path.to_string(),
            reason: SkipReason::Symlink,
        });
        return Ok(());
    }
    if !metadata.is_file() {
        return Ok(());
    }
    if metadata.len() > CHAT_SESSION_FILE_MAX_BYTES {
        scan.skipped.push(SkippedSessionFile {
            relative_path: relative_path.to_string(),
            reason: SkipReason::TooLarge,
        });
        return Ok(());
    }
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(G5Error::Internal {
                message: format!("failed to read session file {relative_path:?}: {error}"),
            })
        }
    };
    if bytes.len() as u64 > CHAT_SESSION_FILE_MAX_BYTES {
        scan.skipped.push(SkippedSessionFile {
            relative_path: relative_path.to_string(),
            reason: SkipReason::TooLarge,
        });
        return Ok(());
    }
    validate_session_file_path(provider_for_rel(relative_path), relative_path).map_err(
        |error| G5Error::Internal {
            message: format!("session file {relative_path:?}: {error}"),
        },
    )?;
    scan.files.push(LocalSessionFile {
        relative_path: relative_path.to_string(),
        sha256: sha256_hex(&bytes),
        bytes,
    });
    Ok(())
}

/// Walk only real directories inside one Claude session. A symlink anywhere
/// in the set fails publication and prevents a partial verified restore.
fn collect_claude_session_dir(
    home: &Path,
    root_relative_path: &str,
    scan: &mut SessionFileScan,
) -> Result<(), G5Error> {
    let mut pending = vec![root_relative_path.to_string()];
    while let Some(relative_path) = pending.pop() {
        let path = home.join(&relative_path);
        let metadata = match std::fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(G5Error::Internal {
                    message: format!("failed to stat session path {relative_path:?}: {error}"),
                })
            }
        };
        if metadata.file_type().is_symlink() {
            scan.skipped.push(SkippedSessionFile {
                relative_path,
                reason: SkipReason::Symlink,
            });
        } else if metadata.is_dir() {
            let entries = std::fs::read_dir(&path).map_err(|error| G5Error::Internal {
                message: format!("failed to list {}: {error}", path.display()),
            })?;
            let mut names = Vec::new();
            for entry in entries {
                let entry = entry.map_err(|error| G5Error::Internal {
                    message: format!("failed to enumerate {}: {error}", path.display()),
                })?;
                if let Some(name) = entry.file_name().to_str() {
                    names.push(name.to_string());
                } else {
                    scan.skipped.push(SkippedSessionFile {
                        relative_path: relative_path.clone(),
                        reason: SkipReason::InvalidPath,
                    });
                }
            }
            names.sort();
            for name in names.into_iter().rev() {
                let child = format!("{relative_path}/{name}");
                if validate_session_file_path(SessionFileProvider::ClaudeCode, &child).is_ok() {
                    pending.push(child);
                } else {
                    scan.skipped.push(SkippedSessionFile {
                        relative_path: child,
                        reason: SkipReason::InvalidPath,
                    });
                }
            }
        } else {
            collect_file(home, &relative_path, scan)?;
        }
    }
    Ok(())
}

fn provider_for_rel(relative_path: &str) -> SessionFileProvider {
    if relative_path.starts_with("sessions/") {
        SessionFileProvider::Codex
    } else {
        SessionFileProvider::ClaudeCode
    }
}

/// Atomically write one restored file under `home`: every existing ancestor
/// between `home` and the file must be a real directory, never a symlink,
/// and the file replaces any previous copy by rename.
fn write_session_file(
    home: &Path,
    provider: SessionFileProvider,
    relative_path: &str,
    bytes: &[u8],
) -> Result<(), String> {
    validate_session_file_path(provider, relative_path)
        .map_err(|error| format!("stored file {relative_path:?}: {error}"))?;
    match std::fs::symlink_metadata(home) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err("the provider home is not a real directory".to_string());
        }
        Ok(_) => {}
        // A fresh host has no provider home yet.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir_all(home).map_err(|error| {
                format!(
                    "failed to create the provider home {}: {error}",
                    home.display()
                )
            })?;
        }
        Err(error) => {
            return Err(format!(
                "failed to stat the provider home {}: {error}",
                home.display()
            ))
        }
    }
    let destination = home.join(relative_path);
    let parent = destination
        .parent()
        .ok_or_else(|| format!("stored file {relative_path:?} has no parent directory"))?;
    let mut current = home.to_path_buf();
    let parent_rel = Path::new(relative_path)
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();
    for component in parent_rel.components() {
        current.push(component);
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(format!(
                    "{} is not a real directory",
                    current.strip_prefix(home).unwrap_or(&current).display()
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                std::fs::create_dir(&current)
                    .map_err(|error| format!("failed to create {}: {error}", current.display()))?;
            }
            Err(error) => return Err(format!("failed to stat {}: {error}", current.display())),
        }
    }
    if std::fs::symlink_metadata(&destination)
        .is_ok_and(|metadata| metadata.file_type().is_symlink() || metadata.is_dir())
    {
        return Err(format!("{relative_path:?} is not a regular file"));
    }
    let temp = unique_temp_path(parent)?;
    let write = (|| -> std::io::Result<()> {
        use std::io::Write as _;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temp, &destination)
    })();
    if let Err(error) = write {
        let _ = std::fs::remove_file(&temp);
        return Err(format!("failed to write {relative_path:?}: {error}"));
    }
    Ok(())
}

fn unique_temp_path(parent: &Path) -> Result<PathBuf, String> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or_default();
    (0..16u8)
        .map(|attempt| {
            parent.join(format!(
                ".g5-session-file-{}-{nanos}-{attempt}.tmp",
                std::process::id()
            ))
        })
        .find(|candidate| !candidate.exists())
        .ok_or_else(|| "failed to allocate a temporary session file path".to_string())
}

fn is_real_dir(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_dir())
}

fn is_regular_file(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_file())
}

/// Whether `relative_path` under `home` is a regular file reached through
/// real directories only, the rule [`scan`] and [`write_session_file`] apply.
fn is_file_under(home: &Path, relative_path: &str) -> bool {
    let mut current = home.to_path_buf();
    let mut components = Path::new(relative_path).components().peekable();
    while let Some(component) = components.next() {
        current.push(component);
        let Ok(metadata) = std::fs::symlink_metadata(&current) else {
            return false;
        };
        if components.peek().is_none() {
            return metadata.is_file();
        }
        if !metadata.is_dir() {
            return false;
        }
    }
    false
}

/// How a turn's session files stand after its harvest, as an adapter reports
/// it on the timeline (`providerSessionFilesState`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionFilesPersistence {
    /// Every file under the session's paths is stored.
    Stored,
    /// The turn left no session file on this host.
    NoFiles,
    /// At least one file is not stored. `reason` is a stable code for the
    /// timeline; `detail` is for logs only.
    Failed { reason: String, detail: String },
}

impl SessionFilesPersistence {
    /// The timeline value: `present`, `absent` or `persist_failed`.
    pub fn state(&self) -> &'static str {
        match self {
            SessionFilesPersistence::Stored => "present",
            SessionFilesPersistence::NoFiles => "absent",
            SessionFilesPersistence::Failed { .. } => "persist_failed",
        }
    }
}

/// [`harvest`] folded into the state a turn reports.
pub async fn harvest_for_turn(
    store: &dyn SessionFileStore,
    chat_session_id: &str,
    layout: &SessionFileLayout,
    provider_session_id: &str,
) -> SessionFilesPersistence {
    match harvest(store, chat_session_id, layout, provider_session_id).await {
        Ok(report) => match report.skipped.first() {
            None if report.found == 0 => SessionFilesPersistence::NoFiles,
            None => SessionFilesPersistence::Stored,
            Some(skipped) => SessionFilesPersistence::Failed {
                reason: match skipped.reason {
                    SkipReason::TooLarge => "session_file_too_large",
                    SkipReason::Symlink => "session_file_is_a_symlink",
                    SkipReason::InvalidPath => "session_file_path_invalid",
                }
                .to_string(),
                detail: format!("{} was not stored", skipped.relative_path),
            },
        },
        Err(error) => SessionFilesPersistence::Failed {
            reason: "session_file_upload_failed".to_string(),
            detail: error.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SID: &str = "0b7c5a9e-1f2d-4c3b-9a8e-7d6c5b4a3f2e";

    #[tokio::test]
    async fn complete_generation_restores_exact_bytes_and_pending_generation_blocks_stale_resume() {
        let store = MemorySessionFileStore::new();
        let source = tempfile::tempdir().unwrap();
        let layout = SessionFileLayout::Codex {
            codex_home: source.path().to_path_buf(),
        };
        let relative = format!("sessions/2026/10/02/rollout-2026-10-02T00-00-00-{SID}.jsonl");
        let path = source.path().join(&relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        // This fact exists only in the provider's rollout, never in a chat message.
        std::fs::write(&path, b"tool result: ORCHID-73").unwrap();
        let first = uuid::Uuid::new_v4();
        store.begin("chat-1", first).await.unwrap();
        let report = harvest_complete(&store, "chat-1", &layout, SID, first)
            .await
            .unwrap();
        assert_eq!(report.found, 1);
        let publication = store
            .published("chat-1", SessionFileProvider::Codex, SID)
            .await
            .unwrap()
            .unwrap();
        store.publish("chat-1", &publication).await.unwrap();
        let mut changed_publication = publication.clone();
        changed_publication.files[0].size += 1;
        assert!(matches!(
            store.publish("chat-1", &changed_publication).await,
            Err(G5Error::Conflict { .. })
        ));
        assert!(matches!(
            store
                .put(
                    "chat-1",
                    SessionFileProvider::Codex,
                    SID,
                    &relative,
                    b"legacy overwrite",
                )
                .await,
            Err(G5Error::Conflict { .. })
        ));

        let replacement = tempfile::tempdir().unwrap();
        let replacement_layout = SessionFileLayout::Codex {
            codex_home: replacement.path().to_path_buf(),
        };
        let stale = replacement.path().join(&relative);
        std::fs::create_dir_all(stale.parent().unwrap()).unwrap();
        std::fs::write(&stale, b"stale local rollout").unwrap();
        assert_eq!(
            restore_verified(&store, "chat-1", &replacement_layout, SID)
                .await
                .unwrap(),
            RestoreOutcome::Restored { files: 1 }
        );
        assert_eq!(std::fs::read(&stale).unwrap(), b"tool result: ORCHID-73");
        let reads = store.content_reads();
        assert_eq!(reads, 1);
        assert_eq!(
            restore_verified(&store, "chat-1", &replacement_layout, SID)
                .await
                .unwrap(),
            RestoreOutcome::Restored { files: 1 }
        );
        assert_eq!(
            store.content_reads(),
            reads,
            "warm verified bytes need no download"
        );
        std::fs::write(&stale, b"corrupt local rollout").unwrap();
        restore_verified(&store, "chat-1", &replacement_layout, SID)
            .await
            .unwrap();
        assert_eq!(
            store.content_reads(),
            reads + 1,
            "changed local bytes must be downloaded again"
        );

        #[cfg(unix)]
        {
            std::fs::remove_file(&stale).unwrap();
            std::os::unix::fs::symlink("missing-rollout", &stale).unwrap();
            assert!(
                matches!(
                    restore_verified(&store, "chat-1", &replacement_layout, SID).await,
                    Err(G5Error::SessionFileStoreUnavailable { .. })
                ),
                "unsafe local state must preserve the published bundle for retry"
            );
            assert!(
                store
                    .published("chat-1", SessionFileProvider::Codex, SID)
                    .await
                    .unwrap()
                    .is_some(),
                "a local restore failure preserves the published bundle"
            );
            std::fs::remove_file(&stale).unwrap();
            assert_eq!(
                restore_verified(&store, "chat-1", &replacement_layout, SID)
                    .await
                    .unwrap(),
                RestoreOutcome::Restored { files: 1 }
            );
        }

        let second = uuid::Uuid::new_v4();
        store.begin("chat-1", second).await.unwrap();
        std::fs::write(&stale, b"new uncommitted provider output").unwrap();
        assert_eq!(
            restore_verified(&store, "chat-1", &replacement_layout, SID)
                .await
                .unwrap(),
            RestoreOutcome::NothingStored
        );
        // A partial upload cannot make the generation resumable.
        store
            .put_fenced(
                "chat-1",
                FencedSessionFileUpload {
                    provider: SessionFileProvider::Codex,
                    provider_session_id: SID,
                    relative_path: &relative,
                    bytes: b"partial",
                    full_sha256: &sha256_hex(b"partial"),
                    base: None,
                    generation: second,
                },
            )
            .await
            .unwrap();
        assert_eq!(
            restore_verified(&store, "chat-1", &replacement_layout, SID)
                .await
                .unwrap(),
            RestoreOutcome::NothingStored
        );
        assert!(store
            .put_fenced(
                "chat-1",
                FencedSessionFileUpload {
                    provider: SessionFileProvider::Codex,
                    provider_session_id: SID,
                    relative_path: &relative,
                    bytes: b"late",
                    full_sha256: &sha256_hex(b"late"),
                    base: None,
                    generation: first,
                },
            )
            .await
            .is_err());
    }

    fn claude_layout(root: &Path) -> SessionFileLayout {
        SessionFileLayout::Claude {
            config_dir: root.join("claude"),
            cwd: PathBuf::from("/work/repo"),
        }
    }

    #[test]
    fn fenced_layout_uses_the_attempt_working_directory_for_both_callers() {
        let home = Path::new("/home/g5");
        let root = Path::new("/work");
        let relative = Path::new("repo/code");
        assert_eq!(
            SessionFileLayout::for_provider(SessionFileProvider::ClaudeCode, home, root, relative),
            SessionFileLayout::Claude {
                config_dir: home.join(".claude"),
                cwd: root.join(relative)
            },
        );
        assert_eq!(
            SessionFileLayout::for_provider(SessionFileProvider::Codex, home, root, relative),
            SessionFileLayout::Codex {
                codex_home: home.join(".codex")
            },
        );
    }

    #[test]
    fn root_target_claude_layout_finds_the_cli_project_key() {
        let home = tempfile::tempdir().unwrap();
        let root = Path::new("/work");
        let layout = SessionFileLayout::for_provider(
            SessionFileProvider::ClaudeCode,
            home.path(),
            root,
            Path::new(""),
        );
        let transcript = layout.home().join(format!("projects/-work/{SID}.jsonl"));
        write(&transcript, b"{\"type\":\"user\"}\n");
        assert!(resume_file_present(&layout, SID));
        assert_eq!(scan(&layout, SID).unwrap().files.len(), 1);
        let dot_target = SessionFileLayout::for_provider(
            SessionFileProvider::ClaudeCode,
            home.path(),
            root,
            Path::new("."),
        );
        assert!(resume_file_present(&dot_target, SID));
    }

    fn write(path: &Path, bytes: &[u8]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    fn seed_claude_session(config_dir: &Path) {
        let project = config_dir.join("projects/-work-repo");
        write(
            &project.join(format!("{SID}.jsonl")),
            b"{\"type\":\"user\"}\n",
        );
        write(
            &project.join(format!("{SID}/subagents/agent-1.jsonl")),
            b"{\"agent\":1}\n",
        );
        write(
            &project.join(format!("{SID}/subagents/agent-1.meta.json")),
            b"{}",
        );
        write(
            &project.join(format!("{SID}/tool-results/result.txt")),
            b"tool-only fact",
        );
        // Another session and a non-session file are not this session's.
        write(&project.join("other-session.jsonl"), b"other");
        write(&config_dir.join(".claude.json"), b"{}");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn verified_claude_restore_cleans_stale_files_and_restores_tool_results() {
        let store = MemorySessionFileStore::new();
        let source = tempfile::tempdir().unwrap();
        let source_layout = claude_layout(source.path());
        seed_claude_session(source_layout.home());
        let generation = uuid::Uuid::new_v4();
        store.begin("chat-1", generation).await.unwrap();
        let report = harvest_complete(&store, "chat-1", &source_layout, SID, generation)
            .await
            .unwrap();
        assert_eq!(report.found, 4);

        let target = tempfile::tempdir().unwrap();
        let target_layout = SessionFileLayout::Claude {
            config_dir: target.path().join("claude"),
            cwd: PathBuf::from("/work/repo"),
        };
        let project = target_layout.home().join("projects/-work-repo");
        let transcript = project.join(format!("{SID}.jsonl"));
        let stale = project.join(format!("{SID}/subagents/stale.jsonl"));
        write(&transcript, b"stale local transcript");
        write(&stale, b"stale local subagent");
        assert_eq!(
            restore_verified(&store, "chat-1", &target_layout, SID)
                .await
                .unwrap(),
            RestoreOutcome::Restored { files: 4 }
        );
        assert_eq!(std::fs::read(transcript).unwrap(), b"{\"type\":\"user\"}\n");
        assert!(!stale.exists());
        assert!(project
            .join(format!("{SID}/subagents/agent-1.jsonl"))
            .exists());
        assert_eq!(
            std::fs::read(project.join(format!("{SID}/tool-results/result.txt"))).unwrap(),
            b"tool-only fact"
        );
        let changed_directory = SessionFileLayout::Claude {
            config_dir: target.path().join("other-claude"),
            cwd: PathBuf::from("/another/workspace"),
        };
        assert!(matches!(
            restore_verified(&store, "chat-1", &changed_directory, SID)
                .await
                .unwrap(),
            RestoreOutcome::Unusable { .. }
        ));
        assert!(!changed_directory
            .home()
            .join(format!("projects/-another-workspace/{SID}.jsonl"))
            .exists());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn non_utf8_claude_session_file_prevents_complete_publication() {
        use std::os::unix::ffi::OsStringExt;
        let source = tempfile::tempdir().unwrap();
        let layout = claude_layout(source.path());
        seed_claude_session(layout.home());
        let session_dir = layout
            .home()
            .join(format!("projects/-work-repo/{SID}/subagents"));
        std::fs::write(
            session_dir.join(std::ffi::OsString::from_vec(vec![0xff])),
            b"opaque",
        )
        .unwrap();
        assert!(scan(&layout, SID)
            .unwrap()
            .skipped
            .iter()
            .any(|file| file.reason == SkipReason::InvalidPath));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn unrelated_non_utf8_codex_name_does_not_block_session_scan() {
        use std::os::unix::ffi::OsStringExt;

        let root = tempfile::tempdir().unwrap();
        let dated = root.path().join("sessions/2026/10/02");
        std::fs::create_dir_all(&dated).unwrap();
        let rollout = format!("rollout-2026-10-02T00-00-00-{SID}.jsonl");
        std::fs::write(dated.join(rollout), b"current session").unwrap();
        std::fs::write(
            dated.join(std::ffi::OsString::from_vec(vec![0xff])),
            b"unrelated",
        )
        .unwrap();
        let layout = SessionFileLayout::Codex {
            codex_home: root.path().to_path_buf(),
        };
        let scan = scan(&layout, SID).unwrap();
        assert_eq!(scan.files.len(), 1);
        assert_eq!(scan.files[0].bytes, b"current session");
    }

    #[test]
    fn claude_scan_finds_the_transcript_and_subagent_files_only() {
        let root = tempfile::tempdir().unwrap();
        let layout = claude_layout(root.path());
        seed_claude_session(layout.home());

        let scan = scan(&layout, SID).unwrap();
        let paths: Vec<&str> = scan
            .files
            .iter()
            .map(|file| file.relative_path.as_str())
            .collect();
        assert_eq!(
            paths,
            vec![
                format!("projects/-work-repo/{SID}.jsonl"),
                format!("projects/-work-repo/{SID}/subagents/agent-1.jsonl"),
                format!("projects/-work-repo/{SID}/subagents/agent-1.meta.json"),
                format!("projects/-work-repo/{SID}/tool-results/result.txt"),
            ]
        );
        assert_eq!(scan.files[0].sha256, sha256_hex(b"{\"type\":\"user\"}\n"));
        assert!(scan.skipped.is_empty());
        assert!(resume_file_present(&layout, SID));
        assert!(!resume_file_present(&layout, "missing"));
    }

    #[test]
    fn codex_scan_matches_rollouts_of_the_session_in_dated_directories() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("codex");
        let layout = SessionFileLayout::Codex {
            codex_home: home.clone(),
        };
        write(
            &home.join(format!(
                "sessions/2026/09/15/rollout-2026-09-15T01-02-03-{SID}.jsonl"
            )),
            b"rollout",
        );
        write(
            &home.join(format!("sessions/2026/09/14/rollout-x-{SID}.jsonl.zst")),
            b"zst",
        );
        write(
            &home.join("sessions/2026/09/15/rollout-x-other.jsonl"),
            b"other",
        );
        write(
            &home.join(format!("sessions/a/b/c/d/rollout-x-{SID}.jsonl")),
            b"too deep",
        );
        write(&home.join("config.toml"), b"model = \"x\"");

        let scan = scan(&layout, SID).unwrap();
        let paths: Vec<&str> = scan
            .files
            .iter()
            .map(|file| file.relative_path.as_str())
            .collect();
        assert_eq!(
            paths,
            vec![
                format!("sessions/2026/09/14/rollout-x-{SID}.jsonl.zst"),
                format!("sessions/2026/09/15/rollout-2026-09-15T01-02-03-{SID}.jsonl"),
            ]
        );
        assert!(resume_file_present(&layout, SID));
    }

    #[cfg(unix)]
    #[test]
    fn scan_never_follows_or_harvests_a_symlink() {
        let root = tempfile::tempdir().unwrap();
        let layout = claude_layout(root.path());
        let project = layout.home().join("projects/-work-repo");
        std::fs::create_dir_all(&project).unwrap();
        let secret = root.path().join("secret");
        std::fs::write(&secret, b"credential").unwrap();
        std::os::unix::fs::symlink(&secret, project.join(format!("{SID}.jsonl"))).unwrap();

        let scan = scan(&layout, SID).unwrap();
        assert!(scan.files.is_empty());
        assert_eq!(
            scan.skipped,
            vec![SkippedSessionFile {
                relative_path: format!("projects/-work-repo/{SID}.jsonl"),
                reason: SkipReason::Symlink,
            }]
        );
        assert!(!resume_file_present(&layout, SID));
    }

    #[tokio::test]
    async fn harvest_uploads_only_changed_files() {
        let root = tempfile::tempdir().unwrap();
        let layout = claude_layout(root.path());
        seed_claude_session(layout.home());
        let store = MemorySessionFileStore::new();

        let first = harvest(&store, "chat-1", &layout, SID).await.unwrap();
        assert_eq!((first.found, first.uploaded, first.unchanged), (4, 4, 0));
        let second = harvest(&store, "chat-1", &layout, SID).await.unwrap();
        assert_eq!((second.found, second.uploaded, second.unchanged), (4, 0, 4));

        write(
            &layout
                .home()
                .join(format!("projects/-work-repo/{SID}.jsonl")),
            b"{\"type\":\"user\"}\n{\"type\":\"assistant\"}\n",
        );
        let third = harvest(&store, "chat-1", &layout, SID).await.unwrap();
        assert_eq!((third.uploaded, third.unchanged), (1, 3));
        assert_eq!(
            store
                .files_of("chat-1")
                .get(&(SID.to_string(), format!("projects/-work-repo/{SID}.jsonl"))),
            Some(&b"{\"type\":\"user\"}\n{\"type\":\"assistant\"}\n".to_vec())
        );
    }

    #[tokio::test]
    async fn append_harvest_transfers_only_new_bytes_and_restores_full_content() {
        for provider in [SessionFileProvider::ClaudeCode, SessionFileProvider::Codex] {
            let source = tempfile::tempdir().unwrap();
            let layout = SessionFileLayout::for_provider(
                provider,
                source.path(),
                Path::new("/work"),
                Path::new("repo"),
            );
            let relative = match provider {
                SessionFileProvider::ClaudeCode => format!("projects/-work-repo/{SID}.jsonl"),
                SessionFileProvider::Codex => format!("sessions/2026/10/03/rollout-x-{SID}.jsonl"),
            };
            let path = layout.home().join(&relative);
            let first_bytes = vec![b'a'; 1024 * 1024];
            write(&path, &first_bytes);
            let tool_result = (provider == SessionFileProvider::ClaudeCode)
                .then(|| format!("projects/-work-repo/{SID}/tool-results/fact.txt"));
            if let Some(tool_result) = &tool_result {
                write(&layout.home().join(tool_result), b"tool-only fact");
            }
            let store = MemorySessionFileStore::new();
            let first = uuid::Uuid::new_v4();
            store.begin("chat-append", first).await.unwrap();
            harvest_complete(&store, "chat-append", &layout, SID, first)
                .await
                .unwrap();
            let transferred = store.transferred_bytes();

            let mut full = first_bytes;
            full.extend_from_slice(b"\nnew turn\n");
            write(&path, &full);
            let second = uuid::Uuid::new_v4();
            store.begin("chat-append", second).await.unwrap();
            harvest_complete(&store, "chat-append", &layout, SID, second)
                .await
                .unwrap();
            assert_eq!(
                store.transferred_bytes() - transferred,
                b"\nnew turn\n".len()
            );

            let target = tempfile::tempdir().unwrap();
            let target_layout = SessionFileLayout::for_provider(
                provider,
                target.path(),
                Path::new("/work"),
                Path::new("repo"),
            );
            assert_eq!(
                restore_verified(&store, "chat-append", &target_layout, SID)
                    .await
                    .unwrap(),
                RestoreOutcome::Restored {
                    files: if tool_result.is_some() { 2 } else { 1 }
                }
            );
            assert_eq!(
                std::fs::read(target_layout.home().join(&relative)).unwrap(),
                full
            );
            if let Some(tool_result) = &tool_result {
                assert_eq!(
                    std::fs::read(target_layout.home().join(tool_result)).unwrap(),
                    b"tool-only fact"
                );
            }
            let reads = store.content_reads();
            restore_verified(&store, "chat-append", &target_layout, SID)
                .await
                .unwrap();
            assert_eq!(
                store.content_reads(),
                reads,
                "warm {provider:?} bytes need no download"
            );
            write(&target_layout.home().join(&relative), b"local corruption");
            restore_verified(&store, "chat-append", &target_layout, SID)
                .await
                .unwrap();
            assert_eq!(
                store.content_reads(),
                reads + 1,
                "changed {provider:?} bytes are downloaded"
            );
            assert_eq!(
                std::fs::read(target_layout.home().join(&relative)).unwrap(),
                full,
                "changed {provider:?} bytes are restored exactly"
            );
        }
    }

    #[tokio::test]
    async fn changed_prefix_uses_a_full_upload() {
        for provider in [SessionFileProvider::ClaudeCode, SessionFileProvider::Codex] {
            let source = tempfile::tempdir().unwrap();
            let layout = SessionFileLayout::for_provider(
                provider,
                source.path(),
                Path::new("/work"),
                Path::new("repo"),
            );
            let relative = match provider {
                SessionFileProvider::ClaudeCode => format!("projects/-work-repo/{SID}.jsonl"),
                SessionFileProvider::Codex => format!("sessions/2026/10/03/rollout-x-{SID}.jsonl"),
            };
            write(&layout.home().join(&relative), b"original");
            let store = MemorySessionFileStore::new();
            let first = uuid::Uuid::new_v4();
            store.begin("chat-prefix", first).await.unwrap();
            harvest_complete(&store, "chat-prefix", &layout, SID, first)
                .await
                .unwrap();
            let transferred = store.transferred_bytes();
            write(&layout.home().join(&relative), b"rewritten longer");
            let second = uuid::Uuid::new_v4();
            store.begin("chat-prefix", second).await.unwrap();
            harvest_complete(&store, "chat-prefix", &layout, SID, second)
                .await
                .unwrap();
            assert_eq!(
                store.transferred_bytes() - transferred,
                b"rewritten longer".len()
            );
        }
    }

    #[tokio::test]
    async fn append_conflict_retries_once_as_a_whole_file() {
        struct ConflictOnce<'a> {
            inner: &'a MemorySessionFileStore,
            reject_append: std::sync::atomic::AtomicBool,
        }
        #[async_trait]
        impl SessionFileStore for ConflictOnce<'_> {
            async fn manifest(
                &self,
                chat: &str,
                sid: &str,
            ) -> Result<Vec<ChatSessionFileManifestItem>, G5Error> {
                self.inner.manifest(chat, sid).await
            }
            async fn content(
                &self,
                chat: &str,
                sid: &str,
                path: &str,
            ) -> Result<ChatSessionFileContent, G5Error> {
                self.inner.content(chat, sid, path).await
            }
            async fn put(
                &self,
                chat: &str,
                provider: SessionFileProvider,
                sid: &str,
                path: &str,
                bytes: &[u8],
            ) -> Result<bool, G5Error> {
                self.inner.put(chat, provider, sid, path, bytes).await
            }
            async fn put_fenced(
                &self,
                chat: &str,
                upload: FencedSessionFileUpload<'_>,
            ) -> Result<bool, G5Error> {
                if upload.base.is_some() && self.reject_append.swap(false, Ordering::SeqCst) {
                    return Err(G5Error::Conflict {
                        reason: "stored prefix changed".into(),
                    });
                }
                self.inner.put_fenced(chat, upload).await
            }
            async fn publish(
                &self,
                chat: &str,
                bundle: &SessionFileBundlePublication,
            ) -> Result<(), G5Error> {
                self.inner.publish(chat, bundle).await
            }
        }
        let source = tempfile::tempdir().unwrap();
        let layout = SessionFileLayout::Codex {
            codex_home: source.path().join("codex"),
        };
        let relative = format!("sessions/2026/10/03/rollout-x-{SID}.jsonl");
        let path = layout.home().join(relative);
        write(&path, b"prior");
        let store = MemorySessionFileStore::new();
        let first = uuid::Uuid::new_v4();
        store.begin("chat-conflict", first).await.unwrap();
        harvest_complete(&store, "chat-conflict", &layout, SID, first)
            .await
            .unwrap();
        let transferred = store.transferred_bytes();
        write(&path, b"prior plus suffix");
        let second = uuid::Uuid::new_v4();
        store.begin("chat-conflict", second).await.unwrap();
        let wrapper = ConflictOnce {
            inner: &store,
            reject_append: std::sync::atomic::AtomicBool::new(true),
        };
        harvest_complete(&wrapper, "chat-conflict", &layout, SID, second)
            .await
            .unwrap();
        assert_eq!(
            store.transferred_bytes() - transferred,
            b"prior plus suffix".len()
        );
    }

    #[tokio::test]
    async fn restore_writes_the_stored_files_into_an_empty_home() {
        let root = tempfile::tempdir().unwrap();
        let source = claude_layout(root.path());
        seed_claude_session(source.home());
        let store = MemorySessionFileStore::new();
        harvest(&store, "chat-1", &source, SID).await.unwrap();

        let target = SessionFileLayout::Claude {
            config_dir: root.path().join("fresh-home/.claude"),
            cwd: PathBuf::from("/work/repo"),
        };
        let outcome = restore_missing(&store, "chat-1", &target, SID)
            .await
            .unwrap();
        assert_eq!(outcome, RestoreOutcome::Restored { files: 4 });
        assert_eq!(
            std::fs::read(
                target
                    .home()
                    .join(format!("projects/-work-repo/{SID}.jsonl"))
            )
            .unwrap(),
            b"{\"type\":\"user\"}\n"
        );
        assert_eq!(
            restore_missing(&store, "chat-1", &target, SID)
                .await
                .unwrap(),
            RestoreOutcome::Present
        );
        assert_eq!(
            restore_missing(&store, "chat-1", &target, "never-stored")
                .await
                .unwrap(),
            RestoreOutcome::NothingStored
        );
    }

    #[tokio::test]
    async fn restore_never_rekeys_a_claude_transcript_to_another_working_directory() {
        let root = tempfile::tempdir().unwrap();
        let store = MemorySessionFileStore::new();
        let provider = SessionFileProvider::ClaudeCode;
        store.insert(
            "chat-1",
            provider,
            SID,
            &format!("projects/-old-worker-path/{SID}.jsonl"),
            b"older",
        );
        store.insert(
            "chat-1",
            provider,
            SID,
            &format!("projects/-newer-worker-path/{SID}.jsonl"),
            b"newer",
        );
        let layout = claude_layout(root.path());
        let outcome = restore_missing(&store, "chat-1", &layout, SID)
            .await
            .unwrap();
        assert!(matches!(outcome, RestoreOutcome::Unusable { .. }));
        assert!(!layout
            .home()
            .join(format!("projects/-work-repo/{SID}.jsonl"))
            .exists());
    }

    #[tokio::test]
    async fn restore_without_the_resume_file_is_unusable() {
        let root = tempfile::tempdir().unwrap();
        let store = MemorySessionFileStore::new();
        store.insert(
            "chat-1",
            SessionFileProvider::ClaudeCode,
            SID,
            &format!("projects/-work-repo/{SID}/subagents/agent-1.jsonl"),
            b"subagent only",
        );
        let layout = claude_layout(root.path());
        assert!(matches!(
            restore_missing(&store, "chat-1", &layout, SID)
                .await
                .unwrap(),
            RestoreOutcome::Unusable { .. }
        ));
        assert!(
            !layout.home().join("projects").exists(),
            "nothing is written for a session that cannot resume"
        );
    }

    /// Review P3.4 round 1: a transcript on this host does not mean every
    /// session file is. The missing subagent transcript comes back, and the
    /// local transcript, newer than the stored copy, is kept.
    #[tokio::test]
    async fn restore_brings_back_missing_files_and_keeps_the_ones_on_this_host() {
        let root = tempfile::tempdir().unwrap();
        let layout = claude_layout(root.path());
        let store = MemorySessionFileStore::new();
        let transcript = format!("projects/-work-repo/{SID}.jsonl");
        let subagent = format!("projects/-work-repo/{SID}/subagents/agent-1.jsonl");
        let provider = SessionFileProvider::ClaudeCode;
        store.insert("chat-1", provider, SID, &transcript, b"{\"turn\":1}\n");
        store.insert("chat-1", provider, SID, &subagent, b"{\"agent\":1}\n");
        // The last harvest failed: this host's transcript is one turn ahead.
        let local = b"{\"turn\":1}\n{\"turn\":2}\n";
        write(&layout.home().join(&transcript), local);

        assert_eq!(
            restore_missing(&store, "chat-1", &layout, SID)
                .await
                .unwrap(),
            RestoreOutcome::Restored { files: 1 }
        );
        assert_eq!(
            std::fs::read(layout.home().join(&subagent)).unwrap(),
            b"{\"agent\":1}\n"
        );
        assert_eq!(
            std::fs::read(layout.home().join(&transcript)).unwrap(),
            local
        );
        assert_eq!(
            restore_missing(&store, "chat-1", &layout, SID)
                .await
                .unwrap(),
            RestoreOutcome::Present
        );
    }

    /// Review P3.4 round 1: a restore that stopped after writing some files
    /// is finished by the next attempt instead of being taken as complete.
    #[tokio::test]
    async fn a_retry_finishes_a_partial_restore() {
        let root = tempfile::tempdir().unwrap();
        let layout = claude_layout(root.path());
        let store = MemorySessionFileStore::new();
        let files = [
            (
                format!("projects/-work-repo/{SID}.jsonl"),
                b"transcript" as &[u8],
            ),
            (
                format!("projects/-work-repo/{SID}/subagents/agent-1.jsonl"),
                b"agent 1",
            ),
            (
                format!("projects/-work-repo/{SID}/subagents/agent-2.jsonl"),
                b"agent 2",
            ),
        ];
        for (path, bytes) in &files {
            store.insert("chat-1", SessionFileProvider::ClaudeCode, SID, path, bytes);
        }
        // A directory in the way of the second subagent file fails its write
        // after the first one landed.
        let obstruction = layout.home().join(&files[2].0);
        std::fs::create_dir_all(&obstruction).unwrap();

        let first = restore_missing(&store, "chat-1", &layout, SID)
            .await
            .unwrap();
        assert!(
            matches!(first, RestoreOutcome::Unusable { .. }),
            "{first:?}"
        );
        assert_eq!(
            std::fs::read(layout.home().join(&files[1].0)).unwrap(),
            b"agent 1"
        );
        assert!(
            !resume_file_present(&layout, SID),
            "the transcript is written after every other file"
        );

        std::fs::remove_dir(&obstruction).unwrap();
        assert_eq!(
            restore_missing(&store, "chat-1", &layout, SID)
                .await
                .unwrap(),
            RestoreOutcome::Restored { files: 2 }
        );
        for (path, bytes) in &files {
            assert_eq!(std::fs::read(layout.home().join(path)).unwrap(), *bytes);
        }
    }

    /// Retargets the Codex mirror's
    /// `codex_harvest_rejects_oversized_rollout_before_reading`: a file past
    /// the cap is skipped on its size and never uploaded.
    #[tokio::test]
    async fn an_oversized_session_file_is_skipped_unread_and_never_uploaded() {
        let root = tempfile::tempdir().unwrap();
        let layout = SessionFileLayout::Codex {
            codex_home: root.path().join("codex"),
        };
        let relative_path = format!("sessions/2026/09/15/rollout-x-{SID}.jsonl");
        let path = layout.home().join(&relative_path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        // Sparse, so the test never writes 128 MiB.
        std::fs::File::create(&path)
            .unwrap()
            .set_len(CHAT_SESSION_FILE_MAX_BYTES + 1)
            .unwrap();
        // Unreadable as well: unless the test runs as root, a scan that read
        // the file before checking its size fails here.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
        }

        let scan = scan(&layout, SID).unwrap();
        assert!(scan.files.is_empty());
        assert_eq!(
            scan.skipped,
            vec![SkippedSessionFile {
                relative_path,
                reason: SkipReason::TooLarge,
            }]
        );
        let store = MemorySessionFileStore::new();
        let persistence = harvest_for_turn(&store, "chat-1", &layout, SID).await;
        assert!(
            matches!(
                persistence,
                SessionFilesPersistence::Failed { ref reason, .. } if reason == "session_file_too_large"
            ),
            "{persistence:?}"
        );
        assert!(store.files_of("chat-1").is_empty());
    }

    /// Retargets the Codex mirror's
    /// `codex_resume_restore_integrity_failures_do_not_write_artifact` through
    /// the real client: content whose sha256, size or compression does not
    /// check out fails the restore, and no local file is written.
    #[tokio::test]
    async fn corrupt_stored_content_fails_the_restore_without_writing_a_local_file() {
        use g5_internal_api::session_files::{
            encode_session_file_body, SESSION_FILE_PROVIDER_HEADER, SESSION_FILE_SHA256_HEADER,
            SESSION_FILE_SIZE_HEADER,
        };
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let relative_path = format!("sessions/2026/09/15/rollout-x-{SID}.jsonl");
        let bytes = b"{\"type\":\"session_meta\"}\n";
        let sha256 = sha256_hex(bytes);
        for corruption in ["sha256", "size", "zstd"] {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path(
                    "/api/internal/chat-sessions/chat-1/session-files/manifest",
                ))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "files": [{
                        "provider": "codex",
                        "providerSessionId": SID,
                        "relativePath": relative_path,
                        "sha256": sha256,
                        "size": bytes.len(),
                        "updatedAt": "2026-09-15T00:00:00Z",
                    }]
                })))
                .mount(&server)
                .await;
            let body = match corruption {
                "zstd" => b"not a zstd frame".to_vec(),
                _ => encode_session_file_body(bytes).unwrap(),
            };
            let reported_sha256 = match corruption {
                "sha256" => "0".repeat(64),
                _ => sha256.clone(),
            };
            let reported_size = match corruption {
                "size" => bytes.len() + 1,
                _ => bytes.len(),
            };
            Mock::given(method("GET"))
                .and(path(
                    "/api/internal/chat-sessions/chat-1/session-files/content",
                ))
                .respond_with(
                    ResponseTemplate::new(200)
                        .insert_header(SESSION_FILE_PROVIDER_HEADER, "codex")
                        .insert_header(SESSION_FILE_SHA256_HEADER, reported_sha256.as_str())
                        .insert_header(SESSION_FILE_SIZE_HEADER, reported_size.to_string().as_str())
                        .set_body_bytes(body),
                )
                .mount(&server)
                .await;
            let root = tempfile::tempdir().unwrap();
            let layout = SessionFileLayout::Codex {
                codex_home: root.path().join("codex"),
            };

            let result = restore_missing(
                &InternalApiClient::new(&server.uri()),
                "chat-1",
                &layout,
                SID,
            )
            .await;
            assert!(result.is_err(), "{corruption}: {result:?}");
            assert!(
                !layout.home().join("sessions").exists(),
                "{corruption}: nothing is written"
            );
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn restore_refuses_to_write_through_a_symlinked_directory() {
        let root = tempfile::tempdir().unwrap();
        let store = MemorySessionFileStore::new();
        store.insert(
            "chat-1",
            SessionFileProvider::Codex,
            SID,
            &format!("sessions/2026/09/15/rollout-x-{SID}.jsonl"),
            b"rollout",
        );
        let home = root.path().join("codex");
        let elsewhere = root.path().join("elsewhere");
        std::fs::create_dir_all(home.join("sessions/2026/09")).unwrap();
        std::fs::create_dir_all(&elsewhere).unwrap();
        std::os::unix::fs::symlink(&elsewhere, home.join("sessions/2026/09/15")).unwrap();
        let layout = SessionFileLayout::Codex {
            codex_home: home.clone(),
        };
        assert!(matches!(
            restore_missing(&store, "chat-1", &layout, SID)
                .await
                .unwrap(),
            RestoreOutcome::Unusable { .. }
        ));
        assert!(std::fs::read_dir(&elsewhere).unwrap().next().is_none());
    }

    #[test]
    fn stored_paths_that_are_not_this_sessions_files_are_not_restored() {
        let layout = claude_layout(Path::new("/tmp/unused"));
        let item = |path: &str| ChatSessionFileManifestItem {
            provider: SessionFileProvider::ClaudeCode,
            provider_session_id: SID.to_string(),
            relative_path: path.to_string(),
            sha256: sha256_hex(b"x"),
            size: 1,
            updated_at: DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap(),
        };
        let plan = restore_plan(
            &layout,
            SID,
            &[
                item(&format!("projects/-work-repo/{SID}.jsonl")),
                item(&format!("projects/-work-repo/{SID}/subagents/a.jsonl")),
                item(&format!("projects/-work-repo/{SID}/tool-results/a.txt")),
                item("projects/-k/other.jsonl"),
                item(&format!("projects/-k/../{SID}.jsonl")),
            ],
        );
        assert_eq!(
            plan,
            vec![
                (
                    format!("projects/-work-repo/{SID}.jsonl"),
                    format!("projects/-work-repo/{SID}.jsonl")
                ),
                (
                    format!("projects/-work-repo/{SID}/subagents/a.jsonl"),
                    format!("projects/-work-repo/{SID}/subagents/a.jsonl")
                ),
                (
                    format!("projects/-work-repo/{SID}/tool-results/a.txt"),
                    format!("projects/-work-repo/{SID}/tool-results/a.txt")
                ),
            ]
        );
    }

    #[tokio::test]
    async fn a_turn_reports_stored_absent_or_failed_session_files() {
        let root = tempfile::tempdir().unwrap();
        let layout = claude_layout(root.path());
        let store = MemorySessionFileStore::new();
        assert_eq!(
            harvest_for_turn(&store, "chat-1", &layout, SID).await,
            SessionFilesPersistence::NoFiles
        );
        seed_claude_session(layout.home());
        let stored = harvest_for_turn(&store, "chat-1", &layout, SID).await;
        assert_eq!(stored, SessionFilesPersistence::Stored);
        assert_eq!(stored.state(), "present");

        struct Unreachable;
        #[async_trait]
        impl SessionFileStore for Unreachable {
            async fn manifest(
                &self,
                _: &str,
                _: &str,
            ) -> Result<Vec<ChatSessionFileManifestItem>, G5Error> {
                Err(G5Error::Internal {
                    message: "api down".to_string(),
                })
            }
            async fn content(
                &self,
                _: &str,
                _: &str,
                _: &str,
            ) -> Result<ChatSessionFileContent, G5Error> {
                unreachable!()
            }
            async fn put(
                &self,
                _: &str,
                _: SessionFileProvider,
                _: &str,
                _: &str,
                _: &[u8],
            ) -> Result<bool, G5Error> {
                unreachable!()
            }
        }
        let failed = harvest_for_turn(&Unreachable, "chat-1", &layout, SID).await;
        assert_eq!(failed.state(), "persist_failed");
        assert!(matches!(
            failed,
            SessionFilesPersistence::Failed { ref reason, .. } if reason == "session_file_upload_failed"
        ));
        assert!(restore_missing(
            &Unreachable,
            "chat-1",
            &claude_layout(&root.path().join("empty")),
            SID
        )
        .await
        .is_err());
    }
}
