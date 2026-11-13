import type { ChatMessageDTO, TimelineEntry } from "./types";

/**
 * Shared provided-agent output projection (core_workflow/chat_agent_session#D18).
 *
 * Legacy provided-agent output persists two forms of durable rows per turn:
 * per-text-block semantic deltas (`provided_agent.output.delta`,
 * `streamPersistence: "semantic"`) and one aggregate final
 * (`provided_agent.output.final`). Both rows stay durable for audit/replay. Every
 * user-facing answer surface — the chat transcript (ChatPanel), the
 * execution-drawer Timeline, and the Terminal trace — must render ONE logical
 * answer per turn by keeping the delta rows in place and suppressing the
 * aggregate final row whenever a same-turn persisted delta exists. Chat, Timeline, and
 * Terminal derive from this single helper so the three surfaces cannot drift
 * (issue #1313).
 *
 * Native task-journal message.final events instead close individual text
 * blocks. Replace only the preceding same-task/attempt incremental run;
 * retain later narration and answers rather than applying legacy turn-wide
 * suppression. Native block IDs pair output across tool activity; old journal
 * rows without them use adjacent output as a conservative fallback.
 */

const OUTPUT_DELTA_EVENT = "provided_agent.output.delta";
const OUTPUT_FINAL_EVENT = "provided_agent.output.final";

/** The parts of an entry's chronological key, computed once per entry
 * object: a transcript of thousands of rows is sorted and scanned on every
 * commit, and parsing the timestamp each comparison dominated that cost. */
interface EntryOrderKey {
  milliseconds: number;
  fraction: string;
  sequence: number;
}

const orderKeys = new WeakMap<object, EntryOrderKey>();

function entryOrderKey(
  entry: Pick<TimelineEntry, "createdAt" | "sequence">,
): EntryOrderKey {
  const cached = orderKeys.get(entry);
  if (cached) return cached;
  const key: EntryOrderKey = {
    milliseconds: Date.parse(entry.createdAt),
    fraction: (entry.createdAt.match(/\.(\d+)/)?.[1] ?? "").padEnd(9, "0"),
    sequence: entry.sequence ?? 0,
  };
  orderKeys.set(entry, key);
  return key;
}

/** REST and WS use different RFC3339 suffixes and fractional precision.
 * Date.parse compares instants; the fractional tie-break keeps sub-ms order.
 */
export function compareTimelineEntries(
  a: Pick<TimelineEntry, "createdAt" | "sequence">,
  b: Pick<TimelineEntry, "createdAt" | "sequence">,
): number {
  const left = entryOrderKey(a);
  const right = entryOrderKey(b);
  const milliseconds = left.milliseconds - right.milliseconds;
  if (!Number.isFinite(milliseconds))
    return a.createdAt.localeCompare(b.createdAt);
  return (
    milliseconds ||
    left.fraction.localeCompare(right.fraction) ||
    left.sequence - right.sequence
  );
}

/** The entries in chronological order: the input itself when it already is
 * (the placement path keeps the held window ordered), otherwise a sorted
 * copy. Called on every commit, so it must not copy an ordered list. */
export function sortedTimelineEntries(
  entries: readonly TimelineEntry[],
): TimelineEntry[] {
  for (let index = 1; index < entries.length; index += 1) {
    if (compareTimelineEntries(entries[index - 1], entries[index]) > 0)
      return [...entries].sort(compareTimelineEntries);
  }
  return entries as TimelineEntry[];
}

/** The projected copy of a native row, reused across commits while its
 * derived block id and anchor are unchanged, so an unchanged turn keeps its
 * entries by identity (chat g5.chat.live-tail-recovery). */
const nativeProjections = new WeakMap<
  TimelineEntry,
  {
    blockId: string;
    anchor: TimelineEntry | undefined;
    projected: TimelineEntry;
  }
>();

function isNativeJournalOutput(entry: TimelineEntry): boolean {
  return (
    entry.metadata?.schema_version === "g5.task-event.v1" &&
    (entry.metadata.kind === "message.delta" ||
      entry.metadata.kind === "message.final")
  );
}

/** Native message.final closes a text block, not the entire assistant turn.
 * Replace its matching native block's deltas with the complete text, including
 * across tools. For old rows without block IDs, pair only adjacent output.
 * Keep distinct snapshots and an unfinished trailing run.
 */
/** The projected copy of a native delta that carries its block id and has no
 * anchor: the one case the general projection would produce for it, kept by
 * identity while the row is unchanged. */
function projectedNativeDelta(
  entry: TimelineEntry,
  blockId: string,
): TimelineEntry {
  const cached = nativeProjections.get(entry);
  if (cached && cached.blockId === blockId && cached.anchor === undefined)
    return cached.projected;
  const payload = entry.metadata?.payload as Record<string, unknown>;
  const text = payload.text_delta;
  const projected: TimelineEntry = {
    ...entry,
    content: typeof text === "string" ? text : entry.content,
    metadata: { ...entry.metadata, blockId },
  };
  nativeProjections.set(entry, { blockId, anchor: undefined, projected });
  return projected;
}

/** The native block id a delta carries, when it is a plain delta: one whose
 * projection needs no run state (no final to anchor to, no derived id). */
function plainNativeDeltaBlockId(entry: TimelineEntry): string | null {
  if (entry.metadata?.kind !== "message.delta") return null;
  const payload = entry.metadata?.payload;
  const blockId =
    payload && typeof payload === "object"
      ? (payload as Record<string, unknown>).block_id
      : undefined;
  return typeof blockId === "string" && blockId.length > 0 ? blockId : null;
}

function projectNativeSnapshots(entries: TimelineEntry[]): TimelineEntry[] {
  // The streaming case, every commit of a long answer: native deltas that
  // all carry their block id and no native final yet. Their projection is
  // each delta's own copy, in place, so the general pass below (run state,
  // sorting, replacement) is not paid per commit.
  let anyNative = false;
  let plain = true;
  for (const entry of entries) {
    if (!isNativeJournalOutput(entry)) continue;
    anyNative = true;
    if (plainNativeDeltaBlockId(entry) === null) {
      plain = false;
      break;
    }
  }
  if (!anyNative) return entries;
  if (plain)
    return entries.map((entry) =>
      isNativeJournalOutput(entry)
        ? projectedNativeDelta(entry, plainNativeDeltaBlockId(entry) as string)
        : entry,
    );
  const pending = new Map<string, TimelineEntry[]>();
  const nativeDeltas = new Map<string, TimelineEntry[]>();
  const seenNativeFinals = new Set<string>();
  const replaced = new Set<TimelineEntry>();
  const blockIds = new Map<TimelineEntry, string>();
  const anchors = new Map<TimelineEntry, TimelineEntry>();
  for (const entry of sortedTimelineEntries(entries)) {
    const turn = providedAgentOutputTurnKey(entry);
    if (!turn) continue;
    const key = `${turn}\u0000${String(entry.metadata?.taskId)}\u0000${String(entry.metadata?.attempt)}`;
    if (!isNativeJournalOutput(entry)) {
      pending.delete(key);
      continue;
    }
    const payload = entry.metadata?.payload;
    const nativeBlockId =
      payload && typeof payload === "object"
        ? (payload as Record<string, unknown>).block_id
        : undefined;
    if (typeof nativeBlockId === "string" && nativeBlockId.length > 0) {
      const nativeKey = `${key}:${nativeBlockId}`;
      blockIds.set(entry, nativeBlockId);
      if (entry.metadata?.kind === "message.delta") {
        if (seenNativeFinals.has(nativeKey)) {
          replaced.add(entry);
          continue;
        }
        const deltas = nativeDeltas.get(nativeKey);
        if (deltas) deltas.push(entry);
        else nativeDeltas.set(nativeKey, [entry]);
      } else {
        if (seenNativeFinals.has(nativeKey)) {
          replaced.add(entry);
          continue;
        }
        seenNativeFinals.add(nativeKey);
        const firstDelta =
          nativeDeltas.get(nativeKey)?.[0] ?? pending.get(key)?.[0];
        if (firstDelta) anchors.set(entry, firstDelta);
        nativeDeltas.get(nativeKey)?.forEach((delta) => replaced.add(delta));
        if (!nativeDeltas.has(nativeKey)) {
          pending.get(key)?.forEach((delta) => replaced.add(delta));
        }
        nativeDeltas.delete(nativeKey);
        pending.delete(key);
      }
      continue;
    }
    if (entry.metadata?.kind === "message.delta") {
      const run = pending.get(key) ?? [];
      run.push(entry);
      pending.set(key, run);
      blockIds.set(entry, `native-${run[0].id}`);
    } else {
      const run = pending.get(key) ?? [];
      if (run[0]) anchors.set(entry, run[0]);
      run.forEach((delta) => replaced.add(delta));
      blockIds.set(
        entry,
        run.length
          ? `native-${run[0].id}`
          : typeof entry.metadata?.blockId === "string" &&
              entry.metadata.blockId !== "native-output"
            ? entry.metadata.blockId
            : `native-${entry.id}`,
      );
      pending.delete(key);
    }
  }
  if (!blockIds.size) return entries;
  const projected = entries
    .filter((entry) => !replaced.has(entry))
    .map((entry) => {
      const blockId = blockIds.get(entry);
      if (blockId === undefined) return entry;
      const anchor = anchors.get(entry);
      const cached = nativeProjections.get(entry);
      if (cached && cached.blockId === blockId && cached.anchor === anchor)
        return cached.projected;
      const payload = entry.metadata?.payload;
      const text =
        payload && typeof payload === "object"
          ? (payload as Record<string, unknown>)[
              entry.metadata?.kind === "message.final"
                ? "content"
                : "text_delta"
            ]
          : undefined;
      const projectedEntry: TimelineEntry = {
        ...entry,
        // The timeline content column is a capped preview; the durable payload
        // holds the complete block for every display consumer.
        content: typeof text === "string" ? text : entry.content,
        // Completing a block updates its text where narration began; a
        // later snapshot must not move that narration past intervening tools.
        createdAt: anchor?.createdAt ?? entry.createdAt,
        sequence: anchor?.sequence ?? entry.sequence,
        metadata: { ...entry.metadata, blockId },
      };
      nativeProjections.set(entry, {
        blockId,
        anchor,
        projected: projectedEntry,
      });
      return projectedEntry;
    });
  return anchors.size ? projected.sort(compareTimelineEntries) : projected;
}

/**
 * True when `entry` is a provided-agent answer-output row (delta or final).
 *
 * Native journal kinds identify output directly, including raw drawer rows.
 * Legacy rows are gated on `phase === "output"` so Codex commentary / unphased deltas — which
 * carry `providerEvent: "provided_agent.output.delta"` but persist as
 * `checkpoint` rows (phase `status`) — are excluded, matching the chat panel's
 * classifier exactly.
 */
export function isProvidedAgentOutputEntry(entry: TimelineEntry): boolean {
  if (isNativeJournalOutput(entry)) return true;
  const providerEvent = entry.metadata?.providerEvent as string | undefined;
  const phase =
    (entry.metadata?.phase as string | undefined) ?? entry.entryType;
  return (
    phase === "output" &&
    (providerEvent === OUTPUT_DELTA_EVENT ||
      providerEvent === OUTPUT_FINAL_EVENT)
  );
}

/**
 * Per-turn identity used to group delta/final rows. The worker stamps
 * `turnId === messageId` (the assistant message id) on every provided-agent
 * frame; prefer `turnId` and fall back to `messageId`. Returns `null` when
 * neither is present — such a row never suppresses another turn's final.
 */
export function providedAgentOutputTurnKey(
  entry: TimelineEntry,
): string | null {
  const turnId = entry.metadata?.turnId as string | undefined;
  const messageId = entry.metadata?.messageId as string | undefined;
  return turnId ?? messageId ?? null;
}

/**
 * True for a live-only partial-output row. While a Claude text block streams,
 * the worker publishes its text so far as a non-persisted
 * `provided_agent.output.delta` with `streamPersistence: "volatile"`, under the
 * block id that block's persisted delta will take. Each such row carries the
 * block's whole text so far, not an increment.
 */
export function isVolatileProvidedAgentOutputDelta(
  entry: TimelineEntry,
): boolean {
  return (
    isProvidedAgentOutputEntry(entry) &&
    entry.metadata?.providerEvent === OUTPUT_DELTA_EVENT &&
    entry.metadata?.streamPersistence === "volatile"
  );
}

function providedAgentOutputBlockKey(entry: TimelineEntry): string | null {
  const turnKey = providedAgentOutputTurnKey(entry);
  const blockId = entry.metadata?.blockId as string | undefined;
  return turnKey && blockId ? `${turnKey}:${blockId}` : null;
}

function isSameOrLaterEntry(
  candidate: TimelineEntry,
  current: TimelineEntry,
): boolean {
  return compareTimelineEntries(candidate, current) >= 0;
}

/**
 * Project native text blocks and legacy aggregate answers for display.
 *
 * After native block projection, returns entries in original order with each legacy aggregate
 * `provided_agent.output.final` row dropped **iff** a same-turn persisted
 * (semantic) `provided_agent.output.delta` row exists in `entries`. A turn
 * whose final row has no matching persisted delta keeps its final row
 * unchanged. Suppression is keyed per turn (never global) and is
 * provider-agnostic (Claude and Codex emit the same delta/final pair).
 *
 * Live partial rows (`streamPersistence: "volatile"`) are provisional and
 * never suppress a final. Only a block's latest live row is kept (each carries
 * the block's text so far), none once the block's persisted delta exists, and
 * none once the turn's final row exists, whatever order the rows arrived in:
 * a turn that ends without a completed block has only its final as the
 * authoritative answer. Native snapshots retain the first delta’s chronological position; non-output entries pass
 * through untouched. Legacy input references are preserved when nothing is
 * dropped; native rows receive derived display block identities.
 */
export function projectProvidedAgentOutput(
  entries: TimelineEntry[],
): TimelineEntry[] {
  entries = projectNativeSnapshots(entries);
  const turnsWithPersistedDelta = new Set<string>();
  const turnsWithFinal = new Set<string>();
  const blocksWithPersistedDelta = new Set<string>();
  const latestVolatileByBlock = new Map<string, TimelineEntry>();
  let sawVolatile = false;
  for (const entry of entries) {
    if (!isProvidedAgentOutputEntry(entry) || isNativeJournalOutput(entry))
      continue;
    const key = providedAgentOutputTurnKey(entry);
    if (entry.metadata?.providerEvent === OUTPUT_FINAL_EVENT) {
      if (key) turnsWithFinal.add(key);
      continue;
    }
    const blockKey = providedAgentOutputBlockKey(entry);
    if (isVolatileProvidedAgentOutputDelta(entry)) {
      sawVolatile = true;
      if (!blockKey) continue;
      const current = latestVolatileByBlock.get(blockKey);
      if (!current || isSameOrLaterEntry(entry, current)) {
        latestVolatileByBlock.set(blockKey, entry);
      }
    } else {
      if (key) turnsWithPersistedDelta.add(key);
      if (blockKey) blocksWithPersistedDelta.add(blockKey);
    }
  }

  if (turnsWithPersistedDelta.size === 0 && !sawVolatile) return entries;

  const projected = entries.filter((entry) => {
    if (isNativeJournalOutput(entry)) return true;
    if (isVolatileProvidedAgentOutputDelta(entry)) {
      const key = providedAgentOutputTurnKey(entry);
      if (key && turnsWithFinal.has(key)) return false;
      const blockKey = providedAgentOutputBlockKey(entry);
      if (!blockKey) return true;
      return (
        !blocksWithPersistedDelta.has(blockKey) &&
        latestVolatileByBlock.get(blockKey) === entry
      );
    }
    if (
      isProvidedAgentOutputEntry(entry) &&
      entry.metadata?.providerEvent === OUTPUT_FINAL_EVENT
    ) {
      const key = providedAgentOutputTurnKey(entry);
      return key === null || !turnsWithPersistedDelta.has(key);
    }
    return true;
  });
  // Preserve the input reference when nothing was dropped (e.g. a lone delta or
  // a lone live row) so callers can useMemo.
  return projected.length === entries.length ? entries : projected;
}

export interface ProvidedAgentOutputProjection {
  messageId: string;
  sessionId: string | null;
  content: string;
  createdAt: string;
}

export function timelineEntryContentText(entry: TimelineEntry): string {
  // Native journal content is provider text, not a legacy message envelope.
  if (entry.metadata?.schema_version === "g5.task-event.v1")
    return entry.content;
  try {
    const parsed = JSON.parse(entry.content) as { content?: unknown };
    return typeof parsed.content === "string" ? parsed.content : entry.content;
  } catch {
    return entry.content;
  }
}

function providedAgentOutputDeltaKey(entry: TimelineEntry): string | null {
  const messageId = entry.metadata?.messageId as string | undefined;
  const blockId = entry.metadata?.blockId as string | undefined;

  if (!isProvidedAgentOutputEntry(entry) || !messageId || !blockId) {
    return null;
  }

  return `${messageId}:${blockId}`;
}

export function buildProvidedAgentOutputProjections(
  entries: TimelineEntry[] | undefined,
): Map<string, ProvidedAgentOutputProjection> {
  const projections = new Map<string, ProvidedAgentOutputProjection>();
  if (!entries) return projections;

  interface OutputBlockProjection {
    blockId: string;
    createdAt: string;
    deltas: string[];
    finalContent: string | null;
  }

  const blocksByMessage = new Map<string, OutputBlockProjection[]>();
  const sorted = sortedTimelineEntries(entries);

  for (const entry of sorted) {
    if (!isProvidedAgentOutputEntry(entry)) continue;
    const messageId = entry.metadata?.messageId as string | undefined;
    const blockId = entry.metadata?.blockId as string | undefined;
    if (!messageId || !blockId) continue;

    const text = timelineEntryContentText(entry);
    if (!text || isInternalAgentText(text)) continue;

    let blocks = blocksByMessage.get(messageId);
    if (!blocks) {
      blocks = [];
      blocksByMessage.set(messageId, blocks);
    }
    let block = blocks.find((candidate) => candidate.blockId === blockId);
    if (!block) {
      block = {
        blockId,
        createdAt: entry.createdAt,
        deltas: [],
        finalContent: null,
      };
      blocks.push(block);
    }

    if (entry.metadata?.providerEvent === "provided_agent.output.final") {
      block.finalContent = text;
    } else {
      block.deltas.push(text);
    }

    const existing = projections.get(messageId);
    projections.set(messageId, {
      messageId,
      sessionId:
        typeof entry.metadata?.sessionId === "string"
          ? entry.metadata.sessionId
          : (existing?.sessionId ?? null),
      content: "",
      createdAt: existing
        ? compareTimelineEntries(existing, entry) <= 0
          ? existing.createdAt
          : entry.createdAt
        : entry.createdAt,
    });
  }

  for (const [messageId, blocks] of blocksByMessage) {
    const projection = projections.get(messageId);
    if (!projection) continue;
    const content = blocks
      .sort(compareTimelineEntries)
      // Deltas are exact substrings of the block's text (the provider's
      // stream frames), so they concatenate byte for byte; a newline join
      // turned every chunk boundary into a soft line break that markdown
      // rendered as a space ("I 'd", "pr ior").
      .map((block) => block.finalContent ?? block.deltas.join(""))
      .filter((text) => text.trim().length > 0)
      .join("\n\n");
    projections.set(messageId, { ...projection, content });
  }

  return projections;
}

export function projectProvidedAgentOutputIntoMessage(
  message: ChatMessageDTO,
  projection: ProvidedAgentOutputProjection | undefined,
): ChatMessageDTO {
  // Diagnostic content has its own meaning; real output still renders from
  // the owned journal blocks without replacing that saved explanation.
  if (message.isErrorNotice || !projection || !projection.content.trim())
    return message;
  const shouldUseProjectedContent =
    message.status === "pending" ||
    message.status === "streaming" ||
    !message.content.trim() ||
    // Historical rows used this placeholder; new steering projects the stdin flush.
    message.content.trim() === "Steering message sent to Claude Code.";
  if (!shouldUseProjectedContent) return message;

  return {
    ...message,
    content: projection.content,
    status: message.status === "pending" ? "streaming" : message.status,
  };
}

export function syntheticProvidedAgentAssistantMessage(
  projection: ProvidedAgentOutputProjection,
  messages: ChatMessageDTO[],
  index: number,
): ChatMessageDTO {
  const maxMessageIndex = messages.reduce(
    (max, message) => Math.max(max, message.messageIndex),
    -1,
  );
  return {
    id: projection.messageId,
    sessionId: projection.sessionId ?? messages[0]?.sessionId ?? "",
    role: "assistant",
    content: projection.content,
    artifacts: null,
    citations: null,
    inputTokens: null,
    outputTokens: null,
    status: "streaming",
    messageIndex: maxMessageIndex + 1 + index,
    createdAt: projection.createdAt,
  };
}

/** A run of consecutive deltas closes at the first paragraph boundary after
 * it holds this many, never inside a code fence: a streaming answer then
 * renders as stable pieces plus one open piece, so each new delta re-renders
 * only the open piece rather than the whole accumulated text (chat
 * g5.chat.live-tail-recovery). Splitting elsewhere would break a paragraph or
 * a fence in two documents, so an answer without paragraph breaks stays one
 * piece and still re-renders whole; the completed text, carried by the
 * block's final row, renders whole in every case. */
const COALESCE_RUN_TARGET_DELTAS = 64;

export function coalesceProvidedAgentOutputDeltas(
  entries: TimelineEntry[],
): TimelineEntry[] {
  const coalesced: TimelineEntry[] = [];
  let pending: TimelineEntry[] = [];
  let pendingKey: string | null = null;
  let pendingTail = "";
  // Code fences are tracked by complete lines across deltas, as Markdown
  // reads them: a line opening with three or more backticks or tildes opens
  // a fence, and only a line of at least as many of the same character
  // closes it, so a piece never closes inside a fence however its markers
  // were split across deltas.
  let openFence: { char: string; length: number } | null = null;
  let pendingLine = "";

  const closeRun = () => {
    if (pending.length === 0) return;

    if (pending.length === 1) {
      coalesced.push(pending[0]);
    } else {
      const first = pending[0];
      const last = pending[pending.length - 1];
      coalesced.push({
        ...last,
        id: first.id,
        // Exact concatenation: consecutive deltas are substrings of one
        // answer, and a newline here became a rendered space at every
        // chunk boundary.
        content: pending.map(timelineEntryContentText).join(""),
        createdAt: first.createdAt,
      });
    }

    pending = [];
    pendingTail = "";
    openFence = null;
    pendingLine = "";
  };
  const flushPending = () => {
    closeRun();
    pendingKey = null;
  };
  const extendRun = (entry: TimelineEntry) => {
    pending.push(entry);
    const text = timelineEntryContentText(entry);
    let rest = pendingLine + text;
    let newline = rest.indexOf("\n");
    while (newline !== -1) {
      const line = rest.slice(0, newline);
      rest = rest.slice(newline + 1);
      const marker = /^ {0,3}(`{3,}|~{3,})(.*)$/.exec(line);
      if (marker) {
        const run = marker[1];
        if (openFence === null) {
          // A backtick info string may not itself contain a backtick.
          if (run[0] === "~" || !marker[2].includes("`")) {
            openFence = { char: run[0], length: run.length };
          }
        } else if (
          run[0] === openFence.char &&
          run.length >= openFence.length &&
          marker[2].trim() === ""
        ) {
          openFence = null;
        }
      }
      newline = rest.indexOf("\n");
    }
    pendingLine = rest;
    pendingTail = (pendingTail + text).slice(-2);
    const insideFence = openFence !== null;
    if (
      !insideFence &&
      pending.length >= COALESCE_RUN_TARGET_DELTAS &&
      pendingTail === "\n\n"
    )
      closeRun();
  };

  for (const entry of entries) {
    const key = providedAgentOutputDeltaKey(entry);
    if (!key) {
      flushPending();
      coalesced.push(entry);
      continue;
    }

    if (pendingKey === key) {
      extendRun(entry);
      continue;
    }

    flushPending();
    pendingKey = key;
    extendRun(entry);
  }

  flushPending();
  return coalesced;
}

/** Legacy hook payloads are internal execution context, never conversation prose. */
export function isInternalAgentText(text: string): boolean {
  const normalized = text.trim();
  return (
    normalized.includes(
      "=== G5_PREAMBLE_V1 - READ IN FULL BEFORE ANY ACTION ===",
    ) ||
    (normalized.includes("You are a subagent in a G5 ontology-driven repo") &&
      normalized.includes("G5_SUBAGENT_BOOTSTRAP"))
  );
}

/** Sparse legacy previews did not identify durable blocks. A saved terminal
 * answer wins over those previews and over unfinished volatile snapshots.
 * Tagged native/per-block output remains authoritative for ordered narration.
 */
export function projectAssistantOutputEntries(
  message: ChatMessageDTO,
  entries: TimelineEntry[],
): TimelineEntry[] {
  const terminalSaved =
    message.status !== "pending" &&
    message.status !== "streaming" &&
    !!message.content.trim();
  return coalesceProvidedAgentOutputDeltas(
    projectProvidedAgentOutput(entries).filter((entry) => {
      if (!terminalSaved || !isProvidedAgentOutputEntry(entry)) return true;
      return (
        entry.metadata?.streamPersistence !== "volatile" &&
        (entry.metadata?.providerEvent !== OUTPUT_DELTA_EVENT ||
          entry.metadata?.provider != null ||
          entry.metadata?.streamPersistence != null ||
          isNativeJournalOutput(entry))
      );
    }),
  );
}
