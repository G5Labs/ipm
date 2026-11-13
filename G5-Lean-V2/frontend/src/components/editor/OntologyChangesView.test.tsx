import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it, vi } from "vitest";
import { server } from "../../test/msw/server";
import {
  selectOntologyChanges,
  type OntologyChangesResponse,
} from "../../hooks/useOntologyChanges";
import {
  OntologyChangesButton,
  OntologyChangesView,
} from "./OntologyChangesView";

const doc = (body: string, metadata = "human-provided") =>
  `# Root\n\n<a id="a"></a>\n[I · a]\n\n${body}\n\n## Metadata\n\n### a\n\n- ${metadata}\n`;
const path = ".g5/ontology/root.g5.md";
const endpoint =
  "*/api/v1/workspaces/:workspace/source-control/ontology-changes";
const repository = (repoId: string) => ({
  repoId,
  repoName: repoId,
  baselineCommit: "baseline1234",
  parentCommit: "parent",
  sourceCommit: "source",
});
function fixture(): OntologyChangesResponse {
  return {
    revision: "revision",
    repositories: [repository("code"), repository("docs")],
    files: [
      {
        repoId: "code",
        path,
        source: "live",
        current: doc("Saved edit"),
        baseline: doc("Original"),
      },
      {
        repoId: "docs",
        path,
        source: "committed",
        current: doc("Added in another repo"),
        baseline: null,
      },
      {
        repoId: "code",
        path: ".g5/ontology/deleted.md",
        source: "live",
        current: null,
        baseline: doc("Removed file body"),
      },
      {
        repoId: "code",
        path: ".g5/ontology/metadata.md",
        source: "live",
        current: doc("Unchanged body", "new metadata"),
        baseline: doc("Unchanged body"),
      },
    ],
  };
}
function mount(data = fixture()) {
  server.use(http.get(endpoint, () => HttpResponse.json({ data })));
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  const onOpenFile = vi.fn();
  const onClick = vi.fn();
  const onNavigate = vi.fn();
  const content = (workspaceId = "ws") => (
    <QueryClientProvider client={client}>
      <OntologyChangesButton
        workspaceId={workspaceId}
        selected
        onClick={onClick}
      />
      <OntologyChangesView
        workspaceId={workspaceId}
        onOpenFile={onOpenFile}
        onNavigate={onNavigate}
      />
    </QueryClientProvider>
  );
  const view = render(content());
  return {
    ...view,
    client,
    onOpenFile,
    onClick,
    onNavigate,
    switchWorkspace: () => view.rerender(content("other")),
  };
}

describe("all ontology statement changes", () => {
  it("does not fetch file bodies merely to display the sidebar entry", async () => {
    const requested = vi.fn();
    server.use(
      http.get(endpoint, () => {
        requested();
        return HttpResponse.json({ data: fixture() });
      }),
    );
    const client = new QueryClient();
    render(
      <QueryClientProvider client={client}>
        <OntologyChangesButton
          workspaceId="ws"
          selected={false}
          onClick={() => {}}
        />
      </QueryClientProvider>,
    );
    await act(async () => {
      await Promise.resolve();
    });
    expect(requested).not.toHaveBeenCalled();
    expect(screen.queryByLabelText("Statement change count")).toBeNull();
  });
  it("keeps current metadata links in the source repository and historical links inert", async () => {
    const data = fixture();
    data.files = [
      {
        ...data.files[0],
        current: doc(
          "Changed **body** with [Body reference](../../docs/reference.md).",
          "Source: [Reference](../../docs/reference.md)",
        ),
        baseline: doc(
          "Original **body** with [Body reference](../../docs/reference.md).",
          "Source: [Reference](../../docs/reference.md)",
        ),
      },
    ];
    // An ontology link opens once its target is read (g5.documents.ontology.links).
    server.use(
      http.get("*/api/v1/workspaces/ws/files/*", () =>
        HttpResponse.json({
          data: {
            path: "docs/reference.md",
            content: "# Reference\n",
            size: 12,
            encoding: "utf-8",
            hash: "h",
            source: "live",
            repoOid: "oid",
            snapshotAt: null,
          },
        }),
      ),
    );
    const { onNavigate } = mount(data);
    const current = await screen.findByRole("region", {
      name: "Modified statement a",
    });
    expect(within(current).getByText("body").closest("strong")).not.toBeNull();
    fireEvent.click(
      within(current).getByRole("link", { name: "Body reference" }),
    );
    await waitFor(() =>
      expect(onNavigate).toHaveBeenCalledWith(
        expect.objectContaining({ id: "docs/reference.md", repoId: "code" }),
      ),
    );
    fireEvent.click(
      within(current).getByRole("button", { name: "Statement details for a" }),
    );
    fireEvent.click(within(current).getByRole("link", { name: "Reference" }));
    await waitFor(() =>
      expect(onNavigate).toHaveBeenCalledWith(
        expect.objectContaining({
          type: "file",
          id: "docs/reference.md",
          repoId: "code",
        }),
      ),
    );
    const before = screen.getByRole("region", { name: "Before statement a" });
    fireEvent.click(
      within(before).getByRole("button", { name: "Statement details for a" }),
    );
    expect(before.querySelector("a[href]")).toBeNull();
    expect(
      within(before).getByRole("link", { name: "Body reference" }),
    ).toHaveAttribute("aria-disabled", "true");
  });

  it("counts bodies once, includes deleted files and keeps repository identities separate", () => {
    const result = selectOntologyChanges(fixture());
    expect(result.count).toBe(3);
    expect(result.incomplete).toBe(false);
    expect(
      result.files.map((file) => [file.repoId, file.entries[0].status]),
    ).toEqual([
      ["code", "Modified"],
      ["code", "Deleted"],
      ["docs", "Added"],
    ]);
    expect(result.files[0].entries[0].current?.body).toContain("Saved edit");
    expect(result.files[0].entries[0].before?.body).toContain("Original");
  });
  it("renders read-only groups, metadata and exact Open file targets", async () => {
    const { onOpenFile, onClick } = mount();
    await screen.findByText("Saved edit");
    expect(
      screen.getByRole("heading", { name: /^code Baseline / }),
    ).toBeVisible();
    expect(screen.getByText("Deleted file")).toBeVisible();
    expect(screen.queryByText("Unchanged body")).toBeNull();
    expect(screen.queryByRole("textbox")).toBeNull();
    expect(screen.queryByRole("button", { name: "Save" })).toBeNull();
    expect(screen.getByLabelText("Statement change count")).toHaveTextContent(
      "3",
    );
    fireEvent.click(screen.getByRole("button", { name: /View all changes/ }));
    expect(onClick).toHaveBeenCalledOnce();
    fireEvent.click(screen.getAllByRole("button", { name: "Open file" })[1]);
    expect(onOpenFile).toHaveBeenCalledWith(
      expect.objectContaining({ repoId: "docs", path }),
    );
    const modified = screen.getByRole("region", {
      name: "Modified statement a",
    });
    fireEvent.click(
      within(modified).getByRole("button", { name: "Statement details for a" }),
    );
    expect(within(modified).getByText("human-provided")).toBeVisible();
  });
  it("keeps the same Before and metadata mounted during refresh; errors never show a complete total", async () => {
    const { client } = mount();
    const before = await screen.findByRole("region", {
      name: "Before statement a",
    });
    fireEvent.click(
      within(before).getByRole("button", { name: "Statement details for a" }),
    );
    let finish!: () => void;
    const barrier = new Promise<void>((resolve) => {
      finish = resolve;
    });
    server.use(
      http.get(endpoint, async () => {
        await barrier;
        return HttpResponse.json({ data: fixture() });
      }),
    );
    let refresh!: Promise<void>;
    act(() => {
      refresh = client.invalidateQueries({
        queryKey: ["source-control-change-graph", "ws"],
      });
    });
    const refreshButton = screen.getByRole("button", {
      name: "Refresh changes",
    });
    await waitFor(() =>
      expect(refreshButton).toHaveAttribute("aria-busy", "true"),
    );
    expect(refreshButton.querySelector("svg")).toHaveClass("animate-spin");
    expect(screen.getByText("3 changed ontology files")).toBeVisible();
    expect(screen.queryByText("Updating comparison…")).toBeNull();
    expect(screen.getByRole("region", { name: "Before statement a" })).toBe(
      before,
    );
    expect(within(before).getByText("human-provided")).toBeVisible();
    await act(async () => {
      finish();
      await refresh;
    });
    await waitFor(() =>
      expect(refreshButton).toHaveAttribute("aria-busy", "false"),
    );
    expect(refreshButton.querySelector("svg")).not.toHaveClass("animate-spin");
    server.use(
      http.get(endpoint, () =>
        HttpResponse.json(
          { error: { message: "Parent unavailable" } },
          { status: 403 },
        ),
      ),
    );
    await act(async () => {
      await client.invalidateQueries({
        queryKey: ["source-control-change-graph", "ws"],
      });
    });
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Comparison unavailable",
    );
    expect(screen.getByLabelText("Statement change count")).toHaveTextContent(
      "—",
    );
    expect(screen.queryByText("Saved edit")).toBeNull();
  });
  it("shows malformed files as incomplete, and distinguishes successful empty from loading", async () => {
    const data = fixture();
    data.files = [
      { ...data.files[0], current: doc("Bad").replace("### a", "### missing") },
    ];
    const result = selectOntologyChanges(data);
    expect(result.incomplete).toBe(true);
    expect(result.files[0].error).toContain("unavailable");
    mount(data);
    expect(await screen.findByRole("alert")).toHaveTextContent("unavailable");
    expect(screen.getByText(/count is incomplete/)).toBeVisible();
  });
  it("does not reuse another workspace's results and reports a successful empty view", async () => {
    const { switchWorkspace } = mount();
    await screen.findByText("Saved edit");
    server.use(
      http.get(endpoint, () =>
        HttpResponse.json({ data: { ...fixture(), files: [] } }),
      ),
    );
    switchWorkspace();
    expect(screen.queryByText("Saved edit")).toBeNull();
    expect(await screen.findByText(/No statement changes/)).toBeVisible();
    await waitFor(() =>
      expect(screen.getByLabelText("Statement change count")).toHaveTextContent(
        "0",
      ),
    );
  });
});
