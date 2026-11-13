#!/bin/sh
# Publish completed fixture repositories to an existing checked workspace
# branch. The fork service creates the member refs; this script only advances
# those exact refs after all fixture SQL identities are ready. <root> is the
# organization directory <base>/<org-id> holding both the staged
# .seed-ready-<repo-id> checkouts and the checked <repo-id> repositories.
set -eu

root=$1
branch=$2
shift 2
[ "$#" -gt 0 ] || { echo 'No seeded repositories to publish' >&2; exit 1; }
git check-ref-format "refs/heads/$branch" >/dev/null

for repo_id do
  case "$repo_id" in ''|*[!0-9a-f-]*) echo 'Invalid seed repository id' >&2; exit 1 ;; esac
  source_repo="$root/.seed-ready-$repo_id"
  target_repo="$root/$repo_id"
  [ -d "$source_repo/.git" ] || { echo "Missing seed source: $repo_id" >&2; exit 1; }
  [ -d "$target_repo" ] || { echo "Missing checked repository: $repo_id" >&2; exit 1; }
  git -c safe.directory='*' -C "$source_repo" fsck --full --strict >/dev/null
  expected=$(git -c safe.directory='*' -C "$source_repo" rev-parse HEAD)
  git -c safe.directory='*' -C "$source_repo" push --force \
    --receive-pack='git -c safe.directory=* receive-pack' \
    "file://$target_repo" "HEAD:refs/heads/$branch" >/dev/null
  # The seed receive-pack runs as root, but the API's later Git pushes run as
  # UID 10001. Git creates object fanout directories during this push; leaving
  # those directories root-owned makes a later receive-pack fail while moving
  # quarantined objects into the permanent store.
  chown -R 10001:10001 "$target_repo"
  actual=$(git -c safe.directory='*' -C "$target_repo" rev-parse "refs/heads/$branch")
  [ "$actual" = "$expected" ] || { echo "Seed ref verification failed: $repo_id" >&2; exit 1; }
done

for repo_id do
  rm -rf "$root/.seed-ready-$repo_id" "$root/.seed-$repo_id"
done
