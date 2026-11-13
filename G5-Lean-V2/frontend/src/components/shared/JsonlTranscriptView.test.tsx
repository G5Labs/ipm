// =============================================================================
// G5 Client — JsonlTranscriptView Component Tests (center-panel D61)
// =============================================================================
// Covers Claude Code session-transcript rendering (user/assistant messages,
// thinking, tool_use/tool_result, meta chips, sidechain marking), malformed
// line resilience, the generic JSONL fallback, chunked reveal, and the
// rich/source toggle contract.
// =============================================================================

import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { JsonlTranscriptView } from "./JsonlTranscriptView";
import { isClaudeTranscript, parseJsonlDocument } from "./jsonlTranscript";

// Mock diagram widgets that MarkdownRenderer pulls in transitively.
vi.mock("./MermaidDiagram", () => ({
  MermaidDiagram: ({ code }: { code: string }) => (
    <div data-testid="mermaid-diagram">{code}</div>
  ),
}));

vi.mock("./ReactFlowDiagramWidget", () => ({
  ReactFlowDiagramWidget: ({ code }: { code: string }) => (
    <div data-testid="react-flow-diagram-widget">{code}</div>
  ),
}));

const SESSION_ID = "d913b22c-7cce-4524-819c-b1ce2740e9cb";

function line(record: unknown): string {
  return JSON.stringify(record);
}

const LONG_THOUGHT = `${"Considering the layout options carefully. ".repeat(6)}FINAL-THOUGHT-TAIL`;

const CLAUDE_TRANSCRIPT = [
  line({ type: "mode", mode: "normal", sessionId: SESSION_ID }),
  line({
    type: "permission-mode",
    permissionMode: "auto",
    sessionId: SESSION_ID,
  }),
  line({
    type: "user",
    message: { role: "user", content: "Rename the **ship** button" },
    sessionId: SESSION_ID,
  }),
  line({
    type: "assistant",
    message: {
      role: "assistant",
      content: [
        { type: "thinking", thinking: LONG_THOUGHT },
        { type: "text", text: "Here are three naming options." },
        { type: "tool_use", name: "Bash", input: { command: "ls mocks/" } },
      ],
    },
    sessionId: SESSION_ID,
  }),
  line({
    type: "user",
    message: {
      role: "user",
      content: [
        {
          type: "tool_result",
          content: [{ type: "text", text: "prepare-to-ship.png" }],
        },
      ],
    },
    sessionId: SESSION_ID,
  }),
  line({
    type: "assistant",
    isSidechain: true,
    message: {
      role: "assistant",
      content: [{ type: "text", text: "Sidechain exploration result" }],
    },
    sessionId: SESSION_ID,
  }),
].join("\n");

function renderView(content: string, maxParsedLines?: number) {
  render(
    <JsonlTranscriptView
      displayPath="docs/ws-1/claude-session.jsonl"
      content={content}
      {...(maxParsedLines !== undefined ? { maxParsedLines } : {})}
    />,
  );
}

// ---------------------------------------------------------------------------
// Parsing helpers
// ---------------------------------------------------------------------------

describe("parseJsonlDocument", () => {
  it("skips blank lines, keeps 1-based line numbers, and captures parse errors", () => {
    const parsed = parseJsonlDocument('{"a":1}\n\nnot-json\n{"b":2}\n');
    expect(parsed.truncated).toBe(false);
    expect(parsed.totalNonEmptyLines).toBe(3);
    const records = parsed.records;
    expect(records).toHaveLength(3);
    expect(records[0]).toMatchObject({ lineNumber: 1, json: { a: 1 } });
    expect(records[1].lineNumber).toBe(3);
    expect(records[1].parseError).toBeTruthy();
    expect(records[1].raw).toBe("not-json");
    expect(records[2]).toMatchObject({ lineNumber: 4, json: { b: 2 } });
  });

  it("stops parsing at the bound while still counting remaining lines", () => {
    const parsed = parseJsonlDocument(
      Array.from({ length: 7 }, (_, i) => `{"n":${i}}`).join("\n"),
      3,
    );
    expect(parsed.records).toHaveLength(3);
    expect(parsed.records[2].json).toEqual({ n: 2 });
    expect(parsed.totalNonEmptyLines).toBe(7);
    expect(parsed.truncated).toBe(true);
  });

  it("handles CRLF line endings and whitespace-only lines", () => {
    const parsed = parseJsonlDocument('{"a":1}\r\n \t\r\n{"b":2}\r\n');
    expect(parsed.totalNonEmptyLines).toBe(2);
    expect(parsed.records).toHaveLength(2);
    expect(parsed.records[0]).toMatchObject({
      lineNumber: 1,
      raw: '{"a":1}',
      json: { a: 1 },
    });
    expect(parsed.records[1]).toMatchObject({ lineNumber: 3, json: { b: 2 } });
  });
});

describe("isClaudeTranscript", () => {
  it("detects records with type user/assistant carrying a message object", () => {
    expect(
      isClaudeTranscript(parseJsonlDocument(CLAUDE_TRANSCRIPT).records),
    ).toBe(true);
    expect(
      isClaudeTranscript(
        parseJsonlDocument('{"type":"metric","value":3}').records,
      ),
    ).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// Claude transcript layout
// ---------------------------------------------------------------------------

describe("JsonlTranscriptView — Claude transcript", () => {
  it("renders user and assistant messages with the transcript header stats", () => {
    renderView(CLAUDE_TRANSCRIPT);

    expect(screen.getByTestId("jsonl-transcript-view")).toBeInTheDocument();
    expect(
      screen.getByText("Claude Code session transcript"),
    ).toBeInTheDocument();
    expect(screen.getByText("2 user")).toBeInTheDocument();
    expect(screen.getByText("2 assistant")).toBeInTheDocument();
    expect(screen.getByText("1 tool calls")).toBeInTheDocument();
    expect(screen.getByText(`session ${SESSION_ID}`)).toBeInTheDocument();

    // User markdown content renders through MarkdownRenderer (bold applied).
    expect(screen.getByText("ship")).toBeInTheDocument();
    // Assistant text block renders.
    expect(
      screen.getByText("Here are three naming options."),
    ).toBeInTheDocument();
  });

  it("collapses thinking and tool blocks until expanded", async () => {
    renderView(CLAUDE_TRANSCRIPT);

    // The thinking tail only exists in the expanded body (header detail is
    // truncated to 120 chars).
    expect(screen.queryByText(/FINAL-THOUGHT-TAIL/)).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /Thinking/ }));
    await waitFor(() =>
      expect(screen.getByText(/FINAL-THOUGHT-TAIL/)).toBeInTheDocument(),
    );

    // Tool call is named in the collapsed header; input appears on expand.
    const toolButton = screen.getByRole("button", { name: /Tool: Bash/ });
    expect(screen.queryByText(/ls mocks\//)).not.toBeInTheDocument();
    fireEvent.click(toolButton);
    await waitFor(() =>
      expect(screen.getByText(/ls mocks\//)).toBeInTheDocument(),
    );

    // Tool result row expands to the textual result payload.
    fireEvent.click(screen.getByRole("button", { name: /Tool result/ }));
    await waitFor(() =>
      expect(screen.getByText(/prepare-to-ship\.png/)).toBeInTheDocument(),
    );
  });

  it("renders meta records as collapsed chips and marks sidechain records", async () => {
    renderView(CLAUDE_TRANSCRIPT);

    expect(screen.getByText("mode: normal")).toBeInTheDocument();
    expect(screen.getByText("permission mode: auto")).toBeInTheDocument();
    expect(screen.getByText("sidechain")).toBeInTheDocument();
    expect(
      screen.getByText("Sidechain exploration result"),
    ).toBeInTheDocument();

    // Meta chip expands to the full record JSON.
    fireEvent.click(screen.getByRole("button", { name: /mode: normal/ }));
    await waitFor(() =>
      expect(screen.getByText(/"sessionId"/)).toBeInTheDocument(),
    );
  });

  it("renders malformed lines inline without breaking the rest", () => {
    const withBadLine = `${CLAUDE_TRANSCRIPT}\n{broken json`;
    renderView(withBadLine);

    expect(screen.getByText(/Line 7 is not valid JSON/)).toBeInTheDocument();
    expect(screen.getByText(/\{broken json/)).toBeInTheDocument();
    expect(screen.getByText("1 malformed")).toBeInTheDocument();
    // Well-formed entries still render.
    expect(
      screen.getByText("Here are three naming options."),
    ).toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// Generic fallback and chunked reveal
// ---------------------------------------------------------------------------

describe("JsonlTranscriptView — generic JSONL", () => {
  it("falls back to a generic record list for non-Claude JSONL", async () => {
    renderView(
      [
        line({ type: "metric", value: 3 }),
        line({ event: "deploy", ok: true }),
      ].join("\n"),
    );

    expect(screen.getByText("JSONL records")).toBeInTheDocument();
    expect(screen.getByText("#1")).toBeInTheDocument();
    expect(screen.getByText("metric")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /#2/ }));
    await waitFor(() =>
      expect(screen.getByText(/"deploy"/)).toBeInTheDocument(),
    );
  });

  it("reveals long documents in chunks through an explicit control", async () => {
    const manyLines = Array.from({ length: 250 }, (_, i) =>
      line({ type: "metric", value: i }),
    ).join("\n");
    renderView(manyLines);

    expect(screen.getByText("250 lines")).toBeInTheDocument();
    expect(screen.getByText("#200")).toBeInTheDocument();
    expect(screen.queryByText("#201")).not.toBeInTheDocument();

    fireEvent.click(
      screen.getByRole("button", { name: /Show 50 more entries/ }),
    );
    await waitFor(() => expect(screen.getByText("#250")).toBeInTheDocument());
    expect(
      screen.queryByRole("button", { name: /more entries/ }),
    ).not.toBeInTheDocument();
  });

  it("shows an explicit notice when parsing stops at the bound", () => {
    renderView(
      Array.from({ length: 5 }, (_, i) =>
        line({ type: "metric", value: i }),
      ).join("\n"),
      2,
    );

    expect(screen.getByText("5 lines")).toBeInTheDocument();
    expect(screen.getByText("parsed first 2")).toBeInTheDocument();
    expect(
      screen.getByText(/Parsing stopped after 2 records/),
    ).toBeInTheDocument();
    expect(screen.getByText(/3 more lines are not shown/)).toBeInTheDocument();
    expect(screen.getByText("#2")).toBeInTheDocument();
    expect(screen.queryByText("#3")).not.toBeInTheDocument();
  });

  it("shows an empty-document hint instead of an empty list", () => {
    renderView("\n\n");
    expect(screen.getByText(/no JSONL records yet/)).toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// Preview-shell header contract
// ---------------------------------------------------------------------------

describe("JsonlTranscriptView — header", () => {
  it("shows the display path without a second source-mode control", () => {
    renderView(CLAUDE_TRANSCRIPT);
    expect(
      screen.getByText("docs/ws-1/claude-session.jsonl"),
    ).toBeInTheDocument();
    expect(screen.queryByRole("group", { name: "Editor mode" })).toBeNull();
  });
});
