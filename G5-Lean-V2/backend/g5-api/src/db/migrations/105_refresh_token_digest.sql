-- Refresh credentials are stored as digests (G5-Lean#660,
-- g5.user.session-authority). The plaintext bearer is returned once to the
-- client that logged in; the database keeps only its hex SHA-256, so a read
-- of these rows or of a backup cannot mint sessions, as api_keys.key_hash
-- already guarantees for delegated keys.
--
-- Existing rows are rewritten in place so valid sessions survive the upgrade:
-- the credential a browser or CLI still holds hashes to the rewritten primary
-- key, and its idle and absolute lifetimes are untouched. The rewrite is
-- one-way by design; the plaintext is not recoverable afterwards.
ALTER TABLE public.refresh_tokens RENAME COLUMN token TO token_hash;

UPDATE public.refresh_tokens
   SET token_hash = encode(sha256(convert_to(token_hash, 'UTF8')), 'hex');

COMMENT ON COLUMN public.refresh_tokens.token_hash IS
    'Hex SHA-256 of the refresh credential (G5-Lean#660); the plaintext bearer is never stored.';
