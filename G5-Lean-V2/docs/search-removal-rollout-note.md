# Workspace search removal — rollout note (draft, not sent)

Prepared for David and the team as part of P8.9 (plan v9 §6, design v9 §9.5).
**This note has not been sent.** Sending it is a separate, user-authorized
release action; this PR only prepares the text.

## What is being removed

Workspace-wide search is removed end to end in the next release:

- the search box in the workspace top bar, its results panel, the detachable
  floating panel and the mobile search row;
- `GET /api/v1/workspaces/:id/search` and the authenticated
  `GET /api/v1/instance/search/status`;
- the internal one-shot `POST /api/internal/workspaces/:id/search/reindex`
  trigger and the `SEARCH_UNAVAILABLE` error code;
- the background search indexer, its reindex queue and the `search` projection
  consumer in the convergence loop;
- text/vector indexing, the OpenAI/local embedding calls they made, and the
  per-workspace `embeddingModel` / `embeddingDimensions` / `embeddingProvider`
  settings;
- the `search_documents` and `search_reindex_queue` tables, and every schema
  object that used pgvector (see the operational notes for the extension
  itself).

No replacement search feature is promised.

## What is **not** affected

- **Chat-history search.** `GET /api/v1/workspaces/:id/sessions/search` and the
  History control in the chat panel keep working; the Postgres full-text
  indexes behind them are untouched.
- **Editor Find.** Monaco's in-file find/replace is unchanged.
- **Agent-local code search.** An agent still runs `rg` and its own
  `semantic_search` tool over the workspace checkout inside its runtime.
  Unrelated MCP tools and agent skill allowlists are unchanged.
- **Docs and ontology.** The document and ontology projections, their
  convergence loop, graphs, grounding and navigation are unchanged; only the
  search consumer was removed from the same loop.
- **Usage and billing history.** Existing `usage_records` rows, including
  historical `embedding` request types, are retained. Shared provider
  credentials (for example `OPENAI_API_KEY`, which Codex also uses) are
  retained.

## What users will notice

The workspace top bar no longer has a search field, on desktop or mobile.
Anyone who used it to jump to a file, ontology node or document now browses the
left navigation — `Core Ontology`, `Project Docs`, `Explainers`, `Archive`,
`Source Control`, `Issues`, `Task Queue` and `Files` — and expands the relevant
tree. Those sections are browsed, not searched; the only additional filter
there is Project Docs' `Source` category filter, which narrows by origin rather
than by name. The workspace switcher still filters workspaces by name, and the
chat panel's History control still filters chat sessions. Saved links and
bookmarks to search results were never durable URLs, so nothing bookmarked
breaks.

## Operational notes

- The release includes a forward migration, `002_remove_workspace_search.sql`.
  It drops the two search tables, the three `workspace_settings` embedding
  columns and the `search`-owned `projection_state` rows. It also drops the
  pgvector extension where it can: the deployment migrator is not a superuser
  and does not own the admin-provisioned extension, so on Cloud SQL it stays
  installed — still providing its own types and operator classes — but
  referenced by nothing. Every application object that used it is gone either
  way; removing the extension itself is an operator action, not a migration
  one.
- **Stop the application before migrating.** A prior-version `g5-api` or
  `g5-api-indexer` still running would query dropped tables. The self-hosted
  `just upgrade` already stops the app services before `db-migrate`; the local
  sequence is documented in `docs/just-commands.md`.
- The database image stays `pgvector/pgvector:pg17`. The immutable baseline
  migration still creates the extension, so a fresh install replaying migration
  history needs pgvector available at migration time. That is a bootstrap
  requirement only — the running schema after migration 002 has no vector type,
  index or operator class, and no runtime indexing or embedding code exists.
- Seed and demo scripts no longer call the reindex endpoint or set an embedding
  provider; they are otherwise unchanged.
