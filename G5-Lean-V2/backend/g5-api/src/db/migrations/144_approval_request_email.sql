-- Issue #991: email a human reviewer when their approval is requested.
--
-- Account-wide delivery preference (g5.user.notifications). Default on, so
-- existing and new users are opted in until they turn it off. The delivery
-- gate reads only `users` and `org_memberships`, which carry no RLS, so this
-- kind needs no definer lookup or grant on the tenant approval tables
-- (g5.governance.approval.request-email).
ALTER TABLE public.users
    ADD COLUMN email_on_approval_request boolean NOT NULL DEFAULT true;
