-- Guided journey marks (ui g5.ui.guided-panel, g5.ui.journey-evidence): a
-- "Skip" is the owner's third explicit mark on a step, distinct from "done".
-- A skipped step leaves the phase's count without claiming completion and is
-- undone like a mark. The latest event per step still carries the whole
-- state: (done=false, skipped=true) is skipped, (done=true, skipped=false)
-- is done, (false, false) is to do. Additive DDL only
-- (docs/database-migrations.md); every existing row reads as not skipped.
ALTER TABLE public.workspace_journey_events
    ADD COLUMN skipped boolean NOT NULL DEFAULT false;
ALTER TABLE public.workspace_journey_events
    ADD CONSTRAINT workspace_journey_events_done_or_skipped_check
    CHECK (NOT (done AND skipped));
