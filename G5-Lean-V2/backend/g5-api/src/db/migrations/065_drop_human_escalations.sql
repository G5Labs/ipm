-- 065_drop_human_escalations.sql — P8.5 slice 4: drop the persisted
-- human-escalation subsystem (design v9 §11.3).
--
-- Escalations are question documents in Git now; the routes, service and
-- WebSocket emitters that wrote these tables are gone, and this change removes
-- the last readers (the open-question metrics and Home candidate captures),
-- the data model and the frontend hook and panel that called the absent
-- routes. `human_escalation_questions` references `human_escalations`, so it
-- drops first. Dropping a table removes its triggers, policies, indexes and
-- grants; the two trigger functions that served only these tables are dropped
-- when the migrator can own them (see 063 for the populated-upgrade case).

DROP TABLE IF EXISTS public.human_escalation_questions;
DROP TABLE IF EXISTS public.human_escalations;

DO $$
DECLARE
    fn text;
BEGIN
    FOREACH fn IN ARRAY ARRAY[
        'human_escalation_questions_parent_org_guard',
        'human_escalation_questions_touch_updated_at'
    ] LOOP
        IF EXISTS (
            SELECT 1
            FROM pg_proc p
            JOIN pg_namespace n ON n.oid = p.pronamespace
            WHERE n.nspname = 'public'
              AND p.proname = fn
              AND pg_has_role(current_user, p.proowner, 'USAGE')
        ) THEN
            EXECUTE format('DROP FUNCTION public.%I()', fn);
        END IF;
    END LOOP;
END $$;
