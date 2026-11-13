import { describe, expect, it } from "vitest";
import {
  buildUnifiedTimeline,
  createUnifiedTimelineCache,
  sameEntries,
  sameRecord,
  sliceApprovalsPerTurn,
} from "./unifiedTimeline";
import { createMockChatMessage } from "../../test/fixtures";
import type { TimelineEntry } from "../../lib/types";
import type { AgentApprovalClassification } from "../../lib/agentApprovals";

function row(
  messageId: string,
  seq: number,
  options: { live?: boolean; approvalId?: string; kind?: string } = {},
): TimelineEntry {
  return {
    id: `${messageId}:${seq}`,
    entryType: options.approvalId ? "tool_call" : "output",
    content: `delta ${seq}`,
    sequence: seq,
    createdAt: `2026-11-02T00:${String(seq).padStart(2, "0")}:00.000Z`,
    metadata: {
      schema_version: "g5.task-event.v1",
      kind: options.kind ?? "message.delta",
      taskId: "task-1",
      attempt: 1,
      seq,
      messageId,
      sessionId: "session-1",
      ...(options.approvalId ? { approvalId: options.approvalId } : {}),
      ...(options.live ? { live: true } : {}),
    },
  };
}

const user = (id: string, index: number) =>
  createMockChatMessage({
    id,
    role: "user",
    content: "ask",
    messageIndex: index,
    createdAt: `2026-11-02T00:${String(index * 10).padStart(2, "0")}:00.000Z`,
  });
const assistant = (id: string, index: number) =>
  createMockChatMessage({
    id,
    role: "assistant",
    content: "answer",
    messageIndex: index,
    createdAt: `2026-11-02T00:${String(index * 10 + 1).padStart(2, "0")}:00.000Z`,
  });

describe("buildUnifiedTimeline with a cache (g5.chat.live-tail-recovery)", () => {
  const messages = [
    user("u1", 0),
    assistant("a1", 1),
    user("u2", 2),
    assistant("a2", 3),
  ];
  const first = [row("a1", 11), row("a1", 12)];
  const second = [row("a2", 31), row("a2", 32)];

  it("keeps an unchanged turn's item and entries by identity when another turn receives rows", () => {
    const cache = createUnifiedTimelineCache();
    const before = buildUnifiedTimeline(messages, [...first, ...second], cache);
    const after = buildUnifiedTimeline(
      messages,
      [...first, ...second, row("a2", 33, { live: true })],
      cache,
    );
    const turnOf = (items: typeof before, id: string) =>
      items.find((item) => item.kind === "message" && item.msg.id === id);
    expect(turnOf(after, "a1")).toBe(turnOf(before, "a1"));
    expect(turnOf(after, "a2")).not.toBe(turnOf(before, "a2"));
    const changed = turnOf(after, "a2");
    expect(changed?.kind === "message" && changed.entries?.length).toBe(3);
    // The user rows keep their message objects too.
    expect(
      turnOf(after, "u1")?.kind === "message" && turnOf(after, "u1"),
    ).toEqual(turnOf(before, "u1"));
  });

  it("re-projects a turn whose live row was replaced by its saved copy", () => {
    const cache = createUnifiedTimelineCache();
    const live = row("a2", 33, { live: true });
    const before = buildUnifiedTimeline(
      messages,
      [...first, ...second, live],
      cache,
    );
    const saved = row("a2", 33);
    const after = buildUnifiedTimeline(
      messages,
      [...first, ...second, saved],
      cache,
    );
    const turn = after.find(
      (item) => item.kind === "message" && item.msg.id === "a2",
    );
    const previous = before.find(
      (item) => item.kind === "message" && item.msg.id === "a2",
    );
    expect(turn).not.toBe(previous);
    expect(turn?.kind === "message" && turn.entries?.[2]).toBe(saved);
  });

  it("orders and de-duplicates entries as the uncached builder did", () => {
    const cached = createUnifiedTimelineCache();
    const shuffled = [second[1], first[1], second[0], first[0], first[0]];
    const withCache = buildUnifiedTimeline(messages, shuffled, cached);
    const withoutCache = buildUnifiedTimeline(messages, shuffled);
    expect(withCache).toEqual(withoutCache);
    const a1 = withCache.find(
      (item) => item.kind === "message" && item.msg.id === "a1",
    );
    expect(
      a1?.kind === "message" && a1.entries?.map((entry) => entry.id),
    ).toEqual(["a1:11", "a1:12"]);
  });

  it("keeps a synthetic turn's message object while its projection is unchanged", () => {
    const cache = createUnifiedTimelineCache();
    const only = [user("u1", 0)];
    const entries = [row("a9", 41, { live: true })];
    const before = buildUnifiedTimeline(only, entries, cache);
    const after = buildUnifiedTimeline(only, [...entries], cache);
    const synthetic = (items: typeof before) =>
      items.find((item) => item.kind === "message" && item.msg.id === "a9");
    expect(synthetic(before)).toBeDefined();
    expect(synthetic(after)).toBe(synthetic(before));
  });
});

describe("sliceApprovalsPerTurn", () => {
  const messages = [user("u1", 0), assistant("a1", 1), assistant("a2", 3)];
  const entries = [
    row("a1", 11, { approvalId: "req-1", kind: "provider_request.opened" }),
    row("a2", 31, { approvalId: "req-2", kind: "provider_request.opened" }),
  ];
  const classification = (pending: string[]): AgentApprovalClassification => ({
    settledOrDead: new Set(
      ["req-1", "req-2"].filter((id) => !pending.includes(id)),
    ),
    dead: new Set(),
    failed: new Set(),
    pending,
  });

  it("gives each turn only its own requests and keeps the slice while they are unchanged", () => {
    const cache = createUnifiedTimelineCache();
    const items = buildUnifiedTimeline(messages, entries, cache);
    const first = sliceApprovalsPerTurn(
      items,
      cache,
      classification(["req-1", "req-2"]),
      new Map(),
    );
    expect(first.get("a1")?.classification?.pending).toEqual(["req-1"]);
    expect(first.get("a2")?.classification?.pending).toEqual(["req-2"]);
    // Another turn's request settles: this turn's slice keeps its identity.
    const second = sliceApprovalsPerTurn(
      items,
      cache,
      classification(["req-1"]),
      new Map(),
    );
    expect(second.get("a1")).toBe(first.get("a1"));
    expect(second.get("a2")).not.toBe(first.get("a2"));
    expect(second.get("a2")?.classification?.pending).toEqual([]);
    expect(second.get("a2")?.classification?.settledOrDead.has("req-2")).toBe(
      true,
    );
  });

  it("gives a turn without requests the shared empty slice", () => {
    const cache = createUnifiedTimelineCache();
    const items = buildUnifiedTimeline(messages, [row("a1", 11)], cache);
    const slices = sliceApprovalsPerTurn(
      items,
      cache,
      classification([]),
      new Map(),
    );
    expect(slices.get("a1")?.classification).toBeUndefined();
    expect(slices.get("a1")).toBe(
      sliceApprovalsPerTurn(items, cache, classification([]), new Map()).get(
        "a1",
      ),
    );
  });
});

describe("identity helpers", () => {
  it("compare entry lists element-wise and records field-wise", () => {
    const a = row("a1", 1);
    const b = row("a1", 2);
    expect(sameEntries([a, b], [a, b])).toBe(true);
    expect(sameEntries([a, b], [a, row("a1", 2)])).toBe(false);
    expect(sameEntries(undefined, [])).toBe(true);
    const message = createMockChatMessage();
    expect(sameRecord(message, { ...message })).toBe(true);
    expect(sameRecord(message, { ...message, content: "other" })).toBe(false);
  });
});
