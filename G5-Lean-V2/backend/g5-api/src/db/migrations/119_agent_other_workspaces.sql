-- Owner setting for whether a workspace's agent may change other workspaces
-- (g5.security.agent-other-workspaces). On by default; the workspace's current
-- owner may turn it off in workspace settings. A same-owner fork copies it and
-- an ownership transfer turns it off.
--
-- DDL only: the column default fills every existing workspace with on.
ALTER TABLE public.workspace_settings
    ADD COLUMN agent_may_modify_other_workspaces boolean NOT NULL DEFAULT true;
