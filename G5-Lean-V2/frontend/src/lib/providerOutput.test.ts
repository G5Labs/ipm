// =============================================================================
// providerOutput.test.ts — shared provided-agent output projection (#1313)
// =============================================================================
// Verifies the one-logical-answer projection (core_workflow/chat_agent_session
// #D18): a durable aggregate `provided_agent.output.final` row is suppressed on
// answer surfaces when a same-turn semantic `provided_agent.output.delta` row
// exists, per turn, provider-agnostically, without dropping non-output rows.
// =============================================================================

import { describe, it, expect } from "vitest";
import type { TimelineEntry } from "./types";
import { projectTaskTimelineEntry } from "./taskTimeline";
import {
  coalesceProvidedAgentOutputDeltas,
  isProvidedAgentOutputEntry,
  isVolatileProvidedAgentOutputDelta,
  providedAgentOutputTurnKey,
  projectProvidedAgentOutput,
} from "./providerOutput";

describe("native block snapshots", () => {
  function native(
    id: string,
    kind: string,
    text: string,
    sequence: number,
    messageId = "turn",
    blockId?: string,
  ) {
    return projectTaskTimelineEntry(
      entry({
        id,
        entryType: kind === "message.final" ? "agent_message" : "output",
        sequence,
        content: text,
        metadata: {
          schema_version: "g5.task-event.v1",
          kind,
          messageId,
          payload:
            kind === "message.delta"
              ? { text_delta: text, block_id: blockId }
              : { content: text, block_id: blockId },
        },
      }),
    );
  }

  it("replaces a partial block with its snapshot without suppressing later completed blocks", () => {
    const rows = [
      native("d1", "message.delta", "Scan", 1),
      native("f1", "message.final", "Scanning the logs.", 2),
      native("f2", "message.final", "Final answer.", 3),
    ];
    const projected = projectProvidedAgentOutput(rows);
    expect(projected.map((row) => row.content)).toEqual([
      "Scanning the logs.",
      "Final answer.",
    ]);
    expect(new Set(projected.map((row) => row.metadata?.blockId)).size).toBe(2);
    expect(projectProvidedAgentOutput(projected)).toEqual(projected);
  });

  it("pairs native ids across tool events and preserves separate blocks on replay", () => {
    const rows = [
      native("d1", "message.delta", "Check", 1, "turn", "message-1:0"),
      native("tool", "tool.started", "", 2),
      native("f1", "message.final", "Checked.", 3, "turn", "message-1:0"),
      native("f2", "message.final", "Answer.", 4, "turn", "message-2:0"),
      native("repeat", "message.final", "Answer.", 5, "turn", "message-2:0"),
    ];
    const projected = projectProvidedAgentOutput(rows);
    expect(projected.map((row) => row.id)).toEqual(["f1", "tool", "f2"]);
    expect(
      projected.filter(isProvidedAgentOutputEntry).map((row) => row.content),
    ).toEqual(["Checked.", "Answer."]);
    expect(projectProvidedAgentOutput(projected)).toEqual(projected);
  });

  it("pairs an old unlabelled delta with a new identity-bearing final", () => {
    const projected = projectProvidedAgentOutput([
      native("old-delta", "message.delta", "Par", 1),
      native(
        "new-final",
        "message.final",
        "Part one.",
        2,
        "turn",
        "native-1:0",
      ),
    ]);
    expect(projected.map((row) => row.content)).toEqual(["Part one."]);
    expect(projected[0].metadata?.blockId).toBe("native-1:0");
  });

  it("retains an unfinished trailing block and scopes snapshots to their own turn", () => {
    const rows = [
      native("d1", "message.delta", "First", 1),
      native("other", "message.delta", "Other turn", 2, "other"),
      native("f1", "message.final", "First block.", 3),
      native("d2", "message.delta", "Next", 4),
    ];
    expect(projectProvidedAgentOutput(rows).map((row) => row.content)).toEqual([
      "First block.",
      "Other turn",
      "Next",
    ]);
  });

  it("uses chronological sequence for replay and does not guess across a tool boundary", () => {
    const delta = native("d1", "message.delta", "Earlier partial", 1);
    const tool = native("tool", "tool.started", "", 2);
    const final = native("f1", "message.final", "Later narration", 3);
    expect(
      projectProvidedAgentOutput([final, tool, delta]).map((row) => row.id),
    ).toEqual(["f1", "tool", "d1"]);
    expect(
      projectProvidedAgentOutput([final, delta]).map((row) => row.id),
    ).toEqual(["f1"]);
  });

  it("does not replace partial text from another task or retry attempt", () => {
    const scoped = (row: TimelineEntry, taskId: string, attempt: number) => ({
      ...row,
      metadata: { ...row.metadata, taskId, attempt },
    });
    const rows = [
      scoped(native("d1", "message.delta", "Earlier attempt", 1), "task-a", 1),
      scoped(native("d2", "message.delta", "Other task", 2), "task-b", 2),
      scoped(native("d3", "message.delta", "Current", 3), "task-a", 2),
      scoped(native("f1", "message.final", "Current answer", 4), "task-a", 2),
    ];
    expect(ids(projectProvidedAgentOutput(rows))).toEqual(["d1", "d2", "f1"]);
  });

  it("pairs real REST/WS timestamps with different fractional precision and no sequence", () => {
    const partial = {
      ...native("d", "message.delta", "Checking", 1),
      sequence: undefined,
      createdAt: "2026-09-25T12:00:00.123Z",
    };
    const final = {
      ...native("f", "message.final", "Checking the files.", 2),
      sequence: undefined,
      createdAt: "2026-09-25T12:00:00.123001+00:00",
    };
    expect(ids(projectProvidedAgentOutput([final, partial]))).toEqual(["f"]);
  });

  it("uses the complete native payload when the timeline content is capped at 64 KiB", () => {
    const text = "x".repeat(64 * 1024) + "END_OF_ANSWER";
    const final = {
      ...native("f", "message.final", text, 3),
      content: text.slice(0, 64 * 1024),
    };
    const rows = [
      native("d1", "message.delta", text.slice(0, 40000), 1),
      native("d2", "message.delta", text.slice(40000), 2),
      final,
    ];
    const projected = projectProvidedAgentOutput(rows);
    expect(projected).toHaveLength(1);
    expect(projected[0].content).toBe(text);
    expect(final.content).toHaveLength(64 * 1024);
  });
});

function entry(overrides: Partial<TimelineEntry>): TimelineEntry {
  return {
    id: "e",
    entryType: "output",
    content: "",
    metadata: {},
    createdAt: "2026-07-18T00:00:00Z",
    ...overrides,
  };
}

// A per-text-block semantic delta row (Claude/Codex final-answer phase).
function outputDelta(
  id: string,
  turnId: string,
  content: string,
  extra: Record<string, unknown> = {},
): TimelineEntry {
  return entry({
    id,
    entryType: "output",
    content,
    metadata: {
      provider: "claude_code",
      providerEvent: "provided_agent.output.delta",
      phase: "output",
      streamPersistence: "semantic",
      messageId: turnId,
      turnId,
      blockId: `claude-code-${turnId}-text-0`,
      ...extra,
    },
  });
}

// The aggregate final answer row.
function outputFinal(
  id: string,
  turnId: string,
  content: string,
  extra: Record<string, unknown> = {},
): TimelineEntry {
  return entry({
    id,
    entryType: "output",
    content,
    metadata: {
      provider: "claude_code",
      providerEvent: "provided_agent.output.final",
      phase: "output",
      messageId: turnId,
      turnId,
      blockId: `claude-code-${turnId}-final`,
      ...extra,
    },
  });
}

const ids = (entries: TimelineEntry[]) => entries.map((e) => e.id);

describe("isProvidedAgentOutputEntry", () => {
  it("is true for output-phase delta and final rows", () => {
    expect(isProvidedAgentOutputEntry(outputDelta("d", "m1", "hi"))).toBe(true);
    expect(isProvidedAgentOutputEntry(outputFinal("f", "m1", "hi"))).toBe(true);
  });

  it("is false for Codex commentary deltas (phase status / checkpoint row)", () => {
    // Codex commentary carries providerEvent=output.delta but persists as a
    // checkpoint (phase "status"), so it is not an answer-output row.
    const commentary = entry({
      id: "c",
      entryType: "checkpoint",
      metadata: {
        providerEvent: "provided_agent.output.delta",
        phase: "status",
        turnId: "m1",
        messageId: "m1",
      },
    });
    expect(isProvidedAgentOutputEntry(commentary)).toBe(false);
  });

  it("is false for non-output families (thought, tool_call)", () => {
    expect(
      isProvidedAgentOutputEntry(
        entry({ entryType: "thought", metadata: { phase: "thought" } }),
      ),
    ).toBe(false);
    expect(
      isProvidedAgentOutputEntry(
        entry({ entryType: "tool_call", metadata: { phase: "tool_call" } }),
      ),
    ).toBe(false);
  });
});

describe("providedAgentOutputTurnKey", () => {
  it("prefers turnId, falls back to messageId, else null", () => {
    expect(
      providedAgentOutputTurnKey(
        entry({ metadata: { turnId: "t", messageId: "m" } }),
      ),
    ).toBe("t");
    expect(
      providedAgentOutputTurnKey(entry({ metadata: { messageId: "m" } })),
    ).toBe("m");
    expect(providedAgentOutputTurnKey(entry({ metadata: {} }))).toBeNull();
  });
});

describe("projectProvidedAgentOutput", () => {
  it("suppresses the aggregate final when a same-turn delta exists (one-block answer)", () => {
    const input = [
      outputDelta("d1", "m1", "The answer."),
      outputFinal("f1", "m1", "The answer."),
    ];
    expect(ids(projectProvidedAgentOutput(input))).toEqual(["d1"]);
  });

  it("keeps all delta blocks in order and drops only the aggregate final (multi-block)", () => {
    const input = [
      outputDelta("d1", "m1", "Part one."),
      outputDelta("d2", "m1", "Part two."),
      outputDelta("d3", "m1", "Part three."),
      outputFinal("f1", "m1", "Part one.Part two.Part three."),
    ];
    expect(ids(projectProvidedAgentOutput(input))).toEqual(["d1", "d2", "d3"]);
  });

  it("keeps a lone final that has no matching delta", () => {
    const input = [outputFinal("f1", "m1", "Only final.")];
    const out = projectProvidedAgentOutput(input);
    expect(ids(out)).toEqual(["f1"]);
    // No delta anywhere → returns the same reference (memo-stable).
    expect(out).toBe(input);
  });

  it("suppresses per turn — a delta for one turn never shadows another turn's final", () => {
    const input = [
      outputDelta("d1", "m1", "Turn one answer."),
      outputFinal("f1", "m1", "Turn one answer."),
      outputFinal("f2", "m2", "Turn two answer (no delta)."),
    ];
    // f1 dropped (m1 has a delta); f2 kept (m2 has no delta).
    expect(ids(projectProvidedAgentOutput(input))).toEqual(["d1", "f2"]);
  });

  it("matches on turnId — a final with a different turnId than the delta is not suppressed", () => {
    const input = [
      outputDelta("d1", "m1", "answer"),
      outputFinal("f1", "m2", "answer"),
    ];
    expect(ids(projectProvidedAgentOutput(input))).toEqual(["d1", "f1"]);
  });

  it("is provider-agnostic (Codex final-answer delta suppresses its final)", () => {
    const codexDelta = outputDelta("cd", "cm1", "Codex answer.", {
      provider: "codex",
      codexPhase: "final_answer",
    });
    const codexFinal = outputFinal("cf", "cm1", "Codex answer.", {
      provider: "codex",
    });
    expect(ids(projectProvidedAgentOutput([codexDelta, codexFinal]))).toEqual([
      "cd",
    ]);
  });

  it("does not let a Codex commentary checkpoint suppress the final", () => {
    // Commentary is a checkpoint (phase status), not an answer-output delta, so
    // the aggregate final is the only answer row and must be kept.
    const commentary = entry({
      id: "cc",
      entryType: "checkpoint",
      metadata: {
        provider: "codex",
        providerEvent: "provided_agent.output.delta",
        phase: "status",
        turnId: "cm1",
        messageId: "cm1",
      },
    });
    const codexFinal = outputFinal("cf", "cm1", "Codex answer.", {
      provider: "codex",
    });
    expect(ids(projectProvidedAgentOutput([commentary, codexFinal]))).toEqual([
      "cc",
      "cf",
    ]);
  });

  it("passes non-output families through untouched and preserves order", () => {
    const input = [
      entry({ id: "th", entryType: "thought", metadata: { phase: "thought" } }),
      entry({
        id: "tc",
        entryType: "tool_call",
        metadata: { phase: "tool_call" },
      }),
      entry({
        id: "tr",
        entryType: "tool_result",
        metadata: { phase: "tool_result" },
      }),
      outputDelta("d1", "m1", "answer"),
      outputFinal("f1", "m1", "answer"),
      entry({
        id: "ck",
        entryType: "checkpoint",
        metadata: { phase: "checkpoint" },
      }),
      entry({ id: "er", entryType: "error", metadata: { phase: "error" } }),
    ];
    expect(ids(projectProvidedAgentOutput(input))).toEqual([
      "th",
      "tc",
      "tr",
      "d1",
      "ck",
      "er",
    ]);
  });

  it("returns the same array reference when there is nothing to suppress", () => {
    const input = [
      entry({ id: "th", entryType: "thought", metadata: { phase: "thought" } }),
    ];
    expect(projectProvidedAgentOutput(input)).toBe(input);
  });

  it("preserves the input reference for a lone delta with no final to drop", () => {
    // A lone delta makes turnsWithDelta non-empty, but nothing is removed, so
    // the original reference must be returned (memo-stable).
    const input = [outputDelta("d1", "m1", "Streaming answer…")];
    const out = projectProvidedAgentOutput(input);
    expect(ids(out)).toEqual(["d1"]);
    expect(out).toBe(input);
  });
});

// A live-only partial row: the text so far of a still-streaming block.
function livePartial(
  id: string,
  turnId: string,
  content: string,
  createdAt: string,
  extra: Record<string, unknown> = {},
): TimelineEntry {
  return {
    ...outputDelta(id, turnId, content, {
      streamPersistence: "volatile",
      ...extra,
    }),
    createdAt,
  };
}

describe("projectProvidedAgentOutput live partial rows", () => {
  it("recognizes live partial rows", () => {
    expect(
      isVolatileProvidedAgentOutputDelta(
        livePartial("live-1", "turn-a", "par", "2026-07-18T00:00:01Z"),
      ),
    ).toBe(true);
    expect(
      isVolatileProvidedAgentOutputDelta(outputDelta("d-1", "turn-a", "x")),
    ).toBe(false);
  });

  it("keeps only a block's latest live row while it streams", () => {
    const entries = [
      livePartial("live-1", "turn-a", "partial ", "2026-07-18T00:00:01Z"),
      livePartial(
        "live-2",
        "turn-a",
        "partial review text",
        "2026-07-18T00:00:02Z",
      ),
    ];
    expect(ids(projectProvidedAgentOutput(entries))).toEqual(["live-2"]);
  });

  it("drops a block's live rows once its persisted delta exists", () => {
    const entries = [
      livePartial("live-1", "turn-a", "partial ", "2026-07-18T00:00:01Z"),
      livePartial(
        "live-2",
        "turn-a",
        "partial review text",
        "2026-07-18T00:00:02Z",
      ),
      {
        ...outputDelta("delta-1", "turn-a", "partial review text"),
        createdAt: "2026-07-18T00:00:03Z",
      },
    ];
    expect(ids(projectProvidedAgentOutput(entries))).toEqual(["delta-1"]);
  });

  it("keeps the live row of a block that has no persisted delta yet", () => {
    const entries = [
      outputDelta("delta-0", "turn-a", "first block"),
      livePartial("live-1", "turn-a", "second bl", "2026-07-18T00:00:05Z", {
        blockId: "claude-code-turn-a-text-1",
      }),
      livePartial("live-b", "turn-b", "other turn", "2026-07-18T00:00:06Z"),
    ];
    expect(ids(projectProvidedAgentOutput(entries))).toEqual([
      "delta-0",
      "live-1",
      "live-b",
    ]);
  });

  it("returns the input reference when no live row is superseded", () => {
    const entries = [
      livePartial("live-1", "turn-a", "partial ", "2026-07-18T00:00:01Z"),
    ];
    expect(projectProvidedAgentOutput(entries)).toBe(entries);
  });
});

describe("projectProvidedAgentOutput final result over live partial rows", () => {
  it("keeps the final result and drops live rows when no completed block arrived", () => {
    const entries = [
      livePartial(
        "live-1",
        "turn-a",
        "provisional text",
        "2026-07-18T00:00:01Z",
      ),
      {
        ...outputFinal("final-a", "turn-a", "authoritative final answer"),
        createdAt: "2026-07-18T00:00:02Z",
      },
    ];
    expect(ids(projectProvidedAgentOutput(entries))).toEqual(["final-a"]);
  });

  it("drops live rows that arrive after the final result", () => {
    const entries = [
      {
        ...outputFinal("final-a", "turn-a", "authoritative final answer"),
        createdAt: "2026-07-18T00:00:02Z",
      },
      livePartial(
        "live-late",
        "turn-a",
        "provisional text",
        "2026-07-18T00:00:03Z",
      ),
    ];
    expect(ids(projectProvidedAgentOutput(entries))).toEqual(["final-a"]);
  });

  it("still prefers the persisted delta over the final once the block completed", () => {
    const entries = [
      livePartial("live-1", "turn-a", "partial", "2026-07-18T00:00:01Z"),
      {
        ...outputDelta("delta-1", "turn-a", "partial answer"),
        createdAt: "2026-07-18T00:00:02Z",
      },
      {
        ...outputFinal("final-a", "turn-a", "partial answer"),
        createdAt: "2026-07-18T00:00:03Z",
      },
    ];
    expect(ids(projectProvidedAgentOutput(entries))).toEqual(["delta-1"]);
  });

  it("finalizing one turn leaves another turn's live rows in place", () => {
    const entries = [
      livePartial("live-a", "turn-a", "provisional a", "2026-07-18T00:00:01Z"),
      {
        ...outputFinal("final-a", "turn-a", "final a"),
        createdAt: "2026-07-18T00:00:02Z",
      },
      livePartial("live-b", "turn-b", "streaming b", "2026-07-18T00:00:03Z"),
    ];
    expect(ids(projectProvidedAgentOutput(entries))).toEqual([
      "final-a",
      "live-b",
    ]);
  });
});

describe("coalesceProvidedAgentOutputDeltas (g5.chat.live-tail-recovery)", () => {
  function delta(seq: number, text: string): TimelineEntry {
    return {
      id: `d-${seq}`,
      entryType: "output",
      content: text,
      sequence: seq,
      createdAt: `2026-11-02T00:00:${String(seq % 60).padStart(2, "0")}.${String(Math.floor(seq / 60)).padStart(3, "0")}Z`,
      metadata: {
        providerEvent: "provided_agent.output.delta",
        phase: "output",
        messageId: "turn-1",
        blockId: "block-1",
      },
    };
  }
  const run = (count: number, text: (seq: number) => string) =>
    Array.from({ length: count }, (_, index) =>
      delta(index + 1, text(index + 1)),
    );

  it("keeps an answer without paragraph breaks as one piece", () => {
    const pieces = coalesceProvidedAgentOutputDeltas(
      run(200, (seq) => `w${seq} `),
    );
    expect(pieces).toHaveLength(1);
    expect(pieces[0].content).toBe(
      run(200, (seq) => `w${seq} `)
        .map((entry) => entry.content)
        .join(""),
    );
  });

  it("closes a piece at the first paragraph break after sixty-four deltas, so earlier pieces stay stable", () => {
    const text = (seq: number) => (seq % 40 === 0 ? `w${seq}\n\n` : `w${seq} `);
    const pieces = coalesceProvidedAgentOutputDeltas(run(200, text));
    // Breaks fall at 40, 80, 120, 160, 200: pieces close at 80 and 160.
    expect(pieces.map((piece) => piece.id)).toEqual(["d-1", "d-81", "d-161"]);
    expect(pieces.map((piece) => piece.content).join("")).toBe(
      run(200, text)
        .map((entry) => entry.content)
        .join(""),
    );
    const longer = coalesceProvidedAgentOutputDeltas(run(230, text));
    expect(longer[0]).toEqual(pieces[0]);
    expect(longer[1]).toEqual(pieces[1]);
    expect(longer).toHaveLength(3);
  });

  it("never closes a piece inside a code fence", () => {
    const text = (seq: number) =>
      seq === 10
        ? "```\n"
        : seq === 150
          ? "```\n\n"
          : seq % 40 === 0
            ? `w${seq}\n\n`
            : `w${seq}\n`;
    const pieces = coalesceProvidedAgentOutputDeltas(run(200, text));
    // The breaks at 40, 80 and 120 fall inside the fence opened at 10; the
    // fence closes at 150 with a paragraph break, where the piece closes.
    expect(pieces.map((piece) => piece.id)).toEqual(["d-1", "d-151"]);
  });

  it("reads a longer fence as one marker and closes it only with a line at least as long", () => {
    // A six-backtick fence arrives as four backticks and two; a three-backtick
    // line inside it is content, not its close.
    const text = (seq: number) =>
      seq === 10
        ? "````"
        : seq === 11
          ? "``\n"
          : seq === 60
            ? "```\n\n"
            : seq === 150
              ? "``````\n\n"
              : seq % 40 === 0
                ? `w${seq}\n\n`
                : `w${seq}\n`;
    const pieces = coalesceProvidedAgentOutputDeltas(run(200, text));
    expect(pieces.map((piece) => piece.id)).toEqual(["d-1", "d-151"]);
  });

  it("counts a fence marker split across deltas, so a piece never closes inside the fence it opened", () => {
    // The opening fence arrives as "``" then "`"; the closing one as "`" then "``".
    const text = (seq: number) =>
      seq === 10
        ? "``"
        : seq === 11
          ? "`\n"
          : seq === 150
            ? "`"
            : seq === 151
              ? "``\n\n"
              : seq % 40 === 0
                ? `w${seq}\n\n`
                : `w${seq}\n`;
    const pieces = coalesceProvidedAgentOutputDeltas(run(200, text));
    expect(pieces.map((piece) => piece.id)).toEqual(["d-1", "d-152"]);
  });
});
