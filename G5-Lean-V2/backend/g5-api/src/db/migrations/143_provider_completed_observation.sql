-- The provider PR number automatic refresh last observed merged or closed on
-- GitHub (g5.git.pr-refresh.completed-provider). While it equals the row's
-- provider PR number, automatic refresh refuses without reading GitHub again;
-- recording the row open (an Update that observes it reopened, or a
-- replacement provider PR) clears it.
ALTER TABLE public.workspace_pull_request_provider_repos
    ADD COLUMN provider_completed_pr_number bigint;
