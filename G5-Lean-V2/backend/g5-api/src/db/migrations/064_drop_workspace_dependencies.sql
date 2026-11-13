-- 064_drop_workspace_dependencies.sql — P8.5 slice 3b: drop the tables whose
-- last reader and writer this slice removes.
--
--   workspace_dependencies  the GET/PUT dependencies routes were the only
--                           writer and reader; no UI called them, the listed
--                           field was always empty, and the routes, service
--                           methods, wire field and frontend schema are gone
--   doc_folders             the docs folder create/delete routes were its only
--                           reader and writer
--   node_dependencies       the ontology deps route was its only reader; the
--                           only writer was that route's test
--
-- Dropping a table removes its triggers, policies, indexes, grants and
-- outgoing foreign keys; no other table references these three. The
-- `set_org_id_from_session` trigger function stays: other tables use it.

DROP TABLE IF EXISTS public.workspace_dependencies;
DROP TABLE IF EXISTS public.doc_folders;
DROP TABLE IF EXISTS public.node_dependencies;
