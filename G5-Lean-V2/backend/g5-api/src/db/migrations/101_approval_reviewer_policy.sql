-- Preserve immutable prior definition versions; new versions default to any human.
ALTER TABLE approval_definitions
    ADD COLUMN require_all_reviewers boolean NOT NULL DEFAULT true,
    ADD COLUMN allow_self_approval boolean NOT NULL DEFAULT false;
ALTER TABLE approval_definitions
    ALTER COLUMN require_all_reviewers SET DEFAULT false;
