// Statement folds in ontology Preview (g5.documents.ontology.disclosure).
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { describe, expect, it, vi } from "vitest";
import { server } from "../../test/msw/server";
import { DocumentPreview, type PreviewReadingState } from "./DocumentPreview";

// Counts how often the document is parsed, through its markup plugin.
const parses = vi.hoisted(() => ({ count: 0 }));
vi.mock("./documentMarkup", async (original) => {
  const actual = await original<typeof import("./documentMarkup")>();
  return {
    ...actual,
    documentMarkup: (...args: Parameters<typeof actual.documentMarkup>) => {
      const markup = actual.documentMarkup(...args);
      return {
        ...markup,
        plugin: () => {
          parses.count++;
          return markup.plugin();
        },
      };
    },
  };
});

const path = ".g5/ontology/features/receiving.g5.md";
const statement = (
  id: string,
  body: string,
  kind = "I",
  aliases: string[] = [],
) =>
  `${aliases.map((a) => `<a id="${a}"></a>\n`).join("")}<a id="${id}"></a>\n[${kind} · ${id}]\n\n${body}\n\n`;
const page = (body: string, entries: [string, string][]) =>
  `# Receiving\n\n${body}## Metadata\n\n${entries
    .map(([id, tags]) => `### ${id}\n\n- ${tags}\n\n`)
    .join("")}`;
const source = page(
  "## Numbers\n\n" +
    statement("receipt", "Every receipt needs a number.") +
    statement(
      "receipt.suggestion",
      "Suggest the next number.\n\n- The receiver may replace it.",
      "I",
      ["receipt-suggestion"],
    ) +
    statement("receipt.suggestion.floor", "Never below the start.") +
    statement(
      "receipt.rules",
      "A number must meet these rules:\n\n- At most 20 characters.",
    ) +
    statement("draft", "Drafts may be incomplete.\n\n- Comments are limited.") +
    statement("plain", "A statement with nothing to open.") +
    "## Elsewhere\n\n" +
    statement(
      "draft.later",
      "Separated from its look-alike parent by a heading.",
    ),
  [
    ["receipt", "agent"],
    ["receipt.suggestion", "agent"],
    ["receipt.suggestion.floor", "agent"],
    ["receipt.rules", "agent · uncertain"],
    ["draft", "agent"],
    ["plain", "agent\n- test-coverage"],
    ["draft.later", "agent"],
  ],
);

function mount(
  content = source,
  props: {
    target?: string;
    workspaceId?: string;
    readingState?: PreviewReadingState;
  } = {},
) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  const view = render(
    <QueryClientProvider client={client}>
      <DocumentPreview
        workspaceId={props.workspaceId ?? ""}
        repoId="repo"
        filePath={path}
        content={content}
        onNavigate={vi.fn()}
        target={props.target}
        targetSeq={1}
        readingState={props.readingState}
      />
    </QueryClientProvider>,
  );
  return view;
}

const shell = (id: string) =>
  document
    .querySelector(`[data-record-span="${id}"]`)!
    .closest<HTMLElement>("[data-testid='semantic-record-shell']")!;
const toggle = (id: string) =>
  within(shell(id)).queryAllByTestId("ontology-statement-toggle")[0] ?? null;
const shown = (text: string) => {
  const element = screen.getByText(text);
  return !element.closest("[hidden]");
};

describe("ontology statement folds", () => {
  it("shows each statement's lead and folds its details and children together", () => {
    mount();
    expect(shown("Every receipt needs a number.")).toBe(true);
    expect(shown("Suggest the next number.")).toBe(false);
    expect(shown("Never below the start.")).toBe(false);
    expect(shown("Comments are limited.")).toBe(false);
    const button = toggle("receipt")!;
    expect(button).toHaveAttribute("aria-expanded", "false");
    expect(button).toHaveAccessibleName(
      "Show details: Every receipt needs a number.",
    );
    fireEvent.click(button);
    expect(button).toHaveAttribute("aria-expanded", "true");
    expect(shown("Suggest the next number.")).toBe(true);
    expect(shown("The receiver may replace it.")).toBe(true);
    // A grandchild nests further, without a fold of its own.
    expect(shown("Never below the start.")).toBe(true);
    expect(toggle("receipt.suggestion")).toBeNull();
    expect(shell("receipt.suggestion")).toHaveClass("ontology-statement-child");
    expect(shell("receipt.suggestion.floor")).toHaveClass(
      "ontology-statement-child",
    );
    // Opening one fold leaves the others closed.
    expect(shown("Comments are limited.")).toBe(false);
  });

  it("marks statements with nothing to open by a dot and keeps look-alike IDs apart", () => {
    mount();
    expect(toggle("plain")).toBeNull();
    expect(
      within(shell("plain")).getByTestId("ontology-statement-dot"),
    ).toBeInTheDocument();
    // A section heading between them: not a child, whatever the ID.
    expect(shell("draft.later")).not.toHaveClass("ontology-statement-child");
    expect(shown("Separated from its look-alike parent by a heading.")).toBe(
      true,
    );
    // A statement whose ID does not extend the one before it is not nested.
    expect(shell("draft")).not.toHaveClass("ontology-statement-child");
  });

  it("opens by keyboard", async () => {
    const user = userEvent.setup();
    mount();
    toggle("draft")!.focus();
    await user.keyboard("{Enter}");
    expect(shown("Comments are limited.")).toBe(true);
    await user.keyboard(" ");
    expect(shown("Comments are limited.")).toBe(false);
  });

  it("expands all folds, keeps others open when one closes, and collapses all", () => {
    mount();
    fireEvent.click(screen.getByTestId("ontology-expand-all"));
    expect(shown("Never below the start.")).toBe(true);
    expect(shown("Comments are limited.")).toBe(true);
    fireEvent.click(toggle("draft")!);
    expect(shown("Comments are limited.")).toBe(false);
    expect(shown("Never below the start.")).toBe(true);
    fireEvent.click(screen.getByTestId("ontology-collapse-all"));
    expect(shown("Never below the start.")).toBe(false);
  });

  it("keeps folds and Expand all across view switches, with the reading view's state", () => {
    // `g5.documents.editor.modes`: a mode switch remounts the reading view
    // with the file view's reading state.
    const readingState: PreviewReadingState = {};
    const first = mount(source, { readingState });
    fireEvent.click(toggle("receipt")!);
    expect(shown("Suggest the next number.")).toBe(true);
    first.unmount();
    const second = mount(source, { readingState });
    expect(shown("Suggest the next number.")).toBe(true);
    expect(shown("Comments are limited.")).toBe(false);
    fireEvent.click(screen.getByTestId("ontology-expand-all"));
    second.unmount();
    mount(source, { readingState });
    expect(shown("Comments are limited.")).toBe(true);
  });

  it("opens the fold holding a navigation target, including a former ID", () => {
    const scroll = vi.fn();
    Element.prototype.scrollIntoView = scroll;
    const view = mount(source, { target: "#receipt.suggestion.floor" });
    expect(shown("Never below the start.")).toBe(true);
    expect(scroll).toHaveBeenCalled();
    expect(screen.queryByText(/Missing target/)).toBeNull();
    view.unmount();
    mount(source, { target: "#receipt-suggestion" });
    expect(shown("Suggest the next number.")).toBe(true);
    expect(screen.queryByText(/Missing target/)).toBeNull();
  });

  it("shows a filtered child with its parent's lead and hides its siblings", () => {
    mount();
    // Filters apply with Compare off.
    fireEvent.click(screen.getByRole("button", { name: "Compare" }));
    fireEvent.click(
      screen.getByRole("button", {
        name: "Show uncertain blocks and open questions",
      }),
    );
    expect(shown("A number must meet these rules:")).toBe(true);
    expect(shown("Every receipt needs a number.")).toBe(true);
    expect(screen.queryByText("Suggest the next number.")).toBeNull();
    expect(screen.queryByText("Drafts may be incomplete.")).toBeNull();
  });

  it("opens folds holding a Compare change", async () => {
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
            content: source.replace(
              "Never below the start.",
              "Never below zero.",
            ),
          },
        }),
      ),
    );
    mount(source, { workspaceId: "ws" });
    expect(
      await screen.findByTestId("ontology-change-label"),
    ).toBeInTheDocument();
    expect(shown("Never below the start.")).toBe(true);
    expect(shown("Comments are limited.")).toBe(false);
  });

  it("keeps a statement's text written directly under its marker", () => {
    mount(
      '# R\n\n<a id="old-x"></a>\n<a id="x"></a>\n[I · x]\nThe lead right under the marker.\n\n- A property.\n\n## Metadata\n\n### x\n\n- agent\n',
    );
    expect(shown("The lead right under the marker.")).toBe(true);
    expect(screen.queryByText(/\[I · x\]/)).toBeNull();
    expect(shown("A property.")).toBe(false);
    expect(
      screen
        .getByText("The lead right under the marker.")
        .closest("[data-source-line]"),
    ).toHaveAttribute("data-source-line", "6");
  });

  it("keeps a statement with no body, and its children under it", () => {
    mount(
      page(
        '<a id="empty"></a>\n[I · empty]\n\n' +
          statement("empty.child", "A child of an empty statement."),
        [
          ["empty", "agent"],
          ["empty.child", "agent"],
        ],
      ),
    );
    expect(document.querySelector('[data-record-span="empty"]')).not.toBeNull();
    expect(toggle("empty")).not.toBeNull();
    expect(shell("empty.child")).toHaveClass("ontology-statement-child");
    expect(shown("A child of an empty statement.")).toBe(false);
  });

  it("reopens a fold the reader closed when a new reason appears inside", () => {
    mount();
    fireEvent.click(screen.getByRole("button", { name: "Compare" }));
    fireEvent.click(screen.getByTestId("ontology-collapse-all"));
    expect(shown("A number must meet these rules:")).toBe(false);
    fireEvent.click(
      screen.getByRole("button", {
        name: "Show uncertain blocks and open questions",
      }),
    );
    // The filter matches a child of the closed fold, which opens again.
    expect(shown("A number must meet these rules:")).toBe(true);
    // The reader may close it again.
    fireEvent.click(toggle("receipt")!);
    expect(shown("A number must meet these rules:")).toBe(false);
  });

  it("keeps a statement's badges beside its lead, above its details", () => {
    mount();
    fireEvent.click(toggle("receipt")!);
    const rules = shell("receipt.rules");
    const badge = within(rules).getByTestId("semantic-record-uncertain-badge");
    const detail = within(rules).getByText("At most 20 characters.");
    expect(
      badge.compareDocumentPosition(detail) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it("opens a fold or verification details without re-rendering the document", () => {
    mount();
    const before = parses.count;
    fireEvent.click(toggle("draft")!);
    expect(shown("Comments are limited.")).toBe(true);
    fireEvent.click(
      within(shell("plain")).getAllByTestId("verification-status")[0],
    );
    expect(screen.getByTestId("verification-panel")).toBeInTheDocument();
    expect(parses.count).toBe(before);
    // Expand all re-renders the document, which the counter sees.
    fireEvent.click(screen.getByTestId("ontology-expand-all"));
    expect(parses.count).toBeGreaterThan(before);
  });

  it("leaves other documents' statements without folds", () => {
    render(
      <QueryClientProvider client={new QueryClient()}>
        <DocumentPreview
          workspaceId=""
          repoId="repo"
          filePath="docs/notes.md"
          content={"[I1. Intent]\n\nFirst body.\n\n- A detail.\n"}
        />
      </QueryClientProvider>,
    );
    expect(screen.queryByTestId("ontology-statement-toggle")).toBeNull();
    expect(screen.getByText("A detail.")).toBeVisible();
  });
});
