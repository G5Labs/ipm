import { randomUUID } from "node:crypto";
import {
  test,
  expect,
  api,
  submitPr,
  request,
  openRequestDialog,
  pickGroup,
  requestComposer,
  selectHuman,
  selectedPairs,
  previewRequests,
  submitRequests,
  requirements,
  reviewDialog,
  assertDecision,
  interceptNextPost,
  decide,
  approvals,
  approvalTimeline,
  timelineEntry,
  type ReviewFixture,
  chooseRequestMode,
  requestModeTrigger,
} from "../workspace-review";
import { TEST_MEMBER_ORG } from "../helpers";

test.describe.configure({ timeout: 420_000 });

async function refuse(
  f: ReviewFixture,
  actor: ReviewFixture["visitor"],
  path: string,
  body: Record<string, unknown>,
  statuses = [403],
) {
  const before = await requirements(f);
  // Reach the authorization/request boundary with an explicitly captured cut;
  // missing-vector validation has its own HTTP regression.
  if (path === "approvals/decisions") {
    const reviewed = await api(
      f.owner.page,
      `/api/v1/workspaces/${f.workspace.id}/review-flow`,
    );
    expect(reviewed.status).toBe(200);
    body = { reviewedGitHeads: reviewed.body.data.heads, ...body };
  }
  const response = await api(
    actor.page,
    `/api/v1/workspaces/${f.workspace.id}/${path}`,
    "POST",
    body,
  );
  expect(statuses, JSON.stringify(response.body)).toContain(response.status);
  expect(await requirements(f)).toEqual(before);
}

test("RV2/RV10 pre-PR requests are admitted once, but no self-approval and no unrequested formal review even for configured people or org admins", async ({
  review: f,
}) => {
  await refuse(f, f.visitor, "pull-requests", {
    targetWorkspaceId: f.parent.id,
    description: "Unauthorized submission",
  });
  // PR_REQUIRED is false: the owner may request a configured human before
  // any PR exists (g5.governance.approval.request-pr).
  const [beforePr] = await request(f);
  await refuse(
    f,
    f.reviewer,
    "approvals/decisions",
    { definitionId: f.security.id, phase: "se", outcome: "approved" },
    [403, 409, 422],
  );
  await submitPr(f);
  await refuse(
    f,
    f.owner,
    "approvals/requests",
    {
      definitionId: f.security.id,
      definitionVersion: 1,
      phase: "se",
      actorUserIds: [f.owner.id],
    },
    [403, 422],
  );
  // The pre-PR request stays the reviewer's one active request after the
  // PR opens; asking again never duplicates it.
  const [id] = await request(f);
  expect(id).toBe(beforePr);
  for (const actor of [f.owner, f.visitor, f.second]) {
    await refuse(
      f,
      actor,
      "approvals/decisions",
      {
        definitionId: f.security.id,
        requestId: id,
        phase: "se",
        outcome: "approved",
      },
      // Owners have no decision authority; another reader has no matching
      // addressed request. Both paths must leave persisted state unchanged.
      [actor === f.owner ? 403 : 409],
    );
  }
  await refuse(f, f.visitor, "approvals/decisions", {
    definitionId: f.security.id,
    phase: "se",
    outcome: "approved",
  });
  const promoted = await api(
    f.owner.page,
    `/api/v1/orgs/${f.orgId}/members/${f.visitor.id}`,
    "PATCH",
    { role: "admin" },
  );
  expect(promoted.status).toBe(200);
  await refuse(
    f,
    f.visitor,
    "approvals/decisions",
    {
      definitionId: f.security.id,
      requestId: id,
      phase: "se",
      outcome: "approved",
    },
    [409],
  );
  await refuse(f, f.visitor, "approvals/requests", {
    definitionId: f.security.id,
    definitionVersion: 1,
    phase: "se",
    actorUserIds: [f.second.id],
  });
  await approvals(f.visitor, f);
  await expect(
    f.visitor.page.getByRole("button", { name: "Approve", exact: true }),
  ).toHaveCount(0);
  await expect(
    f.visitor.page.getByRole("button", {
      name: "Add workspace comment",
      exact: true,
    }),
  ).toBeEnabled();
});

test("RV2 specific picker includes only selected recipients; empty, ineligible and unavailable-version requests cannot add work", async ({
  review: f,
}) => {
  await submitPr(f);
  await approvals(f.owner, f);
  const { dialog, group } = await requestComposer(f.owner, f);
  // Pick lists every requirement's eligible recipients and selects nothing
  // until a checkbox is checked (g5.governance.approval.request-composer).
  const business = pickGroup(dialog, f.business);
  await expect(business).toBeVisible();
  await expect(
    business.getByRole("checkbox", { name: f.second.name, exact: true }),
  ).toHaveCount(0);
  await expect(
    dialog.getByRole("checkbox", {
      name: "Include satisfied approval requirements",
      exact: true,
    }),
  ).toBeDisabled();
  await expect(dialog.getByTestId("approval-request-selected")).toHaveCount(0);
  await expect(dialog.getByTestId("approval-request-preview")).toBeDisabled();
  await selectHuman(group, f.second.name);
  expect(await selectedPairs(dialog)).toEqual([
    `${f.security.name} → ${f.second.name}`,
  ]);
  await group
    .getByRole("checkbox", { name: f.second.name, exact: true })
    .uncheck();
  await expect(dialog.getByTestId("approval-request-selected")).toHaveCount(0);
  await expect(dialog.getByTestId("approval-request-preview")).toBeDisabled();
  for (const actorUserIds of [[], [f.second.id]]) {
    await refuse(
      f,
      f.owner,
      "approvals/requests",
      {
        definitionId: f.business.id,
        definitionVersion: 1,
        phase: "se",
        actorUserIds,
      },
      [400, 422],
    );
  }
  await refuse(
    f,
    f.owner,
    "approvals/requests",
    {
      definitionId: f.business.id,
      definitionVersion: 999,
      phase: "se",
      actorUserIds: [f.reviewer.id],
    },
    [404],
  );
  await f.owner.page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
  expect((await requirements(f)).flatMap((r) => r.requests)).toHaveLength(0);
});

test("RV2 bulk requests select one Any-of and every needed All-of reviewer, then hide covered receipts", async ({
  review: f,
}) => {
  await submitPr(f);
  const created = await api(
    f.owner.page,
    `/api/v1/orgs/${f.orgId}/approval-definitions`,
    "POST",
    {
      name: `All reviewers ${f.workspace.id}`,
      humanUserIds: [f.reviewer.id, f.second.id],
      requireAllReviewers: true,
    },
  );
  expect(created.status, JSON.stringify(created.body)).toBe(201);
  const all = created.body.data;
  const attached = await api(
    f.owner.page,
    `/api/v1/workspaces/${f.parent.id}/approvals/requirements`,
    "POST",
    {
      definitionId: all.id,
    },
  );
  expect(attached.status, JSON.stringify(attached.body)).toBe(200);
  const dialog = await openRequestDialog(f.owner, f);
  // A fresh page-level open selects no mode and nothing to send
  // (g5.governance.approval.request-composer).
  await expect(requestModeTrigger(dialog)).toHaveText(/^Select▾$/);
  await expect(dialog.getByTestId("approval-request-selected")).toHaveCount(0);
  await expect(dialog.getByTestId("approval-request-preview")).toBeDisabled();
  const include = dialog.getByRole("checkbox", {
    name: "Include satisfied approval requirements",
    exact: true,
  });
  await expect(include).toBeEnabled();
  await expect(include).not.toBeChecked();
  await chooseRequestMode(dialog, "all");
  // Request all resolves the evaluator's needs once and shows the fixed
  // pairs (g5.governance.approval.selection): one random configured human
  // for each human-only Any-of, every needed All-of approver.
  const candidates = [f.reviewer, f.second, f.visitor];
  const pairs = await selectedPairs(dialog);
  expect(pairs).toHaveLength(4);
  const anyPair = pairs.filter((pair) =>
    pair.startsWith(`${f.security.name} → `),
  );
  expect(anyPair).toHaveLength(1);
  const chosen = candidates.find(
    (actor) => anyPair[0] === `${f.security.name} → ${actor.name}`,
  );
  expect(chosen, `random Any-of human in ${anyPair[0]}`).toBeTruthy();
  expect([...pairs].sort()).toEqual(
    [
      `${all.name} → ${f.reviewer.name}`,
      `${all.name} → ${f.second.name}`,
      `${f.business.name} → ${f.reviewer.name}`,
      `${f.security.name} → ${chosen!.name}`,
    ].sort(),
  );
  await previewRequests(dialog);
  await expect(dialog.getByTestId("approval-request-recipient")).toHaveCount(4);
  await submitRequests(dialog);
  // Full success closes the dialog; there is no Accepted receipt to clear.
  await expect(dialog).toHaveCount(0);
  const rows = await requirements(f);
  expect(
    rows
      .find((r) => r.definition.id === f.security.id)!
      .requests.map((r) => r.actorUserId),
  ).toEqual([chosen!.id]);
  expect(
    rows
      .find((r) => r.definition.id === all.id)!
      .requests.map((r) => r.actorUserId)
      .sort(),
  ).toEqual([f.reviewer.id, f.second.id].sort());
  expect(
    rows.find((r) => r.definition.id === f.business.id)!.requests,
  ).toHaveLength(1);
  await f.owner.page.getByTestId("workspace-request-review").click();
  await expect(dialog).toBeVisible();
  await expect(dialog.getByText("Accepted", { exact: true })).toHaveCount(0);
  await expect(dialog.getByTestId("approval-request-recipient")).toHaveCount(0);
  await expect(requestModeTrigger(dialog)).toHaveText(/^Select▾$/);
  for (const mode of ["all", "agents"] as const) {
    // Every needed reviewer has an active request; nothing is duplicated.
    await chooseRequestMode(dialog, mode);
    await expect(dialog.getByTestId("approval-request-pair")).toHaveCount(0);
    await expect(
      dialog.getByText("No new approvals to request with this selection."),
    ).toBeVisible();
    await expect(dialog.getByTestId("approval-request-preview")).toBeDisabled();
  }
  await dialog.getByTestId("approval-request-close").click();
  await expect(dialog).toHaveCount(0);
  expect(await requirements(f)).toEqual(rows);
});

test("RV2 satisfied approvals stay folded unless explicitly included for a new request", async ({
  review: f,
}) => {
  await submitPr(f);
  await request(f, f.business);
  await decide(
    f.reviewer,
    f,
    f.business,
    "be",
    "Approve",
    "Satisfied before requesting again",
  );
  await request(f, f.security, [f.second]);
  const dialog = await openRequestDialog(f.owner, f);
  await chooseRequestMode(dialog, "all");
  // Bulk modes skip Approved requirements unless explicitly included, and
  // never duplicate an active request (g5.governance.approval.selection).
  await expect(dialog.getByTestId("approval-request-pair")).toHaveCount(0);
  await expect(
    dialog.getByText("No new approvals to request with this selection."),
  ).toBeVisible();
  await expect(dialog.getByTestId("approval-request-preview")).toBeDisabled();
  await dialog.getByTestId("approval-request-selected-info").click();
  const why = f.owner.page.getByTestId("approval-request-selected-info-popup");
  await expect(why).toContainText(`${f.business.name}: already approved`);
  await f.owner.page
    .getByTestId("approval-request-selected-info-popup-close")
    .click();
  await expect(why).toHaveCount(0);
  await expect(dialog).toBeVisible();
  await dialog
    .getByRole("checkbox", {
      name: "Include satisfied approval requirements",
      exact: true,
    })
    .check();
  expect(await selectedPairs(dialog)).toEqual([
    `${f.business.name} → ${f.reviewer.name}`,
  ]);
  await previewRequests(dialog);
  await submitRequests(dialog);
  await expect(dialog).toHaveCount(0);
  const rows = await requirements(f);
  const business = rows.find((r) => r.definition.id === f.business.id)!;
  expect(business.requests).toHaveLength(2);
  expect(business.decisionLog).toHaveLength(1);
  const retired = await api(
    f.owner.page,
    `/api/v1/workspaces/${f.parent.id}/approvals/requirements/${f.business.id}/retire`,
    "POST",
    {},
  );
  expect(retired.status, JSON.stringify(retired.body)).toBe(200);
  await f.owner.page.reload();
  const legacy = f.owner.page.getByTestId("workspace-approvals-legacy");
  await expect(legacy).toBeVisible();
  await expect(legacy).not.toHaveAttribute("open");
  await expect(
    f.owner.page
      .getByTestId("workspace-approvals-requirement")
      .filter({ hasText: f.business.name }),
  ).toHaveCount(0);
  await legacy.getByTestId("workspace-approvals-legacy-toggle").click();
  const history = await approvalTimeline(f.owner.page, f.business.name);
  await expect(history).toContainText("Satisfied before requesting again");
  await expect(
    history.getByTestId("workspace-approvals-history-card"),
  ).toHaveCount(2);
  const newestPerRequest = await history
    .getByTestId("workspace-approvals-history-card")
    .evaluateAll((cards) =>
      cards.map((card) =>
        Math.max(
          ...Array.from(card.querySelectorAll("time"), (node) =>
            Date.parse(node.getAttribute("datetime")!),
          ),
        ),
      ),
    );
  expect(newestPerRequest).toEqual([...newestPerRequest].sort((a, b) => b - a));
  await f.owner.page.reload();
  await expect(legacy).toBeVisible();
  await expect(legacy).not.toHaveAttribute("open");

  expect(
    rows.find((r) => r.definition.id === f.security.id)!.requests,
  ).toHaveLength(1);
});

test("RV10 misaddressed workspace, definition and phase IDs and cancelled requests cannot settle an approval", async ({
  review: f,
}) => {
  await submitPr(f);
  const [id] = await request(f);
  const reviewed = await api(
    f.reviewer.page,
    `/api/v1/workspaces/${f.workspace.id}/review-flow`,
  );
  expect(reviewed.status).toBe(200);
  const wrongWorkspace = await api(
    f.reviewer.page,
    `/api/v1/workspaces/${f.parent.id}/approvals/decisions`,
    "POST",
    {
      requestId: id,
      definitionId: f.security.id,
      phase: "se",
      outcome: "approved",
      reviewedGitHeads: reviewed.body.data.heads,
    },
  );
  // Requests need no PR (g5.governance.approval.request-pr); this other
  // workspace has no request with this id, so admission refuses it as a
  // conflict before an unrelated request can ever be settled.
  expect(wrongWorkspace.status, JSON.stringify(wrongWorkspace.body)).toBe(409);
  for (const body of [
    {
      requestId: id,
      definitionId: f.business.id,
      phase: "se",
      outcome: "approved",
    },
    {
      requestId: id,
      definitionId: f.security.id,
      phase: "invalid-phase",
      outcome: "approved",
    },
    {
      requestId: randomUUID(),
      definitionId: f.security.id,
      phase: "se",
      outcome: "approved",
    },
  ])
    await refuse(
      f,
      f.reviewer,
      "approvals/decisions",
      body,
      [403, 404, 409, 422],
    );
  const dialog = await reviewDialog(f.reviewer, f);
  await dialog
    .getByLabel("Review comment")
    .fill("My draft must survive a cancellation");
  const cancelled = await api(
    f.owner.page,
    `/api/v1/workspaces/${f.workspace.id}/approvals/requests/${id}/cancel`,
    "POST",
    {},
  );
  expect(cancelled.status).toBe(200);
  await dialog
    .getByRole("button", { name: "Submit review", exact: true })
    .click();
  await expect(dialog.getByRole("alert")).toContainText(
    /cancelled|no longer pending/i,
  );
  await expect(dialog.getByLabel("Review comment")).toHaveValue(
    "My draft must survive a cancellation",
  );
  await refuse(
    f,
    f.reviewer,
    "approvals/decisions",
    {
      requestId: id,
      definitionId: f.security.id,
      phase: "se",
      outcome: "approved",
    },
    [409, 422],
  );
});

for (const loss of ["before-save", "after-save"] as const) {
  test(`RV10 decision ${loss} network failure keeps the draft and retry records exactly one decision and timeline event`, async ({
    review: f,
  }) => {
    await submitPr(f);
    const [id] = await request(f);
    const dialog = await reviewDialog(f.reviewer, f);
    const message = `Retry decision ${loss}`;
    await dialog.getByLabel("Review comment").fill(message);
    const url = `**/api/v1/workspaces/${f.workspace.id}/approvals/decisions`;
    let intercepted = false;
    await interceptNextPost(f.reviewer.page, url, async (route) => {
      if (route.request().method() !== "POST") return route.continue();
      intercepted = true;
      if (loss === "after-save") {
        const saved = await route.fetch();
        expect(saved.status()).toBe(200); // Real server wrote it; only its response is lost.
      }
      await route.abort("failed");
    });
    await dialog
      .getByRole("button", { name: "Submit review", exact: true })
      .click();
    await expect(dialog.getByRole("alert")).toBeVisible();
    expect(intercepted).toBe(true);
    await expect(dialog.getByLabel("Review comment")).toHaveValue(message);
    const count = (await requirements(f))
      .flatMap((r) => r.decisionLog)
      .filter((d) => d.requestId === id).length;
    expect(count).toBe(loss === "after-save" ? 1 : 0);
    await dialog
      .getByRole("button", { name: "Submit review", exact: true })
      .click();
    await expect(dialog).toHaveCount(0);
    await assertDecision(f, id, f.reviewer, "approved", f.pushed, message);
    for (const actor of [f.owner, f.reviewer]) {
      await actor.page.reload();
      await timelineEntry(actor, f, message);
    }
  });
}

test("RV10 failed request submissions retain selections and retry without duplicate durable objects", async ({
  review: f,
}) => {
  // PR submission and its failure recovery belong to Prepare to Ship; the
  // landing offers no PR action (g5.workbench.landing.implementation).
  await submitPr(f);
  const after = await api(
    f.owner.page,
    `/api/v1/workspaces/${f.workspace.id}/pull-requests?role=source&status=open`,
  );
  expect(after.status).toBe(200);
  expect(after.body.data).toHaveLength(1);
  await approvals(f.owner, f);
  const { dialog: picker, group } = await requestComposer(f.owner, f);
  await selectHuman(group, f.reviewer.name);
  await previewRequests(picker);
  await expect(picker.getByTestId("approval-request-recipient")).toHaveText([
    `${f.security.name} → ${f.reviewer.name}`,
  ]);
  // A slow roster refresh must not delay the lost-response error or its
  // stable retry. Hold that independent read until the error is actionable.
  let refreshStarted = false;
  let releaseRefresh!: () => void;
  const refresh = new Promise<void>((resolve) => {
    releaseRefresh = resolve;
  });
  await f.owner.page.route(
    `**/api/v1/workspaces/${f.workspace.id}/approvals`,
    async (route) => {
      if (route.request().method() === "GET") {
        refreshStarted = true;
        await refresh;
      }
      await route.continue();
    },
  );
  const requestPath = `/api/v1/workspaces/${f.workspace.id}/approvals/requests`;
  await interceptNextPost(f.owner.page, `**${requestPath}`, async (route) => {
    const saved = await route.fetch();
    expect(saved.status()).toBe(201);
    await route.abort("failed");
  });
  try {
    await Promise.all([
      f.owner.page.waitForEvent("requestfailed", {
        predicate: (request) =>
          request.method() === "POST" && request.url().endsWith(requestPath),
      }),
      submitRequests(picker),
    ]);
    await expect.poll(() => refreshStarted).toBe(true);
    await expect(
      picker
        .getByTestId("approval-request-recipient")
        .getByRole("alert")
        .filter({ hasText: /Failed or unconfirmed/ }),
    ).toBeVisible();
    await expect(picker.getByTestId("review-request-submit")).toHaveText(
      "Retry failed requests",
    );
    await expect(picker.getByTestId("review-request-submit")).toBeEnabled();
    await expect(
      picker.getByTestId("approval-request-new-selection"),
    ).toBeEnabled();
  } finally {
    releaseRefresh();
  }
  // The response was lost after persistence. The addressed reviewer can
  // finish before the owner retries; retry must not open a successor task.
  await decide(
    f.reviewer,
    f,
    f.security,
    "se",
    "Approve",
    "Reviewed before retry",
  );
  // The retry replays the same operation identity; its success closes the
  // dialog instead of leaving an Accepted receipt or a disabled retry
  // (g5.governance.approval.request-composer).
  await submitRequests(picker);
  await expect(picker).toHaveCount(0);
  const requests = (await requirements(f)).flatMap((r) => r.requests);
  expect(requests).toHaveLength(1);
  expect(requests[0]).toMatchObject({
    actorUserId: f.reviewer.id,
    state: "settled",
  });
});

test("RV10 simultaneous reviewers settle only their addressed request, including double-click submission", async ({
  review: f,
}) => {
  await submitPr(f);
  const [first, second] = await request(f, f.security, [f.reviewer, f.second]);
  const one = await reviewDialog(f.reviewer, f);
  const two = await reviewDialog(f.second, f);
  await one.getByLabel("Review comment").fill("First reviewer outcome");
  await two.getByLabel("Review comment").fill("Second reviewer outcome");
  await two.getByRole("radio", { name: /^Request changes/ }).check();
  await Promise.all([
    one.getByRole("button", { name: "Submit review", exact: true }).dblclick(),
    two.getByRole("button", { name: "Submit review", exact: true }).click(),
  ]);
  await expect(one).toHaveCount(0);
  await expect(two).toHaveCount(0);
  await assertDecision(
    f,
    first,
    f.reviewer,
    "approved",
    f.pushed,
    "First reviewer outcome",
  );
  await assertDecision(
    f,
    second,
    f.second,
    "rejected",
    f.pushed,
    "Second reviewer outcome",
  );
});

test("RV10 membership revocation during a review refuses submission without losing draft text", async ({
  review: f,
}) => {
  await submitPr(f);
  await request(f);
  const dialog = await reviewDialog(f.reviewer, f);
  await dialog
    .getByLabel("Review comment")
    .fill("Draft before membership removal");
  const draftPrefix = `g5:formal-review:${f.reviewer.id}:${f.workspace.id}:`;
  const draftsBefore = await f.reviewer.page.evaluate(
    (prefix) =>
      Object.entries(sessionStorage).filter(([key]) => key.startsWith(prefix)),
    draftPrefix,
  );
  expect(draftsBefore).toHaveLength(1);
  expect(JSON.parse(draftsBefore[0][1]).comment).toBe(
    "Draft before membership removal",
  );
  const workspacePath = new URL(f.reviewer.page.url()).pathname;
  const removed = await api(
    f.owner.page,
    `/api/v1/orgs/${f.orgId}/members/${f.reviewer.id}`,
    "DELETE",
  );
  expect([200, 204], JSON.stringify(removed.body)).toContain(removed.status);
  const submitted = f.reviewer.page.waitForResponse(
    (r) =>
      r.request().method() === "POST" &&
      r.url().endsWith(`/workspaces/${f.workspace.id}/approvals/decisions`),
  );
  await dialog
    .getByRole("button", { name: "Submit review", exact: true })
    .click();
  expect([403, 404]).toContain((await submitted).status());
  // Current membership recovery closes revoked organization views. The
  // person's private draft must survive that forced navigation unchanged.
  await expect(dialog).toHaveCount(0);
  await expect
    .poll(() => new URL(f.reviewer.page.url()).pathname)
    .not.toBe(workspacePath);
  expect(
    await f.reviewer.page.evaluate(
      (prefix) =>
        Object.entries(sessionStorage).filter(([key]) =>
          key.startsWith(prefix),
        ),
      draftPrefix,
    ),
  ).toEqual(draftsBefore);
  expect((await requirements(f)).flatMap((r) => r.decisionLog)).toHaveLength(0);
});

test("RV10 ownership transfer fences a request picker left open by the former owner", async ({
  review: f,
}) => {
  await submitPr(f);
  await approvals(f.owner, f);
  const { dialog, group } = await requestComposer(f.owner, f);
  await selectHuman(group, f.reviewer.name);
  await previewRequests(dialog);
  const transfer = async (from: typeof f.owner, to: typeof f.owner) => {
    const key = randomUUID();
    await expect
      .poll(
        async () => {
          const result = await api(
            from.page,
            `/api/v1/workspaces/${f.workspace.id}/transfer`,
            "POST",
            { toUserId: to.id, idempotencyKey: key },
          );
          expect(result.status, JSON.stringify(result.body)).toBe(200);
          return result.body.data.status;
        },
        { timeout: 120_000, intervals: [1_000, 2_000] },
      )
      .toBe("completed");
    f.retainedRuntimeParticipants.add(from.id);
    f.retainedRuntimeParticipants.add(to.id);
  };
  await transfer(f.owner, f.visitor);
  try {
    // Ownership refresh can close or disable the owner-only composer before
    // the former owner submits. Otherwise admission must refuse the saved
    // selection. Exercise whichever side wins that real refresh race.
    const before = await requirements(f);
    await f.owner.page.evaluate(() => {
      document
        .querySelector<HTMLButtonElement>(
          '[data-testid="review-request-submit"]',
        )
        ?.click();
    });
    await expect(async () => {
      if (await dialog.isVisible()) {
        const send = dialog.getByTestId("review-request-submit");
        const refused = dialog.getByText(/Failed or unconfirmed/);
        expect((await refused.isVisible()) || (await send.isDisabled())).toBe(
          true,
        );
      } else {
        await expect(
          f.owner.page.getByTestId("workspace-request-review"),
        ).toHaveCount(0);
      }
    }).toPass();
    expect(await requirements(f)).toEqual(before);
    await refuse(f, f.owner, "approvals/requests", {
      definitionId: f.security.id,
      definitionVersion: 1,
      phase: "se",
      actorUserIds: [f.reviewer.id],
    });
    await approvals(f.visitor, f);
    await expect(
      f.visitor.page.getByRole("button", {
        name: "Request ▾",
        exact: true,
      }),
    ).toBeEnabled();
  } finally {
    await transfer(f.visitor, f.owner);
  }
});

test("RV10 an approval definition from another organization cannot be used in this workspace", async ({
  review: f,
}) => {
  // The foreign organization is an existing E2E fixture the owner also
  // administers (`g5labs-member` by default): only platform operators create
  // organizations (g5.organization.creation). Each run adds a uniquely named
  // definition there; this disposable stack is never QA.
  const me = await api(f.owner.page, "/api/v1/users/me");
  expect(me.status, JSON.stringify(me.body)).toBe(200);
  const administered = (
    (me.body.data?.orgs ?? []) as Array<{
      orgId: string;
      orgSlug: string;
      role: string;
      orgKind?: string;
    }>
  ).filter(
    (org) =>
      org.orgId !== f.orgId &&
      org.orgKind !== "operator" &&
      org.role === "admin",
  );
  const foreign =
    administered.find((org) => org.orgSlug === TEST_MEMBER_ORG) ??
    administered[0];
  expect(
    foreign,
    `the owner administers a second fixture organization (${TEST_MEMBER_ORG})`,
  ).toBeTruthy();
  const foreignOrg = foreign!.orgId;
  const definition = await api(
    f.owner.page,
    `/api/v1/orgs/${foreignOrg}/approval-definitions`,
    "POST",
    {
      name: `Foreign security ${randomUUID()}`,
      humanUserIds: [f.owner.id],
      phases: ["se"],
    },
  );
  expect(definition.status, JSON.stringify(definition.body)).toBe(201);
  await submitPr(f);
  await refuse(
    f,
    f.owner,
    "approvals/requests",
    {
      definitionId: definition.body.data.id,
      definitionVersion: 1,
      phase: "se",
      actorUserIds: [f.reviewer.id],
    },
    [403, 404, 422],
  );
  const [id] = await request(f);
  await refuse(
    f,
    f.reviewer,
    "approvals/decisions",
    {
      definitionId: definition.body.data.id,
      requestId: id,
      phase: "se",
      outcome: "approved",
    },
    [409],
  );
});
