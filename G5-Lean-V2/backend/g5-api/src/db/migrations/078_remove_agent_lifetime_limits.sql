-- Agent execution has no task lifetime limit. Remove stored launch limits so
-- queued and continued tasks use the same contract as newly admitted tasks.
-- Running provider processes must be restarted by the operator with the new
-- runtime; immutable attempt evidence is intentionally left intact.
ALTER TABLE public.tasks NO FORCE ROW LEVEL SECURITY;
UPDATE public.tasks
SET spec = spec #- '{agent,limits,wallClockMs}'
WHERE kind = 'agent' AND spec #> '{agent,limits,wallClockMs}' IS NOT NULL;
SET CONSTRAINTS ALL IMMEDIATE;
ALTER TABLE public.tasks FORCE ROW LEVEL SECURITY;
