-- Retire the local issue tracker and its hotlists. External issue
-- integration replaces them without data migration; platform bug reporting
-- (platform_issue_*) is a separate feature and is not touched.

-- Issue ontology discovery scheduler grants are specific to the retired
-- discovery workflow.
DROP POLICY IF EXISTS issue_discovery_scheduler_chat_messages ON public.chat_messages;
DROP POLICY IF EXISTS issue_discovery_scheduler_chat_sessions ON public.chat_sessions;
DROP POLICY IF EXISTS issue_discovery_scheduler_jobs ON public.jobs;
DROP POLICY IF EXISTS issue_discovery_scheduler_tasks ON public.tasks;

-- These system-pool read grants carry issue-discovery names but are the only
-- g5_app policies on their tables. Keep their exact semantics under neutral
-- names so retained system-pool readers are unaffected.
ALTER POLICY issue_discovery_component_lookup ON public.ontology_nodes
    RENAME TO system_ontology_node_lookup;
ALTER POLICY issue_discovery_workspace_branch_lookup ON public.workspace_branches
    RENAME TO system_workspace_branch_lookup;
ALTER POLICY issue_discovery_workspace_branch_repo_lookup ON public.workspace_branch_repos
    RENAME TO system_workspace_branch_repo_lookup;

DROP TABLE IF EXISTS public.hotlist_events;
DROP TABLE IF EXISTS public.hotlist_memberships;
DROP TABLE IF EXISTS public.hotlists;
DROP TABLE IF EXISTS public.hotlist_slug_sequence;

DROP TABLE IF EXISTS public.issue_attachments;
DROP TABLE IF EXISTS public.issue_comments;
DROP TABLE IF EXISTS public.issue_components;
DROP TABLE IF EXISTS public.issue_events;
DROP TABLE IF EXISTS public.issue_inference_jobs;
DROP TABLE IF EXISTS public.issue_job_links;
DROP TABLE IF EXISTS public.issue_labels;
DROP TABLE IF EXISTS public.issue_links;
DROP TABLE IF EXISTS public.issue_slug_sequence;
DROP TABLE IF EXISTS public.issues;
