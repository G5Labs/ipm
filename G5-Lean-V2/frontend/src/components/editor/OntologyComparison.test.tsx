import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it, vi } from "vitest";
import { server } from "../../test/msw/server";
import { DocumentPreview } from "../files/DocumentPreview";
import type { OntologyBaseline } from "../../hooks/useOntologyComparison";
const path = ".g5/ontology/root.g5.md";
const doc = (body: string, metadata = "provenance") =>
  `# Root\n\n<a id="a"></a>\n[I · a]\n\n${body}\n\n## Metadata\n\n### a\n\n- ${metadata}\n`;
const endpoint = "*/api/v1/workspaces/ws/source-control/ontology-baseline";
function response(content: string | null): OntologyBaseline {
  return {
    revision: "rev",
    workspaceId: "ws",
    repoId: "repo",
    path,
    baselineCommit: "1234567890abcdef",
    sourceCommit: "head",
    parentCommit: "parent",
    content,
  };
}
function mount(content: string, editable = true) {
  const rewrite = vi.fn(() => true);
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  const view = render(
    <QueryClientProvider client={client}>
      <DocumentPreview
        workspaceId="ws"
        repoId="repo"
        filePath={path}
        content={content}
        canAnswer={editable}
        onRewrite={rewrite}
        onNavigate={vi.fn()}
      />
    </QueryClientProvider>,
  );
  return {
    ...view,
    rewrite,
    client,
    refresh: (next: string, nextPath = path) =>
      view.rerender(
        <QueryClientProvider client={client}>
          <DocumentPreview
            workspaceId="ws"
            repoId="repo"
            filePath={nextPath}
            content={next}
            canAnswer={editable}
            onRewrite={rewrite}
            onNavigate={vi.fn()}
          />
        </QueryClientProvider>,
      ),
  };
}
function toggle() {
  fireEvent.click(screen.getByRole("button", { name: "Compare" }));
}

describe("ontology statement comparison editor", () => {
  it("defaults Compare on per document, places it before Human, and leaves other files unchanged", async () => {
    server.use(
      http.get(endpoint, () =>
        HttpResponse.json({ data: response(doc("Same")) }),
      ),
    );
    const { refresh, rewrite } = mount(doc("Same"));
    const compare = screen.getByRole("button", {
      name: "Compare",
    });
    const human = screen.getByTestId("toolbar-tag-human");
    expect(compare).toHaveAttribute("aria-pressed", "true");
    expect(
      compare.compareDocumentPosition(human) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(human).toBeDisabled();
    toggle();
    expect(compare).toHaveAttribute("aria-pressed", "false");
    expect(human).toBeEnabled();
    refresh(doc("Changed draft"));
    expect(compare).toHaveAttribute("aria-pressed", "false");
    refresh(doc("Same"), ".g5/ontology/other.g5.md");
    expect(screen.getByRole("button", { name: "Compare" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    refresh("");
    expect(screen.getByRole("button", { name: "Compare" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(screen.queryByTestId("toolbar-tag-human")).toBeNull();
    expect(screen.queryByTestId("toolbar-tag-uncertainty")).toBeNull();
    expect(
      await screen.findByRole("region", { name: "Deleted statement a" }),
    ).toHaveTextContent("Same");
    refresh("# Project document", "docs/example.md");
    expect(screen.queryByRole("button", { name: "Compare" })).toBeNull();
    expect(screen.queryByTestId("toolbar-tag-human")).toBeNull();
    expect(rewrite).not.toHaveBeenCalled();
  });
  it.each([true, false])(
    "shows immutable Before with its own details and no current selectors (editable=%s)",
    async (editable) => {
      server.use(
        http.get(endpoint, () =>
          HttpResponse.json({
            data: response(
              doc("- Previous [link](https://example.com)", "old evidence"),
            ),
          }),
        ),
      );
      const { rewrite } = mount(doc("Current body", "new evidence"), editable);
      expect(screen.getByTestId("semantic-record-shell")).toHaveClass(
        "ontology-statement",
      );
      expect(screen.queryByText("Before")).toBeNull();
      expect(screen.getByRole("button", { name: "Compare" })).toHaveAttribute(
        "aria-pressed",
        "true",
      );
      const before = await screen.findByRole("region", {
        name: "Before statement a",
      });
      // Current and Before share the pair's one bullet: the rows sit directly
      // in the pair and neither carries a bullet of its own.
      const current = screen.getByRole("region", {
        name: "Modified statement a",
      });
      expect(current.firstElementChild).toHaveTextContent("Modified");
      expect(current.parentElement).toHaveAttribute(
        "data-testid",
        "ontology-comparison-pair",
      );
      expect(current.nextElementSibling).toBe(before);
      for (const row of [current, before])
        expect(row.closest(".ontology-statement-bullet")).toBeNull();
      expect(
        within(before).getByTestId("semantic-record-shell"),
      ).not.toHaveClass("ontology-statement");
      expect(
        screen.getByTestId("ontology-comparison-status"),
      ).toHaveTextContent("1234567890ab");
      const compareButton = screen.getByRole("button", {
        name: "Compare",
      });
      expect(
        within(compareButton).getByTestId("ontology-comparison-count"),
      ).toHaveTextContent("1");
      expect(
        within(compareButton).getByTestId("ontology-comparison-status"),
      ).toHaveClass("sr-only");
      expect(compareButton).toHaveAttribute(
        "title",
        expect.stringContaining("1234567890ab"),
      );

      expect(screen.queryByTestId("ontology-group-insert")).toBeNull();
      expect(within(before).getByRole("link", { name: "link" })).toBeVisible();
      expect(
        before.querySelector(
          "[data-block-index], [data-record-span], [contenteditable=true], a[href]",
        ),
      ).toBeNull();
      const details = within(before).getByRole("button", {
        name: "Statement details for a",
      });
      fireEvent.click(details);
      expect(within(before).getByText("old evidence")).toBeVisible();
      expect(details).toHaveAttribute(
        "aria-controls",
        within(before).getByTestId("semantic-record-metadata").id,
      );
      fireEvent.contextMenu(before);
      expect(
        screen.queryByRole("menuitem", { name: "Delete statement" }),
      ).toBeNull();
      toggle();
      expect(screen.queryByTestId("ontology-comparison-count")).toBeNull();
      expect(screen.queryByText("Before")).toBeNull();
      expect(rewrite).not.toHaveBeenCalled();
    },
  );
  it("includes unsaved body edits and never serializes historical rows", async () => {
    server.use(
      http.get(endpoint, () =>
        HttpResponse.json({ data: response(doc("Original body")) }),
      ),
    );
    const { rewrite, refresh } = mount(doc("Original body"));
    await waitFor(() =>
      expect(
        screen.getByTestId("ontology-comparison-status"),
      ).toHaveTextContent("0 statement changes"),
    );
    refresh(doc("Unsaved body"));
    expect(
      await screen.findByRole("region", { name: "Before statement a" }),
    ).toHaveTextContent("Original body");
    expect(rewrite).not.toHaveBeenCalled();
    expect(screen.getByText("Unsaved body")).toBeVisible();
  });

  it("keeps metadata-only changes unflagged", async () => {
    server.use(
      http.get(endpoint, () =>
        HttpResponse.json({ data: response(doc("Same", "old")) }),
      ),
    );
    mount(doc("Same", "new"));
    await waitFor(() =>
      expect(
        screen.getByTestId("ontology-comparison-status"),
      ).toHaveTextContent("0 statement changes"),
    );
    expect(screen.queryByText("Before")).toBeNull();
    expect(screen.queryByTestId("ontology-change-label")).toBeNull();
    expect(screen.getByTestId("ontology-comparison-count")).toHaveTextContent(
      "0",
    );
  });
  it("distinguishes absent baseline from unavailable history", async () => {
    server.use(
      http.get(endpoint, () => HttpResponse.json({ data: response(null) })),
    );
    const view = mount(doc("New"));
    expect(
      await screen.findByTestId("ontology-change-label"),
    ).toHaveTextContent("Added");
    view.unmount();
    server.use(
      http.get(endpoint, () =>
        HttpResponse.json(
          {
            error: {
              code: "DIFF_GRAPH_UNAVAILABLE",
              message: "Parent unavailable",
            },
          },
          { status: 409 },
        ),
      ),
    );
    mount(doc("New"));
    await waitFor(() =>
      expect(
        screen.getByTestId("ontology-comparison-status"),
      ).toHaveTextContent(/unavailable/i),
    );
    expect(screen.getByTestId("ontology-comparison-count")).toHaveTextContent(
      "!",
    );
    expect(screen.queryByTestId("ontology-change-label")).toBeNull();
    expect(screen.getByText("New")).toBeVisible();
  });
  it("shows unavailable if a background refresh fails instead of presenting cached results as current", async () => {
    let fail = false;
    server.use(
      http.get(endpoint, () =>
        fail
          ? HttpResponse.json(
              {
                error: {
                  code: "DIFF_GRAPH_UNAVAILABLE",
                  message: "Parent unavailable",
                },
              },
              { status: 409 },
            )
          : HttpResponse.json({ data: response(doc("Old")) }),
      ),
    );
    const { client } = mount(doc("Current"));
    await screen.findByRole("region", { name: "Before statement a" });
    fail = true;
    void client.invalidateQueries({
      queryKey: ["source-control-change-graph"],
    });
    await waitFor(() =>
      expect(
        screen.getByTestId("ontology-comparison-status"),
      ).toHaveTextContent("Comparison unavailable"),
    );
    expect(
      screen.queryByRole("region", { name: "Before statement a" }),
    ).toBeNull();
    expect(screen.getByTestId("ontology-comparison-count")).toHaveTextContent(
      "!",
    );
    expect(screen.getByText("Current")).toBeVisible();
  });
  it("keeps comparison and expanded details mounted during background refresh", async () => {
    let slow = false;
    let releaseRefresh!: () => void;
    const refreshGate = new Promise<void>((resolve) => {
      releaseRefresh = resolve;
    });
    server.use(
      http.get(endpoint, async () => {
        if (slow) await refreshGate;
        return HttpResponse.json({
          data: {
            ...response(doc(slow ? "Current" : "Old")),
            baselineCommit: slow ? "abcdef123456" : "1234567890abcdef",
          },
        });
      }),
    );
    const { client } = mount(doc("Current"));
    const before = await screen.findByRole("region", {
      name: "Before statement a",
    });
    fireEvent.click(
      within(before).getByRole("button", { name: "Statement details for a" }),
    );
    const details = within(before).getByText("provenance");
    expect(details).toBeVisible();
    slow = true;
    void client.invalidateQueries({
      queryKey: ["source-control-change-graph"],
    });
    await waitFor(() =>
      expect(
        screen.getByTestId("ontology-comparison-status"),
      ).toHaveTextContent("Updating"),
    );
    expect(screen.getByRole("region", { name: "Before statement a" })).toBe(
      before,
    );
    expect(details).toBeVisible();
    expect(screen.getByTestId("ontology-comparison-count")).toHaveTextContent(
      "1",
    );
    expect(screen.getByTestId("ontology-comparison-status")).toHaveTextContent(
      "1234567890ab",
    );
    expect(screen.getByText("Current")).toBeVisible();
    releaseRefresh();
    await waitFor(() =>
      expect(
        screen.getByTestId("ontology-comparison-status"),
      ).toHaveTextContent("0 statement changes"),
    );
    expect(screen.getByTestId("ontology-comparison-status")).toHaveTextContent(
      "abcdef123456",
    );
  });
});

describe("ontology Preview toolbar and statement presentation", () => {
  const twoStatements =
    '# Root\n\n<a id="a"></a>\n[I · a]\n\nHuman body.\n\n<a id="b"></a>\n[D · b]\n\nAgent body.\n\n## Metadata\n\n### a\n\n- human-provided\n\n### b\n\n- agent\n';
  function mountWith(
    content: string,
    ids = { workspaceId: "ws", repoId: "repo" },
  ) {
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    return render(
      <QueryClientProvider client={client}>
        <DocumentPreview
          {...ids}
          filePath={path}
          content={content}
          onNavigate={vi.fn()}
        />
      </QueryClientProvider>,
    );
  }

  it("styles toolbar toggles, shows Compare selected and divides it from the filters", async () => {
    server.use(
      http.get(endpoint, () =>
        HttpResponse.json({ data: response(twoStatements) }),
      ),
    );
    mountWith(twoStatements);
    const compare = screen.getByRole("button", { name: "Compare" });
    const human = screen.getByTestId("toolbar-tag-human");
    const risk = screen.getByTestId("toolbar-tag-uncertainty");
    const divider = screen.getByTestId("document-preview-toolbar-divider");
    for (const button of [compare, human, risk])
      expect(button).toHaveClass("preview-toolbar-button", "rounded", "border");
    // Selected state is visible, not only announced through aria-pressed.
    expect(compare).toHaveAttribute("aria-pressed", "true");
    expect(compare.className).toContain("bg-[var(--g5-info)]/15");
    expect(human.className).not.toContain("/15");
    expect(divider).toHaveAttribute("aria-hidden", "true");
    expect(
      compare.compareDocumentPosition(divider) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(
      divider.compareDocumentPosition(human) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(risk).toHaveTextContent("Risk/Question");
    toggle();
    expect(compare.className).not.toContain("bg-[var(--g5-info)]/15");
    fireEvent.click(human);
    expect(human.className).toContain("bg-[var(--g5-ok)]/15");
  });

  it("keeps the Compare title neutral while off and reports a comparison that cannot run", async () => {
    server.use(
      http.get(endpoint, () =>
        HttpResponse.json({ data: response(twoStatements) }),
      ),
    );
    const view = mountWith(twoStatements);
    toggle();
    const compare = screen.getByRole("button", { name: "Compare" });
    expect(compare).toHaveAttribute("aria-pressed", "false");
    expect(compare.title).toMatch(/^Compare statement changes/);
    expect(compare.title).not.toMatch(/Updating|unavailable/i);
    view.unmount();

    const fetched = vi.fn();
    server.use(
      http.get(endpoint, () => {
        fetched();
        return HttpResponse.json({ data: response(twoStatements) });
      }),
    );
    mountWith(twoStatements, { workspaceId: "ws", repoId: "" });
    const unavailable = screen.getByRole("button", { name: "Compare" });
    expect(unavailable).toHaveAttribute("aria-pressed", "true");
    expect(unavailable.title).toBe(
      "Comparison unavailable: workspace and repository are required.",
    );
    expect(screen.getByTestId("ontology-comparison-status")).toHaveTextContent(
      "Comparison unavailable",
    );
    expect(screen.getByTestId("ontology-comparison-count")).toHaveTextContent(
      "!",
    );
    expect(fetched).not.toHaveBeenCalled();
  });

  it("shows no permanent Human badge while the Human filter and details keep provenance", async () => {
    server.use(
      http.get(endpoint, () =>
        HttpResponse.json({ data: response(twoStatements) }),
      ),
    );
    mountWith(twoStatements);
    expect(screen.queryByTestId("semantic-record-human-badge")).toBeNull();
    const human = screen
      .getAllByTestId("semantic-record")
      .find((r) => r.getAttribute("data-record-span") === "a")!;
    expect(within(human).queryByText("Human")).toBeNull();
    fireEvent.click(
      within(human).getByRole("button", { name: "Statement details for a" }),
    );
    expect(
      within(human).getByTestId("semantic-record-metadata"),
    ).toHaveTextContent("human-provided");
    toggle();
    fireEvent.click(screen.getByTestId("toolbar-tag-human"));
    expect(screen.getByText("Human body.")).toBeVisible();
    expect(screen.queryByText("Agent body.")).toBeNull();
  });
});
