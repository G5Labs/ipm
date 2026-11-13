//! Live revision-bound check protocol and normalization. No publication or CI state changes.
use super::*;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
const MAX_PAGES: usize = 10;
const MAX_DETAIL_CHARS: usize = 32_768;
// Branch requirements change rarely, and GitHub enforces them again at its own merge.
const REQUIRED_TTL: Duration = Duration::from_secs(600);
#[derive(Default)]
pub struct PullRequestChecksQuery {
    pub head: Option<String>,
    pub details: bool,
    /// Required-check sets already read, shared by the caller across reads.
    pub required: Option<Arc<RequiredChecksCache>>,
    /// Read the required set from GitHub even when cached (the merge gate).
    pub refresh_required: bool,
}

/// A check the base branch requires: a check-run name or status context, optionally
/// bound to the GitHub App that must post it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct RequiredCheck {
    context: String,
    app: Option<u64>,
}

/// Successful required-check reads keyed by their branch URL (instance, repository
/// and branch), kept for `REQUIRED_TTL`. Only consulted after the caller's own
/// credential has read the PR.
#[derive(Default)]
pub struct RequiredChecksCache(Mutex<HashMap<String, (Instant, RequiredSet)>>);
type RequiredSet = Arc<Vec<RequiredCheck>>;
impl RequiredChecksCache {
    fn get(&self, key: &str) -> Option<RequiredSet> {
        let map = self.0.lock().unwrap_or_else(|e| e.into_inner());
        map.get(key)
            .filter(|(at, _)| at.elapsed() < REQUIRED_TTL)
            .map(|(_, set)| set.clone())
    }
    fn put(&self, key: String, set: RequiredSet) {
        let mut map = self.0.lock().unwrap_or_else(|e| e.into_inner());
        map.retain(|_, (at, _)| at.elapsed() < REQUIRED_TTL);
        map.insert(key, (Instant::now(), set));
    }
}
impl RepositoryClient {
    pub async fn read_pull_request_checks(
        &self,
        number: i64,
        query: &PullRequestChecksQuery,
        redact_values: &[String],
    ) -> Result<Value, ProviderError> {
        self.require(
            "read pull request checks",
            &[RepositoryCapability::ChecksRead],
        )?;
        if number <= 0 || query.head.as_deref().is_some_and(|h| !valid_sha(h)) {
            return Err(ProviderError::invalid(
                "read pull request checks",
                "Checks require a positive PR number and full head SHA",
            ));
        }
        let name = self.repository_name()?;
        let base = self.repo_url(&[])?.to_string();
        let result = CheckReader {
            http: self,
            token: &self.grant.token.0,
            base: base.trim_end_matches('/'),
            secrets: redact_values,
        }
        .observe(name, number, query)
        .await;
        result.map_err(|mut error| {
            error.message = transport::redact(&error.message, redact_values);
            for value in [
                &mut error.target,
                &mut error.request_id,
                &mut error.retry_after,
                &mut error.rate_limit_reset,
            ]
            .into_iter()
            .flatten()
            {
                *value = transport::redact(value, redact_values).into_boxed_str();
            }
            error
        })
    }
}
fn valid_sha(sha: &str) -> bool {
    sha.len() == 40 && sha.bytes().all(|b| b.is_ascii_hexdigit())
}

type ReadFailure = ProviderError;
fn unavailable(message: &str) -> ProviderError {
    ProviderError::new(
        "read pull request checks",
        ProviderErrorKind::InvalidResponse,
        crate::FailureEffect::Refused,
        message,
    )
}
async fn get(
    client: &RepositoryClient,
    secrets: &[String],
    url: &str,
) -> Result<(Value, bool), ReadFailure> {
    let response = client
        .send(
            "read pull request checks",
            reqwest::Method::GET,
            Url::parse(url).map_err(|_| unavailable("Invalid check operation URL"))?,
            Payload::Empty,
            &secrets.iter().map(String::as_str).collect::<Vec<_>>(),
            REPOSITORY_TIMEOUT,
        )
        .await?;
    // Rate-limit evidence is the transport's; a 403 without it is a refusal.
    Ok((response.json()?, response.has_next_page()))
}

async fn pages(
    http: &RepositoryClient,
    secrets: &[String],
    url: &str,
    field: Option<&str>,
    max: usize,
) -> Result<(Vec<Value>, bool), ReadFailure> {
    let mut rows = Vec::new();
    for page in 1..=max {
        let (value, more) = get(http, secrets, &format!("{url}&per_page=100&page={page}")).await?;
        let array = field
            .map_or(&value, |f| &value[f])
            .as_array()
            .ok_or_else(|| unavailable("GitHub returned invalid check enumeration"))?;
        rows.extend(array.iter().cloned());
        if !more {
            if field.is_some()
                && value["total_count"]
                    .as_u64()
                    .is_some_and(|total| total > rows.len() as u64)
            {
                return Err(unavailable("GitHub check enumeration is incomplete"));
            }
            return Ok((rows, false));
        }
    }
    Ok((rows, true))
}

#[derive(PartialEq)]
struct Revision {
    head: String,
    base: String,
    base_ref: String,
    merge: Option<String>,
    state: String,
    merged: bool,
    merge_pending: bool,
}
impl Revision {
    /// GitHub's PR lifecycle as check output reports it: `open`, `closed` or `merged`.
    fn pr_state(&self) -> &str {
        if self.merged {
            "merged"
        } else {
            &self.state
        }
    }
}
async fn revision(
    http: &RepositoryClient,
    secrets: &[String],
    base: &str,
    number: i64,
) -> Result<Revision, ReadFailure> {
    let (pull, _) = get(http, secrets, &format!("{base}/pulls/{number}")).await?;
    if pull["number"].as_i64() != Some(number) {
        return Err(unavailable("GitHub returned a mismatched PR"));
    }
    let sha = |v: &Value| {
        v.as_str()
            .filter(|s| valid_sha(s))
            .map(str::to_ascii_lowercase)
            .ok_or_else(|| unavailable("GitHub returned an invalid PR revision"))
    };
    let state = pull["state"]
        .as_str()
        .filter(|s| matches!(*s, "open" | "closed"))
        .ok_or_else(|| unavailable("GitHub returned an invalid PR state"))?
        .to_owned();
    let open = state == "open";
    let merge = if open && pull["mergeable"] == true {
        Some(sha(&pull["merge_commit_sha"])?)
    } else {
        None
    };
    Ok(Revision {
        head: sha(&pull["head"]["sha"])?,
        base: sha(&pull["base"]["sha"])?,
        base_ref: pull["base"]["ref"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| unavailable("GitHub returned a PR without its base branch"))?
            .to_owned(),
        merge,
        state,
        merged: pull["merged"] == true,
        merge_pending: open && pull["mergeable"].as_bool().is_none(),
    })
}

/// What moved between the start and the end of one check read. Only a change
/// to the PR itself (its head, state or base branch) makes the observed
/// checks belong to another PR revision for good; the others are GitHub
/// settling around an unchanged head, so the read retries them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Moved {
    /// The PR's head, state (including merged) or base branch changed.
    Head,
    /// The base branch advanced.
    Base,
    /// GitHub finished, or restarted, computing mergeability.
    Mergeability,
    /// GitHub recomputed the test-merge commit.
    TestMerge,
    /// A check attempt was added or changed during the read.
    Attempts,
}

impl Moved {
    fn between(before: &Revision, after: &Revision) -> Option<Self> {
        if before == after {
            None
        } else if before.head != after.head
            || before.state != after.state
            || before.merged != after.merged
            || before.base_ref != after.base_ref
        {
            Some(Self::Head)
        } else if before.base != after.base {
            Some(Self::Base)
        } else if before.merge_pending != after.merge_pending
            || before.merge.is_some() != after.merge.is_some()
        {
            Some(Self::Mergeability)
        } else {
            Some(Self::TestMerge)
        }
    }

    fn message(self) -> &'static str {
        match self {
            Self::Head => "The PR's head, state or base branch changed while reading checks; retry for current results.",
            Self::Base => "The base branch kept advancing while reading checks; retry for current results.",
            Self::Mergeability => "GitHub is computing the current test-merge revision; retry shortly.",
            Self::TestMerge => "GitHub kept recomputing the test merge while reading checks; retry for current results.",
            Self::Attempts => "Check attempts changed while reading; retry for current results.",
        }
    }
}

/// Reads per request when GitHub settles around an unchanged head.
const CHECK_READ_ATTEMPTS: u32 = 3;

#[derive(PartialEq)]
struct Snapshot {
    suites: Vec<Value>,
    runs: Vec<Value>,
    statuses: Vec<Value>,
}
struct CheckReader<'a> {
    http: &'a RepositoryClient,
    token: &'a str,
    base: &'a str,
    secrets: &'a [String],
}
impl CheckReader<'_> {
    async fn snapshot(&self, sha: &str) -> Result<Snapshot, ReadFailure> {
        let mut sets = Vec::new();
        for (endpoint, field) in [
            ("check-suites?", Some("check_suites")),
            ("check-runs?filter=latest", Some("check_runs")),
            ("statuses?", None),
        ] {
            let (mut rows, more) = pages(
                self.http,
                self.secrets,
                &format!("{}/commits/{sha}/{endpoint}", self.base),
                field,
                MAX_PAGES,
            )
            .await?;
            if more || rows.len() > 1000 {
                return Err(unavailable(
                    "GitHub check enumeration exceeded 1000 entries; results are incomplete",
                ));
            }
            // Status context selection uses GitHub's newest-first order below.
            if field.is_some() {
                rows.sort_by_key(|v| v["id"].as_u64());
            }
            sets.push(rows);
        }
        let statuses = sets.pop().unwrap();
        let runs = sets.pop().unwrap();
        Ok(Snapshot {
            suites: sets.pop().unwrap(),
            runs,
            statuses,
        })
    }

    /// The base branch's required checks: classic branch protection plus active
    /// rulesets. Any unreadable source fails the read; it never means "none".
    async fn required(
        &self,
        branch: &str,
        cache: Option<&RequiredChecksCache>,
        refresh: bool,
    ) -> Result<RequiredSet, ReadFailure> {
        let url = self
            .http
            .repo_url(&["branches", branch])
            .map_err(|_| unavailable("Invalid base branch URL"))?
            .to_string();
        if let Some(set) = cache.filter(|_| !refresh).and_then(|c| c.get(&url)) {
            return Ok(set);
        }
        let invalid = || unavailable("GitHub returned an invalid required check");
        let mut set = Vec::new();
        let (branch_row, _) = get(self.http, self.secrets, &url).await?;
        let protection = &branch_row["protection"]["required_status_checks"];
        if branch_row["protection"]["enabled"] != false && protection["enforcement_level"] != "off"
        {
            if let Some(checks) = protection["checks"].as_array() {
                for check in checks {
                    set.push(requirement(&check["context"], &check["app_id"]).ok_or_else(invalid)?);
                }
            } else {
                for context in protection["contexts"].as_array().into_iter().flatten() {
                    set.push(requirement(context, &Value::Null).ok_or_else(invalid)?);
                }
            }
        }
        let rules_url = self
            .http
            .repo_url(&["rules", "branches", branch])
            .map_err(|_| unavailable("Invalid base branch URL"))?;
        let (rules, more) = pages(
            self.http,
            self.secrets,
            &format!("{rules_url}?"),
            None,
            MAX_PAGES,
        )
        .await?;
        if more {
            return Err(unavailable(
                "GitHub branch rules exceeded 1000 entries; results are incomplete",
            ));
        }
        for rule in rules
            .iter()
            .filter(|r| r["type"] == "required_status_checks")
        {
            let checks = rule["parameters"]["required_status_checks"]
                .as_array()
                .ok_or_else(invalid)?;
            for check in checks {
                set.push(
                    requirement(&check["context"], &check["integration_id"]).ok_or_else(invalid)?,
                );
            }
        }
        set.sort();
        set.dedup();
        let set = Arc::new(set);
        if let Some(cache) = cache {
            cache.put(url, set.clone());
        }
        Ok(set)
    }

    fn clean(&self, value: &Value) -> Value {
        let mut value = value.clone();
        redact(&mut value, self.secrets);
        redact(&mut value, &[self.token.to_owned()]);
        value
    }

    /// Read the PR's checks, re-reading when only GitHub's view around an
    /// unchanged head moved during the read ([`Moved`]); a moved head is
    /// reported at once.
    async fn observe(
        &self,
        repository: &str,
        number: i64,
        query: &PullRequestChecksQuery,
    ) -> Result<Value, ReadFailure> {
        let mut attempt = 1;
        loop {
            let (mut result, moved) = self.observe_once(repository, number, query).await?;
            match moved {
                None => return Ok(result),
                Some(moved) if moved == Moved::Head || attempt == CHECK_READ_ATTEMPTS => {
                    result["availability"] = json!("pending");
                    result["message"] = json!(moved.message());
                    return Ok(result);
                }
                Some(_) => {
                    tokio::time::sleep(std::time::Duration::from_millis(500 * u64::from(attempt)))
                        .await;
                    attempt += 1;
                }
            }
        }
    }

    /// One read, and what moved during it, if anything.
    async fn observe_once(
        &self,
        repository: &str,
        number: i64,
        query: &PullRequestChecksQuery,
    ) -> Result<(Value, Option<Moved>), ReadFailure> {
        let rev = revision(self.http, self.secrets, self.base, number).await?;
        let mut result = json!({"repository":repository,"number":number,"headRefOid":rev.head,"prState":rev.pr_state(),"expectedHead":query.head,"availability":"ready","checks":[]});
        if query
            .head
            .as_ref()
            .is_some_and(|expected| !expected.eq_ignore_ascii_case(&rev.head))
        {
            result["availability"] = json!("pending");
            result["message"] = json!("Provider head does not yet match --head; a push usually reaches GitHub within a minute. With Auto-push off, publish it with g5d pr update.");
            return Ok((result, None));
        }
        let required = self
            .required(
                &rev.base_ref,
                query.required.as_deref(),
                query.refresh_required,
            )
            .await?;
        result["baseRefName"] = self.clean(&json!(rev.base_ref));
        result["requiredChecks"] = self.clean(&json!(required
            .iter()
            .map(|r| json!({"context":r.context,"appId":r.app}))
            .collect::<Vec<_>>()));
        let mut met = vec![false; required.len()];
        let mut revisions = vec![(rev.head.as_str(), "head")];
        if let Some(merge) = rev.merge.as_deref().filter(|m| *m != rev.head) {
            revisions.push((merge, "test_merge"));
        }
        let mut snapshots = Vec::new();
        for (sha, _) in &revisions {
            snapshots.push(self.snapshot(sha).await?);
        }
        // GitHub judges required checks on the test-merge commit once it has any
        // check or status, and on the head otherwise.
        let evaluated = match snapshots.get(1) {
            Some(merge) if !merge.runs.is_empty() || !merge.statuses.is_empty() => 1,
            _ => 0,
        };
        let mut checks = Vec::new();
        for (index, ((sha, kind), snapshot)) in revisions.iter().zip(&snapshots).enumerate() {
            let judged = index == evaluated;
            let mut suites = HashMap::new();
            for suite in &snapshot.suites {
                let id = identity(suite)?;
                same_revision(suite, sha)?;
                suites.insert(id, suite);
            }
            for run in &snapshot.runs {
                let id = identity(run)?;
                same_revision(run, sha)?;
                let suite = run["check_suite"]["id"]
                    .as_u64()
                    .and_then(|id| suites.get(&id))
                    .ok_or_else(|| {
                        unavailable("GitHub returned a check without its current suite")
                    })?;
                let name = run["name"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| unavailable("GitHub returned an unnamed check"))?;
                let state = if suite["status"] == "queued" && run["status"] == "completed" {
                    "pending"
                } else {
                    run_state(run)
                };
                let bucket = bucket(state);
                let is_required = judged
                    && mark(&required, &mut met, |r| {
                        r.context == name
                            && r.app
                                .is_none_or(|app| run["app"]["id"].as_u64() == Some(app))
                    });
                let mut check = json!({"id":id,"kind":"check_run","name":self.clean(&json!(name)),"state":state.to_ascii_uppercase(),"bucket":bucket,"required":is_required,
                    "repository":repository,"number":number,"headRefOid":rev.head,"commitOid":sha,"revision":kind,
                    "link":self.clean(&json!(safe_link(&run["details_url"]).or_else(|| safe_link(&run["html_url"])))),
                    "startedAt":self.clean(&run["started_at"]),"completedAt":self.clean(&run["completed_at"]),
                    "description":self.clean(&run["output"]["title"]),"event":null,"workflow":null});
                if query.details && matches!(bucket, "fail" | "cancel") {
                    let (details, error) = self.diagnostics(id, &run["output"]).await;
                    check["details"] = details;
                    if let Some(error) = error {
                        checks.push(check);
                        result["checks"] = json!(checks);
                        result["availability"] = json!("unavailable");
                        result["message"] = self.clean(&json!(error.to_string()));
                        result["rateLimited"] = json!(error.rate_limited);
                        result["retryAt"] = json!(error.retry_at(chrono::Utc::now()));
                        return Ok((result, None));
                    }
                }
                checks.push(check);
            }
            let mut contexts = HashSet::new();
            for status in &snapshot.statuses {
                let context = status["context"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| unavailable("GitHub returned an invalid status context"))?;
                if !contexts.insert(context.to_ascii_lowercase()) {
                    continue;
                }
                let state = status["state"]
                    .as_str()
                    .filter(|s| matches!(*s, "success" | "failure" | "error" | "pending"))
                    .unwrap_or("unknown");
                // GitHub does not report which App posted a status, so an App-bound
                // requirement matches a status by context alone.
                let is_required = judged
                    && mark(&required, &mut met, |r| {
                        r.context.eq_ignore_ascii_case(context)
                    });
                let mut check = json!({"id":identity(status)?,"kind":"status","name":self.clean(&json!(context)),"state":state.to_ascii_uppercase(),"bucket":bucket(state),"required":is_required,
                    "repository":repository,"number":number,"headRefOid":rev.head,"commitOid":sha,"revision":kind,"description":self.clean(&status["description"]),
                    "link":self.clean(&json!(safe_link(&status["target_url"]))),"startedAt":self.clean(&status["created_at"]),
                    "completedAt":if matches!(state,"pending"|"unknown") {Value::Null} else {self.clean(&status["updated_at"])},"event":null,"workflow":null});
                if query.details && matches!(bucket(state), "fail" | "cancel") {
                    check["details"] = json!({"availability":"unavailable","summary":self.clean(&status["description"]),"message":"Only a commit status description/link is available; diagnostic logs are unavailable through GitHub checks."});
                }
                checks.push(check);
            }
        }
        let (evaluated_sha, evaluated_kind) = revisions[evaluated];
        for (requirement, _) in required.iter().zip(&met).filter(|(_, met)| !**met) {
            checks.push(json!({"id":null,"kind":"expected","name":self.clean(&json!(requirement.context)),"state":"EXPECTED","bucket":"pending","required":true,
                "repository":repository,"number":number,"headRefOid":rev.head,"commitOid":evaluated_sha,"revision":evaluated_kind,
                "description":self.clean(&json!(format!("Required by {}; not reported on this revision yet.", rev.base_ref))),
                "link":null,"startedAt":null,"completedAt":null,"event":null,"workflow":null}));
        }
        // Only a settled result ends a wait or admits a merge, so only it repeats
        // the observation (g5.git.check-observation); a pending one is reported as
        // read. As in the merge gate's verdict, only a required check that is
        // pending or not yet reported keeps the result pending; an advisory one
        // does not. Recheck attempt identity as well as source/base/test-merge
        // revisions; GitHub settling around an unchanged head is re-read
        // (g5.git.check-read-settling). There is no provider transaction; these
        // are bounded observations, not merge locks.
        let settled = !rev.merge_pending
            && checks
                .iter()
                .all(|check| check["bucket"] != "pending" || check["required"] != true);
        if settled {
            for ((sha, _), snapshot) in revisions.iter().zip(&snapshots) {
                if self.snapshot(sha).await? != *snapshot {
                    return Ok((result, Some(Moved::Attempts)));
                }
            }
            let current = revision(self.http, self.secrets, self.base, number).await?;
            if let Some(moved) = Moved::between(&rev, &current) {
                result["headRefOid"] = json!(current.head);
                result["prState"] = json!(current.pr_state());
                return Ok((result, Some(moved)));
            }
        }
        checks.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
        result["checks"] = json!(checks);
        // Still computing: the checks so far stand, reported as this single
        // pending observation (g5.git.check-observation).
        if rev.merge_pending {
            result["availability"] = json!("pending");
            result["message"] = json!(Moved::Mergeability.message());
        }
        Ok((result, None))
    }

    async fn diagnostics(&self, id: u64, output: &Value) -> (Value, Option<ReadFailure>) {
        let (annotations, more, error) = if output["annotations_count"].as_u64().unwrap_or(0) > 0 {
            match pages(
                self.http,
                self.secrets,
                &format!("{}/check-runs/{id}/annotations?", self.base),
                None,
                1,
            )
            .await
            {
                Ok((rows, more)) => (rows, more, None),
                Err(error) => (Vec::new(), false, Some(error)),
            }
        } else {
            (Vec::new(), false, None)
        };
        let mut details = self.clean(&json!({"title":output["title"],"summary":output["summary"],"text":output["text"],"annotations":annotations.iter().map(|a| json!({
            "path":a["path"],"startLine":a["start_line"],"endLine":a["end_line"],"level":a["annotation_level"],"title":a["title"],"message":a["message"],"rawDetails":a["raw_details"]
        })).collect::<Vec<_>>()}));
        let supplied = ["summary", "text"]
            .iter()
            .any(|k| details[k].as_str().is_some_and(|s| !s.is_empty()))
            || !annotations.is_empty();
        let mut remaining = MAX_DETAIL_CHARS;
        let truncated = truncate(&mut details, &mut remaining)
            || more
            || output["annotations_count"]
                .as_u64()
                .is_some_and(|count| count > annotations.len() as u64);
        details["truncated"] = json!(truncated);
        details["availability"] = json!(if error.is_some() {
            "partial"
        } else if supplied {
            "available"
        } else {
            "unavailable"
        });
        if let Some(error) = &error {
            details["annotationAvailability"] = json!("unavailable");
            details["message"] = self.clean(&json!(error.to_string()));
        } else if !supplied {
            details["message"] = json!("The check provides no diagnostic output; logs are unavailable through GitHub. See its link.");
        }
        (details, error)
    }
}

fn identity(value: &Value) -> Result<u64, ReadFailure> {
    value["id"]
        .as_u64()
        .filter(|id| *id > 0)
        .ok_or_else(|| unavailable("GitHub returned invalid check identity"))
}
fn requirement(context: &Value, app: &Value) -> Option<RequiredCheck> {
    Some(RequiredCheck {
        context: context.as_str().filter(|s| !s.is_empty())?.to_owned(),
        // Null, and classic protection's -1, mean any source.
        app: if app.is_null() || app.as_i64() == Some(-1) {
            None
        } else {
            Some(app.as_u64()?)
        },
    })
}
/// Mark every requirement the row satisfies; true when there was one.
fn mark(
    required: &[RequiredCheck],
    met: &mut [bool],
    matches: impl Fn(&RequiredCheck) -> bool,
) -> bool {
    let mut any = false;
    for (requirement, met) in required.iter().zip(met.iter_mut()) {
        if matches(requirement) {
            *met = true;
            any = true;
        }
    }
    any
}
fn same_revision(value: &Value, sha: &str) -> Result<(), ReadFailure> {
    if value["head_sha"]
        .as_str()
        .is_none_or(|s| !s.eq_ignore_ascii_case(sha))
    {
        Err(unavailable(
            "GitHub returned checks for a different revision",
        ))
    } else {
        Ok(())
    }
}
fn run_state(run: &Value) -> &str {
    let s = if run["status"] == "completed" {
        run["conclusion"].as_str()
    } else {
        run["status"].as_str()
    };
    match s {
        Some(
            s @ ("success" | "neutral" | "skipped" | "failure" | "error" | "timed_out"
            | "action_required" | "startup_failure" | "stale" | "cancelled" | "queued"
            | "in_progress" | "waiting" | "pending" | "requested"),
        ) => s,
        _ => "unknown",
    }
}
fn bucket(state: &str) -> &'static str {
    match state {
        "success" => "pass",
        "neutral" | "skipped" => "skipping",
        "failure" | "error" | "timed_out" | "action_required" | "startup_failure" | "stale" => {
            "fail"
        }
        "cancelled" => "cancel",
        _ => "pending",
    }
}
fn safe_link(value: &Value) -> Option<&str> {
    let text = value.as_str()?;
    let url = reqwest::Url::parse(text).ok()?;
    (matches!(url.scheme(), "https" | "http")
        && url.username().is_empty()
        && url.password().is_none())
    .then_some(text)
}

fn truncate(value: &mut Value, remaining: &mut usize) -> bool {
    match value {
        Value::String(s) => {
            let len = s.chars().count();
            let keep = len.min(*remaining);
            *remaining -= keep;
            if len > keep {
                *s = s.chars().take(keep).collect();
                true
            } else {
                false
            }
        }
        Value::Array(a) => a
            .iter_mut()
            .fold(false, |cut, v| truncate(v, remaining) | cut),
        Value::Object(o) => o
            .values_mut()
            .fold(false, |cut, v| truncate(v, remaining) | cut),
        _ => false,
    }
}
fn redact(value: &mut Value, secrets: &[String]) {
    match value {
        Value::String(s) => {
            for secret in secrets {
                *s = s.replace(secret, "[redacted]");
            }
        }
        Value::Array(a) => {
            for v in a {
                redact(v, secrets);
            }
        }
        Value::Object(o) => {
            for v in o.values_mut() {
                redact(v, secrets);
            }
        }
        _ => (),
    }
}

#[cfg(test)]
mod moved_tests {
    use super::*;

    fn rev(head: &str, base: &str, merge: Option<&str>, pending: bool) -> Revision {
        Revision {
            head: head.into(),
            base: base.into(),
            base_ref: "main".into(),
            merge: merge.map(str::to_owned),
            state: "open".into(),
            merged: false,
            merge_pending: pending,
        }
    }

    #[test]
    fn only_a_change_to_the_pr_itself_counts_as_its_head_moving() {
        let before = rev("h", "b", Some("m"), false);
        assert_eq!(
            Moved::between(&before, &rev("h", "b", Some("m"), false)),
            None
        );
        assert_eq!(
            Moved::between(&before, &rev("h2", "b2", Some("m2"), false)),
            Some(Moved::Head)
        );
        let mut closed = rev("h", "b", None, false);
        closed.state = "closed".into();
        assert_eq!(Moved::between(&before, &closed), Some(Moved::Head));
        let mut merged = rev("h", "b", Some("m"), false);
        merged.merged = true;
        assert_eq!(Moved::between(&before, &merged), Some(Moved::Head));
        assert_eq!(
            Moved::between(&before, &rev("h", "b2", Some("m2"), false)),
            Some(Moved::Base)
        );
        assert_eq!(
            Moved::between(&rev("h", "b", None, true), &rev("h", "b", Some("m"), false)),
            Some(Moved::Mergeability)
        );
        assert_eq!(
            Moved::between(&before, &rev("h", "b", None, true)),
            Some(Moved::Mergeability)
        );
        assert_eq!(
            Moved::between(&before, &rev("h", "b", Some("m2"), false)),
            Some(Moved::TestMerge)
        );
    }
}
