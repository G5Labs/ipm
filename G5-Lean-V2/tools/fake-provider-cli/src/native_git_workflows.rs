//! Deterministic provider actions against the same native Git boundary used by
//! real agents. The task self-context owns the destination ref; the checked-out
//! local task branch is not necessarily a ref the worker may publish.

use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use base64::Engine as _;
use serde::Deserialize;
use serde_json::Value;

use super::{boundary, canonical_base64, directive_tokens, run_task_complete, WorkflowReply};

const GIT_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RepeatedCommit {
    mode: String,
    relative_path: String,
    commits: Vec<Commit>,
    completion_message: Option<String>,
    #[serde(default)]
    measure_timings: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Commit {
    content: String,
    message: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BudgetResume {
    relative_path: String,
    checkpoint_content: String,
    completed_content: String,
    checkpoint_message: String,
    completion_commit_message: String,
    completion_message: String,
    expected_max_turns: u64,
}

fn payload(prompt: &str, prefix: &str) -> Result<Option<Value>, String> {
    let count = prompt.matches(prefix).count();
    if count == 0 {
        return Ok(None);
    }
    let tokens = directive_tokens(prompt, prefix);
    if tokens.len() != count || tokens.iter().any(|token| *token != tokens[0]) {
        return Err("native Git workflow requires identical, well-formed directives".into());
    }
    let bytes = canonical_base64(tokens[0])
        .ok_or("native Git workflow payload must be canonical base64")?;
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|err| format!("native Git workflow requires a JSON payload: {err}"))
}

fn run(binary: &str, args: &[&str], cwd: &Path) -> Result<Output, String> {
    run_with_env(binary, args, cwd, &[])
}

fn run_with_env(
    binary: &str,
    args: &[&str],
    cwd: &Path,
    env: &[(String, String)],
) -> Result<Output, String> {
    let mut child = Command::new(binary)
        .args(args)
        .current_dir(cwd)
        .envs(env.iter().cloned())
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_EDITOR", "true")
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("native Git workflow could not start {binary}: {err}"))?;
    let stdout = super::read_in_background(child.stdout.take().unwrap());
    let stderr = super::read_in_background(child.stderr.take().unwrap());
    let deadline = Instant::now() + GIT_TIMEOUT;
    loop {
        if let Some(status) = child.try_wait().map_err(|err| err.to_string())? {
            return Ok(Output {
                status,
                stdout: stdout.join().unwrap_or_default(),
                stderr: stderr.join().unwrap_or_default(),
            });
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("native Git workflow command timed out: {binary}"));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn checked(binary: &str, args: &[&str], cwd: &Path) -> Result<String, String> {
    let output = run(binary, args, cwd)?;
    checked_output(args, output)
}

fn checked_output(args: &[&str], output: Output) -> Result<String, String> {
    if !output.status.success() {
        return Err(format!(
            "native Git workflow command failed ({}): {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    String::from_utf8(output.stdout).map_err(|err| err.to_string())
}

struct Repository {
    root: PathBuf,
    file: String,
    branch: String,
    git: String,
    git_env: Vec<(String, String)>,
}

impl Repository {
    fn for_file(cwd: &Path, relative: &str) -> Result<Self, String> {
        if relative.is_empty()
            || relative.contains('\\')
            || Path::new(relative)
                .components()
                .any(|part| !matches!(part, Component::Normal(_)))
        {
            return Err(
                "native Git workflow path must be normalized and workspace-relative".into(),
            );
        }
        let g5d = std::env::var("G5_FAKE_G5D_BIN").unwrap_or_default();
        if !Path::new(&g5d).is_absolute() {
            return Err("native Git workflow requires an absolute G5_FAKE_G5D_BIN".into());
        }
        let context: Value = serde_json::from_str(&checked(&g5d, &["task", "context"], cwd)?)
            .map_err(|err| format!("invalid native Git task self-context: {err}"))?;
        let mounts = context["repoMounts"]
            .as_array()
            .ok_or("native Git task self-context omitted repoMounts")?;
        let candidates: Vec<_> = mounts
            .iter()
            .filter_map(|mount| {
                let path = mount["mountPath"].as_str()?;
                let file = relative.strip_prefix(&format!("{path}/"))?;
                Some((mount, path, file))
            })
            .collect();
        let [(mount, mount_path, file)] = candidates.as_slice() else {
            return Err("native Git workflow file must belong to exactly one assigned repo".into());
        };
        let repo_id = mount["repoId"]
            .as_str()
            .ok_or("assigned mount omitted repoId")?;
        let branches = context["branch"]["repoBranches"]
            .as_array()
            .ok_or("native Git task self-context omitted repoBranches")?;
        let branches: Vec<_> = branches
            .iter()
            .filter(|branch| branch["repoId"].as_str() == Some(repo_id))
            .collect();
        let [branch] = branches.as_slice() else {
            return Err("native Git workflow requires one assigned branch for its repo".into());
        };
        let branch = branch["branchName"]
            .as_str()
            .filter(|name| !name.is_empty() && *name != "main" && *name != "master")
            .ok_or("native Git workflow requires a non-main assigned branch")?
            .to_string();
        let workspace = cwd.canonicalize().map_err(|err| err.to_string())?;
        let root = cwd
            .join(mount_path)
            .canonicalize()
            .map_err(|err| err.to_string())?;
        if !root.starts_with(&workspace) || root == workspace {
            return Err("native Git workflow repository escapes the workspace".into());
        }
        let git = std::env::var("G5_FAKE_GIT_BIN").unwrap_or_else(|_| "/usr/bin/git".into());
        if !Path::new(&git).is_absolute() {
            return Err("G5_FAKE_GIT_BIN must be an absolute executable path".into());
        }
        // Deterministic test commits cannot depend on a guest image's user
        // config (fresh clones have none). Keep author/committer identity
        // process-local, including rebases after a peer advances the ref.
        let mut git_env: Vec<(String, String)> = [
            ("GIT_AUTHOR_NAME", "G5 Fake Provider"),
            ("GIT_AUTHOR_EMAIL", "fake-provider@g5.internal"),
            ("GIT_COMMITTER_NAME", "G5 Fake Provider"),
            ("GIT_COMMITTER_EMAIL", "fake-provider@g5.internal"),
        ]
        .into_iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect();
        // Native Git does not read G5D_TOKEN. Use the worker's HTTP credential
        // only for this assigned repo on the configured G5 API. Keep it out of
        // command arguments, repository config, and unrelated remotes.
        if let (Ok(api), Ok(token)) = (std::env::var("G5D_API_URL"), std::env::var("G5D_TOKEN")) {
            if (api.starts_with("http://") || api.starts_with("https://")) && !token.is_empty() {
                let count = std::env::var("GIT_CONFIG_COUNT")
                    .unwrap_or_else(|_| "0".into())
                    .parse::<usize>()
                    .map_err(|_| "invalid inherited GIT_CONFIG_COUNT")?;
                let url = format!("{}/git/{repo_id}", api.trim_end_matches('/'));
                let credential = base64::engine::general_purpose::STANDARD
                    .encode(format!("x-access-token:{token}"));
                git_env.push(("GIT_CONFIG_COUNT".into(), (count + 1).to_string()));
                git_env.push((
                    format!("GIT_CONFIG_KEY_{count}"),
                    format!("http.{url}.extraHeader"),
                ));
                git_env.push((
                    format!("GIT_CONFIG_VALUE_{count}"),
                    format!("Authorization: Basic {credential}"),
                ));
            }
        }
        let repo = Self {
            root,
            file: file.to_string(),
            branch,
            git,
            git_env,
        };
        repo.git(&["check-ref-format", &format!("refs/heads/{}", repo.branch)])?;
        Ok(repo)
    }

    fn git(&self, args: &[&str]) -> Result<String, String> {
        checked_output(args, self.run_git(args)?)
    }

    fn run_git(&self, args: &[&str]) -> Result<Output, String> {
        run_with_env(&self.git, args, &self.root, &self.git_env)
    }

    fn incorporate_remote(&self) -> Result<(), String> {
        self.git(&["fetch", "origin", &format!("refs/heads/{}", self.branch)])?;
        self.git(&["rebase", "FETCH_HEAD"])?;
        Ok(())
    }

    fn commit_and_push(&self, content: &str, message: &str) -> Result<(u128, u128), String> {
        let target = self.root.join(&self.file);
        let parent = target
            .parent()
            .ok_or("native Git workflow file has no parent")?;
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        if !parent
            .canonicalize()
            .map_err(|err| err.to_string())?
            .starts_with(&self.root)
            || target
                .symlink_metadata()
                .is_ok_and(|m| m.file_type().is_symlink())
        {
            return Err(
                "native Git workflow file escapes its assigned repo through a symlink".into(),
            );
        }
        if !self
            .git(&["diff", "--cached", "--name-only"])?
            .trim()
            .is_empty()
        {
            return Err(
                "native Git workflow refuses to commit another writer's staged changes".into(),
            );
        }
        std::fs::write(target, content).map_err(|err| err.to_string())?;
        let started = Instant::now();
        self.git(&["add", "--", &self.file])?;
        self.git(&["commit", "-m", message])?;
        let push_started = Instant::now();
        self.push_head()?;
        Ok((
            started.elapsed().as_millis(),
            push_started.elapsed().as_millis(),
        ))
    }

    fn head_is_published(&self) -> Result<bool, String> {
        let output = run(
            &self.git,
            &["merge-base", "--is-ancestor", "HEAD", "FETCH_HEAD"],
            &self.root,
        )?;
        match output.status.code() {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => Err(format!(
                "cannot verify native Git publication: {}",
                String::from_utf8_lossy(&output.stderr)
            )),
        }
    }

    fn push_head(&self) -> Result<(), String> {
        let refspec = format!("HEAD:refs/heads/{}", self.branch);
        for attempt in 0..3 {
            let output = self.run_git(&["push", "origin", &refspec])?;
            if output.status.success() {
                return Ok(());
            }
            let error = String::from_utf8_lossy(&output.stderr);
            if attempt == 2
                || !(error.contains("non-fast-forward") || error.contains("fetch first"))
            {
                return Err(format!("native Git workflow push failed: {}", error.trim()));
            }
            // Preserve peer commits before retrying a rejected publication.
            self.incorporate_remote()?;
        }
        unreachable!()
    }
}

pub(super) fn repeated_commit(prompt: &str, cwd: &Path) -> Result<Option<WorkflowReply>, String> {
    let Some(value) = payload(prompt, super::REPEATED_COMMIT_PREFIX)? else {
        return Ok(None);
    };
    let fixture: RepeatedCommit = serde_json::from_value(value)
        .map_err(|err| format!("invalid native repeated-commit payload: {err}"))?;
    if !matches!(fixture.mode.as_str(), "chat" | "task")
        || !(1..=10).contains(&fixture.commits.len())
        || fixture
            .commits
            .iter()
            .any(|commit| commit.message.trim().is_empty())
        || (fixture.mode == "task"
            && fixture
                .completion_message
                .as_deref()
                .is_none_or(|s| s.trim().is_empty()))
        || (fixture.mode == "chat" && fixture.completion_message.is_some())
    {
        return Err("invalid native repeated-commit mode, commits or completion message".into());
    }
    let repo = Repository::for_file(cwd, &fixture.relative_path)?;
    repo.incorporate_remote()?;
    let mut commits = Vec::new();
    let mut pushes = Vec::new();
    for commit in &fixture.commits {
        let (total_ms, push_ms) = repo.commit_and_push(&commit.content, &commit.message)?;
        commits.push(total_ms.to_string());
        pushes.push(push_ms.to_string());
    }
    if let Some(summary) = fixture.completion_message {
        run_task_complete(&summary, cwd)?;
    }
    let timing = if fixture.measure_timings {
        format!("; pushDurationsMs={}", pushes.join(","))
    } else {
        String::new()
    };
    Ok(Some(WorkflowReply {
        text: format!(
            "Fake repeated commit completed: mode={}; commits={}; commitDurationsMs={}{}; path={}; {}.",
            fixture.mode, commits.len(), commits.join(","), timing, fixture.relative_path, boundary()
        ),
        read_path: None,
        budget_exhausted: false,
        stream_deltas: None,
    }))
}

pub(super) fn budget_resume(
    prompt: &str,
    cwd: &Path,
    max_turns: Option<u64>,
) -> Result<Option<WorkflowReply>, String> {
    let Some(value) = payload(prompt, super::BUDGET_RESUME_PREFIX)? else {
        return Ok(None);
    };
    let fixture: BudgetResume = serde_json::from_value(value)
        .map_err(|err| format!("invalid native budget-resume payload: {err}"))?;
    if !super::env_truthy("G5_FAKE_BUDGET_RESUME_E2E") {
        return Err("native budget resume requires G5_FAKE_BUDGET_RESUME_E2E=1".into());
    }
    if fixture.expected_max_turns == 0 || max_turns != Some(fixture.expected_max_turns) {
        return Err("native budget resume max-turns does not match the fixture".into());
    }
    if fixture.checkpoint_content == fixture.completed_content
        || [
            &fixture.checkpoint_message,
            &fixture.completion_commit_message,
            &fixture.completion_message,
        ]
        .iter()
        .any(|message| message.trim().is_empty())
    {
        return Err(
            "native budget resume requires distinct contents and non-empty messages".into(),
        );
    }
    let repo = Repository::for_file(cwd, &fixture.relative_path)?;
    repo.incorporate_remote()?;
    let target = repo.root.join(&repo.file);
    let existing = match std::fs::read_to_string(&target) {
        Ok(content) => Some(content),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.to_string()),
    };
    let exhausted = match existing.as_deref() {
        None => {
            repo.commit_and_push(&fixture.checkpoint_content, &fixture.checkpoint_message)?;
            true
        }
        Some(content) if content == fixture.checkpoint_content => {
            let committed = repo.git(&["show", &format!("HEAD:{}", repo.file)])?;
            if committed != fixture.checkpoint_content {
                return Err("native budget resume checkpoint is not durable at Git HEAD".into());
            }
            if !repo.head_is_published()? {
                // A previous attempt committed locally but its push failed.
                // Publish that checkpoint and still emit the budget boundary.
                repo.push_head()?;
                true
            } else {
                repo.commit_and_push(
                    &fixture.completed_content,
                    &fixture.completion_commit_message,
                )?;
                false
            }
        }
        Some(content) if content == fixture.completed_content => {
            if repo.git(&["show", &format!("HEAD:{}", repo.file)])? != fixture.completed_content {
                return Err("native budget resume completion is not durable at Git HEAD".into());
            }
            // Retry a rejected push before reporting lifecycle completion.
            repo.push_head()?;
            false
        }
        Some(_) => return Err("native budget resume found an unexpected checkpoint".into()),
    };
    if !exhausted {
        run_task_complete(&fixture.completion_message, cwd)?;
    }
    Ok(Some(WorkflowReply {
        text: format!(
            "Fake budget {}: path={}; {}.",
            if exhausted {
                "checkpoint persisted before max-turn exhaustion"
            } else {
                "resume completed from the durable checkpoint"
            },
            fixture.relative_path,
            boundary()
        ),
        read_path: None,
        budget_exhausted: exhausted,
        stream_deltas: None,
    }))
}
