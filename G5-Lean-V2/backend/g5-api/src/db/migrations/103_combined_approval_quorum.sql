-- Combined human/agent quorum; human override is enabled by default.
-- Existing immutable versions and decisions remain unchanged.
ALTER TABLE approval_definitions
    ADD COLUMN human_approvals_override_agent boolean NOT NULL DEFAULT true;
