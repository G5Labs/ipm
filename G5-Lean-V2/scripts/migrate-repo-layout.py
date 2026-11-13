#!/usr/bin/env python3
"""One-off move of repository storage to the per-organization layout (#1079).

Before:  {base}/{repo_id}/   {base}/.review-artifacts/{repo_id}   {base}/.{repo_id}.lock
After:   {base}/{org_id}/{repo_id}/   {base}/{org_id}/.review-artifacts/{repo_id}

Stale Filestore lock files are deleted; the API recreates them at
{base}/{org_id}/.{repo_id}.lock. {base}/.worktrees and {base}/.attachments (the
attachment store of G5-Lean#1053, if present) stay.
Top-level entries that no repos row names are reported and left in place, as
are rows without a directory (provider mirrors re-clone those).

The script exits non-zero while any row's old directory {base}/{repo_id} still
exists or a move was refused because its target exists. Run that check before
deploying: the new release looks only at
{base}/{org_id}/{repo_id}, and ensure_publication_mirror silently re-clones a
missing mirror from its provider, which drops G5-only refs (refs/g5/*, flush
snapshots) instead of failing.

QA runbook (QA deploy is forward-only; so is this move):

 1. Stop every writer and keep it stopped:
      kubectl --context=gke_g5-qa-1ccc_us-central1_g5-lean-qa -n g5 \\
        scale deployment/g5-api deployment/g5-daemon --replicas=0
    Wait until no g5-api or g5-daemon pod remains. Do not scale the old release
    back up afterwards: it would not find the moved repositories.
 2. Export the rows with a role that bypasses RLS (repos is FORCE RLS; a
    normal role sees no rows):
      psql "$ADMIN_DATABASE_URL" -X -At -F ' ' -c 'SELECT id, org_id FROM repos' > rows.txt
    (Or pass --database-url where psql can reach the database and the store.)
 3. In a helper pod that mounts PVC g5-repos-pvc at /mnt/g5-repos and runs as
    UID/GID 10001 (or root; new directories then take the base's owner), copy
    this script and rows.txt in and dry-run it:
      python3 migrate-repo-layout.py --base /mnt/g5-repos/repos --rows-file rows.txt --dry-run
    Review every MOVE, ORPHAN, NO-DIRECTORY and REFUSED line.
 4. Run it without --dry-run and confirm exit status 0. Rerunning is safe; a
    completed move reports only "in place" repositories.
 5. Delete the helper pod and deploy the release that reads the new layout.

Dev stacks can be reset instead of migrated. Self-hosted installs are fresh and
need no move.
"""

from __future__ import annotations

import argparse
import os
from pathlib import Path
import stat
import subprocess
import sys
import uuid

REVIEW_ARTIFACTS = ".review-artifacts"
# Shared dot-directories that are not per-repository and never orphans.
SHARED_DIRS = frozenset({".worktrees", ".attachments", REVIEW_ARTIFACTS})


def canonical_uuid(value: str, what: str) -> str:
    try:
        return str(uuid.UUID(value))
    except ValueError:
        raise SystemExit(f"Invalid {what} in repos rows: {value!r}") from None


def parse_rows(text: str) -> list[tuple[str, str]]:
    """Parse `repo_id org_id` lines (space, tab or `|` separated)."""
    rows: list[tuple[str, str]] = []
    seen: set[str] = set()
    for line in text.splitlines():
        line = line.strip()
        if not line:
            continue
        fields = line.replace("|", " ").split()
        if len(fields) != 2:
            raise SystemExit(f"Expected 'repo_id org_id' per line, got {line!r}")
        repo, org = canonical_uuid(fields[0], "repo id"), canonical_uuid(fields[1], "org id")
        if repo in seen:
            raise SystemExit(f"Duplicate repo id in repos rows: {repo}")
        seen.add(repo)
        rows.append((repo, org))
    return rows


def psql(database_url: str, sql: str) -> str:
    # The URL is passed as an argument and never printed: it carries a password.
    result = subprocess.run(
        ["psql", "-X", "-q", "-At", "-F", " ", "-v", "ON_ERROR_STOP=1", "-d", database_url, "-c", sql],
        capture_output=True, text=True, timeout=120,
    )
    if result.returncode != 0:
        raise SystemExit(f"psql failed ({result.returncode}): {result.stderr.strip()}")
    return result.stdout


def read_rows_from_database(database_url: str) -> list[tuple[str, str]]:
    # repos is under FORCE RLS: an ordinary role reads zero rows, which would
    # make every repository look like an orphan and move nothing.
    bypass = psql(database_url,
                  "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname = current_user").strip()
    if bypass != "t":
        raise SystemExit("The database role must be a superuser or have BYPASSRLS to read every repos row")
    return parse_rows(psql(database_url, "SELECT id, org_id FROM repos ORDER BY org_id, id"))


class Mover:
    def __init__(self, base: Path, dry_run: bool, out=print):
        self.base = base
        self.dry_run = dry_run
        self.out = out
        self.base_stat = base.stat()
        self.created: set[Path] = set()
        self.refused = 0

    def say(self, tag: str, message: str) -> None:
        self.out(f"{'DRY-RUN ' if self.dry_run else ''}{tag}: {message}")

    def ensure_dir(self, path: Path) -> None:
        if path.is_dir() or path in self.created:
            return
        if path.exists() or path.is_symlink():
            raise SystemExit(f"Refusing: {path} exists and is not a directory")
        self.ensure_dir(path.parent)
        self.created.add(path)
        if self.dry_run:
            return
        path.mkdir()
        # Match the base's mode, and its owner when run as root, so the API
        # user can create repositories in the new organization directory.
        os.chmod(path, stat.S_IMODE(self.base_stat.st_mode))
        if os.geteuid() == 0:
            os.chown(path, self.base_stat.st_uid, self.base_stat.st_gid)

    def move(self, tag: str, source: Path, target: Path) -> bool:
        """Rename source to target. Returns False when refused."""
        if target.exists() or target.is_symlink():
            self.say("REFUSED", f"{source} -> {target}: target already exists; both left in place")
            self.refused += 1
            return False
        self.ensure_dir(target.parent)
        self.say(tag, f"{source} -> {target}")
        if not self.dry_run:
            os.rename(source, target)
        return True


def migrate(base: Path, rows: list[tuple[str, str]], dry_run: bool = False, out=print) -> int:
    base = base.resolve()
    if not base.is_dir():
        raise SystemExit(f"Repository base is not a directory: {base}")
    mover = Mover(base, dry_run, out)
    old_artifacts = base / REVIEW_ARTIFACTS
    moved_artifacts: set[str] = set()
    left_behind: list[str] = []
    missing: list[tuple[str, str]] = []
    in_place = 0

    for repo, org in rows:
        old, new = base / repo, base / org / repo
        if old.exists() or old.is_symlink():
            if not mover.move("MOVE", old, new):
                left_behind.append(repo)
        elif new.is_dir():
            in_place += 1
        else:
            missing.append((repo, org))

        artifacts = old_artifacts / repo
        if artifacts.exists() or artifacts.is_symlink():
            if mover.move("MOVE-ARTIFACTS", artifacts, base / org / REVIEW_ARTIFACTS / repo):
                moved_artifacts.add(repo)

        lock = base / f".{repo}.lock"
        if lock.exists() or lock.is_symlink():
            mover.say("DELETE-LOCK", str(lock))
            if not dry_run:
                lock.unlink()

    repo_ids = {repo for repo, _ in rows}
    if old_artifacts.is_dir():
        remaining = sorted(set(os.listdir(old_artifacts)) - (moved_artifacts if dry_run else set()))
        if not remaining:
            mover.say("REMOVE-EMPTY", str(old_artifacts))
            if not dry_run:
                old_artifacts.rmdir()
        for entry in remaining:
            if entry not in repo_ids:  # a row's refused move was reported above
                mover.say("ORPHAN", f"{old_artifacts / entry} (no repos row names it; left in place)")

    named = repo_ids | {org for _, org in rows}
    stale_locks = {f".{repo}.lock" for repo, _ in rows}
    for entry in sorted(os.listdir(base)):
        if entry in SHARED_DIRS or entry in named or entry in stale_locks:
            continue
        mover.say("ORPHAN", f"{base / entry} (no repos row names it; left in place)")

    for repo, org in missing:
        mover.say("NO-DIRECTORY", f"repo {repo} (org {org}) has no directory; a provider mirror re-clones it")

    # The guard that matters: no row may keep its pre-#1079 directory.
    if not dry_run:
        left_behind = [repo for repo, _ in rows if (base / repo).exists() or (base / repo).is_symlink()]
    for repo in left_behind:
        mover.say("LEFT-BEHIND", f"{base / repo} still exists")
    moved = len(rows) - in_place - len(missing) - len(left_behind)
    out(f"{'Dry run: ' if dry_run else ''}{len(rows)} rows; {moved} {'to move' if dry_run else 'moved'}, "
        f"{in_place} already in place, {len(missing)} without a directory, {len(left_behind)} left behind")
    if left_behind or mover.refused:
        out("FAILED: old repository directories or refused moves remain; "
            "do not deploy the new layout until this exits 0")
        return 1
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--base", required=True, type=Path,
                        help="repository storage base (e.g. /mnt/g5-repos/repos or /data/repos)")
    source = parser.add_mutually_exclusive_group()
    source.add_argument("--database-url", default=os.environ.get("DATABASE_URL"),
                        help="PostgreSQL URL of a role that bypasses RLS (default: $DATABASE_URL)")
    source.add_argument("--rows-file", type=Path,
                        help="file of 'repo_id org_id' lines, e.g. psql -At -F ' ' output")
    parser.add_argument("--dry-run", action="store_true", help="report what would change; change nothing")
    args = parser.parse_args(argv)
    if args.rows_file:
        rows = parse_rows(args.rows_file.read_text())
    elif args.database_url:
        rows = read_rows_from_database(args.database_url)
    else:
        parser.error("supply --database-url (or DATABASE_URL) or --rows-file")
    if not rows:
        raise SystemExit("No repos rows; refusing to treat every directory as an orphan")
    return migrate(args.base, rows, dry_run=args.dry_run)


if __name__ == "__main__":
    sys.exit(main())
