import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
  act,
} from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { http, HttpResponse } from "msw";
import { server } from "../../test/msw/server";
import {
  OntologyTree,
  boundOntologyTrees,
  ontologyEntries,
} from "./OntologyTree";
import type { BranchSummary, FileTreeEntry } from "../../lib/types";
const file = (name: string): FileTreeEntry => ({
  name,
  path: `.g5/ontology/${name}`,
  type: "file",
  size: 10,
});
const branch = (repoId: string) =>
  ({ repoId, repoName: `Repository ${repoId}` }) as BranchSummary;
function mount(repos = ["one"], searchQuery = "") {
  const select = vi.fn();
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  const view = render(
    <QueryClientProvider client={client}>
      <OntologyTree
        workspaceId="ws"
        branches={repos.map(branch)}
        onSelectFile={select}
        searchQuery={searchQuery}
      />
    </QueryClientProvider>,
  );
  return { ...view, select, client };
}
function respond(entries: FileTreeEntry[], extra = {}) {
  return HttpResponse.json({
    data: { entries, source: "live", truncated: false, ...extra },
  });
}
describe("file-backed ontology discovery", () => {
  it("uses Files requests, flattens one complete repository and selects its exact path", async () => {
    server.use(
      http.get(
        "*/api/v1/workspaces/ws/repos/:repo/files",
        ({ request, params }) => {
          const url = new URL(request.url);
          expect(url.searchParams.get("path")).toBe(".g5/ontology");
          expect(url.searchParams.get("depth")).toBe("-1");
          return respond(
            params.repo === "one"
              ? [file("root.g5.md"), file("ignored.txt")]
              : [],
          );
        },
      ),
    );
    const { select } = mount(["one", "two"]);
    fireEvent.click(await screen.findByRole("treeitem", { name: "root" }));
    await waitFor(() =>
      expect(screen.queryByText("Repository one")).toBeNull(),
    );
    expect(screen.queryByText("ignored.txt")).toBeNull();
    expect(select).toHaveBeenCalledWith("one", ".g5/ontology/root.g5.md");
  });
  it("keeps same-path files distinct across repositories", async () => {
    server.use(
      http.get("*/api/v1/workspaces/ws/repos/:repo/files", () =>
        respond([file("root.g5.md")]),
      ),
    );
    const { select } = mount(["one", "two"]);
    await screen.findByText("Repository two");
    fireEvent.click(
      within(
        screen.getByRole("tree", { name: "Ontology files in Repository two" }),
      ).getByText("root"),
    );
    expect(select).toHaveBeenCalledWith("two", ".g5/ontology/root.g5.md");
  });
  it("preserves folder expansion and replaces stale children on normal Files invalidation", async () => {
    let child = "old.g5.md";
    server.use(
      http.get("*/api/v1/workspaces/ws/repos/:repo/files", () =>
        respond([
          {
            name: "features",
            path: ".g5/ontology/features",
            type: "directory",
            size: null,
            children: [file(child)],
          },
        ]),
      ),
    );
    const { client, select } = mount();
    const folder = await screen.findByRole("treeitem", { name: "features" });
    expect(folder).toHaveAttribute("aria-expanded", "false");
    child = "new.g5.md";
    await act(async () => {
      await client.invalidateQueries({ queryKey: ["file-tree", "ws"] });
    });
    expect(folder).toHaveAttribute("aria-expanded", "false");
    fireEvent.click(folder);
    expect(await screen.findByText("new")).toBeVisible();
    expect(screen.queryByText("old")).toBeNull();
    expect(select).not.toHaveBeenCalled();
  });
  it("keeps incomplete discovery visible during search without claiming an empty tree", async () => {
    server.use(
      http.get("*/api/v1/workspaces/ws/repos/:repo/files", () =>
        respond([], { truncated: true }),
      ),
    );
    mount(["one"], "missing");
    expect(
      await screen.findByText("Ontology listing is incomplete"),
    ).toBeVisible();
    expect(screen.queryByText("No ontology files")).toBeNull();
  });
  it("clears stale files on failed authorization and shows the failure", async () => {
    let denied = false;
    server.use(
      http.get("*/api/v1/workspaces/ws/repos/:repo/files", () =>
        denied
          ? HttpResponse.json(
              { error: { code: "FORBIDDEN", message: "Access revoked" } },
              { status: 403 },
            )
          : respond([file("root.g5.md")]),
      ),
    );
    const { client } = mount();
    await screen.findByText("root");
    denied = true;
    await act(async () => {
      await client.invalidateQueries({ queryKey: ["file-tree", "ws"] });
    });
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Could not read ontology",
    );
    expect(screen.queryByText("root")).toBeNull();
    expect(screen.queryByText("No ontology files")).toBeNull();
  });
  it("lists only *.g5.md pages; other Markdown under .g5/ontology is an ordinary file", () => {
    expect(
      ontologyEntries([
        file("root.g5.md"),
        file("notes.md"),
        {
          name: "features",
          path: ".g5/ontology/features",
          type: "directory",
          size: null,
          children: [file("readme.md")],
        },
      ]).map((entry) => entry.path),
    ).toEqual([".g5/ontology/root.g5.md"]);
  });
  it("shares entry and byte display limits across repositories", () => {
    const trees = [[file("first.g5.md")], [file("second.g5.md")]];
    expect(boundOntologyTrees(trees, 1)).toMatchObject({
      truncated: true,
      trees: [trees[0], []],
    });
    expect(boundOntologyTrees(trees, 100, 1).truncated).toBe(true);
  });
});

describe("findings files (g5.documents.findings.files)", () => {
  function serveFindings(present: string[]) {
    server.use(
      http.get("*/api/v1/workspaces/ws/repos/:repo/files", () =>
        respond([file("root.g5.md")]),
      ),
      http.get("*/api/v1/workspaces/ws/files/*", ({ request }) => {
        const path = decodeURIComponent(
          new URL(request.url).pathname.split("/files/")[1],
        );
        return present.includes(path)
          ? HttpResponse.json({
              data: {
                path,
                content: "# Bugs\n",
                size: 7,
                encoding: "utf-8",
                hash: "h",
                source: "live",
                repoOid: null,
                snapshotAt: null,
              },
            })
          : HttpResponse.json(
              { error: { code: "NOT_FOUND", message: "missing" } },
              { status: 404 },
            );
      }),
    );
  }

  it("lists the findings files that exist after the ontology files, and opens them", async () => {
    serveFindings([".g5/TLA_verification/bugs_issues.md"]);
    const { select } = mount();
    const item = await screen.findByRole("treeitem", {
      name: "Bugs and issues",
    });
    expect(screen.queryByText("Design simplifications")).toBeNull();
    const items = screen.getAllByRole("treeitem").map((i) => i.textContent);
    expect(items).toEqual(["root", "Bugs and issues"]);
    fireEvent.click(item);
    expect(select).toHaveBeenCalledWith(
      "one",
      ".g5/TLA_verification/bugs_issues.md",
    );
  });

  it("does not look for findings in a repository without an ontology", async () => {
    const reads: string[] = [];
    server.use(
      http.get("*/api/v1/workspaces/ws/repos/:repo/files", () => respond([])),
      http.get("*/api/v1/workspaces/ws/files/*", ({ request }) => {
        reads.push(new URL(request.url).pathname);
        return HttpResponse.json(
          { error: { code: "NOT_FOUND", message: "missing" } },
          { status: 404 },
        );
      }),
    );
    mount();
    expect(await screen.findByText("No ontology files")).toBeInTheDocument();
    expect(reads).toEqual([]);
  });

  it("matches search by name", async () => {
    serveFindings([
      ".g5/TLA_verification/bugs_issues.md",
      ".g5/TLA_verification/design_simplification.md",
    ]);
    mount(["one"], "simplif");
    expect(
      await screen.findByRole("treeitem", { name: "Design simplifications" }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("treeitem", { name: "Bugs and issues" }),
    ).toBeNull();
  });
});
