#!/usr/bin/env bash
# Shared fixture construction for seeds that own dedicated repositories.
# Source after defining psql_exec. Product workspace creation uses checked forks;
# these SQL fixtures create their source cut after writing the initial Git data.

seed_create_fixture_workspace() {
  local workspace_name="$1"
  local email_sql name_sql workspace_id
  email_sql="${AUTH_EMAIL//\'/\'\'}"
  name_sql="${workspace_name//\'/\'\'}"
  workspace_id=$(python3 -c 'import uuid; print(uuid.uuid4())')
  psql_exec "
    BEGIN;
    INSERT INTO workspaces (id, org_id, name, owner_user_id, created_by)
      SELECT '${workspace_id}', '${ORG_ID}', '${name_sql}', id, id
      FROM users WHERE email = '${email_sql}';
    INSERT INTO workspace_settings (workspace_id, org_id) VALUES ('${workspace_id}', '${ORG_ID}');
    COMMIT;
  " >/dev/null || return
  printf '%s\n' "$workspace_id"
}

seed_remove_fixture_parent() {
  local child_id="$1"
  # A sibling fork holds the FK and prevents this deletion. Do not bypass it.
  psql_exec "DELETE FROM workspaces WHERE org_id = '${ORG_ID}' AND name = 'seed-source/${child_id}';" >/dev/null
}

seed_capture_fixture_parent() {
  local workspace_id="$1"
  local workspace_name="$2"
  local parent_id repo_ids repo_id
  parent_id=$(psql_exec "SELECT id FROM workspaces WHERE org_id = '${ORG_ID}' AND name = 'seed-source/${workspace_id}';") || return
  [ -n "$parent_id" ] || parent_id=$(python3 -c 'import uuid; print(uuid.uuid4())')
  workspace_name=$(psql_exec "SELECT name FROM workspaces WHERE org_id = '${ORG_ID}' AND id = '${workspace_id}';") || return
  repo_ids=$(psql_exec "SELECT repo_id::text FROM workspace_repos WHERE org_id = '${ORG_ID}' AND workspace_id = '${workspace_id}' ORDER BY repo_id;") || return
  [ -n "$repo_ids" ] || { echo 'Fixture has no repository members' >&2; return 1; }
  for repo_id in $repo_ids; do
    docker exec "$API_CONTAINER" sh -eu -c '
      repo="/data/repos/$1/$2"
      git -c safe.directory="*" -C "$repo" check-ref-format "refs/heads/$3"
      tip=$(git -c safe.directory="*" -C "$repo" rev-parse --verify HEAD^{commit})
      git -c safe.directory="*" -C "$repo" update-ref "refs/heads/$3" "$tip"
      git -c safe.directory="*" -C "$repo" update-ref "refs/heads/seed-source/$4" "$tip"
      git -c safe.directory="*" -C "$repo" symbolic-ref HEAD "refs/heads/$3"
    ' sh "$ORG_ID" "$repo_id" "$workspace_name" "$workspace_id" >/dev/null || return
  done
  psql_exec "
    BEGIN;
    INSERT INTO workspaces (id, org_id, name, owner_user_id, created_by, docs_repo_id)
      SELECT '${parent_id}', org_id, 'seed-source/${workspace_id}', owner_user_id, created_by, docs_repo_id
      FROM workspaces WHERE id = '${workspace_id}' AND org_id = '${ORG_ID}' ON CONFLICT (id) DO NOTHING;
    INSERT INTO workspace_settings (workspace_id, org_id) VALUES ('${parent_id}', '${ORG_ID}') ON CONFLICT (workspace_id) DO NOTHING;
    DELETE FROM workspace_repos parent_member
      WHERE parent_member.workspace_id = '${parent_id}' AND parent_member.org_id = '${ORG_ID}'
        AND NOT EXISTS (SELECT 1 FROM workspace_repos child_member
          WHERE child_member.workspace_id = '${workspace_id}' AND child_member.org_id = '${ORG_ID}'
            AND child_member.repo_id = parent_member.repo_id);
    INSERT INTO workspace_repos (org_id, workspace_id, repo_id, role, root_path)
      SELECT org_id, '${parent_id}', repo_id, role, root_path
      FROM workspace_repos WHERE workspace_id = '${workspace_id}' AND org_id = '${ORG_ID}'
      ON CONFLICT (workspace_id, repo_id) DO UPDATE SET role = EXCLUDED.role, root_path = EXCLUDED.root_path;
    UPDATE workspaces parent SET docs_repo_id = child.docs_repo_id
      FROM workspaces child WHERE parent.id = '${parent_id}' AND child.id = '${workspace_id}' AND child.org_id = '${ORG_ID}';
    UPDATE workspaces SET cloned_from_id = '${parent_id}' WHERE id = '${workspace_id}' AND org_id = '${ORG_ID}';
    COMMIT;
  " >/dev/null || return
}

seed_add_fixture_repo() {
  local repo_name="$1" repo_slug="$2" role="$3" repo_id
  repo_id=$(python3 -c 'import uuid; print(uuid.uuid4())')
  repo_name="${repo_name//\'/\'\'}"
  repo_slug="${repo_slug//\'/\'\'}"
  [[ "$role" = code || "$role" = docs ]] || return 1
  psql_exec "
    BEGIN;
    INSERT INTO repos (id, org_id, name, slug, git_url, default_branch)
      VALUES ('${repo_id}', '${ORG_ID}', '${repo_name}', '${repo_slug}',
        'g5-local://' || (SELECT slug FROM orgs WHERE id = '${ORG_ID}') || '/${repo_slug}', 'main');
    INSERT INTO workspace_repos (org_id, workspace_id, repo_id, role, root_path)
      VALUES ('${ORG_ID}', '${WS_ID}', '${repo_id}', '${role}', '${repo_slug}');
    UPDATE workspaces SET docs_repo_id = '${repo_id}' WHERE id = '${WS_ID}' AND '${role}' = 'docs';
    COMMIT;
  " >/dev/null || return
  printf '%s\n' "$repo_id"
}

seed_install_fixture_repo() {
  local repo_id="$1" source_dir="$2"
  local repo_dir="/data/repos/${ORG_ID}/${repo_id}"
  docker exec "$API_CONTAINER" mkdir -p "$repo_dir"
  tar -C "$source_dir" -cf - . | docker exec -i "$API_CONTAINER" tar --no-same-owner -C "$repo_dir" -xf -
  docker exec "$API_CONTAINER" chmod -R a+rwX "$repo_dir"
}
