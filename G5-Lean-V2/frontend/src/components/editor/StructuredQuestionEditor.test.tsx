// =============================================================================
// StructuredQuestionEditor tests — interactive `g5-question` card (D69)
// =============================================================================
// Render matrix (open / answered / invalid / oversized / zero options /
// capability gating), selection semantics (single vs multi, Something else),
// Answer gating, module-scoped draft LRU survival, Discuss callback, and the
// duplicate-id first-wins notice.
// =============================================================================

import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { act } from "react";
import { describe, expect, it, vi } from "vitest";
import {
  StructuredQuestionEditor,
  type QuestionApplyResult,
  type StructuredQuestionEditorProps,
} from "./StructuredQuestionEditor";
import type { Block } from "../../lib/utils";
import {
  MAX_QUESTION_PAYLOAD_BYTES,
  parseQuestionPayload,
  type AnswerInput,
  type ParsedQuestion,
} from "../../lib/questionBlock";

function questionBlock(content: string): Extract<Block, { type: "code" }> {
  return { type: "code", language: "g5-question", content };
}

const OPEN_PAYLOAD = [
  "schema: 1",
  "id: pick-color",
  "status: open",
  "prompt: Which color should the button use?",
  "severity: high",
  "options:",
  "  - label: Red",
  "    description: Use the red token",
  "    tradeoffs: Clashes with error states",
  "    risks: Users may read it as destructive",
  "    recommended: true",
  "  - label: Blue",
  "    description: Use the info token",
  "",
].join("\n");

const MULTI_PAYLOAD = [
  "schema: 1",
  "id: pick-many",
  "prompt: Which platforms?",
  "multi_select: true",
  "options:",
  "  - label: Web",
  "    description: Browser client",
  "  - label: Mobile",
  "    description: Mobile app",
  "",
].join("\n");

const ANSWERED_PAYLOAD = [
  "schema: 1",
  "id: pick-color",
  "status: answered",
  "prompt: Which color should the button use?",
  "options:",
  "  - label: Red",
  "    description: Use the red token",
  "  - label: Blue",
  "    description: Use the info token",
  "answer:",
  "  selected:",
  "    - Blue",
  "  text: ''",
  "  rationale: Matches the info palette",
  "  answered_by: user",
  "  answered_at: 2026-08-01T12:00:00Z",
  "",
].join("\n");

const FREE_TEXT_ONLY_PAYLOAD = [
  "schema: 1",
  "id: free-form",
  "prompt: Anything else we should know?",
  "options: []",
  "",
].join("\n");

// Typed against the component's own props so a spread override keeps its
// named properties: a `Record<string, unknown>` override contributes only an
// index signature, which erases props like `onSaveAnswers` from the inferred
// return type and breaks `tsc -b` at the assertion sites.
function baseProps(
  overrides: Partial<StructuredQuestionEditorProps> = {},
): StructuredQuestionEditorProps {
  return {
    block: questionBlock(OPEN_PAYLOAD),
    docPath: "specs/example.md",
    fallbackId: "q1",
    canSave: true,
    ...overrides,
  };
}

describe("StructuredQuestionEditor — Something else replaces the options (D69)", () => {
  it("clears a picked option when the user switches to their own answer", async () => {
    const onApplyAnswer = vi
      .fn<(q: ParsedQuestion, a: AnswerInput) => Promise<QuestionApplyResult>>()
      .mockResolvedValue({ status: "applied" });
    render(
      <StructuredQuestionEditor
        {...baseProps({ docPath: "specs/own-answer.md", onApplyAnswer })}
      />,
    );

    // Pick an offered option, then decide to write your own instead.
    fireEvent.click(screen.getAllByTestId("question-option-toggle")[0]);
    fireEvent.click(screen.getByTestId("question-something-else"));
    fireEvent.change(screen.getByTestId("question-free-text"), {
      target: { value: "Neither — use the existing exporter" },
    });
    fireEvent.click(screen.getByTestId("question-answer-button"));

    await waitFor(() => expect(onApplyAnswer).toHaveBeenCalledTimes(1));
    const input = onApplyAnswer.mock.calls[0][1];
    expect(input.text).toBe("Neither — use the existing exporter");
    // The option the user moved away from must not be recorded: the summary
    // would show it, and an agent reading `selected` would act on it.
    expect(input.selected).toEqual([]);
    // Every option toggle reads as unpicked in the UI too.
    for (const toggle of screen.getAllByTestId("question-option-toggle")) {
      expect(toggle).toHaveAttribute("aria-pressed", "false");
    }
  });

  it("closes the free text when the user goes back to an offered option", async () => {
    const onApplyAnswer = vi
      .fn<(q: ParsedQuestion, a: AnswerInput) => Promise<QuestionApplyResult>>()
      .mockResolvedValue({ status: "applied" });
    render(
      <StructuredQuestionEditor
        {...baseProps({ docPath: "specs/back-to-option.md", onApplyAnswer })}
      />,
    );

    fireEvent.click(screen.getByTestId("question-something-else"));
    fireEvent.change(screen.getByTestId("question-free-text"), {
      target: { value: "my own words" },
    });
    fireEvent.click(screen.getAllByTestId("question-option-toggle")[0]);
    fireEvent.click(screen.getByTestId("question-answer-button"));

    await waitFor(() => expect(onApplyAnswer).toHaveBeenCalledTimes(1));
    const input = onApplyAnswer.mock.calls[0][1];
    expect(input.selected).toEqual(["Red"]);
    expect(input.text).toBe("");
  });

  it("normalizes a LEGACY answer carrying both an option and free text", async () => {
    // Answers written under the previous contract can legitimately hold both.
    // Change answer must not copy the pair through, or the very next Save
    // re-submits the contradictory decision this exclusion exists to remove.
    const legacyMixed = [
      "schema: 1",
      "id: pick-color",
      "status: answered",
      "prompt: Which color should the button use?",
      "options:",
      "  - label: Red",
      "    description: Use the red token",
      "  - label: Blue",
      "    description: Use the info token",
      "answer:",
      "  selected:",
      "    - Blue",
      "  text: Neither — use the brand token",
      "  rationale: Recorded before the fields became exclusive",
      "  answered_by: user",
      "  answered_at: 2026-08-01T12:00:00Z",
      "",
    ].join("\n");
    const onApplyAnswer = vi
      .fn<(q: ParsedQuestion, a: AnswerInput) => Promise<QuestionApplyResult>>()
      .mockResolvedValue({ status: "applied" });
    render(
      <StructuredQuestionEditor
        {...baseProps({
          block: questionBlock(legacyMixed),
          docPath: "specs/legacy-mixed.md",
          onApplyAnswer,
        })}
      />,
    );

    fireEvent.click(screen.getByTestId("question-change-answer"));

    // Free text present means the user answered in their own words, so that
    // branch opens and the stale option selection is dropped. Awaited: the
    // click swaps the answered summary for the form, so asserting (or clicking
    // Save) synchronously would race that re-render.
    expect(await screen.findByTestId("question-free-text")).toHaveValue(
      "Neither — use the brand token",
    );
    for (const toggle of screen.getAllByTestId("question-option-toggle")) {
      expect(toggle).toHaveAttribute("aria-pressed", "false");
    }

    fireEvent.click(screen.getByTestId("question-answer-button"));
    await waitFor(() => expect(onApplyAnswer).toHaveBeenCalledTimes(1));
    const input = onApplyAnswer.mock.calls[0][1];
    expect(input.text).toBe("Neither — use the brand token");
    expect(input.selected).toEqual([]);
    // The user's own reasoning is their words too — it survives untouched.
    expect(input.rationale).toBe("Recorded before the fields became exclusive");
  });
});

describe("StructuredQuestionEditor — no duplicated status/severity (D69)", () => {
  it("leaves status and severity to the enclosing record's block tags", () => {
    const block = questionBlock(
      [
        "schema: 1",
        "id: urgent-one",
        "status: open",
        "prompt: Ship it?",
        "severity: critical",
        "options:",
        "  - label: Yes",
        "    description: Ship",
        "",
      ].join("\n"),
    );
    render(<StructuredQuestionEditor {...baseProps({ block })} />);

    // Reported once, on the record row — not twice in two visual languages.
    expect(
      screen.queryByTestId("question-status-chip"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("question-severity-chip"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("question-blocking-chip"),
    ).not.toBeInTheDocument();
    // The question's own content still renders.
    expect(screen.getByText("Ship it?")).toBeInTheDocument();
  });
});

describe("StructuredQuestionEditor — render matrix", () => {
  it("renders an open question card with id, chips, prompt, and options", () => {
    render(
      <StructuredQuestionEditor
        {...baseProps({ docPath: "specs/matrix-open.md" })}
      />,
    );

    expect(
      screen.getByTestId("structured-question-editor"),
    ).toBeInTheDocument();
    expect(screen.getByText("pick-color")).toBeInTheDocument();
    expect(
      screen.getByText("Which color should the button use?"),
    ).toBeInTheDocument();
    const options = screen.getAllByTestId("question-option-toggle");
    expect(options).toHaveLength(2);
    expect(screen.getByText("Recommended")).toBeInTheDocument();
    // Tradeoffs/risks stay collapsed until toggled.
    expect(
      screen.queryByText(/Clashes with error states/),
    ).not.toBeInTheDocument();
    fireEvent.click(screen.getByTestId("question-option-details-toggle"));
    expect(screen.getByText(/Clashes with error states/)).toBeInTheDocument();
    expect(
      screen.getByText(/Users may read it as destructive/),
    ).toBeInTheDocument();
  });

  it("renders the answered summary and returns to a pre-filled form on Change answer", () => {
    const onApplyAnswer = vi
      .fn<(q: ParsedQuestion, a: AnswerInput) => Promise<QuestionApplyResult>>()
      .mockResolvedValue({ status: "applied" });
    render(
      <StructuredQuestionEditor
        {...baseProps({
          block: questionBlock(ANSWERED_PAYLOAD),
          docPath: "specs/matrix-answered.md",
          onApplyAnswer,
        })}
      />,
    );

    const summary = screen.getByTestId("question-answered-summary");
    expect(summary).toHaveTextContent("Blue");
    expect(summary).toHaveTextContent("Matches the info palette");
    expect(summary).toHaveTextContent("Answered by user");
    expect(
      screen.queryByTestId("question-option-toggle"),
    ).not.toBeInTheDocument();

    fireEvent.click(screen.getByTestId("question-change-answer"));
    const options = screen.getAllByTestId("question-option-toggle");
    expect(options[0]).toHaveAttribute("aria-pressed", "false");
    expect(options[1]).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByTestId("question-answer-button")).toBeEnabled();
  });

  it("renders an invalid payload as plain code with an inline notice", () => {
    render(
      <StructuredQuestionEditor
        {...baseProps({
          block: questionBlock("schema: 1\nprompt: [unclosed"),
          docPath: "specs/matrix-invalid.md",
        })}
      />,
    );

    expect(
      screen.queryByTestId("structured-question-editor"),
    ).not.toBeInTheDocument();
    expect(screen.getByTestId("question-invalid-notice")).toHaveTextContent(
      /Invalid question payload/,
    );
    const fallback = screen.getByTestId("structured-question-fallback");
    expect(fallback.querySelector("code")).toHaveTextContent("[unclosed");
  });

  it("renders an unsupported schema as plain code with an inline notice", () => {
    render(
      <StructuredQuestionEditor
        {...baseProps({
          block: questionBlock("schema: 2\nprompt: future"),
          docPath: "specs/matrix-schema.md",
        })}
      />,
    );

    expect(screen.getByTestId("question-invalid-notice")).toHaveTextContent(
      /Invalid question payload/,
    );
  });

  it("renders a legacy pre-schema payload as plain code, naming the expected declaration (#1872)", () => {
    const legacy =
      "id: Q1\n" +
      "title: Exclusion mechanism for the report UI\n" +
      "question: Prefer a data-attribute marker or explicit node refs?\n" +
      "options:\n" +
      "  - label: data-screenshot-exclude attribute\n" +
      "    description: Flag the drawer container with a data attribute.\n";
    render(
      <StructuredQuestionEditor
        {...baseProps({
          block: questionBlock(legacy),
          docPath: "specs/matrix-legacy.md",
        })}
      />,
    );

    expect(
      screen.queryByTestId("structured-question-editor"),
    ).not.toBeInTheDocument();
    expect(screen.getByTestId("question-invalid-notice")).toHaveTextContent(
      "Invalid question payload (schema: undefined; expected schema: 1) — rendering as plain code.",
    );
    const fallback = screen.getByTestId("structured-question-fallback");
    expect(fallback.querySelector("code")).toHaveTextContent(
      "data-screenshot-exclude attribute",
    );
  });

  it("renders an oversized payload read-only with a notice", () => {
    const oversized =
      "schema: 1\nprompt: big\nfiller: " +
      "x".repeat(MAX_QUESTION_PAYLOAD_BYTES);
    render(
      <StructuredQuestionEditor
        {...baseProps({
          block: questionBlock(oversized),
          docPath: "specs/matrix-oversized.md",
        })}
      />,
    );

    expect(
      screen.queryByTestId("structured-question-editor"),
    ).not.toBeInTheDocument();
    expect(screen.getByTestId("question-oversized-notice")).toHaveTextContent(
      /read-only/,
    );
  });

  it("renders a zero-option question as a pure free-text form", () => {
    render(
      <StructuredQuestionEditor
        {...baseProps({
          block: questionBlock(FREE_TEXT_ONLY_PAYLOAD),
          docPath: "specs/matrix-freeform.md",
          onApplyAnswer: vi.fn(),
        })}
      />,
    );

    expect(
      screen.queryByTestId("question-option-toggle"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("question-something-else"),
    ).not.toBeInTheDocument();
    expect(screen.getByTestId("question-free-text")).toBeInTheDocument();
    expect(screen.getByTestId("question-answer-button")).toBeDisabled();
    fireEvent.change(screen.getByTestId("question-free-text"), {
      target: { value: "Ship it" },
    });
    expect(screen.getByTestId("question-answer-button")).toBeEnabled();
  });

  it("hides Answer when canSave is false and when no apply callback exists", () => {
    const { unmount } = render(
      <StructuredQuestionEditor
        {...baseProps({
          docPath: "specs/matrix-readonly.md",
          canSave: false,
          onApplyAnswer: vi.fn(),
        })}
      />,
    );
    expect(
      screen.queryByTestId("question-answer-button"),
    ).not.toBeInTheDocument();
    unmount();

    // No onApplyAnswer (task 1.1 wiring): Answer stays hidden even when the
    // editor itself is editable.
    render(
      <StructuredQuestionEditor
        {...baseProps({ docPath: "specs/matrix-nocallback.md" })}
      />,
    );
    expect(
      screen.queryByTestId("question-answer-button"),
    ).not.toBeInTheDocument();
    // Selection still works in display-only mode.
    const options = screen.getAllByTestId("question-option-toggle");
    fireEvent.click(options[0]);
    expect(options[0]).toHaveAttribute("aria-pressed", "true");
  });

  it("renders the duplicate-id notice when flagged (first block wins)", () => {
    render(
      <StructuredQuestionEditor
        {...baseProps({
          docPath: "specs/matrix-duplicate.md",
          duplicateId: true,
        })}
      />,
    );

    expect(
      screen.queryByTestId("structured-question-editor"),
    ).not.toBeInTheDocument();
    expect(screen.getByTestId("question-invalid-notice")).toHaveTextContent(
      /duplicate question id/,
    );
  });
});

describe("StructuredQuestionEditor — selection semantics", () => {
  it("replaces the selection in single-select mode", () => {
    render(
      <StructuredQuestionEditor
        {...baseProps({ docPath: "specs/select-single.md" })}
      />,
    );

    const options = screen.getAllByTestId("question-option-toggle");
    fireEvent.click(options[0]);
    expect(options[0]).toHaveAttribute("aria-pressed", "true");
    expect(options[1]).toHaveAttribute("aria-pressed", "false");

    fireEvent.click(options[1]);
    expect(options[0]).toHaveAttribute("aria-pressed", "false");
    expect(options[1]).toHaveAttribute("aria-pressed", "true");

    // Toggling the pressed option off clears the selection.
    fireEvent.click(options[1]);
    expect(options[1]).toHaveAttribute("aria-pressed", "false");
  });

  it("toggles selections independently in multi-select mode", () => {
    render(
      <StructuredQuestionEditor
        {...baseProps({
          block: questionBlock(MULTI_PAYLOAD),
          docPath: "specs/select-multi.md",
        })}
      />,
    );

    const options = screen.getAllByTestId("question-option-toggle");
    fireEvent.click(options[0]);
    fireEvent.click(options[1]);
    expect(options[0]).toHaveAttribute("aria-pressed", "true");
    expect(options[1]).toHaveAttribute("aria-pressed", "true");

    fireEvent.click(options[0]);
    expect(options[0]).toHaveAttribute("aria-pressed", "false");
    expect(options[1]).toHaveAttribute("aria-pressed", "true");
  });

  it("swaps between an offered option and the user's own answer, keeping typed words", () => {
    render(
      <StructuredQuestionEditor
        {...baseProps({ docPath: "specs/select-else.md" })}
      />,
    );

    expect(screen.queryByTestId("question-free-text")).not.toBeInTheDocument();
    const options = screen.getAllByTestId("question-option-toggle");
    fireEvent.click(options[0]);

    const pill = screen.getByTestId("question-something-else");
    fireEvent.click(pill);
    expect(pill).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByTestId("question-free-text")).toBeInTheDocument();
    // "Something else" REPLACES the offered options (D69): the selection the
    // user moved away from is dropped rather than recorded alongside.
    expect(options[0]).toHaveAttribute("aria-pressed", "false");

    fireEvent.change(screen.getByTestId("question-free-text"), {
      target: { value: "a darker shade than either" },
    });
    // Going back to an offered option closes the own-answer path...
    fireEvent.click(options[1]);
    expect(options[1]).toHaveAttribute("aria-pressed", "true");
    expect(screen.queryByTestId("question-free-text")).not.toBeInTheDocument();

    // ...without destroying what was typed: re-opening restores it, so a
    // mis-click never costs the user their words.
    fireEvent.click(screen.getByTestId("question-something-else"));
    expect(screen.getByTestId("question-free-text")).toHaveValue(
      "a darker shade than either",
    );
    expect(options[1]).toHaveAttribute("aria-pressed", "false");
  });
});

describe("StructuredQuestionEditor — Answer gating and apply", () => {
  it("keeps Answer disabled until a selection or non-empty free text exists", () => {
    render(
      <StructuredQuestionEditor
        {...baseProps({
          docPath: "specs/gating.md",
          onApplyAnswer: vi.fn(),
        })}
      />,
    );

    const answer = screen.getByTestId("question-answer-button");
    expect(answer).toBeDisabled();

    fireEvent.click(screen.getByTestId("question-something-else"));
    expect(answer).toBeDisabled();
    fireEvent.change(screen.getByTestId("question-free-text"), {
      target: { value: "   " },
    });
    expect(answer).toBeDisabled();
    fireEvent.change(screen.getByTestId("question-free-text"), {
      target: { value: "Neither — use green" },
    });
    expect(answer).toBeEnabled();

    // Selecting an option REPLACES the own-answer path (D69): the free text
    // closes, and Answer stays enabled on the selection alone.
    const options = screen.getAllByTestId("question-option-toggle");
    fireEvent.click(options[0]);
    expect(screen.queryByTestId("question-free-text")).not.toBeInTheDocument();
    expect(options[0]).toHaveAttribute("aria-pressed", "true");
    expect(answer).toBeEnabled();
  });

  it("passes the parsed question and answer input to onApplyAnswer and clears the draft on success", async () => {
    const onApplyAnswer = vi
      .fn<(q: ParsedQuestion, a: AnswerInput) => Promise<QuestionApplyResult>>()
      .mockResolvedValue({ status: "applied" });
    const { unmount } = render(
      <StructuredQuestionEditor
        {...baseProps({ docPath: "specs/apply-ok.md", onApplyAnswer })}
      />,
    );

    fireEvent.click(screen.getAllByTestId("question-option-toggle")[0]);
    fireEvent.click(screen.getByTestId("question-answer-button"));

    await waitFor(() => expect(onApplyAnswer).toHaveBeenCalledTimes(1));
    const [question, input] = onApplyAnswer.mock.calls[0];
    expect(question.id).toBe("pick-color");
    expect(input.selected).toEqual(["Red"]);
    expect(input.answeredBy).toEqual({ kind: "user", extra: {} });
    expect(input.answeredAt).toMatch(/^\d{4}-\d{2}-\d{2}T/);

    // Applied answers clear the module-scoped draft: a remount starts clean.
    unmount();
    render(
      <StructuredQuestionEditor
        {...baseProps({ docPath: "specs/apply-ok.md", onApplyAnswer })}
      />,
    );
    for (const option of screen.getAllByTestId("question-option-toggle")) {
      expect(option).toHaveAttribute("aria-pressed", "false");
    }
  });

  it("writes the answer to the DRAFT and offers no commit control at all", async () => {
    const onApplyAnswer = vi
      .fn<(q: ParsedQuestion, a: AnswerInput) => Promise<QuestionApplyResult>>()
      .mockResolvedValue({ status: "applied" });

    render(
      <StructuredQuestionEditor
        {...baseProps({ docPath: "specs/draft-only.md", onApplyAnswer })}
      />,
    );

    expect(screen.getByTestId("question-answer-button")).toHaveTextContent(
      "Save answer",
    );
    fireEvent.click(screen.getAllByTestId("question-option-toggle")[0]);
    fireEvent.click(screen.getByTestId("question-answer-button"));

    await waitFor(() => expect(onApplyAnswer).toHaveBeenCalledTimes(1));
    // Committing the document is the user's own decision through the ordinary
    // draft bar, never a side effect of answering and never a second button on
    // the card (D10).
    expect(
      screen.queryByTestId("question-save-answers"),
    ).not.toBeInTheDocument();
  });

  it("labels the action Save answer even with no commit context, because the draft write IS the save", async () => {
    const onApplyAnswer = vi
      .fn<(q: ParsedQuestion, a: AnswerInput) => Promise<QuestionApplyResult>>()
      .mockResolvedValue({ status: "applied" });

    render(
      <StructuredQuestionEditor
        {...baseProps({ docPath: "specs/no-commit-ctx.md", onApplyAnswer })}
      />,
    );

    expect(screen.getByTestId("question-answer-button")).toHaveTextContent(
      "Save answer",
    );
    fireEvent.click(screen.getAllByTestId("question-option-toggle")[0]);
    fireEvent.click(screen.getByTestId("question-answer-button"));
    await waitFor(() => expect(onApplyAnswer).toHaveBeenCalledTimes(1));
  });

  it("surfaces a conflict result inline and keeps the form editable", async () => {
    const onApplyAnswer = vi
      .fn<(q: ParsedQuestion, a: AnswerInput) => Promise<QuestionApplyResult>>()
      .mockResolvedValue({ status: "conflict", message: "document changed" });
    render(
      <StructuredQuestionEditor
        {...baseProps({ docPath: "specs/apply-conflict.md", onApplyAnswer })}
      />,
    );

    fireEvent.click(screen.getAllByTestId("question-option-toggle")[0]);
    fireEvent.click(screen.getByTestId("question-answer-button"));

    const conflict = await screen.findByTestId("question-apply-conflict");
    expect(conflict).toHaveTextContent("document changed");
    expect(screen.getByTestId("question-answer-button")).toBeEnabled();
  });
});

describe("StructuredQuestionEditor — draft LRU", () => {
  it("restores selection and free text after unmount/remount", () => {
    const props = baseProps({ docPath: "specs/draft-survives.md" });
    const { unmount } = render(<StructuredQuestionEditor {...props} />);

    fireEvent.click(screen.getAllByTestId("question-option-toggle")[1]);
    fireEvent.click(screen.getByTestId("question-something-else"));
    fireEvent.change(screen.getByTestId("question-free-text"), {
      target: { value: "half-typed answer" },
    });
    unmount();

    render(<StructuredQuestionEditor {...props} />);
    expect(screen.getByTestId("question-free-text")).toHaveValue(
      "half-typed answer",
    );
    expect(screen.getByTestId("question-something-else")).toHaveAttribute(
      "aria-pressed",
      "true",
    );
  });

  it("keeps drafts per docPath#id key", () => {
    const first = baseProps({ docPath: "specs/draft-a.md" });
    const { unmount } = render(<StructuredQuestionEditor {...first} />);
    fireEvent.click(screen.getAllByTestId("question-option-toggle")[0]);
    unmount();

    render(
      <StructuredQuestionEditor
        {...baseProps({ docPath: "specs/draft-b.md" })}
      />,
    );
    for (const option of screen.getAllByTestId("question-option-toggle")) {
      expect(option).toHaveAttribute("aria-pressed", "false");
    }
  });
});

describe("StructuredQuestionEditor — Discuss", () => {
  it("fires onDiscuss with the parsed question, independent of canSave", () => {
    const onDiscuss = vi.fn<(question: ParsedQuestion) => void>();
    render(
      <StructuredQuestionEditor
        {...baseProps({
          docPath: "specs/discuss.md",
          canSave: false,
          onDiscuss,
        })}
      />,
    );

    fireEvent.click(screen.getByTestId("question-discuss-button"));
    expect(onDiscuss).toHaveBeenCalledTimes(1);
    const question = onDiscuss.mock.calls[0][0];
    expect(question.id).toBe("pick-color");
    expect(question.options.map((option) => option.label)).toEqual([
      "Red",
      "Blue",
    ]);
  });
});

describe("StructuredQuestionEditor — visible free text only", () => {
  it("submits only visible free text; hidden drafted text stays in the LRU and submits as empty", async () => {
    const onApplyAnswer = vi
      .fn<(q: ParsedQuestion, a: AnswerInput) => Promise<QuestionApplyResult>>()
      .mockResolvedValue({ status: "applied" });
    render(
      <StructuredQuestionEditor
        {...baseProps({ docPath: "specs/hidden-text.md", onApplyAnswer })}
      />,
    );

    const pill = screen.getByTestId("question-something-else");
    fireEvent.click(pill);
    fireEvent.change(screen.getByTestId("question-free-text"), {
      target: { value: "drafted but hidden" },
    });
    // Close the pill — the text is hidden but retained in the draft LRU.
    fireEvent.click(pill);
    expect(screen.queryByTestId("question-free-text")).not.toBeInTheDocument();
    fireEvent.click(pill);
    expect(screen.getByTestId("question-free-text")).toHaveValue(
      "drafted but hidden",
    );
    fireEvent.click(pill);

    fireEvent.click(screen.getAllByTestId("question-option-toggle")[0]);
    fireEvent.click(screen.getByTestId("question-answer-button"));
    await waitFor(() => expect(onApplyAnswer).toHaveBeenCalledTimes(1));
    const [, input] = onApplyAnswer.mock.calls[0];
    expect(input.selected).toEqual(["Red"]);
    expect(input.text).toBe("");
  });
});

describe("StructuredQuestionEditor — post-apply conflict watch", () => {
  function appliedWithWatch() {
    let conflictCallback: (() => void) | null = null;
    const cancel = vi.fn();
    const watch = vi.fn((onConflict: () => void) => {
      conflictCallback = onConflict;
      return cancel;
    });
    const onApplyAnswer = vi
      .fn<(q: ParsedQuestion, a: AnswerInput) => Promise<QuestionApplyResult>>()
      .mockResolvedValue({ status: "applied", watch });
    return {
      onApplyAnswer,
      watch,
      cancel,
      fireConflict: () => {
        expect(conflictCallback).not.toBeNull();
        act(() => conflictCallback!());
      },
    };
  }

  it("registers the watch on apply and flips to the late-conflict banner with Reload", async () => {
    const { onApplyAnswer, watch, fireConflict } = appliedWithWatch();
    render(
      <StructuredQuestionEditor
        {...baseProps({ docPath: "specs/watch-conflict.md", onApplyAnswer })}
      />,
    );

    fireEvent.click(screen.getAllByTestId("question-option-toggle")[0]);
    fireEvent.click(screen.getByTestId("question-answer-button"));
    await waitFor(() => expect(watch).toHaveBeenCalledTimes(1));
    expect(
      screen.queryByTestId("question-late-conflict"),
    ).not.toBeInTheDocument();

    fireConflict();
    expect(screen.getByTestId("question-late-conflict")).toBeInTheDocument();

    // Reload acknowledges: the banner clears and the card shows the live
    // parse it already renders.
    fireEvent.click(screen.getByTestId("question-reload"));
    expect(
      screen.queryByTestId("question-late-conflict"),
    ).not.toBeInTheDocument();
  });

  it("cancels the watch on unmount and before a newer apply", async () => {
    const { onApplyAnswer, watch, cancel } = appliedWithWatch();
    const props = baseProps({
      docPath: "specs/watch-cancel.md",
      onApplyAnswer,
    });
    const { unmount } = render(<StructuredQuestionEditor {...props} />);

    fireEvent.click(screen.getAllByTestId("question-option-toggle")[0]);
    fireEvent.click(screen.getByTestId("question-answer-button"));
    await waitFor(() => expect(watch).toHaveBeenCalledTimes(1));
    expect(cancel).not.toHaveBeenCalled();

    // A newer apply supersedes the previous watch…
    fireEvent.click(screen.getAllByTestId("question-option-toggle")[1]);
    fireEvent.click(screen.getByTestId("question-answer-button"));
    await waitFor(() => expect(watch).toHaveBeenCalledTimes(2));
    expect(cancel).toHaveBeenCalledTimes(1);

    // …and unmount cancels the surviving one.
    unmount();
    expect(cancel).toHaveBeenCalledTimes(2);
  });
});

describe("StructuredQuestionEditor — answered option removed by authoring", () => {
  /** Answered on "Blue", but authoring has since removed that option. */
  const ORPHANED = [
    "schema: 1",
    "id: pick-color",
    "status: answered",
    "prompt: Which color should the button use?",
    "options:",
    "  - label: Red",
    "    description: Use the red token",
    "answer:",
    "  selected:",
    "    - Blue",
    "  text: ''",
    "  rationale: ''",
    "  answered_by: user",
    "  answered_at: 2026-08-01T12:00:00Z",
    "",
  ].join("\n");

  it("does not let a dead label satisfy Save, and never resubmits it", async () => {
    // Authoring can rename or remove an option while an answer is settled, so
    // `selected` can name a label that no longer renders. Treating it as a
    // valid answer would record a choice no option offers, with nothing shown
    // as selected.
    const onApplyAnswer = vi
      .fn<(q: ParsedQuestion, a: AnswerInput) => Promise<QuestionApplyResult>>()
      .mockResolvedValue({ status: "applied" });
    render(
      <StructuredQuestionEditor
        {...baseProps({
          block: questionBlock(ORPHANED),
          docPath: "specs/orphaned.md",
          onApplyAnswer,
        })}
      />,
    );

    fireEvent.click(screen.getByTestId("question-change-answer"));
    // Nothing renders selected, and Save is not offered as sufficient.
    for (const toggle of await screen.findAllByTestId(
      "question-option-toggle",
    )) {
      expect(toggle).toHaveAttribute("aria-pressed", "false");
    }
    expect(screen.getByTestId("question-answer-button")).toBeDisabled();

    // Picking a live option makes it answerable, and only that label submits.
    fireEvent.click(screen.getAllByTestId("question-option-toggle")[0]);
    fireEvent.click(screen.getByTestId("question-answer-button"));
    await waitFor(() => expect(onApplyAnswer).toHaveBeenCalledTimes(1));
    expect(onApplyAnswer.mock.calls[0][1].selected).toEqual(["Red"]);
  });
});

describe("StructuredQuestionEditor — settled answers freeze their options (D10)", () => {
  it("refuses to save authoring that removes the selected option", () => {
    // Removing the option a settled user answer names would leave the
    // decision pointing at a choice that no longer exists — a silent rewrite
    // of what the person decided. The path to reshaping is Clear answer first.
    const onApplyEdit = vi
      .fn<(q: ParsedQuestion) => Promise<QuestionApplyResult>>()
      .mockResolvedValue({ status: "applied" });
    render(
      <StructuredQuestionEditor
        {...baseProps({
          block: questionBlock(ANSWERED_PAYLOAD), // answered on "Blue"
          docPath: "specs/settled-freeze.md",
          onApplyEdit,
        })}
      />,
    );

    fireEvent.click(screen.getByTestId("question-edit-button"));
    const removes = screen.getAllByTestId("question-edit-option-remove");
    // Options are [Red, Blue]; the answer selects Blue.
    fireEvent.click(removes[1]);

    expect(screen.getByTestId("question-edit-save")).toBeDisabled();
    expect(
      screen.getByTestId("question-edit-selected-removed"),
    ).toHaveTextContent("Clear the answer first");
    expect(onApplyEdit).not.toHaveBeenCalled();
  });

  it("still allows removing an option the answer does not name", async () => {
    const onApplyEdit = vi
      .fn<(q: ParsedQuestion) => Promise<QuestionApplyResult>>()
      .mockResolvedValue({ status: "applied" });
    render(
      <StructuredQuestionEditor
        {...baseProps({
          block: questionBlock(ANSWERED_PAYLOAD),
          docPath: "specs/settled-free-edit.md",
          onApplyEdit,
        })}
      />,
    );

    fireEvent.click(screen.getByTestId("question-edit-button"));
    // Remove Red — the answer selects Blue, so Red is freely editable.
    fireEvent.click(screen.getAllByTestId("question-edit-option-remove")[0]);
    expect(screen.getByTestId("question-edit-save")).toBeEnabled();
    fireEvent.click(screen.getByTestId("question-edit-save"));
    await waitFor(() => expect(onApplyEdit).toHaveBeenCalledTimes(1));
    expect(
      onApplyEdit.mock.calls[0][0].options.map((option) => option.label),
    ).toEqual(["Blue"]);
  });
});

describe("StructuredQuestionEditor — Clear answer (D10)", () => {
  const answeredBlock = () => questionBlock(ANSWERED_PAYLOAD);

  it("returns the question to unanswered through the apply path", async () => {
    const onClearAnswer = vi
      .fn<(q: ParsedQuestion) => Promise<QuestionApplyResult>>()
      .mockResolvedValue({ status: "applied" });
    render(
      <StructuredQuestionEditor
        {...baseProps({
          block: answeredBlock(),
          docPath: "specs/clear.md",
          onClearAnswer,
        })}
      />,
    );

    fireEvent.click(screen.getByTestId("question-clear-answer"));
    await waitFor(() => expect(onClearAnswer).toHaveBeenCalledTimes(1));
    // It clears the question it is showing — the host owns the rewrite.
    expect(onClearAnswer.mock.calls[0][0].id).toBe("pick-color");
  });

  it("is withheld on a read-only card even if the callback is supplied", () => {
    // Clear answer mutates the document, so it obeys the same capability gate
    // as Answer. Keying only on the callback would let a read-only tab write.
    render(
      <StructuredQuestionEditor
        {...baseProps({
          block: answeredBlock(),
          docPath: "specs/clear-readonly.md",
          canSave: false,
          onClearAnswer: vi.fn(),
        })}
      />,
    );
    expect(
      screen.queryByTestId("question-clear-answer"),
    ).not.toBeInTheDocument();
  });

  it("is absent without a live draft, like every other write action", () => {
    render(
      <StructuredQuestionEditor
        {...baseProps({ block: answeredBlock(), docPath: "specs/ro.md" })}
      />,
    );
    expect(
      screen.queryByTestId("question-clear-answer"),
    ).not.toBeInTheDocument();
  });

  it("surfaces a conflict inline instead of pretending the clear landed", async () => {
    const onClearAnswer = vi
      .fn<(q: ParsedQuestion) => Promise<QuestionApplyResult>>()
      .mockResolvedValue({ status: "conflict", message: "document changed" });
    render(
      <StructuredQuestionEditor
        {...baseProps({
          block: answeredBlock(),
          docPath: "specs/clear-conflict.md",
          onClearAnswer,
        })}
      />,
    );

    fireEvent.click(screen.getByTestId("question-clear-answer"));
    expect(
      await screen.findByTestId("question-apply-conflict"),
    ).toHaveTextContent("document changed");
  });
});

describe("StructuredQuestionEditor — question authoring (D69)", () => {
  function editableProps(overrides: Record<string, unknown> = {}) {
    return baseProps({
      onApplyEdit: vi
        .fn<(edited: ParsedQuestion) => Promise<QuestionApplyResult>>()
        .mockResolvedValue({ status: "applied" }),
      ...overrides,
    });
  }

  it("offers Edit question only with canSave and a write context, in open and answered views", () => {
    const open = render(
      <StructuredQuestionEditor
        {...editableProps({ docPath: "specs/edit-open.md" })}
      />,
    );
    expect(screen.getByTestId("question-edit-button")).toBeInTheDocument();
    open.unmount();

    const answered = render(
      <StructuredQuestionEditor
        {...editableProps({
          block: questionBlock(ANSWERED_PAYLOAD),
          docPath: "specs/edit-answered.md",
        })}
      />,
    );
    expect(screen.getByTestId("question-edit-button")).toBeInTheDocument();
    answered.unmount();

    const readOnly = render(
      <StructuredQuestionEditor
        {...editableProps({ canSave: false, docPath: "specs/edit-ro.md" })}
      />,
    );
    expect(
      screen.queryByTestId("question-edit-button"),
    ).not.toBeInTheDocument();
    readOnly.unmount();

    render(
      <StructuredQuestionEditor
        {...editableProps({
          onApplyEdit: undefined,
          docPath: "specs/edit-nocb.md",
        })}
      />,
    );
    expect(
      screen.queryByTestId("question-edit-button"),
    ).not.toBeInTheDocument();
  });

  it("opens a prompt+options form that never renders answer fields", () => {
    render(
      <StructuredQuestionEditor
        {...editableProps({
          block: questionBlock(ANSWERED_PAYLOAD),
          docPath: "specs/edit-guard.md",
        })}
      />,
    );

    fireEvent.click(screen.getByTestId("question-edit-button"));
    expect(screen.getByTestId("question-edit-form")).toBeInTheDocument();
    expect(screen.getByTestId("question-edit-prompt")).toHaveValue(
      "Which color should the button use?",
    );
    expect(screen.getAllByTestId("question-edit-option-label")).toHaveLength(2);

    // The authoring surface exposes no answer inputs or answer summary and
    // never shows the recorded answer values as editable fields.
    expect(screen.queryByTestId("question-free-text")).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("question-answered-summary"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("question-option-toggle"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByDisplayValue("Matches the info palette"),
    ).not.toBeInTheDocument();
  });

  it("saves field edits with the answer mapping and status untouched", async () => {
    const onApplyEdit = vi
      .fn<(edited: ParsedQuestion) => Promise<QuestionApplyResult>>()
      .mockResolvedValue({ status: "applied" });
    render(
      <StructuredQuestionEditor
        {...editableProps({
          block: questionBlock(ANSWERED_PAYLOAD),
          docPath: "specs/edit-save.md",
          onApplyEdit,
        })}
      />,
    );

    fireEvent.click(screen.getByTestId("question-edit-button"));
    fireEvent.change(screen.getByTestId("question-edit-prompt"), {
      target: { value: "Which color should the primary button use?" },
    });
    const labels = screen.getAllByTestId("question-edit-option-label");
    fireEvent.change(labels[0], { target: { value: "Crimson" } });
    fireEvent.click(screen.getByTestId("question-edit-save"));

    await waitFor(() => expect(onApplyEdit).toHaveBeenCalledTimes(1));
    const edited = onApplyEdit.mock.calls[0][0];
    expect(edited.prompt).toBe("Which color should the primary button use?");
    expect(edited.options.map((option) => option.label)).toEqual([
      "Crimson",
      "Blue",
    ]);
    // Answer mapping and status byte-for-byte untouched by authoring.
    const parsedOriginal = parseQuestionPayload(ANSWERED_PAYLOAD, {
      fallbackId: "q1",
    });
    if (!parsedOriginal.ok) throw new Error("fixture must parse");
    expect(edited.status).toBe("answered");
    expect(edited.answer).toEqual(parsedOriginal.question.answer);
    // Form closes on success.
    await waitFor(() =>
      expect(
        screen.queryByTestId("question-edit-form"),
      ).not.toBeInTheDocument(),
    );
  });

  it("adds and removes options and moves the single recommended flag", async () => {
    const onApplyEdit = vi
      .fn<(edited: ParsedQuestion) => Promise<QuestionApplyResult>>()
      .mockResolvedValue({ status: "applied" });
    render(
      <StructuredQuestionEditor
        {...editableProps({ docPath: "specs/edit-options.md", onApplyEdit })}
      />,
    );

    fireEvent.click(screen.getByTestId("question-edit-button"));
    // OPEN_PAYLOAD: Red (recommended), Blue.
    let radios = screen.getAllByTestId("question-edit-option-recommended");
    expect(radios[0]).toBeChecked();
    expect(radios[1]).not.toBeChecked();

    // Move the flag to Blue — at most one stays checked.
    fireEvent.click(radios[1]);
    radios = screen.getAllByTestId("question-edit-option-recommended");
    expect(radios[0]).not.toBeChecked();
    expect(radios[1]).toBeChecked();
    // Toggle it off entirely.
    fireEvent.click(radios[1]);
    expect(
      screen
        .getAllByTestId("question-edit-option-recommended")
        .every((radio) => !(radio as HTMLInputElement).checked),
    ).toBe(true);

    // Remove the first option, add a new one.
    fireEvent.click(screen.getAllByTestId("question-edit-option-remove")[0]);
    expect(screen.getAllByTestId("question-edit-option-label")).toHaveLength(1);
    fireEvent.click(screen.getByTestId("question-edit-add-option"));
    const labels = screen.getAllByTestId("question-edit-option-label");
    expect(labels).toHaveLength(2);
    // A label-less new option blocks Save until filled.
    expect(screen.getByTestId("question-edit-save")).toBeDisabled();
    fireEvent.change(labels[1], { target: { value: "Green" } });
    fireEvent.change(
      screen.getAllByTestId("question-edit-option-description")[1],
      { target: { value: "the fresh option" } },
    );
    expect(screen.getByTestId("question-edit-save")).toBeEnabled();

    fireEvent.click(screen.getByTestId("question-edit-save"));
    await waitFor(() => expect(onApplyEdit).toHaveBeenCalledTimes(1));
    const edited = onApplyEdit.mock.calls[0][0];
    expect(edited.options.map((option) => option.label)).toEqual([
      "Blue",
      "Green",
    ]);
    expect(edited.options.some((option) => option.recommended === true)).toBe(
      false,
    );
    expect(edited.options[1].description).toBe("the fresh option");
  });

  it("cancel discards edits and restores the parsed state", () => {
    render(
      <StructuredQuestionEditor
        {...editableProps({ docPath: "specs/edit-cancel.md" })}
      />,
    );

    fireEvent.click(screen.getByTestId("question-edit-button"));
    fireEvent.change(screen.getByTestId("question-edit-prompt"), {
      target: { value: "scrapped edit" },
    });
    fireEvent.click(screen.getAllByTestId("question-edit-option-remove")[0]);
    fireEvent.click(screen.getByTestId("question-edit-cancel"));

    // Card renders the parsed state again.
    expect(screen.queryByTestId("question-edit-form")).not.toBeInTheDocument();
    expect(
      screen.getByText("Which color should the button use?"),
    ).toBeInTheDocument();
    expect(screen.getAllByTestId("question-option-toggle")).toHaveLength(2);

    // Reopening starts from the parsed state, not the scrapped edits.
    fireEvent.click(screen.getByTestId("question-edit-button"));
    expect(screen.getByTestId("question-edit-prompt")).toHaveValue(
      "Which color should the button use?",
    );
    expect(screen.getAllByTestId("question-edit-option-label")).toHaveLength(2);
  });

  it("surfaces a conflict inline and keeps the form open for retry", async () => {
    const onApplyEdit = vi
      .fn<(edited: ParsedQuestion) => Promise<QuestionApplyResult>>()
      .mockResolvedValue({ status: "conflict", message: "question moved" });
    render(
      <StructuredQuestionEditor
        {...editableProps({ docPath: "specs/edit-conflict.md", onApplyEdit })}
      />,
    );

    fireEvent.click(screen.getByTestId("question-edit-button"));
    fireEvent.change(screen.getByTestId("question-edit-prompt"), {
      target: { value: "new prompt" },
    });
    fireEvent.click(screen.getByTestId("question-edit-save"));

    const error = await screen.findByTestId("question-edit-error");
    expect(error).toHaveTextContent("question moved");
    expect(screen.getByTestId("question-edit-form")).toBeInTheDocument();
    expect(screen.getByTestId("question-edit-save")).toBeEnabled();
    fireEvent.click(screen.getByTestId("question-edit-save"));
    await waitFor(() => expect(onApplyEdit).toHaveBeenCalledTimes(2));
  });
});

describe("StructuredQuestionEditor — review hardening", () => {
  const NO_ANSWER_PATH_PAYLOAD = [
    "schema: 1",
    "id: unanswerable",
    "prompt: Anything?",
    "options: []",
    "allow_free_text: false",
    "",
  ].join("\n");

  it("flags a question with no options and no free text as unanswerable", () => {
    // `allow_free_text` defaults true only when omitted, so this shape parses
    // yet offers no way to answer: say so instead of a silently dead button.
    render(
      <StructuredQuestionEditor
        {...baseProps({
          block: questionBlock(NO_ANSWER_PATH_PAYLOAD),
          docPath: "specs/unanswerable.md",
          onApplyAnswer: vi.fn(),
        })}
      />,
    );

    expect(
      screen.getByTestId("question-unanswerable-notice"),
    ).toBeInTheDocument();
    expect(screen.getByTestId("question-answer-button")).toBeDisabled();
  });

  it("gives the answer textarea an accessible name", () => {
    render(
      <StructuredQuestionEditor
        {...baseProps({
          block: questionBlock(FREE_TEXT_ONLY_PAYLOAD),
          docPath: "specs/a11y.md",
        })}
      />,
    );

    // Reachable by accessible name, not just by test id.
    expect(screen.getByRole("textbox", { name: /your answer/i })).toBe(
      screen.getByTestId("question-free-text"),
    );
  });

  it("blocks an authoring save that would give two options the same label", () => {
    const onApplyEdit = vi
      .fn<(q: ParsedQuestion) => Promise<QuestionApplyResult>>()
      .mockResolvedValue({ status: "applied" });
    render(
      <StructuredQuestionEditor
        {...baseProps({
          docPath: "specs/dup-label.md",
          onApplyEdit,
        })}
      />,
    );

    fireEvent.click(screen.getByTestId("question-edit-button"));
    const labels = screen.getAllByTestId("question-edit-option-label");
    // Rename the second option onto the first one's label.
    fireEvent.change(labels[1], { target: { value: "Red" } });

    expect(
      screen.getByTestId("question-edit-duplicate-label"),
    ).toBeInTheDocument();
    expect(screen.getByTestId("question-edit-save")).toBeDisabled();
    expect(onApplyEdit).not.toHaveBeenCalled();
  });

  it("drops a cached draft once a collaborator answers the same question", () => {
    // Draft cached against the OPEN payload under key `docPath#id`.
    const { unmount } = render(
      <StructuredQuestionEditor
        {...baseProps({
          docPath: "specs/reconcile.md",
          onApplyAnswer: vi.fn(),
        })}
      />,
    );
    fireEvent.click(screen.getAllByTestId("question-option-toggle")[0]);
    expect(screen.getByTestId("question-answer-button")).toBeEnabled();
    unmount();

    // Same doc + id, but a collaborator's answer has landed. The stale draft
    // must NOT reopen an edit session that hides their answer.
    render(
      <StructuredQuestionEditor
        {...baseProps({
          block: questionBlock(ANSWERED_PAYLOAD),
          docPath: "specs/reconcile.md",
          onApplyAnswer: vi.fn(),
        })}
      />,
    );

    expect(screen.getByTestId("question-answered-summary")).toHaveTextContent(
      "Blue",
    );
    expect(
      screen.queryByTestId("question-option-toggle"),
    ).not.toBeInTheDocument();
  });

  it("drops a selected label the live options no longer offer", () => {
    const { unmount } = render(
      <StructuredQuestionEditor
        {...baseProps({
          docPath: "specs/removed-option.md",
          onApplyAnswer: vi.fn(),
        })}
      />,
    );
    // Select "Blue", then the payload loses that option.
    fireEvent.click(screen.getAllByTestId("question-option-toggle")[1]);
    expect(screen.getByTestId("question-answer-button")).toBeEnabled();
    unmount();

    const withoutBlue = OPEN_PAYLOAD.replace(
      "  - label: Blue\n    description: Use the info token\n",
      "",
    );
    render(
      <StructuredQuestionEditor
        {...baseProps({
          block: questionBlock(withoutBlue),
          docPath: "specs/removed-option.md",
          onApplyAnswer: vi.fn(),
        })}
      />,
    );

    // The stale selection must not keep Answer enabled or persist a label
    // that is absent from the live options.
    expect(screen.getAllByTestId("question-option-toggle")).toHaveLength(1);
    expect(screen.getByTestId("question-answer-button")).toBeDisabled();
  });
});

describe("StructuredQuestionEditor — duplicate option labels", () => {
  const DUP_LABEL_PAYLOAD = [
    "schema: 1",
    "id: dup",
    "status: open",
    "prompt: Which color?",
    "options:",
    "  - label: Red",
    "    description: crimson",
    "  - label: Red",
    "    description: scarlet",
    "",
  ].join("\n");

  it("withholds Answer and explains why when two options share a label", async () => {
    render(
      <StructuredQuestionEditor
        {...baseProps({
          block: questionBlock(DUP_LABEL_PAYLOAD),
          docPath: "specs/dup-answer.md",
          onApplyAnswer: vi.fn(),
        })}
      />,
    );

    expect(
      screen.getByTestId("question-duplicate-label-notice"),
    ).toBeInTheDocument();
    // Selecting an option must not make the answer reachable: `answer.selected`
    // stores labels, so the persisted answer could not identify one option.
    fireEvent.click(screen.getAllByTestId("question-option-toggle")[0]);
    await waitFor(() =>
      expect(screen.getByTestId("question-answer-button")).toBeDisabled(),
    );
  });

  it("keeps the authoring form available so the labels can be fixed", () => {
    render(
      <StructuredQuestionEditor
        {...baseProps({
          block: questionBlock(DUP_LABEL_PAYLOAD),
          docPath: "specs/dup-fix.md",
          onApplyEdit: vi.fn(),
        })}
      />,
    );

    // Read-only for answering, but editing is how the payload gets repaired.
    expect(screen.getByTestId("question-edit-button")).toBeEnabled();
  });
});

describe("StructuredQuestionEditor — cached draft vs answer-shape change", () => {
  const MULTI_OPEN = [
    "schema: 1",
    "id: pick-many",
    "status: open",
    "prompt: Which platforms?",
    "multi_select: true",
    "options:",
    "  - label: Web",
    "    description: Browser client",
    "  - label: Mobile",
    "    description: Mobile app",
    "",
  ].join("\n");

  it("clamps a cached multi-select draft when the same question becomes single-select", () => {
    const { unmount } = render(
      <StructuredQuestionEditor
        {...baseProps({
          block: questionBlock(MULTI_OPEN),
          docPath: "specs/mode-flip.md",
          onApplyAnswer: vi.fn(),
        })}
      />,
    );
    // Cache A+B while the question is multi-select.
    const toggles = screen.getAllByTestId("question-option-toggle");
    fireEvent.click(toggles[0]);
    fireEvent.click(toggles[1]);
    expect(toggles[0]).toHaveAttribute("aria-pressed", "true");
    expect(toggles[1]).toHaveAttribute("aria-pressed", "true");
    unmount();

    // A collaborator flips the SAME id to single-select without touching the
    // labels — so a live-label filter alone would keep both selections and
    // submit two values into a single-select question.
    render(
      <StructuredQuestionEditor
        {...baseProps({
          block: questionBlock(
            MULTI_OPEN.replace("multi_select: true", "multi_select: false"),
          ),
          docPath: "specs/mode-flip.md",
          onApplyAnswer: vi.fn(),
        })}
      />,
    );

    const after = screen.getAllByTestId("question-option-toggle");
    const pressed = after.filter(
      (el) => el.getAttribute("aria-pressed") === "true",
    );
    expect(pressed).toHaveLength(1);
  });

  it("clamps a LIVE multi→single change on the mounted card, without a remount", async () => {
    // The unmount/remount test above always re-runs loadAnswerDraft. The
    // production path is a live workspace file rerender of the SAME instance, where the
    // resync effect returns early unless the token notices the shape change.
    const onApplyAnswer = vi
      .fn<(q: ParsedQuestion, a: AnswerInput) => Promise<QuestionApplyResult>>()
      .mockResolvedValue({ status: "applied" });
    const props = {
      docPath: "specs/live-mode-flip.md",
      onApplyAnswer,
    };
    const { rerender } = render(
      <StructuredQuestionEditor
        {...baseProps({ ...props, block: questionBlock(MULTI_OPEN) })}
      />,
    );
    const toggles = screen.getAllByTestId("question-option-toggle");
    fireEvent.click(toggles[0]);
    fireEvent.click(toggles[1]);

    rerender(
      <StructuredQuestionEditor
        {...baseProps({
          ...props,
          block: questionBlock(
            MULTI_OPEN.replace("multi_select: true", "multi_select: false"),
          ),
        })}
      />,
    );

    expect(
      screen
        .getAllByTestId("question-option-toggle")
        .filter((el) => el.getAttribute("aria-pressed") === "true"),
    ).toHaveLength(1);

    // And the clamp reaches the WRITE, not just the pressed state.
    fireEvent.click(screen.getByTestId("question-answer-button"));
    await waitFor(() => expect(onApplyAnswer).toHaveBeenCalledTimes(1));
    expect(onApplyAnswer.mock.calls[0][1].selected).toHaveLength(1);
  });

  it("drops a removed option from a LIVE mounted card instead of submitting it", async () => {
    const onApplyAnswer = vi
      .fn<(q: ParsedQuestion, a: AnswerInput) => Promise<QuestionApplyResult>>()
      .mockResolvedValue({ status: "applied" });
    const props = {
      docPath: "specs/live-option-removal.md",
      onApplyAnswer,
    };
    const { rerender } = render(
      <StructuredQuestionEditor
        {...baseProps({ ...props, block: questionBlock(MULTI_OPEN) })}
      />,
    );
    // Select ONLY the option a collaborator is about to delete.
    fireEvent.click(screen.getAllByTestId("question-option-toggle")[1]);

    rerender(
      <StructuredQuestionEditor
        {...baseProps({
          ...props,
          block: questionBlock(
            MULTI_OPEN.replace(
              "  - label: Mobile\n    description: Mobile app\n",
              "",
            ),
          ),
        })}
      />,
    );

    // The dead label must not survive as a hidden selection: with nothing
    // selected the action is disabled rather than silently writing "Mobile".
    expect(screen.getAllByTestId("question-option-toggle")).toHaveLength(1);
    expect(screen.getByTestId("question-answer-button")).toBeDisabled();
    expect(onApplyAnswer).not.toHaveBeenCalled();
  });
});
