// =============================================================================
// G5 Client — Tolerant localStorage lists
// =============================================================================
// Per-browser bookkeeping lists (recent workspace opens, open workspace tabs)
// share one idiom: a tolerant read that yields an empty list on missing,
// malformed or non-array storage and keeps only entries the caller's guard
// accepts, and a non-fatal write that never breaks the flow that called it.
// Keys, validators, caps and ordering stay with each list's own module.
// =============================================================================

/** Read a JSON array from localStorage, keeping only entries `isItem` accepts. */
export function readStoredList<T>(
  key: string,
  isItem: (value: unknown) => value is T,
): T[] {
  if (typeof localStorage === "undefined") return [];
  try {
    const raw = localStorage.getItem(key);
    if (!raw) return [];
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(isItem);
  } catch {
    return [];
  }
}

/** Write a list to localStorage; quota and serialization failures are swallowed. */
export function writeStoredList<T>(key: string, list: readonly T[]): void {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(key, JSON.stringify(list));
  } catch {
    // Fire-and-forget bookkeeping must never break navigation.
  }
}
