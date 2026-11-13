-- Automatic divergence-completion retries are counted apart from queue
-- pickups (which bump `attempts` on every claim), so the retry limit and its
-- backoff see only completion attempts. Wakes merged into the row by upserts
-- keep the count; the count ends with the row.
ALTER TABLE public.remote_integration_queue
    ADD COLUMN auto_completion_attempts integer NOT NULL DEFAULT 0;
