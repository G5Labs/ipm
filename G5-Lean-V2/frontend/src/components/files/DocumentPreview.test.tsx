import React from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { renderApp } from "../../test/render";
import { DocumentPreview } from "./DocumentPreview";
import { collectQuestionFences } from "../editor/questionAnswerWrite";
import { http, HttpResponse } from "msw";
import { server } from "../../test/msw/server";

const prefix =
  "# Decisions café 中文 😀\r\n\r\nKeep  these **bytes**.\r\n\r\n[Q1. Color]\r\n\r\n```g5-question\r\n";
const suffix =
  "\r\n```\r\n\r\nAfter  [reference][id].\r\n\r\n[id]: https://example.com\r\n";
const question =
  prefix +
  [
    "schema: 1",
    "id: color",
    "status: open",
    "prompt: Pick a color",
    "options:",
    "  - label: Blue",
    "  - label: Red",
  ].join("\r\n") +
  suffix;
const base = { filePath: "README.md", workspaceId: "ws", repoId: "repo" };

describe("read-only document projection", () => {
  it.each([
    ["docs/ws/empty.md", "", false],
    ["docs/ws/blank.md", " \r\n\t", false],
    ["docs/ws/notes.md", "# Notes\nOrdinary Markdown.\n", false],
    ["docs/ws/request.md", "[I1. Uploaded intent]\nKeep this content.\n", true],
    [
      ".semlift/specs/legacy.semlift.md",
      "[D1. Legacy document]\nKeep this content.\n",
      true,
    ],
  ])(
    "renders %s with applicable filters and no retired assessments",
    async (filePath, content, hasRecords) => {
      const requested = vi.fn(() =>
        HttpResponse.json({ error: "Resource not found" }, { status: 404 }),
      );
      server.use(
        http.all("*/api/v1/grounding/review-metadata", requested),
        http.all("*/api/v1/grounding/review-actions", requested),
      );
      renderApp(
        <DocumentPreview {...base} filePath={filePath} content={content} />,
      );
      await screen.findByTestId("document-preview");
      for (const id of ["toolbar-tag-human", "toolbar-tag-uncertainty"]) {
        if (hasRecords) await userEvent.click(screen.getByTestId(id));
        else expect(screen.queryByTestId(id)).toBeNull();
      }
      expect(requested).not.toHaveBeenCalled();
      expect(screen.queryByText(/Review metadata unavailable/)).toBeNull();
      expect(screen.queryByTestId("toolbar-tag-unimplemented")).toBeNull();
      expect(
        screen.queryByTestId("semantic-record-implementation-control"),
      ).toBeNull();
    },
  );

  it("shows ordinary prose after the last filtered record is removed", async () => {
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    const { rerender } = render(
      <DocumentPreview
        {...base}
        content={
          "[I1. Human record] (Tags: human)\nKeep me.\n\n[D1. Other record]\nOther body.\n"
        }
      />,
      {
        wrapper: ({ children }) => (
          <QueryClientProvider client={client}>{children}</QueryClientProvider>
        ),
      },
    );
    await userEvent.click(screen.getByTestId("toolbar-tag-human"));
    expect(screen.getByText("Keep me.")).toBeVisible();
    expect(screen.queryByText("Other body.")).toBeNull();

    for (const content of ["", "Ordinary prose after editing."]) {
      rerender(<DocumentPreview {...base} content={content} />);
      expect(screen.queryByTestId("toolbar-tag-human")).toBeNull();
      expect(screen.queryByTestId("toolbar-tag-uncertainty")).toBeNull();
      if (content) expect(screen.getByText(content)).toBeVisible();
    }
  });

  it.each([true, false])(
    "repairs only the displayed payload through the real rewrite gate (accepted=%s)",
    (accepted) => {
      const content = prefix + "schema: [broken" + suffix;
      const rewrite = vi.fn(() => accepted);
      renderApp(
        <DocumentPreview
          {...base}
          content={content}
          canAnswer
          onRewrite={rewrite}
        />,
      );
      expect(rewrite).not.toHaveBeenCalled();
      fireEvent.click(screen.getByTestId("question-repair-open"));
      const body =
        "schema: 1\nid: color\nstatus: open\nprompt: Repaired\noptions: []";
      fireEvent.change(screen.getByTestId("question-repair-editor"), {
        target: { value: body },
      });
      fireEvent.click(screen.getByTestId("question-repair-save"));
      expect(rewrite).toHaveBeenCalledWith(
        content,
        prefix + body.replaceAll("\n", "\r\n") + suffix,
      );
      if (!accepted) {
        expect(screen.getByTestId("question-repair-error")).toHaveTextContent(
          "The file changed",
        );
        expect(screen.getByTestId("question-repair-editor")).toHaveValue(body);
      }
    },
  );

  it("renders one record label when its title contains a nested reference", () => {
    renderApp(
      <DocumentPreview
        {...base}
        content={"[D1. Preserve questionnaire ([D59])]\nBody stays visible.\n"}
      />,
    );
    expect(screen.getByTestId("semantic-record")).toHaveTextContent(
      "Body stays visible.",
    );
    expect(screen.getAllByText("Preserve questionnaire ([D59])")).toHaveLength(
      1,
    );
    expect(
      screen.queryByText("[D1. Preserve questionnaire ([D59])]"),
    ).toBeNull();
  });

  it("answers a question and synchronizes its label, retaining all other source bytes", async () => {
    const rewrite = vi.fn(() => true);
    const { container } = renderApp(
      <DocumentPreview
        {...base}
        content={question}
        canAnswer
        onRewrite={rewrite}
        currentUserId="human"
        currentUserEmail="human@example.com"
      />,
    );
    expect(rewrite).not.toHaveBeenCalled();
    expect(container.querySelector('[contenteditable="true"]')).toBeNull();
    expect(screen.getByRole("link", { name: "reference" })).toHaveAttribute(
      "href",
      "https://example.com",
    );
    fireEvent.click(screen.getAllByTestId("question-option-toggle")[0]);
    fireEvent.click(screen.getByTestId("question-answer-button"));
    await waitFor(() => expect(rewrite).toHaveBeenCalledOnce());
    const [expected, next] = rewrite.mock.calls[0] as unknown as [
      string,
      string,
    ];
    expect(expected).toBe(question);
    expect(
      next.startsWith(
        prefix.replace("[Q1. Color]", "[Q1. Color] (Tags: answered)"),
      ),
    ).toBe(true);
    expect(next.endsWith(suffix)).toBe(true);
    expect(next.replaceAll("\r\n", "")).not.toContain("\n");
    const result = collectQuestionFences(next)[0].parse;
    expect(result.ok, JSON.stringify({ result, next })).toBe(true);
    expect(result.ok && result.question.status).toBe("answered");
    expect(result.ok && result.question.answer.answered_by?.kind).toBe("user");
  });
  it("clears question status tags while preserving authored tags and CRLF", async () => {
    const content = question
      .replace("[Q1. Color]", "[Q1. Color] (Tags: human, answered, high)")
      .replace("status: open", "status: answered\r\nseverity: high")
      .replace(
        "prompt: Pick a color",
        [
          "answer:",
          "  selected: [Blue]",
          "  answered_by:",
          "    kind: user",
          "    id: human",
          "prompt: Pick a color",
        ].join("\r\n"),
      );
    const rewrite = vi.fn(() => true);
    renderApp(
      <DocumentPreview
        {...base}
        content={content}
        canAnswer
        onRewrite={rewrite}
        currentUserId="human"
      />,
    );
    fireEvent.click(screen.getByTestId("question-clear-answer"));
    await waitFor(() => expect(rewrite).toHaveBeenCalledOnce());
    const [, next] = rewrite.mock.calls[0] as unknown as [string, string];
    expect(next).toContain("[Q1. Color] (Tags: human, high)\r\n");
    expect(next.endsWith(suffix)).toBe(true);
    expect(next.replaceAll("\r\n", "")).not.toContain("\n");
    const result = collectQuestionFences(next)[0].parse;
    expect(result.ok && result.question.status).toBe("open");
  });
  it.each([
    ["[Q1. Color ([D2])]", "[Q1. Color ([D2])] (Tags: answered)", "README.md"],
    [
      '<a id="color"></a>\n[Q · color]',
      '<a id="color"></a>\n[Q · color]',
      ".g5/ontology/questions.g5.md",
    ],
  ])(
    "preserves the authored question identity %s when answering",
    async (label, expected, filePath) => {
      server.use(
        http.get("/api/v1/workspaces/ws/source-control/ontology-baseline", () =>
          HttpResponse.json({
            data: { baselineCommit: "baseline", content: null },
          }),
        ),
      );
      const content =
        question.replaceAll("\r\n", "\n").replace("[Q1. Color]", label) +
        "\n## Metadata\n\n### color\n- agent\n";
      const rewrite = vi.fn(() => true);
      renderApp(
        <DocumentPreview
          {...base}
          filePath={filePath}
          content={content}
          canAnswer
          onRewrite={rewrite}
          currentUserId="human"
        />,
      );
      fireEvent.click(screen.getAllByTestId("question-option-toggle")[0]);
      fireEvent.click(screen.getByTestId("question-answer-button"));
      await waitFor(() => expect(rewrite).toHaveBeenCalledOnce());
      const [, next] = rewrite.mock.calls[0] as unknown as [string, string];
      expect(next).toContain(`${expected}\n\n\`\`\`g5-question\n`);
      expect(next).toContain("\n## Metadata\n\n### color\n- agent\n");
      expect(next).toContain("status: answered");
    },
  );
  it("retains an answer draft when the shared source refuses a stale action", async () => {
    const rewrite = vi.fn(() => false);
    renderApp(
      <DocumentPreview
        {...base}
        content={question}
        canAnswer
        onRewrite={rewrite}
      />,
    );
    fireEvent.click(screen.getAllByTestId("question-option-toggle")[0]);
    fireEvent.click(screen.getByTestId("question-answer-button"));
    await screen.findByText(/The file changed/);
    expect(screen.getAllByTestId("question-option-toggle")[0]).toHaveAttribute(
      "aria-pressed",
      "true",
    );
  });
  it("offers Comment on selected text and reports the selection's source line", async () => {
    const onCommentSelection = vi.fn();
    const content =
      "# Doc\n\nFirst paragraph.\n\nSecond paragraph\nruns on here.\n";
    renderApp(
      <DocumentPreview
        {...base}
        content={content}
        onCommentSelection={onCommentSelection}
      />,
    );
    const host = await screen.findByTestId("document-preview-content");
    const paragraph = screen.getByText(/Second paragraph/).closest("p");
    if (!paragraph) throw new Error("no paragraph");
    const text = Array.from(paragraph.childNodes).find(
      (node) =>
        node.nodeType === Node.TEXT_NODE &&
        node.textContent?.includes("runs on"),
    );
    if (!text) throw new Error("no text node");
    const at = text.textContent!.indexOf("runs on");
    const range = document.createRange();
    range.setStart(text, at);
    range.setEnd(text, at + "runs on".length);
    const selection = window.getSelection()!;
    selection.removeAllRanges();
    selection.addRange(range);
    fireEvent.mouseUp(host);
    await screen.findByTestId("document-preview-selection-comment-button");
    // Another selection, made without a mouse-up, is what the bubble is
    // about once it settles.
    const first = screen.getByText("First paragraph.");
    const other = document.createRange();
    other.selectNodeContents(first);
    selection.removeAllRanges();
    selection.addRange(other);
    document.dispatchEvent(new Event("selectionchange"));
    await new Promise((resolve) => setTimeout(resolve, 250));
    fireEvent.click(
      await screen.findByTestId("document-preview-selection-comment-button"),
    );
    expect(onCommentSelection).toHaveBeenCalledWith({
      line: 3,
      quote: "First paragraph.",
    });
    onCommentSelection.mockClear();
    // Back to the first words, by mouse.
    selection.removeAllRanges();
    selection.addRange(range);
    fireEvent.mouseUp(host);
    fireEvent.click(
      await screen.findByTestId("document-preview-selection-comment-button"),
    );
    expect(onCommentSelection).toHaveBeenCalledWith({
      line: 6,
      quote: "runs on",
    });
    expect(
      screen.queryByTestId("document-preview-selection-comment-button"),
    ).toBeNull();

    // A collapsed selection offers nothing.
    selection.removeAllRanges();
    fireEvent.mouseUp(host);
    await new Promise((resolve) => setTimeout(resolve, 30));
    expect(
      screen.queryByTestId("document-preview-selection-comment-button"),
    ).toBeNull();
  });

  it("offers Comment on a selection made by keyboard, without a mouse-up", async () => {
    const onCommentSelection = vi.fn();
    const content = "# Doc\n\nFirst paragraph.\n\nSecond paragraph.\n";
    renderApp(
      <DocumentPreview
        {...base}
        content={content}
        onCommentSelection={onCommentSelection}
      />,
    );
    await screen.findByTestId("document-preview-content");
    const paragraph = screen.getByText("Second paragraph.");
    const range = document.createRange();
    range.selectNodeContents(paragraph);
    const selection = window.getSelection()!;
    selection.removeAllRanges();
    selection.addRange(range);
    document.dispatchEvent(new Event("selectionchange"));
    fireEvent.click(
      await screen.findByTestId("document-preview-selection-comment-button"),
    );
    expect(onCommentSelection).toHaveBeenCalledWith({
      line: 5,
      quote: "Second paragraph.",
    });
    // Collapsing the selection removes the bubble.
    selection.removeAllRanges();
    selection.addRange(range);
    document.dispatchEvent(new Event("selectionchange"));
    await screen.findByTestId("document-preview-selection-comment-button");
    selection.removeAllRanges();
    document.dispatchEvent(new Event("selectionchange"));
    await waitFor(() =>
      expect(
        screen.queryByTestId("document-preview-selection-comment-button"),
      ).toBeNull(),
    );
  });

  it("drops a bubble when commenting becomes unavailable, so it cannot return stale", async () => {
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    const wrapper = ({ children }: { children: React.ReactNode }) => (
      <QueryClientProvider client={client}>{children}</QueryClientProvider>
    );
    const content = "# Doc\n\nFirst paragraph.\n\nSecond paragraph.\n";
    const onCommentSelection = vi.fn();
    const ui = (offered: boolean) => (
      <DocumentPreview
        {...base}
        content={content}
        onCommentSelection={offered ? onCommentSelection : undefined}
      />
    );
    const { rerender } = render(ui(true), { wrapper });
    const host = await screen.findByTestId("document-preview-content");
    const paragraph = screen.getByText("Second paragraph.");
    const range = document.createRange();
    range.selectNodeContents(paragraph);
    const selection = window.getSelection()!;
    selection.removeAllRanges();
    selection.addRange(range);
    fireEvent.mouseUp(host);
    await screen.findByTestId("document-preview-selection-comment-button");
    // Starting a thread becomes impossible, the selection collapses
    // unobserved, then starting is possible again: no old bubble returns.
    rerender(ui(false));
    expect(
      screen.queryByTestId("document-preview-selection-comment-button"),
    ).toBeNull();
    selection.removeAllRanges();
    rerender(ui(true));
    await new Promise((resolve) => setTimeout(resolve, 200));
    expect(
      screen.queryByTestId("document-preview-selection-comment-button"),
    ).toBeNull();
  });

  it("offers no Comment on selected text while the person may not start a thread", async () => {
    const content = "# Doc\n\nFirst paragraph.\n";
    renderApp(<DocumentPreview {...base} content={content} />);
    const host = await screen.findByTestId("document-preview-content");
    const paragraph = screen.getByText("First paragraph.");
    const range = document.createRange();
    range.selectNodeContents(paragraph);
    const selection = window.getSelection()!;
    selection.removeAllRanges();
    selection.addRange(range);
    fireEvent.mouseUp(host);
    await new Promise((resolve) => setTimeout(resolve, 30));
    expect(
      screen.queryByTestId("document-preview-selection-comment-button"),
    ).toBeNull();
  });

  it("offers no per-record Comment and never makes preview text editable", () => {
    const rewrite = vi.fn();
    const { container } = renderApp(
      <DocumentPreview
        {...base}
        content={"# Doc\n\n[I1. Intent]\n\nBody."}
        onCommentSelection={vi.fn()}
        onRewrite={rewrite}
      />,
    );
    expect(screen.queryByRole("button", { name: /Comment on I1/ })).toBeNull();
    expect(rewrite).not.toHaveBeenCalled();
    expect(container.querySelector('[contenteditable="true"]')).toBeNull();
  });
});

/** Selects `text` inside `element`'s first text node containing it. */
function select(element: HTMLElement, text: string) {
  const walker = document.createTreeWalker(element, NodeFilter.SHOW_TEXT);
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    const at = node.textContent?.indexOf(text) ?? -1;
    if (at < 0) continue;
    const range = document.createRange();
    range.setStart(node, at);
    range.setEnd(node, at + text.length);
    const selection = window.getSelection()!;
    selection.removeAllRanges();
    selection.addRange(range);
    fireEvent.mouseUp(screen.getByTestId("document-preview-content"));
    return;
  }
  throw new Error(`no text ${text}`);
}

describe("selection commenting in Preview", () => {
  // `g5.documents.comments.identity`: selecting current text offers Comment,
  // quoting it and anchoring to its source line.
  const ontology = {
    ...base,
    filePath: ".g5/ontology/doc.g5.md",
    workspaceId: "",
    repoId: "",
  };
  const doc =
    "# Doc\n\n" +
    '<a id="a.one"></a>\n[I · a.one]\n\nFirst body\nwrapped second line.\n\n' +
    "## Metadata\n\n### a.one\n\n- human-provided\n";

  it("anchors the Comment to the selected text's source line", async () => {
    const comment = vi.fn();
    renderApp(
      <DocumentPreview
        {...ontology}
        content={doc}
        onCommentSelection={comment}
      />,
    );
    select(screen.getByText(/First body/), "second line");
    fireEvent.click(
      await screen.findByTestId("document-preview-selection-comment-button"),
    );
    expect(comment).toHaveBeenCalledWith({ line: 7, quote: "second line" });
  });

  it("offers no Comment in statement details or without permission", async () => {
    const comment = vi.fn();
    const { rerender } = renderApp(
      <DocumentPreview
        {...ontology}
        content={doc}
        onCommentSelection={comment}
      />,
    );
    select(screen.getByTestId("semantic-record-metadata"), "human-provided");
    await new Promise((resolve) => setTimeout(resolve, 10));
    expect(
      screen.queryByTestId("document-preview-selection-comment-button"),
    ).toBeNull();
    rerender(
      <QueryClientProvider client={new QueryClient()}>
        <DocumentPreview {...ontology} content={doc} />
      </QueryClientProvider>,
    );
    select(screen.getByText(/First body/), "First");
    await new Promise((resolve) => setTimeout(resolve, 10));
    expect(
      screen.queryByTestId("document-preview-selection-comment-button"),
    ).toBeNull();
  });

  it("leaves a double-click in statement details to the text", () => {
    const edit = vi.fn();
    renderApp(
      <DocumentPreview {...ontology} content={doc} onRequestEdit={edit} />,
    );
    fireEvent.doubleClick(screen.getByTestId("semantic-record-metadata"));
    expect(edit).not.toHaveBeenCalled();
    fireEvent.doubleClick(screen.getByText(/First body/));
    expect(edit).toHaveBeenCalledTimes(1);
  });
});

describe("Preview navigation targets", () => {
  // `g5.documents.ontology.links`: a target jumps once with a brief
  // highlight; ontology fragments follow the ontology's own anchor rule.
  const ontology = {
    ...base,
    filePath: ".g5/ontology/doc.g5.md",
    workspaceId: "",
    repoId: "",
  };
  const doc =
    "# Doc\n\n## Read & write — paths/rules\n\nPlain prose.\n\n" +
    "## Under a heading\n\n[D · h.one]\n\nHeading body.\n\n" +
    '<a id="a.two"></a>\n[I · a.two]\n\nAnchored body.\n\n' +
    "## Metadata\n\n### h.one\n\n- agent\n\n### a.two\n\n- agent\n";

  it.each([
    ["an anchored statement", "#a.two", "Anchored body."],
    [
      "a statement written under a heading",
      "#under-a-heading",
      "Heading body.",
    ],
    [
      "a heading with punctuation",
      "#read-and-write-paths-rules",
      "Read & write",
    ],
  ])("finds %s", async (_name, target, text) => {
    const scrolled = vi.fn();
    Element.prototype.scrollIntoView = scrolled;
    renderApp(
      <DocumentPreview
        {...ontology}
        content={doc}
        target={target}
        targetSeq={1}
      />,
    );
    await waitFor(() => expect(scrolled).toHaveBeenCalledTimes(1));
    expect(scrolled.mock.contexts[0] as HTMLElement).toHaveTextContent(text);
    expect(screen.queryByText(/Missing target/)).toBeNull();
  });

  it("jumps once per request with a brief highlight, and again for a new one", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      const scrolled = vi.fn();
      Element.prototype.scrollIntoView = scrolled;
      const client = new QueryClient();
      const view = (content: string, seq: number) => (
        <QueryClientProvider client={client}>
          <DocumentPreview
            {...ontology}
            content={content}
            target="#a.two"
            targetSeq={seq}
          />
        </QueryClientProvider>
      );
      const { rerender } = render(view(doc, 1));
      await waitFor(() => expect(scrolled).toHaveBeenCalledTimes(1));
      const target = scrolled.mock.contexts[0] as HTMLElement;
      expect(target).toHaveClass("ring-2");
      rerender(view(doc.replace("Plain prose.", "Plain prose, edited."), 1));
      await act(() => vi.advanceTimersByTimeAsync(2100));
      expect(scrolled).toHaveBeenCalledTimes(1);
      expect(target).not.toHaveClass("ring-2");
      rerender(view(doc, 2));
      await waitFor(() => expect(scrolled).toHaveBeenCalledTimes(2));
    } finally {
      vi.useRealTimers();
    }
  });

  it("keeps the highlight for its full time when the view remounts, as after Rich text", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      Element.prototype.scrollIntoView = vi.fn();
      const client = new QueryClient();
      const readingState = {};
      const view = (key: string) => (
        <QueryClientProvider client={client}>
          <DocumentPreview
            key={key}
            {...ontology}
            content={doc}
            target="#a.two"
            targetSeq={1}
            readingState={readingState}
          />
        </QueryClientProvider>
      );
      const highlighted = () => document.querySelector<HTMLElement>(".ring-2");
      const { rerender } = render(view("first"));
      await waitFor(() => expect(highlighted()).not.toBeNull());
      // The same request, already handled, in a new instance of the view.
      await act(() => vi.advanceTimersByTimeAsync(500));
      rerender(view("second"));
      expect(highlighted()).not.toBeNull();
      expect(highlighted()).toHaveTextContent("Anchored body.");
      await act(() => vi.advanceTimersByTimeAsync(1600));
      expect(highlighted()).toBeNull();
    } finally {
      vi.useRealTimers();
    }
  });

  it("names a missing target", async () => {
    renderApp(
      <DocumentPreview
        {...ontology}
        content={doc}
        target="#gone"
        targetSeq={1}
      />,
    );
    expect(await screen.findByText("Missing target: #gone")).toBeVisible();
  });

  it("lists a statement's heading by kind in the outline", () => {
    renderApp(<DocumentPreview {...ontology} content={doc} />);
    const outline = screen.getByTestId("editor-outline");
    expect(outline).toHaveTextContent("Design");
    expect(outline).not.toHaveTextContent("Under a heading");
  });
});

describe("Preview link handling outside ontology files", () => {
  // `g5.documents.file-links.line`: only ontology files take the ontology
  // link handler, so a Project Doc's line links keep their line.
  it("opens a relative source link at its line in a Project Doc", () => {
    const open = vi.fn();
    const navigate = vi.fn();
    renderApp(
      <DocumentPreview
        {...base}
        filePath="docs/ws/notes.md"
        content="See [main](../../src/main.rs#L12)."
        onOpenWorkspaceFile={open}
        onNavigate={navigate}
      />,
    );
    fireEvent.click(screen.getByRole("link", { name: "main" }));
    expect(open).toHaveBeenCalledWith("src/main.rs", "repo", 12);
    expect(navigate).not.toHaveBeenCalled();
  });
});

describe("Preview reading state", () => {
  // `g5.documents.editor.modes`: switching views keeps filters and details.
  it("restores filters and open details after a remount", async () => {
    const reading = {};
    const doc =
      "# Doc\n\n" +
      '<a id="h"></a>\n[I · h]\n\nHuman body.\n\n' +
      '<a id="g"></a>\n[I · g]\n\nAgent body.\n\n' +
      "## Metadata\n\n### h\n\n- human-provided\n\n### g\n\n- agent\n";
    const props = {
      ...base,
      filePath: ".g5/ontology/doc.g5.md",
      workspaceId: "",
      repoId: "",
      content: doc,
      readingState: reading,
    };
    const first = renderApp(<DocumentPreview {...props} />);
    fireEvent.click(screen.getByRole("button", { name: "Compare" }));
    fireEvent.click(screen.getByTestId("toolbar-tag-human"));
    fireEvent.click(screen.getByTestId("semantic-record-details-toggle"));
    first.unmount();
    renderApp(<DocumentPreview {...props} />);
    expect(screen.getByRole("button", { name: "Compare" })).toHaveAttribute(
      "aria-pressed",
      "false",
    );
    expect(screen.getByTestId("toolbar-tag-human")).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(screen.queryByText("Agent body.")).toBeNull();
    expect(
      screen.getByTestId("semantic-record-details-toggle"),
    ).toHaveAttribute("aria-expanded", "true");
  });
});
