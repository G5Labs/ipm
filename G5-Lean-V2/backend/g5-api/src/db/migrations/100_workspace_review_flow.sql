-- Human reviews are explicit tasks on a submitted workspace PR. Existing
-- records remain evidence; they are never assigned to a new PR retroactively.
ALTER TABLE approval_requests ADD COLUMN pull_request_id uuid REFERENCES workspace_pull_requests(id);
ALTER TABLE approval_statuses ADD COLUMN pull_request_id uuid REFERENCES workspace_pull_requests(id),
    ADD COLUMN request_id uuid REFERENCES approval_requests(id),
    ADD COLUMN comment_id uuid REFERENCES comment_index(id) ON DELETE RESTRICT;
CREATE UNIQUE INDEX approval_review_thread ON approval_statuses(comment_id) WHERE comment_id IS NOT NULL;
CREATE INDEX approval_review_checkpoint ON approval_statuses
    (source_workspace_id, pull_request_id, actor_user_id, approval_definition_id, phase, created_at DESC);

-- Observational publication evidence, never Git head authority. A missing
-- journal record means unknown push metadata, not a fabricated commit date.
CREATE TABLE workspace_revision_events (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    observed_at timestamptz NOT NULL,
    heads jsonb NOT NULL CHECK (jsonb_typeof(heads) = 'array'),
    pushed_at jsonb NOT NULL DEFAULT '{}'::jsonb,
    FOREIGN KEY (org_id, workspace_id) REFERENCES workspaces(org_id, id) ON DELETE CASCADE
);
CREATE INDEX workspace_revision_history ON workspace_revision_events(workspace_id, observed_at DESC, id DESC);
ALTER TABLE workspace_revision_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE workspace_revision_events FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON workspace_revision_events
    USING (org_id = ANY(authz.current_org_ids())) WITH CHECK (org_id = ANY(authz.current_org_ids()));
GRANT SELECT, INSERT ON workspace_revision_events TO g5_tenant;

ALTER TABLE comment_index DROP CONSTRAINT comment_index_scope_check;
ALTER TABLE comment_index ADD CONSTRAINT comment_index_scope_check
    CHECK (scope IN ('line', 'suggestion', 'workspace'));
ALTER TABLE comment_index ADD COLUMN phase text CHECK (phase IN ('be','se','ie'));
ALTER TABLE comment_index ADD CONSTRAINT workspace_comment_without_anchor
    CHECK (scope <> 'workspace' OR (repo_id IS NULL AND file_path IS NULL AND line_number IS NULL));
