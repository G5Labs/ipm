//! `g5d pr` commands that follow `gh pr` (`g5.git.pr-cli.*`): the PR
//! selector shared by commands that act on a G5 PR, publication of committed
//! work for `create` and `update`, `edit` and `close`.
use super::{invalid, select_output, text, Api};
use crate::cli::{stable_context, PrCommand, PrViewArgs};
use g5_contracts::G5Error;
use serde_json::{json, Value};
use std::path::PathBuf;
use uuid::Uuid;

impl Api<'_> {
    fn prefix(&self) -> &'static str {
        if self.internal {
            "internal"
        } else {
            "v1"
        }
    }

    /// GET one API path and return its `data`.
    pub(super) async fn get_data(&self, path: &str) -> Result<Value, G5Error> {
        if self.internal {
            let client = stable_context::require_worker_client(self.global)?;
            let response: Value = client.get_json(path).await?;
            response
                .get("data")
                .cloned()
                .ok_or_else(|| invalid("PR API response is missing data"))
        } else {
            let client = self.public_client();
            client.ensure_auth().await?;
            client.get::<Value>(path).await
        }
    }
}

/// A stored PR's canonical description and how its publications compare,
/// from the same read as the landing's (`g5.git.pr-description`).
pub(super) async fn description(api: &Api<'_>, pull_request_id: &str) -> Result<Value, G5Error> {
    let id = Uuid::parse_str(pull_request_id)
        .map_err(|_| invalid("PR API response has no pull request id"))?;
    api.get_data(&format!(
        "/api/{}/workspaces/{}/pull-requests/{id}/description",
        api.prefix(),
        api.workspace
    ))
    .await
}

/// Most PRs one batch description read accepts (the server's cap).
const DESCRIPTION_BATCH: usize = 100;

/// Several stored PRs' canonical descriptions, keyed by PR id, from the batch
/// read: the same resolver, without provider comparison.
pub(super) async fn descriptions(
    api: &Api<'_>,
    ids: &[String],
) -> Result<std::collections::HashMap<String, Value>, G5Error> {
    let mut read = std::collections::HashMap::new();
    for chunk in ids.chunks(DESCRIPTION_BATCH) {
        let data = api
            .get_data(&format!(
                "/api/{}/workspaces/{}/pull-requests/descriptions?ids={}",
                api.prefix(),
                api.workspace,
                chunk.join(",")
            ))
            .await?;
        for entry in data
            .as_array()
            .ok_or_else(|| invalid("PR description response is not a list"))?
        {
            read.insert(text(entry, "pullRequestId").to_owned(), entry.clone());
        }
    }
    Ok(read)
}

/// How a PR argument names a PR (`g5.git.pr-cli.selectors`).
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Selector<'a> {
    /// No argument: this workspace's outgoing G5 PR.
    Outgoing,
    G5(Uuid),
    /// A G5 PR's name, `PR` and six hex digits in any case (`PR3fa9c1`): its number
    /// (`g5.git.pr-cli.selectors`).
    G5Name(u64),
    /// A GitHub number (`42`, `#42`) or PR URL. A bare number is always GitHub's.
    GitHub(&'a str),
    /// A head branch name.
    Branch(&'a str),
}

pub(super) fn classify(selector: Option<&str>) -> Selector<'_> {
    let Some(selector) = selector.map(str::trim).filter(|s| !s.is_empty()) else {
        return Selector::Outgoing;
    };
    if let Ok(id) = Uuid::parse_str(selector) {
        return Selector::G5(id);
    }
    if let Some(number) = g5_name(selector) {
        return Selector::G5Name(number);
    }
    if selector.contains("://")
        || selector
            .strip_prefix('#')
            .unwrap_or(selector)
            .parse::<u64>()
            .is_ok()
    {
        return Selector::GitHub(selector);
    }
    Selector::Branch(selector)
}

/// The number in a G5 PR's name, `PR` and exactly six hex digits, in any
/// case.
fn g5_name(selector: &str) -> Option<u64> {
    let digits = selector
        .get(..2)
        .filter(|prefix| prefix.eq_ignore_ascii_case("pr"))
        .map(|_| &selector[2..])?;
    if digits.len() != 6 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    u64::from_str_radix(digits, 16).ok().filter(|n| *n > 0)
}

/// A G5 PR's name: its number as six zero-padded lowercase hex digits
/// (`PR3fa9c1`).
pub(super) fn g5_pull_request_name(number: u64) -> String {
    format!("PR{number:06x}")
}

/// The selector the server reads: a G5 PR's name in its canonical form
/// (`PR3fa9c1`), anything else as given.
pub(super) fn server_selector(selector: &str) -> String {
    match classify(Some(selector)) {
        Selector::G5Name(number) => g5_pull_request_name(number),
        _ => selector.trim().to_owned(),
    }
}

/// Whether a PR argument names a G5 PR itself (its ID or `PR######`) rather than
/// a GitHub PR or a branch.
pub(super) fn names_g5_pull_request(selector: &str) -> bool {
    matches!(
        classify(Some(selector)),
        Selector::G5(_) | Selector::G5Name(_)
    )
}

/// The PR a branch heads (`g5.git.pr-cli.selectors`).
#[derive(Debug, PartialEq, Eq)]
pub(super) enum BranchPullRequest {
    /// A GitHub PR: its number and repository.
    GitHub(String, String),
    /// A stored G5 PR with no GitHub PR, from a repository listed from G5's
    /// records (`g5.git.pr-no-github`).
    G5(Uuid),
}

/// A branch's PR, as `gh pr view <branch>` picks it: the open one, else the
/// newest.
pub(super) async fn branch_pull_request(
    api: &Api<'_>,
    branch: &str,
    repo: Option<&str>,
) -> Result<BranchPullRequest, G5Error> {
    let path = format!(
        "/api/{}/workspaces/{}/pull-requests/github",
        api.prefix(),
        api.workspace
    );
    let mut url = reqwest::Url::parse(&format!("http://placeholder{path}")).unwrap();
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("head", branch);
        query.append_pair("state", "all");
        query.append_pair("limit", "10");
        if let Some(repo) = repo {
            query.append_pair("repo", repo);
        }
    }
    let data = api
        .get_data(&format!("{path}?{}", url.query().unwrap_or_default()))
        .await?;
    let pulls = data["pullRequests"].as_array().cloned().unwrap_or_default();
    let pull = pulls
        .iter()
        .find(|p| p["state"] == "OPEN")
        .or_else(|| pulls.first())
        .ok_or_else(|| G5Error::NotFound {
            resource: format!("pull request for branch {branch}"),
        })?;
    // A local repository pull request, or a stored G5 PR without a GitHub
    // PR, names its G5 PR (`g5.git.pr-repository`).
    let local = pull["tracker"] == "g5";
    let Some(number) = pull["number"].as_i64().filter(|_| !local) else {
        return match (pull["source"].as_str(), Uuid::parse_str(text(pull, "id"))) {
            (Some("g5"), Ok(id)) => Ok(BranchPullRequest::G5(id)),
            _ => Err(G5Error::NotFound {
                resource: format!("GitHub pull request for branch {branch}"),
            }),
        };
    };
    let repository = pull["repository"]
        .as_str()
        .or(data["repository"].as_str())
        .unwrap_or_default()
        .to_owned();
    Ok(BranchPullRequest::GitHub(number.to_string(), repository))
}

/// The stored G5 PR this workspace's `view` selects: its outgoing PR, or the
/// G5 PR with this ID among those involving the workspace.
pub(super) async fn stored_pull_request(api: &Api<'_>, id: Option<Uuid>) -> Result<Value, G5Error> {
    let view = PrCommand::View(PrViewArgs {
        pull_request: id.map(|id| id.to_string()),
        repo: None,
        comments: false,
    });
    let path = format!(
        "/api/{}/workspaces/{}/pull-requests/info",
        api.prefix(),
        api.workspace
    );
    let listed = api.get_data(&path).await?;
    Ok(select_output(listed, api.workspace, &view)?["pullRequest"].clone())
}

/// The G5 PR a command that acts on one names (`g5.git.pr-cli.selectors`):
/// a G5 ID or this workspace's outgoing PR from stored records, a `PR######`
/// name through the server's read, and a GitHub number, URL or branch through
/// the stored link of an open G5 PR. None or several linked open G5 PRs
/// refuse.
pub(super) async fn g5_pull_request(
    api: &Api<'_>,
    selector: Option<&str>,
    repo: Option<&str>,
) -> Result<Value, G5Error> {
    let (number, repo) = match classify(selector) {
        Selector::Outgoing => return stored_pull_request(api, None).await,
        Selector::G5(id) => return stored_pull_request(api, Some(id)).await,
        // The server resolves a G5 PR's name in any repository.
        Selector::G5Name(number) => (g5_pull_request_name(number), None),
        Selector::GitHub(number) => (number.to_owned(), repo.map(str::to_owned)),
        Selector::Branch(branch) => match branch_pull_request(api, branch, repo).await? {
            BranchPullRequest::GitHub(number, repository) => {
                (number, Some(repository).filter(|r| !r.is_empty()))
            }
            BranchPullRequest::G5(id) => return stored_pull_request(api, Some(id)).await,
        },
    };
    let path = format!(
        "/api/{}/workspaces/{}/pull-requests/info",
        api.prefix(),
        api.workspace
    );
    let mut url = reqwest::Url::parse(&format!("http://placeholder{path}")).unwrap();
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("selector", &number);
        if let Some(repo) = &repo {
            query.append_pair("repo", repo);
        }
    }
    let data = api
        .get_data(&format!("{path}?{}", url.query().unwrap_or_default()))
        .await?;
    let pull = &data["pullRequest"];
    // A G5 PR's name reads that G5 PR (`g5.git.pr-repository`).
    if data["source"] == "g5" {
        let id = Uuid::parse_str(text(pull, "id"))
            .map_err(|_| invalid("PR read returned a G5 PR without an ID"))?;
        return stored_pull_request(api, Some(id)).await;
    }
    let open: Vec<&Value> = pull["g5PullRequests"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|link| link["status"] == "open")
        .collect();
    let display = format!("{}#{}", text(pull, "repository"), pull["number"]);
    match open.as_slice() {
        [link] => {
            let mut link = (*link).clone();
            link["number"] = pull["number"].clone();
            link["repository"] = pull["repository"].clone();
            Ok(link)
        }
        [] => Err(G5Error::NotFound {
            resource: format!(
                "open G5 pull request linked to GitHub PR {display} that you can read"
            ),
        }),
        _ => Err(invalid(&format!(
            "Several open G5 pull requests link GitHub PR {display}; pass a G5 PR UUID"
        ))),
    }
}

/// Exit status when committed-work publication refuses before any effect.
pub(super) const REFUSED: i32 = 4;

/// One member checkout of this VM's workspace.
struct Checkout {
    root: PathBuf,
    branch: String,
    /// The member's canonical Git URL; the runtime maps it to the checkout's origin.
    git_url: String,
}

/// The member checkouts of the workspace this VM runs
/// (`$G5D_WORKSPACE_ROOT/.g5/layout.json`); `None` outside a workspace VM.
fn member_checkouts() -> Option<Vec<Checkout>> {
    let root = PathBuf::from(std::env::var_os("G5D_WORKSPACE_ROOT")?);
    let layout: Value =
        serde_json::from_slice(&std::fs::read(root.join(".g5/layout.json")).ok()?).ok()?;
    Some(
        layout["repos"]
            .as_array()?
            .iter()
            .filter_map(|repo| {
                Some(Checkout {
                    root: root.join(repo["root_path"].as_str()?),
                    branch: repo["branch"].as_str()?.to_owned(),
                    git_url: repo["git_url"].as_str()?.to_owned(),
                })
            })
            .collect(),
    )
}

/// Publish committed work only (`g5.git.pr-cli.create-save`): refuse when a
/// member checkout has uncommitted changes, then push each checkout's
/// unpushed commits to its workspace branch, fast-forward only. Outside a
/// workspace VM there is no checkout here, and G5 publishes the pushed head.
/// `Ok(Err(message))` is a refusal; nothing was pushed unless it says so.
pub(super) async fn publish_committed_work() -> Result<Result<(), String>, G5Error> {
    let Some(checkouts) = member_checkouts() else {
        return Ok(Ok(()));
    };
    let git = crate::cli::native_git::runner()?;
    let mut uncommitted = Vec::new();
    for checkout in &checkouts {
        let name = checkout.root.display().to_string();
        let changes = git
            .uncommitted_changes(&checkout.root)
            .await
            .map_err(G5Error::from)?;
        uncommitted.extend(
            changes
                .iter()
                .map(|change| format!("  {name}: {} {}", change.status, change.path)),
        );
        let on = git
            .checkout_branch(&checkout.root)
            .await
            .map_err(G5Error::from)?;
        if on.as_deref() != Some(checkout.branch.as_str()) {
            return Ok(Err(format!(
                "{name} is not on its workspace branch {} (it is on {}); check it out first.",
                checkout.branch,
                on.as_deref().unwrap_or("a detached HEAD")
            )));
        }
    }
    if !uncommitted.is_empty() {
        let shown = uncommitted.len().min(20);
        let more = uncommitted.len() - shown;
        return Ok(Err(format!(
            "Uncommitted changes would not be published, so nothing was done. g5d pr publishes \
             committed work only; commit (or discard) these first:\n{}{}",
            uncommitted[..shown].join("\n"),
            if more > 0 {
                format!("\n  … and {more} more")
            } else {
                String::new()
            }
        )));
    }
    let mut pushed_roots = Vec::new();
    for checkout in &checkouts {
        let pushed = git
            .push_branch(
                g5_scm::native::BranchPush {
                    repository: checkout.root.clone(),
                    remote: "origin".into(),
                    expected_url: checkout.git_url.clone(),
                    source: g5_scm::native::PushSource::Branch(checkout.branch.clone()),
                    branch: checkout.branch.clone(),
                    update: g5_scm::native::PushUpdate::FastForward,
                },
                &g5_scm::GitAuth::None,
            )
            .await;
        if let Err(error) = pushed {
            let earlier = if pushed_roots.is_empty() {
                String::new()
            } else {
                format!("\nAlready pushed: {}.", pushed_roots.join(", "))
            };
            return Ok(Err(format!(
                "Could not push {} to its workspace branch: {error}{earlier}\nIf origin moved \
                 ahead, fetch and rebase onto it. If you rebased, publish the rewrite with \
                 `git push --force-with-lease=<branch>:<fetched origin head>` first; g5d only \
                 fast-forwards.",
                checkout.root.display()
            )));
        }
        pushed_roots.push(checkout.root.display().to_string());
    }
    Ok(Ok(()))
}

/// `g5d pr create -B NAME`: the target workspace with this name among the
/// targets the UI offers.
pub(super) async fn target_by_name(api: &Api<'_>, name: &str) -> Result<Uuid, G5Error> {
    let targets = api
        .get_data(&format!(
            "/api/{}/workspaces/{}/pull-requests/targets",
            api.prefix(),
            api.workspace
        ))
        .await?;
    let targets = targets.as_array().cloned().unwrap_or_default();
    targets
        .iter()
        .find(|target| text(target, "name").eq_ignore_ascii_case(name.trim()))
        .and_then(|target| text(target, "id").parse().ok())
        .ok_or_else(|| {
            let names: Vec<_> = targets.iter().map(|t| text(t, "name").to_owned()).collect();
            invalid(&format!(
                "No PR target is named {name}; choose one of: {}",
                if names.is_empty() {
                    "(none)".to_owned()
                } else {
                    names.join(", ")
                }
            ))
        })
}

/// `g5d pr create --dry-run`: what would be published into the target.
pub(super) async fn dry_run(api: &Api<'_>, target: Option<Uuid>) -> Result<Value, G5Error> {
    let target = match target {
        Some(target) => target,
        None => {
            let targets = api
                .get_data(&format!(
                    "/api/{}/workspaces/{}/pull-requests/targets",
                    api.prefix(),
                    api.workspace
                ))
                .await?;
            text(&targets[0], "id")
                .parse()
                .map_err(|_| invalid("No PR target is available; pass -B WORKSPACE"))?
        }
    };
    let body = json!({"targetWorkspaceId": target});
    let preview = if api.internal {
        let client = stable_context::require_worker_client(api.global)?;
        client
            .preview_workspace_pull_request(&api.workspace.to_string(), &body)
            .await?
            .get("data")
            .cloned()
            .ok_or_else(|| invalid("PR API response is missing data"))?
    } else {
        let client = api.public_client();
        client.ensure_auth().await?;
        client
            .post::<_, Value>(
                &format!("/api/v1/workspaces/{}/pull-requests/preview", api.workspace),
                &body,
            )
            .await?
    };
    Ok(json!({"targetWorkspaceId": target, "title": preview["title"], "body": preview["body"]}))
}

/// `g5d pr close`: close the G5 PR, after commenting on its GitHub PRs when
/// asked. Returns the closed PR.
pub(super) async fn close(api: &Api<'_>, args: &crate::cli::PrCloseArgs) -> Result<Value, G5Error> {
    let pull = g5_pull_request(api, args.pull_request.as_deref(), None).await?;
    let id = text(&pull, "id").to_owned();
    // Only an open G5 PR closes: refuse before commenting on its GitHub PRs.
    if let Some(status) = pull["status"].as_str().filter(|status| *status != "open") {
        let name = pull["number"].as_str().unwrap_or(&id);
        return Err(invalid(&format!(
            "G5 PR {name} is {status}, not open; nothing was done"
        )));
    }
    if let Some(comment) = args.comment.as_deref().filter(|c| !c.trim().is_empty()) {
        // The stored PR names each of its GitHub PRs; a G5 PR found through a
        // GitHub selector names at least that one.
        let stored = stored_pull_request(api, id.parse().ok()).await.ok();
        let mut targets: Vec<(Value, Value)> = stored
            .iter()
            .flat_map(crate::cli::pr_json::github_publications)
            .filter(|r| r["number"].is_i64())
            .map(|r| (r["number"].clone(), r["repository"].clone()))
            .collect();
        if targets.is_empty() && pull["number"].is_i64() {
            targets.push((pull["number"].clone(), pull["repository"].clone()));
        }
        for (number, repo) in targets {
            let body = json!({
                "selector": number.to_string(), "repo": repo, "body": comment,
                "idempotencyKey": Uuid::new_v4().to_string(),
            });
            if api.internal {
                let client = stable_context::require_worker_client(api.global)?;
                client
                    .post_pull_request_comment(&api.workspace.to_string(), &body)
                    .await?;
            } else {
                let client = api.public_client();
                client.ensure_auth().await?;
                client
                    .post::<_, Value>(
                        &format!(
                            "/api/v1/workspaces/{}/pull-requests/github/comments",
                            api.workspace
                        ),
                        &body,
                    )
                    .await?;
            }
        }
    }
    let closed = if api.internal {
        let client = stable_context::require_worker_client(api.global)?;
        client
            .workspace_agent_post(
                &api.workspace.to_string(),
                &format!("pull-requests/{id}/close"),
                &json!({}),
                false,
            )
            .await?
            .get("data")
            .cloned()
            .ok_or_else(|| invalid("PR API response is missing data"))?
    } else {
        let client = api.public_client();
        client.ensure_auth().await?;
        client
            .post::<_, Value>(
                &format!(
                    "/api/v1/workspaces/{}/pull-requests/{id}/close",
                    api.workspace
                ),
                &json!({}),
            )
            .await?
    };
    Ok(closed.get("pullRequest").cloned().unwrap_or(closed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn selectors_name_g5_ids_github_prs_or_branches() {
        let id = Uuid::new_v4();
        assert_eq!(classify(None), Selector::Outgoing);
        assert_eq!(classify(Some("  ")), Selector::Outgoing);
        assert_eq!(classify(Some(&id.to_string())), Selector::G5(id));
        assert_eq!(classify(Some("42")), Selector::GitHub("42"));
        assert_eq!(classify(Some("#42")), Selector::GitHub("#42"));
        assert_eq!(
            classify(Some("https://github.com/acme/api/pull/42")),
            Selector::GitHub("https://github.com/acme/api/pull/42")
        );
        assert_eq!(
            classify(Some("issue-816-landing")),
            Selector::Branch("issue-816-landing")
        );
        assert_eq!(classify(Some("42abc")), Selector::Branch("42abc"));
        // A G5 PR's name is `PR` and six hex digits, in any case; a bare
        // number stays GitHub's, and anything shorter is a branch.
        assert_eq!(classify(Some("PR3fa9c1")), Selector::G5Name(0x3f_a9c1));
        assert_eq!(classify(Some(" pr3FA9C1 ")), Selector::G5Name(0x3f_a9c1));
        assert_eq!(classify(Some("PR000012")), Selector::G5Name(0x12));
        for branch in [
            "PR",
            "PR0",
            "PR12",
            "PR000000",
            "PR3fa9c10",
            "PR-3fa9c",
            "PR3fa9cg",
            "prefab",
            "pr 3fa9c1",
            "PRs",
            "#PR3fa9c1",
        ] {
            assert_eq!(classify(Some(branch)), Selector::Branch(branch), "{branch}");
        }
        assert_eq!(server_selector("pR3FA9C1"), "PR3fa9c1");
        assert_eq!(server_selector("42"), "42");
        assert!(names_g5_pull_request("PR3fa9c1") && names_g5_pull_request(&id.to_string()));
        assert!(!names_g5_pull_request("7") && !names_g5_pull_request("feature"));
    }

    fn repo(dir: &Path, args: &[&str]) {
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .env_remove("GIT_AUTHOR_NAME")
            .env_remove("GIT_AUTHOR_EMAIL")
            .env_remove("GIT_COMMITTER_NAME")
            .env_remove("GIT_COMMITTER_EMAIL")
            .output()
            .unwrap();
        assert!(
            status.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&status.stderr)
        );
    }

    #[test]
    #[serial_test::serial(g5d_workspace_root_env)]
    fn committed_work_is_pushed_and_uncommitted_work_refuses() {
        let tmp = tempfile::tempdir().unwrap();
        let remote = tmp.path().join("remote.git");
        let work = tmp.path().join("work");
        std::fs::create_dir_all(work.join(".g5")).unwrap();
        repo(
            tmp.path(),
            &["init", "--bare", "-q", remote.to_str().unwrap()],
        );
        repo(
            tmp.path(),
            &[
                "init",
                "-q",
                "-b",
                "feature",
                work.join("code").to_str().unwrap(),
            ],
        );
        let code = work.join("code");
        repo(&code, &["config", "user.email", "a@example.com"]);
        repo(&code, &["config", "user.name", "A"]);
        repo(
            &code,
            &["remote", "add", "origin", remote.to_str().unwrap()],
        );
        std::fs::write(code.join("a.txt"), "1").unwrap();
        repo(&code, &["add", "a.txt"]);
        repo(&code, &["commit", "-q", "-m", "one"]);
        std::fs::write(
            work.join(".g5/layout.json"),
            json!({"repos":[{"root_path":"code","branch":"feature","git_url":remote.to_str().unwrap()}]})
                .to_string(),
        )
        .unwrap();
        // A workspace VM's runtime Git configuration would map the remote;
        // this fixture uses a plain local one.
        let socket = std::env::var_os(g5_contracts::workspace_git::SOCKET_ENV);
        // SAFETY: serialized with every test that reads these variables.
        unsafe {
            std::env::set_var("G5D_WORKSPACE_ROOT", &work);
            std::env::remove_var(g5_contracts::workspace_git::SOCKET_ENV);
        }
        let publish = || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(publish_committed_work())
                .unwrap()
        };

        // Unpushed commits are pushed, fast-forward.
        assert_eq!(publish(), Ok(()));
        let pushed = std::process::Command::new("git")
            .args([
                "--git-dir",
                remote.to_str().unwrap(),
                "rev-parse",
                "feature",
            ])
            .output()
            .unwrap();
        assert!(pushed.status.success());

        // Uncommitted and untracked changes refuse before any push.
        std::fs::write(code.join("a.txt"), "2").unwrap();
        std::fs::write(code.join("new.txt"), "x").unwrap();
        let refused = publish().unwrap_err();
        assert!(
            refused.contains("a.txt") && refused.contains("new.txt"),
            "{refused}"
        );
        assert!(refused.contains("commit (or discard)"), "{refused}");

        // A rewritten branch is not force-pushed.
        repo(&code, &["checkout", "-q", "--", "a.txt"]);
        std::fs::remove_file(code.join("new.txt")).unwrap();
        repo(&code, &["commit", "-q", "--amend", "-m", "rewritten"]);
        let refused = publish().unwrap_err();
        assert!(refused.contains("--force-with-lease"), "{refused}");

        // A checkout off its workspace branch refuses.
        repo(&code, &["checkout", "-q", "-b", "other"]);
        let refused = publish().unwrap_err();
        assert!(
            refused.contains("not on its workspace branch feature"),
            "{refused}"
        );
        unsafe {
            std::env::remove_var("G5D_WORKSPACE_ROOT");
            if let Some(socket) = socket {
                std::env::set_var(g5_contracts::workspace_git::SOCKET_ENV, socket);
            }
        }
    }
}
