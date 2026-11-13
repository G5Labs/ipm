#!/usr/bin/env python3
"""Gate a build-mode deploy on a worker image that covers the promoted commit.

`resolve-worker-image` (infra/ci/cloudbuild-deploy.yaml) pins the READY
`g5-worker` GCE image an environment boots. The deploy and the worker-image
bake start together on a push and the bake takes longer, so without this gate
a push that changed both API code and worker-image inputs deployed the new API
against an older worker image. The API/host contract skew then surfaced as a
red canary or a live fleet on the wrong image.

An image COVERS the promoted commit when its source commit is the promoted
commit or one of its ancestors, and no file changed between the two matches
the worker-image trigger's own `includedFiles`/`ignoredFiles` (the single
source of truth, never duplicated here): it was baked from the same worker
inputs. The check spans the whole range from the image's commit, not just the
promoted commit's own diff. A bake of an earlier merge can finish after a later
merge's deploy starts, so the newest READY ancestor image can predate worker
changes the promoted commit already contains.

Decision, in build mode:

1. Take the READY image whose commit is the promoted commit or its
   graph-nearest ancestor (images are tried newest first, the promoted
   commit's own label first). If it covers the promoted commit, pin it. Its own image needs
   no trigger read; any other pin fails closed while the worker-image trigger
   is disabled.
2. Otherwise the worker inputs changed since that image need a bake. List
   the trigger's recent builds (the `trigger_id` filter runs server-side) and
   keep those for commits in the range:
   - any QUEUED/PENDING/WORKING: wait, bounded by `--wait-mins`, then walk
     again (the finished bake's image covers the promoted commit);
   - newest SUCCESS: its image is publishing; walk again, bounded by
     `--ready-grace-mins`;
   - newest FAILURE/INTERNAL_ERROR/TIMEOUT/CANCELLED/EXPIRED: fail closed;
   - none: re-check a few times (list eventual consistency), then run the
     enabled worker trigger once for the promoted commit. Log its build id
     and track that build directly under the same wait bound and READY grace;
     a failed start or unreadable status fails closed. Concurrent deploys may
     each start a bake; this gate does not lock across deploys.

Ancestry, changed files and range commits come from the local checkout when it
proves the image commit is an ancestor, else from the GitHub compare API
through the Cloud Build connection's read token (the deploy checkout is
shallow). When neither can decide, the gate fails closed.

Output (stdout, single line): projects/<project>/global/images/<name>, the
same contract as resolve_worker_image.py, so the calling step's exactness
stamp and Helm handoff are unchanged. Diagnostics go to stderr.

Stdlib only; runs on the Python <= 3.10 in gcr.io/cloud-builders images.
"""
from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from typing import Callable, Dict, FrozenSet, List, NamedTuple, Optional, Sequence, Tuple

import resolve_worker_image as rwi

Runner = Callable[[Sequence[str]], Tuple[int, str, str]]

RUNNING_STATUSES = frozenset({"QUEUED", "PENDING", "WORKING"})
FAILED_STATUSES = frozenset({"FAILURE", "INTERNAL_ERROR", "TIMEOUT", "CANCELLED", "EXPIRED"})

# Bounded re-checks when no worker-image build is listed for a range whose
# worker inputs changed: the bake and the deploy are created by the same push,
# and the deploy reaches this step well after its own image builds, so a
# genuinely missing build needs recovery (for example, a push skipped while
# the trigger was paused). Three fetches half a minute apart cover
# `gcloud builds list` eventual consistency before starting a bake.
LIST_RECHECKS = 3
LIST_RECHECK_SECS = 30
BUILDS_LIST_LIMIT = 20

HTTP_TIMEOUT_SECONDS = 60
# Upstream 5xx and network errors are retried with these delays between
# attempts: one transient 504 from the compare API (deploy 1953f272) or 503
# from accessReadToken (deploy f683f16c) otherwise failed a deploy outright.
# 4xx never retries; 404 keeps its non-ancestor meaning.
HTTP_RETRY_DELAYS_SECONDS = (5, 15, 45)
# The compare endpoint lists at most 300 files for the whole comparison; a
# list that long may be truncated, so it cannot rule out worker-image inputs.
GITHUB_COMPARE_FILES_CAP = 300


class WorkerTrigger(NamedTuple):
    id: str
    disabled: bool
    included: List[str]
    ignored: List[str]


class Span(NamedTuple):
    """How an image's commit relates to the promoted commit."""

    ancestor: bool  # the image commit is the promoted commit or an ancestor
    files: Optional[List[str]]  # changed paths in between; None = too many to list
    commits: FrozenSet[str]  # full shas after the image commit, up to the promoted one

    @property
    def exact(self) -> bool:
        return self.ancestor and not self.commits and self.files == []


NOT_ANCESTOR = Span(False, [], frozenset())


def _log(message: str) -> None:
    print(message, file=sys.stderr)


# --- Cloud Build -------------------------------------------------------------

def describe_worker_trigger(project: str, region: str, trigger: str, runner: Runner) -> WorkerTrigger:
    rc, out, err = runner(
        [
            "gcloud",
            "builds",
            "triggers",
            "describe",
            trigger,
            f"--project={project}",
            f"--region={region}",
            "--format=json(id,disabled,includedFiles,ignoredFiles)",
        ]
    )
    if rc != 0:
        raise RuntimeError(f"gcloud builds triggers describe failed (exit {rc}){rwi._diag(err)}")
    data = json.loads(out or "{}")
    if not data.get("id"):
        raise RuntimeError(f"gcloud builds triggers describe {trigger} returned no trigger id")
    return WorkerTrigger(
        id=str(data["id"]),
        disabled=bool(data.get("disabled")),
        included=[str(p) for p in data.get("includedFiles") or []],
        ignored=[str(p) for p in data.get("ignoredFiles") or []],
    )


def list_trigger_builds(project: str, region: str, trigger_id: str, runner: Runner) -> List[dict]:
    """Recent builds of the worker-image trigger, newest first.

    Only `trigger_id` (like `create_time` and `status`) is sent to the Cloud
    Build API as a filter. A `substitutions.*` filter makes gcloud page
    through the project's whole build history and filter client-side; that
    outgrew the runner's 120 s timeout and failed deploy 21b20232."""
    rc, out, err = runner(
        [
            "gcloud",
            "builds",
            "list",
            f"--project={project}",
            f"--region={region}",
            f"--limit={BUILDS_LIST_LIMIT}",
            f"--filter=trigger_id=\"{trigger_id}\"",
            "--format=json(id,status,createTime,substitutions.COMMIT_SHA)",
        ]
    )
    if rc != 0:
        raise RuntimeError(f"gcloud builds list failed (exit {rc}){rwi._diag(err)}")
    builds = [
        {
            "id": b["id"],
            "status": b["status"],
            "createTime": b.get("createTime") or "",
            "commit": (b.get("substitutions") or {}).get("COMMIT_SHA") or "",
        }
        for b in json.loads(out or "[]")
        if b.get("id") and b.get("status")
    ]
    builds.sort(key=lambda b: b["createTime"], reverse=True)
    return builds


def run_worker_trigger(project: str, region: str, trigger: str, commit: str, runner: Runner) -> str:
    """Start once; triggers run returns an Operation with metadata.build.id."""
    command = [
        "gcloud", "builds", "triggers", "run", trigger,
        f"--project={project}", f"--region={region}", f"--sha={commit}",
        "--format=value(metadata.build.id)",
    ]
    try:
        rc, out, err = runner(command)
    except (OSError, subprocess.TimeoutExpired) as exc:
        raise RuntimeError(f"could not start worker-image trigger {trigger} for {commit}: {exc}") from exc
    if rc != 0:
        raise RuntimeError(
            f"could not start worker-image trigger {trigger} for {commit} (exit {rc}){rwi._diag(err)}"
        )
    build_id = out.strip()
    if not build_id:
        raise RuntimeError(f"worker-image trigger {trigger} for {commit} returned no build id; refusing to retry")
    _log(f"started worker-image build {build_id} via {trigger} for {commit}")
    return build_id


def describe_worker_build(project: str, region: str, build_id: str, commit: str, runner: Runner) -> dict:
    """Track recovery by id rather than relying on eventually consistent lists."""
    rc, out, err = runner([
        "gcloud", "builds", "describe", build_id,
        f"--project={project}", f"--region={region}",
        "--format=json(id,status,substitutions.COMMIT_SHA)",
    ])
    if rc != 0:
        raise RuntimeError(f"gcloud builds describe {build_id} failed (exit {rc}){rwi._diag(err)}")
    data = json.loads(out or "{}")
    if (data.get("id") != build_id or not data.get("status")
            or (data.get("substitutions") or {}).get("COMMIT_SHA") != commit):
        raise RuntimeError(f"worker-image build {build_id} returned no matching build status for {commit}")
    return {"id": build_id, "status": data["status"], "commit": commit}


# --- Image commit vs promoted commit -------------------------------------------

def _git_lines(runner: Runner, cmd: List[str]) -> List[str]:
    rc, out, err = runner(cmd)
    if rc != 0:
        raise RuntimeError(f"{' '.join(cmd[3:5])} failed (exit {rc}){rwi._diag(err)}")
    return [line.strip() for line in out.splitlines() if line.strip()]


def local_span(label: str, commit: str, repo_dir: str, shallow: bool, runner: Runner) -> Optional[Span]:
    """The span from the local checkout, or None when it cannot decide (the
    label does not resolve, or a shallow checkout lacks the connecting
    history)."""
    full = rwi.resolve_local_sha(repo_dir, label, runner)
    if full is None:
        return None
    if not rwi.is_ancestor(repo_dir, full, commit, runner):
        return None if shallow else NOT_ANCESTOR
    # --no-renames reports a rename as delete + add, so a file moved out of a
    # worker-input path still counts as a change to that path.
    files = _git_lines(runner, ["git", "-C", repo_dir, "diff", "--name-only", "--no-renames", full, commit])
    commits = _git_lines(runner, ["git", "-C", repo_dir, "rev-list", f"{full}..{commit}"])
    return Span(True, files, frozenset(commits))


class GitHubNotFound(RuntimeError):
    pass


class GitHubCompare:
    """Span of an image commit from the GitHub compare API, authenticated with
    the Cloud Build connection's installation read token via
    `repositories:accessReadToken`; gcloud ships no token-fetch command."""

    def __init__(
        self,
        *,
        project: str,
        region: str,
        connection: str,
        repository: str,
        owner: str,
        repo: str,
        runner: Runner,
        opener: Callable[..., object] = urllib.request.urlopen,
        sleep: Callable[[float], None] = time.sleep,
    ) -> None:
        self.project = project
        self.region = region
        self.connection = connection
        self.repository = repository
        self.owner = owner
        self.repo = repo
        self.runner = runner
        self.opener = opener
        self.sleep = sleep

    def _gcloud_token(self) -> str:
        rc, out, err = self.runner(["gcloud", "auth", "print-access-token"])
        if rc != 0 or not out.strip():
            raise RuntimeError(f"could not obtain access token{rwi._diag(err)}")
        return out.strip()

    def _read_token(self) -> str:
        url = (
            "https://cloudbuild.googleapis.com/v2/"
            f"projects/{self.project}/locations/{self.region}/connections/"
            f"{urllib.parse.quote(self.connection)}/repositories/"
            f"{urllib.parse.quote(self.repository)}:accessReadToken"
        )
        request = urllib.request.Request(
            url,
            data=b"{}",
            headers={
                "Authorization": f"Bearer {self._gcloud_token()}",
                "Content-Type": "application/json",
            },
            method="POST",
        )
        payload = self._fetch(request)
        token = (payload.get("token") or "").strip()
        if not token:
            raise RuntimeError("accessReadToken returned no token")
        return token

    def _fetch(self, request: urllib.request.Request) -> dict:
        for delay in (*HTTP_RETRY_DELAYS_SECONDS, None):
            try:
                with self.opener(request, timeout=HTTP_TIMEOUT_SECONDS) as response:  # type: ignore[call-arg]
                    return json.loads(response.read().decode("utf-8"))
            except urllib.error.HTTPError as exc:
                detail = exc.read().decode("utf-8", "replace")[:200]
                error = GitHubNotFound if exc.code == 404 else RuntimeError
                failure = error(f"{request.full_url} failed ({exc.code}): {detail}")
                if exc.code < 500 or delay is None:
                    raise failure from exc
            except (urllib.error.URLError, OSError) as exc:
                failure = RuntimeError(f"{request.full_url} unreachable: {exc}")
                if delay is None:
                    raise failure from exc
            _log(f"{failure}; retrying in {delay}s")
            self.sleep(delay)
        raise AssertionError("unreachable")

    def __call__(self, base: str, head: str) -> Span:
        token = self._read_token()
        request = urllib.request.Request(
            f"https://api.github.com/repos/{urllib.parse.quote(self.owner)}/"
            f"{urllib.parse.quote(self.repo)}/compare/{base}...{head}",
            headers={
                "Authorization": f"Bearer {token}",
                "Accept": "application/vnd.github+json",
                "X-GitHub-Api-Version": "2022-11-28",
            },
        )
        try:
            payload = self._fetch(request)
        except GitHubNotFound as exc:
            _log(f"GitHub cannot compare {base} with {head}; treating that image as a non-ancestor: {exc}")
            return NOT_ANCESTOR
        status = payload.get("status")
        if status in ("behind", "diverged"):
            return NOT_ANCESTOR
        if status not in ("ahead", "identical"):
            raise RuntimeError(f"GitHub compare {base}...{head} returned unexpected status {status!r}")
        entries = payload.get("files") or []
        files: Optional[List[str]] = None
        if len(entries) < GITHUB_COMPARE_FILES_CAP:
            # A rename lists the new path; the old path changed too.
            files = [p for f in entries for p in (f.get("filename"), f.get("previous_filename")) if p]
        commits = frozenset(c["sha"] for c in payload.get("commits") or [] if c.get("sha"))
        return Span(True, files, commits)


# --- Trigger filter matching ---------------------------------------------------

def _glob_to_regex(pattern: str) -> "re.Pattern[str]":
    """Cloud Build trigger file filters: `**` spans directories, `*` and `?`
    stay within one path segment."""
    out = []
    i = 0
    while i < len(pattern):
        c = pattern[i]
        if c == "*":
            if pattern[i : i + 2] == "**":
                if pattern[i : i + 3] == "**/":
                    out.append("(?:.*/)?")
                    i += 3
                else:
                    out.append(".*")
                    i += 2
                continue
            out.append("[^/]*")
        elif c == "?":
            out.append("[^/]")
        else:
            out.append(re.escape(c))
        i += 1
    return re.compile("^" + "".join(out) + "$")


def touches_worker_inputs(files: Sequence[str], included: Sequence[str], ignored: Sequence[str]) -> bool:
    """Cloud Build semantics: files matching `ignoredFiles` are dropped; the
    trigger fires when any remaining file matches `includedFiles` (or when
    `includedFiles` is empty and anything remains)."""
    ignore = [_glob_to_regex(p) for p in ignored]
    include = [_glob_to_regex(p) for p in included]
    remaining = [f for f in files if not any(r.match(f) for r in ignore)]
    if not remaining:
        return False
    if not include:
        return True
    return any(r.match(f) for f in remaining for r in include)


def worker_input_changes(span: Span, trigger: WorkerTrigger) -> List[str]:
    if span.files is None:
        return [f"{GITHUB_COMPARE_FILES_CAP}+ changed files, list truncated"]
    return [f for f in span.files if touches_worker_inputs([f], trigger.included, trigger.ignored)]


# --- Gate ----------------------------------------------------------------------

def image_ref(project: str, chosen: dict) -> str:
    return f"projects/{project}/global/images/{chosen['name']}"


def _label(image: dict) -> str:
    return image["labels"]["g5_git_short_sha"]


def _preview(paths: Sequence[str]) -> str:
    return ", ".join(paths[:5]) + (", …" if len(paths) > 5 else "")


def nearest_ancestor_image(
    images: List[dict], commit: str, span_of: Callable[[str], Span]
) -> Optional[Tuple[dict, Span]]:
    """The READY image whose commit is the promoted commit or its graph-nearest
    ancestor, trying images newest first with the promoted commit's own label
    first (newest wins among rebuilds of one commit).

    Once an ancestor is found, only images of commits inside its span can be
    nearer, so only those are compared further. A rebuild of an older commit
    created after a newer ancestor's image therefore cannot shadow it."""
    ordered = sorted(images, key=lambda image: not commit.startswith(_label(image)))
    best: Optional[Tuple[dict, Span]] = None
    for image in ordered:
        label = _label(image)
        if best is not None and not any(c.startswith(label) for c in best[1].commits):
            continue
        span = span_of(label)
        if span.ancestor and (best is None or len(span.commits) < len(best[1].commits)):
            best = (image, span)
    return best


def await_worker_image(
    *,
    image_project: str,
    family: str,
    commit: str,
    repo_dir: str,
    build_project: str,
    region: str,
    trigger: str,
    wait_secs: int,
    ready_grace_secs: int,
    poll_secs: int,
    runner: Runner,
    github_compare: Optional[Callable[[str, str], Span]] = None,
    sleep: Callable[[float], None] = time.sleep,
    clock: Callable[[], float] = time.monotonic,
) -> str:
    rwi.resolve_artifact_project(build_project, image_project)
    shallow = rwi.is_shallow_repo(repo_dir, runner)
    spans: Dict[str, Span] = {}

    def span_of(label: str) -> Span:
        if label not in spans:
            span = local_span(label, commit, repo_dir, shallow, runner)
            if span is None:
                if github_compare is None:
                    raise RuntimeError(
                        f"cannot tell whether image commit {label} is an ancestor of {commit} "
                        "(shallow checkout, no GitHub access); refusing to pick a worker image"
                    )
                span = github_compare(label, commit)
            spans[label] = span
        return spans[label]

    worker_trigger: Optional[WorkerTrigger] = None
    deadline = clock() + wait_secs
    # READY grace per successful bake: a later bake in the range gets its own.
    grace: Optional[Tuple[str, float]] = None
    misses = 0
    recovery_build_id: Optional[str] = None
    while True:
        images = rwi.eligible_candidates(rwi.list_family_images(image_project, family, runner))
        found = nearest_ancestor_image(images, commit, span_of)
        if found is None:
            raise LookupError(f"no READY sha-labeled {family} image is {commit} or one of its ancestors")
        image, span = found
        if image_project == rwi.ADMIN_ARTIFACT_PROJECT:
            verified = rwi.validate_image_provenance(image, image_project, build_project, region,
                                                    runner, require_success=False)
            if verified["status"] != "SUCCESS":
                remaining = int(deadline - clock())
                if remaining <= 0:
                    raise RuntimeError("Worker image is READY but its trusted QA build has not succeeded")
                _log("Worker image is READY; waiting for its QA publisher to finish verification/summary")
                sleep(min(poll_secs, remaining))
                continue
        if span.exact:
            _log(f"worker image {image['name']} is the promoted commit's own image; no wait")
            return image_ref(image_project, image)

        if worker_trigger is None:
            worker_trigger = describe_worker_trigger(build_project, region, trigger, runner)
        disabled = RuntimeError(
            f"worker-image trigger {trigger} is disabled, so its missing bakes prove nothing about "
            f"{image['name']}; re-enable it (or bake and re-run) before deploying"
        )
        changes = worker_input_changes(span, worker_trigger)
        if not changes:
            if worker_trigger.disabled:
                raise disabled
            _log(
                f"{len(span.files or [])} file(s) changed since {image['name']} and none match "
                f"{trigger}'s file filters; pinning it for {commit}"
            )
            return image_ref(image_project, image)

        changed = f"worker-image inputs changed since {image['name']} ({_preview(changes)})"
        if recovery_build_id is not None:
            builds = [describe_worker_build(build_project, region, recovery_build_id, commit, runner)]
        else:
            builds = [
                b for b in list_trigger_builds(build_project, region, worker_trigger.id, runner)
                if b["commit"] in span.commits
            ]
        running = [b for b in builds if b["status"] in RUNNING_STATUSES]
        if running:
            remaining = int(deadline - clock())
            if remaining <= 0:
                raise RuntimeError(
                    f"{changed}; worker-image build {running[0]['id']} is still {running[0]['status']} "
                    f"after the {wait_secs}s wait bound; refusing to deploy on {image['name']}."
                )
            _log(
                f"{changed}; worker-image build {running[0]['id']} for {running[0]['commit']} is "
                f"{running[0]['status']}; waiting ({remaining}s left before the bound)"
            )
            sleep(min(poll_secs, remaining))
            continue
        if builds:
            newest = builds[0]
            if newest["status"] == "SUCCESS":
                if grace is None or grace[0] != newest["id"]:
                    grace = (newest["id"], clock() + ready_grace_secs)
                if clock() >= grace[1]:
                    raise RuntimeError(
                        f"worker-image build {newest['id']} for {newest['commit']} succeeded but no READY "
                        f"{family} image covering {commit} appeared within {ready_grace_secs}s; "
                        f"refusing to deploy on {image['name']} ({changed})."
                    )
                _log(f"worker-image build {newest['id']} succeeded; waiting for its image to reach READY")
                sleep(poll_secs)
                continue
            if newest["status"] in FAILED_STATUSES:
                raise RuntimeError(
                    f"worker-image build {newest['id']} for {newest['commit']} ended {newest['status']}; "
                    f"refusing to deploy the promoted API on {image['name']} ({changed}). Fix or re-run "
                    f"{trigger} for {newest['commit']}, then re-run this deploy."
                )
            raise RuntimeError(
                f"worker-image build {newest['id']} for {newest['commit']} has unexpected status "
                f"{newest['status']!r}"
            )
        if worker_trigger.disabled:
            raise disabled
        misses += 1
        if misses >= LIST_RECHECKS:
            # An operator may have paused the trigger during the rechecks.
            worker_trigger = describe_worker_trigger(build_project, region, trigger, runner)
            if worker_trigger.disabled:
                raise disabled
            if clock() >= deadline:
                raise RuntimeError(f"{changed}; no {trigger} build appeared within the {wait_secs}s wait bound")
            _log(f"{changed}; no {trigger} build listed after {misses} checks; starting a bake for {commit}")
            recovery_build_id = run_worker_trigger(build_project, region, trigger, commit, runner)
            continue
        _log(
            f"{changed}; no {trigger} build listed for that range yet (check {misses}/{LIST_RECHECKS}); "
            f"re-checking in {LIST_RECHECK_SECS}s"
        )
        sleep(LIST_RECHECK_SECS)


def main(argv: Optional[Sequence[str]] = None, runner: Runner = rwi._default_runner) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--project", required=True, help="GCE project holding the worker images")
    parser.add_argument("--family", required=True)
    parser.add_argument("--commit", required=True, help="full promoted COMMIT_SHA")
    parser.add_argument("--repo-dir", default=".")
    parser.add_argument("--build-project", required=True, help="Cloud Build project of the worker-image trigger")
    parser.add_argument("--region", required=True)
    parser.add_argument("--trigger", required=True, help="worker-image trigger name")
    parser.add_argument("--wait-mins", type=int, default=90)
    parser.add_argument("--ready-grace-mins", type=int, default=10)
    parser.add_argument("--poll-secs", type=int, default=60)
    parser.add_argument("--github-owner", default="")
    parser.add_argument("--github-repo", default="")
    parser.add_argument("--github-connection", default="", help="Cloud Build connection name")
    parser.add_argument("--github-connection-repository", default="", help="Cloud Build repository name")
    args = parser.parse_args(argv)

    if not re.match(r"^[0-9a-f]{40}$", args.commit):
        _log(f"--commit must be a full 40-hex sha, got {args.commit!r}")
        return 1
    if args.wait_mins < 0 or args.ready_grace_mins < 0 or args.poll_secs <= 0:
        _log("--wait-mins and --ready-grace-mins must be >= 0 and --poll-secs > 0")
        return 1

    github_compare: Optional[GitHubCompare] = None
    if all([args.github_owner, args.github_repo, args.github_connection, args.github_connection_repository]):
        github_compare = GitHubCompare(
            project=args.build_project,
            region=args.region,
            connection=args.github_connection,
            repository=args.github_connection_repository,
            owner=args.github_owner,
            repo=args.github_repo,
            runner=runner,
        )

    try:
        ref = await_worker_image(
            image_project=args.project,
            family=args.family,
            commit=args.commit,
            repo_dir=args.repo_dir,
            build_project=args.build_project,
            region=args.region,
            trigger=args.trigger,
            wait_secs=args.wait_mins * 60,
            ready_grace_secs=args.ready_grace_mins * 60,
            poll_secs=args.poll_secs,
            runner=runner,
            github_compare=github_compare,
        )
    except (
        RuntimeError,
        LookupError,
        ValueError,
        KeyError,
        OSError,
        subprocess.TimeoutExpired,
    ) as exc:
        _log(f"await-worker-image failed: {exc}")
        return 1
    print(ref)
    return 0


if __name__ == "__main__":
    sys.exit(main())
