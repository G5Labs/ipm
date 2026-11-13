//! Email: G5's shared outbound transactional email mechanism.
//!
//! Any context may request a typed message for one recipient; this module
//! renders it, records the request in the caller's transaction, and a
//! maintenance worker delivers it through the configured provider
//! (`.g5/ontology/software_architecture/email.g5.md`). The module names no
//! other context: a consumer's "still wanted?" rule reaches it only through a
//! [`DeliveryGate`] the composition root installs in [`EmailGates`].
//!
//! Public: [`Email`], [`EmailRequest`], [`Sender`], [`Message`] and its data structs,
//! [`PurposeKind`], [`RequestOutcome`], [`DeliveryStatus`], [`DeliveryGate`],
//! [`EmailGates`], [`EmailWorker`] and the capture handle. Provider clients,
//! transport errors, retry policy, SQL and templates stay private, so no
//! caller can pick a provider, send directly or write delivery rows:
//!
//! ```
//! use g5_api_core::services::email::{Email, PurposeKind};
//! assert!(!Email::disabled().is_enabled());
//! assert_eq!(PurposeKind::OrgInvitation.daily_limit_per_org(), Some(100));
//! assert_eq!(PurposeKind::WorkspaceOwnerMessage.daily_limit_per_recipient(), Some(50));
//! assert_eq!(
//!     g5_api_core::services::email::link_path(&["acme", "feature/x y"]),
//!     "/acme/feature%2Fx%20y"
//! );
//! ```
//!
//! ```compile_fail
//! use g5_api_core::services::email::transport::ResendTransport;
//! ```
//!
//! ```compile_fail
//! use g5_api_core::services::email::store::insert_or_coalesce;
//! ```
//!
//! A consumer's mailbox name is checked when G5 is built (`g5.email.sender`):
//!
//! ```
//! use g5_api_core::services::email::Sender;
//! const APPROVALS: Sender = Sender::new("approvals");
//! assert_eq!(APPROVALS.mailbox(), "approvals");
//! ```
//!
//! ```compile_fail
//! use g5_api_core::services::email::Sender;
//! const BAD: Sender = Sender::new("Approvals Team");
//! ```
//!
//! ```compile_fail
//! use g5_api_core::services::email::Sender;
//! const ROLE: Sender = Sender::new("postmaster");
//! ```
//!
//! A consumer may also show its own display name, checked the same way:
//!
//! ```
//! use g5_api_core::services::email::Sender;
//! const APPROVALS: Sender = Sender::new("approvals").with_display_name("G5 Approvals");
//! assert_eq!(APPROVALS.display_name(), Some("G5 Approvals"));
//! ```
//!
//! ```compile_fail
//! use g5_api_core::services::email::Sender;
//! const SPOOF: Sender = Sender::new("approvals").with_display_name("Bank <x@y.z>");
//! ```
//!
//! ```compile_fail
//! // A worker comes only from a configured `Email` handle.
//! let _ = g5_api_core::services::email::EmailWorker::new;
//! ```
//!
//! The layering is pinned by `backend/g5-api/tests/service_context_layering.rs`.

mod markdown;
mod render;
mod store;
mod transport;
mod worker;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use crate::config::{EmailConfig, EmailTransportKind, EmailVolumeLimits};
use chrono::{DateTime, Utc};
use g5_contracts::error::G5Error;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

pub use transport::{CaptureTransport, CapturedEmail};
pub use worker::{EmailBatchResult, EmailWorker};

/// What a delivery request is for. A purpose kind fixes the consumer's
/// validity check and its per-organization volume limit; a request's purpose
/// key (chosen by the consumer) identifies the live request it coalesces with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PurposeKind {
    /// An organization invitation (`g5.organization.invitation-email`); the
    /// key is the invitation id.
    OrgInvitation,
    /// A workspace agent's message to the workspace owner
    /// (`g5.workspace.owner-email`); the key is `{workspace_id}:{caller key}`.
    WorkspaceOwnerMessage,
    /// A human reviewer's approval was requested
    /// (`g5.governance.approval.request-email`); the key is
    /// `{request_id}:{org_id}:{reviewer_user_id}`.
    ApprovalRequest,
    /// A human reviewer approved or requested changes on their request
    /// (`g5.governance.approval.decision-email`); the key is
    /// `{request_id}:{org_id}:{owner_user_id}`.
    ApprovalDecision,
}

impl PurposeKind {
    const ALL: [Self; 4] = [
        Self::OrgInvitation,
        Self::WorkspaceOwnerMessage,
        Self::ApprovalRequest,
        Self::ApprovalDecision,
    ];

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OrgInvitation => "org_invitation",
            Self::WorkspaceOwnerMessage => "workspace_owner_message",
            Self::ApprovalRequest => "approval_request",
            Self::ApprovalDecision => "approval_decision",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == value)
    }

    /// New requests one organization may make for this kind in a rolling
    /// 24 hours (`g5.email.volume-limits`).
    #[must_use]
    pub fn daily_limit_per_org(self) -> Option<u32> {
        match self {
            // g5.organization.invitation-email-limit
            Self::OrgInvitation => Some(100),
            // g5.workspace.owner-email-limit
            Self::WorkspaceOwnerMessage => Some(500),
            // g5.governance.approval.request-email (g5.email.volume-limits)
            Self::ApprovalRequest => Some(500),
            // g5.governance.approval.decision-email (g5.email.volume-limits)
            Self::ApprovalDecision => Some(500),
        }
    }

    /// New requests one recipient may receive for this kind in a rolling
    /// 24 hours, counted alongside the organization limit
    /// (`g5.email.volume-limits`).
    #[must_use]
    pub fn daily_limit_per_recipient(self) -> Option<u32> {
        match self {
            Self::OrgInvitation => None,
            // g5.workspace.owner-email-limit
            Self::WorkspaceOwnerMessage => Some(50),
            // g5.governance.approval.request-email (g5.email.volume-limits)
            Self::ApprovalRequest => Some(50),
            // g5.governance.approval.decision-email (g5.email.volume-limits)
            Self::ApprovalDecision => Some(50),
        }
    }

    /// Whether a purpose key names one caller request, so repeating it
    /// returns the existing request, live or ended, instead of a new message
    /// (`g5.email.identity`).
    #[must_use]
    pub fn single_use_keys(self) -> bool {
        match self {
            Self::OrgInvitation => false,
            Self::WorkspaceOwnerMessage => true,
            // The request is a lasting object; its id coalesces, never resends.
            Self::ApprovalRequest => false,
            // A request settles once, so its decision email is one event: a
            // replayed decision returns the existing email, sent or not.
            Self::ApprovalDecision => true,
        }
    }
}

/// A typed message. Each variant carries only data and app-relative paths;
/// this module renders it and resolves links against the public URL.
#[derive(Debug, Clone)]
pub enum Message {
    OrgInvitation(OrgInvitationEmail),
    WorkspaceOwnerMessage(WorkspaceOwnerEmail),
    ApprovalRequest(ApprovalRequestEmail),
    ApprovalDecision(ApprovalDecisionEmail),
}

impl Message {
    #[must_use]
    pub fn purpose_kind(&self) -> PurposeKind {
        match self {
            Self::OrgInvitation(_) => PurposeKind::OrgInvitation,
            Self::WorkspaceOwnerMessage(_) => PurposeKind::WorkspaceOwnerMessage,
            Self::ApprovalRequest(_) => PurposeKind::ApprovalRequest,
            Self::ApprovalDecision(_) => PurposeKind::ApprovalDecision,
        }
    }

    fn purpose_key(&self) -> String {
        match self {
            Self::OrgInvitation(message) => message.invitation_id.to_string(),
            Self::WorkspaceOwnerMessage(message) => {
                format!("{}:{}", message.workspace_id, message.request_key)
            }
            Self::ApprovalRequest(message) => format!(
                "{}:{}:{}",
                message.request_id, message.org_id, message.reviewer_user_id
            ),
            Self::ApprovalDecision(message) => format!(
                "{}:{}:{}",
                message.request_id, message.org_id, message.owner_user_id
            ),
        }
    }
}

/// Data for an organization invitation email.
#[derive(Debug, Clone)]
pub struct OrgInvitationEmail {
    pub invitation_id: Uuid,
    pub org_name: String,
    pub inviter_name: String,
    pub role: String,
    /// App-relative path the link opens, e.g. `/acme`.
    pub link_path: String,
}

/// How the module renders a body G5 did not write (`g5.email.authored-content`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BodyFormat {
    /// Plain text, escaped with its line breaks kept.
    #[default]
    Text,
    /// The Markdown subset (`g5.email.markdown`).
    Markdown,
}

/// Data for a workspace agent's message to the workspace owner. The subject
/// is the agent's plain text and the body is in `format`; the module adds the
/// authorship notice, the footer and the workspace link.
#[derive(Debug, Clone)]
pub struct WorkspaceOwnerEmail {
    pub workspace_id: Uuid,
    /// The caller's idempotency key; with the workspace id, the purpose key.
    pub request_key: String,
    pub workspace_name: String,
    pub org_name: String,
    /// One line, at most [`WorkspaceOwnerEmail::MAX_SUBJECT_CHARS`].
    pub subject: String,
    /// The agent's source text, at most
    /// [`WorkspaceOwnerEmail::MAX_BODY_CHARS`].
    pub body: String,
    pub format: BodyFormat,
    /// `https` repository web roots a Markdown link may lead into, besides
    /// G5 itself (`g5.email.link-policy`).
    pub link_scopes: Vec<String>,
    /// App-relative path of the workspace page; see [`link_path`].
    pub link_path: String,
}

impl WorkspaceOwnerEmail {
    pub const MAX_SUBJECT_CHARS: usize = 120;
    pub const MAX_BODY_CHARS: usize = 20_000;
}

/// Data for the email that tells a human reviewer their approval was requested
/// (`g5.governance.approval.request-email`). The names are G5-controlled text;
/// the note and PR description are text G5 did not write
/// (`g5.email.authored-content`), labelled and bounded: the note is plain text,
/// the description the Markdown subset.
/// The module renders the fixed `[G5 Approval]` subject and the body, and
/// resolves the links.
#[derive(Debug, Clone)]
pub struct ApprovalRequestEmail {
    /// The approval request id, its organization and the reviewer it emails
    /// together form the purpose key, so the delivery gate can re-check the
    /// reviewer without reading the tenant-scoped request.
    pub request_id: Uuid,
    pub org_id: Uuid,
    pub reviewer_user_id: Uuid,
    pub workspace_name: String,
    pub org_name: String,
    /// The approval definition's name.
    pub definition_name: String,
    /// Display name of the owner who requested the review.
    pub requester_name: String,
    /// The requester's note on this request, if any.
    pub note: Option<String>,
    /// The PR's canonical description, if any; the module shows its start.
    pub pull_request_description: Option<String>,
    /// `https` repository web roots a description link may lead into, besides
    /// G5 itself (`g5.email.link-policy`).
    pub link_scopes: Vec<String>,
    /// App-relative path of the request's page; see [`link_path`].
    pub link_path: String,
    /// App-relative path of the Settings page holding the reviewer's
    /// notification preference; see [`link_path`].
    pub settings_link_path: String,
}

impl ApprovalRequestEmail {
    /// Request notes are at most 8192 bytes, so a note is shown whole unless
    /// it is unusually long.
    pub const MAX_NOTE_CHARS: usize = 8_192;
    /// The PR description is shortened to its first lines: enough to judge
    /// the request, with the full text one click away.
    pub const MAX_DESCRIPTION_CHARS: usize = 1_500;
    pub const MAX_DESCRIPTION_LINES: usize = 30;
}

/// Data for the email that tells a workspace owner a human reviewer approved
/// or requested changes on their request
/// (`g5.governance.approval.decision-email`). The names are G5-controlled
/// text; the reviewer's comment is text G5 did not write
/// (`g5.email.authored-content`), shown labelled as plain text. The module
/// renders the fixed `[G5 Approval]` subject and the body, and resolves the
/// links.
#[derive(Debug, Clone)]
pub struct ApprovalDecisionEmail {
    /// The settled request id, its organization and the owner it emails
    /// together form the purpose key, so the delivery gate can re-check the
    /// owner without reading the tenant-scoped request.
    pub request_id: Uuid,
    pub org_id: Uuid,
    pub owner_user_id: Uuid,
    pub workspace_name: String,
    pub org_name: String,
    /// The approval definition's name.
    pub definition_name: String,
    /// Display name of the reviewer who decided.
    pub reviewer_name: String,
    pub outcome: ApprovalDecisionOutcome,
    /// The reviewer's comment, if any.
    pub comment: Option<String>,
    /// App-relative path of the workspace's approvals; see [`link_path`].
    pub link_path: String,
    /// App-relative path of the Settings page holding the owner's
    /// notification preference; see [`link_path`].
    pub settings_link_path: String,
}

impl ApprovalDecisionEmail {
    /// Decision comments are bounded like request notes.
    pub const MAX_COMMENT_CHARS: usize = 8_192;
}

/// The settled outcomes an approval-decision email reports; a withdrawal is
/// never emailed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalDecisionOutcome {
    Approved,
    ChangesRequested,
}

impl ApprovalDecisionOutcome {
    /// The emailed outcome of a recorded decision, `None` for a withdrawal.
    #[must_use]
    pub fn from_recorded(outcome: &str) -> Option<Self> {
        match outcome {
            "approved" => Some(Self::Approved),
            "rejected" => Some(Self::ChangesRequested),
            _ => None,
        }
    }
}

/// An app-relative link path from raw segments, each percent-encoded so any
/// name (for example a workspace's Git branch name) stays one segment.
#[must_use]
pub fn link_path(segments: &[&str]) -> String {
    segments
        .iter()
        .map(|segment| format!("/{}", render::encode_segment(segment)))
        .collect()
}

/// Who a consumer's messages come from (`g5.email.sender`): its mailbox at the
/// configured sending domain and, optionally, its own display name in place of
/// the configured one, e.g. `G5 <invitations@mail.g5labs.ai>` or
/// `G5 Approvals <approvals@mail.g5labs.ai>`. Declare it as a `const` so the
/// checks in [`Sender::new`] and [`Sender::with_display_name`] run when G5 is
/// built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sender {
    mailbox: &'static str,
    display_name: Option<&'static str>,
}

/// RFC 2142 role mailboxes no consumer may send as.
const RESERVED_MAILBOXES: [&str; 7] = [
    "postmaster",
    "abuse",
    "hostmaster",
    "webmaster",
    "security",
    "root",
    "mailer-daemon",
];

impl Sender {
    /// A mailbox name of 1-32 lowercase letters, digits, dots, hyphens or
    /// underscores that starts and ends with a letter or digit, has no repeated
    /// dot and is not a role mailbox. In a `const` an invalid name fails the build.
    #[must_use]
    pub const fn new(mailbox: &'static str) -> Self {
        let bytes = mailbox.as_bytes();
        assert!(
            !bytes.is_empty() && bytes.len() <= 32,
            "email sender mailbox must be 1-32 characters"
        );
        let mut i = 0;
        while i < bytes.len() {
            let c = bytes[i];
            let alphanumeric = c.is_ascii_lowercase() || c.is_ascii_digit();
            assert!(
                alphanumeric || c == b'.' || c == b'-' || c == b'_',
                "email sender mailbox allows lowercase letters, digits, '.', '-' and '_'"
            );
            assert!(
                alphanumeric || (i != 0 && i != bytes.len() - 1),
                "email sender mailbox must start and end with a letter or digit"
            );
            assert!(
                !(c == b'.' && bytes[i + 1] == b'.'),
                "email sender mailbox must not repeat '.'"
            );
            i += 1;
        }
        let mut r = 0;
        while r < RESERVED_MAILBOXES.len() {
            assert!(
                !bytes_eq(bytes, RESERVED_MAILBOXES[r].as_bytes()),
                "email sender mailbox must not be a role mailbox"
            );
            r += 1;
        }
        Self {
            mailbox,
            display_name: None,
        }
    }

    /// Show `name` instead of the configured display name: 1-64 letters,
    /// digits, spaces, dots, hyphens or underscores, without leading or
    /// trailing spaces, so it needs no quoting in a `From` header.
    #[must_use]
    pub const fn with_display_name(self, name: &'static str) -> Self {
        let bytes = name.as_bytes();
        assert!(
            !bytes.is_empty() && bytes.len() <= 64,
            "email sender display name must be 1-64 characters"
        );
        assert!(
            bytes[0] != b' ' && bytes[bytes.len() - 1] != b' ',
            "email sender display name must not start or end with a space"
        );
        let mut i = 0;
        while i < bytes.len() {
            let c = bytes[i];
            assert!(
                c.is_ascii_alphanumeric() || c == b' ' || c == b'.' || c == b'-' || c == b'_',
                "email sender display name allows letters, digits, spaces, '.', '-' and '_'"
            );
            i += 1;
        }
        Self {
            mailbox: self.mailbox,
            display_name: Some(name),
        }
    }

    #[must_use]
    pub const fn mailbox(self) -> &'static str {
        self.mailbox
    }

    /// The consumer's own display name, if it set one.
    #[must_use]
    pub const fn display_name(self) -> Option<&'static str> {
        self.display_name
    }

    /// The `From` mailbox: this sender's display name, or `default_name`,
    /// with its mailbox at `domain`.
    fn mailbox_header(self, default_name: &str, domain: &str) -> String {
        format!(
            "{} <{}@{domain}>",
            self.display_name.unwrap_or(default_name),
            self.mailbox
        )
    }
}

const fn bytes_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

/// A request for one message to one recipient in one organization.
#[derive(Debug, Clone)]
pub struct EmailRequest {
    pub org_id: Uuid,
    /// The consumer's mailbox at the configured sending domain.
    pub sender: Sender,
    pub recipient: String,
    pub message: Message,
}

/// The result of [`Email::request`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestOutcome {
    /// A new message identity was recorded.
    Queued(Uuid),
    /// A live request for the same purpose exists and is now due.
    Coalesced(Uuid),
    /// The kind's single-use key was already used; nothing was recorded and
    /// the existing request, live or ended, is returned.
    Existing(Uuid),
    /// Email is disabled in this deployment; nothing was recorded.
    Disabled,
}

#[derive(Debug, thiserror::Error)]
pub enum EmailError {
    /// The kind's per-organization volume limit is reached.
    #[error("email volume limit reached")]
    Limited { retry_after: Duration },
    /// The request cannot be rendered (bad link path or recipient).
    #[error("invalid email request: {0}")]
    InvalidRequest(&'static str),
    /// Authored content the module refuses to render, with the reason
    /// (`g5.email.markdown`).
    #[error("invalid {field}: {reason}")]
    InvalidContent { field: &'static str, reason: String },
    #[error("email store error: {0}")]
    Database(#[from] sqlx::Error),
}

impl From<EmailError> for G5Error {
    fn from(error: EmailError) -> Self {
        match error {
            EmailError::Limited { retry_after } => G5Error::RateLimited {
                retry_after_secs: Some(retry_after.as_secs().max(1)),
            },
            EmailError::InvalidRequest(reason) => G5Error::ValidationError {
                details: serde_json::json!({ "field": "email", "error": reason }),
            },
            EmailError::InvalidContent { field, reason } => G5Error::ValidationError {
                details: serde_json::json!({ "field": field, "error": reason }),
            },
            EmailError::Database(error) => G5Error::Internal {
                message: format!("Failed to record email delivery: {error}"),
            },
        }
    }
}

/// Lifecycle state of one delivery request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryState {
    Queued,
    Sending,
    Sent,
    Failed,
    Cancelled,
}

impl DeliveryState {
    fn parse(value: &str) -> Self {
        match value {
            "queued" => Self::Queued,
            "sending" => Self::Sending,
            "sent" => Self::Sent,
            "cancelled" => Self::Cancelled,
            _ => Self::Failed,
        }
    }
}

/// The observable status of one delivery request (`g5.email.observability`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryStatus {
    pub id: Uuid,
    pub state: DeliveryState,
    /// An attempt may have reached the provider without a definite answer.
    pub outcome_uncertain: bool,
    pub terminal_reason: Option<String>,
    pub attempts: i32,
    pub updated_at: DateTime<Utc>,
}

/// The consumer's decision whether a queued message is still wanted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Validity {
    Wanted,
    /// Cancel with a short consumer reason, recorded as `cancelled:<reason>`.
    Cancelled(&'static str),
}

/// A consumer's "still wanted?" rule (`g5.email.validity`), checked before
/// every send attempt.
#[async_trait::async_trait]
pub trait DeliveryGate: Send + Sync {
    async fn still_wanted(
        &self,
        pool: &PgPool,
        purpose_key: &str,
        recipient: &str,
    ) -> Result<Validity, sqlx::Error>;
}

/// One validity check per purpose kind. Every field must be filled, so a new
/// kind cannot be sent without its consumer's check.
#[derive(Clone)]
pub struct EmailGates {
    pub org_invitation: Arc<dyn DeliveryGate>,
    pub workspace_owner_message: Arc<dyn DeliveryGate>,
    pub approval_request: Arc<dyn DeliveryGate>,
    pub approval_decision: Arc<dyn DeliveryGate>,
}

impl EmailGates {
    fn for_kind(&self, kind: PurposeKind) -> &dyn DeliveryGate {
        match kind {
            PurposeKind::OrgInvitation => self.org_invitation.as_ref(),
            PurposeKind::WorkspaceOwnerMessage => self.workspace_owner_message.as_ref(),
            PurposeKind::ApprovalRequest => self.approval_request.as_ref(),
            PurposeKind::ApprovalDecision => self.approval_decision.as_ref(),
        }
    }
}

/// The application-facing email handle, cheap to clone. The default is
/// [`Email::disabled`].
#[derive(Clone, Default)]
pub struct Email {
    inner: Option<Arc<Enabled>>,
}

struct Enabled {
    from_name: String,
    from_domain: String,
    public_base_url: String,
    transport: Arc<dyn transport::Transport>,
    capture: Option<CaptureTransport>,
    /// Off only in local stacks (`G5_EMAIL_VOLUME_LIMITS`).
    enforce_volume_limits: bool,
}

impl std::fmt::Debug for Email {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Email")
            .field("enabled", &self.inner.is_some())
            .finish()
    }
}

impl Email {
    /// Email that records nothing and sends nothing.
    #[must_use]
    pub fn disabled() -> Self {
        Self { inner: None }
    }

    /// Build the handle from validated configuration. `public_base_url` is
    /// the browser origin that message links resolve against.
    pub fn from_config(config: &EmailConfig, public_base_url: &str) -> Result<Self, String> {
        config.validate()?;
        let (transport, capture): (Arc<dyn transport::Transport>, _) = match config.transport {
            EmailTransportKind::Disabled => return Ok(Self::disabled()),
            EmailTransportKind::Capture => {
                let capture = CaptureTransport::default();
                (Arc::new(capture.clone()), Some(capture))
            }
            EmailTransportKind::Resend => (
                Arc::new(transport::ResendTransport::new(
                    &config.resend_api_url,
                    config.resend_api_key.as_deref().unwrap_or_default(),
                )?),
                None,
            ),
        };
        Ok(Self {
            inner: Some(Arc::new(Enabled {
                from_name: config.from_name.clone(),
                from_domain: config.from_domain.clone().unwrap_or_default(),
                public_base_url: public_base_url.trim_end_matches('/').to_string(),
                transport,
                capture,
                enforce_volume_limits: config.volume_limits == EmailVolumeLimits::Enforce,
            })),
        })
    }

    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.inner.is_some()
    }

    /// Messages kept by the `capture` transport, if that is the transport.
    #[must_use]
    pub fn capture(&self) -> Option<CaptureTransport> {
        self.inner.as_ref().and_then(|inner| inner.capture.clone())
    }

    /// Record a delivery request on the caller's connection, inside its
    /// transaction (`g5.email.handoff`). At most one live request exists per
    /// purpose: asking again while one is live makes it due now, and a
    /// single-use key returns its existing request (`g5.email.identity`). A
    /// kind's volume limits are checked under a transaction-scoped lock
    /// (`g5.email.volume-limits`).
    pub async fn request(
        &self,
        conn: &mut PgConnection,
        request: EmailRequest,
    ) -> Result<RequestOutcome, EmailError> {
        let Some(inner) = self.inner.as_ref() else {
            return Ok(RequestOutcome::Disabled);
        };
        let recipient = request.recipient.trim();
        if !render::is_plausible_recipient(recipient) {
            return Err(EmailError::InvalidRequest(
                "recipient is not a valid address",
            ));
        }
        let kind = request.message.purpose_kind();
        let key = request.message.purpose_key();
        let rendered = render::render(&request.message, &inner.public_base_url)?;
        let sender = request
            .sender
            .mailbox_header(&inner.from_name, &inner.from_domain);

        let org_limit = kind
            .daily_limit_per_org()
            .filter(|_| inner.enforce_volume_limits);
        let recipient_limit = kind
            .daily_limit_per_recipient()
            .filter(|_| inner.enforce_volume_limits);
        let limited = org_limit.is_some() || recipient_limit.is_some();
        if limited {
            // Serializes this organization's requests of this kind, including
            // the single-use lookup below, until the caller's transaction ends.
            store::lock_volume(conn, request.org_id, kind).await?;
        }
        if kind.single_use_keys() {
            if let Some(id) = store::find_by_purpose(conn, kind, &key).await? {
                return Ok(RequestOutcome::Existing(id));
            }
        }
        if limited {
            if let Some(id) = store::coalesce_live(conn, kind, &key).await? {
                return Ok(RequestOutcome::Coalesced(id));
            }
            let scopes = [
                org_limit.map(|limit| (limit, None)),
                recipient_limit.map(|limit| (limit, Some(recipient))),
            ];
            for (limit, recipient) in scopes.into_iter().flatten() {
                if let Some(retry_after) =
                    store::volume_exceeded(conn, request.org_id, kind, recipient, limit).await?
                {
                    return Err(EmailError::Limited { retry_after });
                }
            }
        }

        let (id, inserted) = store::insert_or_coalesce(
            conn,
            store::NewDelivery {
                id: Uuid::new_v4(),
                org_id: request.org_id,
                kind,
                key: &key,
                sender: &sender,
                recipient,
                rendered: &rendered,
            },
        )
        .await?;
        Ok(if inserted {
            tracing::info!(
                target: LOG_TARGET,
                delivery_id = %id,
                purpose_kind = kind.as_str(),
                purpose_key = %key,
                org_id = %request.org_id,
                "Email delivery requested"
            );
            RequestOutcome::Queued(id)
        } else {
            RequestOutcome::Coalesced(id)
        })
    }

    /// The latest request's status for each of `keys` of one kind in one
    /// organization. Keys without a request are absent, and every key is
    /// absent while email is disabled.
    pub async fn latest_status(
        &self,
        pool: &PgPool,
        org_id: Uuid,
        kind: PurposeKind,
        keys: &[String],
    ) -> Result<HashMap<String, DeliveryStatus>, EmailError> {
        if keys.is_empty() || !self.is_enabled() {
            return Ok(HashMap::new());
        }
        Ok(store::latest_status(pool, org_id, kind, keys).await?)
    }

    /// The delivery worker for this handle, or `None` when email is disabled.
    #[must_use]
    pub fn worker(&self, pool: PgPool, gates: EmailGates) -> Option<EmailWorker> {
        self.inner
            .as_ref()
            .map(|inner| EmailWorker::new(pool, inner.transport.clone(), gates))
    }
}

const LOG_TARGET: &str = "g5_api::email";

#[cfg(test)]
mod tests {
    use super::Sender;

    #[test]
    fn the_from_header_uses_the_consumers_name_or_the_configured_one() {
        const INVITATIONS: Sender = Sender::new("invitations");
        const APPROVALS: Sender = Sender::new("approvals").with_display_name("G5 Approvals");
        assert_eq!(
            INVITATIONS.mailbox_header("G5", "mail.g5labs.ai"),
            "G5 <invitations@mail.g5labs.ai>"
        );
        assert_eq!(
            APPROVALS.mailbox_header("Acme Platform", "mail.acme.test"),
            "G5 Approvals <approvals@mail.acme.test>"
        );
    }
}
