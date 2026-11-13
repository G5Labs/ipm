// =============================================================================
// G5 Client — QuestionCard Component Tests
// =============================================================================
// Tests the read-only `g5-question` card (center-panel D69): open payloads
// render header/prompt/options with the recommended badge, answered payloads
// render the answer summary, malformed and oversized payloads fall back to
// plain code with an inline notice, and the card exposes zero interactive
// controls.
// =============================================================================

import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { QuestionCard } from "./QuestionCard";
import { MAX_QUESTION_PAYLOAD_BYTES } from "../../lib/questionBlock";

const OPEN_PAYLOAD = `schema: 1
id: session-storage-backend
status: open
prompt: Which backend should persist chat session state?
severity: high
multi_select: false
allow_free_text: true
options:
  - label: Postgres table
    description: Reuse the existing primary database.
    recommended: true
  - label: Redis
    description: Store sessions in Redis.
    tradeoffs: Adds an infrastructure dependency.
    risks: Cache eviction can drop sessions.
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
`;

const ANSWERED_PAYLOAD = `schema: 1
id: session-storage-backend
status: answered
prompt: Which backend should persist chat session state?
options:
  - label: Postgres table
    description: Reuse the existing primary database.
    recommended: true
  - label: Redis
    description: Store sessions in Redis.
answer:
  selected:
    - Postgres table
  text: "Prefer fewer moving parts."
  rationale: "We already operate Postgres."
  answered_by: user
  answered_at: "2026-08-10T12:00:00Z"
`;

describe("QuestionCard", () => {
  describe("open payload", () => {
    it("renders header, prompt, and options with the recommended badge", () => {
      render(<QuestionCard content={OPEN_PAYLOAD} />);

      expect(screen.getByTestId("question-card")).toBeInTheDocument();
      expect(screen.getByText("session-storage-backend")).toBeInTheDocument();
      expect(screen.getByTestId("question-card-status-chip")).toHaveTextContent(
        "Open",
      );
      expect(
        screen.getByTestId("question-card-severity-chip"),
      ).toHaveTextContent("High");
      expect(
        screen.getByText("Which backend should persist chat session state?"),
      ).toBeInTheDocument();

      const options = screen.getAllByTestId("escalation-option");
      expect(options).toHaveLength(2);
      expect(options[0]).toHaveTextContent("Postgres table");
      expect(options[0]).toHaveTextContent("Recommended");
      expect(options[1]).toHaveTextContent("Redis");
      expect(options[1]).not.toHaveTextContent("Recommended");
    });

    it("renders tradeoffs and risks statically expanded", () => {
      render(<QuestionCard content={OPEN_PAYLOAD} />);

      expect(
        screen.getByText(/Adds an infrastructure dependency\./),
      ).toBeInTheDocument();
      expect(
        screen.getByText(/Cache eviction can drop sessions\./),
      ).toBeInTheDocument();
    });

    it("does not render an answer summary for an open question", () => {
      render(<QuestionCard content={OPEN_PAYLOAD} />);

      expect(
        screen.queryByTestId("question-card-answered-summary"),
      ).not.toBeInTheDocument();
    });
  });

  describe("answered payload", () => {
    it("renders the answer summary with selection, text, rationale, and byline", () => {
      render(<QuestionCard content={ANSWERED_PAYLOAD} />);

      expect(screen.getByTestId("question-card-status-chip")).toHaveTextContent(
        "Answered",
      );
      const summary = screen.getByTestId("question-card-answered-summary");
      expect(summary).toHaveTextContent("Postgres table");
      expect(summary).toHaveTextContent("Prefer fewer moving parts.");
      expect(summary).toHaveTextContent(
        "Rationale: We already operate Postgres.",
      );
      expect(summary).toHaveTextContent(
        "Answered by user at 2026-08-10T12:00:00Z",
      );
    });
  });

  describe("fallbacks", () => {
    it("renders malformed YAML as plain code with an invalid notice", () => {
      render(<QuestionCard content={"schema: 1\n  bad:\n indent: ]["} />);

      const fallback = screen.getByTestId("question-card-fallback");
      expect(fallback).toBeInTheDocument();
      expect(
        screen.getByTestId("question-card-invalid-notice"),
      ).toHaveTextContent(/Invalid question payload/);
      expect(fallback.querySelector("pre code")).not.toBeNull();
      expect(screen.queryByTestId("question-card")).not.toBeInTheDocument();
    });

    it("renders a non-question mapping as plain code with an invalid notice", () => {
      render(<QuestionCard content={"just a scalar"} />);

      expect(
        screen.getByTestId("question-card-invalid-notice"),
      ).toBeInTheDocument();
    });

    it("renders a legacy pre-schema payload as plain code, naming the expected declaration (#1872)", () => {
      const legacy =
        "id: Q1\n" +
        "title: Exclusion mechanism for the report UI\n" +
        "question: Prefer a data-attribute marker or explicit node refs?\n" +
        "options:\n" +
        "  - label: data-screenshot-exclude attribute\n" +
        "    description: Flag the drawer container with a data attribute.\n";
      render(<QuestionCard content={legacy} />);

      expect(
        screen.getByTestId("question-card-invalid-notice"),
      ).toHaveTextContent(
        "Invalid question payload (schema: undefined; expected schema: 1) — rendering as plain code.",
      );
      const fallback = screen.getByTestId("question-card-fallback");
      expect(fallback.querySelector("pre code")).not.toBeNull();
      expect(screen.queryByTestId("question-card")).not.toBeInTheDocument();
    });

    it("renders an oversized payload as plain code with the oversized notice", () => {
      const oversized = `prompt: ${"x".repeat(MAX_QUESTION_PAYLOAD_BYTES)}`;
      render(<QuestionCard content={oversized} />);

      expect(
        screen.getByTestId("question-card-oversized-notice"),
      ).toHaveTextContent(/32 KiB cap/);
      expect(screen.queryByTestId("question-card")).not.toBeInTheDocument();
    });
  });

  describe("read-only contract", () => {
    it("exposes no interactive controls for open or answered payloads", () => {
      const { container: openContainer } = render(
        <QuestionCard content={OPEN_PAYLOAD} />,
      );
      const { container: answeredContainer } = render(
        <QuestionCard content={ANSWERED_PAYLOAD} />,
      );

      for (const container of [openContainer, answeredContainer]) {
        expect(
          container.querySelectorAll(
            "button, textarea, input, select, a, [role='button']",
          ),
        ).toHaveLength(0);
      }
    });
  });
});

describe("QuestionCard — urgency language parity (D69)", () => {
  const withSeverity = (severity: string, answered = false) =>
    [
      "schema: 1",
      "id: parity-check",
      `status: ${answered ? "answered" : "open"}`,
      "prompt: Ship it?",
      `severity: ${severity}`,
      "options:",
      "  - label: Yes",
      "    description: Ship",
      ...(answered
        ? [
            "answer:",
            "  selected:",
            "    - Yes",
            '  text: ""',
            '  rationale: ""',
            "  answered_by:",
            "    kind: user",
            "  answered_at: 2026-08-11T09:00:00Z",
          ]
        : []),
      "",
    ].join("\n");

  it("carries the blocking chip the interactive card has, not a partial mapping", () => {
    render(<QuestionCard content={withSeverity("critical")} />);
    expect(screen.getByTestId("question-card-blocking-chip")).toHaveTextContent(
      "Blocking",
    );
  });

  it("drops every urgency signal once answered, chip colour included", () => {
    render(<QuestionCard content={withSeverity("critical", true)} />);
    expect(
      screen.queryByTestId("question-card-blocking-chip"),
    ).not.toBeInTheDocument();
    // The severity TEXT stays (useful history); only the urgency colour goes.
    const chip = screen.getByTestId("question-card-severity-chip");
    expect(chip).toHaveTextContent("Critical");
    expect(chip.className).not.toContain("blocked");
  });

  // This card is rendered standalone by `MarkdownRenderer`, with no enclosing
  // `[Q<N>.]` record frame to inherit an accent from. Without its own spine and
  // tint the read-only surface shows no urgency at all, which is the partial
  // mapping D69 forbids.
  it("paints the urgency spine and tint for an open critical question", () => {
    render(<QuestionCard content={withSeverity("critical")} />);
    const card = screen.getByTestId("question-card");
    expect(card.style.borderLeftColor).toBe("var(--g5-blocked)");
    // The blocking tier is thicker than the ordinary spine, and tinted.
    expect(card.style.borderLeftWidth).toBe("5px");
    expect(card.style.backgroundColor).toContain("var(--g5-blocked)");
  });

  it("paints a plain severity spine with no tint below the blocking tier", () => {
    render(<QuestionCard content={withSeverity("medium")} />);
    const card = screen.getByTestId("question-card");
    expect(card.style.borderLeftColor).toBe("var(--g5-warn)");
    expect(card.style.borderLeftWidth).toBe("3px");
    expect(card.style.backgroundColor).toBe("");
  });

  it("keeps the OPEN treatment for an agent-proposed answer", () => {
    // Only a USER provenance is a settled human decision (D10), so an agent
    // answer must not spend the urgency: the spine stays red and thick, the
    // Blocking chip stays, and the status chip does not take the human green.
    // Without this gate the card reads as decided when nobody decided.
    const agentAnswered = withSeverity("critical", true).replace(
      "    kind: user",
      "    kind: agent",
    );
    render(<QuestionCard content={agentAnswered} />);

    const card = screen.getByTestId("question-card");
    expect(card.style.borderLeftColor).toBe("var(--g5-blocked)");
    expect(card.style.borderLeftWidth).toBe("5px");
    expect(
      screen.getByTestId("question-card-blocking-chip"),
    ).toBeInTheDocument();
    // The chip still REPORTS the payload, but not in the decided colour.
    const status = screen.getByTestId("question-card-status-chip");
    expect(status).toHaveTextContent("Answered");
    expect(status.className).not.toContain("ok");
    // Severity is not muted either — the question is still urgent.
    expect(
      screen.getByTestId("question-card-severity-chip").className,
    ).not.toContain("muted");
  });

  it("returns the spine to neutral once answered", () => {
    render(<QuestionCard content={withSeverity("critical", true)} />);
    const card = screen.getByTestId("question-card");
    expect(card.style.borderLeftColor).toBe("var(--g5-stroke)");
    expect(card.style.borderLeftWidth).toBe("3px");
    expect(card.style.backgroundColor).toBe("");
  });
});
