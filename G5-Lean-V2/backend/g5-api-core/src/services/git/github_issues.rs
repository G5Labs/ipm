//! Domain-facing compatibility adapters. The selected credential is bound to
//! one repository capability before the SCM provider executes any protocol.
use crate::services::git::github_app::GithubAppClient;
pub use g5_scm::providers::issues::*;
use g5_scm::providers::{RepositoryCapability, RepositoryIdentity};

pub async fn fetch_repository(
    client: &GithubAppClient,
    token: &str,
    repository_id: u64,
) -> Result<GithubRepositoryInfo, IssueError> {
    let selected = client
        .repository_protocol(
            token,
            RepositoryIdentity::Id(repository_id),
            RepositoryCapability::MetadataRead,
        )
        .map_err(IssueError::from)?;
    g5_scm::providers::issues::fetch_repository(&selected).await
}

pub async fn fetch_issue(
    client: &GithubAppClient,
    token: &str,
    repository_id: u64,
    number: u64,
) -> Result<GithubIssue, IssueError> {
    let selected = client
        .repository_protocol(
            token,
            RepositoryIdentity::Id(repository_id),
            RepositoryCapability::IssuesRead,
        )
        .map_err(IssueError::from)?;
    g5_scm::providers::issues::fetch_issue(&selected, number).await
}

pub async fn fetch_issue_comments(
    client: &GithubAppClient,
    token: &str,
    repository_id: u64,
    number: u64,
) -> Result<Vec<GithubIssueComment>, IssueError> {
    let selected = client
        .repository_protocol(
            token,
            RepositoryIdentity::Id(repository_id),
            RepositoryCapability::IssuesRead,
        )
        .map_err(IssueError::from)?;
    g5_scm::providers::issues::fetch_issue_comments(&selected, number).await
}

pub async fn fetch_issue_timeline(
    client: &GithubAppClient,
    token: &str,
    repository_id: u64,
    number: u64,
) -> Result<(Vec<GithubTimelineItem>, bool), IssueError> {
    let selected = client
        .repository_protocol(
            token,
            RepositoryIdentity::Id(repository_id),
            RepositoryCapability::IssuesRead,
        )
        .map_err(IssueError::from)?;
    g5_scm::providers::issues::fetch_issue_timeline(&selected, number).await
}

pub async fn post_issue_comment(
    client: &GithubAppClient,
    user_token: &str,
    repository_id: u64,
    number: u64,
    body: &str,
) -> CommentPostOutcome {
    let selected = match client.repository_protocol(
        user_token,
        RepositoryIdentity::Id(repository_id),
        RepositoryCapability::IssueCommentWrite,
    ) {
        Ok(client) => client,
        Err(error) => return CommentPostOutcome::Rejected(error.into()),
    };
    g5_scm::providers::issues::post_issue_comment(&selected, number, body).await
}

pub async fn update_issue_comment(
    client: &GithubAppClient,
    user_token: &str,
    repository_id: u64,
    comment_id: &str,
    body: &str,
) -> CommentPostOutcome {
    let selected = match client.repository_protocol(
        user_token,
        RepositoryIdentity::Id(repository_id),
        RepositoryCapability::IssueCommentWrite,
    ) {
        Ok(client) => client,
        Err(error) => return CommentPostOutcome::Rejected(error.into()),
    };
    g5_scm::providers::issues::update_issue_comment(&selected, comment_id, body).await
}

pub async fn fetch_issue_comment(
    client: &GithubAppClient,
    token: &str,
    repository_id: u64,
    comment_id: &str,
) -> Result<GithubIssueComment, IssueError> {
    let selected = client
        .repository_protocol(
            token,
            RepositoryIdentity::Id(repository_id),
            RepositoryCapability::IssuesRead,
        )
        .map_err(IssueError::from)?;
    g5_scm::providers::issues::fetch_issue_comment(&selected, comment_id).await
}

pub async fn fetch_issue_comments_since(
    client: &GithubAppClient,
    token: &str,
    repository_id: u64,
    number: u64,
    since: &str,
    page: u32,
) -> Result<Vec<GithubIssueComment>, IssueError> {
    let selected = client
        .repository_protocol(
            token,
            RepositoryIdentity::Id(repository_id),
            RepositoryCapability::IssuesRead,
        )
        .map_err(IssueError::from)?;
    g5_scm::providers::issues::fetch_issue_comments_since(&selected, number, since, page).await
}

pub async fn search_repository_issues(
    client: &GithubAppClient,
    token: &str,
    full_name: &str,
    user_query: &str,
    filter: IssueSearchFilter<'_>,
) -> Result<GithubIssueSearchPage, IssueError> {
    let selected = client
        .repository_protocol(
            token,
            RepositoryIdentity::Name(full_name.to_owned()),
            RepositoryCapability::IssuesRead,
        )
        .map_err(IssueError::from)?;
    g5_scm::providers::issues::search_repository_issues(&selected, user_query, filter).await
}

pub async fn create_issue(
    client: &GithubAppClient,
    user_token: &str,
    repository_id: u64,
    title: &str,
    body: &str,
    assignees: &[String],
    labels: &[String],
) -> IssueCreateOutcome {
    let selected = match client.repository_protocol(
        user_token,
        RepositoryIdentity::Id(repository_id),
        RepositoryCapability::IssueWrite,
    ) {
        Ok(client) => client,
        Err(error) => return IssueCreateOutcome::Rejected(error.into()),
    };
    g5_scm::providers::issues::create_issue(&selected, title, body, assignees, labels).await
}

pub async fn update_issue_state(
    client: &GithubAppClient,
    user_token: &str,
    repository_id: u64,
    number: u64,
    change: IssueStateChange,
) -> IssueUpdateOutcome {
    let selected = match client.repository_protocol(
        user_token,
        RepositoryIdentity::Id(repository_id),
        RepositoryCapability::IssueWrite,
    ) {
        Ok(client) => client,
        Err(error) => return IssueUpdateOutcome::Rejected(error.into()),
    };
    g5_scm::providers::issues::update_issue_state(&selected, number, change).await
}

pub async fn update_issue(
    client: &GithubAppClient,
    user_token: &str,
    repository_id: u64,
    number: u64,
    edit: &IssueEdit,
) -> IssueUpdateOutcome {
    let selected = match client.repository_protocol(
        user_token,
        RepositoryIdentity::Id(repository_id),
        RepositoryCapability::IssueWrite,
    ) {
        Ok(client) => client,
        Err(error) => return IssueUpdateOutcome::Rejected(error.into()),
    };
    g5_scm::providers::issues::update_issue(&selected, number, edit).await
}

pub async fn is_assignable(
    client: &GithubAppClient,
    token: &str,
    repository_id: u64,
    login: &str,
) -> Result<bool, IssueError> {
    let selected = client
        .repository_protocol(
            token,
            RepositoryIdentity::Id(repository_id),
            RepositoryCapability::IssuesRead,
        )
        .map_err(IssueError::from)?;
    g5_scm::providers::issues::is_assignable(&selected, login).await
}

pub async fn fetch_issues_created_by(
    client: &GithubAppClient,
    token: &str,
    repository_id: u64,
    creator: &str,
    since: &str,
    page: u32,
) -> Result<(Vec<GithubIssue>, bool), IssueError> {
    let selected = client
        .repository_protocol(
            token,
            RepositoryIdentity::Id(repository_id),
            RepositoryCapability::IssuesRead,
        )
        .map_err(IssueError::from)?;
    g5_scm::providers::issues::fetch_issues_created_by(&selected, creator, since, page).await
}

pub async fn fetch_assignable_users(
    client: &GithubAppClient,
    token: &str,
    repository_id: u64,
) -> Result<Vec<GithubIssueUser>, IssueError> {
    let selected = client
        .repository_protocol(
            token,
            RepositoryIdentity::Id(repository_id),
            RepositoryCapability::IssuesRead,
        )
        .map_err(IssueError::from)?;
    g5_scm::providers::issues::fetch_assignable_users(&selected).await
}

pub async fn add_sub_issue(
    client: &GithubAppClient,
    user_token: &str,
    repository_id: u64,
    parent: u64,
    sub_issue_id: u64,
) -> SubIssueOutcome {
    let selected = match client.repository_protocol(
        user_token,
        RepositoryIdentity::Id(repository_id),
        RepositoryCapability::IssueWrite,
    ) {
        Ok(client) => client,
        Err(error) => return SubIssueOutcome::Rejected(error.into()),
    };
    g5_scm::providers::issues::add_sub_issue(&selected, parent, sub_issue_id).await
}

pub async fn fetch_parent_issue(
    client: &GithubAppClient,
    token: &str,
    repository_id: u64,
    number: u64,
) -> Result<Option<GithubIssue>, IssueError> {
    let selected = client
        .repository_protocol(
            token,
            RepositoryIdentity::Id(repository_id),
            RepositoryCapability::IssuesRead,
        )
        .map_err(IssueError::from)?;
    g5_scm::providers::issues::fetch_parent_issue(&selected, number).await
}

/// The domain repository's Issues-read protocol client.
fn issues_reader(
    client: &GithubAppClient,
    token: &str,
    repository_id: u64,
) -> Result<g5_scm::providers::RepositoryClient, IssueError> {
    client
        .repository_protocol(
            token,
            RepositoryIdentity::Id(repository_id),
            RepositoryCapability::IssuesRead,
        )
        .map_err(IssueError::from)
}

pub async fn fetch_rendered_issue(
    client: &GithubAppClient,
    token: &str,
    repository_id: u64,
    number: u64,
) -> Result<RenderedIssue, IssueError> {
    let selected = issues_reader(client, token, repository_id)?;
    g5_scm::providers::issues::fetch_rendered_issue(&selected, number).await
}

pub async fn fetch_rendered_comments(
    client: &GithubAppClient,
    token: &str,
    repository_id: u64,
    number: u64,
) -> Result<Vec<RenderedComment>, IssueError> {
    let selected = issues_reader(client, token, repository_id)?;
    g5_scm::providers::issues::fetch_rendered_comments(&selected, number).await
}

pub async fn fetch_rendered_comment(
    client: &GithubAppClient,
    token: &str,
    repository_id: u64,
    number: u64,
    comment_id: &str,
) -> Result<RenderedComment, IssueError> {
    let selected = issues_reader(client, token, repository_id)?;
    g5_scm::providers::issues::fetch_rendered_comment(&selected, number, comment_id).await
}

/// Open one attachment of the domain repository's issue text; no
/// credential is sent with it.
pub async fn open_attachment(
    client: &GithubAppClient,
    token: &str,
    repository_id: u64,
    attachment: &RenderedAttachment,
) -> Result<AttachmentContent, IssueError> {
    let selected = issues_reader(client, token, repository_id)?;
    g5_scm::providers::issues::open_attachment(&selected, attachment).await
}

// ---------------------------------------------------------------------------
// Labels (`g5.issue.labels.github`)
// ---------------------------------------------------------------------------

pub use g5_scm::providers::issues::labels::{
    LabelEdit, LabelRemoveOutcome, LabelWriteOutcome, LabelsAddOutcome,
};

fn label_writer(
    client: &GithubAppClient,
    user_token: &str,
    repository_id: u64,
) -> Result<g5_scm::providers::RepositoryClient, IssueError> {
    client
        .repository_protocol(
            user_token,
            RepositoryIdentity::Id(repository_id),
            RepositoryCapability::IssueWrite,
        )
        .map_err(IssueError::from)
}

pub async fn fetch_label_page(
    client: &GithubAppClient,
    token: &str,
    repository_id: u64,
    page: u32,
) -> Result<(Vec<GithubIssueLabel>, bool), IssueError> {
    let selected = issues_reader(client, token, repository_id)?;
    labels::fetch_label_page(&selected, page).await
}

pub async fn fetch_label(
    client: &GithubAppClient,
    token: &str,
    repository_id: u64,
    name: &str,
) -> Result<Option<GithubIssueLabel>, IssueError> {
    let selected = issues_reader(client, token, repository_id)?;
    labels::fetch_label(&selected, name).await
}

pub async fn create_label(
    client: &GithubAppClient,
    user_token: &str,
    repository_id: u64,
    name: &str,
    color: Option<&str>,
    description: &str,
) -> LabelWriteOutcome {
    match label_writer(client, user_token, repository_id) {
        Ok(selected) => labels::create_label(&selected, name, color, description).await,
        Err(error) => LabelWriteOutcome::Rejected(error),
    }
}

pub async fn update_label(
    client: &GithubAppClient,
    user_token: &str,
    repository_id: u64,
    current: &str,
    edit: &LabelEdit,
) -> LabelWriteOutcome {
    match label_writer(client, user_token, repository_id) {
        Ok(selected) => labels::update_label(&selected, current, edit).await,
        Err(error) => LabelWriteOutcome::Rejected(error),
    }
}

pub async fn delete_label(
    client: &GithubAppClient,
    user_token: &str,
    repository_id: u64,
    name: &str,
) -> LabelRemoveOutcome {
    match label_writer(client, user_token, repository_id) {
        Ok(selected) => labels::delete_label(&selected, name).await,
        Err(error) => LabelRemoveOutcome::Rejected(error),
    }
}

pub async fn add_issue_labels(
    client: &GithubAppClient,
    user_token: &str,
    repository_id: u64,
    number: u64,
    names: &[String],
) -> LabelsAddOutcome {
    match label_writer(client, user_token, repository_id) {
        Ok(selected) => labels::add_issue_labels(&selected, number, names).await,
        Err(error) => LabelsAddOutcome::Rejected(error),
    }
}

pub async fn remove_issue_label(
    client: &GithubAppClient,
    user_token: &str,
    repository_id: u64,
    number: u64,
    name: &str,
) -> LabelRemoveOutcome {
    match label_writer(client, user_token, repository_id) {
        Ok(selected) => labels::remove_issue_label(&selected, number, name).await,
        Err(error) => LabelRemoveOutcome::Rejected(error),
    }
}
