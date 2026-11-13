-- Run against QA and production using an authorized read-only inventory role.
-- row_security=off errors for an RLS-filtered role rather than reporting a false zero.
-- Outputs aggregates only; no comment text or personal data is selected.
BEGIN READ ONLY;
SET LOCAL row_security = off;
WITH inventory AS (
    SELECT c.id, c.resolved,
        CASE
            WHEN n.id IS NULL THEN 'missing_node'
            WHEN n.org_id IS DISTINCT FROM c.org_id
              OR n.workspace_id IS DISTINCT FROM c.workspace_id THEN 'scope_mismatch'
            WHEN n.file_path IS NULL OR btrim(n.file_path) = '' THEN 'missing_file_path'
            WHEN n.repo_id IS NULL THEN 'missing_repository'
            WHEN NOT EXISTS (
                SELECT 1 FROM workspace_repos m
                WHERE m.org_id = c.org_id AND m.workspace_id = c.workspace_id
                  AND m.repo_id = n.repo_id
            ) THEN 'unbound_repository'
            ELSE 'file_identity_available'
        END AS identity_status,
        (SELECT count(*) FROM comment_replies r WHERE r.comment_id = c.id) AS replies
    FROM comment_index c
    LEFT JOIN ontology_nodes n ON n.id = c.node_id
    WHERE c.scope = 'node'
)
SELECT coalesce(identity_status, 'TOTAL') AS identity_status,
       count(*) AS threads,
       count(*) FILTER (WHERE resolved) AS resolved_threads,
       coalesce(sum(replies), 0) AS replies
FROM inventory
GROUP BY GROUPING SETS ((identity_status), ())
ORDER BY identity_status;
ROLLBACK;
