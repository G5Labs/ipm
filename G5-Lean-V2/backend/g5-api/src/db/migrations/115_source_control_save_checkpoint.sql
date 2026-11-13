-- Only an explicit Save changes advances this observation, never ordinary Git publication.
ALTER TABLE workspaces ADD COLUMN source_control_save_generation bigint NOT NULL DEFAULT 0;
