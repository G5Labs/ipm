/**
 * PR-tier selection for parametrised test matrices.
 *
 * The g5-ci-e2e pull-request check runs the reduced tier (`prTier`): one
 * representative per parameter axis, because the sibling variants exercise the
 * same code path with different parameters and each costs about a minute of
 * serial shard time. The full matrix (`all`) is the opt-in: set
 * `G5_E2E_FULL_MATRIX=1` (a future main/nightly trigger) to run every variant.
 * Test titles are identical in both tiers, so a kept variant has the same
 * id, receipts and timing history as before.
 */
export function matrix<T>(all: T[], prTier: T[]): T[] {
  return process.env.G5_E2E_FULL_MATRIX ? all : prTier;
}
