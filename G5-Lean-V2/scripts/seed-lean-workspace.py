#!/usr/bin/env python3
"""Seed an independent, history-preserving local Lean fixture; never reset a stack."""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import urllib.error
import urllib.parse
import urllib.request
import uuid

ROOT = Path(__file__).resolve().parents[1]
MARKER = "G5 Lean local committed seed: "


def run(*args, input=None):
    return subprocess.run(args, input=input, text=True, stdout=subprocess.PIPE,
                          stderr=subprocess.PIPE, check=True).stdout.strip()


def git(repo, *args):
    return run("git", "-C", str(repo), *args)


def literal(value):
    return "'" + str(value).replace("'", "''") + "'"


def pin_source(source):
    if git(source, "rev-parse", "--is-shallow-repository") != "false":
        raise ValueError("Shallow source is unsupported; fetch full history first.")
    sha = git(source, "rev-parse", "--verify", "HEAD^{commit}")
    entries = git(source, "ls-tree", "-r", sha).splitlines()
    if any(row.startswith("160000 ") for row in entries):
        raise ValueError("Submodules are unsupported; seed ordinary repositories separately.")
    git(source, "cat-file", "-e", f"{sha}:.g5/ontology/root.g5.md")
    return sha


def stage(source, sha, directory, name):
    code, docs = directory / "code", directory / "docs"
    for repo in (code, docs):
        run("git", "init", "-q", "--bare", "--template=", str(repo))
    # Fetch only the pinned commit and ancestry. No hardlinks, alternates or remote
    # config survive, even if the source branch moves while seeding.
    git(code, "fetch", "--quiet", "--no-tags", str(source), sha)
    git(code, "update-ref", f"refs/heads/{name}", sha)
    empty_tree = run("git", "-C", str(docs), "mktree", input="")
    docs_sha = git(docs, "-c", "commit.gpgsign=false", "-c", "user.name=G5 Seed", "-c", "user.email=seed@g5.local",
                   "commit-tree", empty_tree, "-m", "Initialize empty Lean docs")
    git(docs, "update-ref", f"refs/heads/{name}", docs_sha)
    for repo in (code, docs):
        # Server-side merges advance refs without maintaining a checkout/index.
        # Bare storage avoids rejecting a later push to the seed parent branch.
        git(repo, "symbolic-ref", "HEAD", f"refs/heads/{name}")
        git(repo, "branch", "-f", "main", "HEAD")
        git(repo, "fsck", "--full", "--no-reflogs")
    if git(code, "rev-parse", "HEAD") != sha or git(docs, "ls-tree", "-r", "HEAD"):
        raise ValueError("Staged commit or empty docs verification failed.")
    return code, docs


class Stack:
    def __init__(self, api):
        url = urllib.parse.urlsplit(api)
        if url.scheme != "http" or url.hostname not in ("localhost", "127.0.0.1", "::1") or url.path not in ("", "/") or url.query or url.fragment or url.username:
            raise ValueError("Only a loopback HTTP dev API is supported.")
        self.api = api.rstrip("/")
        project = os.environ.get("G5_COMPOSE_PROJECT", "g5-dev")
        self.api_container = project + "-api"
        self.pg_container = project + "-postgres"
        self.token = None

    def sql(self, sql):
        return run("docker", "exec", "-i", self.pg_container, "psql", "-X", "-qAt",
                   "-U", "g5", "-d", "g5_dev", "-v", "ON_ERROR_STOP=1", input=sql)

    def request(self, path, data=None):
        headers = {"Content-Type": "application/json"}
        if self.token:
            headers["Authorization"] = "Bearer " + self.token
        request = urllib.request.Request(self.api + path, headers=headers,
            data=json.dumps(data).encode() if data is not None else None)
        try:
            with urllib.request.urlopen(request, timeout=120) as response:
                raw = response.read()
                return json.loads(raw).get("data") if raw else None
        except urllib.error.HTTPError as error:
            # Never print auth response bodies or bearer credentials.
            raise RuntimeError(f"API {path}: HTTP {error.code}; no successful seed claimed") from None

    def install(self, repo, org, rid):
        # Repository storage is <base>/<org-id>/<repo-id>.
        target = f"/data/repos/{org}/{rid}"
        run("docker", "exec", self.api_container, "mkdir", "-p", f"/data/repos/{org}")
        run("docker", "exec", self.api_container, "mkdir", target)
        run("docker", "cp", str(repo) + "/.", self.api_container + ":" + target)
        owner = run("docker", "exec", self.api_container, "id", "-u") + ":" + run("docker", "exec", self.api_container, "id", "-g")
        run("docker", "exec", "-u", "0", self.api_container, "chown", "-R", owner, target)
        run("docker", "exec", self.api_container, "chmod", "-R", "a+rwX", target)
        actual = run("docker", "exec", self.api_container, "git", "-C", target, "rev-parse", "HEAD")
        if actual != git(repo, "rev-parse", "HEAD"):
            raise ValueError("Installed Git head differs from staged head.")
        run("docker", "exec", self.api_container, "git", "-C", target, "fsck", "--full")


def publish_sql(org, user, name, sha, wid, code, docs):
    q = literal
    sql = f"""BEGIN;
SET LOCAL lock_timeout = '15s';
-- Also wait for in-flight name readers: a write-only table lock would let
-- a fork read "available" during this transaction and insert after it commits.
LOCK TABLE workspaces, workspace_forks IN ACCESS EXCLUSIVE MODE;
DO $$ BEGIN
IF EXISTS (SELECT 1 FROM workspaces WHERE org_id={q(org)} AND lower(name)=lower({q(name)}) AND deleted_at IS NULL)
OR EXISTS (SELECT 1 FROM workspace_forks WHERE org_id={q(org)} AND lower(proposed_name)=lower({q(name)}) AND status IN ('pending','aborting'))
THEN RAISE EXCEPTION 'Workspace name already exists or is claimed by a pending fork'; END IF;
END $$;
"""
    for rid, role in ((code, "code"), (docs, "docs")):
        slug = 'lean-' + role + '-' + rid
        git_url = f"'g5-local://' || (SELECT slug FROM orgs WHERE id={q(org)}) || {q('/' + slug)}"
        sql += f"INSERT INTO repos (id,org_id,name,slug,git_url,default_branch) VALUES ({q(rid)},{q(org)},{q(role)},{q(slug)},{git_url},'main');\n"
    sql += f"""INSERT INTO workspaces (id,org_id,name,owner_user_id,created_by,docs_repo_id,cloned_from_id,is_base,description)
VALUES ({q(wid)},{q(org)},{q(name)},{q(user)},{q(user)},{q(docs)},NULL,false,{q(MARKER+sha)});
INSERT INTO workspace_settings (workspace_id,org_id) VALUES ({q(wid)},{q(org)});
"""
    for rid, role in ((code, "code"), (docs, "docs")):
        sql += f"INSERT INTO workspace_repos (org_id,workspace_id,repo_id,role,root_path) VALUES ({q(org)},{q(wid)},{q(rid)},{q(role)},{q(role)});\n"
    return sql + "COMMIT;"


def verify(stack, wid, code_id, docs_id, source, sha):
    workspace = stack.request(f'/api/v1/workspaces/{wid}')
    if {b['repoId'] for b in workspace['branches']} != {code_id, docs_id}:
        raise ValueError("Published workspace membership differs from the staged fixture.")
    if workspace.get('clonedFromId') is not None or workspace.get('isBase') is not False:
        raise ValueError('Seed must be an ordinary standalone workspace.')
    actual = set()
    for rid in (code_id, docs_id):
        listing = stack.request(f'/api/v1/workspaces/{wid}/repos/{rid}/files?path=.g5/ontology&depth=-1')
        if listing.get('truncated') or listing.get('liveFallback') or not listing.get('source'):
            raise ValueError("Ontology discovery is incomplete or unavailable.")
        if rid == code_id and listing.get('repoOid') != sha:
            raise ValueError("API ontology revision differs from the pinned commit.")
        def files(entries):
            for entry in entries:
                if entry['type'] == 'file' and entry['path'].endswith('.md'):
                    yield rid, entry['path']
                yield from files(entry.get('children') or [])
        actual.update(files(listing['entries']))
    expected_paths = subprocess.check_output(
        ['git', '-C', str(source), 'ls-tree', '-rz', '--name-only', sha, '--', '.g5/ontology/']
    ).decode().split('\0')
    expected_paths = {p for p in expected_paths if p.endswith('.md')}
    if actual != {(code_id, p) for p in expected_paths}:
        raise ValueError("API ontology paths differ from the committed code ontology (docs should be empty).")
    root = stack.request(f'/api/v1/workspaces/{wid}/files/.g5/ontology/root.g5.md?repoId={code_id}')
    expected = subprocess.check_output(['git', '-C', str(source), 'show', f'{sha}:.g5/ontology/root.g5.md']).decode()
    if root['content'] != expected:
        raise ValueError("API ontology root differs from the pinned commit.")
    print(f"Verified {len(expected_paths)} ontology files and exact root content.", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, default=ROOT, help="local Git checkout (default: this checkout)")
    parser.add_argument("--name", default="G5-Lean")
    parser.add_argument("--org", default="g5labs", help="existing local organization slug")
    parser.add_argument("--email", default="demo@g5labs.ai", help="existing local workspace owner")
    parser.add_argument("--api-url", default="http://localhost:" + os.environ.get("G5_PORT_API", "7500"))
    parser.add_argument("--replace", type=uuid.UUID, help="explicitly delete this prior Lean seed workspace through G5 before creating the replacement")
    args = parser.parse_args()
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_-]{0,79}", args.name) or args.name.lower() in ("main", "base"):
        raise ValueError("Name must be a simple Git branch name, other than main/base (max 80 characters).")
    source = args.source.resolve()
    sha = pin_source(source)
    stack = Stack(args.api_url)
    # Ensure the chosen HTTP port belongs to this stack, not another local API.
    bindings = json.loads(run("docker", "inspect", stack.api_container, "--format", '{{json .NetworkSettings.Ports}}'))
    if str(urllib.parse.urlsplit(stack.api).port or 80) not in [p['HostPort'] for p in bindings.get('7500/tcp', []) or []]:
        raise ValueError("API URL does not match the selected Compose API container's published port.")
    row = stack.sql(f"SELECT json_build_object('org',o.id,'user',u.id) FROM orgs o JOIN org_memberships m ON m.org_id=o.id JOIN users u ON u.id=m.user_id WHERE o.slug={literal(args.org)} AND u.email={literal(args.email)};")
    if not row:
        raise ValueError("Existing organization/user membership required; run dev-up first.")
    identity = json.loads(row)
    org, user = identity['org'], identity['user']
    existing = stack.sql(f"SELECT json_build_object('id',id,'owner',owner_user_id,'description',description) FROM workspaces WHERE org_id={literal(org)} AND lower(name)=lower({literal(args.name)}) AND deleted_at IS NULL;")
    old = json.loads(existing) if existing else None
    if old and not args.replace:
        raise ValueError(f"Workspace {args.name} already exists ({old['id']}); unchanged. To replace this seed explicitly, use --replace {old['id']}.")
    if args.replace and (not old or old['id'] != str(args.replace) or old['owner'] != user or not (old['description'] or '').startswith(MARKER)):
        raise ValueError("--replace must identify the exact existing Lean seed workspace owned by this user.")
    stack.token = stack.request('/api/v1/auth/test-token', {'email': args.email})['jwt']
    if not any(o['orgId'] == org for o in stack.request('/api/v1/orgs')):
        raise ValueError("API and database organization identity do not match.")
    wid, code_id, docs_id = [str(uuid.uuid4()) for _ in range(3)]
    print(f"Source commit: {sha}\nWorkspace: {args.name}\nTarget ID: {wid}", flush=True)
    with tempfile.TemporaryDirectory(prefix='g5-lean-seed-') as tmp:
        code, docs = stage(source, sha, Path(tmp), args.name)
        print("Staged pinned history and empty docs; installing dedicated repositories.", flush=True)
        # These UUID directories are never reused. On ambiguous failure, retain
        # them for diagnosis instead of risking removal after a successful COMMIT.
        print(f"Repository IDs: code={code_id}, docs={docs_id}", flush=True)
        stack.install(code, org, code_id)
        stack.install(docs, org, docs_id)
        if args.replace:
            stack.request(f'/api/v1/workspaces/{old["id"]}/delete', {})
        stack.sql(publish_sql(org, user, args.name, sha, wid, code_id, docs_id))
        print(f"Published {wid}; verifying through the API.", flush=True)
        verify(stack, wid, code_id, docs_id, code, sha)
    url = 'http://localhost:' + os.environ.get('G5_PORT_EDGE', '3000') + '/' + urllib.parse.quote(args.org) + '/' + urllib.parse.quote(args.name)
    print(f"Verified G5-Lean seed: {url}\nCommit: {sha}\nWorkspace ID: {wid}\nCompute is stopped; use Start in G5 when needed.")


def cli():
    try:
        main()
    except (ValueError, RuntimeError, subprocess.CalledProcessError, urllib.error.URLError) as error:
        if isinstance(error, subprocess.CalledProcessError):
            # Git/SQL diagnostics are useful; never echo a full auth command.
            message = (error.stderr or '').strip() or f'Command failed with exit status {error.returncode}.'
        else:
            message = str(error)
        print('Seed failed: ' + message, file=sys.stderr)
        sys.exit(1)


if __name__ == '__main__':
    cli()
