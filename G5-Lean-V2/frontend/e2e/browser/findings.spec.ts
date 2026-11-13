import { expect, test, type Page } from "@playwright/test";
import { createMockUser, MOCK_INSTANCE_INFO } from "../../src/test/fixtures";

// The findings presentation in a real browser (g5.documents.findings): the
// production file view decides entries against a native tracker served here,
// and writes the decision line live, without a commit.
const PATH = ".g5/TLA_verification/bugs_issues.md";
const entry = (id: string, title: string, status: string, body: string) =>
  `<a id="${id}"></a>\n### ${title}\n\n${status}\n\n${body}\n\n`;
const FILE =
  "# Bugs and issues\n\nFound by the verification run of 1 November 2026 and not fixed.\n\n## Actionable\n\n" +
  entry(
    "rcv-001",
    "Duplicate receipt lines bypass the cumulative receiving ceiling",
    "Severity: Critical · Decision: Open",
    "Found by: [asbuilt-dup](results/receipt-submit/20261024T012111Z-asbuilt-dup-a/receipt.json), [intended-safety](results/receipt-submit/20261024T011905Z-intended-safety-b/receipt.json)\n\nReproduced by: [ReceivingTests](../../src/ReceivingTests.cs#L40) failed with two lines for one item.\n\nSubmitting one receipt with two lines for the same purchase-order item lets each line pass the ceiling on its own, so the order is over-received. Proposed repair: aggregate surviving lines per order line inside the submit transaction. Governing: [receipt-quantity-atomicity](../ontology/features/receiving.g5.md#receipt-quantity-atomicity).",
  ) +
  entry(
    "rcv-002",
    "A stale draft can receive against a closed purchase order",
    "Severity: High · Decision: Open",
    "Submit does not recheck the order's state, so a terminal order reopens as received.",
  ) +
  entry(
    "rcv-003",
    "Receipt switch skips the ownership check",
    "Severity: Medium · Decision: Open",
    "Switching the order reuses only the status check.",
  ) +
  entry(
    "rcv-004",
    "Receipt numbers restart after a settings save",
    "Severity: Low · Decision: Open",
    "A settings save restarts the sequence.",
  ) +
  "## Other entries\n\n" +
  entry(
    "rcv-q-001",
    "Whether recall after approval is allowed is undecided",
    "Severity: Unranked",
    "An open intent question, not a defect.",
  );

interface Issue {
  number: number;
  title: string;
  state: "open" | "closed";
  updatedAt: string;
  key: string;
}

async function serve(page: Page, content = FILE) {
  const user = createMockUser({ avatarUrl: null });
  const jwt = `e30.${Buffer.from(JSON.stringify({ sub: user.id, org: user.activeOrgId, scopes: [], exp: 9999999999 })).toString("base64url")}.sig`;
  const state = {
    content,
    hash: 1,
    writes: [] as { content: string; commitAndPush: unknown }[],
    issues: [] as Issue[],
    edits: [] as Record<string, unknown>[],
    states: [] as string[],
    unprovided: [] as string[],
  };
  const summary = (issue: Issue) => ({
    provider: "g5",
    trackerId: "tracker",
    domainId: "domain",
    externalIssueId: String(issue.number),
    locator: { repositoryId: "g5", number: issue.number },
    displayKey: `G5L-${issue.number}`,
    canonicalUrl: `https://g5.example/g5labs/issues/${issue.number}`,
    title: issue.title,
    state: issue.state,
    stateReason: null,
    assignees: [],
    labels: [],
    updatedAt: issue.updatedAt,
    contentTrusted: true,
    primaryWorkspace: null,
  });
  const fileData = () => ({
    path: PATH,
    content: state.content,
    size: state.content.length,
    encoding: "utf-8",
    hash: `h${state.hash}`,
    source: "live",
    repoOid: null,
    snapshotAt: null,
    vmId: "vm-1",
  });
  await page.route("**/api/**", async (route) => {
    const request = route.request();
    const url = new URL(request.url());
    const path = url.pathname;
    const method = request.method();
    const json = (data: unknown, status = 200) =>
      route.fulfill({ status, json: { data } });
    if (path.endsWith("/auth/refresh")) return json({ jwt });
    if (path.endsWith("/users/me")) return json(user);
    if (path.endsWith("/instance/info"))
      return json({ ...MOCK_INSTANCE_INFO, features: ["external_issues"] });
    if (path === "/api/v1/orgs/org-g5labs/config")
      return json({
        scope: "org",
        settings: { external_issues_enabled: true },
        secretKeys: [],
        effectiveSettings: {},
        effectiveSecretKeys: [],
      });
    if (
      path ===
      "/api/v1/workspaces/ws/files/.g5/TLA_verification/results/index.jsonl"
    ) {
      const content = [
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
      ]
        .map((row) => JSON.stringify(row))
        .join("\n");
      return json({
        path: "index.jsonl",
        content,
        size: content.length,
        encoding: "utf-8",
        hash: "i",
        source: "live",
        repoOid: null,
        snapshotAt: null,
      });
    }
    if (path === `/api/v1/workspaces/ws/files/${PATH}`) {
      if (method === "GET") return json(fileData());
      const body = request.postDataJSON();
      if (body.expectedHash !== `h${state.hash}`)
        return route.fulfill({
          status: 409,
          json: { error: { code: "CONFLICT", message: "The file changed." } },
        });
      state.content = body.content;
      state.hash += 1;
      state.writes.push({
        content: body.content,
        commitAndPush: body.commitAndPush,
      });
      return json({
        path: PATH,
        hash: `h${state.hash}`,
        size: body.content.length,
        encoding: "utf-8",
        vmId: "vm-1",
      });
    }
    if (path === "/api/v1/orgs/org-g5labs/issues/create-target")
      return json({
        state: "ready",
        destination: {
          provider: "g5",
          domainId: "domain",
          trackerId: "tracker",
          repositoryId: "g5",
          displayKey: "G5L",
          contentTrusted: true,
        },
        githubLogin: null,
        assignableUsers: [],
        error: null,
      });
    if (path === "/api/v1/orgs/org-g5labs/issues" && method === "POST") {
      const body = request.postDataJSON();
      let issue = state.issues.find((i) => i.key === body.idempotencyKey);
      if (!issue && !body.checkOnly) {
        issue = {
          number: state.issues.length + 1,
          title: body.title,
          state: "open",
          updatedAt: `v${Date.now()}`,
          key: body.idempotencyKey,
        };
        state.issues.push(issue);
      }
      return json(
        {
          state: issue ? "created" : "not_found",
          sent: !body.checkOnly,
          issue: issue && {
            externalIssueId: String(issue.number),
            repositoryId: "g5",
            number: issue.number,
            displayKey: `G5L-${issue.number}`,
            title: issue.title,
            url: `https://g5.example/g5labs/issues/${issue.number}`,
          },
          idempotencyKey: body.idempotencyKey,
          destination: {},
          githubLogin: null,
          title: body.title,
          payload: body.body,
          assignees: [],
          message: null,
          parent: null,
        },
        body.checkOnly ? 200 : 201,
      );
    }
    if (path === "/api/v1/orgs/org-g5labs/issues/resolve") {
      const number = Number(url.searchParams.get("ref")!.split("-")[1]);
      return json(summary(state.issues[number - 1]));
    }
    const native = path.match(
      /^\/api\/v1\/orgs\/org-g5labs\/issues\/g5\/(\d+)(\/state)?$/,
    );
    if (native) {
      const issue = state.issues[Number(native[1]) - 1];
      const body = request.postDataJSON();
      if (native[2]) {
        state.states.push(body.state);
        issue.state = body.state;
        issue.updatedAt = `v${Date.now()}s`;
        return json({
          requested: body.state,
          outcome: "applied",
          issue: summary(issue),
          githubLogin: null,
          message: null,
        });
      }
      state.edits.push(body);
      if (body.expectedUpdatedAt !== issue.updatedAt)
        return route.fulfill({
          status: 409,
          json: { error: { code: "CONFLICT", message: "issue_changed" } },
        });
      issue.title = body.title;
      issue.updatedAt = `v${Date.now()}e`;
      return json(summary(issue));
    }
    state.unprovided.push(`${method} ${path}`);
    return route.fulfill({
      status: 404,
      json: { error: { code: "NOT_FOUND", message: `Unprovided: ${path}` } },
    });
  });
  return state;
}

const row = (page: Page, id: string) =>
  page.locator(`[data-testid='finding-row'][data-entry-id='${id}']`);

test("findings show severity, filter, and decide one issue end to end", async ({
  page,
}) => {
  const state = await serve(page);
  await page.goto("/e2e/browser/findings.html");
  const first = row(page, "rcv-001");
  await expect(first.getByTestId("finding-severity")).toHaveText("Critical");
  await expect(first.getByTestId("finding-severity")).toHaveCSS(
    "background-color",
    "rgb(127, 29, 29)",
  );
  await expect(row(page, "rcv-002").getByTestId("finding-severity")).toHaveCSS(
    "background-color",
    "rgb(220, 38, 38)",
  );
  // The row sits on one line under its heading at desktop width.
  const box = (await first.boundingBox())!;
  expect(box.height).toBeLessThan(40);
  await expect(
    row(page, "rcv-q-001").getByTestId("finding-action"),
  ).toHaveCount(0);
  await page.screenshot({
    path: "/tmp/findings-shots/initial.png",
    fullPage: true,
  });

  // The filter stays in reach while the reader scrolls a long file.
  await row(page, "rcv-q-001").scrollIntoViewIfNeeded();
  await expect(page.getByTestId("findings-filter")).toBeInViewport();

  // Each entry is a card edged in its severity's colour.
  await expect(
    first.locator("xpath=ancestor::*[contains(@class,'finding-entry')][1]"),
  ).toHaveCSS("border-left-color", "rgb(185, 28, 28)");
  await expect(first.getByTestId("finding-decision")).toHaveCSS(
    "color",
    /rgb\(\d+, \d+, \d+\)/,
  );

  // Evidence: the runs that found it, as recorded, and what reproduced it.
  const card = first.locator(
    "xpath=ancestor::*[contains(@class,'finding-entry')][1]",
  );
  const found = card.locator(
    "[data-testid='finding-evidence'][data-kind='found-by']",
  );
  await found.getByText("Found by").click();
  await expect(found.getByTestId("verification-run-outcome")).toHaveText([
    "EXPECTED_VIOLATION",
    "PASS",
  ]);
  await expect(found).toContainText("280,850");
  await expect(
    card.locator("[data-testid='finding-evidence'][data-kind='reproduced-by']"),
  ).toContainText("failed with two lines for one item.");
  // Without a reproduction, the heading alone and in bold.
  const unreproduced = row(page, "rcv-002")
    .locator("xpath=ancestor::*[contains(@class,'finding-entry')][1]")
    .locator("[data-testid='finding-evidence'][data-kind='reproduced-by']");
  await expect(unreproduced).toHaveAttribute("data-empty", "true");
  await expect(unreproduced).not.toContainText("No reproduction evidence");
  await expect(
    unreproduced.getByText("A test or probe that made it happen in the code"),
  ).toHaveCSS("font-weight", "600");
  await card.screenshot({ path: "/tmp/findings-shots/browser-evidence.png" });

  // Filter: hiding Medium hides its entry and, while filtering, the
  // description around the entries; the headings introducing them stay.
  const preview = page.getByTestId("markdown-preview");
  const intro = preview.getByText(
    "Found by the verification run of 1 November 2026 and not fixed.",
  );
  await expect(intro).toBeVisible();
  await page
    .locator("[data-testid='finding-filter'][data-severity='Medium']")
    .click();
  await expect(
    preview.getByRole("heading", {
      name: "Receipt switch skips the ownership check",
    }),
  ).toBeHidden();
  await expect(intro).toBeHidden();
  await expect(
    preview.getByRole("heading", { name: "Actionable" }),
  ).toBeVisible();
  await expect(page.getByTestId("findings-shown")).toContainText("Showing");
  await page.getByTestId("finding-filter-reset").click();
  await expect(
    preview.getByRole("heading", {
      name: "Receipt switch skips the ownership check",
    }),
  ).toBeVisible();
  await expect(intro).toBeVisible();

  // Need fix files one [High] issue and writes the line live.
  await first.getByRole("button", { name: "Need fix" }).click();
  await expect(first.getByRole("link", { name: "G5L-1" })).toBeVisible();
  expect(state.issues).toHaveLength(1);
  expect(state.issues[0].title).toBe(
    "[High] Duplicate receipt lines bypass the cumulative receiving ceiling",
  );
  expect(state.writes.at(-1)!.commitAndPush).toBe(false);
  expect(state.content).toContain(
    "Severity: Critical · Decision: Need fix — [G5L-1](https://g5.example/g5labs/issues/1)",
  );

  // Defer retitles it; Irrelevant closes it; Need fix reopens it.
  await first.getByRole("button", { name: "Defer" }).click();
  await expect(first.getByRole("button", { name: "Defer" })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  expect(state.issues[0].title).toMatch(/^\[Low\] /);
  await first.getByRole("button", { name: "Irrelevant" }).click();
  await expect(first).toContainText("G5L-1 (closed)");
  expect(state.issues[0].state).toBe("closed");
  await first.getByRole("button", { name: "Need fix" }).click();
  await expect(first.getByRole("button", { name: "Need fix" })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  expect(state.issues[0]).toMatchObject({ state: "open" });
  expect(state.issues[0].title).toMatch(/^\[High\] /);
  expect(state.issues).toHaveLength(1);
  expect(state.writes.every((w) => w.commitAndPush === false)).toBe(true);

  // Irrelevant on an untouched entry creates nothing.
  const fourth = row(page, "rcv-004");
  await fourth.getByRole("button", { name: "Irrelevant" }).click();
  await expect(
    fourth.getByRole("button", { name: "Irrelevant" }),
  ).toHaveAttribute("aria-pressed", "true");
  expect(state.issues).toHaveLength(1);
  expect(state.content).toContain(
    "Severity: Low · Decision: Irrelevant — user decided this issue is not important.",
  );

  // Chat seeds the composer with the entry.
  await row(page, "rcv-002").getByTestId("finding-chat").click();
  expect(
    await page.evaluate(
      () =>
        (window as unknown as { findingsTest: { seeded: string[] } })
          .findingsTest.seeded,
    ),
  ).toEqual([
    'Let\'s discuss the bug "A stale draft can receive against a closed purchase order" (rcv-002, severity High, decision Open) in .g5/TLA_verification/bugs_issues.md.',
  ]);
  await page.screenshot({
    path: "/tmp/findings-shots/decided.png",
    fullPage: true,
  });
  expect(
    state.unprovided.filter((p) => !p.includes("/ws/") || p.includes("files")),
  ).toEqual([]);
});

// Each entry names the run that first recorded it (g5.documents.findings.files);
// the reader filters by run together with severity, and shows no run control
// for a file whose entries name none.
const FILE_WITH_RUNS =
  "# Bugs and issues\n\n## Actionable\n\n" +
  entry(
    "run-001",
    "Uplift finding one",
    "Severity: Critical · Decision: Open\n\nRun: Uplift 2026-10-24",
    "Found by: source review",
  ) +
  entry(
    "run-002",
    "Uplift finding two",
    "Severity: High · Decision: Open\n\nRun: Uplift 2026-10-24",
    "Found by: source review",
  ) +
  entry(
    "run-003",
    "Base re-verification finding",
    "Severity: High · Decision: Open\n\nRun: Base re-verification 2026-11-06 (incremental)",
    "Found by: source review",
  ) +
  entry(
    "run-004",
    "An entry from before runs were named",
    "Severity: Low · Decision: Open",
    "Found by: source review",
  );

test("findings filter by the run that recorded them, with severity", async ({
  page,
}) => {
  await serve(page, FILE_WITH_RUNS);
  await page.goto("/e2e/browser/findings.html");
  const runs = page.getByTestId("findings-run-filter");
  await expect(runs).toBeVisible();
  await expect(runs.locator("option")).toHaveText([
    "All runs",
    "Uplift 2026-10-24",
    "Base re-verification 2026-11-06 (incremental)",
    "No run named",
  ]);
  await expect(page.getByTestId("findings-shown")).toHaveText("4 entries");

  await runs.selectOption({
    label: "Base re-verification 2026-11-06 (incremental)",
  });
  await expect(row(page, "run-003")).toBeVisible();
  for (const id of ["run-001", "run-002", "run-004"]) {
    await expect(row(page, id)).toBeHidden();
  }
  await expect(page.getByTestId("findings-shown")).toHaveText(
    "Showing 1 of 4 entries",
  );

  // Both filters apply: the uplift's run with High hidden leaves Critical.
  await runs.selectOption({ label: "Uplift 2026-10-24" });
  await page
    .locator("[data-testid='finding-filter'][data-severity='High']")
    .click();
  await expect(row(page, "run-001")).toBeVisible();
  await expect(row(page, "run-002")).toBeHidden();
  await expect(page.getByTestId("findings-shown")).toHaveText(
    "Showing 1 of 4 entries",
  );

  await runs.selectOption({ label: "No run named" });
  await expect(row(page, "run-004")).toBeVisible();
  await expect(row(page, "run-001")).toBeHidden();

  // Show all clears both filters.
  await page.getByTestId("finding-filter-reset").click();
  await expect(runs).toHaveValue("all");
  await expect(page.getByTestId("findings-shown")).toHaveText("4 entries");
  for (const id of ["run-001", "run-002", "run-003", "run-004"]) {
    await expect(row(page, id)).toBeVisible();
  }
});

test("a findings file whose entries name no run shows no run control", async ({
  page,
}) => {
  await serve(page);
  await page.goto("/e2e/browser/findings.html");
  await expect(row(page, "rcv-001")).toBeVisible();
  await expect(page.getByTestId("findings-run-filter")).toHaveCount(0);
});
