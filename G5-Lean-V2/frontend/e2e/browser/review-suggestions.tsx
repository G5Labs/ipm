import React from "react";
import { createRoot } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import { ComposerOwner } from "../../src/components/comments/ComposerOwner";
import { SideThreads } from "../../src/components/comments/SideThreads";
import { DocumentPreview } from "../../src/components/files/DocumentPreview";
import { FallbackSuggestionList } from "../../src/components/workspace/ReviewSuggestionOverlay";
import type { ReviewSuggestion } from "../../src/lib/types";
import { reviewEditFixture } from "../../src/test/reviewEdits";
import "../../src/index.css";

document.documentElement.dataset.theme = "alpine-graphite";
document.documentElement.dataset.mode = "light";

const mode = new URLSearchParams(location.search).get("view");
const content =
  mode === "mixed"
    ? "Intro.\n\nAlpha middle Omega ending Final.\n\nEnd.\n"
    : mode === "overlap"
      ? "Intro.\n\nhuman-provided\n\nEnd.\n"
      : "Intro.\n\nLGTM\n\nEnd.\n";
const proposal: ReviewSuggestion = {
  id: "whitespace",
  publicationId: "publication",
  findingId: null,
  repoId: "repo",
  path: "docs/spec.md",
  kind: "hunk",
  beforeLines: content,
  afterLines: content.replace("LGTM", " LGTM"),
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
  createdAt: "2026-10-15T00:00:00Z",
  edits: [reviewEditFixture("leading-space", content, 8, 8, " ")],
};
const overlay = {
  workspaceId: "source",
  suggestions:
    mode === "mixed"
      ? [
          ["a1", "Alpha", " North"],
          ["a2", "Alpha", " South"],
          ["b1", "Omega", "East"],
          ["b2", "Omega", "West"],
          ["solo", "Final", "Done"],
        ].map(([id, word, replacement]) => ({
          ...proposal,
          id: `mixed-${id}`,
          authorLabel: id,
          edits: [
            reviewEditFixture(
              `mixed-${id}-edit`,
              content,
              content.indexOf(word),
              content.indexOf(word) + word.length,
              replacement,
            ),
          ],
        }))
      : mode === "overlap"
        ? ["zcl", "kaihui-zheng", "alex", "mira", "sam", "lee"].map(
            (authorLabel, i) => ({
              ...proposal,
              id: `alternative-${i}`,
              authorLabel,
              edits: [
                reviewEditFixture(
                  `alternative-edit-${i}`,
                  content,
                  8,
                  i === 0 ? 22 : 13,
                  i === 0
                    ? " an inplace edit with leading space"
                    : i === 1
                      ? "kaihui"
                      : `person-${i}`,
                ),
              ],
            }),
          )
        : [proposal],
  viewerOwnsWorkspace: true,
  viewerUserId: "owner",
  ownerVmRunning: true,
  sourceWritable: true,
};
const sidebarOverlay = {
  ...overlay,
  suggestions: Array.from({ length: 14 }, (_, i) => ({
    ...proposal,
    id: `sidebar-${i}`,
    commentId: `sidebar-thread-${i}`,
    edits: undefined,
    beforeLines: "LGTM\n",
    afterLines: ` LGTM ${i}\n`,
    anchorStart: 3,
  })),
};

createRoot(document.getElementById("root")!).render(
  <QueryClientProvider
    client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
  >
    <MemoryRouter>
      <ComposerOwner scope="browser-review-fixture">
        <div
          data-testid="available-parent"
          style={{ height: "calc(100vh - 80px)", margin: 40 }}
        >
          {mode === "sidebar" ? (
            <SideThreads
              title="Comments on this file"
              testId="file-sidebar"
              comments={[]}
              content={content}
              notice={null}
              suggestionOverlay={sidebarOverlay}
            >
              <div data-testid="file-document" style={{ height: 2000 }}>
                File content
              </div>
            </SideThreads>
          ) : mode === "below-editor" ? (
            <FallbackSuggestionList
              content={content}
              overlay={sidebarOverlay}
            />
          ) : (
            <DocumentPreview
              filePath="docs/spec.md"
              workspaceId=""
              repoId=""
              content={content}
              canAnswer
              onRewrite={() => true}
              onNavigate={() => {}}
              suggestionOverlay={overlay}
            />
          )}
        </div>
      </ComposerOwner>
    </MemoryRouter>
  </QueryClientProvider>,
);
