import React from "react";
import { act, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { renderApp } from "../../test/render";
import { reviewEditFixture } from "../../test/reviewEdits";
import { sha256Utf8 } from "../../lib/commentAnchors";
import { expandReviewSuggestionEdits } from "../../lib/reviewSuggestionEdits";
import type { ReviewSuggestion } from "../../lib/types";
import {
  MonacoSuggestionZones,
  resolveDisplayAnchor,
  suggestionZoneAfterLine,
  type SuggestionOverlayProps,
} from "./ReviewSuggestionOverlay";

function suggestion(overrides: Partial<ReviewSuggestion>): ReviewSuggestion {
  return {
    id: "b-parent",
    publicationId: "publication",
    findingId: null,
    repoId: "repo",
    path: "docs/spec.md",
    kind: "hunk",
    beforeLines: "The red fox quiet.\n",
    afterLines: "The blue fox sleepy.\n",
    anchorStart: 1,
    contentHash: null,
    anchorContext: null,
    authorKind: "human",
    authorLabel: "alex",
    note: "",
    commentId: null,
    status: "open",
    statusBy: null,
    statusAt: null,
    supersedes: null,
    adoptedFrom: null,
    createdAt: "2026-10-01T09:08:00.000Z",
    ...overrides,
  };
}

function monacoHost(content: string) {
  const zones: Array<{
    id: string;
    zone: { afterLineNumber: number; domNode: HTMLElement };
  }> = [];
  let nextId = 0;
  const removed = vi.fn();
  const editor = {
    getModel: () => ({ getValue: () => content }),
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
          const id = `zone-${nextId++}`;
          document.body.appendChild(zone.domNode);
          zones.push({ id, zone });
          return id;
        },
        removeZone: (id) => {
          removed(id);
          const index = zones.findIndex((zone) => zone.id === id);
          if (index >= 0) {
            zones[index].zone.domNode.remove();
            zones.splice(index, 1);
          }
        },
        layoutZone: () => undefined,
      }),
  };
  return { editor, zones, removed };
}

function mountSuggestions(
  content: string,
  initial: ReviewSuggestion[],
  selectedSource?: SuggestionOverlayProps["selectedSource"],
) {
  const host = monacoHost(content);
  let update: (rows: ReviewSuggestion[]) => void = () => undefined;
  const Host = () => {
    const [rows, setRows] = React.useState(initial);
    React.useEffect(() => {
      update = setRows;
    }, []);
    return (
      <MonacoSuggestionZones
        editor={host.editor}
        mountVersion={1}
        overlay={{
          workspaceId: "source",
          suggestions: rows,
          viewerOwnsWorkspace: true,
          viewerUserId: "owner",
          ownerVmRunning: true,
          sourceWritable: true,
          selectedSource,
        }}
      />
    );
  };
  renderApp(<Host />);
  return { ...host, update: (rows: ReviewSuggestion[]) => update(rows) };
}

describe("Monaco suggestion placement metadata refresh", () => {
  it.each(["bound", "absent", "stale"] as const)(
    "moves the same card and retains its draft when only its source frame changes with %s digest proof",
    async (proofKind) => {
      const captured = "The red fox quiet.\n";
      const projected = "The tan fox quiet.\n";
      const live = `prefix\n${projected}`;
      const capturedHash = await sha256Utf8(captured);
      const initial = suggestion({
        beforeLines: captured,
        expectedBeforeLines: captured,
        contentHash: capturedHash,
        edits: [
          reviewEditFixture("red", captured, 4, 7, "blue", {
            status: "rejected",
          }),
          reviewEditFixture("quiet", captured, 12, 17, "sleepy"),
        ],
      });
      const [first] = expandReviewSuggestionEdits([initial]).filter(
        (row) => row.status === "open",
      );
      const [updated] = expandReviewSuggestionEdits([
        { ...initial, expectedBeforeLines: projected },
      ]).filter((row) => row.status === "open");
      expect(updated.id).toBe(first.id);
      expect(updated.reviewEdit?.currentStart).toBe(
        first.reviewEdit?.currentStart,
      );
      expect(updated.reviewEdit?.currentEnd).toBe(first.reviewEdit?.currentEnd);
      expect(updated.reviewEdit?.sourceConflict).toBe(
        first.reviewEdit?.sourceConflict,
      );
      expect(resolveDisplayAnchor(first, live).state).toBe("lost");
      expect(suggestionZoneAfterLine(updated, live)).toBe(2);
      const [accepted] = expandReviewSuggestionEdits([
        suggestion({
          id: "kept-a",
          beforeLines: captured,
          expectedBeforeLines: projected,
          afterLines: projected,
          contentHash: capturedHash,
          status: "accepted",
          edits: [
            reviewEditFixture("winner", captured, 4, 7, "tan", {
              status: "accepted",
              writtenHash: await sha256Utf8(live),
            }),
          ],
        }),
      ]);
      const proof =
        proofKind === "bound"
          ? { content: live, digest: await sha256Utf8(live) }
          : proofKind === "stale"
            ? { content: captured, digest: capturedHash }
            : undefined;
      const { zones, update, removed } = mountSuggestions(
        live,
        [accepted, first],
        proof,
      );
      const acceptedCard = await screen.findByTestId(
        "review-suggestion-kept-a:winner",
      );
      const card = screen.getByTestId("review-suggestion-b-parent:quiet");
      expect(acceptedCard).toHaveTextContent("Accepted");
      expect(zones).toHaveLength(2);
      expect(zones[1].zone.afterLineNumber).toBe(1);
      const originalZone = zones[1];
      const user = userEvent.setup();
      await user.click(
        screen.getByTestId("review-suggestion-edit-toggle-b-parent:quiet"),
      );
      const draft = screen.getByTestId("review-suggestion-edit-b-parent:quiet");
      await user.clear(draft);
      await user.type(draft, "peaceful");

      act(() => update([accepted, updated]));
      await waitFor(() => expect(zones[1].zone.afterLineNumber).toBe(2));
      expect(zones).toHaveLength(2);
      expect(zones[1]).toBe(originalZone);
      expect(removed).not.toHaveBeenCalled();
      expect(screen.getByTestId("review-suggestion-kept-a:winner")).toBe(
        acceptedCard,
      );
      expect(screen.getByTestId("review-suggestion-b-parent:quiet")).toBe(card);
      expect(screen.getByTestId("review-suggestion-edit-b-parent:quiet")).toBe(
        draft,
      );
      expect(draft).toHaveValue("peaceful");
      expect(
        screen.getByTestId("suggestion-anchor-note-b-parent:quiet"),
      ).toHaveTextContent("Moved from line 1");
      expect(
        card.querySelector('[data-testid="review-hunk-diff"]'),
      ).not.toBeNull();
    },
  );

  it("relocates an ambiguous same-bounds child when new surrounding context proves one occurrence", async () => {
    const source = "return 1;\n";
    const live = "prefix\nreturn 1;\nbridge\nreturn 1;\ntail\n";
    const [initial] = expandReviewSuggestionEdits([
      suggestion({
        beforeLines: source,
        afterLines: "return 0;\n",
        contentHash: await sha256Utf8(source),
        edits: [reviewEditFixture("return", source, 7, 8, "0")],
      }),
    ]);
    const updated = {
      ...initial,
      anchorContext: JSON.stringify({
        version: 2,
        leading: ["bridge"],
        trailing: ["tail"],
      }),
    };
    expect(resolveDisplayAnchor(initial, live).state).toBe("ambiguous");
    expect(resolveDisplayAnchor(updated, live)).toEqual({
      line: 4,
      state: "moved",
    });
    const { zones, update, removed } = mountSuggestions(live, [initial], {
      content: live,
      digest: await sha256Utf8(live),
    });
    const card = await screen.findByTestId("review-suggestion-b-parent:return");
    const originalZone = zones[0];
    expect(originalZone.zone.afterLineNumber).toBe(1);
    expect(
      screen.getByTestId("suggestion-anchor-note-b-parent:return"),
    ).toHaveTextContent("occur several times");
    act(() => update([updated]));
    await waitFor(() => expect(zones[0].zone.afterLineNumber).toBe(4));
    expect(zones[0]).toBe(originalZone);
    expect(removed).not.toHaveBeenCalled();
    expect(screen.getByTestId("review-suggestion-b-parent:return")).toBe(card);
    expect(
      screen.getByTestId("suggestion-anchor-note-b-parent:return"),
    ).toHaveTextContent("Moved from line 1");
  });

  it.each(["bound", "absent", "stale"] as const)(
    "changes captured-basis placement only when refreshed metadata agrees with %s raw proof",
    async (proofKind) => {
      const captured = "prefix\nold text.\n";
      const capturedHash = await sha256Utf8(captured);
      const [initial] = expandReviewSuggestionEdits([
        suggestion({
          beforeLines: captured,
          expectedBeforeLines: "historical\nextra\nold text.\n",
          contentHash: await sha256Utf8("Different.\n"),
          edits: [
            reviewEditFixture("restored", captured, 7, 10, "new", {
              sourceConflict: true,
              currentStart: 17,
              currentEnd: 20,
            }),
          ],
        }),
      ]);
      const updated = { ...initial, contentHash: capturedHash };
      const proof =
        proofKind === "bound"
          ? { content: captured, digest: capturedHash }
          : proofKind === "stale"
            ? { content: "Different.\n", digest: capturedHash }
            : undefined;
      const { zones, update, removed } = mountSuggestions(
        captured,
        [initial],
        proof,
      );
      const card = await screen.findByTestId(
        "review-suggestion-b-parent:restored",
      );
      const originalZone = zones[0];
      expect(originalZone.zone.afterLineNumber).toBe(0);
      act(() => update([updated]));
      const expectedLine = proofKind === "bound" ? 2 : 0;
      await waitFor(() =>
        expect(zones[0].zone.afterLineNumber).toBe(expectedLine),
      );
      expect(zones[0]).toBe(originalZone);
      expect(removed).not.toHaveBeenCalled();
      expect(screen.getByTestId("review-suggestion-b-parent:restored")).toBe(
        card,
      );
      if (proofKind === "bound") {
        expect(
          screen.queryByTestId("suggestion-anchor-note-b-parent:restored"),
        ).toBeNull();
      } else {
        expect(
          screen.getByTestId("suggestion-anchor-note-b-parent:restored"),
        ).toHaveTextContent("source needs attention");
      }
      expect(updated.reviewEdit?.sourceConflict).toBe(true);
    },
  );
});
