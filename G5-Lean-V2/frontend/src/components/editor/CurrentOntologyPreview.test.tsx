import React from "react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { DocumentPreview } from "../files/DocumentPreview";
import { SemanticRecordFrame } from "./SemanticRecordFrame";
const source =
  "# Visible document\n\n## Hidden statement title\n\n[I · example.keep]\n\nExisting body.\n\n## Metadata\n\n### example.keep\n- provenance: [source](../../source.md)\n";
function mount(
  content = source,
  nodeId = ".g5/ontology/root.g5.md",
  editable = true,
) {
  const rewrite = vi.fn(() => true);
  const navigate = vi.fn();
  const view = render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <DocumentPreview
        workspaceId=""
        repoId=""
        filePath={nodeId}
        content={content}
        canAnswer={editable}
        onRewrite={rewrite}
        onNavigate={navigate}
      />
    </QueryClientProvider>,
  );
  return { ...view, rewrite, navigate };
}
describe("Core Ontology current-format editor", () => {
  it("retains legacy uncertain-tag filtering in Project Docs", () => {
    mount(
      "# Legacy document\n\n[I1. Ordinary]\n\nOrdinary body.\n\n[I2. Risk] (Tags: uncertain)\n\nRisk body.\n\n[Q3. Plain question]\n\nNo structured question payload.\n",
      "docs/example.md",
    );
    fireEvent.click(
      screen.getByRole("button", {
        name: "Show uncertain blocks and open questions",
      }),
    );
    expect(screen.getByTestId("toolbar-risk-question-count")).toHaveTextContent(
      "1",
    );
    expect(screen.getByText("Risk body.")).toBeVisible();
    expect(screen.queryByText("Ordinary body.")).toBeNull();
    expect(screen.queryByText("No structured question payload.")).toBeNull();
  });
  it("filters and counts Q statements and uncertain metadata once per statement", () => {
    const rows = [
      ["I", "plain", "agent"],
      ["I", "risk", "agent · uncertain"],
      ["D", "decision", "human-verified; uncertain"],
      ["Q", "question", "agent"],
      ["Q", "overlap", "agent, uncertain"],
    ];
    const content =
      "# Filter test\n\n" +
      rows
        .map(
          ([kind, id]) =>
            `<a id="${id}"></a>\n[${kind} · ${id}]\n\nBody ${id}.\n\n`,
        )
        .join("") +
      "## Metadata\n\n" +
      rows.map(([, id, flags]) => `### ${id}\n\n- ${flags}\n\n`).join("");
    const { rewrite } = mount(content);
    fireEvent.click(screen.getByRole("button", { name: "Compare" }));
    expect(screen.getByTestId("toolbar-risk-question-count")).toHaveTextContent(
      "4",
    );
    fireEvent.click(
      screen.getByRole("button", {
        name: "Show uncertain blocks and open questions",
      }),
    );
    expect(screen.queryByText("Body plain.")).toBeNull();
    for (const id of ["risk", "decision", "question", "overlap"])
      expect(screen.getByText(`Body ${id}.`)).toBeVisible();
    fireEvent.click(
      screen.getByRole("button", { name: "Show human-authored blocks" }),
    );
    expect(screen.getByText("Body decision.")).toBeVisible();
    for (const id of ["plain", "risk", "question", "overlap"])
      expect(screen.queryByText(`Body ${id}.`)).toBeNull();
    expect(rewrite).not.toHaveBeenCalled();
  });

  it("omits group insertion from read-only ontology and Project Docs", () => {
    const view = mount(source, ".g5/ontology/root.g5.md", false);
    expect(screen.queryByTestId("ontology-group-insert")).toBeNull();
    view.unmount();
    mount(source, "docs/test.md");
    expect(screen.queryByTestId("ontology-group-insert")).toBeNull();
  });

  it("marks a statement with nothing to open by a dot, without changing source", () => {
    const { rewrite } = mount();
    expect(screen.getByTestId("document-preview-content")).toHaveClass(
      "ontology-statement-list",
    );
    expect(screen.getByTestId("semantic-record-shell")).toHaveClass(
      "ontology-statement",
    );
    expect(screen.getByTestId("ontology-statement-dot")).toBeInTheDocument();
    expect(screen.queryByTestId("ontology-statement-toggle")).toBeNull();

    expect(rewrite).not.toHaveBeenCalled();
  });

  it.each([
    "- First\n- Second",
    "- Parent\n  - Nested",
    "1. First\n2. Second",
    "- [ ] Pending\n- [x] Complete",
  ])("renders genuine lists inside the statement's marker: %s", (body) => {
    mount(source.replace("Existing body.", body));
    expect(
      within(screen.getByTestId("semantic-record-body")).getAllByRole("list")
        .length,
    ).toBeGreaterThan(0);
    expect(screen.getByTestId("semantic-record-shell")).toHaveClass(
      "ontology-statement",
    );
  });

  it.each([true, false])(
    "toggles independent details by keyboard (editable=%s)",
    async (editable) => {
      const user = userEvent.setup();
      const { rewrite } = mount(
        "[I1. Intent]\n\nFirst body.\n\n[D1. Design]\n\nSecond body.",
        "docs/details.md",
        editable,
      );
      const toggles = screen.getAllByTestId("semantic-record-details-toggle");
      const panels = screen.getAllByTestId("semantic-record-metadata");
      const title = screen.getByText("Intent", { selector: "strong" });
      expect(title).toBeVisible();
      expect(toggles[0].getAttribute("aria-controls")).not.toBe(
        toggles[1].getAttribute("aria-controls"),
      );
      expect(toggles[0]).toHaveAttribute("aria-controls", panels[0].id);
      expect(toggles[0]).toHaveAttribute("aria-expanded", "false");
      expect(
        screen.getAllByTestId("semantic-record-kind")[0],
      ).not.toBeVisible();
      toggles[0].focus();
      await user.keyboard("{Enter}");
      expect(toggles[0]).toHaveFocus();
      expect(toggles[0]).toHaveAttribute("aria-expanded", "true");
      expect(panels[0]).toBeVisible();
      expect(panels[0]).toHaveTextContent("Type: Intent");
      expect(panels[0]).toHaveTextContent("ID: I1");
      expect(panels[1]).not.toBeVisible();
      expect(title).toBeVisible();
      expect(panels[0]).not.toContainElement(title);
      await user.keyboard(" ");
      expect(panels[0]).not.toBeVisible();
      expect(rewrite).not.toHaveBeenCalled();
    },
  );

  it("gives a plain question no kind badge and keeps an unsafe-source warning visible with details closed", () => {
    mount(source.replace("[I ·", "[Q ·").split("## Metadata")[0]);
    expect(screen.getByTestId("semantic-record-metadata")).not.toBeVisible();
    // The kind is not a tag: Type in the details says Q.
    expect(screen.queryByTestId("semantic-record-question-kind")).toBeNull();
    // The outline may name the kind; the statement shows no badge for it.
    expect(
      within(screen.getByTestId("semantic-record-shell")).queryByText(
        "Question",
      ),
    ).not.toBeVisible();
    expect(
      screen.getByText(/Missing or ambiguous footer metadata/),
    ).toBeVisible();
  });
  it("hides IDs inside Metadata and removes tracing without losing authored links or creation restrictions", () => {
    mount();
    expect(screen.getByTestId("semantic-record-id")).toHaveTextContent(
      "example.keep",
    );
    expect(screen.getByTestId("semantic-record-id")).not.toBeVisible();
    expect(document.getElementById("document-outline")).not.toHaveTextContent(
      "example.keep",
    );
    expect(screen.queryByTestId("semantic-record-parent-control")).toBeNull();
    expect(screen.queryByTestId("semantic-record-child-control")).toBeNull();
    expect(screen.queryByTestId("semantic-record-relationships")).toBeNull();
    expect(screen.queryByTestId("semantic-record-title")).toBeNull();
    expect(screen.queryByTestId("semantic-record-title-input")).toBeNull();
    expect(screen.queryByText("Hidden statement title")).toBeNull();
    expect(screen.queryByRole("heading", { name: "Metadata" })).toBeNull();
    expect(document.getElementById("document-outline")).not.toHaveTextContent(
      "Metadata",
    );
    expect(screen.getAllByText("Visible document").at(-1)).toBeVisible();
    const details = screen.getByTestId("semantic-record-metadata");
    expect(details).not.toBeVisible();
    expect(within(details).getByRole("link", { hidden: true })).toHaveAttribute(
      "href",
      "../../source.md",
    );
    fireEvent.click(screen.getByTestId("semantic-record-details-toggle"));
    expect(screen.getByTestId("semantic-record-id")).toBeVisible();
    expect(screen.queryByTestId("spec-block-insert-below")).toBeNull();
    expect(screen.queryByText("Add Intent")).toBeNull();
    expect(screen.queryByText("Add Decision")).toBeNull();
    expect(screen.queryByText("Add Question")).toBeNull();
  });
  it("also hides IDs and removes tracing for Project Docs statements", () => {
    mount("[I1. Existing intent]\n\nDocument body.\n", "docs/requirement.md");
    const id = screen.getByTestId("semantic-record-id");
    expect(id).toHaveTextContent("I1");
    expect(id).not.toBeVisible();
    expect(screen.getByTestId("semantic-record-kind")).toHaveTextContent(
      "Intent",
    );
    expect(screen.queryByTestId("semantic-record-parent-control")).toBeNull();
    expect(screen.queryByTestId("semantic-record-child-control")).toBeNull();
    fireEvent.click(screen.getByTestId("semantic-record-details-toggle"));
    expect(id).toBeVisible();
  });
  it("discloses current-format footer IDs only once even in Project Docs", () => {
    mount(source, "docs/current.md");
    expect(screen.getAllByText("example.keep")).toHaveLength(1);
    expect(screen.getByTestId("semantic-record-id")).not.toBeVisible();
    expect(document.getElementById("document-outline")).not.toHaveTextContent(
      "example.keep",
    );
    fireEvent.click(screen.getByTestId("semantic-record-details-toggle"));
    expect(screen.getByTestId("semantic-record-id")).toBeVisible();
  });
  it("keeps the ID inspectable when footer metadata is missing", () => {
    mount(source.split("## Metadata")[0]);
    expect(screen.getByTestId("semantic-record-id")).not.toBeVisible();
    fireEvent.click(screen.getByTestId("semantic-record-details-toggle"));
    expect(screen.getByTestId("semantic-record-id")).toBeVisible();
    expect(
      screen.queryByRole("menuitem", { name: "Delete statement" }),
    ).toBeNull();
  });

  it("previews the exact local block and opens its authored heading", async () => {
    const { navigate } = mount(
      source.replace(
        "# Visible document",
        "# Visible document\n\n[Jump](#hidden-statement-title)",
      ),
    );
    fireEvent.click(screen.getByRole("link", { name: "Jump" }));
    const preview = await screen.findByTestId("ontology-reference");
    expect(within(preview).getByText("example.keep")).not.toBeVisible();
    expect(within(preview).getByText("root.g5.md")).toBeVisible();
    expect(
      within(preview).queryByText("Current draft"),
    ).not.toBeInTheDocument();
    fireEvent.click(
      within(preview).getByRole("button", { name: "Reference details" }),
    );
    expect(within(preview).getByText("example.keep")).toBeVisible();
    expect(within(preview).getByText("Type: Intent")).toBeVisible();
    expect(
      within(preview).queryByText(
        /^(Current draft|Live files|Committed files)$/,
      ),
    ).not.toBeInTheDocument();
    expect(within(preview).getByRole("link", { name: "source" })).toBeVisible();
    fireEvent.click(
      within(preview).getByRole("button", { name: "Open document" }),
    );
    expect(navigate).toHaveBeenCalledWith(
      expect.objectContaining({
        id: ".g5/ontology/root.g5.md",
        intentRecordNumber: "#hidden-statement-title",
      }),
    );
  });
  it("keeps the reference header and close control available while loading", async () => {
    mount(
      source.replace(
        "# Visible document",
        "# Visible document\n\n[Remote](other.g5.md#remote)",
      ),
    );
    fireEvent.click(screen.getByRole("link", { name: "Remote" }));
    const preview = await screen.findByTestId("ontology-reference");
    expect(within(preview).getByRole("status")).toHaveTextContent(
      "Loading reference",
    );
    expect(within(preview).getByText("other.g5.md")).toBeVisible();
    expect(
      within(preview).getByRole("button", { name: "Open document" }),
    ).toBeDisabled();
    expect(
      within(preview).getByRole("button", { name: "Reference details" }),
    ).toBeDisabled();
    fireEvent.click(
      within(preview).getByRole("button", { name: "Close reference" }),
    );
    expect(screen.queryByTestId("ontology-reference")).toBeNull();
  });
  it("shows only the selected block provenance and resets details on preview navigation", async () => {
    mount(
      '# Document\n\n[Second](#two)\n\n<a id="one"></a>\n[I · one]\n\nFirst body. [Second](#two)\n\n<a id="two"></a>\n[D · two]\n\nSecond body. [First](#one)\n\n## Metadata\n\n### one\n- first provenance\n\n### two\n- second provenance\n',
    );
    fireEvent.click(
      within(screen.getAllByTestId("semantic-record")[0]).getByRole("link", {
        name: "Second",
      }),
    );
    const preview = await screen.findByTestId("ontology-reference");
    const details = within(preview).getByRole("button", {
      name: "Reference details",
    });
    fireEvent.click(details);
    expect(within(preview).getByText("second provenance")).toBeVisible();
    expect(within(preview).queryByText("first provenance")).toBeNull();
    fireEvent.click(within(preview).getByRole("link", { name: "First" }));
    await waitFor(() =>
      expect(details).toHaveAttribute("aria-expanded", "false"),
    );
    expect(
      within(preview).getByTestId("ontology-reference-content"),
    ).toHaveTextContent("First body.");
    expect(within(preview).getByText("first provenance")).not.toBeVisible();
    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByTestId("ontology-reference")).toBeNull();
  });

  it("opens authored footer source links in the source repository and reports missing fragments", async () => {
    const { navigate } = mount(
      source.replace(
        "# Visible document",
        "# Visible document\n\n[Broken](#missing)",
      ),
    );
    fireEvent.click(screen.getByRole("link", { name: "Broken" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Missing target",
    );
    const preview = screen.getByTestId("ontology-reference");
    expect(within(preview).getByText("root.g5.md")).toBeVisible();
    expect(
      within(preview).getByRole("button", { name: "Open document" }),
    ).toBeDisabled();
    fireEvent.click(
      within(preview).getByRole("button", { name: "Close reference" }),
    );
    expect(screen.queryByTestId("ontology-reference")).toBeNull();
    expect(screen.getByRole("link", { name: "Broken" })).toHaveFocus();
    fireEvent.click(screen.getByTestId("semantic-record-details-toggle"));
    fireEvent.click(screen.getByRole("link", { name: "source" }));
    await waitFor(() =>
      expect(navigate).toHaveBeenCalledWith(
        expect.objectContaining({ type: "file", id: "source.md" }),
      ),
    );
  });
});

describe("shared statement question details", () => {
  it("keeps open questions, severity, blocking and uncertainty outside details", () => {
    render(
      <SemanticRecordFrame
        record={{ id: "Q1", kind: "Q", title: "", spanStart: 0, spanEnd: 1 }}
        tags={[{ kind: "uncertainty", syntax: "inline" }]}
        questionAccent={{
          severity: "critical",
          answered: false,
          humanAnswered: false,
        }}
      >
        Needs a decision.
      </SemanticRecordFrame>,
    );
    expect(screen.getByTestId("semantic-record-metadata")).not.toBeVisible();
    for (const suffix of [
      "question-status",
      "question-severity",
      "question-blocking",
      "uncertain-badge",
    ]) {
      expect(screen.getByTestId(`semantic-record-${suffix}`)).toBeVisible();
    }
    expect(
      screen.getByTestId("semantic-record-question-status"),
    ).toHaveTextContent("Open");
    expect(screen.getByTestId("semantic-record-kind")).not.toBeVisible();
    expect(
      screen.getByTestId("semantic-record-uncertain-badge"),
    ).toHaveAttribute("data-tag-source", "authored");
    expect(
      screen.getByTestId("semantic-record-uncertain-badge"),
    ).not.toHaveAttribute("title");
    expect(
      screen.getByTestId("semantic-record-uncertain-badge"),
    ).not.toHaveClass("cursor-context-menu");
  });
});

describe("compact shared statement menu", () => {
  it("places metadata after content and has no permanent delete button", () => {
    mount();
    expect(
      screen.queryByRole("button", { name: "Delete statement" }),
    ).toBeNull();
    const content = screen.getByText("Existing body.");
    const metadata = screen.getByTestId("semantic-record-metadata");
    expect(
      content.compareDocumentPosition(metadata) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(screen.getByTestId("semantic-record-kind")).toHaveTextContent(
      "Intent",
    );
    expect(screen.getByTestId("semantic-record")).not.toHaveAttribute(
      "aria-keyshortcuts",
    );
  });

  it("does not offer deletion in a read-only document", () => {
    mount(source, ".g5/ontology/root.g5.md", false);
    fireEvent.contextMenu(screen.getByTestId("semantic-record"));
    fireEvent.keyDown(screen.getByTestId("semantic-record"), {
      key: "F10",
      shiftKey: true,
    });
    expect(
      screen.queryByRole("menuitem", { name: "Delete statement" }),
    ).toBeNull();
  });
});

it("renders anchored siblings under a visible section and previews only the targeted statement", async () => {
  const text =
    '# Document\n\n## Shared section\n\n[Second](#second)\n\n<a id="first"></a>\n[I · first]\n\nFirst body. [Second](#second)\n\n<a id="second"></a>\n[D · second]\n\nSecond body.\n\n## Metadata\n\n### first\n- human-provided\n\n### second\n- agent\n';
  const { rewrite } = mount(text);
  expect(screen.getByRole("heading", { name: "Shared section" })).toBeVisible();
  expect(screen.queryByRole("alert")).toBeNull();
  expect(document.querySelector('[data-record-span="second"]')).toHaveAttribute(
    "data-record-span",
    "second",
  );
  fireEvent.click(
    within(screen.getAllByTestId("semantic-record")[0]).getByRole("link", {
      name: "Second",
    }),
  );
  const preview = await screen.findByTestId("ontology-reference");
  expect(preview).toHaveTextContent("Second body.");
  expect(preview).not.toHaveTextContent("First body.");
  fireEvent.keyDown(document, { key: "Escape" });
  expect(rewrite).not.toHaveBeenCalled();
});
