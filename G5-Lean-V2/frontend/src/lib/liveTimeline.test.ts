import { describe, expect, it } from "vitest";
import type { TimelineEntry } from "./types";
import {
  highestHeldSequence,
  insertLiveRows,
  liveRowOpensGap,
  liveRowsOpenGap,
  onLiveTailRequested,
  persistedSequenceCeilings,
  reconcileLiveRows,
  requestLiveTail,
  retainLiveRows,
  timelineRowExecution,
} from "./liveTimeline";

function row(
  seq: number,
  options: { live?: boolean; attempt?: number; id?: string; at?: string } = {},
): TimelineEntry {
  const attempt = options.attempt ?? 1;
  return {
    id: options.id ?? `task-1:${attempt}:${seq}`,
    entryType: "output",
    content: `delta ${seq}`,
    sequence: seq,
    createdAt:
      options.at ?? `2026-10-30T00:00:${String(seq).padStart(2, "0")}.000Z`,
    metadata: {
      schema_version: "g5.task-event.v1",
      taskId: "task-1",
      attempt,
      seq,
      ...(options.live ? { live: true } : {}),
    },
  };
}

const window = (items: TimelineEntry[]) => ({
  items,
  cursor: null,
  hasMore: false,
});

describe("liveTimeline (chat g5.chat.replay)", () => {
  it("reads an execution identity only from complete metadata", () => {
    expect(timelineRowExecution(row(3))).toEqual({
      attemptKey: "task-1:1",
      seq: 3,
    });
    expect(timelineRowExecution({ metadata: { taskId: "t" } })).toBeNull();
    expect(timelineRowExecution({ metadata: {} })).toBeNull();
  });

  it("ceilings come from saved rows only, per execution", () => {
    const ceilings = persistedSequenceCeilings([
      row(1),
      row(4),
      row(9, { live: true }),
      row(2, { attempt: 2 }),
    ]);
    expect(ceilings.get("task-1:1")).toBe(4);
    expect(ceilings.get("task-1:2")).toBe(2);
  });

  it("inserts live rows once, in order, and ignores duplicates and reorders", () => {
    const held = window([row(1), row(2)]);
    const first = insertLiveRows(held, [row(4, { live: true })]);
    const second = insertLiveRows(first, [
      row(3, { live: true }),
      row(4, { live: true }),
    ]);
    expect(second.items.map((item) => item.id)).toEqual([
      "task-1:1:1",
      "task-1:1:2",
      "task-1:1:3",
      "task-1:1:4",
    ]);
    // A duplicate changes nothing, and the same window object is returned.
    expect(insertLiveRows(second, [row(3, { live: true })])).toBe(second);
  });

  it("keeps a live row below the held saved rows until its own saved copy is held", () => {
    // Saved row 5 proves rows 3 and 4 are saved server-side, not that the
    // browser read them: the live row stands in for its copy meanwhile.
    const held = window([row(1), row(2), row(5)]);
    const next = insertLiveRows(held, [
      row(3, { live: true }),
      row(6, { live: true }),
    ]);
    expect(next.items.map((item) => item.id)).toEqual([
      "task-1:1:1",
      "task-1:1:2",
      "task-1:1:3",
      "task-1:1:5",
      "task-1:1:6",
    ]);
    // The saved copy replaces it; a live copy of a saved row is a duplicate.
    const read = insertLiveRows(next, [row(3), row(5, { live: true })]);
    expect(
      read.items.map((item) => [item.id, item.metadata.live ?? false]),
    ).toEqual([
      ["task-1:1:1", false],
      ["task-1:1:2", false],
      ["task-1:1:3", false],
      ["task-1:1:5", false],
      ["task-1:1:6", true],
    ]);
  });

  it("a saved row placed live replaces the live row with its id and nothing else", () => {
    const held = window([
      row(1),
      row(2, { live: true }),
      row(3, { live: true }),
    ]);
    const stored = { ...row(2), metadata: { ...row(2).metadata, status: "x" } };
    const placed = insertLiveRows(held, [stored]);
    expect(placed).not.toBe(held);
    expect(
      placed.items.map((item) => [item.id, item.metadata.live ?? false]),
    ).toEqual([
      ["task-1:1:1", false],
      ["task-1:1:2", false],
      ["task-1:1:3", true],
    ]);
    // The stored row's own fields won, not the live row's.
    expect(placed.items[1].metadata.status).toBe("x");
    // Saved over saved is a duplicate; a live row over its saved row too.
    expect(insertLiveRows(placed, [row(2)])).toBe(placed);
    expect(insertLiveRows(placed, [row(2, { live: true })])).toBe(placed);
    // A stored row that was never held live is placed as a saved row.
    const more = insertLiveRows(placed, [row(5)]);
    expect(more.items.map((item) => item.id)).toEqual([
      "task-1:1:1",
      "task-1:1:2",
      "task-1:1:3",
      "task-1:1:5",
    ]);
    expect(more.items[3].metadata.live).toBeUndefined();
    // Live rows the stored row proves saved stay until a history read brings
    // their saved copies.
    expect(more.items[2].metadata.live).toBe(true);
  });

  it("a saved row replaces its live row by id and nothing below it", () => {
    // Saved row 3 proves live row 2 is saved server-side, not that its copy
    // was read: the row stays until that copy arrives.
    const items = [
      row(1),
      row(2, { live: true }),
      row(3, { live: true }),
      row(3),
      row(4, { live: true }),
    ];
    expect(
      reconcileLiveRows(items).map((item) => [
        item.id,
        item.metadata.live ?? false,
      ]),
    ).toEqual([
      ["task-1:1:1", false],
      ["task-1:1:2", true],
      ["task-1:1:3", false],
      ["task-1:1:4", true],
    ]);
    const untouched = [row(1), row(2, { live: true })];
    expect(reconcileLiveRows(untouched)).toBe(untouched);
  });

  it("a refresh keeps live rows beyond the saved prefix it returned", () => {
    const held = window([
      row(1),
      row(2),
      row(3, { live: true }),
      row(4, { live: true }),
    ]);
    const refreshed = window([row(1), row(2), row(3)]);
    const merged = retainLiveRows(held, refreshed);
    expect(merged.items.map((item) => [item.id, item.metadata.live])).toEqual([
      ["task-1:1:1", undefined],
      ["task-1:1:2", undefined],
      ["task-1:1:3", undefined],
      ["task-1:1:4", true],
    ]);
  });

  it("another attempt's saved rows never cover this attempt's live rows", () => {
    const held = window([row(2, { live: true })]);
    const refreshed = window([row(7, { attempt: 2 })]);
    const merged = retainLiveRows(held, refreshed);
    expect(merged.items.map((item) => item.id)).toEqual([
      "task-1:1:2",
      "task-1:2:7",
    ]);
  });

  it("rows without execution identity are kept by id alone", () => {
    const legacyLive: TimelineEntry = {
      id: "legacy-live",
      entryType: "output",
      content: "x",
      createdAt: "2026-10-30T00:00:10.000Z",
      metadata: { live: true },
    };
    const next = insertLiveRows(window([row(1)]), [legacyLive]);
    expect(next.items.map((item) => item.id)).toEqual([
      "task-1:1:1",
      "legacy-live",
    ]);
    expect(
      retainLiveRows(next, window([row(1), row(2)])).items.at(-1)?.id,
    ).toBe("legacy-live");
  });
});

describe("liveTimeline gap recovery (chat g5.chat.live-tail-recovery)", () => {
  it("a live row more than one above the highest held sequence opens a gap", () => {
    const held = [row(1), row(2), row(3, { live: true })];
    expect(highestHeldSequence(held, "task-1:1")).toBe(3);
    expect(highestHeldSequence(held, "task-1:2")).toBe(-1);
    expect(liveRowOpensGap(held, row(4, { live: true }))).toBe(false);
    expect(liveRowOpensGap(held, row(6, { live: true }))).toBe(true);
    // A reordered row below the held sequence fills a hole, no gap.
    expect(liveRowOpensGap(held, row(2, { live: true }))).toBe(false);
    // Another execution's first rows: contiguous from 1, gap above.
    expect(liveRowOpensGap(held, row(1, { live: true, attempt: 2 }))).toBe(
      false,
    );
    expect(liveRowOpensGap(held, row(3, { live: true, attempt: 2 }))).toBe(
      true,
    );
    // Saved rows and rows without identity never open a gap.
    expect(liveRowOpensGap(held, row(9))).toBe(false);
    expect(
      liveRowOpensGap(held, {
        id: "x",
        entryType: "output",
        content: "",
        createdAt: "2026-10-30T00:00:00.000Z",
        metadata: { live: true },
      }),
    ).toBe(false);
  });

  it("tail requests reach the session's subscribers until they unsubscribe", () => {
    const seen: string[] = [];
    const stop = onLiveTailRequested("s1", () => seen.push("s1"));
    onLiveTailRequested("s2", () => seen.push("s2"));
    requestLiveTail("s1");
    requestLiveTail("s3");
    expect(seen).toEqual(["s1"]);
    stop();
    requestLiveTail("s1");
    expect(seen).toEqual(["s1"]);
  });

  it("a batch opens a gap only where a live row skips past the rows before it, in or out of the batch", () => {
    const held = [row(1), row(2)];
    expect(
      liveRowsOpenGap(held, [row(3, { live: true }), row(4, { live: true })]),
    ).toBe(false);
    expect(
      liveRowsOpenGap(held, [row(3, { live: true }), row(5, { live: true })]),
    ).toBe(true);
    expect(liveRowsOpenGap(held, [row(4, { live: true })])).toBe(true);
    // A saved row placed by the batch counts as held for the rows after it.
    expect(liveRowsOpenGap(held, [row(3), row(4, { live: true })])).toBe(false);
    // Another execution's rows are judged on their own.
    expect(
      liveRowsOpenGap(held, [
        row(1, { live: true, attempt: 2 }),
        row(3, { live: true }),
      ]),
    ).toBe(false);
  });
});
