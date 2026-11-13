// Verification status and details in ontology Preview
// (g5.documents.ontology.verification).
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactElement } from "react";
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
import { DocumentPreview } from "./DocumentPreview";

const path = ".g5/ontology/features/receiving.g5.md";
const tla = ".g5/TLA_verification";
const statement = (id: string, body: string) =>
  `<a id="${id}"></a>\n[I · ${id}]\n\n${body}\n\n`;
const link = (concern: string) =>
  `State machines: [${concern}](../../TLA_verification/models/${concern}/manifest.json)`;
const source = (entries: Record<string, string>) =>
  "# Receiving\n\n" +
  statement("number", "Every receipt needs a number.") +
  statement("number.unique", "Unique per vendor.") +
  statement("draft", "Drafts may be incomplete.") +
  statement("plain", "Nothing recorded.") +
  statement("tested", "Covered by tests.") +
  "## Metadata\n\n" +
  ["number", "number.unique", "draft", "plain", "tested"]
    .map((id) => `### ${id}\n\n- agent\n${entries[id] ?? ""}\n`)
    .join("\n");

const files: Record<string, string> = {
  [`${tla}/models/numbering/manifest.json`]: JSON.stringify({
    concern: "numbering",
    question: "Is the receipt number unique per vendor?",
    ontology_ids: ["number.unique"],
    runs: [
      {
        label: "safety",
        spec: "Numbering.tla",
        cfg: "safety.cfg",
        kind: "safety",
        variant: "as-built",
        expect: "pass",
      },
      {
        label: "ctl-no-index",
        spec: "Numbering.tla",
        cfg: "ctl.cfg",
        kind: "control",
        variant: "control",
        expect: "violation:Unique",
      },
    ],
  }),
  [`${tla}/models/unrun/manifest.json`]: JSON.stringify({
    question: "Can a draft be incomplete?",
    runs: [{ label: "safety", kind: "safety", expect: "pass" }],
  }),
  [`${tla}/models/numbering/BINDINGS.md`]:
    "# Numbering bindings\n\n| Action | Code |\n|---|---|\n| Create | [Handler](../../../../src/Create.cs#L10) |\n",
  [`${tla}/results/index.jsonl`]: [
    {
      run_id: "20261024T010000Z-safety-a",
      concern: "numbering",
      label: "safety",
      kind: "safety",
      variant: "as-built",
      expect: "pass",
      outcome: "PASS",
      distinct_states: 26827,
      states_generated: 86478,
      depth: 13,
    },
    {
      run_id: "20261023T010000Z-safety-old",
      concern: "numbering",
      label: "safety",
      kind: "safety",
      variant: "as-built",
      expect: "pass",
      outcome: "TIMEOUT",
    },
    {
      run_id: "20261024T020000Z-ctl",
      concern: "numbering",
      label: "ctl-no-index",
      kind: "control",
      variant: "control",
      expect: "violation:Unique",
      outcome: "EXPECTED_VIOLATION",
      distinct_states: 173,
    },
  ]
    .map((row) => JSON.stringify(row))
    .join("\n"),
};
/** Serves `files`; returns the paths read, in order. */
function serve(overrides: { index?: () => Response } = {}) {
  const read: string[] = [];
  server.use(
    http.get("*/api/v1/workspaces/ws/source-control/ontology-baseline", () =>
      HttpResponse.json(
        { error: { code: "NOT_FOUND", message: "none" } },
        { status: 404 },
      ),
    ),
    http.get("*/api/v1/workspaces/ws/files/*", ({ request }) => {
      const file = decodeURIComponent(
        new URL(request.url).pathname.split("/files/")[1],
      );
      read.push(file);
      if (file === `${tla}/results/index.jsonl` && overrides.index)
        return overrides.index();
      const content = files[file];
      return content === undefined
        ? HttpResponse.json(
            { error: { code: "NOT_FOUND", message: "missing" } },
            { status: 404 },
          )
        : HttpResponse.json({
            data: {
              path: file,
              content,
              size: content.length,
              encoding: "utf-8",
              hash: "h",
              source: "live",
              repoOid: "oid",
              snapshotAt: null,
            },
          });
    }),
  );
  return read;
}

function mount(content: string, onOpen = vi.fn()) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  const view = (text: string): ReactElement => (
    <QueryClientProvider client={client}>
      <DocumentPreview
        workspaceId="ws"
        repoId="repo"
        filePath={path}
        content={text}
        onNavigate={vi.fn()}
        onOpenWorkspaceFile={onOpen}
      />
    </QueryClientProvider>
  );
  const { rerender } = render(view(content));
  return Object.assign(onOpen, {
    edit: (text: string) => rerender(view(text)),
  });
}

const statusOf = (id: string) =>
  within(
    document
      .querySelector(`[data-record-span="${id}"]`)!
      .closest<HTMLElement>("[data-testid='semantic-record-shell']")!,
  ).getAllByTestId("verification-status")[0];

describe("ontology verification status", () => {
  const tagged = {
    "number.unique": `- design-verified\n\n${link("numbering")}\n`,
    draft: `- design-verified\n\n${link("unrun")}\n`,
    tested: "- test-coverage\n",
  };

  it("shows each statement's status, Design verified only with evidence", async () => {
    serve();
    mount(source(tagged));
    expect(statusOf("number.unique")).toHaveTextContent("Checking…");
    expect(await screen.findAllByText("Design verified")).toHaveLength(2);
    expect(statusOf("number.unique")).toHaveTextContent("Design verified");
    // The parent shows Design verified through its child.
    expect(statusOf("number")).toHaveTextContent("Design verified");
    // A tag with no recorded run shows Unverified.
    expect(statusOf("draft")).toHaveTextContent("Unverified");
    expect(statusOf("plain")).toHaveTextContent("Unverified");
    expect(statusOf("tested")).toHaveTextContent("Test Coverage");
    expect(statusOf("number")).toHaveAccessibleName(
      "Verification: Design verified",
    );
  });

  it("opens nothing for Unverified, with or without a tag", async () => {
    serve();
    mount(source(tagged));
    await screen.findAllByText("Design verified");
    for (const id of ["draft", "plain"]) {
      expect(statusOf(id)).toHaveTextContent("Unverified");
      expect(statusOf(id).tagName).not.toBe("BUTTON");
      fireEvent.click(statusOf(id));
    }
    expect(screen.queryByTestId("verification-panel")).toBeNull();
  });

  it("shows what the metadata records for other tags, not state machines", async () => {
    serve();
    mount(
      source({
        tested: `- test-coverage\n\nTests: [NumberTests](../../tests/NumberTests.cs) cover reuse.\n\n${link("numbering")}\n\nExpands: [number](#number)\n`,
      }),
    );
    fireEvent.click(statusOf("tested"));
    const panel = screen.getByTestId("verification-panel");
    const details = within(panel).getByTestId("verification-metadata");
    expect(details).toHaveTextContent("cover reuse");
    expect(details).not.toHaveTextContent("test-coverage");
    expect(details).not.toHaveTextContent("Expands");
    expect(within(panel).queryByTestId("verification-machine")).toBeNull();
    expect(statusOf("tested")).toHaveTextContent("Test Coverage");
  });

  it("reads only the linked manifests and the run index, never listing folders", async () => {
    const read = serve();
    mount(source(tagged));
    await screen.findAllByText("Design verified");
    expect([...new Set(read)].sort()).toEqual([
      `${tla}/models/numbering/manifest.json`,
      `${tla}/models/unrun/manifest.json`,
      `${tla}/results/index.jsonl`,
    ]);
  });

  it("starts the run index while the manifests are still being read", async () => {
    const read = serve();
    let release!: () => void;
    const held = new Promise<void>((resolve) => (release = resolve));
    // Manifests wait; the handler below `serve`'s then answers them.
    server.use(
      http.get("*/api/v1/workspaces/ws/files/*", async ({ request }) => {
        if (request.url.includes("manifest.json")) await held;
        return undefined;
      }),
    );
    mount(source(tagged));
    await vi.waitFor(() =>
      expect(read).toContain(`${tla}/results/index.jsonl`),
    );
    expect(read.filter((f) => f.endsWith("manifest.json"))).toEqual([]);
    expect(screen.queryByText("Design verified")).toBeNull();
    release();
    await screen.findAllByText("Design verified");
    // One read of the index, shared by the early start and the check.
    expect(read.filter((f) => f === `${tla}/results/index.jsonl`)).toHaveLength(
      1,
    );
  });

  it("reads stale evidence again when the file changes", async () => {
    serve();
    const view = mount(source(tagged));
    await screen.findAllByText("Design verified");
    expect(statusOf("draft")).toHaveTextContent("Unverified");
    const index = files[`${tla}/results/index.jsonl`];
    files[`${tla}/results/index.jsonl`] =
      `${index}\n${JSON.stringify({ concern: "unrun", label: "safety", outcome: "PASS" })}`;
    const now = Date.now();
    const clock = vi.spyOn(Date, "now").mockReturnValue(now + 60_000);
    try {
      view.edit(`${source(tagged)}\n`);
      await vi.waitFor(() =>
        expect(statusOf("draft")).toHaveTextContent("Design verified"),
      );
    } finally {
      clock.mockRestore();
      files[`${tla}/results/index.jsonl`] = index;
    }
  });

  it("names the children a parent is verified through and opens them", async () => {
    serve();
    const scroll = vi.fn();
    Element.prototype.scrollIntoView = scroll;
    mount(source(tagged));
    await screen.findAllByText("Design verified");
    fireEvent.click(statusOf("number"));
    const panel = screen.getByTestId("verification-panel");
    expect(panel).toHaveTextContent(
      "Design verified through child statements:",
    );
    expect(
      screen
        .getByText("Unique per vendor.", { selector: "p" })
        .closest("[hidden]"),
    ).not.toBeNull();
    fireEvent.click(
      within(panel).getByRole("button", { name: "Unique per vendor." }),
    );
    expect(
      screen
        .getByText("Unique per vendor.", { selector: "p" })
        .closest("[hidden]"),
    ).toBeNull();
    expect(scroll).toHaveBeenCalled();
  });

  it("shows the state machine's question, then its saved bindings and runs", async () => {
    serve();
    const onOpen = mount(source(tagged));
    await screen.findAllByText("Design verified");
    fireEvent.click(screen.getByTestId("ontology-expand-all"));
    fireEvent.click(statusOf("number.unique"));
    const panel = screen.getByTestId("verification-panel");
    expect(
      await within(panel).findByText(
        "Is the receipt number unique per vendor?",
      ),
    ).toBeInTheDocument();
    expect(within(panel).getByRole("heading", { level: 4 })).toHaveTextContent(
      "Numbering: What this state machine checks",
    );
    const details = within(panel)
      .getByText("State machine and verification details")
      .closest("details")!;
    details.open = true;
    fireEvent(details, new Event("toggle"));
    expect(
      await within(panel).findByText("Numbering bindings"),
    ).toBeInTheDocument();
    expect(within(panel).getByText("PASS")).toHaveClass(
      "verification-outcome-matched",
    );
    expect(within(panel).getByText("EXPECTED_VIOLATION")).toHaveClass(
      "verification-outcome-matched",
    );
    expect(within(panel).getByText("26,827")).toBeInTheDocument();
    expect(
      within(panel).getByText("Earlier and retired runs · 1"),
    ).toBeInTheDocument();
    expect(within(panel).getByText("TIMEOUT")).toHaveClass(
      "verification-outcome-incomplete",
    );
    // Bindings links resolve from BINDINGS.md's own folder.
    expect(
      within(panel)
        .getByRole("link", { name: "Handler" })
        .closest("[data-link-base]"),
    ).toHaveAttribute("data-link-base", `${tla}/models/numbering/BINDINGS.md`);
    fireEvent.click(within(panel).getAllByRole("button", { name: "Open" })[0]);
    expect(onOpen).toHaveBeenCalledWith(
      `${tla}/results/numbering/20261024T010000Z-safety-a/receipt.json`,
      "repo",
      undefined,
    );
  });

  it("does not claim Design verified when the evidence cannot be read", async () => {
    serve({
      index: () =>
        HttpResponse.json(
          { error: { code: "INTERNAL", message: "boom" } },
          { status: 500 },
        ),
    });
    mount(source(tagged));
    await screen.findAllByText("Test Coverage");
    await vi.waitFor(() =>
      expect(statusOf("number.unique")).toHaveTextContent("Unverified"),
    );
  });

  it("names a linked state machine whose evidence could not be read, with Retry", async () => {
    const broken = `${tla}/models/broken/manifest.json`;
    serve();
    server.use(
      http.get(`*/api/v1/workspaces/ws/files/${broken}`, () =>
        HttpResponse.json(
          { error: { code: "INTERNAL", message: "boom" } },
          { status: 500 },
        ),
      ),
    );
    mount(
      source({
        "number.unique": `- design-verified\n\n${link("numbering")}; ${link("broken").replace("State machines: ", "")}\n`,
      }),
    );
    await screen.findAllByText("Design verified");
    fireEvent.click(statusOf("number.unique"));
    const panel = screen.getByTestId("verification-panel");
    expect(
      await within(panel).findByText(/broken: its manifest could not be read/),
    ).toBeInTheDocument();
    // Retry reads it again.
    files[broken] = JSON.stringify({ question: "Fixed?", runs: [] });
    server.resetHandlers();
    serve();
    try {
      fireEvent.click(within(panel).getByTestId("verification-retry"));
      await vi.waitFor(() =>
        expect(within(panel).queryByText(/could not be read/)).toBeNull(),
      );
    } finally {
      delete files[broken];
    }
  });

  it("sets no status from two different verification tags", async () => {
    serve();
    mount(source({ plain: "- test-coverage\n- manual-check-review\n" }));
    expect(statusOf("plain")).toHaveTextContent("Unverified");
    fireEvent.click(statusOf("plain"));
    expect(screen.queryByTestId("verification-panel")).toBeNull();
  });
});

describe("findings in the Design verified details (g5.documents.findings)", () => {
  it("marks Design verified with the most severe linked finding still tracked, its children's included", async () => {
    const bug = (id: string, severity: string, decision: string) =>
      `<a id="${id}"></a>\n### Finding ${id}\n\nSeverity: ${severity} · Decision: ${decision}\n\nDetails.\n\n`;
    files[`${tla}/bugs_issues.md`] =
      "# Bugs\n\n" +
      bug(
        "crit-gone",
        "Critical",
        "Irrelevant — user decided this issue is not important.",
      ) +
      bug("high", "High", "Defer — [ACME-4](https://g5.example/i/4)") +
      bug("medium", "Medium", "Open") +
      bug("low", "Low", "Open") +
      bug("unranked", "Unranked", "Open");
    const bugs = (...ids: string[]) =>
      `Findings: ${ids.map((id) => `[${id}](../../TLA_verification/bugs_issues.md#${id})`).join(", ")}`;
    serve();
    mount(
      source({
        // The child links an Irrelevant Critical and a deferred High: High.
        "number.unique": `- design-verified\n\n${link("numbering")}\n\n${bugs("crit-gone", "high")}\n`,
        // Only minor findings: no dot.
        number: `${bugs("low", "unranked")}\n`,
        tested: `- test-coverage\n\n${bugs("medium")}\n`,
      }),
    );
    await screen.findAllByText("Design verified");
    const dot = (id: string) =>
      within(statusOf(id)).queryByTestId("verification-status-dot");
    await waitFor(() => expect(dot("number.unique")).not.toBeNull());
    expect(dot("number.unique")).toHaveAttribute("data-severity", "High");
    expect(statusOf("number.unique")).toHaveAccessibleName(
      "Verification: Design verified, with a High finding",
    );
    // The parent counts its child's findings; its own are minor.
    expect(dot("number")).toHaveAttribute("data-severity", "High");
    // Only Design verified shows a dot.
    expect(statusOf("tested")).toHaveTextContent("Test Coverage");
    expect(dot("tested")).toBeNull();
    delete files[`${tla}/bugs_issues.md`];
  });

  it("shows no dot when every linked finding is minor or Irrelevant", async () => {
    files[`${tla}/bugs_issues.md`] =
      '# Bugs\n\n<a id="low"></a>\n### Low\n\nSeverity: Low · Decision: Open\n\n<a id="gone"></a>\n### Gone\n\nSeverity: High · Decision: Irrelevant — user decided this issue is not important.\n';
    serve();
    mount(
      source({
        "number.unique": `- design-verified\n\n${link("numbering")}\n\nFindings: [low](../../TLA_verification/bugs_issues.md#low), [gone](../../TLA_verification/bugs_issues.md#gone)\n`,
      }),
    );
    await screen.findAllByText("Design verified");
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(
      within(statusOf("number.unique")).queryByTestId(
        "verification-status-dot",
      ),
    ).toBeNull();
    expect(statusOf("number.unique")).toHaveAccessibleName(
      "Verification: Design verified",
    );
    delete files[`${tla}/bugs_issues.md`];
  });

  it("lists the bugs and simplifications the metadata links, with severity and decision", async () => {
    files[`${tla}/bugs_issues.md`] =
      '# Bugs\n\n<a id="rcv-001"></a>\n### Duplicate receipt numbers slip through\n\nSeverity: High · Decision: Defer — [ACME-4](https://g5.example/i/4)\n\nDetails.\n';
    serve();
    const onNavigate = vi.fn();
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    render(
      <QueryClientProvider client={client}>
        <DocumentPreview
          workspaceId="ws"
          repoId="repo"
          filePath={path}
          content={source({
            "number.unique": `- design-verified\n\n${link("numbering")}\n\nBugs: [receipt numbers](../../TLA_verification/bugs_issues.md#rcv-001), [gone](../../TLA_verification/bugs_issues.md#rcv-404)\n`,
            draft:
              "- test-coverage\n\nBugs: [x](../../TLA_verification/bugs_issues.md#rcv-001)\n",
          })}
          onNavigate={onNavigate}
        />
      </QueryClientProvider>,
    );
    await screen.findAllByText("Design verified");
    fireEvent.click(screen.getByTestId("ontology-expand-all"));
    fireEvent.click(statusOf("number.unique"));
    const panel = screen.getByTestId("verification-panel");
    const section = await within(panel).findByTestId("verification-findings");
    expect(section).toHaveTextContent("Related bugs and simplifications");
    const [found, missing] = await within(section).findAllByTestId(
      "verification-finding",
    );
    await waitFor(() =>
      expect(found).toHaveTextContent(
        "High Duplicate receipt numbers slip through · Defer ACME-4",
      ),
    );
    expect(within(found).getByRole("link", { name: "ACME-4" })).toHaveAttribute(
      "href",
      "https://g5.example/i/4",
    );
    expect(missing).toHaveTextContent(
      "rcv-404 was not found in bugs_issues.md.",
    );
    fireEvent.click(
      within(found).getByRole("button", {
        name: "Duplicate receipt numbers slip through",
      }),
    );
    expect(onNavigate).toHaveBeenCalledWith(
      expect.objectContaining({
        type: "file",
        id: `${tla}/bugs_issues.md`,
        intentRecordNumber: "#rcv-001",
      }),
    );
    // Other statuses show their metadata, not a findings section.
    fireEvent.click(statusOf("draft"));
    expect(screen.getAllByTestId("verification-findings")).toHaveLength(1);
    delete files[`${tla}/bugs_issues.md`];
  });
});
