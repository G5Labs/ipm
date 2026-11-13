import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { server } from "../../test/msw/server";
import { renderApp } from "../../test/render";
import type { ReviewSuggestion } from "../../lib/types";
import { ontologyStatements } from "../../lib/ontologyComparison";
import { DocumentPreview } from "../files/DocumentPreview";
import { OntologyComparisonPair } from "./OntologyComparisonPair";
import { OntologyChangesView } from "./OntologyChangesView";
import {
  INSERTED_HIGHLIGHT,
  REMOVED_HIGHLIGHT,
  textBlocks,
} from "./useChangeHighlights";

// Change highlights over the shared comparison pair
// (g5.documents.ontology.change-highlights). jsdom has no CSS Custom
// Highlight API, so a minimal registry stands in for the browser's.
const path = ".g5/ontology/root.g5.md";
const doc = (statements: [string, string][]) =>
  `# Root\n\n${statements
    .map(([id, body]) => `<a id="${id}"></a>\n[I · ${id}]\n\n${body}\n\n`)
    .join("")}## Metadata\n\n${statements
    .map(([id]) => `### ${id}\n\n- agent\n\n`)
    .join("")}`;
const current = doc([
  [
    "a",
    "Agents push to the workspace branch. Only the owner may **force-push** to the branch. The server rejects stale leases with a clear error.",
  ],
  ["c", "Added body."],
]);
const baseline = doc([
  [
    "a",
    "Agents push to the workspace branch. Each push needs a fresh lease from the server. The server rejects stale leases.",
  ],
  ["gone", "Removed body."],
]);

class FakeHighlight extends Set<Range> {}
const registry = new Map<string, FakeHighlight>();
const marks = (name: string) =>
  [...(registry.get(name) ?? [])].map((r) => r.toString());

function installHighlights() {
  registry.clear();
  vi.stubGlobal("Highlight", FakeHighlight);
  vi.stubGlobal("CSS", { escape: CSS.escape, highlights: registry });
}

function serve() {
  server.use(
    http.get("*/api/v1/workspaces/ws/source-control/ontology-baseline", () =>
      HttpResponse.json({
        data: {
          revision: "rev",
          workspaceId: "ws",
          repoId: "repo",
          path,
          baselineCommit: "1234567890abcdef",
          sourceCommit: "head",
          parentCommit: "parent",
          content: baseline,
        },
      }),
    ),
    http.get("*/api/v1/workspaces/ws/source-control/ontology-changes", () =>
      HttpResponse.json({
        data: {
          revision: "rev",
          repositories: [
            {
              repoId: "repo",
              repoName: "repo",
              sourceCommit: "head",
              parentCommit: "parent",
              baselineCommit: "1234567890abcdef",
            },
          ],
          files: [{ repoId: "repo", path, source: "live", current, baseline }],
        },
      }),
    ),
  );
}

function mount(view: "file" | "all") {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      {view === "file" ? (
        <DocumentPreview
          workspaceId="ws"
          repoId="repo"
          filePath={path}
          content={current}
          onNavigate={vi.fn()}
        />
      ) : (
        <OntologyChangesView
          workspaceId="ws"
          onNavigate={vi.fn()}
          onOpenFile={vi.fn()}
        />
      )}
    </QueryClientProvider>,
  );
}

afterEach(() => {
  vi.unstubAllGlobals();
  registry.clear();
});

describe.each(["file", "all"] as const)("change highlights (%s)", (view) => {
  beforeEach(installHighlights);

  it("marks the replaced sentence whole and only the added words of the edited one", async () => {
    serve();
    mount(view);
    await screen.findByRole("region", { name: "Before statement a" });
    await waitFor(() =>
      expect(marks(INSERTED_HIGHLIGHT).sort()).toEqual([
        "Only the owner may force-push to the branch.",
        "with a clear error",
      ]),
    );
    // The Added and Deleted statements are never marked.
    expect(marks(REMOVED_HIGHLIGHT)).toEqual([
      "Each push needs a fresh lease from the server.",
    ]);
    // Before shows only the sentence that lost text, until asked for all.
    const before = screen.getByRole("region", { name: "Before statement a" });
    const excerpt = within(before).getByTestId("ontology-before-excerpt");
    expect(excerpt).toHaveTextContent(
      "… Each push needs a fresh lease from the server. …",
    );
    expect(
      [...excerpt.querySelectorAll(".ontology-excerpt-removed")].map(
        (el) => el.textContent,
      ),
    ).toEqual(["Each push needs a fresh lease from the server."]);
    const showFull = within(before).getByRole("button", {
      name: "Show full, statement a",
    });
    showFull.focus();
    fireEvent.click(showFull);
    const full = screen.getByRole("region", { name: "Before statement a" });
    expect(full).toHaveTextContent(
      "Agents push to the workspace branch. Each push needs a fresh lease from the server. The server rejects stale leases.",
    );
    expect(marks(REMOVED_HIGHLIGHT)).toEqual([
      "Each push needs a fresh lease from the server.",
    ]);
    // Show less is another button, at the bottom of the full row; focus
    // follows it there and back.
    const showLess = within(full).getByRole("button", {
      name: "Show less, statement a",
    });
    expect(showLess).toHaveAttribute("aria-expanded", "true");
    expect(showLess).toHaveFocus();
    fireEvent.click(showLess);
    expect(
      within(
        screen.getByRole("region", { name: "Before statement a" }),
      ).getByRole("button", { name: "Show full, statement a" }),
    ).toHaveFocus();
  });

  it("paints over the rendered text without changing it", async () => {
    // The statement bodies, without React's generated ids elsewhere; the
    // full Before row, which an excerpt may hide, is the one compared.
    const bodies = () =>
      [
        screen.getByRole("region", { name: "Modified statement a" }),
        document.querySelector("[data-before-source]")!,
      ].map(
        (row) =>
          row.querySelector('[data-testid="semantic-record-blocks"]')!
            .innerHTML,
      );
    serve();
    vi.unstubAllGlobals();
    const plain = mount(view);
    await screen.findByRole("region", { name: "Before statement a" });
    const html = bodies();
    plain.unmount();
    installHighlights();
    mount(view);
    await screen.findByRole("region", { name: "Before statement a" });
    await waitFor(() => expect(marks(REMOVED_HIGHLIGHT)).toHaveLength(1));
    expect(bodies()).toEqual(html);
  });
});

describe("change highlights beside a pending suggestion", () => {
  beforeEach(installHighlights);

  it("shows the pair without highlights while a suggestion's preview stands in for its text", async () => {
    serve();
    const covered = "The server rejects stale leases with a clear error.";
    const suggestion: ReviewSuggestion = {
      id: "review",
      publicationId: "publication",
      findingId: null,
      repoId: "repo",
      path,
      kind: "hunk",
      beforeLines: `${covered}\n`,
      afterLines: "The server refuses stale leases.\n",
      anchorStart: 0,
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
    };
    const content = current.replace(
      "The server rejects stale leases with a clear error.",
      `\n\n${covered}`,
    );
    suggestion.anchorStart = content.split("\n").indexOf(covered) + 1;
    renderApp(
      <DocumentPreview
        workspaceId="ws"
        repoId="repo"
        filePath={path}
        content={content}
        onNavigate={vi.fn()}
        suggestionOverlay={{
          workspaceId: "ws-source-1",
          suggestions: [suggestion],
          viewerOwnsWorkspace: true,
          viewerUserId: "owner",
          ownerVmRunning: true,
          sourceWritable: true,
        }}
      />,
    );
    const modified = await screen.findByRole("region", {
      name: "Modified statement a",
    });
    await waitFor(() =>
      expect(
        modified.querySelector("[data-review-preview]"),
      ).toBeInTheDocument(),
    );
    // The covered sentence is still in the file, so it is not "removed".
    expect(marks(REMOVED_HIGHLIGHT)).toEqual([]);
    expect(marks(INSERTED_HIGHLIGHT)).toEqual([]);
  });
});

describe("change highlights on one pair", () => {
  beforeEach(installHighlights);
  const statement = (body: string) => ontologyStatements(doc([["s", body]]))[0];
  const pair = (now: string, then: string, key = "s") => (
    <OntologyComparisonPair
      key={key}
      status="Modified"
      statementId={key}
      before={statement(then)}
    >
      <div data-testid="semantic-record-blocks">
        <p>{now}</p>
      </div>
    </OntologyComparisonPair>
  );

  it("follows the Current row's text as it changes", async () => {
    const view = render(
      pair("Retry five times. Then fail.", "Retry three times. Then fail."),
    );
    expect(marks(INSERTED_HIGHLIGHT)).toEqual(["five"]);
    view.rerender(
      pair("Retry seven times. Then fail.", "Retry three times. Then fail."),
    );
    await waitFor(() => expect(marks(INSERTED_HIGHLIGHT)).toEqual(["seven"]));
    expect(marks(REMOVED_HIGHLIGHT)).toEqual(["three"]);
  });

  it("hides Before when nothing was removed, behind Show before", () => {
    render(
      pair(
        "Retry three times. Then fail loudly.",
        "Retry three times. Then fail.",
      ),
    );
    expect(marks(INSERTED_HIGHLIGHT)).toEqual(["loudly"]);
    expect(screen.queryByRole("region", { name: "Before statement s" })).toBe(
      null,
    );
    const show = screen.getByRole("button", {
      name: "Show before, statement s",
    });
    expect(show).toHaveAttribute("aria-expanded", "false");
    expect(
      screen.getByRole("region", { name: "Modified statement s" }),
    ).toContainElement(show);
    fireEvent.click(show);
    const before = screen.getByRole("region", { name: "Before statement s" });
    expect(before).toHaveTextContent("Retry three times. Then fail.");
    // The same control, in the same place, hides it again.
    expect(show).toHaveAccessibleName("Hide before, statement s");
    expect(show).toHaveAttribute("aria-expanded", "true");
    expect(
      within(before).queryByTestId("ontology-comparison-toggle"),
    ).toBeNull();
    fireEvent.click(show);
    expect(screen.queryByRole("region", { name: "Before statement s" })).toBe(
      null,
    );
    // The marks stay on the hidden row, never on a remounted one.
    expect(marks(INSERTED_HIGHLIGHT)).toEqual(["loudly"]);
    // The hidden row carries no control of its own.
    expect(
      document.querySelectorAll('[data-testid="ontology-comparison-toggle"]'),
    ).toHaveLength(1);
  });

  it("shows Before whole, with no control, when every sentence lost text or without highlights", () => {
    render(pair("Retry times now.", "Retry three times now."));
    const before = screen.getByRole("region", { name: "Before statement s" });
    expect(marks(REMOVED_HIGHLIGHT)).toEqual(["three"]);
    expect(before).toHaveTextContent("Retry three times now.");
    expect(
      within(before).queryByRole("button", { name: /^(Show|Hide)/ }),
    ).toBeNull();
    // A replaced statement falls back to the full pair. (The helper's Before
    // rows are all named after statement s.)
    render(pair("Something else entirely.", "Retry three times.", "t"));
    expect(
      screen.getAllByRole("region", { name: "Before statement s" })[1],
    ).toHaveTextContent("Retry three times.");
    expect(screen.queryByRole("button", { name: /statement t/ })).toBeNull();
  });

  it("keeps each pair's ranges when another pair unmounts", () => {
    const one = render(
      pair("Keep alpha now. Done.", "Keep beta now. Done.", "x"),
    );
    render(pair("Keep gamma now. Done.", "Keep delta now. Done.", "y"));
    expect(marks(INSERTED_HIGHLIGHT).sort()).toEqual(["alpha", "gamma"]);
    one.unmount();
    expect(marks(INSERTED_HIGHLIGHT)).toEqual(["gamma"]);
    expect(marks(REMOVED_HIGHLIGHT)).toEqual(["delta"]);
  });

  it("marks nothing for an Added statement or without browser support", () => {
    render(
      <OntologyComparisonPair status="Added" statementId="n">
        <div data-testid="semantic-record-blocks">New text.</div>
      </OntologyComparisonPair>,
    );
    expect(marks(INSERTED_HIGHLIGHT)).toEqual([]);
    vi.unstubAllGlobals();
    expect(() =>
      render(
        pair("Retry five times. Then fail.", "Retry three times. Then fail."),
      ),
    ).not.toThrow();
  });
});

describe("textBlocks", () => {
  it("reads one entry per block and skips text that is not the statement's", () => {
    const root = document.createElement("div");
    root.innerHTML =
      "<p>First <em>line</em><br>second <span data-change-highlight-ignore>(edited)</span></p>" +
      '<ul><li>item</li></ul><div data-review-suggestions="">Review</div>' +
      '<span class="sr-only">hidden</span><button>Edit</button>';
    expect(textBlocks(root).map((b) => b.text)).toEqual([
      "First line",
      "second ",
      "item",
    ]);
  });
});
