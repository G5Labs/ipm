-- Start settlement (g5.runtime.start.settlement, G5-Lean#626).
--
-- A placed incarnation whose Start command may still reach its host cannot be
-- released on host absence alone. The lifecycle owner stamps this column under
-- the incarnation's row lock immediately before it enqueues `vm.start` on the
-- host connection; a release that finds no VM on the host fences on the stamp
-- it read (never sent: nothing can land; sent: the host saw the Start before
-- it answered). Placement and every custody release clear it.
--
-- Schema only; existing rows are NULL, which the release rule reads as a Start
-- that was never recorded as sent. Rows already `starting` at deploy keep the
-- previous behavior for that one incarnation.
ALTER TABLE public.workspace_vms
    ADD COLUMN start_command_sent_at timestamptz;

COMMENT ON COLUMN public.workspace_vms.start_command_sent_at IS
    'When the lifecycle owner enqueued vm.start for the current incarnation; NULL until then and after release (g5.runtime.start.settlement).';
