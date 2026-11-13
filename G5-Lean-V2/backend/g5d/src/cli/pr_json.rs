//! Field-selected PR output. The server's read API and its provenance envelope
//! remain unchanged; only the CLI's public JSON representation is projected.
use crate::cli::{PrCommand, PrViewArgs};
use g5_contracts::G5Error;
use serde_json::{json, Value};

const CHECKS: &[&str] = &[
    "bucket",
    "completedAt",
    "description",
    "event",
    "link",
    "name",
    "startedAt",
    "state",
    "workflow",
    "id",
    "kind",
    "repository",
    "number",
    "headRefOid",
    "details",
    "commitOid",
    "revision",
    "required",
];

/// `g5d pr comment`: the recorded submission.
const COMMENT: &[&str] = &[
    "id",
    "state",
    "url",
    "destination",
    "threadId",
    "repository",
    "number",
    "author",
    "body",
    "origin",
    "idempotencyKey",
    "createdAt",
    "publishedAt",
    "error",
];
/// `g5d pr resolve`.
const RESOLVE: &[&str] = &["threadId", "repository", "number", "isResolved", "changed"];

const COMMON: &[&str] = &[
    "id",
    "source",
    "title",
    "body",
    "state",
    "number",
    "url",
    "repository",
    "headRefName",
    "headRefOid",
    "baseRefName",
    "baseRefOid",
    "createdAt",
    "updatedAt",
    "mergedAt",
];
const GITHUB: &[&str] = &[
    "author",
    "isDraft",
    "mergeable",
    "mergeStateStatus",
    "additions",
    "deletions",
    "changedFiles",
    "commentCount",
    "reviewCommentCount",
    "comments",
    "commentsHasMore",
    "reviewThreads",
    "reviewThreadsHasMore",
    "reviews",
    "reviewsHasMore",
    "reviewDecision",
    "latestReviews",
    "latestReviewsHasMore",
    "g5PullRequests",
];
/// `g5d pr list` in repository mode (`g5.git.pr-list`): what one listing
/// returns per PR, in `view`'s names.
const LIST_REPOSITORY: &[&str] = &[
    "source",
    "id",
    "number",
    "title",
    "body",
    "state",
    "url",
    "repository",
    "author",
    "isDraft",
    "headRefName",
    "headRefOid",
    "baseRefName",
    "createdAt",
    "updatedAt",
    "mergedAt",
    "closedAt",
    "labels",
    "g5PullRequests",
    "g5Number",
];
const G5: &[&str] = &[
    "sourceWorkspaceId",
    "sourceWorkspaceName",
    "targetWorkspaceId",
    "targetWorkspaceName",
    "status",
    "localMergeStatus",
    "providerStatus",
    "providerRepos",
    "descriptionOverride",
    "sourceHeads",
    "targetHeads",
    "preflight",
    "lastPreflightFingerprint",
    "blockingMessage",
    "mergeEvidence",
    "sourceAutoPush",
    "autoMergeRequest",
    "mergedBy",
    "mergeOverride",
    "g5Number",
    "repositoryPullRequests",
    "sourceWorkspaceDeleted",
    "targetWorkspaceDeleted",
];

pub(crate) enum Selection {
    Human,
    Available(Vec<&'static str>),
    Fields(Vec<String>),
}

impl Selection {
    pub fn parse(raw: Option<Option<&str>>, command: &PrCommand) -> Result<Self, G5Error> {
        let Some(raw) = raw else {
            return Ok(Self::Human);
        };
        let github = matches!(command, PrCommand::View(PrViewArgs { pull_request: Some(selector), .. })
            if !crate::cli::pr::names_g5_pull_request(selector));
        let mut available: Vec<_> = COMMON
            .iter()
            .chain(if github { GITHUB } else { G5 })
            .copied()
            .collect();
        let own = match command {
            PrCommand::Checks(_) => Some(CHECKS),
            PrCommand::Comment(_) => Some(COMMENT),
            PrCommand::Resolve(_) => Some(RESOLVE),
            PrCommand::List(args) if args.repository_mode() => Some(LIST_REPOSITORY),
            _ => None,
        };
        let listing = own == Some(LIST_REPOSITORY);
        if let Some(own) = own {
            available = own.to_vec();
        }
        if matches!(
            command,
            PrCommand::Create(_) | PrCommand::Update(_) | PrCommand::Edit(_)
        ) {
            available.push("publication");
        }
        available.sort_unstable();
        let Some(raw) = raw else {
            return Ok(Self::Available(available));
        };
        let mut fields = Vec::new();
        for field in raw.split(',').map(str::trim) {
            if listing
                && !available.contains(&field)
                && (COMMON.contains(&field) || GITHUB.contains(&field))
            {
                return Err(G5Error::ValidationError {
                    details: json!({
                        "message": format!("JSON field {field:?} needs one PR's full read; list the PRs, then use `g5d pr view <number|url> --json {field}`")
                    }),
                });
            }
            if !github && own.is_none() && !available.contains(&field) && GITHUB.contains(&field) {
                return Err(G5Error::ValidationError {
                    details: json!({
                        "message": format!("JSON field {field:?} is only available for a live GitHub PR; stored G5 PRs do not carry it. Use `g5d pr view <number|url> --json {field}`")
                    }),
                });
            }
            if !available.contains(&field) {
                return Err(G5Error::ValidationError {
                    details: json!({
                        "message": format!("Unknown JSON field {field:?}. Available fields: {}", available.join(", "))
                    }),
                });
            }
            if !fields.iter().any(|existing| existing == field) {
                fields.push(field.to_owned());
            }
        }
        Ok(Self::Fields(fields))
    }

    pub fn print_available_fields(&self) -> bool {
        if let Self::Available(fields) = self {
            println!("Available fields:\n  {}", fields.join("\n  "));
            true
        } else {
            false
        }
    }

    pub fn needs_comments(&self) -> bool {
        matches!(self, Self::Fields(fields) if fields.iter().any(|field| matches!(field.as_str(), "comments" | "commentsHasMore" | "reviewThreads" | "reviewThreadsHasMore" | "reviews" | "reviewsHasMore")))
    }

    pub fn needs_reviews(&self) -> bool {
        matches!(self, Self::Fields(fields) if fields.iter().any(|field| matches!(field.as_str(), "reviewDecision" | "latestReviews" | "latestReviewsHasMore")))
    }

    /// Whether `body` is among the selected fields.
    pub fn selects_body(&self) -> bool {
        matches!(self, Self::Fields(fields) if fields.iter().any(|f| f == "body"))
    }

    /// Whether the output shows a stored PR's description: text output, or
    /// `body` in the selected fields.
    pub fn shows_description(&self) -> bool {
        match self {
            Self::Human => true,
            Self::Available(_) => false,
            Self::Fields(fields) => fields.iter().any(|f| f == "body"),
        }
    }

    pub fn needs_details(&self) -> bool {
        matches!(self, Self::Fields(fields) if fields.iter().any(|f| f == "details"))
    }

    pub fn project(&self, envelope: &Value) -> Option<Value> {
        let Self::Fields(fields) = self else {
            return None;
        };
        // Repository listing rows already carry view's names and their own source.
        let listing = envelope["mode"] == "repository";
        let project_one = |pull: &Value| -> Value {
            fields
                .iter()
                .map(|field| {
                    let value = if listing {
                        pull[field].clone()
                    } else {
                        field_value(pull, &envelope["source"], field)
                    };
                    (field.clone(), value)
                })
                .collect()
        };
        if let Some(result) = envelope.get("result") {
            return Some(
                fields
                    .iter()
                    .map(|field| (field.clone(), result[field].clone()))
                    .collect(),
            );
        }
        if let Some(checks) = envelope["checks"].as_array() {
            return Some(Value::Array(
                checks
                    .iter()
                    .map(|check| {
                        fields
                            .iter()
                            .map(|field| (field.clone(), check[field].clone()))
                            .collect()
                    })
                    .collect(),
            ));
        }
        Some(if let Some(pull) = envelope.get("pullRequest") {
            project_one(pull)
        } else {
            Value::Array(
                envelope["pullRequests"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(project_one)
                    .collect(),
            )
        })
    }
}

/// gh's `mergeable` value for GitHub's REST boolean; null is still computing.
pub(crate) fn mergeable(value: &Value) -> &'static str {
    match value.as_bool() {
        Some(true) => "MERGEABLE",
        Some(false) => "CONFLICTING",
        None => "UNKNOWN",
    }
}

/// gh's `mergeStateStatus` value for GitHub's REST `mergeable_state`.
pub(crate) fn merge_state(value: &Value) -> String {
    let state = value.as_str().unwrap_or("unknown").to_ascii_uppercase();
    match state.as_str() {
        "BEHIND" | "BLOCKED" | "CLEAN" | "DIRTY" | "DRAFT" | "HAS_HOOKS" | "UNSTABLE" => state,
        _ => "UNKNOWN".into(),
    }
}

/// A stored G5 PR's recorded GitHub publications, one per GitHub repository
/// pull request (`g5.git.pr-adapter`), in one shape: `repoId`, `repository`,
/// `number`, `url`, `providerStatus`, `lastError`, `headRefName`,
/// `baseRefName`, `providerHeadSha`, `providerBaseSha`. They come from the
/// adapter's `repositoryPullRequests`; only a server that predates repository
/// pull requests sends just `providerRepos`, read the same way.
pub(crate) fn github_publications(pull: &Value) -> Vec<Value> {
    if let Some(repositories) = pull["repositoryPullRequests"].as_array() {
        return repositories
            .iter()
            // Only the GitHub adapter's rows carry its record.
            .filter(|r| r["github"].is_object())
            .map(|r| {
                let record = &r["github"];
                json!({
                    "repoId": r["repoId"], "repository": r["repository"],
                    "number": r["number"], "url": r["url"],
                    "providerStatus": record["providerStatus"], "lastError": record["lastError"],
                    "headRefName": r["headRefName"], "baseRefName": r["baseRefName"],
                    "providerHeadSha": record["providerHeadSha"],
                    "providerBaseSha": record["providerBaseSha"],
                })
            })
            .collect();
    }
    pull["providerRepos"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|r| {
            json!({
                "repoId": r["repoId"], "repository": r["providerRepoFullName"],
                "number": r["providerPrNumber"], "url": r["providerPrUrl"],
                "providerStatus": r["providerStatus"], "lastError": r["lastError"],
                "headRefName": r["sourceRemoteBranch"], "baseRefName": r["targetRemoteBranch"],
                "providerHeadSha": r["providerHeadSha"], "providerBaseSha": r["providerBaseSha"],
            })
        })
        .collect()
}

fn field_value(pull: &Value, source: &Value, field: &str) -> Value {
    if field == "source" {
        return source.clone();
    }
    if source == "github" {
        return match field {
            "mergeable" => json!(mergeable(&pull[field])),
            "mergeStateStatus" => json!(merge_state(&pull[field])),
            _ => pull[field].clone(),
        };
    }
    match field {
        "state" => pull["status"]
            .as_str()
            .map(|s| json!(s.to_ascii_uppercase()))
            .unwrap_or(Value::Null),
        // The canonical description, from the read the landing uses
        // (`g5.git.pr-description`); attached only when `body` is selected.
        "body" => pull[crate::cli::pr::DESCRIPTION_READ]["description"].clone(),
        // The G5 PR's name, `PR3fa9c1`, which also names its local repository
        // pull requests (`g5.git.pr-local-number`); `number` stays GitHub's.
        "g5Number" => pull["number"].clone(),
        "number" | "url" | "repository" | "headRefName" | "headRefOid" | "baseRefName"
        | "baseRefOid" => {
            // A G5 PR may cover several provider PRs. Never choose one arbitrarily.
            let repos = github_publications(pull);
            let [repo] = repos.as_slice() else {
                return Value::Null;
            };
            let key = match field {
                "headRefOid" => "providerHeadSha",
                "baseRefOid" => "providerBaseSha",
                other => other,
            };
            repo[key].clone()
        }
        _ => pull[field].clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn view(selector: Option<&str>) -> PrCommand {
        PrCommand::View(PrViewArgs {
            pull_request: selector.map(str::to_owned),
            repo: None,
            comments: false,
        })
    }
    #[test]
    fn conversation_commands_select_their_own_fields() {
        let comment = PrCommand::Comment(crate::cli::PrCommentArgs {
            pull_request: Some("42".into()),
            repo: None,
            body: Some("x".into()),
            body_file: None,
            reply_to: None,
            idempotency_key: None,
        });
        let selection = Selection::parse(Some(Some("url,state,threadId")), &comment).unwrap();
        let result =
            json!({"result": {"url": "u", "state": "published", "threadId": null, "body": "x"}});
        assert_eq!(
            selection.project(&result).unwrap(),
            json!({"url": "u", "state": "published", "threadId": null})
        );
        // PR fields belong to view, not comment.
        assert!(Selection::parse(Some(Some("title")), &comment).is_err());
        let resolve = PrCommand::Resolve(crate::cli::PrResolveArgs {
            thread: "PRRT_1".into(),
            repo: None,
            unresolve: false,
        });
        assert!(matches!(
            Selection::parse(Some(None), &resolve).unwrap(),
            Selection::Available(fields) if fields.contains(&"isResolved") && !fields.contains(&"url")
        ));
    }
    #[test]
    fn github_fields_have_plain_shapes_and_gh_mergeability_values() {
        let selection = Selection::parse(
            Some(Some(
                "title,mergeable,mergeStateStatus,author,comments,title",
            )),
            &view(Some("42")),
        )
        .unwrap();
        assert!(selection.needs_comments());
        for field in [
            "reviewThreads",
            "reviewThreadsHasMore",
            "reviews",
            "reviewsHasMore",
        ] {
            let selection = Selection::parse(Some(Some(field)), &view(Some("42"))).unwrap();
            assert!(selection.needs_comments(), "{field}");
        }
        for field in ["reviewDecision", "latestReviews", "latestReviewsHasMore"] {
            let selection = Selection::parse(Some(Some(field)), &view(Some("42"))).unwrap();
            assert!(
                selection.needs_reviews() && !selection.needs_comments(),
                "{field}"
            );
        }
        // Stored PRs name the live command instead of only listing fields.
        let error = Selection::parse(Some(Some("title,comments")), &view(None))
            .err()
            .unwrap()
            .to_string();
        assert!(
            error.contains("g5d pr view <number|url> --json comments"),
            "{error}"
        );
        let error = Selection::parse(Some(Some("typo")), &view(None))
            .err()
            .unwrap()
            .to_string();
        assert!(error.contains("Available fields"), "{error}");
        for (raw, expected) in [
            (json!(true), "MERGEABLE"),
            (json!(false), "CONFLICTING"),
            (Value::Null, "UNKNOWN"),
        ] {
            assert_eq!(selection.project(&json!({"source":"github","pullRequest":{"title":"Fix", "mergeable":raw,"mergeStateStatus":"clean","author":{"login":"dev"}}})).unwrap(),
                json!({"title":"Fix","mergeable":expected,"mergeStateStatus":"CLEAN","author":{"login":"dev"},"comments":null}));
        }
    }
    #[test]
    fn github_fields_come_from_the_adapters_repository_pull_requests() {
        // g5.git.pr-adapter: a G5 PR's GitHub fields come from its GitHub
        // repository pull request, never a local or unpublished one.
        let pull = json!({"id":"g5","status":"open","number":"PR000012",
            "providerRepos":[{"providerPrNumber":999,"providerRepoFullName":"stale/raw"}],
            "repositoryPullRequests":[
                {"repoId":"a","repository":"acme/api","adapter":"github","number":42,
                 "url":"https://github.com/acme/api/pull/42","headRefName":"feature","baseRefName":"main",
                 "github":{"providerStatus":"open","lastError":null,"providerHeadSha":"h","providerBaseSha":"b"}},
                {"repoId":"d","repository":"docs","adapter":"g5","number":null},
                {"repoId":"w","repository":"acme/web","adapter":"github","number":null}]});
        let publications = github_publications(&pull);
        assert_eq!(publications.len(), 1);
        assert_eq!(publications[0]["repository"], "acme/api");
        let source = json!("g5");
        assert_eq!(field_value(&pull, &source, "number"), 42);
        assert_eq!(field_value(&pull, &source, "headRefOid"), "h");
        assert_eq!(field_value(&pull, &source, "headRefName"), "feature");
        assert_eq!(field_value(&pull, &source, "g5Number"), "PR000012");
        // Only a server without repository pull requests is read from its raw records.
        let legacy =
            json!({"providerRepos":[{"providerPrNumber":7,"providerRepoFullName":"acme/api"}]});
        assert_eq!(field_value(&legacy, &source, "number"), 7);
    }

    #[test]
    fn workspace_aliases_preserve_unknown_and_multi_repository_state() {
        let selection = Selection::parse(
            Some(Some("source,id,state,body,number,url,headRefOid")),
            &view(None),
        )
        .unwrap();
        let mut pull = json!({"id":"id","status":"open","descriptionOverride":null});
        let project = |p: Value| {
            selection
                .project(&json!({"source":"g5","pullRequest":p}))
                .unwrap()
        };
        let expected = json!({"source":"g5","id":"id","state":"OPEN","body":null,"number":null,"url":null,"headRefOid":null});
        assert_eq!(project(pull.clone()), expected);
        pull["providerRepos"] = json!([{"providerPrNumber":42,"providerPrUrl":"https://github.com/a/b/pull/42","providerHeadSha":"abc"}]);
        assert_eq!(project(pull.clone())["number"], 42);
        assert_eq!(project(pull.clone())["headRefOid"], "abc");
        pull["providerRepos"]
            .as_array_mut()
            .unwrap()
            .push(json!({"providerPrNumber":43}));
        assert_eq!(project(pull), expected);
    }
    #[test]
    fn list_and_status_are_arrays_and_fields_are_validated_by_source() {
        for command in [
            PrCommand::Status,
            PrCommand::List(crate::cli::PrListArgs {
                state: "open".into(),
                limit: 30,
                ..Default::default()
            }),
        ] {
            let selection = Selection::parse(Some(Some("title,state")), &command).unwrap();
            assert_eq!(
                selection.project(&json!({"source":"g5","pullRequests":[],"hasMore":false})),
                Some(json!([]))
            );
            assert_eq!(selection.project(&json!({"source":"g5","pullRequests":[{"title":"A","status":"closed"}],"hasMore":true})), Some(json!([{"title":"A","state":"CLOSED"}])));
        }
        for raw in ["", "title,,state", "title,", "unknown", " ", "comments"] {
            assert!(
                Selection::parse(Some(Some(raw)), &view(None)).is_err(),
                "{raw}"
            );
        }
        assert!(Selection::parse(Some(Some("id")), &view(Some("42"))).is_ok());
        // `PR######` names a G5 PR: G5 fields, not GitHub's.
        assert!(Selection::parse(Some(Some("g5Number,status")), &view(Some("PR3fa9c1"))).is_ok());
        assert!(Selection::parse(Some(Some("comments")), &view(Some("pr3fa9c1"))).is_err());
        assert!(Selection::parse(Some(Some("g5Number")), &view(Some("7"))).is_err());
        assert!(
            matches!(Selection::parse(Some(None), &view(Some("42"))).unwrap(), Selection::Available(fields) if fields.contains(&"author") && fields.contains(&"id"))
        );
        assert!(matches!(
            Selection::parse(None, &view(None)).unwrap(),
            Selection::Human
        ));
    }
}

#[cfg(test)]
mod parser_tests {
    use crate::cli::pr_json::Selection;
    use crate::cli::{Cli, Command, PrCommand};
    use clap::{CommandFactory, Parser};

    #[test]
    fn gh_output_flags_fail_with_guidance() {
        let hint = |args: &[&str]| {
            let error = Cli::try_parse_from(args).unwrap_err();
            let args: Vec<_> = args.iter().map(std::ffi::OsString::from).collect();
            crate::cli::gh_habit_hint(&args, error.kind())
        };
        for args in [
            &["g5d", "pr", "view", "42", "--json", "title", "-q", ".title"][..],
            &[
                "g5d", "pr", "view", "42", "--json", "title", "--jq", ".title",
            ],
            &["g5d", "pr", "list", "--json", "title", "--jq=.[0]"],
            &["g5d", "pr", "view", "42", "--template", "{{.title}}"],
        ] {
            assert!(hint(args).unwrap().contains("| jq"), "{args:?}");
        }
        assert!(hint(&["g5d", "pr", "view", "42", "--web"])
            .unwrap()
            .contains("--json url"));
        // Ordinary usage errors and `-q` as --quiet are unaffected.
        assert!(hint(&["g5d", "pr", "view", "42", "--typo"]).is_none());
        assert!(hint(&["g5d", "pr", "view", "1", "2"]).is_none());
        assert!(Cli::try_parse_from(["g5d", "pr", "view", "-q", "42"]).is_ok());
    }

    #[test]
    fn gh_pr_verbs_and_flags_g5_lacks_fail_with_guidance() {
        let hint = |args: &[&str]| {
            let error = Cli::try_parse_from(args).unwrap_err();
            let args: Vec<_> = args.iter().map(std::ffi::OsString::from).collect();
            crate::cli::gh_habit_hint(&args, error.kind())
        };
        for (args, expected) in [
            (
                &["g5d", "pr", "review", "42", "--approve"][..],
                "never by an agent",
            ),
            (&["g5d", "pr", "ready", "42"], "draft"),
            (&["g5d", "pr", "checkout", "42"], "git fetch"),
            (&["g5d", "pr", "reopen", "42"], "g5d pr create"),
            (
                &["g5d", "pr", "update-branch", "--rebase"],
                "--force-with-lease",
            ),
            (&["g5d", "pr", "diff"], "git diff"),
            (&["g5d", "pr", "create", "--draft"], "draft"),
            (&["g5d", "pr", "create", "--reviewer", "octo"], "on GitHub"),
            (&["g5d", "pr", "create", "-H", "feature"], "-B WORKSPACE"),
            (&["g5d", "pr", "edit", "--add-label", "bug"], "on GitHub"),
            (&["g5d", "pr", "edit", "--base", "main"], "keeps its target"),
            (&["g5d", "pr", "merge", "--squash"], "rebase only"),
            (
                &["g5d", "pr", "merge", "--delete-branch"],
                "keeps the workspace's branch",
            ),
            (&["g5d", "pr", "merge", "--admin"], "no admin bypass"),
        ] {
            let found = hint(args).unwrap_or_else(|| panic!("no hint for {args:?}"));
            assert!(found.contains(expected), "{args:?}: {found}");
        }
        // gh's merge flags that G5 honors parse.
        assert!(Cli::try_parse_from(["g5d", "pr", "merge", "--rebase"]).is_ok());
        assert!(Cli::try_parse_from(["g5d", "pr", "merge", "42", "-r", "--auto"]).is_ok());
    }

    #[test]
    fn edit_close_and_a_hidden_update_follow_gh() {
        let pr = |args: &[&str]| {
            let Command::Pr(pr) = Cli::try_parse_from(args).unwrap().command else {
                panic!()
            };
            pr.command
        };
        let PrCommand::Edit(edit) = pr(&["g5d", "pr", "edit", "issue-816", "-t", "New", "-F", "-"])
        else {
            panic!()
        };
        assert_eq!(edit.pull_request.as_deref(), Some("issue-816"));
        assert_eq!(edit.title.as_deref(), Some("New"));
        assert!(Cli::try_parse_from(["g5d", "pr", "edit", "-b", "x", "-F", "f"]).is_err());
        let PrCommand::Close(close) = pr(&["g5d", "pr", "close", "42", "-c", "Superseded"]) else {
            panic!()
        };
        assert_eq!(close.comment.as_deref(), Some("Superseded"));
        let PrCommand::Create(create) = pr(&["g5d", "pr", "create", "-B", "main", "--dry-run"])
        else {
            panic!()
        };
        assert_eq!(create.base.as_deref(), Some("main"));
        assert!(create.dry_run);
        assert!(Cli::try_parse_from([
            "g5d",
            "pr",
            "create",
            "-B",
            "main",
            "--target-workspace-id",
            "00000000-0000-0000-0000-000000000000",
        ])
        .is_err());
        let PrCommand::Checks(checks) = pr(&["g5d", "pr", "checks", "--required"]) else {
            panic!()
        };
        assert!(checks.required);
        // `update` still parses but is hidden from `g5d pr --help`.
        assert!(matches!(pr(&["g5d", "pr", "update"]), PrCommand::Update(_)));
        let mut command = Cli::command();
        let help = command
            .find_subcommand_mut("pr")
            .unwrap()
            .render_help()
            .to_string();
        assert!(!help.contains("\n  update"), "{help}");
        assert!(
            help.contains("\n  edit") && help.contains("\n  close"),
            "{help}"
        );
        assert!(help.contains("git push && g5d pr checks"), "{help}");
        // Edit and update with text report their publication.
        let fields = Selection::parse(Some(None), &pr(&["g5d", "pr", "edit", "-t", "x"])).unwrap();
        assert!(
            matches!(fields, Selection::Available(f) if f.contains(&"publication") && f.contains(&"sourceAutoPush"))
        );
    }

    #[test]
    fn command_tree_and_pr_json_positions_are_valid() {
        Cli::command().debug_assert();
        for args in [
            vec!["g5d", "pr", "view", "42", "--json", "title,url"],
            vec!["g5d", "pr", "view", "--json=title,url", "42"],
            vec!["g5d", "pr", "--json", "title,url", "view", "42"],
            vec!["g5d", "pr", "view", "42", "--json"],
            vec!["g5d", "pr", "--json", "view", "42"],
        ] {
            let cli = Cli::try_parse_from(&args).unwrap();
            assert!(!cli.global.json, "{args:?}");
            let Command::Pr(pr) = cli.command else {
                panic!()
            };
            assert!(pr.json_fields.is_some());
            assert!(
                matches!(pr.command, crate::cli::PrCommand::View(view) if view.pull_request.as_deref() == Some("42"))
            );
        }
        let cli = Cli::try_parse_from(["g5d", "--json", "pr", "status"]).unwrap();
        assert!(cli.global.json);
    }

    #[test]
    fn other_commands_keep_boolean_json_and_positionals() {
        for args in [
            vec!["g5d", "--json", "doctor"],
            vec!["g5d", "doctor", "--json"],
            vec!["g5d", "--json", "issue", "list"],
        ] {
            let cli = Cli::try_parse_from(&args).unwrap();
            assert!(cli.global.json, "{args:?}");
        }
        // `g5d issue` takes gh's optional field list like `g5d pr`; its
        // positions are covered in `cli::issue`.
        assert!(
            !Cli::try_parse_from(["g5d", "issue", "list", "--json"])
                .unwrap()
                .global
                .json
        );
    }
}
