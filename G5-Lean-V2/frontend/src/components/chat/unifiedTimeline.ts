// =============================================================================
// G5 Client — the transcript's unified timeline, built once per changed turn
// (chat g5.chat.live-tail-recovery: a render re-projects only the turns whose
// rows changed)
// =============================================================================
// The panel's commit cost used to grow with every row held: each placement
// re-sorted and re-projected the whole transcript and handed every turn a
// fresh entries array, so React re-rendered them all. The builder below keeps
// a cache keyed by assistant message: a turn whose entries did not change
// keeps its projection, its entries array and its item by identity, and the
// approval classification each turn receives is sliced to its own requests
// so it, too, keeps identity. Unchanged turns are then skipped by the
// memoized turn component.

import type { ChatMessageDTO, TimelineEntry } from "../../lib/types";
import type {
  AgentApprovalClassification,
  TimedOutQuestionSubState,
} from "../../lib/agentApprovals";
import {
  buildProvidedAgentOutputProjections,
  compareTimelineEntries,
  projectProvidedAgentOutputIntoMessage,
  sortedTimelineEntries,
  syntheticProvidedAgentAssistantMessage,
  type ProvidedAgentOutputProjection,
} from "../../lib/providerOutput";

export type TimelineItem =
  | { kind: "message"; msg: ChatMessageDTO; entries?: TimelineEntry[] }
  | { kind: "trace"; entry: TimelineEntry };

/** The approval state one turn needs: its own requests only. */
export interface TurnApprovalSlice {
  classification: AgentApprovalClassification | undefined;
  timedOut: ReadonlyMap<string, TimedOutQuestionSubState> | undefined;
}

interface TurnCache {
  /** The turn's entries as last grouped, in order. */
  bucket: TimelineEntry[];
  projection: ProvidedAgentOutputProjection | undefined;
  /** The message object the turn rendered with, reused while unchanged. */
  msg?: ChatMessageDTO;
  item?: TimelineItem;
  sliceKey?: string;
  slice?: TurnApprovalSlice;
}

export interface UnifiedTimelineCache {
  turns: Map<string, TurnCache>;
}

export function createUnifiedTimelineCache(): UnifiedTimelineCache {
  return { turns: new Map() };
}

const EMPTY_ENTRIES: TimelineEntry[] = [];
const EMPTY_SLICE: TurnApprovalSlice = {
  classification: undefined,
  timedOut: undefined,
};

/** Element-wise identity of two entry lists. */
export function sameEntries(
  a: readonly TimelineEntry[] | undefined,
  b: readonly TimelineEntry[] | undefined,
): boolean {
  const left = a ?? EMPTY_ENTRIES;
  const right = b ?? EMPTY_ENTRIES;
  if (left === right) return true;
  if (left.length !== right.length) return false;
  for (let index = 0; index < left.length; index += 1)
    if (left[index] !== right[index]) return false;
  return true;
}

/** Field-wise identity of two records (a message DTO's fields are scalars
 * or arrays the query cache keeps by identity while unchanged). */
export function sameRecord<T extends object>(a: T, b: T): boolean {
  if (a === b) return true;
  const left = a as Record<string, unknown>;
  const right = b as Record<string, unknown>;
  const keys = Object.keys(left);
  if (keys.length !== Object.keys(right).length) return false;
  for (const key of keys) {
    if (!Object.prototype.hasOwnProperty.call(right, key)) return false;
    if (!Object.is(left[key], right[key])) return false;
  }
  return true;
}

/** The entries in chronological order without duplicate ids; the input
 * itself when it already is, which the placement path keeps it. */
function orderedUnique(entries: readonly TimelineEntry[]): TimelineEntry[] {
  const ordered = sortedTimelineEntries(entries);
  const ids = new Set<string>();
  let unique = true;
  for (const entry of ordered) {
    if (ids.has(entry.id)) {
      unique = false;
      break;
    }
    ids.add(entry.id);
  }
  if (unique) return ordered;
  const seen = new Set<string>();
  const result: TimelineEntry[] = [];
  for (const entry of ordered) {
    if (seen.has(entry.id)) continue;
    seen.add(entry.id);
    result.push(entry);
  }
  return result;
}

/**
 * Builds the transcript's items: every chat message in order, assistant
 * turns carrying the activity entries they own (by message id, or for old
 * event formats by chronological window), and leftover entries as trace
 * items. With a cache, a turn whose entries did not change keeps its
 * projection, entries and item by identity.
 */
export function buildUnifiedTimeline(
  messages: ChatMessageDTO[],
  timelineEntries: TimelineEntry[] | undefined,
  cache: UnifiedTimelineCache = createUnifiedTimelineCache(),
): TimelineItem[] {
  const entries = orderedUnique(timelineEntries ?? EMPTY_ENTRIES);
  const byMessage = new Map<string, TimelineEntry[]>();
  const unassigned: TimelineEntry[] = [];
  for (const entry of entries) {
    const messageId = entry.metadata?.messageId;
    if (typeof messageId === "string") {
      const bucket = byMessage.get(messageId);
      if (bucket) bucket.push(entry);
      else byMessage.set(messageId, [entry]);
    } else unassigned.push(entry);
  }
  const known = new Set(messages.map((message) => message.id));
  const previous = cache.turns;
  const turns = new Map<string, TurnCache>();
  const projections = new Map<string, ProvidedAgentOutputProjection>();
  for (const [messageId, grouped] of byMessage) {
    const cached = previous.get(messageId);
    let turn: TurnCache;
    if (cached && sameEntries(cached.bucket, grouped)) {
      turn = cached;
      byMessage.set(messageId, cached.bucket);
    } else {
      turn = {
        bucket: grouped,
        projection: buildProvidedAgentOutputProjections(grouped).get(messageId),
        sliceKey: cached?.sliceKey,
        slice: cached?.slice,
      };
    }
    turns.set(messageId, turn);
    // Tool/question events can precede both text output and the persisted
    // assistant row. Give that turn its real message key immediately so later
    // hydration cannot remount its disclosures or in-flight request controls.
    const projection =
      turn.projection ??
      (known.has(messageId)
        ? undefined
        : {
            messageId,
            sessionId:
              typeof turn.bucket[0].metadata?.sessionId === "string"
                ? turn.bucket[0].metadata.sessionId
                : null,
            content: "",
            createdAt: turn.bucket[0].createdAt,
          });
    if (projection) projections.set(messageId, projection);
  }
  cache.turns = turns;

  const synthetic = [...projections.values()]
    .filter((projection) => !known.has(projection.messageId))
    .map((projection, index) => {
      const next = syntheticProvidedAgentAssistantMessage(
        projection,
        messages,
        index,
      );
      const turn = turns.get(projection.messageId);
      return turn?.msg && sameRecord(turn.msg, next) ? turn.msg : next;
    });
  const knownSorted = [...messages].sort(
    (a, b) => a.messageIndex - b.messageIndex,
  );
  const sorted = knownSorted.map((message) => {
    const projected = projectProvidedAgentOutputIntoMessage(
      message,
      projections.get(message.id),
    );
    if (projected === message) return message;
    const turn = turns.get(message.id);
    return turn?.msg && sameRecord(turn.msg, projected) ? turn.msg : projected;
  });
  for (const message of synthetic.sort(compareTimelineEntries)) {
    const next = sorted.findIndex(
      (candidate) => compareTimelineEntries(candidate, message) > 0,
    );
    sorted.splice(next < 0 ? sorted.length : next, 0, message);
  }

  const items: TimelineItem[] = [];
  let unassignedIndex = 0;
  for (let i = 0; i < sorted.length; i++) {
    const msg = sorted[i];
    if (msg.role !== "assistant") {
      items.push({ kind: "message", msg });
      continue;
    }
    const bucket = byMessage.get(msg.id);
    byMessage.delete(msg.id);
    let owned = bucket ?? EMPTY_ENTRIES;
    // Old AgentLoop events have no message identity. Associate only within
    // the delivered turn's chronological window, never by provider name.
    const start = i > 0 ? sorted[i - 1].createdAt : "";
    const end =
      sorted.slice(i + 1).find((message) => message.role === "user")
        ?.createdAt ?? "\uffff";
    let adopted: TimelineEntry[] | null = null;
    while (
      unassignedIndex < unassigned.length &&
      unassigned[unassignedIndex].createdAt < end
    ) {
      const entry = unassigned[unassignedIndex++];
      if (entry.createdAt >= start) (adopted ??= [...owned]).push(entry);
    }
    if (adopted) owned = adopted.sort(compareTimelineEntries);
    const turn = turns.get(msg.id);
    if (
      turn?.item &&
      turn.item.kind === "message" &&
      turn.msg === msg &&
      sameEntries(turn.item.entries, owned)
    ) {
      items.push(turn.item);
      continue;
    }
    const item: TimelineItem = { kind: "message", msg, entries: owned };
    if (turn) {
      turn.msg = msg;
      turn.item = item;
    }
    items.push(item);
  }
  const remaining = [
    ...unassigned.slice(unassignedIndex),
    ...[...byMessage.values()].flat(),
  ].sort(compareTimelineEntries);
  for (const entry of remaining) items.push({ kind: "trace", entry });
  return items;
}

/** The approval ids a turn's entries refer to, in order, once each. */
function turnApprovalIds(entries: readonly TimelineEntry[]): string[] {
  const ids: string[] = [];
  const seen = new Set<string>();
  for (const entry of entries) {
    const approvalId = entry.metadata?.approvalId;
    if (typeof approvalId !== "string" || seen.has(approvalId)) continue;
    seen.add(approvalId);
    ids.push(approvalId);
  }
  return ids;
}

/**
 * Slices the session-wide approval classification and timed-out question
 * sub-states to each assistant turn's own requests, so a turn whose
 * requests did not change keeps the same slice objects and is not
 * re-rendered when another turn's request settles.
 */
export function sliceApprovalsPerTurn(
  items: readonly TimelineItem[],
  cache: UnifiedTimelineCache,
  classification: AgentApprovalClassification | undefined,
  timedOut: ReadonlyMap<string, TimedOutQuestionSubState> | undefined,
): Map<string, TurnApprovalSlice> {
  const slices = new Map<string, TurnApprovalSlice>();
  for (const item of items) {
    if (item.kind !== "message" || item.msg.role !== "assistant") continue;
    const ids = turnApprovalIds(item.entries ?? EMPTY_ENTRIES);
    if (!ids.length) {
      slices.set(item.msg.id, EMPTY_SLICE);
      continue;
    }
    const key = ids
      .map((id) => {
        const pending = classification?.pending.indexOf(id) ?? -1;
        return `${id}:${classification?.settledOrDead.has(id) ? "s" : ""}${
          classification?.dead.has(id) ? "d" : ""
        }${classification?.failed.has(id) ? "f" : ""}${
          pending >= 0 ? `p${pending}` : ""
        }:${timedOut?.get(id) ?? ""}`;
      })
      .join("|");
    const turn = cache.turns.get(item.msg.id);
    if (turn?.slice && turn.sliceKey === key) {
      slices.set(item.msg.id, turn.slice);
      continue;
    }
    const own = new Set(ids);
    const slice: TurnApprovalSlice = {
      classification: classification
        ? {
            settledOrDead: new Set(
              [...classification.settledOrDead].filter((id) => own.has(id)),
            ),
            dead: new Set([...classification.dead].filter((id) => own.has(id))),
            failed: new Set(
              [...classification.failed].filter((id) => own.has(id)),
            ),
            pending: classification.pending.filter((id) => own.has(id)),
          }
        : undefined,
      timedOut: timedOut
        ? new Map([...timedOut].filter(([id]) => own.has(id)))
        : undefined,
    };
    if (turn) {
      turn.sliceKey = key;
      turn.slice = slice;
    }
    slices.set(item.msg.id, slice);
  }
  return slices;
}
