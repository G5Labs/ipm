import React from "react";
import { QueryClientProvider } from "@tanstack/react-query";
import {
  act,
  fireEvent,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { HttpResponse, http } from "msw";
import { MemoryRouter } from "react-router-dom";
import { renderApp } from "../../test/render";
import { server } from "../../test/msw/server";
import { MockWebSocketProvider } from "../../test/msw/ws-mock";
import { AuthProvider, RequireAuth } from "../../hooks/useAuth";
import type { ReviewSuggestion } from "../../lib/types";
import {
  DocumentPreview,
  type DocumentPreviewProps,
} from "../files/DocumentPreview";
import { reviewEditFixture } from "../../test/reviewEdits";
import { ComposerOwner } from "../comments/ComposerOwner";
import { getApiToken, setApiToken, tokenPrincipal } from "../../lib/api";

const hash = vi.hoisted(() => ({ selectedSource: vi.fn() }));
vi.mock("../../lib/commentAnchors", async (original) => ({
  ...(await original<typeof import("../../lib/commentAnchors")>()),
  sha256Utf8: hash.selectedSource,
}));
const anchors = await vi.importActual<
  typeof import("../../lib/commentAnchors")
>("../../lib/commentAnchors");

beforeEach(() => {
  hash.selectedSource.mockReset().mockImplementation(anchors.sha256Utf8);
  window.getSelection()?.removeAllRanges();
});

function review(overrides: Partial<ReviewSuggestion> = {}): ReviewSuggestion {
  return {
    id: "review",
    publicationId: "publication",
    findingId: null,
    repoId: "repo",
    path: "docs/spec.md",
    kind: "hunk",
    beforeLines: "Original.\n",
    afterLines: "Proposed.\n",
    anchorStart: 1,
    contentHash: null,
    anchorContext: null,
    authorKind: "human",
    authorLabel: "Reviewer",
    note: "",
    commentId: null,
    status: "open",
    statusBy: null,
    statusAt: null,
    supersedes: null,
    adoptedFrom: null,
    createdAt: "2026-09-30T12:00:00Z",
    ...overrides,
  };
}

function editor(
  content: string,
  suggestion: ReviewSuggestion,
  extra: Partial<DocumentPreviewProps> = {},
) {
  return (
    <DocumentPreview
      workspaceId=""
      repoId=""
      filePath="docs/spec.md"
      content={content}
      canAnswer
      onRewrite={vi.fn(() => true)}
      onNavigate={vi.fn()}
      suggestionOverlay={{
        workspaceId: "ws-source-1",
        suggestions: [suggestion],
        viewerOwnsWorkspace: true,
        viewerUserId: "owner",
        ownerVmRunning: true,
        sourceWritable: true,
      }}
      {...extra}
    />
  );
}

function rerender(
  view: ReturnType<typeof renderApp>,
  next: React.ReactElement,
) {
  view.rerender(
    <QueryClientProvider client={view.queryClient}>
      <MemoryRouter>
        <AuthProvider>
          <MockWebSocketProvider controller={view.ws}>
            {next}
          </MockWebSocketProvider>
        </AuthProvider>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

/** The card is in the margin beside the document, which holds the preview. */
function expectBesideDocument(card: HTMLElement, preview: HTMLElement) {
  expect(screen.getByTestId("document-margin")).toContainElement(card);
  expect(screen.getByTestId("document-preview-content")).toContainElement(
    preview,
  );
  expect(screen.getByTestId("document-preview-content")).not.toContainElement(
    card,
  );
}

const question = [
  "```g5-question",
  "schema: 1",
  "id: canonical-question",
  "status: open",
  "prompt: Which color should we use?",
  "options:",
  "  - label: Blue",
  "    description: Use the info token",
  "```",
].join("\n");

describe("whitespace source previews", () => {
  it.each(["[I1. First]\nBody.\n", question])(
    "keeps canonical structured controls beside whitespace source edits: %s",
    async (content) => {
      const proposal = review({
        beforeLines: content,
        contentHash: await anchors.sha256Utf8(content),
        edits: [reviewEditFixture("structured-space", content, 0, 0, " ")],
      });
      renderApp(editor(content, proposal));
      const diff = await screen.findByTestId("spec-review-source-diff");
      expect(diff.querySelector("pre")!.textContent).toBe("·");
      expect(
        screen.getByTestId("spec-suggestion-card-review:structured-space"),
      ).not.toHaveTextContent("Needs attention");
      if (content === question)
        expect(
          screen.getByTestId("structured-question-editor"),
        ).toBeInTheDocument();
      else
        expect(screen.getByText("First", { exact: true })).toBeInTheDocument();
    },
  );
  it("explains a filtered boundary instead of moving its inline diff below the document", async () => {
    const content =
      "# Plan\n\n[I1. Keep] (Tags: human)\n\nBody one.\n\n[I2. Other]\n\nBody two.\n";
    const at = content.indexOf("[I2.");
    const proposal = review({
      beforeLines: content,
      contentHash: await anchors.sha256Utf8(content),
      edits: [reviewEditFixture("boundary", content, at, at, "\n")],
    });
    renderApp(editor(content, proposal));
    await screen.findByTestId("spec-review-source-diff");
    fireEvent.click(screen.getByTestId("toolbar-tag-human"));
    expect(screen.queryByTestId("spec-review-source-diff")).toBeNull();
    expect(screen.getByText("Review target hidden by filters.")).toBeVisible();
    expect(
      screen.getByTestId("spec-suggestion-card-review:boundary"),
    ).toBeVisible();
  });

  it.each([
    { at: 0, remove: "", add: "\n", boundary: 0 },
    { at: 7, remove: "", add: "\n", boundary: 1 },
    { at: 8, remove: "", add: " ", boundary: null },
    { at: 13, remove: "\n", add: "", boundary: 2 },
    { at: 20, remove: "", add: "\n", boundary: 3 },
  ])(
    "renders visible source bytes beside the card at $at",
    async ({ at, remove, add, boundary }) => {
      const content = "Intro.\n\nLGTM\n\nEnd.\n";
      // Use the actual requested location for the EOF case.
      if (at === 20) at = content.length;
      const proposal = review({
        beforeLines: content,
        afterLines:
          content.slice(0, at) + add + content.slice(at + remove.length),
        contentHash: await anchors.sha256Utf8(content),
        edits: [
          reviewEditFixture("space", content, at, at + remove.length, add),
        ],
      });
      const onRewrite = vi.fn();
      renderApp(editor(content, proposal, { onRewrite }));
      const preview = await screen.findByTestId("spec-review-source-diff");
      const card = screen.getByTestId("spec-suggestion-card-review:space");
      expectBesideDocument(card, preview);
      expect(card).not.toHaveTextContent("Needs attention");
      const marker = within(preview).getByRole("button", {
        name: new RegExp(add ? "Suggested addition" : "Suggested removal"),
      });
      expect(marker.textContent).toContain(add === " " ? "·" : "↵");
      fireEvent.click(marker);
      expect(screen.getByTestId("spec-review-edit-review:space")).toHaveFocus();
      const container = preview.closest(".spec-review-container")!;
      const rows = [
        ...screen
          .getByTestId("document-preview-content")
          .querySelectorAll("[data-block-index], .spec-review-container"),
      ];
      if (boundary !== null) {
        const next = rows.findIndex(
          (node) => node.getAttribute("data-block-index") === String(boundary),
        );
        if (next >= 0) expect(rows.indexOf(container)).toBeLessThan(next);
        if (boundary > 0) {
          const previous = rows.findIndex(
            (node) =>
              node.getAttribute("data-block-index") === String(boundary - 1),
          );
          expect(rows.indexOf(container)).toBeGreaterThan(previous);
        }
      }
      for (const name of ["Accept", "Edit", "Reject", "Discussion"])
        expect(within(card).getByRole("button", { name })).toBeEnabled();
      expect(onRewrite).not.toHaveBeenCalled();
    },
  );

  it("combines independent whitespace and text edits without repeating the canonical block", async () => {
    const content = "Intro.\n\nLGTM friend.\n\nEnd.\n";
    const proposal = review({
      beforeLines: content,
      contentHash: await anchors.sha256Utf8(content),
      edits: [
        reviewEditFixture("space", content, 8, 8, " "),
        reviewEditFixture("word", content, 13, 20, "team."),
      ],
    });
    renderApp(editor(content, proposal));
    const preview = await screen.findByTestId("spec-review-source-diff");
    expect(preview.querySelector("pre")!.textContent).toBe(
      "·LGTM·friend.team.",
    );
    expect(screen.getAllByTestId(/^spec-suggestion-card-/)).toHaveLength(2);
    expect(screen.getAllByTestId("spec-review-source-diff")).toHaveLength(1);
  });

  it("offers original/current/suggested source for ambiguous placement", async () => {
    const content = "LGTM\n\nLGTM\n";
    renderApp(
      editor(content, review({ beforeLines: "LGTM\n", afterLines: " LGTM\n" })),
    );
    expect(screen.queryByTestId("spec-review-source-diff")).toBeNull();
    const card = await screen.findByTestId("spec-suggestion-card-review");
    expect(card).toHaveTextContent("more than once");
    expect(
      [...card.querySelectorAll("pre")].map((node) => node.textContent),
    ).toEqual(["LGTM\n", " LGTM\n"]);
    const current = within(card)
      .getByText("Current document source")
      .closest("details")!;
    current.open = true;
    fireEvent(current, new Event("toggle"));
    expect(current.querySelector("pre")!.textContent).toBe(content);
  });
});

describe("DocumentPreview independent durable edit bubbles", () => {
  it.each(
    ["start", "middle", "end"].flatMap((position) =>
      [false, true, "canonical" as const].map((identical) => ({
        position,
        identical,
      })),
    ),
  )(
    "renders ordered same-point blocks at $position with canonical identity once (identical=$identical)",
    async ({ position, identical }) => {
      const content = "Left.\n\nRight.\n";
      const at =
        position === "start" ? 0 : position === "middle" ? 7 : content.length;
      const canonical = position === "start" ? "Left." : "Right.";
      const first = identical === "canonical" ? canonical : "First added.";
      const addedTexts = [first, identical ? first : "Second added."];
      const deltas = addedTexts.map((text) =>
        position === "end" ? `\n${text}\n` : `${text}\n\n`,
      );
      const parent = review({
        id: "ordered-blocks",
        beforeLines: content,
        afterLines: content.slice(0, at) + deltas.join("") + content.slice(at),
        contentHash: await anchors.sha256Utf8(content),
        edits: deltas.map((delta, index) =>
          reviewEditFixture(`added-${index}`, content, at, at, delta),
        ),
      });
      const onRewrite = vi.fn();
      renderApp(editor(content, parent, { onRewrite }));
      const preview = await screen.findByTestId(
        "spec-suggestion-preview-ordered-blocks:added-0",
      );
      const rows = [...preview.children] as HTMLElement[];
      expect(rows.map((row) => row.textContent)).toEqual(
        position === "end"
          ? [canonical, ...addedTexts]
          : [...addedTexts, canonical],
      );
      const canonicalIndex = position === "start" ? "0" : "1";
      expect(rows.map((row) => row.getAttribute("data-block-index"))).toEqual(
        position === "end"
          ? [canonicalIndex, null, null]
          : [null, null, canonicalIndex],
      );
      expect(
        preview.querySelectorAll(`[data-block-index="${canonicalIndex}"]`),
      ).toHaveLength(1);
      expect(
        within(preview).getAllByText(canonical, { exact: true }),
      ).toHaveLength(identical === "canonical" ? 3 : 1);
      const additions = [
        ...preview.querySelectorAll<HTMLElement>(
          '[data-spec-review-change="add"]',
        ),
      ];
      expect(additions).toHaveLength(2);
      for (const [index, added] of additions.entries()) {
        expect(added).toHaveTextContent(addedTexts[index]);
        expect(added.closest("[data-block-index]")).toBeNull();
        expect(added).toHaveAttribute("data-comment-connector-ignore");
        fireEvent.click(added);
        expect(
          screen.getByTestId(`spec-review-edit-ordered-blocks:added-${index}`),
        ).toHaveFocus();
        const selected = document.createRange();
        selected.selectNodeContents(added);
        window.getSelection()!.removeAllRanges();
        window.getSelection()!.addRange(selected);
        fireEvent.mouseUp(added);
        expect(screen.queryByTestId("selection-toolbar-comment")).toBeNull();
      }
      for (let index = 0; index < 2; index++) {
        const card = screen.getByTestId(
          `spec-suggestion-card-ordered-blocks:added-${index}`,
        );
        expect(card).not.toHaveTextContent("Needs attention");
        expect(
          within(card).getByRole("button", { name: "Accept" }),
        ).toBeInTheDocument();
        expect(
          within(card).getByRole("button", { name: "Reject" }),
        ).toBeInTheDocument();
      }
      expect(onRewrite).not.toHaveBeenCalled();
    },
  );

  it("matches the native flow's actual producer ledger and canonical indices after selected decisions", async () => {
    const original = "old\n\nQuiet.\n";
    const parent = review({
      id: "native-fixture",
      beforeLines: original,
      afterLines:
        "replacement\n\nQuiet. added\n\nSecond added block.\n\nQuiet.\n",
      contentHash: await anchors.sha256Utf8(original),
      edits: [
        reviewEditFixture("replacement", original, 0, 3, "replacement"),
        reviewEditFixture("added", original, 5, 5, "Quiet. added\n\n"),
        reviewEditFixture(
          "remaining",
          original,
          5,
          5,
          "Second added block.\n\n",
        ),
      ],
    });
    const view = renderApp(editor(original, parent));
    await screen.findByTestId("spec-suggestion-preview-native-fixture:added");
    const stages = [
      {
        source: original,
        accepted: 0,
        index: 1,
        additions: ["Quiet. added", "Second added block."],
        text: [
          "oldreplacement",
          "Quiet. added",
          "Second added block.",
          "Quiet.",
        ],
      },
      {
        source: "replacement\n\nQuiet.\n",
        accepted: 1,
        index: 1,
        additions: ["Quiet. added", "Second added block."],
        text: ["replacement", "Quiet. added", "Second added block.", "Quiet."],
      },
      {
        source: "replacement\n\nQuiet. added\n\nQuiet.\n",
        accepted: 2,
        index: 2,
        additions: ["Second added block."],
        text: ["replacement", "Quiet. added", "Second added block.", "Quiet."],
      },
    ];
    for (const stage of stages) {
      const current = {
        ...parent,
        expectedBeforeLines: stage.source,
        edits: parent.edits!.map((edit, index) => ({
          ...edit,
          status: index < stage.accepted ? "accepted" : "open",
          acceptedAfterText: index < stage.accepted ? edit.afterText : null,
          currentStart: index ? stage.source.indexOf("Quiet.\n") : 0,
          currentEnd: index
            ? stage.source.indexOf("Quiet.\n")
            : stage.accepted
              ? 11
              : 3,
        })),
      };
      await act(async () => rerender(view, editor(stage.source, current)));
      const content = screen.getByTestId("document-preview-content");
      expect(
        [...content.querySelectorAll("p")]
          .map((node) => node.textContent)
          .filter((text) => stage.text.includes(text ?? "")),
      ).toEqual(stage.text);
      expect(
        [...content.querySelectorAll('[data-spec-review-change="add"]')].map(
          (node) => node.textContent,
        ),
      ).toEqual(stage.additions);
      const canonical = within(content).getByText("Quiet.", { exact: true });
      expect(canonical.closest("[data-block-index]")).toHaveAttribute(
        "data-block-index",
        String(stage.index),
      );
      expect(canonical.closest("[data-comment-connector-ignore]")).toBeNull();
      expect(content).not.toHaveTextContent("Needs attention");
    }
  });

  it("renders only the open captured edit after restoring original bytes, keeping accepted history intact", async () => {
    const original = "prefix keep old.\n";
    const historical = "é🦀\nchanged keep old.\n";
    const at = original.indexOf("old");
    const currentAt = historical.indexOf("old");
    const bytes = (value: string) => new TextEncoder().encode(value).length;
    const parent = review({
      id: "restored-rich",
      beforeLines: original,
      expectedBeforeLines: historical,
      contentHash: await anchors.sha256Utf8(original),
      edits: [
        reviewEditFixture(
          "historically-accepted",
          original,
          0,
          6,
          "é🦀\nchanged",
          {
            status: "accepted",
            acceptedAfterText: "é🦀\nchanged",
          },
        ),
        reviewEditFixture("open", original, at, at + 3, "new", {
          sourceConflict: true,
          currentStart: bytes(historical.slice(0, currentAt)),
          currentEnd: bytes(historical.slice(0, currentAt + 3)),
        }),
      ],
    });
    const onRewrite = vi.fn();
    renderApp(editor(original, parent, { onRewrite }));
    const preview = await screen.findByTestId(
      "spec-suggestion-preview-restored-rich:open",
    );
    expect(preview).toHaveTextContent("prefix keep oldnew.");
    expect(preview).not.toHaveTextContent("é🦀");
    const card = screen.getByTestId("spec-suggestion-card-restored-rich:open");
    expect(card).not.toHaveTextContent("source needs attention");
    expect(card).toHaveTextContent("Open");
    expect(parent.edits![0].status).toBe("accepted");
    expect(parent.edits![1].sourceConflict).toBe(true);
    expect(onRewrite).not.toHaveBeenCalled();
  });

  it("shows an owner-conflicted empty insertion as its own attention card without inserting proposed text", async () => {
    const onRewrite = vi.fn();
    renderApp(
      editor(
        "",
        review({
          id: "conflicted-parent",
          beforeLines: "",
          afterLines: "Proposed text.\n",
          contentHash: await anchors.sha256Utf8("Historical.\n"),
          edits: [
            reviewEditFixture("conflicted", "", 0, 0, "Proposed text.\n", {
              sourceConflict: true,
              commentId: "conflicted-discussion",
            }),
          ],
        }),
        { onRewrite },
      ),
    );
    const card = await screen.findByTestId(
      "spec-suggestion-card-conflicted-parent:conflicted",
    );
    expect(card).toHaveTextContent(
      "This edit’s source needs attention after other accepted changes.",
    );
    expect(
      within(card).getByRole("button", { name: "Discussion" }),
    ).toBeInTheDocument();
    expect(
      screen.queryByTestId(
        "spec-suggestion-preview-conflicted-parent:conflicted",
      ),
    ).toBeNull();
    expect(
      document.querySelector('[data-spec-review-change="add"]'),
    ).toBeNull();
    expect(onRewrite).not.toHaveBeenCalled();
  });

  it("shows the failed edit and current source without the removed parent-context disclosure", async () => {
    renderApp(editor("Canonical.\n", review({ beforeLines: "Missing.\n" })));
    const card = await screen.findByTestId("spec-suggestion-card-review");
    expect(card).toHaveTextContent("Missing.");
    expect(card).toHaveTextContent("Proposed.");
    expect(within(card).queryByText("Original proposal context")).toBeNull();
    const details = within(card).getByText("Current document source")
      .parentElement as HTMLDetailsElement;
    details.open = true;
    fireEvent(details, new Event("toggle"));
    expect(card).toHaveTextContent("Canonical.");
  });

  it("keeps every large-review edit independently actionable without duplicating parent context", async () => {
    const content = "Alpha ".repeat(140) + "\n";
    const edits = Array.from({ length: 129 }, (_, i) =>
      reviewEditFixture(`large-${i}`, content, i * 6, i * 6 + 5, "Beta", {
        expectedBeforeLines: undefined,
        expectedAfterLines: undefined,
      }),
    );
    renderApp(
      editor(
        content,
        review({
          id: "large-parent",
          beforeLines: content,
          expectedBeforeLines: content,
          afterLines: content.replace("Alpha", "Beta"),
          edits,
        }),
      ),
    );
    const card = await screen.findByTestId(
      "spec-suggestion-card-large-parent:large-128",
    );
    expect(
      screen.getAllByTestId(/^spec-suggestion-card-large-parent:/),
    ).toHaveLength(129);
    expect(within(card).getByRole("button", { name: "Accept" })).toBeEnabled();
    expect(within(card).getByRole("button", { name: "Reject" })).toBeEnabled();
    expect(within(card).queryByText("Original", { exact: true })).toBeNull();
    expect(
      [...card.querySelectorAll("pre")].map((node) => node.textContent),
    ).toEqual(["Alpha", "Beta"]);
    expect(within(card).queryByText("Original proposal context")).toBeNull();
    expect(
      [...card.querySelectorAll("pre")].some(
        (node) => node.textContent === content,
      ),
    ).toBe(false);
  });

  it("carries pending draft replacement words to the new parent instance and targets that parent", async () => {
    const content = "Alpha unchanged.\n";
    const parent = review({
      id: "old-parent",
      beforeLines: content,
      afterLines: "Beta unchanged.\n",
      edits: [reviewEditFixture("stable-edit", content, 0, 5, "Beta")],
    });
    const view = renderApp(
      <ComposerOwner scope="ws-source-1">
        {editor(content, parent)}
      </ComposerOwner>,
    );
    const oldCard = await screen.findByTestId(
      "spec-suggestion-card-old-parent:stable-edit",
    );
    fireEvent.click(within(oldCard).getByRole("button", { name: "Edit" }));
    fireEvent.change(
      await screen.findByRole("textbox", { name: "Edited replacement" }),
      { target: { value: "Pending owner wording" } },
    );
    const current = { ...parent, id: "new-parent" };
    const writes: unknown[] = [];
    server.use(
      http.post(
        "/api/v1/workspaces/ws-source-1/review/suggestions/new-parent/accept",
        async ({ request }) => {
          writes.push(await request.json());
          return HttpResponse.json({
            data: {
              suggestion: {
                ...current,
                status: "accepted",
                edits: [
                  {
                    ...current.edits![0],
                    status: "accepted",
                    acceptedAfterText: "Pending owner wording",
                    statusAt: "2026-10-01T16:11:00Z",
                  },
                ],
              },
              writtenHash: "next",
              alreadyApplied: false,
            },
          });
        },
      ),
    );
    await act(async () => {
      rerender(
        view,
        <ComposerOwner scope="ws-source-1">
          {editor(content, current)}
        </ComposerOwner>,
      );
    });
    const currentCard = screen.getByTestId(
      "spec-suggestion-card-new-parent:stable-edit",
    );
    expect(
      within(currentCard).getByRole("textbox", { name: "Edited replacement" }),
    ).toHaveValue("Pending owner wording");
    fireEvent.click(
      within(currentCard).getByRole("button", { name: "Accept edited" }),
    );
    await waitFor(() =>
      expect(writes).toEqual([
        { editId: "stable-edit", afterLines: "Pending owner wording" },
      ]),
    );
  });

  it("does not carry draft words after the rendered principal changes", async () => {
    const content = "Alpha unchanged.\n";
    const parent = review({
      id: "old-owner-parent",
      beforeLines: content,
      afterLines: "Beta unchanged.\n",
      edits: [reviewEditFixture("owner-stable-edit", content, 0, 5, "Beta")],
    });
    const view = renderApp(
      <ComposerOwner scope="ws-source-1">
        {editor(content, parent)}
      </ComposerOwner>,
    );
    const card = await screen.findByTestId(
      "spec-suggestion-card-old-owner-parent:owner-stable-edit",
    );
    fireEvent.click(within(card).getByRole("button", { name: "Edit" }));
    fireEvent.change(
      within(card).getByRole("textbox", { name: "Edited replacement" }),
      { target: { value: "Private old-owner draft" } },
    );
    const token = getApiToken();
    const currentPrincipal = tokenPrincipal(token);
    expect(currentPrincipal).not.toBeNull();
    const org = currentPrincipal!.split(":")[1];
    const encode = (value: object) =>
      btoa(JSON.stringify(value))
        .replace(/\+/g, "-")
        .replace(/\//g, "_")
        .replace(/=+$/, "");
    try {
      await act(async () => {
        setApiToken(
          `${encode({ alg: "HS256" })}.${encode({ sub: "another-owner", org, scopes: [] })}.changed`,
        );
      });
      await waitFor(() =>
        expect(
          screen.queryByRole("textbox", { name: "Edited replacement" }),
        ).toBeNull(),
      );
      const current = { ...parent, id: "new-owner-parent" };
      await act(async () => {
        rerender(
          view,
          <ComposerOwner scope="ws-source-1">
            {editor(content, current)}
          </ComposerOwner>,
        );
      });
      const next = screen.getByTestId(
        "spec-suggestion-card-new-owner-parent:owner-stable-edit",
      );
      fireEvent.click(within(next).getByRole("button", { name: "Edit" }));
      expect(
        within(next).getByRole("textbox", { name: "Edited replacement" }),
      ).toHaveValue("Beta");
    } finally {
      setApiToken(token);
    }
  });

  it("renders the screenshot as two bubbles and submits only the selected replacement", async () => {
    const content = "new line from reviewer.\n";
    const onRewrite = vi.fn();
    const parent = review({
      id: "published-parent",
      beforeLines: content,
      afterLines:
        "suggest to add this line from reviewer.\n\n> one more thing....\n",
      contentHash: await anchors.sha256Utf8(content),
      edits: [
        reviewEditFixture(
          "replace-phrase",
          content,
          0,
          3,
          "suggest to add this",
        ),
        reviewEditFixture(
          "add-block",
          content,
          content.length,
          content.length,
          "\n> one more thing....\n",
        ),
      ],
    });
    const received: unknown[] = [];
    server.use(
      http.post(
        "/api/v1/workspaces/ws-source-1/review/suggestions/published-parent/accept",
        async ({ request }) => {
          const body = await request.json();
          received.push(body);
          return HttpResponse.json({
            data: {
              suggestion: {
                ...parent,
                edits: parent.edits!.map((edit) =>
                  edit.id === "replace-phrase"
                    ? {
                        ...edit,
                        status: "accepted",
                        acceptedAfterText: "owner phrase",
                        statusAt: "2026-10-01T16:10:00Z",
                      }
                    : edit,
                ),
              },
              writtenHash: "next",
              alreadyApplied: false,
            },
          });
        },
      ),
    );
    renderApp(editor(content, parent, { onRewrite }));
    const first = await screen.findByTestId(
      "spec-suggestion-card-published-parent:replace-phrase",
    );
    const second = screen.getByTestId(
      "spec-suggestion-card-published-parent:add-block",
    );
    expect(first).toHaveTextContent("Replace: new suggest to add this");
    expect(second).toHaveTextContent("Add block: one more thing....");
    expect(screen.queryByText(/Accept or reject them together/)).toBeNull();
    const preview = screen.getByTestId(
      "spec-suggestion-preview-published-parent:replace-phrase",
    );
    expect(preview).toHaveTextContent(
      "newsuggest to add this line from reviewer.",
    );
    expect(preview).toHaveTextContent("one more thing....");
    const removal = within(preview).getByRole("button", {
      name: "Suggested removal: new",
    });
    const addition = within(preview).getByRole("button", {
      name: "Suggested addition: suggest to add this",
    });
    expect(removal.getAttribute("aria-controls")).toBe(
      addition.getAttribute("aria-controls"),
    );
    fireEvent.keyDown(removal, { key: "Enter" });
    expect(
      screen.getByTestId("spec-review-edit-published-parent:replace-phrase"),
    ).toHaveFocus();
    fireEvent.click(within(first).getByRole("button", { name: "Edit" }));
    const editBox = within(first).getByRole("textbox", {
      name: "Edited replacement",
    });
    expect(editBox).toHaveValue("suggest to add this");
    fireEvent.change(editBox, { target: { value: "owner phrase" } });
    fireEvent.click(
      within(first).getByRole("button", { name: "Accept edited" }),
    );
    await waitFor(() =>
      expect(received).toEqual([
        { editId: "replace-phrase", afterLines: "owner phrase" },
      ]),
    );
    expect(
      within(second).getByRole("button", { name: "Accept" }),
    ).toBeEnabled();
    expect(onRewrite).not.toHaveBeenCalled();
  });

  it("retains the open same-paragraph sibling and its canonical source quote after refresh", async () => {
    const original = "Alpha unchanged middle clause Omega.\n";
    const current = "Beta unchanged middle clause Omega.\n";
    const omega = original.indexOf("Omega"),
      currentOmega = current.indexOf("Omega");
    const parent = review({
      id: "parent",
      beforeLines: original,
      afterLines: "Beta unchanged middle clause Gamma.\n",
      contentHash: await anchors.sha256Utf8(original),
      edits: [
        reviewEditFixture("alpha", original, 0, 5, "Beta"),
        reviewEditFixture("omega", original, omega, omega + 5, "Gamma"),
      ],
    });
    const view = renderApp(editor(original, parent));
    await screen.findByTestId("spec-suggestion-card-parent:alpha");
    expect(
      screen.getByTestId("spec-suggestion-card-parent:omega"),
    ).toBeInTheDocument();
    const updated = {
      ...parent,
      edits: [
        {
          ...parent.edits![0],
          status: "accepted",
          acceptedAfterText: "Beta",
          statusAt: "2026-10-01T16:10:00Z",
        },
        {
          ...parent.edits![1],
          currentStart: currentOmega,
          currentEnd: currentOmega + 5,
          expectedBeforeLines: current,
          expectedAfterLines: current.replace("Omega", "Gamma"),
        },
      ],
    };
    rerender(view, editor(current, updated));
    expect(
      screen.queryByTestId("spec-suggestion-card-parent:alpha"),
    ).toBeNull();
    const remaining = screen.getByTestId("spec-suggestion-card-parent:omega");
    expect(remaining).not.toHaveTextContent("Needs attention");
    expect(
      within(remaining).getByRole("button", { name: "Accept" }),
    ).toBeEnabled();
  });

  it("scopes empty child discussions independently and preserves original parent context", async () => {
    const content = "Alpha unchanged middle clause Omega.\n";
    const parent = review({
      id: "parent-thread",
      beforeLines: content,
      afterLines: content.replace("Alpha", "Beta"),
      commentId: "old-parent-comment",
      edits: [reviewEditFixture("edit-thread", content, 0, 5, "Beta")],
    });
    const reads: Array<string | null> = [],
      sent: unknown[] = [];
    server.use(
      http.get(
        "/api/v1/workspaces/ws-source-1/review/suggestions/parent-thread/replies",
        ({ request }) => {
          reads.push(new URL(request.url).searchParams.get("editId"));
          return HttpResponse.json({ data: { replies: [] } });
        },
      ),
      http.post(
        "/api/v1/workspaces/ws-source-1/review/suggestions/parent-thread/replies",
        async ({ request }) => {
          sent.push(await request.json());
          return HttpResponse.json({ data: { replies: [] } });
        },
      ),
    );
    renderApp(editor(content, parent));
    const card = await screen.findByTestId(
      "spec-suggestion-card-parent-thread:edit-thread",
    );
    fireEvent.click(within(card).getByRole("button", { name: "Discussion" }));
    const reply = await within(card).findByRole("textbox", { name: "Reply" });
    fireEvent.change(reply, { target: { value: "This phrase only." } });
    fireEvent.click(within(card).getByRole("button", { name: "Reply" }));
    await waitFor(() =>
      expect(sent).toEqual([
        { content: "This phrase only.", editId: "edit-thread" },
      ]),
    );
    fireEvent.click(
      within(card).getByRole("button", { name: "Original review discussion" }),
    );
    await waitFor(() => expect(reads).toContain(null));
    expect(reads).toContain("edit-thread");
    expect(
      screen.getByTestId("review-suggestion-replies-parent-thread"),
    ).not.toContainElement(reply);
  });
});

describe("DocumentPreview review source identities", () => {
  it.each([false, true])(
    "retains canonical question controls when a spanning hunk changes the question: %s",
    async (changesQuestion) => {
      const content = `First.\n\n${question}\n\nLast.\n`;
      const proposedQuestion = changesQuestion
        ? question.replace("Which color should we use?", "Proposed question?")
        : question;
      const onDiscussQuestion = vi.fn();
      const onRewrite = vi.fn();
      renderApp(
        editor(
          content,
          review({
            beforeLines: content,
            afterLines: `First changed.\n\n${proposedQuestion}\n\nLast changed.\n`,
          }),
          {
            onRewrite,
            onDiscussQuestion,
          },
        ),
      );
      expect(
        screen.getByText("Which color should we use?"),
      ).toBeInTheDocument();
      expect(screen.getAllByTestId("question-answer-button")).toHaveLength(1);
      fireEvent.click(screen.getByTestId("question-discuss-button"));
      expect(onDiscussQuestion).toHaveBeenCalledWith(
        expect.objectContaining({ id: "canonical-question", status: "open" }),
        undefined,
      );
      expect(screen.getByTestId("question-answer-button")).toBeDisabled();
      expect(screen.queryByTestId("spec-suggestion-preview-review")).toBeNull();
      expect(
        screen.getByTestId("spec-suggestion-card-review"),
      ).toHaveTextContent("Needs attention");
      expect(onRewrite).not.toHaveBeenCalled();
    },
  );

  it("uses the selected raw digest to identify the original duplicate block", async () => {
    const content = "Repeated.\n\nRepeated.\n";
    renderApp(
      editor(
        content,
        review({
          beforeLines: "Repeated.\n",
          afterLines: "Changed.\n",
          anchorStart: 3,
          contentHash: await anchors.sha256Utf8(content),
        }),
      ),
    );
    const preview = await screen.findByTestId("spec-suggestion-preview-review");
    expect(preview.querySelector('[data-block-index="1"]')).toBeInTheDocument();
    expect(preview).toHaveTextContent("Changed.");
    expect(screen.getAllByText("Repeated.")).toHaveLength(1);
    expect(hash.selectedSource).toHaveBeenCalledWith(content);
  });

  it("does not trust a late digest after the selected node and content switch", async () => {
    const oldContent = "Repeated.\n\nRepeated.\n";
    const newContent = "Outside.\n\nRepeated.\n\nRepeated.\n";
    const pending: Array<{
      content: string;
      resolve: (digest: string) => void;
    }> = [];
    hash.selectedSource.mockImplementation(
      (content: string) =>
        new Promise<string>((resolve) => pending.push({ content, resolve })),
    );
    const suggestion = review({
      beforeLines: "Repeated.\n",
      afterLines: "Changed.\n",
      anchorStart: 3,
      contentHash: await anchors.sha256Utf8(oldContent),
    });
    const view = renderApp(editor(oldContent, suggestion));
    await waitFor(() => expect(pending).toHaveLength(1));
    rerender(
      view,
      editor(newContent, suggestion, { filePath: "docs/other.md" }),
    );
    await waitFor(() => expect(pending.at(-1)?.content).toBe(newContent));
    await act(async () =>
      pending[0].resolve(await anchors.sha256Utf8(oldContent)),
    );
    expect(screen.queryByTestId("spec-suggestion-preview-review")).toBeNull();
    expect(screen.getByTestId("spec-suggestion-card-review")).toHaveTextContent(
      "Needs attention",
    );
    await act(async () =>
      pending.at(-1)!.resolve(await anchors.sha256Utf8(newContent)),
    );
    expect(screen.queryByTestId("spec-suggestion-preview-review")).toBeNull();
  });

  it("keeps untrusted duplicate placement in attention if crypto fails", async () => {
    const content = "Repeated.\n\nRepeated.\n";
    hash.selectedSource.mockRejectedValue(new Error("Crypto unavailable"));
    renderApp(
      editor(
        content,
        review({
          beforeLines: "Repeated.\n",
          afterLines: "Changed.\n",
          anchorStart: 3,
          contentHash: await anchors.sha256Utf8(content),
        }),
      ),
    );
    await waitFor(() => expect(hash.selectedSource).toHaveBeenCalled());
    expect(screen.queryByTestId("spec-suggestion-preview-review")).toBeNull();
    expect(screen.getByTestId("spec-suggestion-card-review")).toHaveTextContent(
      "Needs attention",
    );
  });

  it("does not hash closed hunks", async () => {
    const content = "Repeated.\n\nRepeated.\n";
    const suggestion = review({
      beforeLines: "Repeated.\n",
      afterLines: "Changed.\n",
      anchorStart: 3,
      contentHash: await anchors.sha256Utf8(content),
    });
    renderApp(editor(content, { ...suggestion, status: "rejected" }));
    await act(async () => undefined);
    expect(hash.selectedSource).not.toHaveBeenCalled();
  });
});

describe("canonical link destination review boundaries", () => {
  const originalHref = "https://old.example/path";

  it("keeps visible link-label edits inline when the destination is unchanged", () => {
    const before = `Read [docs](${originalHref})\n`;
    renderApp(
      editor(
        before,
        review({
          beforeLines: before,
          afterLines: `Please read [guide](${originalHref})\n`,
        }),
      ),
    );
    const preview = screen.getByTestId("spec-suggestion-preview-review");
    expect(preview.querySelector("ins")).not.toBeNull();
    expect(
      [...preview.querySelectorAll("a")].every(
        (a) => a.getAttribute("href") === originalHref,
      ),
    ).toBe(true);
    expect(
      screen.getByTestId("spec-suggestion-card-review"),
    ).not.toHaveTextContent("Needs attention");
  });
});

it("keeps a changed destination on a retained link label in attention", async () => {
  const before =
    "Read A [B](https://one.example) and [A](https://two.example).\n";
  const after =
    "Read [A](https://one.example) [B](https://two.example) and B.\n";
  renderApp(editor(before, review({ beforeLines: before, afterLines: after })));
  expect(screen.queryByTestId("spec-suggestion-preview-review")).toBeNull();
  const card = screen.getByTestId("spec-suggestion-card-review");
  expect(card).toHaveTextContent("Needs attention");
  expect(card).toHaveTextContent(before.trim());
  expect(card).toHaveTextContent(after.trim());
  const canonical = document.querySelector<HTMLElement>(
    '[data-block-index="0"]',
  )!;
  expect(within(canonical).getByRole("link", { name: "B" })).toHaveAttribute(
    "href",
    "https://one.example",
  );
});

describe("overlapping suggestions in the document and adjacent cards", () => {
  const content = "Intro.\n\nhuman-provided\n\nEnd.\n";
  async function competing() {
    const at = content.indexOf("human");
    const contentHash = await anchors.sha256Utf8(content);
    return [
      review({
        id: "whole",
        authorLabel: "zcl",
        beforeLines: content,
        contentHash,
        edits: [
          reviewEditFixture(
            "whole-edit",
            content,
            at,
            at + 14,
            " an inplace edit with leading space",
          ),
        ],
      }),
      review({
        id: "part",
        authorLabel: "kaihui-zheng",
        beforeLines: content,
        contentHash,
        edits: [reviewEditFixture("part-edit", content, at, at + 5, "kaihui")],
      }),
    ];
  }
  function withProposals(source: string, suggestions: ReviewSuggestion[]) {
    return (
      <ComposerOwner scope="overlap-review">
        {editor(source, suggestions[0], {
          suggestionOverlay: {
            workspaceId: "ws-source-1",
            suggestions,
            viewerOwnsWorkspace: true,
            viewerUserId: "owner",
            ownerVmRunning: true,
            sourceWritable: true,
          },
        })}
      </ComposerOwner>
    );
  }
  it.each(["durable", "legacy"])(
    "keeps the preview unchanged when a %s summary receives focus",
    async (kind) => {
      const proposals = await competing();
      if (kind === "legacy") {
        for (const proposal of proposals) {
          const edit = proposal.edits![0];
          proposal.afterLines = content.replace(
            edit.beforeText,
            edit.afterText,
          );
          proposal.edits = undefined;
        }
      }
      renderApp(withProposals(content, proposals));
      const cards = await screen.findAllByTestId(/^spec-suggestion-card-/);
      const [whole, part] = cards;
      const preview = screen.getByTestId(/^spec-suggestion-preview-/);
      const originalPreview = preview.textContent;
      const summary = within(part).getAllByTestId(/^spec-review-edit-/)[0];
      fireEvent.focus(summary);
      expect(whole).toHaveAttribute("data-selected", "true");
      expect(part).toHaveAttribute("data-selected", "false");
      expect(preview.textContent).toBe(originalPreview);
      // Explicit activation still selects the alternative and highlights it.
      fireEvent.click(summary);
      expect(part).toHaveAttribute("data-selected", "true");
      expect(preview).toHaveTextContent("kaihui");
      expect(preview).not.toHaveTextContent("inplace");
      const selectedPreview = preview.textContent;
      fireEvent.focus(within(whole).getAllByTestId(/^spec-review-edit-/)[0]);
      expect(part).toHaveAttribute("data-selected", "true");
      expect(preview.textContent).toBe(selectedPreview);
      fireEvent.click(
        within(whole).getByRole("radio", { name: "Preview in document" }),
      );
      expect(preview.textContent).toBe(originalPreview);
    },
  );
  it("keeps both cards beside one selected preview and preserves literal drafts and decision identity", async () => {
    const proposals = await competing();
    const requests: unknown[] = [];
    server.use(
      http.post(
        "*/api/v1/workspaces/ws-source-1/review/suggestions/whole/accept",
        async ({ request }) => {
          requests.push(await request.json());
          return HttpResponse.json(
            { error: { code: "CONFLICT", message: "Source changed; retry." } },
            { status: 409 },
          );
        },
      ),
    );
    renderApp(withProposals(content, proposals));
    const whole = await screen.findByTestId(
      "spec-suggestion-card-whole:whole-edit",
    );
    const part = screen.getByTestId("spec-suggestion-card-part:part-edit");
    const preview = screen.getByTestId(
      "spec-suggestion-preview-whole:whole-edit",
    );
    expectBesideDocument(whole, preview);
    expectBesideDocument(part, preview);
    // One conflict, one box, naming what is previewed.
    const box = whole.closest("[data-testid^='spec-review-conflict-']")!;
    expect(box).toContainElement(part);
    expect(box).toHaveTextContent("2 suggestions for the same text");
    expect(box).toHaveTextContent("Previewing zcl.");
    expect(preview).toHaveTextContent(
      "human-provided·an·inplace·edit·with·leading·space",
    );
    expect(preview).not.toHaveTextContent("kaihui");
    fireEvent.click(
      within(part).getByRole("radio", { name: "Preview in document" }),
    );
    expect(preview).toHaveTextContent("humankaihui-provided");
    expect(preview).not.toHaveTextContent("inplace");
    expect(screen.getAllByTestId(/spec-suggestion-preview-/)).toHaveLength(1);
    fireEvent.click(within(whole).getByRole("button", { name: "Edit" }));
    fireEvent.change(
      within(whole).getByRole("textbox", { name: "Edited replacement" }),
      { target: { value: "  owner replacement" } },
    );
    fireEvent.click(
      within(whole).getByRole("radio", { name: "Preview in document" }),
    );
    fireEvent.click(
      within(part).getByRole("radio", { name: "Preview in document" }),
    );
    expect(
      within(whole).getByRole("textbox", { name: "Edited replacement" }),
    ).toHaveValue("  owner replacement");
    fireEvent.click(
      within(whole).getByRole("button", { name: "Accept edited" }),
    );
    await waitFor(() =>
      expect(requests).toEqual([
        { editId: "whole-edit", afterLines: "  owner replacement" },
      ]),
    );
    expect(
      await within(whole).findByText("Source changed; retry."),
    ).toBeInTheDocument();
    expect(
      within(whole).getByRole("textbox", { name: "Edited replacement" }),
    ).toHaveValue("  owner replacement");
    expect(within(part).getByRole("button", { name: "Accept" })).toBeEnabled();
  });

  it.each([0, 1])(
    "rechecks the other proposal when author %s is accepted and source refreshes",
    async (accepted) => {
      server.use(
        http.get(
          "*/api/v1/workspaces/ws-source-1/review/suggestions/:id/replies",
          () => HttpResponse.json({ data: { replies: [] } }),
        ),
      );
      const proposals = await competing();
      const view = renderApp(withProposals(content, proposals));
      await screen.findAllByRole("radio", { name: "Preview in document" });
      const peerId = accepted === 0 ? "part:part-edit" : "whole:whole-edit";
      const peer = screen.getByTestId(`spec-suggestion-card-${peerId}`);
      fireEvent.click(within(peer).getByRole("button", { name: "Edit" }));
      fireEvent.change(
        within(peer).getByRole("textbox", { name: "Edited replacement" }),
        { target: { value: " Retained peer draft" } },
      );
      fireEvent.click(within(peer).getByRole("button", { name: "Discussion" }));
      fireEvent.change(
        await within(peer).findByRole("textbox", { name: "Reply" }),
        { target: { value: "Keep this discussion draft." } },
      );
      const current =
        accepted === 0
          ? content.replace(
              "human-provided",
              " an inplace edit with leading space",
            )
          : content.replace("human", "kaihui");
      const updated = proposals.map((p, i) =>
        i === accepted
          ? {
              ...p,
              status: "accepted",
              edits: p.edits!.map((e) => ({ ...e, status: "accepted" })),
            }
          : p,
      );
      rerender(view, withProposals(current, updated));
      const remaining = await screen.findByTestId(
        `spec-suggestion-card-${accepted === 0 ? "part:part-edit" : "whole:whole-edit"}`,
      );
      await waitFor(() =>
        expect(remaining).toHaveTextContent(/changed|attention/),
      );
      expect(screen.queryByTestId("spec-review-source-diff")).toBeNull();
      expect(remaining).toHaveTextContent("Open");
      expect(
        within(remaining).getByRole("textbox", { name: "Edited replacement" }),
      ).toHaveValue(" Retained peer draft");
      if (!within(remaining).queryByRole("textbox", { name: "Reply" }))
        fireEvent.click(
          within(remaining).getByRole("button", { name: "Discussion" }),
        );
      expect(
        await within(remaining).findByRole("textbox", { name: "Reply" }),
      ).toHaveValue("Keep this discussion draft.");
      expect(
        within(remaining).getByRole("button", { name: "Reject" }),
      ).toBeEnabled();
    },
  );
});

describe("arbitrary author alternatives", () => {
  const source = "Alpha.\n\nBravo.\n\nCharlie.\n\nDelta.\n";
  function view(proposals: ReviewSuggestion[]) {
    return (
      <ComposerOwner scope="many-authors">
        {editor(source, proposals[0], {
          suggestionOverlay: {
            workspaceId: "ws-source-1",
            suggestions: proposals,
            viewerOwnsWorkspace: true,
            viewerUserId: "owner",
            ownerVmRunning: true,
            sourceWritable: true,
          },
        })}
      </ComposerOwner>
    );
  }
  it.each([3, 12])(
    "keeps all %s cards mounted across selection, arrival order, and removal",
    async (count) => {
      const proposals = Array.from({ length: count }, (_, i) =>
        review({
          id: `author-${i}`,
          authorLabel: `Person ${i}`,
          beforeLines: source,
          edits: [reviewEditFixture(`edit-${i}`, source, 0, 5, `Person ${i}`)],
        }),
      );
      const mounted = renderApp(view(proposals));
      await screen.findByTestId("spec-suggestion-card-author-0:edit-0");
      expect(screen.getAllByTestId(/^spec-suggestion-card-/)).toHaveLength(
        count,
      );
      expect(screen.queryByText("Previewing in document")).toBeNull();
      expect(screen.queryByText("Original proposal context")).toBeNull();
      for (let i = count - 1; i >= 0; i--) {
        const card = screen.getByTestId(
          `spec-suggestion-card-author-${i}:edit-${i}`,
        );
        fireEvent.click(
          within(card).getByRole("radio", { name: "Preview in document" }),
        );
        expect(card).toHaveAttribute("data-selected", "true");
        // The chosen alternative's radio is the only one checked.
        expect(
          within(card).getByRole("radio", { name: "Preview in document" }),
        ).toBeChecked();
        expect(
          screen
            .getAllByRole("radio", { name: "Preview in document" })
            .filter((radio) => (radio as HTMLInputElement).checked),
        ).toHaveLength(1);
        expect(screen.getAllByTestId(/^spec-suggestion-preview-/)).toHaveLength(
          1,
        );
        expect(
          [...document.querySelectorAll("ins")]
            .map((node) => node.textContent)
            .join(""),
        ).toBe(`Person ${i}`);
        expect(
          document.querySelectorAll('[data-block-index="0"]'),
        ).toHaveLength(1);
      }
      const first = screen.getByTestId("spec-suggestion-card-author-0:edit-0");
      fireEvent.click(within(first).getByRole("button", { name: "Edit" }));
      fireEvent.change(
        within(first).getByRole("textbox", { name: "Edited replacement" }),
        { target: { value: "Persistent draft" } },
      );
      rerender(mounted, view([...proposals].reverse()));
      expect(screen.getByTestId("spec-suggestion-card-author-0:edit-0")).toBe(
        first,
      );
      expect(first).toHaveAttribute("data-selected", "true");
      expect(
        within(first).getByRole("textbox", { name: "Edited replacement" }),
      ).toHaveValue("Persistent draft");
      rerender(mounted, view(proposals.slice(1)));
      expect(screen.getAllByTestId(/^spec-suggestion-card-/)).toHaveLength(
        count - 1,
      );
      expect(
        screen.getByTestId("spec-suggestion-card-author-1:edit-1"),
      ).toHaveAttribute("data-selected", "true");
    },
  );
});

describe("compatible edits alongside alternatives", () => {
  it.each([false, true])(
    "keeps every card in a compatible transitive display group (source diff=%s)",
    async (sourceDiff) => {
      const source = "Alpha left.\n\nMiddle right.\n";
      let finishDigest!: (digest: string) => void;
      hash.selectedSource.mockReturnValueOnce(
        new Promise<string>((resolve) => {
          finishDigest = resolve;
        }),
      );
      const specifications = [
        ["first", "Alpha", sourceDiff ? " North" : "North"],
        ["bridge", "left.\n\nMiddle", "west.\n\nCenter"],
        ["last", "right", "East"],
      ];
      const proposals = specifications.map(([id, before, after]) =>
        review({
          id,
          beforeLines: source,
          edits: [
            reviewEditFixture(
              `${id}-edit`,
              source,
              source.indexOf(before),
              source.indexOf(before) + before.length,
              after,
            ),
          ],
        }),
      );
      const renderProposals = (suggestions: ReviewSuggestion[]) => (
        <RequireAuth>
          <ComposerOwner scope="compatible-chain">
            {editor(source, suggestions[0], {
              suggestionOverlay: {
                workspaceId: "ws-source-1",
                suggestions,
                viewerOwnsWorkspace: true,
                viewerUserId: "owner",
                ownerVmRunning: true,
                sourceWritable: true,
              },
            })}
          </ComposerOwner>
        </RequireAuth>
      );
      const mounted = renderApp(renderProposals(proposals));
      await screen.findByTestId(/^spec-suggestion-preview-/);
      expect(screen.getAllByTestId(/^spec-suggestion-card-/)).toHaveLength(3);
      const last = screen.getByTestId("spec-suggestion-card-last:last-edit");
      fireEvent.click(within(last).getByRole("button", { name: /^Edit$/ }));
      fireEvent.change(
        await screen.findByTestId("review-suggestion-edit-last:last-edit"),
        {
          target: { value: "Retained last draft" },
        },
      );
      // Source verification may rebuild the preview after editing starts.
      // Control that transition instead of racing crypto with a cached DOM card.
      await act(async () => {
        finishDigest(await anchors.sha256Utf8(source));
      });
      expect(
        screen.getByTestId("review-suggestion-edit-last:last-edit"),
      ).toHaveValue("Retained last draft");
      for (const order of [
        proposals,
        [...proposals].reverse(),
        [proposals[1], proposals[2], proposals[0]],
      ]) {
        rerender(mounted, renderProposals(order));
        expect(screen.getAllByTestId(/^spec-suggestion-card-/)).toHaveLength(3);
        const previews = screen.getAllByTestId(/^spec-suggestion-preview-/);
        expect(previews).toHaveLength(1);
        expect(
          within(previews[0]).queryByTestId("spec-review-source-diff") !== null,
        ).toBe(sourceDiff);
        if (!sourceDiff)
          for (const index of [0, 1])
            expect(
              document.querySelectorAll(`[data-block-index="${index}"]`),
            ).toHaveLength(1);
        const additions = [...previews[0].querySelectorAll("ins")]
          .map((n) => n.textContent)
          .join("");
        expect(additions).toContain("North");
        expect(additions).toContain("west");
        expect(additions).toContain("Center");
        expect(additions).toContain("East");
        expect(
          screen.queryByRole("radio", { name: "Preview in document" }),
        ).toBeNull();
        expect(
          within(
            screen.getByTestId("spec-suggestion-card-last:last-edit"),
          ).getByRole("textbox", { name: "Edited replacement" }),
        ).toHaveValue("Retained last draft");
      }
      // Removing the bridge separates the two locations without losing either card.
      rerender(mounted, renderProposals([proposals[0], proposals[2]]));
      expect(screen.getAllByTestId(/^spec-suggestion-card-/)).toHaveLength(2);
      expect(screen.getAllByTestId(/^spec-suggestion-preview-/)).toHaveLength(
        2,
      );
      expect(
        within(
          screen.getByTestId("spec-suggestion-card-last:last-edit"),
        ).getByRole("textbox", { name: "Edited replacement" }),
      ).toHaveValue("Retained last draft");
    },
  );
  it.each([false, true])(
    "keeps disjoint edits and selections in independent conflict sets together (source diff=%s)",
    async (sourceDiff) => {
      const source = "Alpha middle Omega ending Final.\n";
      const specifications = [
        ["a1", "Alpha", sourceDiff ? " North" : "North"],
        ["a2", "Alpha", sourceDiff ? " South" : "South"],
        ["b1", "Omega", "East"],
        ["b2", "Omega", "West"],
        ["solo", "Final", "Done"],
      ];
      const proposals = specifications.map(([id, word, replacement]) =>
        review({
          id,
          authorLabel: id,
          beforeLines: source,
          edits: [
            reviewEditFixture(
              `${id}-edit`,
              source,
              source.indexOf(word),
              source.indexOf(word) + word.length,
              replacement,
            ),
          ],
        }),
      );
      const renderProposals = (suggestions: ReviewSuggestion[]) => (
        <ComposerOwner scope="mixed-conflicts">
          {editor(source, suggestions[0], {
            suggestionOverlay: {
              workspaceId: "ws-source-1",
              suggestions,
              viewerOwnsWorkspace: true,
              viewerUserId: "owner",
              ownerVmRunning: true,
              sourceWritable: true,
            },
          })}
        </ComposerOwner>
      );
      const view = renderApp(renderProposals(proposals));
      await screen.findByTestId("spec-suggestion-card-a1:a1-edit");
      const additions = () => {
        const previews = screen.getAllByTestId(/^spec-suggestion-preview-/);
        expect(previews).toHaveLength(1);
        return [...previews[0].querySelectorAll("ins")].map(
          (node) => node.textContent,
        );
      };
      expect(additions()).toEqual([
        sourceDiff ? "·North" : "North",
        "East",
        "Done",
      ]);
      // Two conflict sets, two alternatives each: one box per set.
      expect(
        screen.getAllByRole("radio", { name: "Preview in document" }),
      ).toHaveLength(4);
      expect(screen.getAllByTestId(/^spec-review-conflict-/)).toHaveLength(2);
      const choose = (id: string) =>
        fireEvent.click(
          within(
            screen.getByTestId(`spec-suggestion-card-${id}:${id}-edit`),
          ).getByRole("radio", { name: "Preview in document" }),
        );
      choose("a2");
      expect(additions()).toEqual([
        sourceDiff ? "·South" : "South",
        "East",
        "Done",
      ]);
      choose("b2");
      expect(additions()).toEqual([
        sourceDiff ? "·South" : "South",
        "West",
        "Done",
      ]);
      const solo = screen.getByTestId("spec-suggestion-card-solo:solo-edit");
      expect(
        within(solo).queryByRole("radio", { name: "Preview in document" }),
      ).toBeNull();
      fireEvent.focus(screen.getByTestId("spec-review-edit-solo:solo-edit"));
      expect(additions()).toEqual([
        sourceDiff ? "·South" : "South",
        "West",
        "Done",
      ]);
      rerender(view, renderProposals([...proposals].reverse()));
      expect(additions()).toEqual([
        sourceDiff ? "·South" : "South",
        "West",
        "Done",
      ]);
      expect(screen.getAllByTestId(/^spec-suggestion-card-/)).toHaveLength(5);
      // An arriving bridge merges the two conflicts. Both previously selected
      // IDs are remembered, but selecting either must now exclude the other.
      const bridge = review({
        id: "bridge",
        beforeLines: source,
        edits: [
          reviewEditFixture(
            "bridge-edit",
            source,
            0,
            source.indexOf("Omega") + 5,
            "Combined",
          ),
        ],
      });
      rerender(view, renderProposals([...proposals].reverse().concat(bridge)));
      expect(additions()).toEqual(["West", "Done"]);
      choose("a2");
      expect(additions()).toEqual([sourceDiff ? "·South" : "South", "Done"]);
      rerender(view, renderProposals([...proposals].reverse()));
      expect(additions()).toEqual([
        sourceDiff ? "·South" : "South",
        "West",
        "Done",
      ]);
      if (!sourceDiff)
        expect(
          document.querySelectorAll('[data-block-index="0"]'),
        ).toHaveLength(1);
    },
  );
});

describe("compatible insertion at an alternative boundary", () => {
  it.each([false, true])(
    "renders a leading insertion once regardless of publication order (reversed=%s)",
    async (reverse) => {
      const source = "human-provided\n";
      const proposals = [
        review({
          id: "north",
          beforeLines: source,
          edits: [reviewEditFixture("north-edit", source, 0, 5, "north")],
        }),
        review({
          id: "south",
          beforeLines: source,
          edits: [reviewEditFixture("south-edit", source, 0, 5, "south")],
        }),
        review({
          id: "space",
          beforeLines: source,
          edits: [reviewEditFixture("space-edit", source, 0, 0, " ")],
        }),
      ];
      if (reverse) proposals.reverse();
      renderApp(
        <ComposerOwner scope="boundary-alternatives">
          {editor(source, proposals[0], {
            suggestionOverlay: {
              workspaceId: "ws-source-1",
              suggestions: proposals,
              viewerOwnsWorkspace: true,
              viewerUserId: "owner",
              ownerVmRunning: true,
              sourceWritable: true,
            },
          })}
        </ComposerOwner>,
      );
      await screen.findByTestId("spec-suggestion-card-space:space-edit");
      const preview = screen
        .getByTestId("spec-review-source-diff")
        .querySelector("pre")!;
      expect(
        [...preview.querySelectorAll("ins")].map((node) => node.textContent),
      ).toEqual(["·", reverse ? "south" : "north"]);
      // Read proposed source by excluding deletions, preserving canonical suffix once.
      const proposedSource = () =>
        [...preview.childNodes]
          .filter(
            (node) => !(node instanceof HTMLElement && node.tagName === "DEL"),
          )
          .map((node) => node.textContent)
          .join("");
      expect(proposedSource()).toBe(`·${reverse ? "south" : "north"}-provided`);
      const other = reverse ? "north" : "south";
      fireEvent.click(
        within(
          screen.getByTestId(`spec-suggestion-card-${other}:${other}-edit`),
        ).getByRole("radio", { name: "Preview in document" }),
      );
      expect(
        [...preview.querySelectorAll("ins")].map((node) => node.textContent),
      ).toEqual(["·", other]);
      expect(proposedSource()).toBe(`·${other}-provided`);
    },
  );
});

it("keeps ordered-list numbering and start in a review preview", async () => {
  const before = "5. First item.\n6. Second item.\n";
  renderApp(
    editor(
      before,
      review({
        beforeLines: before,
        afterLines: "5. Updated first item.\n6. Second item.\n",
      }),
    ),
  );
  const preview = await screen.findByTestId("spec-suggestion-preview-review");
  const lists = preview.querySelectorAll("ol");
  expect(lists).toHaveLength(2);
  for (const list of lists) {
    expect(list).toHaveAttribute("start", "5");
    expect(list.querySelectorAll("li")).toHaveLength(2);
  }
  expect(lists[0]).toHaveTextContent("First item.");
  expect(lists[1]).toHaveTextContent("Updated first item.");
  expect(preview.querySelector("ul")).toBeNull();
});
