// Findings files in Preview (g5.documents.findings): severity, decisions that
// file or change one issue, the decision line written live, and the filters.
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { HttpResponse, http } from "msw";
import { Route, Routes } from "react-router-dom";
import { act, useEffect, useState } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { renderApp } from "../../test/render";
import { createMockUser } from "../../test/fixtures";
import {
  makeIssueCreateTarget,
  makeIssueCreation,
  makeIssueSummary,
  serveExternalIssuesFeature,
} from "../../test/externalIssueFixtures";
import { server } from "../../test/msw/server";
import { DocumentPreview } from "./DocumentPreview";
import { findingDiscussPrompt, parseFindings } from "../../lib/findings";

const PATH = ".g5/TLA_verification/bugs_issues.md";
const ORG = "/api/v1/orgs/:orgId";
const entry = (id: string, title: string, status: string, body = "") =>
  `<a id="${id}"></a>\n### ${title}\n\n${status}\n\n${body}\n`;

const FILE =
  "# Bugs and issues\n\nFound by the run of 2026-10-31.\n\n## Actionable\n\n" +
  entry(
    "rcv-001",
    "Duplicate receipt lines bypass the ceiling",
    "Severity: Critical · Decision: Open",
    "Two lines each pass alone. See [the rule](../ontology/receiving.g5.md#ceiling).",
  ) +
  entry(
    "rcv-002",
    "A stale draft receives against a closed order",
    "Severity: High · Decision: Need fix — [acme/api#12](https://github.com/acme/api/issues/12)",
  ) +
  entry(
    "rcv-003",
    "Receipt switch skips ownership",
    "Severity: Medium · Decision: Irrelevant — user decided this issue is not important. Closed [acme/api#12](https://github.com/acme/api/issues/12).",
  ) +
  entry(
    "rcv-004",
    "Imports accept a weird value",
    "Severity: Low · Decision: Maybe",
  ) +
  "\n## Other entries\n\n" +
  entry(
    "rcv-q-001",
    "Whether recall is allowed is undecided",
    "Severity: Medium",
  );

interface Requests {
  checks: Record<string, unknown>[];
  creates: Record<string, unknown>[];
  resolves: string[];
  edits: Record<string, unknown>[];
  states: Record<string, unknown>[];
}

function serveTracker({
  found = false,
  issue = makeIssueSummary({
    title: "[High] A stale draft receives against a closed order",
    contentTrusted: true,
  }),
  trusted = true,
}: {
  found?: boolean;
  issue?: ReturnType<typeof makeIssueSummary>;
  trusted?: boolean;
} = {}): Requests {
  const requests: Requests = {
    checks: [],
    creates: [],
    resolves: [],
    edits: [],
    states: [],
  };
  server.use(
    http.get(`${ORG}/issues/create-target`, () =>
      HttpResponse.json({
        data: makeIssueCreateTarget({
          destination: {
            ...makeIssueCreateTarget().destination!,
            contentTrusted: trusted,
          },
        }),
      }),
    ),
    http.post(`${ORG}/issues`, async ({ request }) => {
      const body = (await request.json()) as Record<string, unknown>;
      if (body.checkOnly) {
        requests.checks.push(body);
        return HttpResponse.json({
          data: found
            ? makeIssueCreation({ sent: false, title: String(body.title) })
            : makeIssueCreation({ state: "not_found", issue: null }),
        });
      }
      requests.creates.push(body);
      return HttpResponse.json(
        { data: makeIssueCreation({ title: String(body.title) }) },
        { status: 201 },
      );
    }),
    http.get(`${ORG}/issues/resolve`, ({ request }) => {
      requests.resolves.push(new URL(request.url).searchParams.get("ref")!);
      return HttpResponse.json({ data: issue });
    }),
    http.patch(`${ORG}/issues/github/:repo/:number`, async ({ request }) => {
      const body = (await request.json()) as Record<string, unknown>;
      requests.edits.push(body);
      return HttpResponse.json({
        data: {
          outcome: "applied",
          issue: { ...issue, title: body.title },
          githubLogin: "alex-gh",
          message: null,
        },
      });
    }),
    http.post(
      `${ORG}/issues/github/:repo/:number/state`,
      async ({ request }) => {
        const body = (await request.json()) as Record<string, unknown>;
        requests.states.push(body);
        return HttpResponse.json({
          data: {
            requested: body.state,
            outcome: "applied",
            issue: { ...issue, state: body.state },
            githubLogin: "alex-gh",
            message: null,
          },
        });
      },
    ),
  );
  return requests;
}

function mount(
  options: {
    path?: string;
    content?: string;
    blockedReason?: string | null;
    write?: (expected: string, next: string) => Promise<void>;
    onDiscuss?: (text: string) => void;
    onCommentSelection?: (at: { line: number; quote: string }) => void;
  } = {},
) {
  const write = vi.fn(options.write ?? (async () => {}));
  renderApp(
    <Routes>
      <Route
        path="/:org/:workspace/file/*"
        element={
          <DocumentPreview
            workspaceId="ws"
            repoId="repo"
            filePath={options.path ?? PATH}
            content={options.content ?? FILE}
            onDiscussFinding={options.onDiscuss}
            onCommentSelection={options.onCommentSelection}
            findings={{
              blockedReason: options.blockedReason ?? null,
              write,
            }}
          />
        }
      />
    </Routes>,
    {
      route: `/g5labs/current/file/${options.path ?? PATH}`,
      user: createMockUser(),
    },
  );
  return write;
}

const RUNS = [
  {
    run_id: "20261024T012111Z-asbuilt-dup-a",
    concern: "receipt-submit",
    label: "asbuilt-dup",
    kind: "divergence",
    variant: "as-built",
    expect: "violation:NoOverReceipt",
    outcome: "EXPECTED_VIOLATION",
    distinct_states: 3,
  },
  {
    run_id: "20261024T011905Z-intended-safety-b",
    concern: "receipt-submit",
    label: "intended-safety",
    kind: "safety",
    variant: "intended",
    expect: "pass",
    outcome: "PASS",
    distinct_states: 280850,
  },
  {
    run_id: "20261024T020000Z-liveness-asbuilt-c",
    concern: "order-conversion",
    label: "liveness-asbuilt",
    kind: "liveness",
    variant: "as-built",
    expect: "pass",
    outcome: "PASS",
    distinct_states: 50297,
  },
  {
    run_id: "20261024T020100Z-liveness-simplified-d",
    concern: "order-conversion",
    label: "liveness-simplified",
    kind: "liveness",
    variant: "simplification",
    expect: "pass",
    outcome: "PASS",
    distinct_states: 28059,
  },
  {
    run_id: "20261024T020200Z-split-e",
    concern: "order-conversion",
    label: "split",
    kind: "safety",
    variant: "candidate",
    expect: "pass",
    outcome: "VIOLATION(Inv)",
    distinct_states: 29,
  },
  {
    run_id: "20261024T020300Z-bigger-f",
    concern: "order-conversion",
    label: "bigger",
    kind: "liveness",
    variant: "simplification",
    expect: "pass",
    outcome: "PASS",
    distinct_states: 57900,
  },
];
const receipt = (i: number) =>
  `results/${RUNS[i].concern}/${RUNS[i].run_id}/receipt.json`;

/** Serves the run index next to the findings files; returns how often it was read. */
function serveRunIndex() {
  const reads: string[] = [];
  server.use(
    http.get(
      "*/api/v1/workspaces/ws/files/.g5/TLA_verification/results/index.jsonl",
      () => {
        reads.push("index");
        const content = RUNS.map((r) => JSON.stringify(r)).join("\n");
        return HttpResponse.json({
          data: {
            path: ".g5/TLA_verification/results/index.jsonl",
            content,
            size: content.length,
            encoding: "utf-8",
            hash: "h",
            source: "live",
            repoOid: "oid",
            snapshotAt: null,
          },
        });
      },
    ),
  );
  return reads;
}

const row = (id: string) =>
  document.querySelector<HTMLElement>(
    `[data-testid='finding-row'][data-entry-id='${id}']`,
  )!;
const action = (id: string, kind: string) =>
  within(row(id)).getByRole("button", { name: kind });
/** The decision part of an entry's status line in what was written. */
const writtenDecision = (write: ReturnType<typeof vi.fn>, id: string) => {
  const next = write.mock.calls.at(-1)![1] as string;
  return parseFindings(next).entries.find((e) => e.id === id)!;
};

describe("findings presentation", () => {
  beforeEach(() => serveExternalIssuesFeature());

  it("shows each entry's severity and, for actionable entries, its decision and buttons", async () => {
    serveTracker();
    mount();
    await screen.findAllByTestId("finding-row");
    expect(
      within(row("rcv-001")).getByTestId("finding-severity"),
    ).toHaveTextContent("Critical");
    expect(
      within(row("rcv-001")).getByTestId("finding-decision"),
    ).toHaveTextContent("Open");
    expect(
      within(row("rcv-001")).getByTestId("finding-decision"),
    ).toHaveAttribute("data-decision", "Open");
    // Each entry is its own card, edged in its severity's colour.
    expect(row("rcv-001").closest(".finding-entry")).toHaveAttribute(
      "data-severity",
      "critical",
    );
    expect(screen.queryByTestId("findings-notice")).toBeNull();
    expect(
      within(row("rcv-002")).getByRole("link", { name: "acme/api#12" }),
    ).toHaveAttribute("href", "https://github.com/acme/api/issues/12");
    expect(action("rcv-002", "Need fix")).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(action("rcv-002", "Need fix")).toBeDisabled();
    expect(row("rcv-003")).toHaveTextContent(
      "Irrelevant · acme/api#12 (closed)",
    );
    // A decision G5 does not write gets a note, not buttons.
    expect(
      within(row("rcv-004")).queryByRole("button", { name: "Defer" }),
    ).toBeNull();
    expect(
      within(row("rcv-004")).getByTestId("finding-malformed"),
    ).toBeVisible();
    // Other entries: severity only.
    expect(
      within(row("rcv-q-001")).queryAllByTestId("finding-action"),
    ).toHaveLength(0);
    expect(
      within(row("rcv-q-001")).getByTestId("finding-severity"),
    ).toHaveTextContent("Medium");
    // The status line itself is not shown as text.
    expect(screen.queryByText(/Severity: Critical/)).toBeNull();
  });

  it("reaches an entry from a link to its anchor", async () => {
    serveTracker();
    renderApp(
      <Routes>
        <Route
          path="/:org/:workspace/file/*"
          element={
            <DocumentPreview
              workspaceId="ws"
              repoId="repo"
              filePath={PATH}
              content={FILE}
              target="#rcv-q-001"
              findings={{ blockedReason: null }}
            />
          }
        />
      </Routes>,
      { route: `/g5labs/current/file/${PATH}`, user: createMockUser() },
    );
    await screen.findAllByTestId("finding-row");
    expect(screen.queryByText(/Missing target/)).toBeNull();
    expect(
      document.querySelector("[data-finding-id='rcv-q-001']"),
    ).toContainElement(row("rcv-q-001"));
  });

  it("filters entries by severity", async () => {
    serveTracker();
    mount();
    await screen.findAllByTestId("finding-row");
    const medium = screen
      .getAllByTestId("finding-filter")
      .find((b) => b.dataset.severity === "Medium")!;
    expect(medium).toHaveTextContent("Medium 2");
    await userEvent.click(medium);
    expect(medium).toHaveAttribute("aria-pressed", "false");
    // The document hides the severity; CSS matches each entry's own.
    const hidden = (id: string) => {
      const severity = row(id)
        .closest<HTMLElement>("[data-finding-entry]")!
        .getAttribute("data-severity");
      return screen
        .getByTestId("findings-document")
        .hasAttribute(`data-hide-${severity}`);
    };
    expect(hidden("rcv-003")).toBe(true);
    expect(hidden("rcv-q-001")).toBe(true);
    expect(hidden("rcv-001")).toBe(false);
    await userEvent.click(medium);
    expect(hidden("rcv-003")).toBe(false);
  });

  it("reads as a filter, and a filtered page shows only entries and the headings that introduce them", async () => {
    serveTracker();
    mount();
    await screen.findAllByTestId("finding-row");
    const filter = screen.getByRole("group", { name: "Filter by severity" });
    expect(filter).toHaveTextContent("Filter by severity");
    expect(screen.getByTestId("findings-shown")).toHaveTextContent("5 entries");
    // No entry names its run, so there is no run filter.
    expect(screen.queryByTestId("findings-run-filter")).toBeNull();
    expect(screen.queryByText("Run")).toBeNull();
    const reset = screen.getByRole("button", { name: "Show all" });
    expect(reset).toBeDisabled();
    const document_ = screen.getByTestId("findings-document");
    const intro = screen.getByText("Found by the run of 2026-10-31.");
    const title = screen.getByRole("heading", { name: "Bugs and issues" });
    const actionable = screen.getByRole("heading", { name: "Actionable" });
    const other = screen.getByRole("heading", { name: "Other entries" });
    // The description is wrapped apart from entries; CSS hides it when filtered.
    expect(intro.closest(".findings-context")).not.toBeNull();
    expect(title.closest(".findings-context")).not.toBeNull();
    expect(actionable.closest(".findings-section-heading")).not.toBeNull();
    expect(document_).not.toHaveAttribute("data-filtered");

    const toggle = (severity: string) =>
      userEvent.click(
        within(filter)
          .getAllByTestId("finding-filter")
          .find((b) => b.dataset.severity === severity)!,
      );
    await toggle("Medium");
    expect(document_).toHaveAttribute("data-filtered");
    expect(screen.getByTestId("findings-shown")).toHaveTextContent(
      "Showing 3 of 5 entries",
    );
    // "Other entries" holds only a Medium entry, so its heading goes too.
    expect(other.closest(".findings-section-heading")).toHaveAttribute(
      "hidden",
    );
    expect(actionable.closest(".findings-section-heading")).not.toHaveAttribute(
      "hidden",
    );
    expect(reset).toBeEnabled();
    await userEvent.click(reset);
    expect(document_).not.toHaveAttribute("data-filtered");
    expect(other.closest(".findings-section-heading")).not.toHaveAttribute(
      "hidden",
    );
  });

  it("disables decisions with the reason when the owner cannot decide here", async () => {
    serveTracker();
    mount({
      blockedReason:
        "The workspace owner decides here while the workspace is running.",
    });
    await screen.findAllByTestId("finding-row");
    expect(action("rcv-001", "Defer")).toBeDisabled();
    expect(action("rcv-001", "Defer")).toHaveAttribute(
      "title",
      "The workspace owner decides here while the workspace is running.",
    );
    // Said once for the page, too.
    expect(screen.getByTestId("findings-notice")).toHaveTextContent(
      "The workspace owner decides here while the workspace is running.",
    );
  });

  it("seeds the chat with the entry", async () => {
    serveTracker();
    const onDiscuss = vi.fn();
    mount({ onDiscuss });
    await screen.findAllByTestId("finding-row");
    await userEvent.click(within(row("rcv-002")).getByTestId("finding-chat"));
    expect(onDiscuss).toHaveBeenCalledWith(
      'Let\'s discuss the bug "A stale draft receives against a closed order" (rcv-002, severity High, decision Need fix with acme/api#12) in .g5/TLA_verification/bugs_issues.md.',
    );
    const [first] = parseFindings(FILE).entries;
    expect(
      findingDiscussPrompt(first, "design_simplification.md", "x.md"),
    ).toContain("the design simplification");
  });
});

describe("findings evidence", () => {
  beforeEach(() => serveExternalIssuesFeature());

  it("shows the runs that found a bug and what reproduced it, or that nothing did", async () => {
    serveTracker();
    serveRunIndex();
    mount({
      content:
        "# Bugs and issues\n\n## Actionable\n\n" +
        entry(
          "rcv-001",
          "Duplicate lines bypass the ceiling",
          "Severity: High · Decision: Open",
          `Found by: [asbuilt-dup](${receipt(0)}), [intended-safety](${receipt(1)}), [gone](results/receipt-submit/missing/receipt.json)\n\nReproduced by: [ReceivingTests](../../src/ReceivingTests.cs#L40) failed with two lines.\n\nWhat goes wrong.`,
        ) +
        // Lines written without a blank line between them, and no reproduction.
        entry(
          "rcv-002",
          "Drafts keep a stale order",
          "Severity: Medium · Decision: Open",
          "Found by: source review\nReproduced by: none\n\nWhy.",
        ) +
        entry(
          "rcv-003",
          "Imports accept negative numbers",
          "Severity: Low · Decision: Open",
          "No evidence lines.",
        ),
    });
    await screen.findAllByTestId("finding-row");
    const card = (id: string) =>
      row(id).closest<HTMLElement>(".finding-entry")!;
    const evidence = (id: string, kind: string) =>
      card(id).querySelector<HTMLElement>(
        `[data-testid='finding-evidence'][data-kind='${kind}']`,
      )!;
    const found = evidence("rcv-001", "found-by");
    expect(found).toHaveTextContent("Found by");
    // Closed until asked for: the summary counts the runs and machines.
    expect(found).toHaveTextContent(
      "3 recorded runs of 1 state machine that showed this problem",
    );
    expect(within(found).queryAllByTestId("verification-run-row")).toHaveLength(
      0,
    );
    await userEvent.click(within(found).getByText("Found by"));
    const rows = await within(found).findAllByTestId("verification-run-row");
    expect(rows).toHaveLength(3);
    await waitFor(() => expect(rows[0]).toHaveTextContent("asbuilt-dup"));
    expect(rows[0]).toHaveTextContent("Receipt submit · divergence · as-built");
    expect(
      within(rows[0]).getByTestId("verification-run-outcome"),
    ).toHaveTextContent("EXPECTED_VIOLATION");
    expect(rows[1]).toHaveTextContent("280,850");
    expect(rows[2]).toHaveTextContent("Not in the run index");
    const reproduced = evidence("rcv-001", "reproduced-by");
    expect(reproduced).toHaveTextContent("Reproduced by");
    expect(
      within(reproduced).getByRole("link", { name: "ReceivingTests" }),
    ).toBeVisible();
    expect(reproduced).toHaveTextContent("failed with two lines.");
    expect(reproduced).not.toHaveAttribute("data-empty");
    // No receipts: the line as written. `none`, or no line at all: the
    // heading alone, in bold, and nothing under it.
    expect(evidence("rcv-002", "found-by")).toHaveTextContent("source review");
    for (const id of ["rcv-002", "rcv-003"]) {
      const missing = evidence(id, "reproduced-by");
      expect(missing).toHaveAttribute("data-empty", "true");
      expect(missing).toHaveClass("finding-evidence-missing");
      expect(missing).toHaveTextContent(
        /^Reproduced by\s*A test or probe that made it happen in the code\s*None recorded\.$/,
      );
      expect(missing).not.toHaveTextContent(/no reproduction evidence/i);
      expect(missing.querySelector(".finding-evidence-body")).toBeNull();
    }
    // The status line and the evidence labels are not shown as raw text.
    expect(screen.queryByText(/^Found by:/)).toBeNull();
  });

  it.each([", not like for like.", ". Scope qualification:"])(
    "reports recorded state-count scores with scope and outcome qualifications (%s)",
    async (scopeMarker) => {
      serveTracker();
      serveRunIndex();
      const simp = (id: string, line: string) =>
        entry(
          id,
          `Simplification ${id}`,
          "Severity: Medium · Decision: Open",
          `${line}\n\nWhy it is simpler.`,
        );
      mount({
        path: ".g5/TLA_verification/design_simplification.md",
        content:
          "# Design simplifications\n\n## Proposals\n\n" +
          simp(
            "s1",
            `Comparison: [As-built](${receipt(2)}) → [Proposed simplification](${receipt(3)}). Both runs used the same bounds.`,
          ) +
          simp(
            "s2",
            `Comparison: [As-built](${receipt(2)}) → none. The proposal was not run.`,
          ) +
          simp(
            "s3",
            `Comparison: [As-built](${receipt(2)}) → [Proposed repair](${receipt(3)})${scopeMarker} The repair also changes guards.`,
          ) +
          simp(
            "s4",
            `Comparison: [As-built](${receipt(2)}) → [Split commit](${receipt(4)}). The split stops at a counterexample.`,
          ) +
          simp("s5", "Comparison: none. No model covers this change.") +
          simp(
            "s6",
            `Comparison: [As-built](${receipt(2)}) → [Proposed simplification](${receipt(5)}).`,
          ) +
          simp(
            "s7",
            `Comparison: [As-built](${receipt(2)}) → [Same model](${receipt(2)}).`,
          ),
      });
      await screen.findAllByTestId("finding-row");
      const comparison = (id: string) =>
        row(id)
          .closest<HTMLElement>(".finding-entry")!
          .querySelector<HTMLElement>("[data-testid='finding-comparison']")!;
      const s1 = comparison("s1");
      expect(s1).toHaveTextContent("Evidence for the simplification");
      await waitFor(() =>
        expect(within(s1).getByTestId("finding-reduction")).toHaveTextContent(
          "44.2% fewer explored states",
        ),
      );
      expect(s1).toHaveTextContent("Proposal / baseline: 0.558×");
      const [asBuilt, simplified] = within(s1).getAllByTestId(
        "verification-run-row",
      );
      expect(asBuilt).toHaveTextContent("As-built");
      expect(asBuilt).toHaveTextContent("50,297");
      expect(simplified).toHaveTextContent("Proposed simplification");
      expect(simplified).toHaveTextContent("28,059");
      expect(
        within(s1).getAllByRole("button", { name: "Inspect run" }),
      ).toHaveLength(2);
      expect(s1).toHaveTextContent("Both runs used the same bounds.");
      // The comparison ends the entry, after the text it supports.
      const s1Entry = row("s1").closest<HTMLElement>(".finding-entry")!;
      expect(
        within(s1Entry)
          .getByText("Why it is simpler.")
          .compareDocumentPosition(s1) & Node.DOCUMENT_POSITION_FOLLOWING,
      ).toBeTruthy();
      expect(s1Entry.lastElementChild).toBe(s1);
      await waitFor(() =>
        expect(
          within(comparison("s2")).getByTestId("finding-no-reduction"),
        ).toHaveTextContent("No proposed-model run is linked"),
      );
      expect(
        within(comparison("s2")).getAllByTestId("verification-run-row")[1],
      ).toHaveTextContent("No run linked");
      expect(comparison("s2")).toHaveTextContent("The proposal was not run.");
      expect(
        within(comparison("s3")).getByTestId("finding-reduction"),
      ).toHaveTextContent("44.2% fewer explored states");
      expect(comparison("s3")).toHaveTextContent(
        "The runs differ in scope or behavior",
      );
      expect(comparison("s3")).toHaveTextContent(
        "The repair also changes guards.",
      );
      expect(
        within(comparison("s4")).getByTestId("finding-reduction"),
      ).toHaveTextContent("99.9% fewer explored states");
      expect(comparison("s4")).toHaveTextContent(
        "did not complete a passing exploration",
      );
      expect(
        within(comparison("s5")).queryAllByTestId("verification-run-row"),
      ).toHaveLength(0);
      expect(comparison("s5")).toHaveTextContent("No as-built run is linked");
      expect(comparison("s5")).toHaveTextContent(
        "No state-count score available",
      );
      // More states, or as many, are stated plainly and not marked as a reduction.
      const more = within(comparison("s6")).getByTestId("finding-reduction");
      expect(more).toHaveTextContent("15.1% more explored states");
      expect(more).toHaveTextContent("Proposal / baseline: 1.151×");
      expect(more).toHaveClass("finding-no-reduction");
      expect(
        within(comparison("s7")).getByTestId("finding-reduction"),
      ).toHaveTextContent("0% change in explored states");
      expect(within(s1).getByTestId("finding-reduction")).toHaveClass(
        "finding-reduction",
      );
      expect(comparison("s5")).toHaveTextContent(
        "No model covers this change.",
      );
    },
  );
});

describe("findings run filter", () => {
  beforeEach(() => serveExternalIssuesFeature());

  const UPLIFT = "Uplift 2026-10-24";
  const BASE = "Base re-verification 2026-11-06 (incremental)";
  const ran = (label: string, body = "Why.") => `Run: ${label}\n\n${body}`;
  const RUNS_FILE =
    "# Bugs and issues\n\nFound by the runs below.\n\n## Actionable\n\n" +
    entry(
      "a-1",
      "Critical one",
      "Severity: Critical · Decision: Open",
      ran(UPLIFT),
    ) +
    entry("a-2", "High one", "Severity: High · Decision: Open", ran(BASE)) +
    entry("a-3", "Medium one", "Severity: Medium · Decision: Open", "Why.") +
    entry("a-4", "Low one", "Severity: Low · Decision: Open", ran(UPLIFT)) +
    "\n## Other entries\n\n" +
    entry("q-1", "Other one", "Severity: Medium", ran(BASE));

  const runFilter = () =>
    screen.getByRole<HTMLSelectElement>("combobox", { name: "Run" });
  /** Whether an entry shows under both filters: its own and the document's. */
  const shows = (id: string) => {
    const card = row(id).closest<HTMLElement>("[data-finding-entry]")!;
    return (
      !card.hidden &&
      !screen
        .getByTestId("findings-document")
        .hasAttribute(`data-hide-${card.dataset.severity}`)
    );
  };
  const shown = () =>
    ["a-1", "a-2", "a-3", "a-4", "q-1"].filter((id) => shows(id));
  const headingHidden = (name: string) =>
    screen
      .getByRole("heading", { name, hidden: true })
      .closest(".findings-section-heading")!
      .hasAttribute("hidden");
  const toggle = (severity: string) =>
    userEvent.click(
      screen
        .getAllByTestId("finding-filter")
        .find((b) => b.dataset.severity === severity)!,
    );

  it("offers all runs, each run in the file's order, then the entries naming none", async () => {
    serveTracker();
    mount({ content: RUNS_FILE });
    await screen.findAllByTestId("finding-row");
    const select = runFilter();
    expect(select).toHaveAttribute("data-testid", "findings-run-filter");
    expect(
      within(select)
        .getAllByRole("option")
        .map((o) => o.textContent),
    ).toEqual(["All runs", UPLIFT, BASE, "No run named"]);
    expect(select).toHaveDisplayValue("All runs");
    expect(screen.getByTestId("findings-document")).not.toHaveAttribute(
      "data-filtered",
    );
    expect(shown()).toEqual(["a-1", "a-2", "a-3", "a-4", "q-1"]);
    // The line itself stays in the entry, under the status line and before
    // the evidence, here an empty Reproduced by.
    const card = row("a-1").closest<HTMLElement>("[data-finding-entry]")!;
    const order = [
      row("a-1"),
      within(card).getByText(`Run: ${UPLIFT}`),
      within(card).getByTestId("finding-evidence"),
      within(card).getByText("Why."),
    ];
    for (let i = 1; i < order.length; i++)
      expect(
        order[i - 1].compareDocumentPosition(order[i]) &
          Node.DOCUMENT_POSITION_FOLLOWING,
      ).toBeTruthy();
  });

  it("offers No run named only when some entry names none", async () => {
    serveTracker();
    mount({
      content:
        "# Bugs and issues\n\n" +
        entry(
          "a-1",
          "Critical one",
          "Severity: Critical · Decision: Open",
          ran(BASE),
        ) +
        entry(
          "a-2",
          "High one",
          "Severity: High · Decision: Open",
          ran(UPLIFT),
        ) +
        entry("a-3", "Low one", "Severity: Low · Decision: Open", ran(BASE)),
    });
    await screen.findAllByTestId("finding-row");
    expect(
      within(runFilter())
        .getAllByRole("option")
        .map((o) => o.textContent),
    ).toEqual(["All runs", BASE, UPLIFT]);
  });

  it("shows one run's entries and the headings that introduce them", async () => {
    serveTracker();
    mount({ content: RUNS_FILE });
    await screen.findAllByTestId("finding-row");
    const document_ = screen.getByTestId("findings-document");
    const intro = screen.getByText("Found by the runs below.");
    await userEvent.selectOptions(runFilter(), UPLIFT);
    expect(shown()).toEqual(["a-1", "a-4"]);
    expect(document_).toHaveAttribute("data-filtered");
    expect(intro.closest(".findings-context")).not.toBeNull();
    expect(screen.getByTestId("findings-shown")).toHaveTextContent(
      "Showing 2 of 5 entries",
    );
    expect(headingHidden("Actionable")).toBe(false);
    expect(headingHidden("Other entries")).toBe(true);

    await userEvent.selectOptions(runFilter(), BASE);
    expect(shown()).toEqual(["a-2", "q-1"]);
    expect(headingHidden("Other entries")).toBe(false);

    await userEvent.selectOptions(runFilter(), "No run named");
    expect(shown()).toEqual(["a-3"]);
    expect(headingHidden("Actionable")).toBe(false);
    expect(headingHidden("Other entries")).toBe(true);

    await userEvent.selectOptions(runFilter(), "All runs");
    expect(shown()).toEqual(["a-1", "a-2", "a-3", "a-4", "q-1"]);
    expect(document_).not.toHaveAttribute("data-filtered");
    expect(headingHidden("Other entries")).toBe(false);
  });

  it("combines with the severity filter, and Show all clears both", async () => {
    serveTracker();
    mount({ content: RUNS_FILE });
    await screen.findAllByTestId("finding-row");
    const reset = screen.getByRole("button", { name: "Show all" });
    await userEvent.selectOptions(runFilter(), BASE);
    expect(reset).toBeEnabled();
    await toggle("Medium");
    // Only the High entry is from that run and not hidden by severity.
    expect(shown()).toEqual(["a-2"]);
    expect(screen.getByTestId("findings-shown")).toHaveTextContent(
      "Showing 1 of 5 entries",
    );
    // The other section's only entry is from that run but Medium.
    expect(headingHidden("Other entries")).toBe(true);
    await toggle("High");
    expect(shown()).toEqual([]);
    expect(headingHidden("Actionable")).toBe(true);
    // The severity counts stay the file's.
    expect(
      screen
        .getAllByTestId("finding-filter")
        .find((b) => b.dataset.severity === "Medium"),
    ).toHaveTextContent("Medium 2");

    await userEvent.click(reset);
    expect(runFilter()).toHaveDisplayValue("All runs");
    expect(shown()).toEqual(["a-1", "a-2", "a-3", "a-4", "q-1"]);
    expect(screen.getByTestId("findings-document")).not.toHaveAttribute(
      "data-filtered",
    );
    expect(reset).toBeDisabled();
    expect(headingHidden("Actionable")).toBe(false);
  });

  it("writes a decision without touching the Run: line", async () => {
    serveTracker();
    const write = mount({ content: RUNS_FILE });
    await waitFor(() => expect(action("a-1", "Irrelevant")).toBeEnabled());
    await userEvent.selectOptions(runFilter(), UPLIFT);
    await userEvent.click(action("a-1", "Irrelevant"));
    await waitFor(() => expect(write).toHaveBeenCalledTimes(1));
    expect(write.mock.calls[0][1]).toBe(
      RUNS_FILE.replace(
        "Severity: Critical · Decision: Open\n",
        "Severity: Critical · Decision: Irrelevant — user decided this issue is not important.\n",
      ),
    );
    expect(writtenDecision(write, "a-1")).toMatchObject({
      decision: "Irrelevant",
      run: UPLIFT,
    });
  });
});

describe("findings and selection commenting", () => {
  beforeEach(() => serveExternalIssuesFeature());

  /** Selects `text` inside `element` and releases the mouse in the document. */
  const select = (element: HTMLElement, text: string) => {
    const walker = document.createTreeWalker(element, NodeFilter.SHOW_TEXT);
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      const at = node.textContent?.indexOf(text) ?? -1;
      if (at < 0) continue;
      const range = document.createRange();
      range.setStart(node, at);
      range.setEnd(node, at + text.length);
      window.getSelection()!.removeAllRanges();
      window.getSelection()!.addRange(range);
      fireEvent.mouseUp(screen.getByTestId("document-preview-content"));
      return;
    }
    throw new Error(`no text ${text}`);
  };
  const settle = () => new Promise((resolve) => setTimeout(resolve, 10));

  it("offers Comment on an entry's text but not on its controls or the filter", async () => {
    // `g5.documents.comments.identity`: only the file's own text is commented.
    serveTracker();
    mount({ onCommentSelection: vi.fn() });
    await screen.findAllByTestId("finding-row");
    select(row("rcv-001"), "Need fix");
    await settle();
    expect(
      screen.queryByTestId("document-preview-selection-comment-button"),
    ).toBeNull();
    select(screen.getByTestId("findings-toolbar"), "Filter by severity");
    await settle();
    expect(
      screen.queryByTestId("document-preview-selection-comment-button"),
    ).toBeNull();
    select(screen.getByText(/Two lines each pass alone/), "Two lines");
    expect(
      await screen.findByTestId("document-preview-selection-comment-button"),
    ).toBeVisible();
  });
});

describe("findings decisions", () => {
  beforeEach(() => serveExternalIssuesFeature());

  it("Need fix files one [High] issue under the entry's key, then writes the line live", async () => {
    const requests = serveTracker();
    const write = mount();
    await waitFor(() => expect(action("rcv-001", "Need fix")).toBeEnabled());
    await userEvent.click(action("rcv-001", "Need fix"));
    await waitFor(() => expect(write).toHaveBeenCalledTimes(1));
    const key = `finding:repo:${PATH}#rcv-001`;
    expect(requests.checks).toHaveLength(1);
    expect(requests.checks[0]).toMatchObject({ idempotencyKey: key });
    expect(requests.creates).toHaveLength(1);
    const created = requests.creates[0];
    expect(created).toMatchObject({
      title: "[High] Duplicate receipt lines bypass the ceiling",
      assignees: [],
      idempotencyKey: key,
      domainId: "domain-1",
    });
    expect(created.parent).toBeUndefined();
    expect(String(created.body)).toMatch(
      /^Two lines each pass alone\. See \[the rule\]\(http:\/\/[^)]+\/g5labs\/current\/file\/\.g5\/ontology\/receiving\.g5\.md\?branch=repo#ceiling\)\.\n\nFrom \[Bugs and issues: Duplicate receipt lines bypass the ceiling\]\(http:\/\/[^)]+#rcv-001\)$/,
    );
    expect(write.mock.calls[0][0]).toBe(FILE);
    expect(writtenDecision(write, "rcv-001")).toMatchObject({
      decision: "Need fix",
      issue: {
        key: "acme/api#77",
        url: "https://github.com/acme/api/issues/77",
      },
    });
    expect(
      await within(row("rcv-001")).findByTestId("finding-phase"),
    ).toHaveTextContent("committed with its next save");
  });

  it("finds an issue an earlier press filed and retitles it instead of creating another", async () => {
    const requests = serveTracker({ found: true });
    const write = mount();
    await waitFor(() => expect(action("rcv-001", "Defer")).toBeEnabled());
    await userEvent.click(action("rcv-001", "Defer"));
    await waitFor(() => expect(write).toHaveBeenCalledTimes(1));
    expect(requests.creates).toHaveLength(0);
    expect(requests.resolves).toEqual(["acme/api#77"]);
    expect(requests.edits).toEqual([
      {
        title: "[Low] A stale draft receives against a closed order",
        seen: { title: "[High] A stale draft receives against a closed order" },
      },
    ]);
    expect(writtenDecision(write, "rcv-001").decision).toBe("Defer");
  });

  it("Defer retitles the recorded issue; Irrelevant closes it", async () => {
    const requests = serveTracker();
    const write = mount();
    await waitFor(() => expect(action("rcv-002", "Defer")).toBeEnabled());
    await userEvent.click(action("rcv-002", "Defer"));
    await waitFor(() => expect(write).toHaveBeenCalledTimes(1));
    expect(requests.resolves).toEqual(["acme/api#12"]);
    expect(requests.edits[0]).toMatchObject({
      title: "[Low] A stale draft receives against a closed order",
    });
    expect(requests.states).toHaveLength(0);
    expect(writtenDecision(write, "rcv-002")).toMatchObject({
      decision: "Defer",
      issue: { key: "acme/api#12" },
    });

    await userEvent.click(action("rcv-002", "Irrelevant"));
    await waitFor(() => expect(write).toHaveBeenCalledTimes(2));
    expect(requests.states).toEqual([{ state: "closed" }]);
    const next = write.mock.calls[1][1] as string;
    expect(next).toContain(
      "Decision: Irrelevant — user decided this issue is not important. Closed [acme/api#12](https://github.com/acme/api/issues/12).",
    );
  });

  it("Need fix after Irrelevant reopens the same closed issue", async () => {
    const requests = serveTracker({
      issue: makeIssueSummary({
        title: "[Low] Receipt switch skips ownership",
        state: "closed",
        contentTrusted: true,
      }),
    });
    const write = mount();
    await waitFor(() => expect(action("rcv-003", "Need fix")).toBeEnabled());
    await userEvent.click(action("rcv-003", "Need fix"));
    await waitFor(() => expect(write).toHaveBeenCalledTimes(1));
    expect(requests.creates).toHaveLength(0);
    expect(requests.edits[0]).toMatchObject({
      title: "[High] Receipt switch skips ownership",
    });
    expect(requests.states).toEqual([{ state: "open" }]);
    expect(writtenDecision(write, "rcv-003")).toMatchObject({
      decision: "Need fix",
      issue: { key: "acme/api#12" },
    });
  });

  it("Irrelevant without an issue only marks the file", async () => {
    const requests = serveTracker();
    const write = mount();
    await waitFor(() => expect(action("rcv-001", "Irrelevant")).toBeEnabled());
    await userEvent.click(action("rcv-001", "Irrelevant"));
    await waitFor(() => expect(write).toHaveBeenCalledTimes(1));
    // It looks for an issue an earlier press filed, finds none, sends nothing.
    expect(requests.checks).toHaveLength(1);
    expect(requests.creates).toHaveLength(0);
    expect(requests.states).toHaveLength(0);
    expect(write.mock.calls[0][1]).toContain(
      "Severity: Critical · Decision: Irrelevant — user decided this issue is not important.\n",
    );
  });

  it("keeps the issue and offers Retry when the line cannot be written", async () => {
    const requests = serveTracker();
    let fail = true;
    const write = mount({
      write: async () => {
        if (fail) throw new Error("The file changed. Review it and retry.");
      },
    });
    await waitFor(() => expect(action("rcv-001", "Need fix")).toBeEnabled());
    await userEvent.click(action("rcv-001", "Need fix"));
    const note = await within(row("rcv-001")).findByRole("alert");
    expect(note).toHaveTextContent(
      "acme/api#77 is up to date, but the decision could not be written: The file changed.",
    );
    fail = false;
    await userEvent.click(
      within(note).getByRole("button", { name: "Retry recording" }),
    );
    await waitFor(() => expect(write).toHaveBeenCalledTimes(2));
    expect(requests.creates).toHaveLength(1);
    expect(writtenDecision(write, "rcv-001").issue?.key).toBe("acme/api#77");
  });

  it("an untrusted destination reviews the issue in the shared dialog first", async () => {
    const requests = serveTracker({ trusted: false });
    const write = mount();
    await waitFor(() => expect(action("rcv-001", "Need fix")).toBeEnabled());
    await userEvent.click(action("rcv-001", "Need fix"));
    const dialog = await screen.findByRole("dialog");
    expect(
      within(dialog).getByDisplayValue(
        "[High] Duplicate receipt lines bypass the ceiling",
      ),
    ).toBeVisible();
    expect(requests.creates).toHaveLength(0);
    expect(write).not.toHaveBeenCalled();
    // These issues get no assignee and no parent.
    expect(within(dialog).queryByTestId("create-issue-parent")).toBeNull();
    expect(within(dialog).queryByTestId("create-issue-assign-me")).toBeNull();
  });
});

describe("findings decisions over a changing file", () => {
  beforeEach(() => serveExternalIssuesFeature());

  /**
   * A host like FileView: the live file in state, and a write bound to the
   * content it holds, refused when the expected text is not current.
   */
  function mountLive() {
    const control = {
      content: FILE,
      fail: true,
      writes: [] as string[],
      change: (_next: string) => {},
    };
    function Host() {
      const [content, setContent] = useState(FILE);
      useEffect(() => {
        control.content = content;
        control.change = setContent;
      }, [content]);
      return (
        <DocumentPreview
          workspaceId="ws"
          repoId="repo"
          filePath={PATH}
          content={content}
          findings={{
            blockedReason: null,
            write: async (expected, next) => {
              if (control.fail)
                throw new Error("The tracker write landed late.");
              if (expected !== content)
                throw new Error("The file changed or a Save is in progress.");
              control.writes.push(next);
              setContent(next);
            },
          }}
        />
      );
    }
    renderApp(
      <Routes>
        <Route path="/:org/:workspace/file/*" element={<Host />} />
      </Routes>,
      { route: `/g5labs/current/file/${PATH}`, user: createMockUser() },
    );
    return control;
  }

  it("Retry recording writes over the file as it is now", async () => {
    const requests = serveTracker();
    const live = mountLive();
    await waitFor(() => expect(action("rcv-001", "Need fix")).toBeEnabled());
    await userEvent.click(action("rcv-001", "Need fix"));
    const note = await within(row("rcv-001")).findByRole("alert");
    expect(note).toHaveTextContent("could not be written");
    // Meanwhile the file changes: another entry is decided elsewhere.
    act(() =>
      live.change(
        live.content.replace(
          "Severity: Low · Decision: Maybe",
          "Severity: Low · Decision: Irrelevant — user decided this issue is not important.",
        ),
      ),
    );
    live.fail = false;
    await userEvent.click(
      within(row("rcv-001")).getByRole("button", { name: "Retry recording" }),
    );
    await waitFor(() => expect(live.writes).toHaveLength(1));
    const written = parseFindings(live.writes[0]).entries;
    expect(written.find((e) => e.id === "rcv-001")).toMatchObject({
      decision: "Need fix",
      issue: { key: "acme/api#77" },
    });
    expect(written.find((e) => e.id === "rcv-004")?.decision).toBe(
      "Irrelevant",
    );
    expect(requests.creates).toHaveLength(1);
  });

  it("Irrelevant closes an issue an earlier press filed, found by the entry's key", async () => {
    const requests = serveTracker({ found: true });
    const write = mount();
    await waitFor(() => expect(action("rcv-001", "Irrelevant")).toBeEnabled());
    await userEvent.click(action("rcv-001", "Irrelevant"));
    await waitFor(() => expect(write).toHaveBeenCalledTimes(1));
    expect(requests.checks).toHaveLength(1);
    expect(requests.creates).toHaveLength(0);
    expect(requests.states).toEqual([{ state: "closed" }]);
    expect(writtenDecision(write, "rcv-001")).toMatchObject({
      decision: "Irrelevant",
      issue: { key: "acme/api#77" },
    });
  });

  it("an untrusted GitHub issue shows its new title for review before it is sent", async () => {
    const requests = serveTracker({
      issue: makeIssueSummary({
        title: "[High] A stale draft receives against a closed order",
        contentTrusted: false,
      }),
    });
    const write = mount();
    await waitFor(() => expect(action("rcv-002", "Defer")).toBeEnabled());
    await userEvent.click(action("rcv-002", "Defer"));
    const confirm = await within(row("rcv-002")).findByRole("alert");
    expect(confirm).toHaveTextContent(
      "Change the title of acme/api#12 on GitHub to “[Low] A stale draft receives against a closed order”?",
    );
    expect(requests.edits).toHaveLength(0);
    await userEvent.click(
      within(confirm).getByRole("button", { name: "Confirm" }),
    );
    await waitFor(() => expect(write).toHaveBeenCalledTimes(1));
    expect(requests.edits).toEqual([
      {
        title: "[Low] A stale draft receives against a closed order",
        seen: { title: "[High] A stale draft receives against a closed order" },
      },
    ]);
  });
});
