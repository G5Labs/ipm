#!/bin/sh
# Publish completed fixture mirrors only after their SQL identities and links
# exist. <root> is the organization directory <base>/<org-id>; workspace readers
# resolve <root>/<repo-id>, never these staging paths.
set -eu
root=$1
shift
[ "$#" -gt 0 ] || { echo 'No seeded repositories to publish' >&2; exit 1; }
for repo_id do
  case "$repo_id" in ''|*[!0-9a-f-]*) echo 'Invalid seed repository id' >&2; exit 1 ;; esac
  [ ! -e "$root/$repo_id" ] || { echo "Seed destination already exists: $repo_id" >&2; exit 1; }
  git -c safe.directory='*' -C "$root/.seed-ready-$repo_id" fsck --full --strict >/dev/null
done
for repo_id do
  mv "$root/.seed-ready-$repo_id" "$root/$repo_id"
done
