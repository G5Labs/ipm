#!/usr/bin/env python3
"""Resolve the GCE worker image pinned to a promoted commit.

`resolve-worker-image` (cloudbuild_pipelines ontology [D30]; helm [D27];
issues #1860/#1883) selects which `g5-worker` GCE image an environment's
deploy pins via `api.env.G5_WORKER_IMAGE`, so worker hosts boot an image
consistent with the commit the environment deployed instead of the floating
family head.

The script has two mutually exclusive modes:

* Origin-build selection (`--commit`) proves the best READY family image from
  the local Git checkout using the precedence below.
* Deploy-only handoff validation (`--provided-ref`) accepts only a concrete
  image in the configured project, describes it through GCE, and requires the
  configured family plus READY status. This path never invokes Git: the origin
  build already made the selection, and promotions must carry that exact
  artifact rather than independently re-resolve it.

Origin-build selection precedence over READY family images labeled
`g5_git_short_sha`:

1. Exact: an image whose short-sha label RESOLVES IN THE LOCAL CHECKOUT to
   the promoted commit itself (newest creationTimestamp wins among rebuild
   duplicates). A bare prefix match is NOT proof — a 7-hex prefix can
   collide across commits — and the checkout always contains the promoted
   commit, so the true label always confirms; an unresolvable, ambiguous,
   or colliding label demotes the candidate to the phases below.
2. Nearest ancestor: among ALL candidates whose labeled sha resolves in the
   LOCAL checkout history and is `git merge-base --is-ancestor` of the
   promoted commit, the GRAPH-nearest one — a proven ancestor that is not
   itself an ancestor of another proven ancestor (image creation time only
   breaks ties between rebuilds of the same source commit, so a rebuilt
   image of an old commit can never outrank a newer-commit ancestor). The
   script never fetches: Cloud Build steps have no in-build GitHub token,
   so remote history is unreachable by contract. In a SHALLOW checkout a
   negative ancestry verdict is unproven (the connecting history may be
   absent) and the candidate stays eligible for the timestamp fallback;
   only a full clone's negative verdict excludes a candidate.
3. Conservative timestamp fallback (heuristic, NOT an ancestry proof),
   consulted only after EVERY candidate has been checked for provable
   ancestry — a proven ancestor outranks any unresolved candidate, even a
   newer one. Only when no candidate proves ancestry is the newest
   locally-unresolvable candidate accepted, and only iff its
   creationTimestamp is at or before the promoted commit's committer time.
   Under normal packer operation (images are built only from freshly pushed
   main commits) this excludes descendants, and it deliberately
   under-shoots — an image built after the commit landed is skipped even
   when its source commit is an ancestor. Residual, accepted risk (logged
   loudly at selection): a manually re-imported image carrying an old sha
   label, or extreme clock skew, could pass the time gate without being a
   true ancestor.
4. No candidate: exit nonzero; the calling step fails the deploy before any
   environment mutation (fail-closed).

Output (stdout, single line): projects/<project>/global/images/<name>
Diagnostics go to stderr. Exit codes: 0 = resolved; 1 = no eligible image or
acquisition failure.

Stdlib only; must stay compatible with the Python <= 3.10 shipped in
gcr.io/cloud-builders images (creationTimestamp keeps millisecond precision,
which `datetime.fromisoformat` accepts on 3.7+; never feed it nanoseconds).
"""
from __future__ import annotations

import argparse
import datetime
import json
import re
import subprocess
import sys
from typing import Callable, List, Optional, Sequence, Tuple

from release_artifacts import ADMIN_ARTIFACT_PROJECT, QA_BUILD_PROJECT, resolve_artifact_project

Runner = Callable[[Sequence[str]], Tuple[int, str, str]]

_IMAGE_NAME_RE = re.compile(r"^[a-z][-a-z0-9]{0,61}[a-z0-9]$")
_SHA_RE = re.compile(r"^[0-9a-f]{7,40}$")


def _default_runner(cmd: Sequence[str]) -> Tuple[int, str, str]:
    proc = subprocess.run(
        list(cmd),
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        timeout=120,
    )
    return proc.returncode, proc.stdout, proc.stderr


def _diag(stderr: str) -> str:
    # Bounded, single-line diagnostic excerpt so operators can tell an IAM
    # denial or repo-safety failure from "no matching image" (no secrets are
    # ever passed on these command lines).
    excerpt = " ".join(stderr.strip().split())
    return f": {excerpt[:300]}" if excerpt else ""


def _log(message: str) -> None:
    print(message, file=sys.stderr)


def list_family_images(project: str, family: str, runner: Runner) -> List[dict]:
    rc, out, err = runner(
        [
            "gcloud",
            "compute",
            "images",
            "list",
            f"--project={project}",
            "--no-standard-images",
            f"--filter=family={family} AND status=READY",
            "--format=json(name,labels,creationTimestamp)",
        ]
    )
    if rc != 0:
        raise RuntimeError(f"gcloud compute images list failed (exit {rc}){_diag(err)}")
    return json.loads(out or "[]")


def validate_promoted_image(
    project: str,
    family: str,
    image_ref: str,
    runner: Runner,
) -> dict:
    prefix = f"projects/{project}/global/images/"
    if not image_ref.startswith(prefix):
        raise ValueError(
            f"provided worker image must be a concrete image in project {project!r}"
        )
    name = image_ref[len(prefix) :]
    if not _IMAGE_NAME_RE.fullmatch(name):
        raise ValueError(f"provided worker image has invalid concrete image name {name!r}")

    rc, out, err = runner(
        [
            "gcloud",
            "compute",
            "images",
            "describe",
            name,
            f"--project={project}",
            "--format=json(name,family,status,labels)",
        ]
    )
    if rc != 0:
        raise RuntimeError(
            f"gcloud compute images describe failed (exit {rc}){_diag(err)}"
        )
    image = json.loads(out or "{}")
    if not isinstance(image, dict):
        raise ValueError("worker image describe returned a non-object payload")
    if image.get("name") != name:
        raise ValueError(
            f"worker image describe returned name {image.get('name')!r}, expected {name!r}"
        )
    if image.get("status") != "READY":
        raise ValueError(
            f"worker image {name!r} has status {image.get('status')!r}, expected 'READY'"
        )
    if image.get("family") != family:
        raise ValueError(
            f"worker image {name!r} has family {image.get('family')!r}, expected {family!r}"
        )
    if project == ADMIN_ARTIFACT_PROJECT:
        validate_image_provenance(image, project, QA_BUILD_PROJECT, "us-central1", runner)
    return image


def validate_image_provenance(image, image_project, build_project, region, runner, *, require_success=True):
    """Admin-hosted images must still originate from the trusted QA bake.

    Legacy QA images lack these labels and remain eligible in the unchanged QA
    lane. New admin images have no legacy exception or timestamp-only proof.
    """
    resolve_artifact_project(build_project, image_project)
    labels = image.get("labels") or {}
    build_id, commit = labels.get("g5_build_id", ""), labels.get("g5_git_commit", "")
    if (labels.get("g5_build_project") != build_project or not re.fullmatch(r"[0-9a-f-]{36}", build_id)
            or not re.fullmatch(r"[0-9a-f]{40}", commit) or labels.get("g5_git_short_sha") != commit[:7]):
        raise ValueError("Admin worker image lacks trusted QA build identity")
    rc, raw, err = runner(["gcloud", "builds", "describe", build_id,
                           f"--project={build_project}", f"--region={region}", "--format=json"])
    if rc:
        raise RuntimeError(f"cannot verify worker build provenance{_diag(err)}")
    build = json.loads(raw)
    rc, trigger_id, err = runner(["gcloud", "builds", "triggers", "describe", "g5-packer-worker-main-qa",
                                  f"--project={build_project}", f"--region={region}", "--format=value(id)"])
    if rc or not trigger_id.strip():
        raise RuntimeError(f"cannot resolve trusted worker trigger{_diag(err)}")
    provenance, source = build.get("sourceProvenance") or {}, build.get("source") or {}
    resolved = [provenance.get("resolvedRepoSource", {}).get("commitSha"),
                provenance.get("resolvedGitSource", {}).get("revision"),
                provenance.get("resolvedConnectedRepository", {}).get("revision"),
                source.get("gitSource", {}).get("revision"), source.get("connectedRepository", {}).get("revision")]
    sha = next((value for value in resolved if value and re.fullmatch(r"[0-9a-f]{40}", value)), None)
    substitutions = build.get("substitutions") or {}
    if (sha != commit or build.get("buildTriggerId") != trigger_id.strip()
            or build.get("serviceAccount", "").split("/")[-1] != f"g5-lean-images@{build_project}.iam.gserviceaccount.com"
            or substitutions.get("_ARTIFACT_PROJECT") != image_project
            or substitutions.get("_GCE_IMAGE_PROJECT") != image_project
            or substitutions.get("_AR_PROJECT") != image_project
            or substitutions.get("_RUN_FULL_BUILD") != "true"):
        raise ValueError("Worker image build source, producer or artifact ownership does not match")
    allowed = {"SUCCESS"} if require_success else {"SUCCESS", "WORKING", "QUEUED"}
    if build.get("status") not in allowed:
        raise ValueError("Worker image source build has not succeeded")
    return build


def creation_epoch(image: dict) -> float:
    # GCE returns RFC3339; python<=3.10 fromisoformat rejects a trailing 'Z',
    # so normalize it. A timezone-naive value would silently be compared in
    # the builder's local timezone — reject it instead.
    raw = image["creationTimestamp"]
    parsed = datetime.datetime.fromisoformat(raw.replace("Z", "+00:00"))
    if parsed.tzinfo is None:
        raise ValueError(f"timezone-naive creationTimestamp {raw!r} on {image.get('name')!r}")
    return parsed.timestamp()


def commit_committer_epoch(repo_dir: str, commit: str, runner: Runner) -> int:
    rc, out, err = runner(["git", "-C", repo_dir, "show", "-s", "--format=%ct", commit])
    if rc != 0 or not out.strip():
        raise RuntimeError(f"cannot read committer time of {commit} in {repo_dir}{_diag(err)}")
    return int(out.strip().splitlines()[-1])


def resolve_local_sha(repo_dir: str, short_sha: str, runner: Runner) -> Optional[str]:
    # One classifier for every phase: `--disambiguate` lists ALL local
    # objects matching the prefix with a deterministic exit code, so an
    # ambiguous prefix (or a shallow-checkout miss) demotes the candidate
    # (None) instead of aborting the deploy, while a genuine git operational
    # failure still raises — never silently degraded into the heuristic.
    rc, out, err = runner(["git", "-C", repo_dir, "rev-parse", f"--disambiguate={short_sha}"])
    if rc != 0:
        raise RuntimeError(
            f"git rev-parse --disambiguate failed (exit {rc}) for {short_sha!r} in {repo_dir}{_diag(err)}"
        )
    commits = []
    for oid in out.split():
        rc2, kind, err2 = runner(["git", "-C", repo_dir, "cat-file", "-t", oid])
        if rc2 != 0:
            raise RuntimeError(f"git cat-file -t failed (exit {rc2}) for {oid} in {repo_dir}{_diag(err2)}")
        if kind.strip() == "commit":
            commits.append(oid)
    if len(commits) == 1:
        return commits[0]
    if len(commits) > 1:
        _log(f"prefix {short_sha!r} is ambiguous locally ({len(commits)} commits); demoting")
    return None


def is_ancestor(repo_dir: str, ancestor: str, descendant: str, runner: Runner) -> bool:
    # rc 0 = ancestor; rc 1 = not an ancestor; anything else is fatal.
    rc, _, err = runner(
        ["git", "-C", repo_dir, "merge-base", "--is-ancestor", ancestor, descendant]
    )
    if rc == 0:
        return True
    if rc == 1:
        return False
    raise RuntimeError(
        f"git merge-base --is-ancestor failed (exit {rc}) for {ancestor} vs {descendant}{_diag(err)}"
    )


def is_shallow_repo(repo_dir: str, runner: Runner) -> bool:
    rc, out, err = runner(["git", "-C", repo_dir, "rev-parse", "--is-shallow-repository"])
    if rc != 0:
        raise RuntimeError(f"git rev-parse --is-shallow-repository failed (exit {rc}){_diag(err)}")
    return out.strip() == "true"


def eligible_candidates(images: List[dict]) -> List[dict]:
    candidates = []
    for image in images:
        name = image.get("name", "")
        label = (image.get("labels") or {}).get("g5_git_short_sha", "")
        if not _IMAGE_NAME_RE.match(name):
            _log(f"skipping image with unexpected name: {name!r}")
            continue
        if not _SHA_RE.match(label):
            _log(f"skipping {name}: missing/invalid g5_git_short_sha label {label!r}")
            continue
        candidates.append(image)
    candidates.sort(key=creation_epoch, reverse=True)
    return candidates


def select_image(
    candidates: List[dict],
    commit: str,
    repo_dir: str,
    runner: Runner,
) -> Tuple[dict, str]:
    # Exact requires the label to RESOLVE to the promoted commit locally —
    # prefix match alone is not proof (7-hex collisions exist). The checkout
    # always contains the promoted commit, so the true label confirms;
    # anything else demotes into the phases below.
    for candidate in candidates:
        label = candidate["labels"]["g5_git_short_sha"]
        if not commit.startswith(label):
            continue
        if resolve_local_sha(repo_dir, label, runner) == commit:
            return candidate, "exact"

    # Phase 1: check EVERY candidate for provable ancestry — a proven
    # ancestor outranks any unresolved candidate, even a newer one. Among
    # proven ancestors pick the GRAPH-nearest commit (not the newest-built
    # image: a rebuild of an old commit must not outrank a newer-commit
    # ancestor); creation order only breaks rebuild ties per commit. In a
    # SHALLOW checkout a negative merge-base verdict is NOT proof — the
    # connecting history may simply be absent — so shallow negatives join
    # the timestamp pool; only a full clone's negative excludes a candidate.
    shallow = is_shallow_repo(repo_dir, runner)
    unresolved: List[dict] = []
    proven: List[Tuple[dict, str]] = []
    for candidate in candidates:
        label = candidate["labels"]["g5_git_short_sha"]
        full = resolve_local_sha(repo_dir, label, runner)
        if full is None:
            unresolved.append(candidate)
            continue
        if is_ancestor(repo_dir, full, commit, runner):
            proven.append((candidate, full))
        elif shallow:
            _log(
                f"{candidate['name']}: negative ancestry in a shallow checkout "
                "is unproven; keeping it eligible for the timestamp fallback"
            )
            unresolved.append(candidate)
    if proven:
        # candidates are newest-created-first, so the first entry per sha is
        # that commit's newest rebuild.
        newest_per_sha: dict = {}
        for cand, full in proven:
            newest_per_sha.setdefault(full, cand)
        shas = list(newest_per_sha)
        for sha in shas:
            if all(
                other == sha or not is_ancestor(repo_dir, sha, other, runner)
                for other in shas
            ):
                return newest_per_sha[sha], "ancestor"

    # Phase 2: heuristic only when no candidate proves ancestry (candidates
    # are newest-first, so the first time-eligible unresolved one wins).
    commit_epoch = commit_committer_epoch(repo_dir, commit, runner)
    for candidate in unresolved:
        if creation_epoch(candidate) <= commit_epoch:
            _log(
                f"{candidate['name']}: sha "
                f"{candidate['labels']['g5_git_short_sha']} not in local "
                "history (shallow checkout) and no candidate proves "
                "ancestry; accepting via conservative timestamp fallback"
            )
            return candidate, "timestamp-fallback"
    raise LookupError(
        f"no eligible worker image at or before commit {commit}: "
        f"{len(candidates)} labeled candidates, none exact, ancestor, or "
        "created before the commit's committer time"
    )


def main(argv: Optional[Sequence[str]] = None, runner: Runner = _default_runner) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--project", required=True)
    parser.add_argument("--build-project", help="QA coordinator; validates the approved artifact owner")
    parser.add_argument("--family", required=True)
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument("--commit", help="full promoted COMMIT_SHA")
    source.add_argument(
        "--provided-ref",
        help="concrete origin-build worker image ref to validate for deploy-only use",
    )
    parser.add_argument("--repo-dir", default=".", help="checkout with the promoted commit")
    args = parser.parse_args(argv)

    if args.commit is not None and not re.match(r"^[0-9a-f]{40}$", args.commit):
        _log(f"--commit must be a full 40-hex sha, got {args.commit!r}")
        return 1

    try:
        if args.build_project:
            resolve_artifact_project(args.build_project, args.project)
        if args.provided_ref is not None:
            image = validate_promoted_image(
                args.project,
                args.family,
                args.provided_ref,
                runner,
            )
            _log(
                f"validated promoted worker image {image['name']} "
                f"(family={image['family']}, status={image['status']})"
            )
            print(args.provided_ref)
            return 0

        images = list_family_images(args.project, args.family, runner)
        candidates = eligible_candidates(images)
        if not candidates:
            _log(f"no READY sha-labeled images in family {args.family!r}")
            return 1
        chosen, mode = select_image(candidates, args.commit, args.repo_dir, runner)
        if args.project == ADMIN_ARTIFACT_PROJECT:
            if mode == "timestamp-fallback":
                raise ValueError("Admin worker images require proven source ancestry")
            validate_image_provenance(chosen, args.project, QA_BUILD_PROJECT, "us-central1", runner)
    except (
        RuntimeError,
        LookupError,
        ValueError,
        KeyError,
        OSError,
        subprocess.TimeoutExpired,
    ) as exc:
        _log(f"resolve-worker-image failed: {exc}")
        return 1

    _log(
        f"selected {chosen['name']} (mode={mode}, "
        f"sha={chosen['labels']['g5_git_short_sha']}, "
        f"created={chosen['creationTimestamp']})"
    )
    print(f"projects/{args.project}/global/images/{chosen['name']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
