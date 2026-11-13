//! Platform report protocol. Signing, cache, attribution and outbox recovery stay with the owner.
use super::*;
use base64::Engine as _;
use serde::Deserialize;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

const REPORT_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Clone, Debug)]
pub struct ReportGrant(RepositoryGrant);
impl ReportGrant {
    pub fn new(
        endpoints: ProviderEndpoints,
        repository: String,
        selected_token: String,
    ) -> Result<Self, ProviderError> {
        endpoints.github("configure report delivery")?;
        RepositoryGrant::new(
            endpoints,
            RepositoryIdentity::Name(repository),
            RepositoryCapability::ReportDelivery,
            selected_token,
        )
        .map(Self)
    }
}
/// The report owner's narrowed installation-token request: the one destination
/// repository by name, with issues and contents write.
impl InstallationTokenRequest {
    pub fn report_delivery(repository: &str) -> Result<Self, ProviderError> {
        let (_, name) = repository_name(repository)?;
        Ok(Self {
            repository_ids: BTreeSet::new(),
            repositories: BTreeSet::from([name.to_owned()]),
            permissions: BTreeMap::from([
                ("issues".to_owned(), "write".to_owned()),
                ("contents".to_owned(), "write".to_owned()),
            ]),
        })
    }
}
pub struct ReportClient {
    repository: RepositoryClient,
}
impl ProviderClients {
    pub fn reports(&self, grant: ReportGrant) -> ReportClient {
        ReportClient {
            repository: self.repository(grant.0),
        }
    }
}
#[derive(Debug, Clone, Deserialize)]
pub struct CreatedReportIssue {
    pub number: i64,
    pub html_url: String,
    body: Option<String>,
}
impl ReportClient {
    fn repo_url(&self, suffix: &[&str]) -> Result<Url, ProviderError> {
        self.repository.repo_url(suffix)
    }
    async fn request(
        &self,
        operation: &'static str,
        method: reqwest::Method,
        url: Url,
        payload: Payload<'_>,
    ) -> Result<Response, ProviderError> {
        self.repository
            .send(operation, method, url, payload, &[], REPORT_TIMEOUT)
            .await
    }
    fn full_name(&self) -> Result<&str, ProviderError> {
        self.repository.repository_name()
    }
    fn validate_issue(
        &self,
        response: &Response,
        issue: CreatedReportIssue,
    ) -> Result<CreatedReportIssue, ProviderError> {
        let (owner, name) = repository_name(self.full_name()?)?;
        let path = self
            .repository
            .grant
            .endpoints
            .exact_web_path(&issue.html_url);
        let valid_url = path.as_deref().is_some_and(|parts| {
            matches!(parts, [o, r, kind, number]
                if o.eq_ignore_ascii_case(owner) && r.eq_ignore_ascii_case(name)
                    && kind == "issues" && number.parse::<i64>().ok() == Some(issue.number))
        });
        if issue.number <= 0 || !valid_url {
            return Err(response.failure(
                ProviderErrorKind::InvalidResponse,
                "The report issue identity or URL is invalid for this destination",
            ));
        }
        Ok(issue)
    }
    pub async fn create_issue(
        &self,
        title: &str,
        body: &str,
        labels: &[String],
    ) -> Result<CreatedReportIssue, ProviderError> {
        let response = self
            .request(
                "create report issue",
                reqwest::Method::POST,
                self.repo_url(&["issues"])?,
                Payload::Json(json!({"title":title,"body":body,"labels":labels})),
            )
            .await?;
        self.validate_issue(&response, response.json()?)
    }
    pub async fn find_issue_by_marker(
        &self,
        submission_id: Uuid,
    ) -> Result<Option<CreatedReportIssue>, ProviderError> {
        let marker = format!("<!-- g5-report:{submission_id} -->");
        let response = self
            .request(
                "find report issue",
                reqwest::Method::GET,
                query(
                    self.repository.grant.endpoints.api(&["search", "issues"]),
                    &[
                        (
                            "q",
                            format!("repo:{} type:issue in:body \"{marker}\"", self.full_name()?),
                        ),
                        ("per_page", "2".into()),
                    ],
                ),
                Payload::Empty,
            )
            .await?;
        #[derive(Deserialize)]
        struct Page {
            items: Vec<CreatedReportIssue>,
        }
        let page: Page = response.json()?;
        page.items
            .into_iter()
            .find(|issue| {
                issue
                    .body
                    .as_deref()
                    .is_some_and(|body| body.contains(&marker))
            })
            .map(|issue| self.validate_issue(&response, issue))
            .transpose()
    }
    fn asset_url(
        &self,
        response: &Response,
        sha: &str,
        path: &[&str],
    ) -> Result<String, ProviderError> {
        if sha.is_empty() || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(response.failure(
                ProviderErrorKind::InvalidResponse,
                "The screenshot commit returned an invalid SHA",
            ));
        }
        let (owner, name) = repository_name(self.full_name()?)?;
        let mut segments = vec![owner, name, "raw", sha];
        segments.extend_from_slice(path);
        Ok(self.repository.grant.endpoints.web(&segments).to_string())
    }
    pub async fn find_screenshot_url(&self, id: Uuid) -> Result<Option<String>, ProviderError> {
        let filename = format!("{id}.png");
        let response = self
            .request(
                "find report screenshot",
                reqwest::Method::GET,
                self.repo_url(&["contents", "assets", "issues", &filename])?,
                Payload::Empty,
            )
            .await?;
        if response.status == 404 {
            return Ok(None);
        }
        let _: serde_json::Map<String, serde_json::Value> = response.json()?;
        let path = format!("assets/issues/{filename}");
        let response = self
            .request(
                "find report screenshot commit",
                reqwest::Method::GET,
                query(
                    self.repo_url(&["commits"])?,
                    &[("path", path), ("per_page", "1".into())],
                ),
                Payload::Empty,
            )
            .await?;
        #[derive(Deserialize)]
        struct Commit {
            sha: String,
        }
        let commits: Vec<Commit> = response.json()?;
        let sha = commits.first().ok_or_else(|| {
            response.failure(
                ProviderErrorKind::InvalidResponse,
                "Existing screenshot asset has no commit history",
            )
        })?;
        self.asset_url(&response, &sha.sha, &["assets", "issues", &filename])
            .map(Some)
    }
    pub async fn put_screenshot(&self, id: Uuid, png: &[u8]) -> Result<String, ProviderError> {
        let filename = format!("{id}.png");
        let response=self.request("create report screenshot",reqwest::Method::PUT,self.repo_url(&["contents","assets","issues",&filename])?,Payload::Json(json!({"message":format!("platform bug report screenshot {id}"),"content":base64::engine::general_purpose::STANDARD.encode(png)}))).await?;
        #[derive(Deserialize)]
        struct Commit {
            sha: String,
        }
        #[derive(Deserialize)]
        struct Result {
            commit: Commit,
            #[serde(rename = "content")]
            _content: serde_json::Map<String, serde_json::Value>,
        }
        let result: Result = response.json()?;
        self.asset_url(
            &response,
            &result.commit.sha,
            &["assets", "issues", &filename],
        )
    }
}
