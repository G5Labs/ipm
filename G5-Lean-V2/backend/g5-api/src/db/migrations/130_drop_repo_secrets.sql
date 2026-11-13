-- No current secret writer or helper caller remains (#903).
-- Verify external SQL callers and serving/rollback images before rollout;
-- see docs/db-audit-903.md. Drop the SECURITY DEFINER helper first, including
-- its PUBLIC/g5_tenant/g5_app EXECUTE grants, in this same SQLx transaction.
-- Unlike 063's inert trigger orphan, this callable helper must not survive a
-- table drop: a SQL-language body does not register a table dependency.
-- If an older owner still owns the helper, fail before removing either object.
-- The deployment owner must own it (or have the owner's privileges); do not
-- silently skip it, grant new privileges, or hide dependencies with CASCADE.
SET LOCAL lock_timeout = '5s';
DROP FUNCTION public.repo_webhook_secret_for_current_org(uuid);
DROP TABLE public.repo_secrets;
