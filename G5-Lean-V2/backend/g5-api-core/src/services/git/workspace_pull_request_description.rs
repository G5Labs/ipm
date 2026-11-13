//! The canonical G5 description of a workspace PR and how each provider
//! publication compares with it (`g5.git.pr-description`). A read made when
//! shown: nothing is stored and neither side is ever written.
use super::{
    legacy_retry_payload, token_from_auth, ProviderCandidate, PullRequestProviderContext,
    WorkspacePullRequestProviderLifecycleService,
};
use crate::auth::RepositoryOperation;
use g5_authz::ScopedDb;
use g5_contracts::G5Error;
use g5_scm::providers::{PullRequestLifecycle, RepositoryCapability};
use serde::Serialize;
use uuid::Uuid;

/// Where the canonical G5 description comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DescriptionSource {
    /// The description saved with the G5 PR.
    Saved,
    /// The workspace's PR-body Project Doc.
    ProjectDoc,
}

/// How one provider body compares with what G5 would publish there.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DescriptionComparison {
    Matches,
    Differs,
    /// The provider body or the expected body could not be read.
    Unavailable,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestDescriptionView {
    pub pull_request_id: Uuid,
    /// `None` when the PR has neither a saved description nor a Project Doc.
    pub description: Option<String>,
    pub description_source: Option<DescriptionSource>,
    /// GitHub's rendering of `description`; `None` without a publication or
    /// when GitHub could not render it.
    pub description_html: Option<String>,
    /// One entry per published provider PR, ordered by repository.
    pub repositories: Vec<ProviderDescriptionView>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderDescriptionView {
    pub repo_id: Uuid,
    pub repository: String,
    pub number: i64,
    pub url: Option<String>,
    /// `None` when GitHub could not be read.
    pub lifecycle: Option<PullRequestLifecycle>,
    pub comparison: DescriptionComparison,
    pub body: Option<String>,
    pub body_html: Option<String>,
    pub error: Option<String>,
}

#[derive(sqlx::FromRow)]
struct PublishedRepo {
    org_id: Uuid,
    repo_id: Uuid,
    provider_repo_full_name: String,
    git_url: String,
    source_remote_branch: String,
    target_remote_branch: String,
    provider_pr_number: i64,
    provider_pr_url: Option<String>,
    source_head_sha: Option<String>,
    target_head_sha: Option<String>,
}

/// Most PRs one batch description read resolves.
pub const MAX_DESCRIPTION_BATCH: usize = 100;

/// One PR's canonical description without provider comparison or rendering:
/// what a listing's `body` needs.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestDescriptionText {
    pub pull_request_id: Uuid,
    pub description: Option<String>,
    pub description_source: Option<DescriptionSource>,
}

/// The saved description, when it is not blank.
fn saved_description(context: &PullRequestProviderContext) -> Option<&str> {
    context
        .description_override
        .as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

/// The canonical G5 description: the saved one, else the Project Doc.
fn canonical_description(
    context: &PullRequestProviderContext,
) -> Option<(String, DescriptionSource)> {
    if let Some(saved) = saved_description(context) {
        return Some((saved.to_owned(), DescriptionSource::Saved));
    }
    context
        .pr_body_doc
        .as_deref()
        .filter(|text| !text.trim().is_empty())
        .map(|doc| (doc.to_owned(), DescriptionSource::ProjectDoc))
}

/// Transport line endings are the only normalization: whitespace and Markdown
/// differences stay meaningful.
fn same_body(published: &str, expected: &str) -> bool {
    published.replace("\r\n", "\n") == expected.replace("\r\n", "\n")
}

impl WorkspacePullRequestProviderLifecycleService {
    /// The canonical G5 description alone, read on `db` without any provider
    /// call; `None` when the PR has neither a saved description nor a Project
    /// Doc. Used to quote the PR in the approval-request email
    /// (`g5.governance.approval.request-email`).
    pub async fn read_canonical_description(
        &self,
        db: &mut ScopedDb,
        pull_request_id: Uuid,
    ) -> Result<Option<String>, G5Error> {
        let context = self.load_context(db, pull_request_id).await?;
        Ok(canonical_description(&context).map(|(text, _)| text))
    }

    pub async fn read_pull_request_description(
        &self,
        mut db: ScopedDb,
        workspace_id: Uuid,
        pull_request_id: Uuid,
        actor: Uuid,
    ) -> Result<PullRequestDescriptionView, G5Error> {
        // Any PR the reader can read (`g5.git.pr-reads`); the path workspace
        // is only the caller's authorized scope.
        let _ = workspace_id;
        let reader = self.pull_request_reader(&db, actor).await?;
        self.ensure_readable_pull_request(&mut db, reader, pull_request_id)
            .await?;
        let context = self.load_context(&mut db, pull_request_id).await?;
        let published: Vec<PublishedRepo> = sqlx::query_as(
            r#"
            SELECT p.org_id, p.repo_id, p.provider_repo_full_name, r.git_url,
                   p.source_remote_branch, p.target_remote_branch,
                   p.provider_pr_number, p.provider_pr_url,
                   p.source_head_sha, p.target_head_sha
              FROM workspace_pull_request_provider_repos p
              JOIN repos r ON r.id = p.repo_id AND r.org_id = p.org_id
             WHERE p.pull_request_id = $1 AND p.provider_pr_number IS NOT NULL
             ORDER BY lower(p.provider_repo_full_name), p.repo_id
            "#,
        )
        .bind(pull_request_id)
        .fetch_all(db.conn())
        .await?;
        // Release the read transaction before Git work, token minting or
        // provider I/O.
        db.rollback().await.map_err(|e| G5Error::Internal {
            message: e.to_string(),
        })?;

        let description = canonical_description(&context);
        let reads = futures::future::join_all(
            published
                .iter()
                .map(|repo| self.read_publication(&context, repo, actor)),
        )
        .await;
        let mut repositories = Vec::with_capacity(reads.len());
        let mut render_token = None;
        for (view, token) in reads {
            if render_token.is_none() {
                render_token = token;
            }
            repositories.push(view);
        }
        // Bare references need one repository to resolve against; with several
        // publications GitHub links only qualified references.
        let render_context = match published.as_slice() {
            [only] => Some(only.provider_repo_full_name.as_str()),
            _ => None,
        };
        let description_html = match (&description, render_token) {
            (Some((text, _)), Some((token, name))) => {
                match self.repository_protocol(&token, &name, RepositoryCapability::PullRequestRead)
                {
                    Ok(client) => match client.render_markdown(text, render_context).await {
                        Ok(html) => Some(html),
                        Err(error) => {
                            tracing::warn!(
                                pull_request_id = %pull_request_id,
                                error = %error,
                                "GitHub Markdown rendering failed; the local renderer is used"
                            );
                            None
                        }
                    },
                    Err(_) => None,
                }
            }
            _ => None,
        };
        Ok(PullRequestDescriptionView {
            pull_request_id,
            description_source: description.as_ref().map(|(_, source)| *source),
            description: description.map(|(text, _)| text),
            description_html,
            repositories,
        })
    }

    /// The canonical descriptions of several PRs on the path workspace's
    /// sides, in the requested order, through the same resolver as
    /// [`Self::read_pull_request_description`] but with no provider call.
    /// Records are read in one transaction; Project Docs are read after it,
    /// and only for PRs without a saved description, which would win anyway.
    pub async fn read_pull_request_descriptions(
        &self,
        mut db: ScopedDb,
        workspace_id: Uuid,
        pull_request_ids: &[Uuid],
        actor: Uuid,
    ) -> Result<Vec<PullRequestDescriptionText>, G5Error> {
        let mut ids: Vec<Uuid> = Vec::with_capacity(pull_request_ids.len());
        for id in pull_request_ids {
            if !ids.contains(id) {
                ids.push(*id);
            }
        }
        if ids.len() > MAX_DESCRIPTION_BATCH {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "message": format!("At most {MAX_DESCRIPTION_BATCH} pull requests per description read")
                }),
            });
        }
        // Any PRs the reader can read (`g5.git.pr-reads`).
        let _ = workspace_id;
        let reader = self.pull_request_reader(&db, actor).await?;
        let belonging = self
            .readable_pull_request_ids(
                &mut db,
                reader,
                &super::read::ReadablePullRequestFilter {
                    ids: Some(&ids),
                    ..Default::default()
                },
            )
            .await?;
        if let Some(missing) = ids.iter().find(|id| !belonging.contains(id)) {
            return Err(G5Error::NotFound {
                resource: format!("workspace pull request {missing}"),
            });
        }
        let mut records = Vec::with_capacity(ids.len());
        for id in &ids {
            records.push(self.load_context_record(&mut db, *id).await?);
        }
        db.rollback().await.map_err(|e| G5Error::Internal {
            message: e.to_string(),
        })?;

        use futures::StreamExt;
        let resolved = futures::stream::iter(records)
            .map(|(mut context, source_names)| async move {
                if saved_description(&context).is_none() {
                    context.pr_body_doc =
                        self.read_context_pr_body_doc(&context, &source_names).await;
                }
                let description = canonical_description(&context);
                PullRequestDescriptionText {
                    pull_request_id: context.id,
                    description_source: description.as_ref().map(|(_, source)| *source),
                    description: description.map(|(text, _)| text),
                }
            })
            .buffered(8)
            .collect::<Vec<_>>()
            .await;
        Ok(resolved)
    }

    /// One provider publication: GitHub's lifecycle and body, compared with
    /// the serializer's output for this repository. Also returns the read
    /// token and repository when GitHub was reachable, for rendering.
    async fn read_publication(
        &self,
        context: &PullRequestProviderContext,
        repo: &PublishedRepo,
        actor: Uuid,
    ) -> (ProviderDescriptionView, Option<(String, String)>) {
        let mut view = ProviderDescriptionView {
            repo_id: repo.repo_id,
            repository: repo.provider_repo_full_name.clone(),
            number: repo.provider_pr_number,
            url: repo.provider_pr_url.clone(),
            lifecycle: None,
            comparison: DescriptionComparison::Unavailable,
            body: None,
            body_html: None,
            error: None,
        };
        let token = match self
            .credentials
            .resolve(
                repo.org_id,
                repo.repo_id,
                actor,
                &repo.git_url,
                RepositoryOperation::ReadPullRequests,
            )
            .await
        {
            Ok(auth) => token_from_auth(&auth),
            Err(error) => {
                tracing::debug!(repo_id = %repo.repo_id, error = %error, "PR description read has no credentials");
                None
            }
        };
        let Some(token) = token else {
            view.error = Some("GitHub repository credentials are unavailable".into());
            return (view, None);
        };
        let read = match self.repository_protocol(
            &token,
            &repo.provider_repo_full_name,
            RepositoryCapability::PullRequestRead,
        ) {
            Ok(client) => client
                .read_pull_request_description(repo.provider_pr_number)
                .await
                .map_err(|error| error.to_string()),
            Err(error) => Err(error.to_string()),
        };
        match read {
            Ok(pull) => {
                // Derive the serializer's bodies only once there is a provider
                // body to compare them with: the preferred payload, and for a
                // derived one also the legacy retry GitHub may have been sent
                // after refusing it with 422.
                let expected = match (&repo.source_head_sha, &repo.target_head_sha) {
                    (Some(source_head), Some(target_head)) => Some({
                        let preferred = self
                            .build_provider_pr_payload(
                                context,
                                &ProviderCandidate {
                                    repo_id: repo.repo_id,
                                    provider_full_name: repo.provider_repo_full_name.clone(),
                                    git_url: repo.git_url.clone(),
                                    source_local_branch_name: String::new(),
                                    source_remote_branch: repo.source_remote_branch.clone(),
                                    target_remote_branch: repo.target_remote_branch.clone(),
                                    source_head: source_head.clone(),
                                    target_head: target_head.clone(),
                                },
                            )
                            .await;
                        let mut bodies = vec![preferred.body];
                        if preferred.derived {
                            bodies.push(legacy_retry_payload(context).body);
                        }
                        bodies
                    }),
                    _ => None,
                };
                view.lifecycle = Some(pull.lifecycle);
                view.comparison = match (&pull.body, &expected) {
                    (Some(body), Some(expected))
                        if expected.iter().any(|candidate| same_body(body, candidate)) =>
                    {
                        DescriptionComparison::Matches
                    }
                    (_, Some(_)) => DescriptionComparison::Differs,
                    (_, None) => DescriptionComparison::Unavailable,
                };
                view.body = pull.body;
                view.body_html = pull.body_html;
                (view, Some((token, repo.provider_repo_full_name.clone())))
            }
            Err(error) => {
                tracing::warn!(
                    repo_id = %repo.repo_id,
                    pull_request = repo.provider_pr_number,
                    error = %error,
                    "GitHub PR description read failed"
                );
                view.error = Some("GitHub could not be read".into());
                (view, None)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(saved: Option<&str>, doc: Option<&str>) -> PullRequestProviderContext {
        PullRequestProviderContext {
            id: Uuid::nil(),
            org_id: Uuid::nil(),
            status: "open".into(),
            source_workspace_id: Uuid::nil(),
            source_workspace_name: "source".into(),
            source_docs_repo_id: None,
            source_org_slug: "org".into(),
            target_workspace_name: "target".into(),
            external_url: "https://g5.example".into(),
            preflight: serde_json::Value::Null,
            description_override: saved.map(str::to_owned),
            title: "Title".into(),
            pr_body_doc: doc.map(str::to_owned),
            repos: None,
        }
    }

    #[test]
    fn saved_description_wins_over_the_project_doc_and_blank_is_none() {
        assert_eq!(
            canonical_description(&context(Some(" Saved\n"), Some("Doc"))),
            Some(("Saved".into(), DescriptionSource::Saved))
        );
        assert_eq!(
            canonical_description(&context(Some("  "), Some("Doc body\n"))),
            Some(("Doc body\n".into(), DescriptionSource::ProjectDoc))
        );
        assert_eq!(canonical_description(&context(None, Some(" \n"))), None);
        assert_eq!(canonical_description(&context(None, None)), None);
    }

    #[test]
    fn legacy_retry_keeps_the_description_precedence_without_commit_text() {
        let provenance = "Created by G5 workspace https://g5.example/org/source";
        assert_eq!(
            legacy_retry_payload(&context(Some("Saved"), Some("Doc"))).body,
            format!("Saved\n{provenance}")
        );
        assert_eq!(
            legacy_retry_payload(&context(None, Some("Doc"))).body,
            format!("Doc\n{provenance}")
        );
        assert_eq!(legacy_retry_payload(&context(None, None)).body, provenance);
    }

    #[test]
    fn only_transport_line_endings_are_normalized() {
        assert!(same_body("a\r\nb\r\n", "a\nb\n"));
        assert!(!same_body("a\nb", "a\nb\n"));
        assert!(!same_body("a  \nb", "a\nb"));
        assert!(!same_body("*a*", "_a_"));
    }
}
