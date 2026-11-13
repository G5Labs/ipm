//! Message templates. Every consumer-supplied value is plain text: it is
//! stripped of control characters, bounded, and HTML-escaped in the HTML part.
//! An authored body may instead be the Markdown subset, rendered by
//! [`super::markdown`]. Links are app-relative paths resolved against the
//! public URL.

use super::markdown::{self, LinkPolicy};
use super::{
    ApprovalDecisionEmail, ApprovalDecisionOutcome, ApprovalRequestEmail, BodyFormat, EmailError,
    Message, OrgInvitationEmail, WorkspaceOwnerEmail,
};

/// One rendered message. Fixed when the request is recorded, so every attempt
/// sends identical content under the same idempotency key.
pub(super) struct Rendered {
    pub(super) subject: String,
    pub(super) text: String,
    pub(super) html: String,
}

const MAX_FIELD_CHARS: usize = 120;

pub(super) fn render(message: &Message, public_base_url: &str) -> Result<Rendered, EmailError> {
    match message {
        Message::OrgInvitation(invitation) => org_invitation(invitation, public_base_url),
        Message::WorkspaceOwnerMessage(message) => {
            workspace_owner_message(message, public_base_url)
        }
        Message::ApprovalRequest(message) => approval_request(message, public_base_url),
        Message::ApprovalDecision(message) => approval_decision(message, public_base_url),
    }
}

/// The Settings row, under Notifications, whose toggle governs both approval
/// emails (`g5.user.notifications`); each footer names it.
const APPROVAL_SETTING: &str = "Approval requests";

/// The reviewer-notification body (`g5.governance.approval.request-email`). The
/// subject carries the fixed `[G5 Approval]` prefix so recipients can filter on
/// it. The names are G5-controlled, so they are plain-text bounded and escaped.
/// The requester's note and the start of the PR description are text G5 did
/// not write (`g5.email.authored-content`): each follows G5's own opening line
/// under a label naming its source, and the text part keeps it as written. In
/// the HTML part the note is escaped plain text; the description is the
/// [`markdown`] subset under the link policy, cut only between whole blocks,
/// and falls back to escaped plain text when no block fits or it is refused.
fn approval_request(
    message: &ApprovalRequestEmail,
    public_base_url: &str,
) -> Result<Rendered, EmailError> {
    let workspace = plain(&message.workspace_name);
    let org = plain(&message.org_name);
    let definition = plain(&message.definition_name);
    let requester = plain(&message.requester_name);
    let link = absolute_link(public_base_url, &message.link_path)?;
    let settings_link = absolute_link(public_base_url, &message.settings_link_path)?;

    // Each quoted block: its label, its text-part source, its HTML and
    // whether it was shortened.
    let mut quoted = Vec::new();
    if let Some(note) = authored_block(message.note.as_deref()) {
        let note = bounded(&note, ApprovalRequestEmail::MAX_NOTE_CHARS);
        let html = quoted_plain_html(&note);
        quoted.push((format!("Note from {requester}:"), note, html, false));
    }
    if let Some(description) = authored_block(message.pull_request_description.as_deref()) {
        let (description, html, shortened) =
            description_block(&description, message, public_base_url);
        quoted.push((
            "Pull request description:".to_string(),
            description,
            html,
            shortened,
        ));
    }
    const SHORTENED: &str = "Shortened. Open the request to read the full description.";

    let subject = format!("[G5 Approval] {workspace}: {definition} requested by {requester}");
    let mut text = format!(
        "{requester} requested your approval for \"{definition}\" on the G5 workspace \
         {workspace} ({org}).\n"
    );
    for (label, body, _, shortened) in &quoted {
        text.push_str(&format!("\n{label}\n\n{body}\n"));
        if *shortened {
            text.push_str(&format!("\n({SHORTENED})\n"));
        }
    }
    text.push_str(&format!(
        "\n\
         Open the request to approve or request changes:\n\
         {link}\n\
         \n\
         --\n\
         You are receiving this because you are a configured reviewer. To stop these \
         emails, turn off \"{APPROVAL_SETTING}\" under Notifications in your G5 \
         Settings:\n\
         {settings_link}\n"
    ));

    let mut sections = String::new();
    for (label, _, html, shortened) in &quoted {
        sections.push_str(&format!(
            "<p style=\"margin:16px 0 4px;color:#59636e;font-size:13px\">{label}</p>{html}",
            label = escape_html(label),
        ));
        if *shortened {
            sections.push_str(&format!(
                "<p style=\"margin:4px 0 0;color:#59636e;font-size:13px\">{SHORTENED}</p>"
            ));
        }
    }
    let html = format!(
        "<!doctype html>\
         <html><body style=\"font-family:-apple-system,Segoe UI,Helvetica,Arial,sans-serif;\
         color:#1f2328;line-height:1.5\">\
         <p><strong>{requester}</strong> requested your approval for \
         <strong>{definition}</strong> on the G5 workspace <strong>{workspace}</strong> \
         ({org}).</p>\
         {sections}\
         <p style=\"margin-top:20px\"><a href=\"{href}\" style=\"display:inline-block;\
         padding:8px 16px;background:#1f6feb;color:#ffffff;text-decoration:none;\
         border-radius:6px\">Open the approval request</a></p>\
         <p style=\"color:#59636e;font-size:13px\">Or open this link: {href}</p>\
         <p style=\"color:#59636e;font-size:13px\">You are receiving this because you are a \
         configured reviewer. To stop these emails, turn off \"{APPROVAL_SETTING}\" \
         under Notifications in your \
         <a href=\"{settings_href}\" style=\"color:#59636e\">G5 Settings</a>.</p>\
         </body></html>",
        requester = escape_html(&requester),
        definition = escape_html(&definition),
        workspace = escape_html(&workspace),
        org = escape_html(&org),
        href = escape_html(&link),
        settings_href = escape_html(&settings_link),
    );
    Ok(Rendered {
        subject,
        text,
        html,
    })
}

/// The owner-notification body (`g5.governance.approval.decision-email`). It
/// shares the request email's fixed `[G5 Approval]` subject prefix; the names
/// are G5-controlled plain text, and the reviewer's comment is text G5 did not
/// write (`g5.email.authored-content`), quoted as escaped plain text under a
/// label naming the reviewer.
fn approval_decision(
    message: &ApprovalDecisionEmail,
    public_base_url: &str,
) -> Result<Rendered, EmailError> {
    let workspace = plain(&message.workspace_name);
    let org = plain(&message.org_name);
    let definition = plain(&message.definition_name);
    let reviewer = plain(&message.reviewer_name);
    let link = absolute_link(public_base_url, &message.link_path)?;
    let settings_link = absolute_link(public_base_url, &message.settings_link_path)?;
    let (subject_outcome, verb) = match message.outcome {
        ApprovalDecisionOutcome::Approved => ("approved", "approved"),
        ApprovalDecisionOutcome::ChangesRequested => ("changes requested", "requested changes on"),
    };
    let comment = authored_block(message.comment.as_deref())
        .map(|comment| bounded(&comment, ApprovalDecisionEmail::MAX_COMMENT_CHARS));
    let label = format!("Comment from {reviewer}:");

    let subject =
        format!("[G5 Approval] {workspace}: {definition} {subject_outcome} by {reviewer}");
    let mut text =
        format!("{reviewer} {verb} \"{definition}\" on your G5 workspace {workspace} ({org}).\n");
    if let Some(comment) = &comment {
        text.push_str(&format!("\n{label}\n\n{comment}\n"));
    }
    text.push_str(&format!(
        "\n\
         Open the workspace's approvals:\n\
         {link}\n\
         \n\
         --\n\
         You are receiving this because you own this workspace. To stop these emails, \
         turn off \"{APPROVAL_SETTING}\" under Notifications in your G5 Settings:\n\
         {settings_link}\n"
    ));

    let section = comment.as_deref().map_or_else(String::new, |comment| {
        format!(
            "<p style=\"margin:16px 0 4px;color:#59636e;font-size:13px\">{label}</p>{html}",
            label = escape_html(&label),
            html = quoted_plain_html(comment),
        )
    });
    let html = format!(
        "<!doctype html>\
         <html><body style=\"font-family:-apple-system,Segoe UI,Helvetica,Arial,sans-serif;\
         color:#1f2328;line-height:1.5\">\
         <p><strong>{reviewer}</strong> {verb} <strong>{definition}</strong> on your G5 \
         workspace <strong>{workspace}</strong> ({org}).</p>\
         {section}\
         <p style=\"margin-top:20px\"><a href=\"{href}\" style=\"display:inline-block;\
         padding:8px 16px;background:#1f6feb;color:#ffffff;text-decoration:none;\
         border-radius:6px\">Open the approvals</a></p>\
         <p style=\"color:#59636e;font-size:13px\">Or open this link: {href}</p>\
         <p style=\"color:#59636e;font-size:13px\">You are receiving this because you own \
         this workspace. To stop these emails, turn off \"{APPROVAL_SETTING}\" under \
         Notifications in your \
         <a href=\"{settings_href}\" style=\"color:#59636e\">G5 Settings</a>.</p>\
         </body></html>",
        reviewer = escape_html(&reviewer),
        definition = escape_html(&definition),
        workspace = escape_html(&workspace),
        org = escape_html(&org),
        href = escape_html(&link),
        settings_href = escape_html(&settings_link),
    );
    Ok(Rendered {
        subject,
        text,
        html,
    })
}

/// Escaped authored plain text in a quoted block.
fn quoted_plain_html(body: &str) -> String {
    format!(
        "<div style=\"white-space:pre-wrap;margin:0;padding:8px 12px;\
         border-left:3px solid #d1d9e0;background:#f6f8fa\">{}</div>",
        escape_html(body)
    )
}

/// The start of the PR description: its text-part source, its HTML and
/// whether it was shortened. Whole top-level blocks within the budget are
/// rendered as the Markdown subset; when not even the first block fits, or the
/// subset refuses the kept text, the first lines are shown as escaped plain
/// text instead, so an approval email is never refused for its description.
fn description_block(
    description: &str,
    message: &ApprovalRequestEmail,
    public_base_url: &str,
) -> (String, String, bool) {
    let max_lines = ApprovalRequestEmail::MAX_DESCRIPTION_LINES;
    let max_chars = ApprovalRequestEmail::MAX_DESCRIPTION_CHARS;
    let policy = LinkPolicy::new(public_base_url, &message.link_scopes);
    let rendered = markdown::leading_blocks(description, max_lines, max_chars).and_then(
        |(kept, shortened)| {
            let html = markdown::render(&kept, &policy).ok()?;
            Some((kept, html, shortened))
        },
    );
    if let Some((kept, html, shortened)) = rendered {
        let html = format!(
            "<div style=\"margin:0;padding:0 12px;border-left:3px solid #d1d9e0\">{html}</div>"
        );
        return (kept, html, shortened);
    }
    let (kept, shortened) = leading_lines(description, max_lines, max_chars);
    let html = quoted_plain_html(&kept);
    (kept, html, shortened)
}

/// Authored multi-line text, normalized like an owner message's plain body;
/// `None` when absent or blank.
fn authored_block(value: Option<&str>) -> Option<String> {
    value
        .map(|value| text_block(value, "    "))
        .filter(|block| !block.trim().is_empty())
}

/// At most `max_lines` lines and `max_chars` characters from the start of
/// `value`, cut at a line end where one falls in the second half of the
/// budget. Returns the kept text and whether anything was dropped.
fn leading_lines(value: &str, max_lines: usize, max_chars: usize) -> (String, bool) {
    let mut kept = value.lines().take(max_lines).collect::<Vec<_>>().join("\n");
    let mut shortened = kept.len() < value.len();
    if kept.chars().count() > max_chars {
        let mut cut: String = kept.chars().take(max_chars - 1).collect();
        if let Some(line_end) = cut.rfind('\n').filter(|end| *end >= cut.len() / 2) {
            cut.truncate(line_end);
        } else {
            cut.push('…');
        }
        kept = cut;
        shortened = true;
    }
    (kept.trim_end().to_string(), shortened)
}

/// The agent's own words, framed so they are not read as G5's voice: the
/// authorship notice comes first in both parts (`g5.email.authored-content`).
fn workspace_owner_message(
    message: &WorkspaceOwnerEmail,
    public_base_url: &str,
) -> Result<Rendered, EmailError> {
    let workspace = plain(&message.workspace_name);
    let org = plain(&message.org_name);
    let subject = bounded(
        &plain(&message.subject),
        WorkspaceOwnerEmail::MAX_SUBJECT_CHARS,
    );
    let tab = match message.format {
        BodyFormat::Text => " ",
        // Keeps a tab-indented list item or code block indented.
        BodyFormat::Markdown => "    ",
    };
    // The consumer bounds the source. Plain text is also bounded here, as
    // before; Markdown is never cut, since a cut could break its structure,
    // and the renderer bounds its output instead.
    let body = text_block(&message.body, tab);
    let body = match message.format {
        BodyFormat::Text => bounded(&body, WorkspaceOwnerEmail::MAX_BODY_CHARS),
        BodyFormat::Markdown => body,
    };
    let content = match message.format {
        BodyFormat::Text => format!(
            "<div style=\"white-space:pre-wrap\">{}</div>",
            escape_html(&body)
        ),
        BodyFormat::Markdown => markdown::render(
            &body,
            &LinkPolicy::new(public_base_url, &message.link_scopes),
        )
        .map_err(|refused| EmailError::InvalidContent {
            field: "body",
            reason: refused.0,
        })?,
    };
    let link = absolute_link(public_base_url, &message.link_path)?;
    let notice = format!(
        "Message from the agent in your G5 workspace {workspace} ({org}). G5 did not write it."
    );
    let footer = format!(
        "Sent by the agent in your G5 workspace {workspace} ({org}). G5 did not write this \
         message, and an agent never needs your password or keys."
    );

    let text = format!("{notice}\n\n{body}\n\n--\n{footer}\nOpen the workspace: {link}\n");
    let html = format!(
        "<!doctype html>\
         <html><body style=\"font-family:-apple-system,Segoe UI,Helvetica,Arial,sans-serif;\
         color:#1f2328;line-height:1.5\">\
         <p style=\"margin:0 0 20px;padding:8px 12px;background:#f6f8fa;\
         border:1px solid #d1d9e0;border-radius:6px;color:#59636e;font-size:13px\">{notice}</p>\
         <div>{content}</div>\
         <hr style=\"border:none;border-top:1px solid #d1d9e0;margin:24px 0 12px\">\
         <p style=\"color:#59636e;font-size:13px\">{footer}</p>\
         <p style=\"color:#59636e;font-size:13px\"><a href=\"{href}\">Open {workspace} \
         on G5</a></p>\
         </body></html>",
        notice = escape_html(&notice),
        footer = escape_html(&footer),
        workspace = escape_html(&workspace),
        href = escape_html(&link),
    );
    Ok(Rendered {
        subject: format!("[{workspace}] {subject}"),
        text,
        html,
    })
}

/// Multi-line text: line breaks kept (CRLF normalized), each tab becomes
/// `tab`, other control characters dropped and trailing blank space trimmed.
fn text_block(value: &str, tab: &str) -> String {
    let mut normalized = String::with_capacity(value.len());
    for c in value.replace("\r\n", "\n").chars() {
        match c {
            '\n' => normalized.push('\n'),
            '\t' => normalized.push_str(tab),
            c if c.is_control() => {}
            c => normalized.push(c),
        }
    }
    let trimmed = normalized
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n");
    trimmed.trim_matches('\n').to_string()
}

fn bounded(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        value.to_string()
    } else {
        let mut bounded: String = value.chars().take(max_chars - 1).collect();
        bounded.push('…');
        bounded
    }
}

/// Percent-encode everything but unreserved characters, so the value stays
/// one path segment (`encodeURIComponent` without its `!*'()` exceptions).
pub(super) fn encode_segment(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

fn org_invitation(
    message: &OrgInvitationEmail,
    public_base_url: &str,
) -> Result<Rendered, EmailError> {
    let org = plain(&message.org_name);
    let inviter = plain(&message.inviter_name);
    let role = plain(&message.role);
    let link = absolute_link(public_base_url, &message.link_path)?;
    let article = if role.starts_with(['a', 'e', 'i', 'o', 'u']) {
        "an"
    } else {
        "a"
    };

    let subject = format!("{inviter} invited you to join {org} on G5");
    let text = format!(
        "{inviter} invited you to join the {org} organization on G5 as {article} {role}.\n\
         \n\
         Sign in with this email address to accept the invitation:\n\
         {link}\n\
         \n\
         If you were not expecting this invitation, you can ignore this email.\n"
    );
    let html = format!(
        "<!doctype html>\
         <html><body style=\"font-family:-apple-system,Segoe UI,Helvetica,Arial,sans-serif;\
         color:#1f2328;line-height:1.5\">\
         <p><strong>{inviter}</strong> invited you to join the <strong>{org}</strong> \
         organization on G5 as {article} {role}.</p>\
         <p>Sign in with this email address to accept the invitation.</p>\
         <p><a href=\"{href}\" style=\"display:inline-block;padding:8px 16px;\
         background:#1f6feb;color:#ffffff;text-decoration:none;border-radius:6px\">\
         Open {org} on G5</a></p>\
         <p style=\"color:#59636e;font-size:13px\">Or open this link: {href}</p>\
         <p style=\"color:#59636e;font-size:13px\">If you were not expecting this \
         invitation, you can ignore this email.</p>\
         </body></html>",
        inviter = escape_html(&inviter),
        org = escape_html(&org),
        role = escape_html(&role),
        href = escape_html(&link),
    );
    Ok(Rendered {
        subject,
        text,
        html,
    })
}

/// Collapse whitespace, drop control characters and bound the length.
fn plain(value: &str) -> String {
    let collapsed = value
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    bounded(&collapsed, MAX_FIELD_CHARS)
}

pub(super) fn escape_html(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(c),
        }
    }
    escaped
}

/// Resolve an app-relative path (`/segment/...` of URL-safe characters and
/// `%XX` escapes) against the public base URL, keeping any base path prefix.
fn absolute_link(public_base_url: &str, path: &str) -> Result<String, EmailError> {
    let bytes = path.as_bytes();
    let escapes_valid = bytes.iter().enumerate().all(|(i, byte)| {
        *byte != b'%'
            || (i + 2 < bytes.len()
                && bytes[i + 1].is_ascii_hexdigit()
                && bytes[i + 2].is_ascii_hexdigit())
    });
    let safe = path.starts_with('/')
        && !path.starts_with("//")
        && escapes_valid
        && path
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '-' | '_' | '.' | '~' | '%'));
    if !safe || path.split('/').any(|segment| segment == "..") {
        return Err(EmailError::InvalidRequest(
            "link path must be an app-relative path",
        ));
    }
    let link = format!("{}{path}", public_base_url.trim_end_matches('/'));
    url::Url::parse(&link)
        .map(|url| url.to_string())
        .map_err(|_| EmailError::InvalidRequest("public URL cannot form a link"))
}

/// A recipient is one bare `local@domain` address without spaces or
/// control characters; the provider performs full validation.
pub(super) fn is_plausible_recipient(address: &str) -> bool {
    match address.split_once('@') {
        Some((local, domain)) => {
            !local.is_empty()
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
                && address.len() <= 254
                && address.chars().filter(|c| *c == '@').count() == 1
                && !address.chars().any(|c| {
                    c.is_whitespace() || c.is_control() || matches!(c, '<' | '>' | ',' | ';')
                })
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use g5_contracts::error::G5Error;
    use uuid::Uuid;

    fn invitation(org: &str, inviter: &str, path: &str) -> Message {
        Message::OrgInvitation(OrgInvitationEmail {
            invitation_id: Uuid::nil(),
            org_name: org.to_string(),
            inviter_name: inviter.to_string(),
            role: "admin".to_string(),
            link_path: path.to_string(),
        })
    }

    #[test]
    fn invitation_names_org_inviter_role_and_links_the_org_page() {
        let rendered = render(&invitation("Acme", "Ada", "/acme"), "https://g5.example/").unwrap();
        assert_eq!(rendered.subject, "Ada invited you to join Acme on G5");
        assert!(rendered.text.contains("as an admin"));
        assert!(rendered.text.contains("https://g5.example/acme\n"));
        assert!(rendered.html.contains("href=\"https://g5.example/acme\""));
    }

    #[test]
    fn consumer_values_are_escaped_and_bounded() {
        let long = "x".repeat(500);
        let rendered = render(
            &invitation("<script>&\"Acme\"\r\nBcc: x", &long, "/acme"),
            "https://g5.example",
        )
        .unwrap();
        assert!(!rendered.subject.contains(['\r', '\n']));
        assert!(rendered
            .html
            .contains("&lt;script&gt;&amp;&quot;Acme&quot; Bcc: x"));
        assert!(!rendered.html.contains("<script>"));
        assert!(rendered.subject.chars().count() < 200);
    }

    #[test]
    fn links_keep_a_base_path_and_refuse_foreign_targets() {
        let rendered = render(&invitation("A", "B", "/acme"), "https://host/g5/").unwrap();
        assert!(rendered.text.contains("https://host/g5/acme"));
        for path in [
            "acme",
            "//evil.example/x",
            "/a/../b",
            "/a?b",
            "https://evil.example",
        ] {
            assert!(
                render(&invitation("A", "B", path), "https://host").is_err(),
                "{path:?} must be refused"
            );
        }
    }

    fn owner_message(subject: &str, body: &str, path: &str) -> Message {
        owner_message_in(BodyFormat::Text, subject, body, path)
    }

    fn owner_message_in(format: BodyFormat, subject: &str, body: &str, path: &str) -> Message {
        Message::WorkspaceOwnerMessage(WorkspaceOwnerEmail {
            workspace_id: Uuid::nil(),
            request_key: "k".to_string(),
            workspace_name: "feature/login".to_string(),
            org_name: "Acme".to_string(),
            subject: subject.to_string(),
            body: body.to_string(),
            format,
            link_scopes: vec!["https://github.com/acme/api".to_string()],
            link_path: path.to_string(),
        })
    }

    #[test]
    fn owner_messages_keep_the_agents_lines_and_name_the_workspace_and_its_agent() {
        let path = super::super::link_path(&["acme", "feature/login"]);
        assert_eq!(path, "/acme/feature%2Flogin");
        let rendered = render(
            &owner_message(
                "Tests\r\npassed",
                "Done.\r\n\n\t- 3 files\u{7}\nBcc: x@y.z  \n",
                &path,
            ),
            "https://g5.example",
        )
        .unwrap();
        assert_eq!(rendered.subject, "[feature/login] Tests passed");
        assert!(rendered.text.starts_with(
            "Message from the agent in your G5 workspace feature/login (Acme). G5 did not \
             write it.\n\nDone.\n\n - 3 files\nBcc: x@y.z\n\n--\n"
        ));
        assert!(rendered
            .text
            .contains("Sent by the agent in your G5 workspace feature/login (Acme)."));
        assert!(rendered
            .text
            .contains("https://g5.example/acme/feature%2Flogin\n"));
        assert!(rendered.html.contains("white-space:pre-wrap"));
    }

    #[test]
    fn owner_message_html_is_escaped_text_and_bounded() {
        let body = format!(
            "<a href=\"https://evil.example\">G5 login</a>{}",
            "x".repeat(30_000)
        );
        let rendered = render(&owner_message("<b>hi</b>", &body, "/acme/ws"), "https://h").unwrap();
        assert!(!rendered.html.contains("<a href=\"https://evil.example\">"));
        assert!(rendered
            .html
            .contains("&lt;a href=&quot;https://evil.example&quot;&gt;"));
        assert!(
            rendered.subject.contains("<b>hi</b>"),
            "subjects are plain text"
        );
        assert!(rendered.text.chars().count() < WorkspaceOwnerEmail::MAX_BODY_CHARS + 400);
    }

    #[test]
    fn markdown_bodies_render_the_subset_under_the_notice() {
        let source =
            "## Summary\r\n\n- **3** PRs merged: [PR 7](https://github.com/acme/api/pull/7)\n\
                      - [login](https://evil.example/login)\n\n<b>raw</b>\t\n";
        let rendered = render(
            &owner_message_in(BodyFormat::Markdown, "Digest", source, "/acme/ws"),
            "https://g5.example",
        )
        .unwrap();
        let html = &rendered.html;
        let notice = html
            .find("Message from the agent in your G5 workspace")
            .unwrap();
        let content = html.find("<h2 ").unwrap();
        assert!(notice < content, "the notice comes before the agent's text");
        assert!(html.contains("<strong>3</strong>"));
        assert!(html.contains("<a href=\"https://github.com/acme/api/pull/7\""));
        assert!(html.contains("login (https://evil.example/login)"));
        assert!(!html.contains("href=\"https://evil.example"));
        assert!(html.contains("&lt;b&gt;raw&lt;/b&gt;"));
        assert!(!html.contains("white-space:pre-wrap\">## Summary"));
        // The text part is the normalized source, not a rendering.
        assert!(rendered.text.contains(
            "\n\n## Summary\n\n- **3** PRs merged: [PR 7](https://github.com/acme/api/pull/7)\n\
             - [login](https://evil.example/login)\n\n<b>raw</b>\n\n--\n"
        ));
        assert!(rendered.text.starts_with("Message from the agent"));
        assert_eq!(rendered.subject, "[feature/login] Digest");
    }

    #[test]
    fn a_markdown_body_outside_the_bounds_is_refused_and_tabs_keep_nesting() {
        let error = render(
            &owner_message_in(
                BodyFormat::Markdown,
                "s",
                "- a\n  - b\n    - c\n",
                "/acme/ws",
            ),
            "https://g5.example",
        )
        .err()
        .unwrap();
        match G5Error::from(error) {
            G5Error::ValidationError { details } => {
                assert_eq!(details["field"], "body");
                let reason = details["error"].as_str().unwrap();
                assert!(
                    reason.contains("lists nest at most 2 levels (line 3)"),
                    "{reason}"
                );
            }
            other => panic!("{other:?}"),
        }
        let rendered = render(
            &owner_message_in(BodyFormat::Markdown, "s", "- a\n\t- b\n", "/acme/ws"),
            "https://g5.example",
        )
        .unwrap();
        assert_eq!(rendered.html.matches("<ul ").count(), 2);
    }

    #[test]
    fn plain_text_bodies_are_never_rendered_as_markdown() {
        let rendered = render(
            &owner_message("s", "# Title\n[x](https://github.com/acme/api)", "/acme/ws"),
            "https://g5.example",
        )
        .unwrap();
        assert!(rendered
            .html
            .contains("white-space:pre-wrap\"># Title\n[x](https://github.com/acme/api)</div>"));
        assert!(!rendered.html.contains("<h1"));
        assert!(!rendered.html.contains("href=\"https://github.com"));
    }

    #[test]
    fn link_paths_accept_only_well_formed_escapes() {
        assert!(absolute_link("https://h", "/a/b%2Fc").is_ok());
        for path in ["/a/%2", "/a/%zz", "/a%"] {
            assert!(absolute_link("https://h", path).is_err(), "{path:?}");
        }
    }

    #[test]
    fn approval_request_has_the_fixed_prefix_and_links_the_request() {
        let rendered = render(
            &Message::ApprovalRequest(ApprovalRequestEmail {
                request_id: Uuid::nil(),
                org_id: Uuid::nil(),
                reviewer_user_id: Uuid::nil(),
                workspace_name: "feature/login".to_string(),
                org_name: "Acme".to_string(),
                definition_name: "Security review".to_string(),
                requester_name: "Ada".to_string(),
                note: None,
                pull_request_description: None,
                link_scopes: Vec::new(),
                link_path: super::super::link_path(&["acme", "feature/login"]),
                settings_link_path: super::super::link_path(&["acme", "settings"]),
            }),
            "https://g5.example",
        )
        .unwrap();
        assert_eq!(
            rendered.subject,
            "[G5 Approval] feature/login: Security review requested by Ada"
        );
        assert!(rendered.text.contains("Ada requested your approval"));
        assert!(rendered
            .text
            .contains("https://g5.example/acme/feature%2Flogin"));
        assert!(rendered
            .html
            .contains("href=\"https://g5.example/acme/feature%2Flogin\""));
        // The footer points at the Notifications section of Settings.
        assert!(rendered.text.contains(
            "under Notifications in your G5 Settings:\nhttps://g5.example/acme/settings\n"
        ));
        assert!(rendered
            .html
            .contains("href=\"https://g5.example/acme/settings\""));
        assert!(!rendered.text.contains("account settings"));
        // Without a note or description, neither label appears.
        assert!(!rendered.text.contains("Note from"));
        assert!(!rendered.text.contains("Pull request description"));
        // G5-controlled values are still escaped in the HTML part.
        let escaped = render(
            &Message::ApprovalRequest(ApprovalRequestEmail {
                request_id: Uuid::nil(),
                org_id: Uuid::nil(),
                reviewer_user_id: Uuid::nil(),
                workspace_name: "<b>ws</b>".to_string(),
                org_name: "Acme".to_string(),
                definition_name: "R&D".to_string(),
                requester_name: "Ada".to_string(),
                note: None,
                pull_request_description: None,
                link_scopes: Vec::new(),
                link_path: "/acme/ws".to_string(),
                settings_link_path: "/acme/settings".to_string(),
            }),
            "https://g5.example",
        )
        .unwrap();
        assert!(escaped.html.contains("&lt;b&gt;ws&lt;/b&gt;"));
        assert!(!escaped.html.contains("<b>ws</b>"));
    }

    fn approval_with(note: Option<&str>, description: Option<&str>) -> Message {
        Message::ApprovalRequest(ApprovalRequestEmail {
            request_id: Uuid::nil(),
            org_id: Uuid::nil(),
            reviewer_user_id: Uuid::nil(),
            workspace_name: "ws".to_string(),
            org_name: "Acme".to_string(),
            definition_name: "Security review".to_string(),
            requester_name: "Ada".to_string(),
            note: note.map(str::to_string),
            pull_request_description: description.map(str::to_string),
            link_scopes: vec!["https://github.com/acme/api".to_string()],
            link_path: "/acme/ws".to_string(),
            settings_link_path: "/acme/settings".to_string(),
        })
    }

    #[test]
    fn approval_request_quotes_the_note_and_description_under_labels() {
        let rendered = render(
            &approval_with(
                Some("Check the\r\n<b>migration</b> & tests.\n"),
                Some("## Summary\n\nAdds [a link](https://evil.example)."),
            ),
            "https://g5.example",
        )
        .unwrap();
        // G5's own line comes first, then each authored block under its label.
        let opening = rendered.text.find("Ada requested your approval").unwrap();
        let note = rendered
            .text
            .find("Note from Ada:\n\nCheck the\n<b>migration</b> & tests.\n");
        let description = rendered.text.find(
            "Pull request description:\n\n## Summary\n\nAdds [a link](https://evil.example).\n",
        );
        assert!(note.is_some() && description.is_some(), "{}", rendered.text);
        assert!(opening < note.unwrap() && note.unwrap() < description.unwrap());
        // HTML: the note is escaped plain text; the description is the
        // Markdown subset, whose links follow the link policy.
        assert!(rendered
            .html
            .contains("Check the\n&lt;b&gt;migration&lt;/b&gt; &amp; tests."));
        assert!(!rendered.html.contains("<b>migration</b>"));
        assert!(rendered.html.contains("<h2 "), "{}", rendered.html);
        assert!(!rendered.html.contains("## Summary"));
        assert!(!rendered.html.contains("href=\"https://evil.example"));
        assert!(rendered.html.contains("a link (https://evil.example)"));
        assert!(!rendered.text.contains("Shortened."));
        // The note is never rendered, even when it looks like Markdown.
        let note = render(
            &approval_with(Some("**bold** [x](https://github.com/acme/api)"), None),
            "https://g5.example",
        )
        .unwrap();
        assert!(note
            .html
            .contains("**bold** [x](https://github.com/acme/api)"));
        assert!(!note.html.contains("<strong>bold"));
        assert!(!note.html.contains("href=\"https://github.com"));
        // Blank authored text is left out.
        let blank = render(
            &approval_with(Some(" \n\t"), Some("")),
            "https://g5.example",
        )
        .unwrap();
        assert!(!blank.text.contains("Note from"));
        assert!(!blank.text.contains("Pull request description"));
    }

    #[test]
    fn approval_request_renders_the_description_and_links_the_organizations_repositories() {
        let rendered = render(
            &approval_with(
                None,
                Some(
                    "**Fixes** [#12](https://github.com/acme/api/issues/12) and \
                     [other](https://github.com/rival/app/pull/1).\n\n<img src=x onerror=alert(1)>",
                ),
            ),
            "https://g5.example",
        )
        .unwrap();
        assert!(
            rendered.html.contains("<strong>Fixes</strong>"),
            "{}",
            rendered.html
        );
        assert!(rendered
            .html
            .contains("href=\"https://github.com/acme/api/issues/12\""));
        assert!(!rendered.html.contains("href=\"https://github.com/rival"));
        assert!(!rendered.html.contains("<img"));
        assert!(rendered.html.contains("&lt;img"));
        // The text part keeps the source as written.
        assert!(rendered
            .text
            .contains("**Fixes** [#12](https://github.com/acme/api/issues/12)"));
    }

    #[test]
    fn approval_request_cuts_the_description_between_whole_blocks() {
        let fence = (1..=40)
            .map(|n| format!("step {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        let description = format!("## Summary\n\nIntro.\n\n```sh\n{fence}\n```\n\nAfter.");
        let rendered = render(
            &approval_with(None, Some(&description)),
            "https://g5.example",
        )
        .unwrap();
        // The fence would cross the 30-line budget, so it is dropped whole
        // rather than left open; both parts keep the same blocks.
        assert!(rendered.html.contains("Intro."));
        assert!(!rendered.html.contains("step 1"));
        assert!(!rendered.html.contains("```"));
        assert!(rendered
            .text
            .contains("Pull request description:\n\n## Summary\n\nIntro.\n\n(Shortened."));
        assert!(rendered.html.contains("Shortened. Open the request"));
    }

    #[test]
    fn approval_request_falls_back_to_plain_text_when_the_subset_refuses() {
        // A reference-style definition among the kept blocks is refused by
        // the subset, so the description is shown as escaped plain text rather
        // than not sent.
        let rendered = render(
            &approval_with(
                None,
                Some("See [the plan][p] **now**.\n\n[p]: https://evil.example\n\nMore."),
            ),
            "https://g5.example",
        )
        .unwrap();
        assert!(rendered.html.contains("See [the plan][p] **now**."));
        assert!(!rendered.html.contains("<strong>now"));
        assert!(!rendered.html.contains("href=\"https://evil.example"));
        assert!(rendered.html.contains("More."));
    }

    #[test]
    fn approval_request_shortens_a_long_description() {
        // One 50-line paragraph: no whole block fits, so the first lines are
        // shown as plain text.
        let many_lines = (1..=50)
            .map(|n| format!("line {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        let rendered = render(
            &approval_with(None, Some(&many_lines)),
            "https://g5.example",
        )
        .unwrap();
        assert!(rendered.text.contains("line 30\n"));
        assert!(!rendered.text.contains("line 31"));
        assert!(rendered
            .text
            .contains("(Shortened. Open the request to read the full description.)"));
        assert!(rendered.html.contains("Shortened. Open the request"));

        // One long paragraph is cut by characters with an ellipsis.
        let long = "word ".repeat(1_000);
        let (kept, shortened) = leading_lines(
            long.trim_end(),
            ApprovalRequestEmail::MAX_DESCRIPTION_LINES,
            ApprovalRequestEmail::MAX_DESCRIPTION_CHARS,
        );
        assert!(shortened);
        assert!(kept.chars().count() <= ApprovalRequestEmail::MAX_DESCRIPTION_CHARS);
        assert!(kept.ends_with('…'));

        // Long lines are cut at a line end when one is in the second half.
        let paragraphs = format!("{}\n{}", "a".repeat(1_000), "b".repeat(1_000));
        let (kept, shortened) = leading_lines(&paragraphs, 30, 1_500);
        assert!(shortened);
        assert_eq!(kept, "a".repeat(1_000));

        // A description within both bounds is kept whole.
        let (kept, shortened) = leading_lines("short\ntext", 30, 1_500);
        assert_eq!((kept.as_str(), shortened), ("short\ntext", false));
    }

    fn decision(outcome: ApprovalDecisionOutcome, comment: Option<&str>) -> Message {
        Message::ApprovalDecision(ApprovalDecisionEmail {
            request_id: Uuid::nil(),
            org_id: Uuid::nil(),
            owner_user_id: Uuid::nil(),
            workspace_name: "feature/login".to_string(),
            org_name: "Acme".to_string(),
            definition_name: "Security review".to_string(),
            reviewer_name: "Grace".to_string(),
            outcome,
            comment: comment.map(str::to_string),
            link_path: super::super::link_path(&["acme", "feature/login"]),
            settings_link_path: super::super::link_path(&["acme", "settings"]),
        })
    }

    #[test]
    fn approval_decision_names_the_outcome_and_links_the_approvals() {
        let approved = render(
            &decision(ApprovalDecisionOutcome::Approved, None),
            "https://g5.example",
        )
        .unwrap();
        assert_eq!(
            approved.subject,
            "[G5 Approval] feature/login: Security review approved by Grace"
        );
        assert!(approved.text.starts_with(
            "Grace approved \"Security review\" on your G5 workspace feature/login (Acme).\n"
        ));
        assert!(approved
            .text
            .contains("approvals:\nhttps://g5.example/acme/feature%2Flogin\n"));
        assert!(approved
            .html
            .contains("href=\"https://g5.example/acme/feature%2Flogin\""));
        // No comment, no label.
        assert!(!approved.text.contains("Comment from"));
        // The footer names the one toggle that governs both approval emails.
        assert!(approved.text.contains(
            "turn off \"Approval requests\" under Notifications in your G5 Settings:\n\
             https://g5.example/acme/settings\n"
        ));
        assert!(approved
            .html
            .contains("href=\"https://g5.example/acme/settings\""));

        let changes = render(
            &decision(
                ApprovalDecisionOutcome::ChangesRequested,
                Some("Please <b>split</b> the migration.\nThanks"),
            ),
            "https://g5.example",
        )
        .unwrap();
        assert_eq!(
            changes.subject,
            "[G5 Approval] feature/login: Security review changes requested by Grace"
        );
        assert!(changes
            .text
            .starts_with("Grace requested changes on \"Security review\""));
        // The comment is the reviewer's text, quoted as written under a label
        // after G5's own opening line, and escaped in the HTML part.
        assert!(changes
            .text
            .contains("\nComment from Grace:\n\nPlease <b>split</b> the migration.\nThanks\n"));
        assert!(changes.text.find("Grace requested") < changes.text.find("Comment from"));
        assert!(changes
            .html
            .contains("Please &lt;b&gt;split&lt;/b&gt; the migration.\nThanks"));
        assert!(!changes.html.contains("<b>split</b>"));
    }

    #[test]
    fn approval_request_footer_names_the_shared_setting() {
        let rendered = render(&approval_with(None, None), "https://g5.example").unwrap();
        assert!(rendered
            .text
            .contains("turn off \"Approval requests\" under Notifications"));
        assert!(!rendered.text.contains("requests my approval\""));
    }

    #[test]
    fn recipients_must_be_one_bare_address() {
        assert!(is_plausible_recipient("new@example.com"));
        for bad in [
            "new",
            "a@b",
            "a b@example.com",
            "a@example.com, b@example.com",
            "A <a@example.com>",
            "a@@example.com",
        ] {
            assert!(!is_plausible_recipient(bad), "{bad:?}");
        }
    }
}
