// Run by the ignored native API fixture. Every successful request reaches the
// production API and PostgreSQL; only the deliberate lost ACK is intercepted.
import { chromium, expect } from "@playwright/test";
let input = "";
for await (const chunk of process.stdin) input += chunk;
const f = JSON.parse(input);
const root = `http://127.0.0.1:5174/${f.org}/${f.workspace}?view=approvals`;
const browser = await chromium.launch({ headless: true });
async function dismissGuidedIntro(page) {
  await page.getByTestId("action-ribbon").waitFor({ state: "visible" });
  await page.keyboard.press("Escape");
}
async function actor(token, submission) {
  const context = await browser.newContext({
    viewport: { width: 1100, height: 850 },
  });
  if (process.argv[2] === "inventory")
    await context.addCookies([
      {
        name: "g5_refresh",
        value: f.ownerRefreshToken,
        domain: "127.0.0.1",
        path: "/api/v1/auth",
        httpOnly: true,
        sameSite: "Lax",
      },
    ]);
  await context.addInitScript(
    ({ token, submission, workspaceId }) => {
      localStorage.setItem("g5_auth_token", token);
      if (submission) {
        const encoded = token
          .split(".")[1]
          .replace(/-/g, "+")
          .replace(/_/g, "/");
        const payload = JSON.parse(
          atob(encoded.padEnd(Math.ceil(encoded.length / 4) * 4, "=")),
        );
        sessionStorage.setItem(
          `g5:approval-submission:${workspaceId}:${payload.sub}`,
          JSON.stringify(submission),
        );
      }
    },
    {
      token,
      submission,
      workspaceId: f.workspaceId,
    },
  );
  const page = await context.newPage();
  page.on("pageerror", (error) => process.stderr.write(`${error.stack}\n`));
  await page.goto(root);
  await dismissGuidedIntro(page);
  await expect(page.getByTestId("action-ribbon")).toHaveCount(1);
  await expect(
    page.getByRole("heading", { name: /^Approvals Required to merge into / }),
  ).toBeVisible();
  await expect(page.getByTestId("workspace-commit")).toHaveCount(0);
  const headings = [
    page.locator("#approvals-required-heading"),
    page
      .getByTestId("workspace-approvals-reviews")
      .getByRole("heading", { name: /^Reviews/ }),
    page.locator("#approvals-configure-heading"),
  ];
  const headingSize = await headings[0].evaluate(
    (node) => getComputedStyle(node).fontSize,
  );
  for (const heading of headings)
    await expect(heading).toHaveCSS("font-size", headingSize);
  await expect(headings[2]).toHaveText(
    "Configure Approvals Required to merge into this workspace",
  );
  await expect(
    page.getByRole("heading", { name: "Approval history", exact: true }),
  ).toHaveCount(0);
  await expect(
    page.getByTestId("workspace-approvals-advanced-toggle"),
  ).toHaveCount(0);
  if (
    process.env.G5_NATIVE_APPROVAL_SCREENSHOT &&
    token === f.ownerToken &&
    process.argv[2] === "request"
  )
    await page.screenshot({
      path: process.env.G5_NATIVE_APPROVAL_SCREENSHOT,
      fullPage: true,
    });
  return page;
}
// The one Request approvals dialog (g5.governance.approval.request-composer).
// Its accessible name follows its step, so address it by test id.
function requestDialog(page) {
  return page.getByTestId("approval-request-dialog");
}
async function openRequest(page) {
  await page.getByTestId("workspace-request-review").click();
  const dialog = requestDialog(page);
  await expect(dialog).toBeVisible();
  const fresh = dialog.getByTestId("approval-request-new-selection");
  if (await fresh.count()) await fresh.click();
  return dialog;
}
// Request mode is a themed listbox: open the ▾ trigger, then pick the mode.
async function chooseMode(dialog, mode) {
  await dialog.getByTestId("approval-request-mode").click();
  await dialog.getByTestId(`approval-request-mode-option-${mode}`).click();
}
async function selectedPairs(dialog) {
  return (
    await dialog.getByTestId("approval-request-pair").allTextContents()
  ).map((text) => text.replace(/\s+/g, " ").trim());
}
async function pick(page, recipient) {
  const dialog = await openRequest(page);
  await chooseMode(dialog, "pick");
  await dialog
    .getByRole("group", { name: "Mixed approval", exact: true })
    .getByRole("checkbox", { name: recipient, exact: true })
    .check();
  await dialog.getByTestId("approval-request-preview").click();
  await expect(
    dialog.getByRole("heading", { name: "Review request", exact: true }),
  ).toBeVisible();
  return dialog;
}
// Full success closes the dialog after the roster refresh; no Accepted
// receipt reads as a decision.
async function submit(dialog) {
  await dialog.getByTestId("review-request-submit").click();
  await expect(dialog).toHaveCount(0);
}
async function checkBulkRequests(page) {
  const api = async (path, body) =>
    page.evaluate(
      async ({ path, body, token }) => {
        const response = await fetch(`/api/v1${path}`, {
          method: body === undefined ? "GET" : "POST",
          headers: {
            "Content-Type": "application/json",
            Authorization: `Bearer ${token}`,
          },
          ...(body === undefined ? {} : { body: JSON.stringify(body) }),
        });
        const result = await response.json();
        if (!response.ok)
          throw new Error(
            `${path}: ${response.status} ${JSON.stringify(result)}`,
          );
        return result.data;
      },
      { path, body, token: f.ownerToken },
    );
  const prior = await api(`/workspaces/${f.workspaceId}/approvals`);
  const parent = prior.outgoing.targetWorkspaceId;
  const mixed = prior.outgoing.requirements.find(
    (row) => row.definition.id === f.definitionId,
  );
  const humanIds = mixed.definition.humans.map((h) => h.userId);
  expect(humanIds).toHaveLength(2);
  const orgId = (await api(`/workspaces/${f.workspaceId}`)).orgId;
  // The main journey is complete; replace only this disposable target's
  // requirements so the bulk roster has one single, one Any-of and one All-of.
  for (const row of prior.outgoing.requirements)
    await api(
      `/workspaces/${parent}/approvals/requirements/${row.definition.id}/retire`,
      {},
    );
  const definitions = [];
  for (const [name, humanUserIds, requireAllReviewers] of [
    ["Native Bulk Single", [f.reviewerId], false],
    ["Native Bulk Any", humanIds, false],
    ["Native Bulk All", humanIds, true],
  ]) {
    const definition = await api(`/orgs/${orgId}/approval-definitions`, {
      name,
      humanUserIds,
      requireAllReviewers,
    });
    await api(`/workspaces/${parent}/approvals/requirements`, {
      definitionId: definition.id,
    });
    definitions.push(definition);
  }
  const [single, any, all] = definitions;
  await page.goto(root);
  await dismissGuidedIntro(page);
  let dialog = await openRequest(page);
  await expect(dialog.getByTestId("approval-request-mode")).toHaveText(
    /^Select▾$/,
  );
  await expect(dialog.getByTestId("approval-request-preview")).toBeDisabled();
  await chooseMode(dialog, "all");
  // Request all: the single human, one random human for Any-of and every
  // All-of human (g5.governance.approval.selection).
  const pairs = await selectedPairs(dialog);
  const anyPair = pairs.filter((pair) => pair.startsWith(`${any.name} → `));
  expect(anyPair).toHaveLength(1);
  expect([
    `${any.name} → Ben reviewer`,
    `${any.name} → Rahul reviewer`,
  ]).toContain(anyPair[0]);
  expect([...pairs].sort()).toEqual(
    [
      `${all.name} → Ben reviewer`,
      `${all.name} → Rahul reviewer`,
      anyPair[0],
      `${single.name} → Ben reviewer`,
    ].sort(),
  );
  await dialog.getByTestId("approval-request-preview").click();
  await submit(dialog);
  dialog = await openRequest(page);
  await expect(dialog.getByTestId("approval-request-recipient")).toHaveCount(0);
  for (const mode of ["all", "agents"]) {
    await chooseMode(dialog, mode);
    await expect(dialog.getByTestId("approval-request-pair")).toHaveCount(0);
    await expect(
      dialog.getByText("No new approvals to request with this selection."),
    ).toBeVisible();
    await expect(dialog.getByTestId("approval-request-preview")).toBeDisabled();
  }
  await dialog.getByTestId("approval-request-close").click();
  await expect(dialog).toHaveCount(0);
  const reviewer = await actor(f.reviewerToken);
  await reviewer
    .getByTestId("workspace-approvals-requirement")
    .filter({ hasText: single.name })
    .getByRole("button", { name: "Approve", exact: true })
    .click();
  const decision = reviewer.getByRole("dialog", {
    name: "Submit review",
    exact: true,
  });
  await decision.getByLabel("Review comment").fill("Native bulk approval");
  await decision
    .getByRole("button", { name: "Submit review", exact: true })
    .click();
  await expect(decision).toHaveCount(0);
  await page.reload();
  await dismissGuidedIntro(page);
  dialog = await openRequest(page);
  await chooseMode(dialog, "all");
  await expect(dialog.getByTestId("approval-request-pair")).toHaveCount(0);
  await dialog
    .getByRole("checkbox", {
      name: "Include satisfied approval requirements",
      exact: true,
    })
    .check();
  expect(await selectedPairs(dialog)).toEqual([
    `${single.name} → Ben reviewer`,
  ]);
  await dialog.getByTestId("approval-request-preview").click();
  await submit(dialog);
  await api(
    `/workspaces/${parent}/approvals/requirements/${single.id}/retire`,
    {},
  );
  await page.reload();
  await dismissGuidedIntro(page);
  const legacy = page.getByTestId("workspace-approvals-legacy");
  await expect(legacy).toBeVisible();
  await expect(legacy).not.toHaveAttribute("open");
  await legacy.getByTestId("workspace-approvals-legacy-toggle").click();
  await legacy
    .getByRole("button", { name: `Timeline for ${single.name}`, exact: true })
    .click();
  const timeline = legacy.getByRole("region", {
    name: `Timeline for ${single.name}`,
    exact: true,
  });
  await expect(timeline).toContainText("Native bulk approval");
  await expect(
    timeline.getByTestId("workspace-approvals-history-card"),
  ).toHaveCount(2);
  const latest = await timeline
    .getByTestId("workspace-approvals-history-card")
    .evaluateAll((cards) =>
      cards.map((card) =>
        Math.max(
          ...Array.from(card.querySelectorAll("time"), (node) =>
            Date.parse(node.getAttribute("datetime")),
          ),
        ),
      ),
    );
  expect(latest).toEqual([...latest].sort((a, b) => b - a));
  await page.reload();
  await dismissGuidedIntro(page);
  await expect(legacy).toBeVisible();
  await expect(legacy).not.toHaveAttribute("open");
}

try {
  if (process.argv[2] === "request") {
    const owner = await actor(f.ownerToken);
    await expect(owner.getByTestId("approvals-reviews-toggle")).toHaveAttribute(
      "aria-expanded",
      "false",
    );
    // Mixed Any-of uses the same chooser for humans and agents. Previewing
    // agents-only must select exactly the agent without admitting paid work.
    const selection = await openRequest(owner);
    await chooseMode(selection, "agents");
    expect(await selectedPairs(selection)).toEqual([
      "Mixed approval → Mixed approval agent",
    ]);
    await selection.getByTestId("approval-request-close").click();
    await expect(selection).toHaveCount(0);
    let dialog = await pick(owner, "Ben reviewer");
    await submit(dialog);
    const reviewer = await actor(f.reviewerToken);
    const requirement = reviewer
      .getByTestId("workspace-approvals-requirement")
      .filter({ hasText: "Mixed approval" });
    await expect(reviewer.getByTestId("workspace-request-review")).toHaveCount(
      0,
    );
    await requirement
      .getByRole("button", { name: "Approve", exact: true })
      .click();
    let decision = reviewer.getByRole("dialog", { name: "Submit review" });
    await decision.getByLabel("Review comment").fill("Native browser approval");
    await decision
      .getByRole("button", { name: "Submit review", exact: true })
      .click();
    await expect(decision).toHaveCount(0);
    await owner.reload();
    await dismissGuidedIntro(owner);
    dialog = await pick(owner, "Ben reviewer");
    await submit(dialog);
    const compact = owner
      .getByTestId("workspace-approvals-requirement")
      .filter({ hasText: "Mixed approval" });
    // A newer Waiting request keeps the earlier approval counting and shown
    // beneath it (g5.governance.approval.request-status).
    await expect(
      compact.getByTestId("workspace-approvals-requirement-status"),
    ).toHaveText("Approved");
    await expect(compact.getByText("Waiting", { exact: true })).toBeVisible();
    await expect(
      compact.getByTestId("workspace-approvals-prior"),
    ).toContainText("Approved");
    await reviewer.reload();
    await dismissGuidedIntro(reviewer);
    await requirement
      .getByRole("button", { name: "Reject", exact: true })
      .click();
    decision = reviewer.getByRole("dialog", { name: "Submit review" });
    await decision
      .getByLabel("Review comment")
      .fill("Native browser requests a correction");
    await reviewer.keyboard.press("Escape");
    await reviewer.reload();
    await dismissGuidedIntro(reviewer);
    await requirement
      .getByRole("button", { name: "Approve", exact: true })
      .click();
    decision = reviewer.getByRole("dialog", { name: "Submit review" });
    await expect(
      decision.getByRole("radio", { name: "Request changes", exact: true }),
    ).toBeChecked();
    await expect(decision.getByLabel("Review comment")).toHaveValue(
      "Native browser requests a correction",
    );
    await decision
      .getByRole("button", { name: "Submit review", exact: true })
      .click();
    await expect(decision).toHaveCount(0);
    await owner.reload();
    await dismissGuidedIntro(owner);
    let reviewId;
    await owner.route(
      `**/api/v1/workspaces/${f.workspaceId}/approvals/reviews`,
      async (route) => {
        const response = await route.fetch();
        expect(response.status()).toBe(201);
        reviewId = (await response.json()).data.reviewId;
        await route.abort("failed");
      },
      { times: 1 },
    );
    dialog = await pick(owner, "Mixed approval agent");
    await dialog.getByTestId("review-request-submit").click();
    await expect(dialog.getByText(/Failed or unconfirmed/)).toBeVisible();
    expect(reviewId).toBeTruthy();
    const submission = await owner.evaluate((workspaceId) => {
      const key = Object.keys(sessionStorage).find((key) =>
        key.startsWith(`g5:approval-submission:${workspaceId}:`),
      );
      return JSON.parse(sessionStorage.getItem(key));
    }, f.workspaceId);
    process.stdout.write(JSON.stringify({ reviewId, submission }));
  } else if (process.argv[2] === "inventory") {
    const owner = await actor(f.ownerToken);
    const privateReads = [];
    owner.on("request", (request) => {
      if (request.url().includes(`/api/v1/workspaces/${f.childId}/`))
        privateReads.push(request.url());
    });
    await owner.goto(`http://127.0.0.1:5174/${f.org}`);
    const workspace = owner.getByTestId(`home-workspace-${f.childId}`);
    const compute = owner.getByTestId(`home-compute-${f.childId}`);
    await expect(workspace).toBeVisible();
    await expect(
      workspace.getByText("Agent review", { exact: true }),
    ).toBeVisible();
    await expect(workspace.getByTestId("home-workspace-open")).toHaveAttribute(
      "href",
      `/${f.org}/${f.workspace}?view=approvals`,
    );
    await expect(workspace.getByTestId("home-workspace-fork")).toHaveCount(0);
    await expect(workspace.getByTestId("home-workspace-remove")).toHaveCount(0);
    await expect(workspace.getByTestId("home-workspace-rename")).toHaveCount(0);
    await expect(compute).toBeVisible();
    await expect(
      compute.getByText("Agent review", { exact: true }),
    ).toBeVisible();
    await expect(compute.getByTestId("home-compute-shutdown")).toHaveCount(0);
    await expect(compute.getByTestId("home-compute-open")).toHaveAttribute(
      "href",
      `/${f.org}/${f.workspace}?view=approvals`,
    );
    expect(privateReads).toEqual([]);
    await compute.getByTestId("home-compute-open").click();
    await expect(
      owner.getByRole("heading", {
        name: /^Approvals Required to merge into /,
      }),
    ).toBeVisible();
    await owner.goto("http://127.0.0.1:5174/projects");
    const projectName = owner.getByText(f.childName, { exact: true });
    const projectRow = owner.getByRole("row").filter({ hasText: f.childName });
    await expect(
      projectRow.getByText("Agent review", { exact: true }),
    ).toBeVisible();
    for (const name of [
      "Abort workspace tasks",
      "Fork workspace",
      "More actions",
    ])
      await expect(
        projectRow.getByRole("button", { name, exact: true }),
      ).toHaveCount(0);
    await projectName.click();
    await expect(owner).toHaveURL(
      new RegExp(`/${f.workspace}\\?view=approvals`),
    );
    await expect(
      owner.getByRole("heading", {
        name: /^Approvals Required to merge into /,
      }),
    ).toBeVisible();
    expect(privateReads).toEqual([]);
    process.stdout.write(JSON.stringify({ inventory: true }));
  } else {
    const owner = await actor(f.ownerToken, f.submission);
    // A page-level open resumes the unfinished batch at its retry step.
    await owner.getByTestId("workspace-request-review").click();
    const dialog = requestDialog(owner);
    await expect(dialog.getByTestId("review-request-submit")).toHaveText(
      "Retry failed requests",
    );
    const response = owner.waitForResponse(
      (r) =>
        r.request().method() === "POST" &&
        r.url().endsWith("/approvals/reviews"),
    );
    await dialog.getByTestId("review-request-submit").click();
    const receipt = await response;
    expect(receipt.status()).toBe(200);
    expect((await receipt.json()).data.reviewId).toBe(f.reviewId);
    // Success closes the dialog: no Accepted receipt, no disabled retry.
    await expect(dialog).toHaveCount(0);
    const compact = owner
      .getByTestId("workspace-approvals-requirement")
      .filter({ hasText: "Mixed approval" });
    await expect(
      compact.getByRole("button", { name: "Reviewed commits: matching" }),
    ).toHaveCount(2);
    const pagesBefore = owner.context().pages().length;
    const readReport = compact.getByRole("button", {
      name: "Read report",
      exact: true,
    });
    await readReport.click();
    const reportPage = owner.getByRole("dialog", {
      name: "Agent review report",
    });
    await expect(reportPage).toBeVisible();
    expect(owner.context().pages()).toHaveLength(pagesBefore);
    const viewer = reportPage.getByTestId("agent-review-report-view");
    await expect(
      viewer.getByRole("heading", { name: "Review findings" }),
    ).toBeVisible();
    await expect(reportPage.getByRole("textbox")).toHaveCount(0);
    await expect(
      reportPage.getByText("Browse reviewed commits", { exact: true }),
    ).toHaveCount(0);
    await reportPage.getByRole("button", { name: "Raw", exact: true }).click();
    const raw = await reportPage
      .getByTestId("approval-report-source")
      .textContent();
    expect(raw).toContain("repoName:");
    expect(raw).toContain("repoId:");
    expect(raw).toContain("workspaceBase:");
    const retained = await owner.evaluate(
      async ({ workspaceId, reviewId }) => {
        const response = await fetch(
          `/api/v1/workspaces/${workspaceId}/approvals/reviews/${reviewId}/report`,
          {
            headers: {
              Authorization: `Bearer ${localStorage.getItem("g5_auth_token")}`,
            },
          },
        );
        return (await response.json()).data.document;
      },
      { workspaceId: f.workspaceId, reviewId: f.reviewId },
    );
    expect(raw).toBe(retained);
    await reportPage
      .getByRole("button", { name: "Close", exact: true })
      .click();
    await expect(readReport).toBeFocused();
    const direct = await owner.context().newPage();
    await direct.goto(
      `${new URL(owner.url()).origin}/${f.org}/review-reports/report/${f.workspaceId}/${f.reviewId}`,
    );
    await expect(
      direct.getByRole("heading", { name: "Review findings" }),
    ).toBeVisible();
    await direct.reload();
    await expect(
      direct.getByRole("heading", { name: "Review findings" }),
    ).toBeVisible();
    await direct.close();
    // A completed retained submission must not reopen as an old receipt list.
    await owner.getByTestId("workspace-request-review").click();
    await expect(
      dialog.getByRole("heading", { name: "Request approvals", exact: true }),
    ).toBeVisible();
    await expect(dialog.getByTestId("approval-request-recipient")).toHaveCount(
      0,
    );
    await expect(dialog.getByTestId("approval-request-mode")).toHaveText(
      /^Select▾$/,
    );
    await dialog.getByTestId("approval-request-close").click();
    await expect(dialog).toHaveCount(0);
    const info = compact.getByRole("button", {
      name: "Timeline for Mixed approval",
      exact: true,
    });
    await expect(info).toHaveAttribute("aria-expanded", "false");
    await info.focus();
    await owner.keyboard.press("Enter");
    const approvalTimeline = compact.getByRole("region", {
      name: "Timeline for Mixed approval",
      exact: true,
    });
    await expect(approvalTimeline).toBeVisible();
    await expect(
      approvalTimeline.getByTestId("workspace-approvals-history-card"),
    ).toHaveCount(3);
    await expect(
      approvalTimeline.getByText("Review requested", { exact: true }),
    ).toHaveCount(3);
    await expect(
      approvalTimeline.getByText("Decision finalized", { exact: true }),
    ).toHaveCount(3);
    for (const reviewer of await approvalTimeline
      .getByTestId("approval-reviewer-timeline")
      .all()) {
      const times = await reviewer
        .locator("time")
        .evaluateAll((nodes) =>
          nodes.map((node) => Date.parse(node.getAttribute("datetime"))),
        );
      // Each request has a newest-first event list; verify each independently.
      for (const card of await reviewer
        .getByTestId("workspace-approvals-history-card")
        .all()) {
        const events = await card
          .getByRole("list", { name: "Request timeline" })
          .locator("time")
          .evaluateAll((nodes) =>
            nodes.map((node) => Date.parse(node.getAttribute("datetime"))),
          );
        expect(events).toEqual([...events].sort((a, b) => b - a));
      }
      const latestPerRequest = await reviewer
        .getByTestId("workspace-approvals-history-card")
        .evaluateAll((cards) =>
          cards.map((card) =>
            Math.max(
              ...Array.from(card.querySelectorAll("time"), (node) =>
                Date.parse(node.getAttribute("datetime")),
              ),
            ),
          ),
        );
      expect(latestPerRequest).toEqual(
        [...latestPerRequest].sort((a, b) => b - a),
      );
      expect(times.length).toBeGreaterThan(0);
    }
    await expect(approvalTimeline).toContainText("Native browser approval");
    await expect(approvalTimeline).toContainText(
      "Native browser requests a correction",
    );
    await info.focus();
    await owner.keyboard.press("Space");
    await expect(approvalTimeline).toHaveCount(0);
    const visitor = await actor(f.visitorToken);
    await expect(visitor.getByTestId("workspace-request-review")).toHaveCount(
      0,
    );
    await expect(visitor.getByTestId("workspace-submit-review")).toHaveCount(0);
    await visitor.setViewportSize({ width: 1024, height: 720 });
    const help = visitor.getByLabel("About reviews and comments");
    await expect(
      visitor.getByText("Published reviews", { exact: true }),
    ).not.toBeVisible();
    await help.focus();
    await visitor.keyboard.press("Enter");
    await expect(
      visitor.getByText("Published reviews", { exact: true }),
    ).toBeVisible();
    await expect(
      visitor.getByText("Workspace comments", { exact: true }).first(),
    ).toBeVisible();
    await visitor.keyboard.press("Escape");
    await expect(
      visitor.getByText("Published reviews", { exact: true }),
    ).not.toBeVisible();
    await expect(
      visitor.getByTestId("approvals-reviews-toggle"),
    ).toHaveAttribute("aria-expanded", "false");
    await expect(
      visitor.getByRole("button", {
        name: "Add workspace comment",
        exact: true,
      }),
    ).toBeInViewport();
    const commentAction = visitor.getByRole("button", {
      name: "Add workspace comment",
      exact: true,
    });
    await commentAction.focus();
    await visitor.keyboard.press("Enter");
    const feedback = visitor.getByRole("dialog", {
      name: "Add workspace comment",
      exact: true,
    });
    await expect(
      visitor.getByTestId("approvals-reviews-toggle"),
    ).toHaveAttribute("aria-expanded", "true");
    await feedback
      .getByLabel("Comment", { exact: true })
      .fill("Native browser shared feedback");
    await feedback
      .getByRole("button", { name: "Post comment", exact: true })
      .click();
    await expect(feedback).toHaveCount(0);
    await expect(visitor.getByTestId("approvals-reviews-toggle")).toContainText(
      "0 published reviews",
    );
    await expect(
      visitor.getByText("Native browser shared feedback", { exact: true }),
    ).toBeVisible();
    await visitor.goto(
      `http://127.0.0.1:5174/${f.org}/${f.workspace}?view=activity`,
    );
    await dismissGuidedIntro(visitor);
    await expect(
      visitor.getByTestId("workspace-activity-open-reviews"),
    ).toHaveCount(0);
    await expect(visitor.getByTestId("review-leave-feedback")).toHaveCount(0);
    await expect(visitor.getByTestId("activity-comment-composer")).toHaveCount(
      0,
    );
    await visitor.goto(
      `http://127.0.0.1:5174/${f.org}/${f.workspace}?view=reviews`,
    );
    await expect(
      visitor.getByTestId("approvals-reviews-toggle"),
    ).toHaveAttribute("aria-expanded", "true");
    await expect(
      visitor.getByText("Native browser shared feedback", { exact: true }),
    ).toBeVisible();
    await owner.reload();
    await dismissGuidedIntro(owner);
    await owner.getByTestId("approvals-reviews-toggle").click();
    await expect(
      owner.getByText("Native browser shared feedback", { exact: true }),
    ).toBeVisible();
    await owner
      .locator('[data-testid^="comment-thread-"]')
      .filter({ hasText: "Native browser shared feedback" })
      .getByTestId("review-reply-open")
      .click();
    const reply = owner
      .locator('[data-testid^="comment-thread-"]')
      .filter({ hasText: "Native browser shared feedback" })
      .getByLabel("Write a reply", { exact: true });
    await reply.fill("Native browser reply");
    await owner.getByTestId("approvals-reviews-toggle").click();
    await owner.getByTestId("approvals-reviews-toggle").click();
    await expect(reply).toHaveValue("Native browser reply");
    await owner
      .locator('[data-testid^="comment-thread-"]')
      .filter({ hasText: "Native browser shared feedback" })
      .getByTestId("review-reply-submit")
      .click();
    await visitor.reload();
    await dismissGuidedIntro(visitor);
    if (
      (await visitor
        .getByTestId("approvals-reviews-toggle")
        .getAttribute("aria-expanded")) === "false"
    )
      await visitor.getByTestId("approvals-reviews-toggle").click();
    await visitor
      .locator('[data-testid^="comment-thread-"]')
      .filter({ hasText: "Native browser shared feedback" })
      .getByTestId("review-reply-open")
      .click();
    await expect(
      visitor.getByText("Native browser reply", { exact: true }),
    ).toBeVisible();
    let startResponse = visitor.waitForResponse(
      (r) =>
        r.request().method() === "POST" && r.url().endsWith("/review/start"),
    );
    await visitor.getByTestId("review-workspace-start").click();
    let started = await startResponse;
    expect([200, 201]).toContain(started.status());
    const copy = (await started.json()).data;
    expect(copy.review.reviewWorkspaceId).toBeTruthy();
    await expect(visitor).not.toHaveURL(root);
    await visitor.goto(root);
    await dismissGuidedIntro(visitor);
    startResponse = visitor.waitForResponse(
      (r) =>
        r.request().method() === "POST" && r.url().endsWith("/review/start"),
    );
    await visitor.getByTestId("review-workspace-start").click();
    started = await startResponse;
    expect([200, 201]).toContain(started.status());
    const reopened = (await started.json()).data;
    expect(reopened.review.reviewWorkspaceId).toBe(
      copy.review.reviewWorkspaceId,
    );
    expect(reopened.created).toBe(false);
    await expect(visitor).not.toHaveURL(root);
    await expect(visitor.getByTestId("review-workspace-start")).toHaveCount(0);
    await visitor.goto(root);
    await dismissGuidedIntro(visitor);
    await visitor.setViewportSize({ width: 390, height: 844 });
    expect(
      await visitor.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    ).toBe(true);
    // A selected composer left open across transfer may disappear on live
    // ownership refresh; its captured operation must still be refused by API.
    await owner.goto(root);
    await dismissGuidedIntro(owner);
    await pick(owner, "Ben reviewer");
    // The reviewed selection is persisted only when submitted; the operation
    // it would send is this one.
    const target = {
      clientRequestId: crypto.randomUUID(),
      definitionId: f.definitionId,
      definitionVersion: 1,
      actorUserId: f.reviewerId,
    };
    const toUserId = JSON.parse(
      Buffer.from(f.visitorToken.split(".")[1], "base64url").toString(),
    ).sub;
    const moved = await owner.evaluate(
      async ({ workspaceId, toUserId }) => {
        const response = await fetch(
          `/api/v1/workspaces/${workspaceId}/transfer`,
          {
            method: "POST",
            headers: {
              "Content-Type": "application/json",
              Authorization: `Bearer ${localStorage.getItem("g5_auth_token")}`,
            },
            body: JSON.stringify({
              toUserId,
              idempotencyKey: crypto.randomUUID(),
            }),
          },
        );
        return { status: response.status, body: await response.json() };
      },
      { workspaceId: f.workspaceId, toUserId },
    );
    expect(moved.status, JSON.stringify(moved.body)).toBe(200);
    expect(moved.body.data.status).toBe("completed");
    const denied = await owner.evaluate(
      async ({ workspaceId, target }) => {
        const response = await fetch(
          `/api/v1/workspaces/${workspaceId}/approvals/requests`,
          {
            method: "POST",
            headers: {
              "Content-Type": "application/json",
              Authorization: `Bearer ${localStorage.getItem("g5_auth_token")}`,
            },
            body: JSON.stringify({
              clientRequestId: target.clientRequestId,
              definitionId: target.definitionId,
              definitionVersion: target.definitionVersion,
              actorUserIds: [target.actorUserId],
            }),
          },
        );
        return response.status;
      },
      { workspaceId: f.workspaceId, target },
    );
    expect(denied).toBe(403);
    await owner.goto(root);
    await dismissGuidedIntro(owner);
    await expect(
      owner.getByRole("heading", {
        name: /^Approvals Required to merge into /,
      }),
    ).toBeVisible();
    await expect(owner.getByTestId("workspace-request-review")).toHaveCount(0);
    const newOwner = await actor(f.visitorToken);
    await expect(
      newOwner.getByTestId("workspace-request-review"),
    ).toBeVisible();
    await checkBulkRequests(newOwner);
    process.stdout.write(JSON.stringify({ passed: true }));
  }
} catch (error) {
  for (const context of browser.contexts())
    for (const page of context.pages()) {
      await page
        .screenshot({
          path: `/tmp/g5-approvals-${process.argv[2]}-${browser.contexts().indexOf(context)}.png`,
          fullPage: true,
        })
        .catch(() => {});
      process.stderr.write(
        `Page ${page.url()}: ${(await page.locator("body").innerText()).slice(0, 4000)}\n`,
      );
      const authority = await page
        .evaluate(async (workspaceId) => {
          const headers = {
            Authorization: `Bearer ${localStorage.getItem("g5_auth_token")}`,
          };
          const workspace = await fetch(`/api/v1/workspaces/${workspaceId}`, {
            headers,
          });
          const detail = await workspace.json();
          const review = await fetch(
            `/api/v1/workspaces/${workspaceId}/review`,
            { headers },
          );
          const identity = await review.json();
          return {
            workspaceStatus: workspace.status,
            owner: detail.data?.ownerUserId,
            readOnly: detail.data?.readOnly,
            reviewStatus: review.status,
            asReview: identity.data?.asReview?.reviewWorkspaceId,
          };
        }, f.workspaceId)
        .catch(() => null);
      process.stderr.write(`Authority: ${JSON.stringify(authority)}\n`);
    }
  throw error;
} finally {
  await browser.close();
}
