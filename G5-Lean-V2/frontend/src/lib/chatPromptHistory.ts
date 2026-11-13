// =============================================================================
// Chat prompt history in this browser
// =============================================================================
// Prompts a person submitted from the Chat composer, newest first, kept in
// localStorage per person, workspace and chat so Up/Down recall only the
// current chat's prompts (chat g5.chat.ui.prompt-history and its .store
// decision). They never leave the browser, so a VM stop, restart or
// replacement cannot lose them, and they are cleared on sign-out. Navigation
// is a pure step over a snapshot of the entries, so it never submits and a
// recalled prompt is edited like any other draft.
// =============================================================================

export const CHAT_PROMPT_HISTORY_PREFIX = "g5:chat-prompt-history:";
export const MAX_CHAT_PROMPT_HISTORY = 100;
/** Longer prompts (pasted logs, files) are sent but not recorded. */
export const MAX_CHAT_PROMPT_HISTORY_CHARS = 20_000;
/** Browser-wide bounds: per-chat buffers must not exhaust shared storage. */
export const MAX_CHAT_PROMPT_HISTORY_CHATS = 200;
export const MAX_CHAT_PROMPT_HISTORY_TOTAL_CHARS = 1_000_000;

/** Whose history, without the chat: the owner supplies it, the panel adds
 * the chat it shows. */
export interface ChatPromptHistoryScope {
  userId: string;
  workspaceId: string;
}

export function chatPromptHistoryKey(
  userId: string,
  workspaceId: string,
  sessionId: string,
): string {
  return `${CHAT_PROMPT_HISTORY_PREFIX}${userId}:${workspaceId}:${sessionId}`;
}

interface StoredHistory {
  /** When a prompt was last recorded here; the least recent is evicted first. */
  usedAt: number;
  entries: string[];
}

// The page's history when browser storage cannot be used at all.
const pageHistory = new Map<string, StoredHistory>();
let legacyDiscarded = false;

function storage(): Storage | null {
  try {
    return typeof localStorage === "undefined" ? null : localStorage;
  } catch {
    return null;
  }
}

function parseStored(raw: string | null): StoredHistory | null {
  if (!raw) return null;
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
      return null;
    }
    const { usedAt, entries } = parsed as Record<string, unknown>;
    if (!Array.isArray(entries)) return null;
    return {
      usedAt:
        typeof usedAt === "number" && Number.isFinite(usedAt) ? usedAt : 0,
      entries: entries.filter(
        (entry): entry is string => typeof entry === "string",
      ),
    };
  } catch {
    return null;
  }
}

function historyKeys(store: Storage): string[] {
  const keys: string[] = [];
  for (let index = 0; index < store.length; index += 1) {
    const key = store.key(index);
    if (key?.startsWith(CHAT_PROMPT_HISTORY_PREFIX)) keys.push(key);
  }
  return keys;
}

/** A workspace-wide buffer from before history was per chat: `user:workspace`. */
function isLegacyKey(key: string): boolean {
  return key.slice(CHAT_PROMPT_HISTORY_PREFIX.length).split(":").length === 2;
}

/** Earlier workspace-wide buffers mix every chat's prompts and carry no chat
 * identity, so they are dropped rather than copied into any chat. */
function discardLegacyHistory(store: Storage): void {
  if (legacyDiscarded) return;
  legacyDiscarded = true;
  for (const key of historyKeys(store)) {
    if (isLegacyKey(key)) store.removeItem(key);
  }
}

export function readChatPromptHistory(key: string): string[] {
  // A failed write keeps the newest history in page memory.
  const page = pageHistory.get(key);
  if (page) return [...page.entries];
  const store = storage();
  if (store) {
    try {
      discardLegacyHistory(store);
      const stored = parseStored(store.getItem(key));
      if (stored) return stored.entries;
    } catch {
      // Fall through to the page's history.
    }
  }
  return [];
}

/** The history with `prompt` recorded as its newest entry. An earlier
 * identical entry moves to the front instead of repeating. */
export function withRecordedPrompt(
  entries: readonly string[],
  prompt: string,
): string[] {
  if (!prompt || prompt.length > MAX_CHAT_PROMPT_HISTORY_CHARS) {
    return [...entries];
  }
  return [prompt, ...entries.filter((entry) => entry !== prompt)].slice(
    0,
    MAX_CHAT_PROMPT_HISTORY,
  );
}

/** Serialize `entries`, dropping the oldest until it fits the total budget. */
function serializeWithinBudget(entries: string[], usedAt: number): string {
  let kept = entries;
  let raw = JSON.stringify({ usedAt, entries: kept });
  while (raw.length > MAX_CHAT_PROMPT_HISTORY_TOTAL_CHARS && kept.length > 1) {
    kept = kept.slice(0, -1);
    raw = JSON.stringify({ usedAt, entries: kept });
  }
  return raw;
}

/** Evict the least recently recorded chats, never `current`, until the
 * browser holds at most the chat and character budgets. */
function enforceBudget(store: Storage, current: string): void {
  const buffers = historyKeys(store).map((key) => {
    const raw = store.getItem(key) ?? "";
    return { key, size: raw.length, usedAt: parseStored(raw)?.usedAt ?? 0 };
  });
  let count = buffers.length;
  let total = buffers.reduce((sum, buffer) => sum + buffer.size, 0);
  const evictable = buffers
    .filter((buffer) => buffer.key !== current)
    .sort((left, right) => left.usedAt - right.usedAt);
  for (const buffer of evictable) {
    if (
      count <= MAX_CHAT_PROMPT_HISTORY_CHATS &&
      total <= MAX_CHAT_PROMPT_HISTORY_TOTAL_CHARS
    ) {
      break;
    }
    store.removeItem(buffer.key);
    count -= 1;
    total -= buffer.size;
  }
}

export function recordChatPromptHistory(key: string, prompt: string): void {
  // A prompt that cannot be recalled must not hold one of the chat slots.
  if (!prompt || prompt.length > MAX_CHAT_PROMPT_HISTORY_CHARS) return;
  const usedAt = Date.now();
  const entries = withRecordedPrompt(readChatPromptHistory(key), prompt);
  const store = storage();
  if (store) {
    try {
      store.setItem(key, serializeWithinBudget(entries, usedAt));
      enforceBudget(store, key);
      pageHistory.delete(key);
      return;
    } catch {
      // A full or unavailable store: free space once, then keep the page's copy.
      try {
        enforceBudget(store, key);
        store.setItem(key, serializeWithinBudget(entries, usedAt));
        pageHistory.delete(key);
        return;
      } catch {
        // Keep the history for this page instead.
      }
    }
  }
  pageHistory.set(key, { usedAt, entries });
}

/** Remove every person's chat prompt history stored in this browser. */
export function clearChatPromptHistory(): void {
  pageHistory.clear();
  const store = storage();
  if (!store) return;
  try {
    for (const key of historyKeys(store)) store.removeItem(key);
  } catch {
    // Storage unavailable: there is nothing to clear.
  }
}

/** Test seam: forget the once-per-page legacy discard. */
export function resetChatPromptHistoryForTests(): void {
  legacyDiscarded = false;
  pageHistory.clear();
}

/**
 * Where the composer is in its history. `recalled` is the text navigation
 * last placed in the composer: once the composer holds anything else (an
 * edit, a seed, a send), browsing has ended and that text is the draft.
 */
export interface PromptHistoryCursor {
  /** Snapshot of the entries when browsing began, newest first. */
  entries: readonly string[];
  index: number;
  /** The draft to restore after moving past the newest entry. */
  draft: string;
  recalled: string;
}

export interface PromptHistoryStep {
  cursor: PromptHistoryCursor | null;
  value: string;
}

/** One Up ("older") or Down ("newer") step, or null at a boundary. */
export function stepPromptHistory(
  entries: readonly string[],
  cursor: PromptHistoryCursor | null,
  value: string,
  direction: "older" | "newer",
): PromptHistoryStep | null {
  const active = cursor && cursor.recalled === value ? cursor : null;
  if (direction === "newer") {
    if (!active) return null;
    const index = active.index - 1;
    if (index < 0) return { cursor: null, value: active.draft };
    const recalled = active.entries[index];
    return { cursor: { ...active, index, recalled }, value: recalled };
  }
  const snapshot = active ? active.entries : entries;
  const index = active ? active.index + 1 : 0;
  if (index >= snapshot.length) return null;
  const recalled = snapshot[index];
  return {
    cursor: {
      entries: snapshot,
      index,
      draft: active ? active.draft : value,
      recalled,
    },
    value: recalled,
  };
}
