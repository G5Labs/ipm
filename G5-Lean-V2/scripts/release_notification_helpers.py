#!/usr/bin/env python3
"""Shared release-notification helpers ported from G5.

Ontology: g5.quality.delivery.cutoff-summary. Extracted from G5
generation/scripts/notify_prod_deploy.py; no deployment or approval entrypoint.
Stdlib only, compatible with Cloud Build Python 3.10.
"""

from __future__ import annotations

import datetime as dt
import json
import re
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from typing import Optional
from zoneinfo import ZoneInfo

from check_deploy_freshness import parse_time

POST_ATTEMPTS = 3
POST_RETRY_SLEEP_SECONDS = 10
GCLOUD_TIMEOUT_SECONDS = 300
TOKEN_TIMEOUT_SECONDS = 60
HTTP_TIMEOUT_SECONDS = 60
WEBHOOK_PLACEHOLDER_MARKER = "PLACEHOLDER"
DISPLAY_TZ = "America/Los_Angeles"
DISPLAY_TZ_LABEL = "PT"
MAX_SECTION_CHARS = 2900
MAX_ROWS_PER_SECTION = 20
MAX_ROW_SECTIONS = 40
SQUASH_SUBJECT_RE = re.compile(r"^(?P<title>.*)\s+\(#(?P<number>\d+)\)$")
MERGE_SUBJECT_RE = re.compile(r"^Merge pull request #(?P<number>\d+)\b")


class GcloudClient:
    """Thin effect wrapper over gcloud reads (pattern of approve_release_train)."""

    def __init__(self, project: str, region: str, run=subprocess.run,
                 opener=urllib.request.urlopen):
        self.project = project
        self.region = region
        self._run = run
        self._opener = opener

    def _gcloud(self, args: list, with_region: bool = True) -> str:
        full = ["gcloud"] + args + [f"--project={self.project}"]
        if with_region:
            full.append(f"--region={self.region}")
        try:
            result = self._run(full, capture_output=True, text=True,
                               timeout=GCLOUD_TIMEOUT_SECONDS)
        except subprocess.TimeoutExpired as exc:
            raise RuntimeError(
                f"gcloud {args[0]} timed out after {GCLOUD_TIMEOUT_SECONDS}s"
            ) from exc
        if result.returncode != 0:
            raise RuntimeError(f"gcloud {args[0]} failed: {result.stderr.strip()}")
        return result.stdout

    def access_secret(self, secret_name: str, version: str = "latest") -> str:
        return self._gcloud(
            ["secrets", "versions", "access", version, f"--secret={secret_name}"],
            with_region=False,
        ).strip()

    def _access_token(self) -> str:
        try:
            result = self._run(
                ["gcloud", "auth", "print-access-token"], capture_output=True, text=True,
                timeout=TOKEN_TIMEOUT_SECONDS,
            )
        except subprocess.TimeoutExpired as exc:
            raise RuntimeError(
                f"token minting timed out after {TOKEN_TIMEOUT_SECONDS}s"
            ) from exc
        if result.returncode != 0:
            raise RuntimeError(f"could not obtain access token: {result.stderr.strip()}")
        return result.stdout.strip()

    def github_read_token(self, connection: str, repository: str) -> str:
        """GitHub installation read token from the Cloud Build v2
        `repositories:accessReadToken` endpoint. The gcloud CLI ships no
        working token-fetch command (`gcloud builds connections
        fetch-access-token` does not exist in current gcloud)."""
        url = (
            "https://cloudbuild.googleapis.com/v2/"
            f"projects/{self.project}/locations/{self.region}/connections/"
            f"{urllib.parse.quote(connection)}/repositories/"
            f"{urllib.parse.quote(repository)}:accessReadToken"
        )
        request = urllib.request.Request(
            url,
            data=b"{}",
            headers={
                "Authorization": f"Bearer {self._access_token()}",
                "Content-Type": "application/json",
            },
            method="POST",
        )
        try:
            with self._opener(request, timeout=HTTP_TIMEOUT_SECONDS) as response:
                payload = json.loads(response.read().decode("utf-8"))
        except urllib.error.HTTPError as exc:
            detail = exc.read().decode("utf-8", "replace")[:200]
            raise RuntimeError(f"accessReadToken failed ({exc.code}): {detail}") from exc
        except urllib.error.URLError:
            raise
        except OSError as exc:
            # A read()-side socket timeout/reset raises bare OSError; normalize
            # into the URLError path the callers' note-based recovery handles.
            raise urllib.error.URLError(exc) from exc
        token = (payload.get("token") or "").strip()
        if not token:
            raise RuntimeError("accessReadToken returned no token")
        return token


def secret_absent(exc: Exception) -> bool:
    """True only for an explicitly missing secret/version (gcloud NOT_FOUND) —
    the one secret failure allowed to skip cleanly (). Everything
    else (permission, network, command) is an outage and must fail soft with
    the loud marker instead of masquerading as "not configured"."""
    return "NOT_FOUND" in str(exc)


def _github_get(token: str, path: str, opener=urllib.request.urlopen) -> object:
    request = urllib.request.Request(
        f"https://api.github.com/{path}",
        headers={
            "Authorization": f"token {token}",
            "Accept": "application/vnd.github+json",
        },
    )
    try:
        with opener(request, timeout=HTTP_TIMEOUT_SECONDS) as response:
            return json.loads(response.read().decode("utf-8"))
    except urllib.error.URLError:
        raise
    except OSError as exc:
        # A read()-side socket timeout/reset raises bare OSError; normalize
        # into the URLError path so the notification still posts with its
        # unavailable note instead of skipping the message.
        raise urllib.error.URLError(exc) from exc


def fetch_compare(token: str, owner: str, repo: str, base: str, head: str,
                  opener=urllib.request.urlopen) -> dict:
    return _github_get(
        token,
        f"repos/{urllib.parse.quote(owner)}/{urllib.parse.quote(repo)}/compare/"
        f"{urllib.parse.quote(base)}...{urllib.parse.quote(head)}",
        opener=opener,
    )


def fetch_commit_pulls(token: str, owner: str, repo: str, sha: str,
                       opener=urllib.request.urlopen) -> list:
    """Pull requests associated with one commit — resolves the merged PR for
    squash, merge, and rebase merges alike, including squash commits whose
    subject carries no (#N) suffix (common on this repo's main)."""
    return _github_get(
        token,
        f"repos/{urllib.parse.quote(owner)}/{urllib.parse.quote(repo)}/commits/"
        f"{urllib.parse.quote(sha)}/pulls",
        opener=opener,
    )


def _subject_row(commit: dict) -> Optional[dict]:
    """Fallback PR row parsed from the commit subject: squash-merge
    `<title> (#N)` primarily, `Merge pull request #N ...` secondarily (title
    then comes from the merge-commit body's first non-empty line, when any)."""
    message = ((commit.get("commit") or {}).get("message")) or ""
    lines = message.splitlines()
    subject = lines[0].strip() if lines else ""
    merged_raw = (((commit.get("commit") or {}).get("committer")) or {}).get("date") or ""
    try:
        merged_at = parse_time(merged_raw) if merged_raw else None
    except ValueError:
        merged_at = None
    author = ((commit.get("author") or {}).get("login")
              or (((commit.get("commit") or {}).get("author")) or {}).get("name")
              or "unknown")

    squash = SQUASH_SUBJECT_RE.match(subject)
    merge = MERGE_SUBJECT_RE.match(subject) if not squash else None
    if squash:
        number = int(squash.group("number"))
        title = squash.group("title").strip()
    elif merge:
        number = int(merge.group("number"))
        body_lines = [line.strip() for line in lines[1:] if line.strip()]
        title = body_lines[0] if body_lines else subject
    else:
        return None
    return {"number": number, "title": title, "author": author, "merged_at": merged_at}


def pr_rows(compare: dict, get_pulls=None, base_ref: Optional[str] = None):
    """(rows, commits_without_pr, commits_truncated, association_failures)
    from one compare payload.

    Each commit resolves to its merged PR(s) through `get_pulls(sha)` (the
    commits/{sha}/pulls association); commits whose association read fails or
    returns no merged PR fall back to subject parsing. When `base_ref` is set,
    only PRs merged INTO that branch qualify — the association endpoint
    returns every PR touching the commit, so an unfiltered read would also
    list backport/release-branch PRs sharing the commit. Rows are
    deduped by PR number (newest merged time wins) and ordered by merge time.
    """
    rows: dict = {}
    without_pr = 0
    association_failures = 0
    for commit in compare.get("commits") or []:
        found = []
        filtered_by_base_ref = False
        sha = commit.get("sha") or ""
        if sha and get_pulls is not None:
            try:
                for pull in get_pulls(sha):
                    merged_raw = pull.get("merged_at")
                    if not merged_raw:
                        continue
                    if base_ref and ((pull.get("base") or {}).get("ref") or "") != base_ref:
                        filtered_by_base_ref = True
                        continue
                    try:
                        merged_at = parse_time(merged_raw)
                    except ValueError:
                        merged_at = None
                    found.append({
                        "number": int(pull["number"]),
                        "title": (pull.get("title") or "").strip() or "(untitled)",
                        "author": ((pull.get("user") or {}).get("login")) or "unknown",
                        "merged_at": merged_at,
                    })
            except (RuntimeError, ValueError, KeyError, TypeError,
                    urllib.error.URLError) as exc:
                association_failures += 1
                print(f"PR association lookup failed for {sha[:9]}: {exc}", file=sys.stderr)

        if not found:
            if filtered_by_base_ref:
                # The association read succeeded and every merged PR targets
                # another branch; the subject fallback must not resurrect a
                # deliberately excluded PR from its "(#N)" suffix.
                without_pr += 1
                continue
            fallback = _subject_row(commit)
            if fallback is None:
                without_pr += 1
                continue
            found = [fallback]

        for row in found:
            existing = rows.get(row["number"])
            if existing is None or (
                row["merged_at"] is not None
                and (existing["merged_at"] is None or row["merged_at"] > existing["merged_at"])
            ):
                rows[row["number"]] = row

    epoch = dt.datetime(1970, 1, 1, tzinfo=dt.timezone.utc)
    ordered = sorted(rows.values(), key=lambda row: (row["merged_at"] or epoch, row["number"]))
    total = compare.get("total_commits")
    listed = len(compare.get("commits") or [])
    truncated = max(0, total - listed) if isinstance(total, int) else 0
    return ordered, without_pr, truncated, association_failures


def format_merged_time(merged_at: Optional[dt.datetime]) -> str:
    if merged_at is None:
        return "unknown"
    local = merged_at.astimezone(ZoneInfo(DISPLAY_TZ))
    return f"{local.strftime('%b %d %H:%M')} {DISPLAY_TZ_LABEL}"


def escape_mrkdwn(text: str) -> str:
    return text.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


def format_row(row: dict, owner: str, repo: str) -> str:
    url = f"https://github.com/{owner}/{repo}/pull/{row['number']}"
    return (
        f"• <{url}|#{row['number']}> · *{escape_mrkdwn(row['author'])}* · "
        f"{escape_mrkdwn(row['title'])} · _{format_merged_time(row['merged_at'])}_"
    )


def chunk_rows(lines: list) -> list:
    """Group row lines into section-sized texts within Slack limits."""
    sections = []
    current: list = []
    current_len = 0
    for line in lines:
        if current and (
            len(current) >= MAX_ROWS_PER_SECTION
            or current_len + len(line) + 1 > MAX_SECTION_CHARS
        ):
            sections.append("\n".join(current))
            current, current_len = [], 0
        current.append(line)
        current_len += len(line) + 1
    if current:
        sections.append("\n".join(current))
    return sections


def post_webhook(webhook_url: str, payload: dict, opener=urllib.request.urlopen,
                 sleep=time.sleep) -> None:
    body = json.dumps(payload).encode("utf-8")
    last_error: Optional[Exception] = None
    for attempt in range(1, POST_ATTEMPTS + 1):
        request = urllib.request.Request(
            webhook_url,
            data=body,
            headers={"Content-Type": "application/json"},
            method="POST",
        )
        try:
            with opener(request, timeout=HTTP_TIMEOUT_SECONDS) as response:
                response.read()
            return
        except urllib.error.HTTPError as exc:
            if exc.code < 500:
                detail = exc.read().decode("utf-8", "replace")
                raise RuntimeError(f"Slack webhook rejected the message ({exc.code}): {detail}") from exc
            last_error = exc
        except OSError as exc:
            # URLError and socket timeouts alike (bounded I/O).
            last_error = exc
        if attempt < POST_ATTEMPTS:
            print(
                f"webhook post attempt {attempt}/{POST_ATTEMPTS} failed: {last_error}; retrying",
                file=sys.stderr,
            )
            sleep(POST_RETRY_SLEEP_SECONDS)
    raise RuntimeError(f"Slack webhook post failed after {POST_ATTEMPTS} attempts: {last_error}")
