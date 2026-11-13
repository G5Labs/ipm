/**
 * ReviewSuggestionOverlay — suggestions shown in place while reading a file:
 * the read-only rail with jump links, Monaco view zones filled through
 * portals, and per-block cards for documents.
 */

import React from "react";
import {
  act,
  fireEvent,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { HttpResponse, http } from "msw";
import { describe, expect, it, vi } from "vitest";

import { renderApp } from "../../test/render";
import {
  ComposerStoreContext,
  createComposerStore,
} from "../../hooks/useComposerStore";
import { server } from "../../test/msw/server";
import type { ReviewSuggestion } from "../../lib/types";
import { reviewEditFixture } from "../../test/reviewEdits";
import { expandReviewSuggestionEdits } from "../../lib/reviewSuggestionEdits";
import { sha256Utf8 } from "../../lib/commentAnchors";
import { CodeEditor } from "../editor/CodeEditor";
import {
  DocumentPreview,
  type DocumentPreviewProps,
} from "../files/DocumentPreview";
import {
  BlockSuggestions,
  FallbackSuggestionList,
  MonacoSuggestionZones,
  splitHighlightedLines,
  suggestionZonesByLine,
  suggestionZoneHeight,
  suggestionsByLine,
  resolveDisplayAnchor,
  suggestionZoneAfterLine,
  useFileSuggestions,
  type SuggestionOverlayProps,
} from "./ReviewSuggestionOverlay";

// These source cases exercise the read-only fallback. Native Monaco zones
// are covered below with an editor host and in the local browser journeys.
vi.mock("@monaco-editor/react", () => {
  throw new Error("Exercise the source fallback without a browser runtime");
});

const SOURCE_ID = "ws-source-1";

function suggestion(overrides: Partial<ReviewSuggestion>): ReviewSuggestion {
  return {
    id: "sug-1",
    publicationId: "pub-1",
    findingId: null,
    repoId: "repo-api",
    path: "src/main.rs",
    kind: "hunk",
    beforeLines: "return 1;\n",
    afterLines: "return 0;\n",
    anchorStart: 3,
    contentHash: "abc",
    anchorContext: null,
    authorKind: "human",
    authorLabel: "alex",
    note: "",
    commentId: "c-1",
    status: "open",
    statusBy: null,
    statusAt: null,
    supersedes: null,
    adoptedFrom: null,
    createdAt: "2026-09-23T14:31:00.000Z",
    ...overrides,
  };
}

function overlay(
  suggestions: ReviewSuggestion[],
  extra: Partial<SuggestionOverlayProps> = {},
): SuggestionOverlayProps {
  return {
    workspaceId: SOURCE_ID,
    suggestions,
    viewerOwnsWorkspace: true,
    viewerUserId: "user-owner",
    ownerVmRunning: true,
    sourceWritable: true,
    ...extra,
  };
}

function documentEditor(
  content: string,
  suggestions: ReviewSuggestion[],
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
      suggestionOverlay={overlay(suggestions)}
      {...extra}
    />
  );
}

describe("ReviewSuggestionOverlay", () => {
  it("relayouts the same Monaco card when exact restored-source proof arrives, and refuses it for another buffer", async () => {
    let value = "prefix\nold text.\n";
    const original = value;
    const [child] = expandReviewSuggestionEdits([
      suggestion({
        id: "restored-monaco",
        anchorStart: 1,
        beforeLines: original,
        expectedBeforeLines: "historical\nextra\nold text.\n",
        contentHash: await sha256Utf8(original),
        edits: [
          reviewEditFixture("edit", original, 7, 10, "new", {
            sourceConflict: true,
            currentStart: 17,
            currentEnd: 20,
          }),
        ],
      }),
    ]);
    const zones: Array<{ afterLineNumber: number; domNode: HTMLElement }> = [];
    const editor = {
      getModel: () => ({ getValue: () => value }),
      changeViewZones: (
        callback: (accessor: {
          addZone: (zone: {
            afterLineNumber: number;
            domNode: HTMLElement;
          }) => string;
          removeZone: (id: string) => void;
          layoutZone: (id: string) => void;
        }) => void,
      ) =>
        callback({
          addZone: (zone) => {
            zones.push(zone);
            document.body.appendChild(zone.domNode);
            return "stable-zone";
          },
          removeZone: () => {
            zones[0]?.domNode.remove();
            zones.splice(0);
          },
          layoutZone: () => undefined,
        }),
    };
    let select: (
      proof: SuggestionOverlayProps["selectedSource"],
    ) => void = () => undefined;
    const Host = () => {
      const [proof, setProof] =
        React.useState<SuggestionOverlayProps["selectedSource"]>();
      React.useEffect(() => {
        select = setProof;
      }, []);
      return (
        <MonacoSuggestionZones
          editor={editor}
          mountVersion={1}
          overlay={overlay([child], { selectedSource: proof })}
        />
      );
    };
    renderApp(<Host />);
    const card = await screen.findByTestId(
      "review-suggestion-restored-monaco:edit",
    );
    expect(zones[0].afterLineNumber).toBe(0);
    act(() => select({ content: original, digest: child.contentHash! }));
    await waitFor(() => expect(zones[0].afterLineNumber).toBe(2));
    expect(screen.getByTestId("review-suggestion-restored-monaco:edit")).toBe(
      card,
    );
    expect(zones).toHaveLength(1);
    expect(
      screen.queryByTestId("suggestion-anchor-note-restored-monaco:edit"),
    ).toBeNull();
    value = "Different.\n";
    act(() => select({ content: original, digest: child.contentHash! }));
    await waitFor(() => expect(zones[0].afterLineNumber).toBe(0));
    expect(
      screen.getByTestId("suggestion-anchor-note-restored-monaco:edit"),
    ).toHaveTextContent("source needs attention");
  });

  it.each(["FILES", "raw document"])(
    "binds the restored %s buffer digest before clearing historical-context attention",
    async (surface) => {
      const original = "prefix\nold text.\n";
      const historical = "é🦀\nmore\nold text.\n";
      const at = original.indexOf("old");
      const currentAt = historical.indexOf("old");
      const bytes = (value: string) => new TextEncoder().encode(value).length;
      const parent = suggestion({
        id: "restored-display",
        anchorStart: 1,
        beforeLines: original,
        expectedBeforeLines: historical,
        contentHash: await sha256Utf8(original),
        edits: [
          reviewEditFixture("restored", original, at, at + 3, "new", {
            sourceConflict: true,
            currentStart: bytes(historical.slice(0, currentAt)),
            currentEnd: bytes(historical.slice(0, currentAt + 3)),
          }),
        ],
      });
      const cards = expandReviewSuggestionEdits([parent]);
      renderApp(
        surface === "FILES" || surface === "document source" ? (
          <CodeEditor
            filePath="docs/spec.md"
            content={original}
            // The independent raw proof must not use this version-precondition hash.
            contentHash="opaque-file-version"
            language="markdown"
            editable={false}
            showComments={false}
            comments={[]}
            suggestionOverlay={overlay(cards)}
          />
        ) : (
          documentEditor(original, cards, {})
        ),
      );
      const card = screen.getByTestId(
        "review-suggestion-restored-display:restored",
      );
      await waitFor(() =>
        expect(
          screen.queryByTestId(
            "suggestion-anchor-note-restored-display:restored",
          ),
        ).toBeNull(),
      );
      expect(card).toHaveTextContent("Open");
      expect(card).toHaveTextContent("old");
      expect(card).toHaveTextContent("new");
      expect(cards[0].reviewEdit?.sourceConflict).toBe(true);
    },
  );

  it("keeps a digest bound to a different raw buffer from clearing source-conflict attention", async () => {
    const original = "Old.\n";
    const [child] = expandReviewSuggestionEdits([
      suggestion({
        id: "digest-mismatch",
        beforeLines: original,
        contentHash: await sha256Utf8(original),
        edits: [
          reviewEditFixture("edit", original, 0, 3, "New", {
            sourceConflict: true,
          }),
        ],
      }),
    ]);
    renderApp(
      <FallbackSuggestionList
        overlay={overlay([child], {
          selectedSource: {
            content: "Different.\n",
            digest: child.contentHash!,
          },
        })}
        content={original}
      />,
    );
    expect(
      screen.getByTestId("suggestion-anchor-note-digest-mismatch:edit"),
    ).toHaveTextContent("source needs attention");
  });

  it("places restored raw and Monaco child coordinates only with an exact content-bound captured digest", async () => {
    const original = "prefix\nold text.\n";
    const historical = "é🦀\nmore\nold text.\n";
    const at = original.indexOf("old");
    const historicalAt = historical.indexOf("old");
    const bytes = (value: string) => new TextEncoder().encode(value).length;
    const parent = suggestion({
      id: "restored-parent",
      anchorStart: 1,
      beforeLines: original,
      expectedBeforeLines: historical,
      contentHash: await sha256Utf8(original),
      edits: [
        reviewEditFixture("restored", original, at, at + 3, "new", {
          sourceConflict: true,
          currentStart: bytes(historical.slice(0, historicalAt)),
          currentEnd: bytes(historical.slice(0, historicalAt + 3)),
        }),
      ],
    });
    const [child] = expandReviewSuggestionEdits([parent]);
    const anchor = resolveDisplayAnchor(child, original, parent.contentHash!);
    expect(anchor.state).toBe("anchored");
    expect(anchor.line).toBe(1);
    expect(suggestionZoneAfterLine(child, original, parent.contentHash!)).toBe(
      2,
    );
    expect(resolveDisplayAnchor(child, original).state).toBe("unverified");
    expect(
      resolveDisplayAnchor(child, original, await sha256Utf8(historical)).state,
    ).toBe("unverified");
    expect(child.reviewEdit?.sourceConflict).toBe(true);
    expect(child.status).toBe("open");
  });

  it.each([true, false, undefined])(
    "uses owner-derived sourceConflict=%s for FILES/raw insertion placement",
    (sourceConflict) => {
      const content = "Intro.\nNext.\n";
      const parent = suggestion({
        id: "source-conflict-parent",
        anchorStart: 2,
        beforeLines: "",
        afterLines: "Added.\n",
        anchorContext: JSON.stringify({
          version: 2,
          leading: ["Intro."],
          trailing: ["Next."],
        }),
        edits: [
          reviewEditFixture("insertion", "", 0, 0, "Added.\n", {
            sourceConflict,
          }),
        ],
      });
      const cards = expandReviewSuggestionEdits([parent]);
      expect(resolveDisplayAnchor(cards[0], content)).toEqual({
        line: 2,
        state: sourceConflict ? "unverified" : "anchored",
      });
      if (sourceConflict) {
        // A refused read projection's current offsets establish no location.
        const untrustedFrame = "untrusted\nframe\n";
        expect(
          suggestionZoneAfterLine(
            {
              ...cards[0],
              beforeLines: untrustedFrame,
              reviewEdit: {
                ...cards[0].reviewEdit!,
                currentEnd: untrustedFrame.length,
              },
            },
            content,
          ),
        ).toBe(1);
      }
      renderApp(
        <CodeEditor
          filePath="docs/spec.md"
          content={content}
          language="markdown"
          editable={false}
          showComments={false}
          comments={[]}
          suggestionOverlay={overlay(cards)}
        />,
      );
      const id = "source-conflict-parent:insertion";
      const card = screen.getByTestId(`review-suggestion-${id}`);
      expect(card).toHaveTextContent("Open");
      expect(card).toHaveTextContent("Added.");
      expect(
        within(card).getByRole("button", { name: "Discussion" }),
      ).toBeInTheDocument();
      const note = screen.queryByTestId(`suggestion-anchor-note-${id}`);
      if (sourceConflict)
        expect(note).toHaveTextContent(
          "This edit’s source needs attention after other accepted changes.",
        );
      else expect(note).toBeNull();
    },
  );

  it.each(["PROJECT DOCS", "document source", "FILES"])(
    "discloses parent withdrawal of two open edits in %s before sending a parent-only action",
    async (surface) => {
      const content = "Alpha middle Omega.\n";
      const parent = suggestion({
        id: "withdraw-parent",
        anchorStart: 1,
        beforeLines: content,
        afterLines: "Beta changed Gamma.\n",
        reviewerUserId: "user-reviewer",
        edits: [
          reviewEditFixture("alpha", content, 0, 5, "Beta"),
          reviewEditFixture("settled", content, 6, 12, "changed", {
            status: "rejected",
            commentId: "settled-discussion",
          }),
          reviewEditFixture("omega", content, 13, 18, "Gamma"),
        ],
      });
      const writes: unknown[] = [];
      server.use(
        http.post(
          `/api/v1/workspaces/${SOURCE_ID}/review/suggestions/withdraw-parent/withdraw`,
          async ({ request }) => {
            writes.push(await request.json());
            return HttpResponse.json({
              data: {
                suggestion: {
                  ...parent,
                  status: "withdrawn",
                  edits: parent.edits!.map((edit) =>
                    edit.status === "open"
                      ? { ...edit, status: "withdrawn" }
                      : edit,
                  ),
                },
                writtenHash: null,
                alreadyApplied: false,
              },
            });
          },
        ),
      );
      const cards = expandReviewSuggestionEdits([parent]);
      const reviewOverlay = overlay(cards, {
        viewerOwnsWorkspace: false,
        viewerUserId: "user-reviewer",
      });
      renderApp(
        surface === "FILES" || surface === "document source" ? (
          <CodeEditor
            filePath="docs/spec.md"
            content={content}
            language="markdown"
            editable={false}
            showComments={false}
            comments={[]}
            suggestionOverlay={reviewOverlay}
          />
        ) : (
          documentEditor(content, cards, { suggestionOverlay: reviewOverlay })
        ),
      );
      const id = "withdraw-parent:alpha";
      await screen.findByTestId(`review-suggestion-${id}`);
      const button = screen.getByTestId(`review-suggestion-withdraw-${id}`);
      expect(button).toHaveTextContent("Withdraw all open edits");
      fireEvent.click(button);
      const confirm = screen.getByTestId(
        `review-suggestion-withdraw-confirm-${id}`,
      );
      expect(confirm).toHaveTextContent(
        "Withdraw all 2 remaining open edits in this change?",
      );
      expect(writes).toEqual([]);
      fireEvent.click(
        screen.getByTestId(`review-suggestion-withdraw-keep-${id}`),
      );
      expect(writes).toEqual([]);
      fireEvent.click(screen.getByTestId(`review-suggestion-withdraw-${id}`));
      fireEvent.click(
        screen.getByTestId(`review-suggestion-withdraw-go-${id}`),
      );
      await waitFor(() => expect(writes).toEqual([{}]));
    },
  );

  it("retains cards on label-only records", () => {
    const content = "Intro.\n\n[I1. Intent]\n";
    for (const [id, beforeLines, afterLines, anchorContext] of [
      ["producer-stub", content, content + "\nBody.\n", null],
      [
        "context-stub",
        "",
        "\nBody.\n",
        JSON.stringify({
          version: 2,
          leading: ["Intro.", "", "[I1. Intent]"],
          trailing: [],
        }),
      ],
    ] as const) {
      const rewrite = vi.fn();
      const view = renderApp(
        documentEditor(
          content,
          [
            suggestion({
              id,
              beforeLines,
              afterLines,
              anchorStart: beforeLines ? 1 : 4,
              anchorContext,
            }),
          ],
          { onRewrite: rewrite },
        ),
      );
      expect(screen.getByTestId(`review-suggestion-${id}`)).toBeInTheDocument();
      expect(screen.getAllByTestId("semantic-record")).toHaveLength(1);
      expect(screen.getByTestId("semantic-record")).toHaveAttribute(
        "data-record-span",
        "I1",
      );
      expect(
        within(screen.getByTestId("semantic-record")).getByTestId(
          "semantic-record-id",
        ),
      ).toHaveTextContent(/^I1$/);
      expect(
        screen.getByTestId(`spec-suggestion-preview-${id}`),
      ).toHaveTextContent("Body.");
      expect(rewrite).not.toHaveBeenCalled();
      view.unmount();
    }
  });
  it("retains non-text sibling cards anchored on a label-only record", () => {
    renderApp(
      documentEditor("[I1. Intent]\n", [
        suggestion({
          id: "new-on-label",
          kind: "new_file",
          beforeLines: null,
          afterLines: "File body.\n",
          anchorStart: 1,
        }),
      ]),
    );
    expect(
      screen.getByTestId("review-suggestion-new-on-label"),
    ).toHaveTextContent("File body.");
    expect(screen.getAllByTestId("semantic-record")).toHaveLength(1);
    expect(screen.getByTestId("semantic-record")).toHaveAttribute(
      "data-record-span",
      "I1",
    );
    expect(
      within(screen.getByTestId("semantic-record")).getByTestId(
        "semantic-record-id",
      ),
    ).toHaveTextContent(/^I1$/);
  });
  it("keeps attention and original context visible while editing an unplaced proposal", async () => {
    renderApp(
      documentEditor("Repeated.\n\nRepeated.\n", [
        suggestion({
          id: "uncertain-edit",
          beforeLines: "Repeated.\n",
          afterLines: "Published.\n",
          anchorStart: 1,
        }),
      ]),
    );
    const card = screen.getByTestId("spec-suggestion-card-uncertain-edit");
    await userEvent.click(
      screen.getByTestId("review-suggestion-edit-toggle-uncertain-edit"),
    );
    fireEvent.change(
      screen.getByTestId("review-suggestion-edit-uncertain-edit"),
      { target: { value: "Draft.\n" } },
    );
    expect(card).toHaveTextContent("Needs attention");
    expect(within(card).queryByText("Original proposal context")).toBeNull();
    expect(card).toHaveTextContent("Repeated.");
    expect(card).toHaveTextContent("Published.");
    expect(within(card).getByTestId("review-hunk-diff")).toHaveTextContent(
      "Draft.",
    );
    expect(card).not.toHaveTextContent("Inline text shows");
  });

  it("uses the same preview for browse entry points", () => {
    renderApp(
      documentEditor(
        "Old.\n",
        [
          suggestion({
            id: "browse",
            beforeLines: "Old.\n",
            afterLines: "New.\n",
            anchorStart: 1,
          }),
        ],
        {},
      ),
    );
    expect(screen.queryByTestId("review-hunk-diff")).toBeNull();
    expect(
      screen.getByTestId("spec-suggestion-preview-browse"),
    ).toBeInTheDocument();
  });
  it("renders trusted empty-file alternatives separately while refusing stale proposals", () => {
    const record = suggestion({
      id: "empty",
      beforeLines: "",
      afterLines: "First.\n\nSecond.\n",
      anchorStart: 1,
      contentHash:
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    });
    let view = renderApp(documentEditor("", [record]));
    expect(
      screen
        .getByTestId("spec-suggestion-preview-empty")
        .querySelectorAll('[data-spec-review-change="add"]'),
    ).toHaveLength(2);
    expect(screen.queryByTestId("empty-rich-editor")).not.toBeInTheDocument();
    view.unmount();
    view = renderApp(documentEditor("", [{ ...record, contentHash: "stale" }]));
    expect(
      screen.queryByTestId("spec-suggestion-preview-empty"),
    ).not.toBeInTheDocument();
    expect(screen.getByTestId("spec-suggestion-card-empty")).toHaveTextContent(
      "Needs attention",
    );
    view.unmount();
    view = renderApp(
      documentEditor("", [
        record,
        { ...record, id: "competing", afterLines: "Alternative.\n" },
      ]),
    );
    const preview = screen.getByTestId("spec-suggestion-preview-empty");
    expect(preview).toHaveTextContent("First.");
    expect(preview).not.toHaveTextContent("Alternative.");
    const competing = screen.getByTestId("spec-suggestion-card-competing");
    expect(competing).not.toHaveTextContent("Needs attention");
    fireEvent.click(
      within(competing).getByRole("radio", { name: "Preview in document" }),
    );
    expect(preview).toHaveTextContent("Alternative.");
    expect(preview).not.toHaveTextContent("First.");
    expect(document.querySelectorAll("[data-block-index]")).toHaveLength(0);
    view.unmount();
    view = renderApp(documentEditor("", [record]));
    expect(
      screen.queryByRole("button", { name: "Edit document text" }),
    ).toBeNull();
  });
  it("previews body changes after an unchanged record label in the first paragraph", () => {
    const before = "[I1. Intent]\nOld body.\n";
    const record = suggestion({
      id: "first-body",
      beforeLines: before,
      afterLines: "[I1. Intent]\nNew body.\n",
      anchorStart: 1,
    });
    const view = renderApp(documentEditor(before, [record]));
    const preview = screen.getByTestId("spec-suggestion-preview-first-body");
    expect(preview).toHaveTextContent("New body.");
    expect(preview).not.toHaveTextContent("Intent");
    expect(screen.getAllByTestId("semantic-record")).toHaveLength(1);
    expect(screen.getByTestId("semantic-record")).toHaveAttribute(
      "data-record-span",
      "I1",
    );
    expect(
      within(screen.getByTestId("semantic-record")).getByTestId(
        "semantic-record-id",
      ),
    ).toHaveTextContent(/^I1$/);
    view.unmount();
    renderApp(
      documentEditor(before, [
        { ...record, afterLines: "[I2. Different]\nNew body.\n" },
      ]),
    );
    expect(
      screen.queryByTestId("spec-suggestion-preview-first-body"),
    ).not.toBeInTheDocument();
    expect(
      screen.getByTestId("spec-suggestion-card-first-body"),
    ).toHaveTextContent("Needs attention");
  });
  it("keeps unchanged interior paragraphs when one hunk inserts and edits paragraphs", () => {
    const before = "First.\n\nUnchanged middle.\n\nOld end.\n";
    renderApp(
      documentEditor(before, [
        suggestion({
          id: "aligned",
          beforeLines: before,
          afterLines: "First.\n\nInserted.\n\nUnchanged middle.\n\nNew end.\n",
          anchorStart: 1,
        }),
      ]),
    );
    const preview = screen.getByTestId("spec-suggestion-preview-aligned");
    const middle = within(preview).getByText("Unchanged middle.");
    expect(middle.closest("ins,del,[data-spec-review-change]")).toBeNull();
    expect(preview.textContent?.match(/Unchanged middle\./g)).toHaveLength(1);
    expect(
      within(preview)
        .getByText("Inserted.")
        .closest('[data-spec-review-change="add"]'),
    ).not.toBeNull();
  });
  it("keeps distinct parent hunks in one paragraph as separate alternative cards with original actions", async () => {
    const accepted: string[] = [];
    server.use(
      http.post(
        `*/api/v1/workspaces/${SOURCE_ID}/review/suggestions/:id/accept`,
        ({ params }) => {
          accepted.push(String(params.id));
          return HttpResponse.json({
            data: {
              suggestion: suggestion({
                id: String(params.id),
                status: "accepted",
              }),
              writtenHash: "next",
              alreadyApplied: false,
            },
          });
        },
      ),
    );
    renderApp(
      documentEditor("Alpha line.\nMiddle line.\nOmega line.\n", [
        suggestion({
          id: "alpha",
          beforeLines: "Alpha line.\n",
          afterLines: "First line.\n",
          anchorStart: 1,
        }),
        suggestion({
          id: "omega",
          beforeLines: "Omega line.\n",
          afterLines: "Last line.\n",
          anchorStart: 3,
        }),
      ]),
    );
    for (const id of ["alpha", "omega"]) {
      expect(
        screen.getByTestId(`spec-suggestion-card-${id}`),
      ).not.toHaveTextContent("Needs attention");
      expect(screen.getAllByTestId(`review-suggestion-${id}`)).toHaveLength(1);
    }
    expect(screen.getAllByTestId(/^spec-suggestion-preview-/)).toHaveLength(1);
    const canonical = document.querySelector('[data-block-index="0"]');
    expect(canonical?.querySelector("del")).toHaveTextContent("Alpha");
    expect(canonical).toHaveTextContent("Middle line.");
    expect(canonical).toHaveTextContent("Omega line.");
    expect(canonical?.querySelector("ins")).toHaveTextContent("First");
    await userEvent.click(
      screen.getByTestId("review-suggestion-edit-toggle-omega"),
    );
    expect(
      screen.getByTestId("spec-suggestion-card-alpha"),
    ).not.toHaveTextContent("Needs attention");
    expect(
      screen.getByTestId("spec-suggestion-card-omega"),
    ).not.toHaveTextContent("Needs attention");
    expect(
      within(screen.getByTestId("spec-suggestion-card-omega")).getByTestId(
        "review-hunk-diff",
      ),
    ).toBeInTheDocument();
    await userEvent.click(
      screen.getByTestId("review-suggestion-edit-toggle-omega"),
    );
    expect(
      screen.getByTestId("spec-suggestion-card-omega"),
    ).not.toHaveTextContent("Needs attention");
    await userEvent.click(screen.getByTestId("review-suggestion-accept-omega"));
    await waitFor(() => expect(accepted).toEqual(["omega"]));
  });

  it("previews disjoint source hunks separately when formatting makes their visible ranges conflict", () => {
    renderApp(
      documentEditor("**Alpha.**\nMiddle.\nOmega.\n", [
        suggestion({
          id: "format-only",
          beforeLines: "**Alpha.**\n",
          afterLines: "*Alpha.*\n",
          anchorStart: 1,
        }),
        suggestion({
          id: "last-word",
          beforeLines: "Omega.\n",
          afterLines: "Last.\n",
          anchorStart: 3,
        }),
      ]),
    );
    for (const id of ["format-only", "last-word"]) {
      expect(
        screen.getByTestId(`spec-suggestion-card-${id}`),
      ).not.toHaveTextContent("Needs attention");
    }
  });
  it("groups suggestions by line and sizes view zones from the diff", () => {
    const byLine = suggestionsByLine([
      suggestion({ id: "a", anchorStart: 3 }),
      suggestion({ id: "b", anchorStart: 3 }),
      suggestion({ id: "c", anchorStart: 9 }),
    ]);
    expect([...byLine.keys()]).toEqual([3, 9]);
    expect(byLine.get(3)?.map((s) => s.id)).toEqual(["a", "b"]);
    // A removed row, an added row and the hunk header.
    expect(suggestionZoneHeight(suggestion({}))).toBe(72 + 3 * 20);
    // Context both sides share takes one row, not two.
    expect(
      suggestionZoneHeight(
        suggestion({ beforeLines: "a\nb\n", afterLines: "a\nb\nc\n" }),
      ),
    ).toBe(72 + 4 * 20);
    expect(
      suggestionZoneHeight(
        suggestion({ kind: "new_file", beforeLines: null, afterLines: "x\n" }),
      ),
    ).toBe(72 + 20);
    expect(
      suggestionZoneHeight(suggestion({ kind: "comment", note: "why?" })),
    ).toBe(72 + 40);
  });

  it("keeps a compact child in git diff style and sizes and places its own source edit", () => {
    const source = "Context before.\nAlpha unchanged.\nContext after.\n";
    const at = source.indexOf("Alpha");
    const [child] = expandReviewSuggestionEdits([
      suggestion({
        beforeLines: source,
        expectedBeforeLines: source,
        anchorStart: 1,
        edits: [
          reviewEditFixture("child", source, at, at + 5, "Beta", {
            expectedBeforeLines: undefined,
            expectedAfterLines: undefined,
          }),
        ],
      }),
    ]);
    expect(suggestionZoneAfterLine(child, source)).toBe(2);
    expect(suggestionZoneHeight(child)).toBe(72 + 3 * 20);
    expect(child.beforeLines).toBe(source);
    expect(child.afterLines).toBe("Beta");
  });

  it("splits highlighted HTML into well-formed lines and groups zones by the line they follow", () => {
    // A span running across lines is closed at the line end and reopened.
    expect(
      splitHighlightedLines(
        '<span style="a">/* one\ntwo */</span> <span>x</span>\nthree',
      ),
    ).toEqual([
      '<span style="a">/* one</span>',
      '<span style="a">two */</span> <span>x</span>',
      "three",
    ]);
    expect(splitHighlightedLines("")).toEqual([""]);
    expect(splitHighlightedLines("a\n")).toEqual(["a", ""]);
    const zones = suggestionZonesByLine(
      [
        suggestion({ id: "a", anchorStart: 3 }),
        suggestion({ id: "b", anchorStart: 3 }),
        suggestion({ id: "top", anchorStart: null, kind: "new_file" }),
      ],
      "a\nb\nreturn 1;\nc\n",
    );
    expect(zones.get(3)?.map((s) => s.id)).toEqual(["a", "b"]);
    expect(zones.get(0)?.map((s) => s.id)).toEqual(["top"]);
  });

  it("places a suggestion by where its replaced lines are in the displayed file", () => {
    const published = suggestion({
      anchorStart: 3,
      beforeLines: "return 1;\n",
    });
    // Unchanged file: at the published line.
    expect(resolveDisplayAnchor(published, "a\nb\nreturn 1;\nc\n")).toEqual({
      line: 3,
      state: "anchored",
    });
    // Two lines inserted above: found once, shown there.
    const grown = "// x\n// y\na\nb\nreturn 1;\nc\n";
    expect(resolveDisplayAnchor(published, grown)).toEqual({
      line: 5,
      state: "moved",
    });
    expect(suggestionZoneAfterLine(published, grown)).toBe(5);
    // A three-line block published at line 20, now gone from a two-line
    // file: the zone stays on a rendered row (the last one).
    expect(
      suggestionZoneAfterLine(
        suggestion({ anchorStart: 20, beforeLines: "x\ny\nz\n" }),
        "a\nb\n",
      ),
    ).toBe(3);
    expect(suggestionsByLine([published], grown).get(5)).toHaveLength(1);
    // Replaced lines gone: kept at the published line, marked lost.
    expect(resolveDisplayAnchor(published, "a\nb\nc\n")).toEqual({
      line: 3,
      state: "lost",
    });
    // Present twice with no published context: not found uniquely, so
    // marked, even though one twin sits at the published line.
    expect(
      resolveDisplayAnchor(published, "z\nz\nreturn 1;\nq\nreturn 1;\n"),
    ).toEqual({ line: 3, state: "ambiguous" });
    // Present twice and neither at the published line: ambiguous.
    expect(
      resolveDisplayAnchor(published, "return 1;\nz\nz\nz\nreturn 1;\n"),
    ).toEqual({ line: 3, state: "ambiguous" });
    // Twins that swapped places: the published context picks the right
    // one even though the old line number still matches a block.
    const framed = suggestion({
      anchorStart: 2,
      beforeLines: "    x\n",
      anchorContext: JSON.stringify({
        version: 2,
        leading: ["fn one() {"],
        trailing: ["}", "fn two() {", "    x"],
      }),
    });
    expect(
      resolveDisplayAnchor(
        framed,
        "fn two() {\n    x\n}\nfn one() {\n    x\n}\nfn two() {\n    x\n}\n",
      ),
    ).toEqual({ line: 5, state: "moved" });
    // A version-1 context is ignored: the twins stay ambiguous.
    expect(
      resolveDisplayAnchor(
        suggestion({
          ...framed,
          anchorContext: JSON.stringify({
            version: 1,
            leading: ["fn one() {"],
            trailing: ["}"],
          }),
        }),
        "fn two() {\n    x\n}\nfn one() {\n    x\n}\nfn two() {\n    x\n}\n",
      ),
    ).toEqual({ line: 2, state: "ambiguous" });
    // A comment follows the line text it was left on; without that text
    // nothing places it, so it is marked at the published line.
    const comment = suggestion({
      kind: "comment",
      anchorStart: 2,
      beforeLines: "b\n",
      afterLines: null,
      note: "Rename this.",
    });
    expect(resolveDisplayAnchor(comment, "// x\na\nb\nc\n")).toEqual({
      line: 3,
      state: "moved",
    });
    expect(resolveDisplayAnchor(comment, "a\nc\n")).toEqual({
      line: 2,
      state: "lost",
    });
    expect(
      resolveDisplayAnchor(
        suggestion({ ...comment, beforeLines: null }),
        "// x\na\nb\nc\n",
      ),
    ).toEqual({ line: 2, state: "unverified" });
    // An empty text is a blank line, looked for like any other.
    expect(
      resolveDisplayAnchor(
        suggestion({ ...comment, anchorStart: 2, beforeLines: "" }),
        "a\n\nb\n",
      ),
    ).toEqual({ line: 2, state: "anchored" });
    expect(
      resolveDisplayAnchor(
        suggestion({ ...comment, anchorStart: 2, beforeLines: "" }),
        "a\n\nb\n\nc\n",
      ),
    ).toEqual({ line: 2, state: "ambiguous" });
    // A pure insertion is placed only where its published context pins it.
    const insertion = suggestion({
      anchorStart: 2,
      beforeLines: "",
      afterLines: "inserted\n",
      anchorContext: JSON.stringify({
        version: 2,
        leading: ["a"],
        trailing: ["b"],
      }),
    });
    expect(resolveDisplayAnchor(insertion, "x\na\nb\n")).toEqual({
      line: 2,
      state: "unverified",
    });
    expect(resolveDisplayAnchor(insertion, "a\nb\na\nb\n")).toEqual({
      line: 2,
      state: "ambiguous",
    });
    expect(
      resolveDisplayAnchor(
        suggestion({ ...insertion, anchorContext: null }),
        "x\na\nb\n",
      ),
    ).toEqual({ line: 2, state: "unverified" });
    // Without content (or for non-hunks) the published line is used as is.
    expect(resolveDisplayAnchor(published, null)).toEqual({
      line: 3,
      state: "anchored",
    });
    // A comment recorded by the internal route on a CRLF file, and on a line
    // with trailing spaces: the line as the file holds it, placed in place.
    const crlf = "before\r\ntarget\r\nafter\r\nmore\r\ntarget\r\nend\r\n";
    expect(
      resolveDisplayAnchor(
        suggestion({
          ...comment,
          anchorStart: 2,
          beforeLines: "target\r",
          anchorContext: JSON.stringify({
            version: 2,
            leading: ["before\r"],
            trailing: ["after\r"],
          }),
        }),
        crlf,
      ),
    ).toEqual({ line: 2, state: "anchored" });
    expect(
      resolveDisplayAnchor(
        suggestion({ ...comment, anchorStart: 2, beforeLines: "target  " }),
        "before\ntarget  \nafter\n",
      ),
    ).toEqual({ line: 2, state: "anchored" });
  });

  it("adds one Monaco view zone per suggestion and renders the card into it", async () => {
    const zones: { id: string; node: HTMLElement; after: number }[] = [];
    let nextId = 0;
    const editor = {
      changeViewZones: (
        callback: (accessor: {
          addZone: (zone: {
            afterLineNumber: number;
            heightInPx: number;
            domNode: HTMLElement;
          }) => string;
          removeZone: (id: string) => void;
        }) => void,
      ) => {
        callback({
          addZone: (zone) => {
            const id = `zone-${nextId++}`;
            document.body.appendChild(zone.domNode);
            zones.push({ id, node: zone.domNode, after: zone.afterLineNumber });
            return id;
          },
          removeZone: (id) => {
            const index = zones.findIndex((zone) => zone.id === id);
            if (index >= 0) {
              zones[index].node.remove();
              zones.splice(index, 1);
            }
          },
        });
      },
    };
    const { unmount } = renderApp(
      <MonacoSuggestionZones
        editor={editor}
        mountVersion={1}
        overlay={overlay([
          suggestion({ id: "sug-1", anchorStart: 3 }),
          suggestion({ id: "sug-2", anchorStart: 8 }),
        ])}
      />,
    );
    await waitFor(() => expect(zones).toHaveLength(2));
    // Each hunk replaces one line, so its zone follows that line.
    expect(zones.map((zone) => zone.after)).toEqual([3, 8]);
    expect(
      await screen.findByTestId("review-suggestion-sug-1"),
    ).toBeInTheDocument();
    expect(screen.getByTestId("review-suggestion-sug-2")).toHaveTextContent(
      "−return 1;",
    );
    act(() => unmount());
    expect(zones).toHaveLength(0);
  });

  it("re-places existing zones when the file changes instead of recreating them", async () => {
    let value = "a\nb\nreturn 1;\nc\n";
    let onChange: (() => void) | null = null;
    const zones: {
      id: string;
      zone: { afterLineNumber: number; domNode: HTMLElement };
    }[] = [];
    const laidOut: string[] = [];
    let nextId = 0;
    const editor = {
      getModel: () => ({ getValue: () => value }),
      onDidChangeModelContent: (listener: () => void) => {
        onChange = listener;
        return { dispose: () => undefined };
      },
      changeViewZones: (
        callback: (accessor: {
          addZone: (zone: {
            afterLineNumber: number;
            heightInPx: number;
            domNode: HTMLElement;
          }) => string;
          removeZone: (id: string) => void;
          layoutZone: (id: string) => void;
        }) => void,
      ) => {
        callback({
          addZone: (zone) => {
            const id = `zone-${nextId++}`;
            document.body.appendChild(zone.domNode);
            zones.push({ id, zone });
            return id;
          },
          removeZone: (id) => {
            const index = zones.findIndex((zone) => zone.id === id);
            if (index >= 0) {
              zones[index].zone.domNode.remove();
              zones.splice(index, 1);
            }
          },
          layoutZone: (id) => laidOut.push(id),
        });
      },
    };
    renderApp(
      <MonacoSuggestionZones
        editor={editor}
        mountVersion={1}
        overlay={overlay([suggestion({ id: "sug-1", anchorStart: 3 })])}
      />,
    );
    await waitFor(() => expect(zones).toHaveLength(1));
    expect(zones[0].zone.afterLineNumber).toBe(3);
    const card = await screen.findByTestId("review-suggestion-sug-1");
    // The owner inserts two lines above the replaced line.
    value = "// x\n// y\na\nb\nreturn 1;\nc\n";
    act(() => onChange?.());
    await waitFor(() => expect(zones[0].zone.afterLineNumber).toBe(5));
    // Same zone, laid out again; the card was not remounted.
    expect(zones).toHaveLength(1);
    expect(zones[0].id).toBe("zone-0");
    expect(laidOut).toContain("zone-0");
    expect(screen.getByTestId("review-suggestion-sug-1")).toBe(card);
    expect(
      screen.getByTestId("suggestion-anchor-note-sug-1"),
    ).toHaveTextContent(/moved/i);
  });

  it("keeps a card's zone and draft when another suggestion arrives", async () => {
    const zones: { id: string; node: HTMLElement }[] = [];
    let nextId = 0;
    const editor = {
      changeViewZones: (
        callback: (accessor: {
          addZone: (zone: { domNode: HTMLElement }) => string;
          removeZone: (id: string) => void;
          layoutZone: (id: string) => void;
        }) => void,
      ) => {
        callback({
          addZone: (zone) => {
            const id = `zone-${nextId++}`;
            document.body.appendChild(zone.domNode);
            zones.push({ id, node: zone.domNode });
            return id;
          },
          removeZone: (id) => {
            const index = zones.findIndex((zone) => zone.id === id);
            if (index >= 0) {
              zones[index].node.remove();
              zones.splice(index, 1);
            }
          },
          layoutZone: () => undefined,
        });
      },
    };
    const first = suggestion({ id: "sug-1", anchorStart: 3 });
    let setList: (list: ReviewSuggestion[]) => void = () => undefined;
    const expose = (set: (list: ReviewSuggestion[]) => void) => {
      setList = set;
    };
    const Host: React.FC<{
      expose: (set: (list: ReviewSuggestion[]) => void) => void;
    }> = ({ expose }) => {
      const [list, set] = React.useState([first]);
      React.useEffect(() => expose(set), [expose, set]);
      return (
        <MonacoSuggestionZones
          editor={editor}
          mountVersion={1}
          overlay={overlay(list)}
        />
      );
    };
    renderApp(<Host expose={expose} />);
    const user = userEvent.setup();
    await user.click(
      await screen.findByTestId("review-suggestion-edit-toggle-sug-1"),
    );
    const draft = screen.getByTestId("review-suggestion-edit-sug-1");
    await user.clear(draft);
    await user.type(draft, "return 42;");
    // A refetch brings a second suggestion: the first zone and its card
    // (draft included) stay; only the new zone is added.
    act(() => setList([first, suggestion({ id: "sug-2", anchorStart: 8 })]));
    await waitFor(() => expect(zones).toHaveLength(2));
    expect(zones.map((zone) => zone.id)).toEqual(["zone-0", "zone-1"]);
    expect(screen.getByTestId("review-suggestion-edit-sug-1")).toBe(draft);
    expect(draft).toHaveValue("return 42;");
    // Dropping the second one removes only its zone.
    act(() => setList([first]));
    await waitFor(() => expect(zones).toHaveLength(1));
    expect(zones[0].id).toBe("zone-0");
    expect(draft).toHaveValue("return 42;");
  });

  it("keeps undo inside an inline card's input instead of the document history", async () => {
    const { DocumentPreview } = await import("../files/DocumentPreview");
    renderApp(
      <DocumentPreview
        workspaceId=""
        repoId=""
        filePath="docs/spec.md"
        content={"Hello there.\n\nSecond paragraph.\n"}
        canAnswer
        onRewrite={vi.fn(() => true)}
        onNavigate={vi.fn()}
        suggestionOverlay={overlay([
          suggestion({
            id: "sug-doc",
            path: "docs/spec.md",
            anchorStart: 1,
            beforeLines: "Hello there.\n",
            afterLines: "Hello, reviewer.\n",
          }),
        ])}
      />,
    );
    const user = userEvent.setup();
    await user.click(
      await screen.findByTestId("review-suggestion-edit-toggle-sug-doc"),
    );
    const draft = screen.getByTestId("review-suggestion-edit-sug-doc");
    // Not prevented: the textarea keeps its native undo. The document's
    // capture-phase handler prevents the same shortcut outside the card.
    expect(fireEvent.keyDown(draft, { key: "z", ctrlKey: true })).toBe(true);
    expect(fireEvent.keyDown(draft, { key: "z", metaKey: true })).toBe(true);
    // Formatting shortcuts (the window-level listener) leave the card's
    // input alone too: no list toggle, no bold, on the containing block.
    expect(fireEvent.keyDown(draft, { key: "l", ctrlKey: true })).toBe(true);
    expect(fireEvent.keyDown(draft, { key: "b", metaKey: true })).toBe(true);
    expect(
      fireEvent.keyDown(screen.getByText("Second paragraph."), {
        key: "l",
        ctrlKey: true,
      }),
    ).toBe(true);
    expect(
      fireEvent.keyDown(screen.getByTestId("spec-suggestion-card-sug-doc"), {
        key: "z",
        ctrlKey: true,
      }),
    ).toBe(true);
    expect(
      fireEvent.keyDown(screen.getByText("Second paragraph."), {
        key: "z",
        ctrlKey: true,
      }),
    ).toBe(true);
  });

  it("renders pushed document edits in DocumentPreview without duplicating the accepted paragraph or a Git diff", async () => {
    const { DocumentPreview } = await import("../files/DocumentPreview");
    const before = "As the owner, i started this doc for business alignment.\n";
    const after =
      "As the owner, I started this doc to align on business of wire transfer fee.\n\nOK, as a reviewer, I will collaborate with you on this doc.\n";
    const rewrite = vi.fn(() => true);
    renderApp(
      <DocumentPreview
        workspaceId=""
        repoId=""
        filePath="docs/spec.md"
        content={before}
        canAnswer
        onRewrite={rewrite}
        onNavigate={vi.fn()}
        suggestionOverlay={overlay([
          suggestion({
            id: "example",
            path: "docs/spec.md",
            anchorStart: 1,
            beforeLines: before,
            afterLines: after,
          }),
        ])}
      />,
    );
    const preview = await screen.findByTestId(
      "spec-suggestion-preview-example",
    );
    expect(preview).toHaveTextContent("As the owner,");
    expect(preview.querySelector("del")).toHaveTextContent("i");
    expect(preview.querySelector("ins")).toHaveTextContent("I");
    expect(preview).toHaveTextContent(
      "OK, as a reviewer, I will collaborate with you on this doc.",
    );
    expect(screen.queryByTestId("review-hunk-diff")).not.toBeInTheDocument();
    expect(screen.getAllByTestId("review-suggestion-example")).toHaveLength(1);
    expect(rewrite).not.toHaveBeenCalled();
    const user = userEvent.setup();
    await user.click(within(preview).getAllByRole("button")[0]);
    expect(screen.getByTestId("spec-suggestion-card-example")).toHaveAttribute(
      "data-selected",
      "true",
    );
    expect(
      screen.queryByRole("button", { name: "Edit document text" }),
    ).toBeNull();
  });

  it("preserves Markdown formatting without writing canonical source while suggestions are visible", async () => {
    const before = "**Hello there.**\n\nOutside.\n";
    const rewrite = vi.fn(() => true);
    renderApp(
      documentEditor(
        before,
        [
          suggestion({
            id: "formatted",
            anchorStart: 1,
            beforeLines: "**Hello there.**\n",
            afterLines: "**Hello reviewer.**\n",
          }),
        ],
        { onRewrite: rewrite },
      ),
    );
    const preview = screen.getByTestId("spec-suggestion-preview-formatted");
    expect(preview.querySelector("del strong")).toHaveTextContent("there");
    expect(preview.querySelector("ins strong")).toHaveTextContent("reviewer");
    expect(document.querySelector('[contenteditable="true"]')).toBeNull();
    expect(rewrite).not.toHaveBeenCalled();
    expect(screen.getByText("Outside.")).toBeVisible();
  });

  it("keeps ambiguous, changed and overlapping proposals visible without guessing their document location", async () => {
    renderApp(
      documentEditor("Repeated.\n\nRepeated.\n\nUnique.\n", [
        suggestion({
          id: "ambiguous",
          anchorStart: 1,
          beforeLines: "Repeated.\n",
          afterLines: "Changed.\n",
        }),
        suggestion({
          id: "lost",
          anchorStart: 5,
          beforeLines: "Gone.\n",
          afterLines: "Replacement.\n",
        }),
        suggestion({
          id: "overlap1",
          anchorStart: 5,
          beforeLines: "Unique.\n",
          afterLines: "One.\n",
        }),
        suggestion({
          id: "overlap2",
          anchorStart: 5,
          beforeLines: "Unique.\n",
          afterLines: "Two.\n",
        }),
      ]),
    );
    const canonicalBlocks = [
      ...document.querySelectorAll("[data-block-index]"),
    ];
    expect(
      canonicalBlocks.filter((block) => block.textContent === "Repeated."),
    ).toHaveLength(2);
    const preview = screen.getByTestId("spec-suggestion-preview-overlap1");
    expect(preview.querySelector("del")).toHaveTextContent("Unique");
    expect(preview.querySelector("ins")).toHaveTextContent("One");
    expect(document.querySelectorAll('[data-block-index="2"]')).toHaveLength(1);
    for (const id of ["ambiguous", "lost"]) {
      expect(
        screen.queryByTestId(`spec-suggestion-preview-${id}`),
      ).not.toBeInTheDocument();
      expect(
        screen.getByTestId(`spec-suggestion-card-${id}`),
      ).toHaveTextContent("Needs attention");
    }
    for (const id of ["overlap1", "overlap2"])
      expect(
        screen.getByTestId(`spec-suggestion-card-${id}`),
      ).not.toHaveTextContent("Needs attention");
    expect(
      screen.getByTestId("spec-suggestion-card-ambiguous"),
    ).toHaveTextContent("Changed.");
    expect(screen.queryByTestId("review-hunk-diff")).not.toBeInTheDocument();
  });

  it("uses the parent change for edited acceptance and retains it through refusal and retry", async () => {
    const accepts: unknown[] = [];
    server.use(
      http.post(
        `*/api/v1/workspaces/${SOURCE_ID}/review/suggestions/retry/accept`,
        async ({ request }) => {
          accepts.push(await request.json());
          return accepts.length === 1
            ? HttpResponse.json(
                {
                  error: {
                    code: "CONFLICT",
                    message: "The live file changed.",
                  },
                },
                { status: 409 },
              )
            : HttpResponse.json({
                data: {
                  suggestion: suggestion({ id: "retry", status: "accepted" }),
                  writtenHash: "spec-v2",
                  alreadyApplied: false,
                },
              });
        },
      ),
    );
    const rewrite = vi.fn();
    renderApp(
      documentEditor(
        "Original.\n",
        [
          suggestion({
            id: "retry",
            anchorStart: 1,
            beforeLines: "Original.\n",
            afterLines: "Proposed.\n",
          }),
        ],
        { onRewrite: rewrite },
      ),
    );
    const user = userEvent.setup();
    await user.click(screen.getByTestId("review-suggestion-edit-toggle-retry"));
    const draft = screen.getByTestId("review-suggestion-edit-retry");
    fireEvent.change(draft, { target: { value: "Adjusted.\n" } });
    const editPreview = screen.getByTestId("review-hunk-diff");
    expect(editPreview).toHaveTextContent("Adjusted.");
    expect(editPreview).not.toHaveTextContent("Proposed.");
    expect(
      screen.getByText(/The document view is unchanged/),
    ).toBeInTheDocument();
    await user.click(screen.getByTestId("review-suggestion-accept-retry"));
    await waitFor(() =>
      expect(accepts).toEqual([{ afterLines: "Adjusted.\n" }]),
    );
    expect(draft).toHaveValue("Adjusted.\n");
    expect(
      screen.getByTestId("spec-suggestion-preview-retry"),
    ).toBeInTheDocument();
    await user.click(screen.getByTestId("review-suggestion-accept-retry"));
    await waitFor(() => expect(accepts).toHaveLength(2));
    expect(accepts[1]).toEqual({ afterLines: "Adjusted.\n" });
    expect(rewrite).not.toHaveBeenCalled();
  });

  it("places insertions at their published context and relocates a partial rich block", () => {
    renderApp(
      documentEditor("# Intro\n\nFirst.\n\nSecond.\n\n- One\n- Two\n", [
        suggestion({
          id: "insertion",
          anchorStart: 5,
          beforeLines: "",
          afterLines: "Added.\n\n",
          anchorContext: JSON.stringify({
            version: 2,
            leading: ["First.", ""],
            trailing: ["Second."],
          }),
        }),
        suggestion({
          id: "list",
          anchorStart: 1,
          beforeLines: "- Two\n",
          afterLines: "- Revised\n",
        }),
      ]),
    );
    const insertion = screen.getByTestId("spec-suggestion-preview-insertion");
    expect(insertion).toHaveTextContent("Added.");
    expect(insertion).toHaveTextContent("Second.");
    const list = screen.getByTestId("spec-suggestion-preview-list");
    expect(list.querySelectorAll("ul")).toHaveLength(2);
    expect(list).toHaveTextContent("One");
    expect(list).toHaveTextContent("Revised");
    expect(screen.getByRole("heading", { name: "Intro" })).toBeInTheDocument();
  });

  it("keeps a relocated pure insertion in attention when Accept cannot relocate it", () => {
    renderApp(
      documentEditor("# Intro\n\nFirst.\n\nSecond.\n", [
        suggestion({
          id: "relocated-insertion",
          anchorStart: 3,
          beforeLines: "",
          afterLines: "Added.\n\n",
          anchorContext: JSON.stringify({
            version: 2,
            leading: ["First.", ""],
            trailing: ["Second."],
          }),
        }),
      ]),
    );
    expect(
      screen.queryByTestId("spec-suggestion-preview-relocated-insertion"),
    ).not.toBeInTheDocument();
    expect(
      screen.getByTestId("spec-suggestion-card-relocated-insertion"),
    ).toHaveTextContent("Needs attention");
    expect(screen.getByText("Second.")).toBeInTheDocument();
    expect(screen.queryByText("Original proposal context")).toBeNull();
    expect(
      screen.getByTestId("spec-suggestion-card-relocated-insertion"),
    ).toHaveTextContent("Added.");
  });

  it("keeps DocumentPreview and FILES card authority identical when an author display label matches a nonowner's user ID", () => {
    const content = "Old.\n";
    const record = suggestion({
      id: "parity",
      authorLabel: "alex",
      anchorStart: 1,
      beforeLines: content,
      afterLines: "New.\n",
    });
    const reviewOverlay = overlay([record], {
      viewerOwnsWorkspace: false,
      viewerUserId: "alex",
      ownerVmRunning: false,
      sourceWritable: true,
    });
    const doc = renderApp(
      documentEditor(content, [record], {
        canAnswer: false,
        suggestionOverlay: reviewOverlay,
      }),
    );
    expect(
      screen.getByTestId("spec-suggestion-preview-parity"),
    ).toBeInTheDocument();
    expect(
      screen.queryByTestId("review-suggestion-withdraw-parity"),
    ).not.toBeInTheDocument();
    doc.unmount();
    renderApp(
      <CodeEditor
        filePath="docs/spec.md"
        content={content}
        language="markdown"
        editable={false}
        showComments={false}
        comments={[]}
        onAddComment={vi.fn()}
        onResolveComment={vi.fn()}
        onReplyToComment={vi.fn()}
        suggestionOverlay={reviewOverlay}
      />,
    );
    expect(screen.getByTestId("review-hunk-diff")).toBeInTheDocument();
    expect(
      screen.queryByTestId("review-suggestion-withdraw-parity"),
    ).not.toBeInTheDocument();
  });

  it("keeps nonowners and stopped views within the existing action authority", () => {
    const record = suggestion({
      id: "readonly",
      anchorStart: 1,
      beforeLines: "Old.\n",
      afterLines: "New.\n",
    });
    renderApp(
      documentEditor("Old.\n", [record], {
        canAnswer: false,
        suggestionOverlay: overlay([record], {
          viewerOwnsWorkspace: false,
          ownerVmRunning: false,
          sourceWritable: true,
        }),
      }),
    );
    expect(
      screen.getByTestId("spec-suggestion-preview-readonly"),
    ).toBeInTheDocument();
    expect(
      screen.queryByTestId("review-suggestion-accept-readonly"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Edit document text" }),
    ).not.toBeInTheDocument();
  });

  it("offers no decision or reply on a card whose source state is not known", () => {
    // Unknown is closed: a card offers controls only when its source is
    // known to accept thread writes (`g5.documents.comments.controls`).
    const record = suggestion({
      id: "unknown",
      anchorStart: 1,
      beforeLines: "Old.\n",
      afterLines: "New.\n",
    });
    renderApp(
      documentEditor("Old.\n", [record], {
        canAnswer: false,
        suggestionOverlay: overlay([record], { sourceWritable: undefined }),
      }),
    );
    expect(
      screen.getByTestId("spec-suggestion-preview-unknown"),
    ).toBeInTheDocument();
    for (const control of ["accept", "reject"]) {
      expect(
        screen.queryByTestId(`review-suggestion-${control}-unknown`),
      ).not.toBeInTheDocument();
    }
  });

  it("leaves unchanged hunk context in semantic record frames and isolates the changed body", () => {
    const content = "[I1. Intent]\n\nOld body.\n\n[D2. Other]\n\nUnchanged.\n";
    renderApp(
      documentEditor(content, [
        suggestion({
          id: "record-body",
          anchorStart: 1,
          beforeLines: "[I1. Intent]\n\nOld body.\n",
          afterLines: "[I1. Intent]\n\nNew body.\n",
        }),
      ]),
    );
    expect(
      screen.getByTestId("spec-suggestion-preview-record-body"),
    ).toHaveTextContent("New body.");
    expect(
      screen.getByTestId("spec-suggestion-preview-record-body"),
    ).not.toHaveTextContent("Intent");
    expect(screen.getByText("Unchanged.")).toBeInTheDocument();
  });

  it("preserves an added paragraph at EOF when the canonical document has no final newline", () => {
    renderApp(
      documentEditor("First.", [
        suggestion({
          id: "eof",
          anchorStart: 1,
          beforeLines: "First.\n",
          afterLines: "First.\n\nAdded.\n",
        }),
      ]),
    );
    const preview = screen.getByTestId("spec-suggestion-preview-eof");
    const paragraphs = [...preview.querySelectorAll("p")];
    expect(paragraphs.map((p) => p.textContent)).toEqual(["First.", "Added."]);
    expect(paragraphs[0].querySelector("ins,del")).toBeNull();
  });

  it("retains a reviewer-created new-file card beside inline hunks without inventing an owner-document update", () => {
    renderApp(
      documentEditor("Original.\n", [
        suggestion({
          id: "ordinary-hunk",
          beforeLines: "Original.\n",
          afterLines: "Revised.\n",
          anchorStart: 1,
        }),
        suggestion({
          id: "reviewer-doc",
          kind: "new_file",
          path: "docs/reviewer/new.md",
          beforeLines: null,
          afterLines: "# Reviewer document\n",
          anchorStart: null,
        }),
      ]),
    );
    const card = screen.getByTestId("review-suggestion-reviewer-doc");
    expect(card.querySelector("pre")).toHaveTextContent("# Reviewer document");
    expect(card).toHaveTextContent("docs/reviewer/new.md");
    expect(
      screen.queryByTestId("spec-suggestion-card-reviewer-doc"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("spec-suggestion-preview-reviewer-doc"),
    ).not.toBeInTheDocument();
    expect(
      screen.getByTestId("spec-suggestion-preview-ordinary-hunk"),
    ).toHaveTextContent("Revised.");
  });

  it("retains existing comment-card placement and canonical content beside text suggestions", () => {
    renderApp(
      documentEditor("First.\n\nSecond.\n", [
        suggestion({
          id: "text-edit",
          anchorStart: 1,
          beforeLines: "First.\n",
          afterLines: "Revised.\n",
        }),
        suggestion({
          id: "comment",
          kind: "comment",
          anchorStart: 3,
          beforeLines: "Second.\n",
          afterLines: null,
          note: "Existing discussion.",
        }),
      ]),
    );
    // The comment's card is in the margin, not in its block, which keeps
    // its canonical text.
    const comment = screen.getByTestId("review-suggestion-comment");
    expect(screen.getByTestId("document-margin")).toContainElement(comment);
    expect(comment.closest("[data-block-index]")).toBeNull();
    const block = document.querySelector('[data-block-index="1"]')!;
    expect(block).toHaveTextContent("Second.");
    expect(block).not.toContainElement(comment);
    expect(
      screen.getByTestId("spec-suggestion-preview-text-edit"),
    ).toBeInTheDocument();
  });

  it("lists suggestions with their lines when there are no line rows (textarea fallback)", () => {
    renderApp(
      <FallbackSuggestionList
        overlay={overlay([
          suggestion({ id: "sug-1", anchorStart: 3 }),
          suggestion({ id: "sug-2", anchorStart: 8, note: "Off by one." }),
        ])}
        content={"a\nb\nreturn 1;\nc\n"}
      />,
    );
    const list = screen.getByTestId("suggestion-fallback-list");
    expect(list).toHaveTextContent("2 suggestions in this file");
    expect(
      within(list).getByTestId("suggestion-fallback-sug-1"),
    ).toHaveTextContent("Line 3");
    expect(
      within(list).getByTestId("review-suggestion-accept-sug-1"),
    ).toBeInTheDocument();
    expect(within(list).getByText("Off by one.")).toBeInTheDocument();
  });

  it("shows a card kept from an earlier commit on its own file only", async () => {
    server.use(
      http.get(`*/api/v1/workspaces/${SOURCE_ID}/review/suggestions`, () =>
        HttpResponse.json({ data: { suggestions: [] } }),
      ),
    );
    // The page's owner holds a reply to a suggestion on src/main.rs, with
    // the record it was last listed with; a commit has taken it out.
    const store = createComposerStore();
    const subject = { kind: "suggestion" as const, id: "sug-kept" };
    store.set("suggestion/sug-kept/reply", "Why?", {
      subject,
      active: true,
      remove: false,
    });
    store.setRecord(subject, suggestion({ id: "sug-kept" }));
    const keep: ReadonlySet<string> = new Set(["sug-kept"]);
    const Probe: React.FC<{ path: string }> = ({ path }) => {
      const list = useFileSuggestions(SOURCE_ID, "repo-api", path, true, keep);
      return (
        <span data-testid={`probe-${path}`}>
          {list.suggestions.map((row) => row.id).join(",") || "none"}
        </span>
      );
    };
    renderApp(
      <ComposerStoreContext.Provider value={store}>
        <Probe path="src/main.rs" />
        <Probe path="src/other.rs" />
      </ComposerStoreContext.Provider>,
    );
    await waitFor(() =>
      expect(screen.getByTestId("probe-src/main.rs")).toHaveTextContent(
        "sug-kept",
      ),
    );
    expect(screen.getByTestId("probe-src/other.rs")).toHaveTextContent(
      /^none$/,
    );
  });

  it("shows a file's open rows of the current commit, and a decided one still being answered", async () => {
    const urls: string[] = [];
    server.use(
      http.get(
        `*/api/v1/workspaces/${SOURCE_ID}/review/suggestions`,
        ({ request }) => {
          urls.push(request.url);
          return HttpResponse.json({
            data: {
              suggestions: [
                suggestion({ id: "sug-open" }),
                suggestion({ id: "sug-answered", status: "rejected" }),
                suggestion({ id: "sug-done", status: "accepted" }),
              ],
            },
          });
        },
      ),
    );
    const Probe: React.FC<{ keep?: ReadonlySet<string> }> = ({ keep }) => {
      const list = useFileSuggestions(
        SOURCE_ID,
        "repo-api",
        "src/main.rs",
        true,
        keep,
      );
      return (
        <span data-testid="probe">
          {list.suggestions.map((row) => row.id).join(",")}
        </span>
      );
    };
    const Harness: React.FC = () => {
      const [keep, setKeep] = React.useState<ReadonlySet<string>>();
      return (
        <>
          <button
            type="button"
            onClick={() => setKeep(new Set(["sug-answered"]))}
          >
            Reply box busy
          </button>
          <Probe keep={keep} />
        </>
      );
    };
    renderApp(<Harness />);
    await waitFor(() =>
      expect(screen.getByTestId("probe")).toHaveTextContent("sug-open"),
    );
    expect(screen.getByTestId("probe")).toHaveTextContent(/^sug-open$/);
    const params = new URL(urls[0]).searchParams;
    expect(params.get("currentOnly")).toBe("true");
    expect(params.get("path")).toBe("src/main.rs");
    // A card whose reply box is busy stays, with its decision.
    fireEvent.click(screen.getByRole("button", { name: "Reply box busy" }));
    expect(screen.getByTestId("probe")).toHaveTextContent(
      /^sug-open,sug-answered$/,
    );
  });

  it("says on a review copy that the cards are the source's published proposals", () => {
    renderApp(
      <FallbackSuggestionList
        overlay={overlay([suggestion({ id: "sug-1", anchorStart: 3 })], {
          viewerOwnsWorkspace: false,
          ownerVmRunning: false,
          sourceWritable: true,
          onCopyOf: "kaihui-test3",
        })}
        content={"a\nb\nreturn 1;\nc\n"}
      />,
    );
    expect(screen.getByTestId("copy-file-proposals-hint")).toHaveTextContent(
      "Published proposals on kaihui-test3 for this file, from every reviewer",
    );
    // A copy never accepts: the card is read-only.
    expect(screen.queryByTestId("review-suggestion-accept-sug-1")).toBeNull();
  });

  it("renders document block suggestions as read-only rows", () => {
    renderApp(
      <BlockSuggestions
        overlay={overlay([suggestion({ id: "sug-9", note: "Reword." })], {
          viewerOwnsWorkspace: false,
        })}
        suggestions={[suggestion({ id: "sug-9", note: "Reword." })]}
      />,
    );
    const block = screen.getByTestId("block-suggestions");
    expect(block).toHaveTextContent("Reword.");
    expect(block).toHaveAttribute("contenteditable", "false");
    expect(
      screen.queryByTestId("review-suggestion-accept-sug-9"),
    ).not.toBeInTheDocument();
  });
});
