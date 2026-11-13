import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor, within } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it, vi } from "vitest";
import { server } from "../../test/msw/server";
import { DocumentPreview } from "../files/DocumentPreview";
import { OntologyChangesView } from "./OntologyChangesView";

// One shared comparison presentation for per-file Preview and All changes
// (g5.documents.ontology.statement-changes, g5.documents.ontology.all-changes).
const path = ".g5/ontology/root.g5.md";
const doc = (statements: [string, string][]) =>
  `# Root\n\n${statements
    .map(([id, body]) => `<a id="${id}"></a>\n[I · ${id}]\n\n${body}\n\n`)
    .join("")}## Metadata\n\n${statements
    .map(([id]) => `### ${id}\n\n- agent\n\n`)
    .join("")}`;
const current = doc([
  ["a", "Current ![Diagram](diagram.png)"],
  ["c", "Added body"],
]);
const baseline = doc([
  ["a", "Old body"],
  ["gone", "Removed body"],
]);
const png = "iVBORw0KGgo=";

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
    http.get("*/api/v1/workspaces/ws/files/*", ({ request }) => {
      const url = new URL(request.url);
      return url.pathname.endsWith("/.g5/ontology/diagram.png") &&
        url.searchParams.get("repoId") === "repo"
        ? HttpResponse.json({
            data: {
              path: ".g5/ontology/diagram.png",
              content: png,
              encoding: "base64",
              size: 8,
              hash: "h",
              source: "live",
              repoOid: null,
              snapshotAt: null,
            },
          })
        : HttpResponse.json({ error: "not found" }, { status: 404 });
    }),
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

/** The class structure of a row from its pair down to its label and frame. */
function rowShape(row: HTMLElement) {
  return {
    pair: row.parentElement!.className,
    row: row.className,
    first: row.firstElementChild!.className,
    // Only the pair carries the bullet: neither the row nor its frame does.
    rowBullet: !!row.closest(".ontology-statement-bullet"),
  };
}

describe.each(["file", "all"] as const)(
  "shared comparison pair (%s)",
  (view) => {
    it("gives each pair one bullet, with Before directly under its Current row", async () => {
      serve();
      mount(view);
      const modified = await screen.findByRole("region", {
        name: "Modified statement a",
      });
      const before = await screen.findByRole("region", {
        name: "Before statement a",
      });
      const deleted = screen.getByRole("region", {
        name: "Deleted statement gone",
      });
      const added = screen.getByRole("region", {
        name: "Added statement c",
      });
      expect(rowShape(modified)).toEqual({
        pair: "ontology-comparison-pair",
        row: "ontology-comparison-row ontology-comparison-current",
        first: "ontology-comparison-label",
        rowBullet: false,
      });
      expect(rowShape(added)).toEqual(rowShape(modified));
      expect(rowShape(before)).toEqual({
        pair: "ontology-comparison-pair",
        row: "ontology-comparison-row ontology-comparison-before",
        first: "ontology-comparison-label",
        rowBullet: false,
      });
      expect(before.parentElement).toBe(modified.parentElement);
      expect(modified.nextElementSibling).toBe(before);
      expect(rowShape(deleted)).toEqual({
        ...rowShape(before),
        pair: "ontology-comparison-pair ontology-comparison-deleted",
      });
      expect(deleted).toHaveAttribute("data-ontology-deleted-id", "gone");
      expect(deleted).toHaveAttribute("data-ontology-before", "true");
      expect(modified.firstElementChild).toHaveTextContent("Modified");
      expect(before.firstElementChild).toHaveTextContent("Before");
      expect(deleted.firstElementChild).toHaveTextContent("Deleted");
      expect(added.firstElementChild).toHaveTextContent("Added");
    });

    it("loads a current statement's relative image from the file's repository", async () => {
      serve();
      mount(view);
      const modified = await screen.findByRole("region", {
        name: "Modified statement a",
      });
      // Only the file's own path and repository serve the image, which
      // rendered Markdown shows through an object URL.
      await waitFor(() =>
        expect(
          within(modified).getByRole("img", { name: "Diagram" }),
        ).toHaveAttribute("src", "blob:mock-url"),
      );
    });
  },
);
