-- Issue #1101: legacy Microsoft links used sub and unverified email hints.
-- They cannot establish the new validated tenant/object identity.
-- As of 2026-11-05 no Entra login has run in any deployment (issue assumption),
-- so no session or credential revocation accompanies this safety net.
-- users is global and has no RLS; this requires no temporary definer role.
DO $$
DECLARE
    cleared bigint;
BEGIN
    UPDATE public.users
       SET oauth_provider = NULL, oauth_id = NULL
     WHERE oauth_provider = 'microsoft';
    GET DIAGNOSTICS cleared = ROW_COUNT;
    RAISE NOTICE 'Cleared % legacy Microsoft account links (G5-Lean#1101)', cleared;
END $$;
