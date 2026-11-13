-- 002_remove_workspace_search.sql
--
-- P8.9 — remove workspace search end to end (plan v9 §6, design v9 §9.5).
--
-- Every producer and consumer of the workspace-search schema is deleted in the
-- same change: the `GET /workspaces/:id/search` route, `/instance/search/status`,
-- the internal `/search/reindex` trigger, `SearchService`, the background
-- `search_indexer` task and the `search` projection consumer in
-- `projection_convergence`. Nothing reads or writes these objects after this
-- migration, so the drops below cannot race a live writer.
--
-- Retained deliberately: chat-history search (`chat_sessions`/`chat_messages`
-- GIN indexes on `to_tsvector`, which is core PostgreSQL, not pgvector),
-- editor Find, agent-local code search, and the docs/ontology projections with
-- their `projection_state` rows.
--
-- Historical usage rows are retained, including `usage_records` with
-- `request_type = 'embedding'`: they are accounting history. The CHECK
-- constraint that admits that value is deliberately left in place — no new
-- embedding usage is generated because the only producer is gone.
--
-- pgvector: `001_lean_baseline.sql` is immutable and still does
-- `CREATE EXTENSION vector` and `search_documents.embedding vector(1536)`, so
-- a fresh-install replay of that history still needs a migration-time pgvector
-- capability. The database images stay on `pgvector/pgvector:pg17` for that
-- bootstrap reason alone. This migration drops the only live objects that used
-- the extension and then the extension itself, so the running schema carries no
-- vector type, index or operator class.

-- SQLx applies each migration inside one transaction; this file adds no
-- explicit BEGIN/COMMIT, matching 001_lean_baseline.sql.

-- ---------------------------------------------------------------------------
-- 1. Search tables.
--
-- Dropping the tables removes their indexes (idx_sd_org, idx_search_branch,
-- idx_search_embedding, idx_search_fts, idx_reindex_queue_cleanup,
-- idx_reindex_queue_org_status, idx_reindex_queue_status), their
-- primary/unique/foreign-key constraints, their RLS policies
-- (tenant_isolation, tenant_projection_queue, system_projection_queue), the
-- trg_sd_set_org trigger and their g5_app/g5_tenant grants. The shared
-- set_org_id_from_session() function is used by many retained tables and
-- stays.
-- ---------------------------------------------------------------------------
DROP TABLE IF EXISTS public.search_reindex_queue;
DROP TABLE IF EXISTS public.search_documents;

-- ---------------------------------------------------------------------------
-- 2. Search-owned workspace settings.
--
-- Dropping the columns also drops workspace_settings_embedding_provider_check,
-- which constrains only embedding_provider.
-- ---------------------------------------------------------------------------
ALTER TABLE public.workspace_settings
    DROP COLUMN IF EXISTS embedding_model,
    DROP COLUMN IF EXISTS embedding_dimensions,
    DROP COLUMN IF EXISTS embedding_provider;

-- ---------------------------------------------------------------------------
-- 3. Search-owned projection checkpoints only.
--
-- 'head_cache', 'docs' and 'ontology' rows belong to retained projections.
-- ---------------------------------------------------------------------------
DELETE FROM public.projection_state WHERE projection = 'search';

-- ---------------------------------------------------------------------------
-- 4. The projection vocabulary itself.
--
-- `projection_state_projection_check` still admits 'search'. Leaving it would
-- keep a writable slot for a projection that no longer exists, so it is
-- re-created without that value: a future producer has to add a migration
-- rather than silently resurrect the consumer. The retained projections keep
-- their exact spellings.
-- ---------------------------------------------------------------------------
ALTER TABLE public.projection_state
    DROP CONSTRAINT IF EXISTS projection_state_projection_check;
ALTER TABLE public.projection_state
    ADD CONSTRAINT projection_state_projection_check
    CHECK (projection = ANY (ARRAY['docs'::text, 'ontology'::text, 'head_cache'::text]));

-- ---------------------------------------------------------------------------
-- 5. The pgvector extension, now that its only dependents are gone.
--
-- The extension is dropped only when this migration's role can drop it. In a
-- single-role database (a developer machine, a test harness) the baseline
-- created the extension and owns it, so it goes away here and the resulting
-- schema carries no pgvector objects at all.
--
-- In CI and in Cloud SQL the extension is pre-created out of band by the
-- platform admin — `scripts/ci-provision-migrator.sh` does exactly this,
-- because the deployment migrator is deliberately NOT a superuser and cannot
-- `CREATE EXTENSION` for a superuser-only extension. There the extension is
-- owned by another role and `DROP EXTENSION` raises `must be owner of
-- extension vector`. Failing the migration for that would make the whole
-- removal undeployable, so the drop is attempted and skipped with a notice.
--
-- This is not a silent partial removal: every search-owned object that *used*
-- pgvector — `search_documents.embedding` and the `idx_search_embedding` HNSW
-- index — is dropped unconditionally in section 1 above. What may remain is the
-- extension itself, still installed and therefore still supplying its own
-- types, functions, operators and operator classes, but referenced by nothing
-- in this schema. The platform admin provisioned it and only the platform admin
-- can remove it. `docs/database-migrations.md` records that the baseline still
-- needs it at migration time regardless.
--
-- Plain DROP (not CASCADE): if any object still depended on the extension the
-- drop must fail loudly rather than silently delete retained schema. A
-- dependency error is therefore re-raised, not swallowed.
-- ---------------------------------------------------------------------------
DO $drop_vector$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'vector') THEN
        BEGIN
            DROP EXTENSION vector;
        EXCEPTION
            WHEN insufficient_privilege THEN
                RAISE NOTICE
                    'pgvector left in place: it is owned by %, not by this migration role (%). Every search-owned vector object was dropped; removing the admin-provisioned extension itself is an operator action.',
                    (SELECT pg_get_userbyid(extowner) FROM pg_extension WHERE extname = 'vector'),
                    current_user;
        END;
    END IF;
END
$drop_vector$;
