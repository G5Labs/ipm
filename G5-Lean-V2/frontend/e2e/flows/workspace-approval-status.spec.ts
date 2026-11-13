// Issue #974 end to end through the real UI and API: one authoritative
// requirement status (g5.governance.approval.status) read the same way by
// Approvals, the workspace Overview, Home and Prepare to Ship; requests
// without a PR (g5.governance.approval.request-pr); approvals kept across a
// source push (g5.governance.approval.source-commit-policy); and the request
// dialog's prefilled Request again / Request outstanding approvals entries
// (g5.governance.approval.request-composer, .request-status).
import {
  test,
  expect,
  api,
  approvals,
  approvalTimeline,
  decide,
  landing,
  openRequestDialog,
  pickGroup,
  previewRequests,
  requestComposer,
  requestDialog,
  requirements,
  revise,
  selectedPairs,
  selectHuman,
  setPhase,
  submitPr,
  submitRequests,
  type Actor,
  type Definition,
  type ReviewFixture,
  chooseRequestMode,
  requestModeTrigger,
} from "../workspace-review";
import { TEST_ORG } from "../helpers";

test.describe.configure({ timeout: 420_000 });

type Counts = `${number} approved, ${number} rejected, ${number} pending`;

function requirementRow(f: ReviewFixture, definition: Definition) {
  return f.owner.page
    .getByTestId("workspace-approvals-requirement")
    .filter({ hasText: definition.name });
}

async function requirementState(f: ReviewFixture, definition: Definition) {
  const row = (await requirements(f)).find(
    (r) => r.definition.id === definition.id,
  );
  expect(row, `requirement ${definition.name}`).toBeTruthy();
  return row as unknown as {
    state: string;
    currentness?: { state: string };
    requests: { requestId: string; actorUserId: string; state: string }[];
  };
}

/** The owner's Approvals page, reloaded so every read is fresh. */
async function ownerApprovals(f: ReviewFixture) {
  await approvals(f.owner, f);
  await f.owner.page.reload();
  await expect(
    f.owner.page.getByTestId("workspace-approvals-outgoing"),
  ).toBeVisible();
}

async function expectCurrentness(
  f: ReviewFixture,
  definition: Definition,
  message: string,
  dismiss: "Escape" | "close",
) {
  const page = f.owner.page;
  await requirementRow(f, definition)
    .getByTestId("workspace-approvals-currentness")
    .click();
  const popup = page.getByTestId("workspace-approvals-currentness-popup");
  await expect(popup).toContainText(message);
  if (dismiss === "Escape") await page.keyboard.press("Escape");
  else
    await page
      .getByTestId("workspace-approvals-currentness-popup-close")
      .click();
  await expect(popup).toHaveCount(0);
  // Dismissing the popup keeps the reader on Approvals.
  await expect(page).toHaveURL(/view=approvals/);
}

/**
 * The same requirement counts on the Overview summary, the Home Workspaces
 * cell and Prepare to Ship ①, with ④'s generic approval blocker while any
 * requirement is unsatisfied (g5.workbench.landing.summary,
 * g5.ui.home-workspaces, g5.workbench.ship-approvals,
 * g5.workbench.ship-to-parent).
 */
async function expectCountsEverywhere(
  f: ReviewFixture,
  counts: Counts,
  icons: {
    definition: Definition;
    word: "Approved" | "Rejected" | "Pending";
  }[],
) {
  const page = f.owner.page;
  await landing(f.owner, f);
  await expect(page.getByTestId("landing-summary-approvals")).toContainText(
    `· ${counts}`,
  );

  await page.goto(`/${TEST_ORG}/home`, { waitUntil: "domcontentloaded" });
  const tile = page.getByTestId("home-workspaces");
  await expect(tile).toBeVisible({ timeout: 15_000 });
  await tile.getByTestId("home-workspace-search").fill(f.workspace.name);
  const cell = tile
    .getByTestId(`home-workspace-${f.workspace.id}`)
    .getByTestId("home-workspace-approvals");
  await expect(cell).toHaveAttribute("title", counts, { timeout: 15_000 });

  await landing(f.owner, f);
  const ship = page
    .getByTestId("ribbon-action-merge")
    .or(page.getByTestId("ribbon-action-create-pr"))
    .first();
  await expect(ship).toBeEnabled({ timeout: 15_000 });
  await ship.click();
  await expect(
    page.getByRole("heading", { name: /Prepare to Ship/ }),
  ).toBeVisible({ timeout: 15_000 });
  await expect(page.getByTestId("prepare-to-ship-approvals-counts")).toHaveText(
    counts,
  );
  // One icon per requirement, in the response's alphabetical order.
  const shown = page.getByTestId("prepare-to-ship-approval-icon");
  await expect(shown).toHaveCount(icons.length);
  for (const [index, { definition, word }] of icons.entries())
    await expect(shown.nth(index)).toHaveAttribute(
      "aria-label",
      `${definition.name}: ${word}`,
    );
  const unsatisfied = icons.filter(({ word }) => word !== "Approved");
  await expect(
    page.getByTestId("prepare-to-ship-approvals-message"),
  ).toHaveText(
    unsatisfied
      .map(({ definition, word }) => `${definition.name} ${word.toLowerCase()}`)
      .join("; "),
  );
  const note = page.getByTestId("prepare-to-ship-ship-note");
  await expect(page.getByTestId("prepare-to-ship-ship-button")).toBeDisabled();
  if (unsatisfied.length > 0) {
    await expect(note).toContainText("Approval(s) required before merging");
    // The blocker never repeats requirement names or statuses.
    for (const { definition } of icons)
      await expect(note).not.toContainText(definition.name);
  }
  await page.getByTestId("prepare-to-ship-close-button").click();
  await expect(
    page.getByRole("heading", { name: /Prepare to Ship/ }),
  ).toHaveCount(0);

  // The server refuses the merge with exactly the same generic reason.
  const state = await api(
    page,
    `/api/v1/workspaces/${f.workspace.id}/approvals`,
  );
  expect(state.status, JSON.stringify(state.body)).toBe(200);
  const admission = state.body.data.outgoing.mergeAdmission;
  expect(admission.admitted).toBe(unsatisfied.length === 0);
  if (unsatisfied.length > 0)
    expect(admission.reason).toBe("Approval(s) required before merging");
}

async function pickAndSubmit(
  f: ReviewFixture,
  definition: Definition,
  recipient: Actor,
  note?: string,
) {
  const { dialog, group } = await requestComposer(f.owner, f, definition);
  await selectHuman(group, recipient.name);
  expect(await selectedPairs(dialog)).toEqual([
    `${definition.name} → ${recipient.name}`,
  ]);
  if (note) await dialog.getByLabel("Note for reviewers (optional)").fill(note);
  await previewRequests(dialog);
  await expect(dialog.getByTestId("approval-request-recipient")).toHaveText([
    `${definition.name} → ${recipient.name}`,
  ]);
  const admitted = f.owner.page.waitForResponse(
    (r) =>
      r.request().method() === "POST" &&
      r.url().endsWith(`/workspaces/${f.workspace.id}/approvals/requests`),
  );
  await submitRequests(dialog);
  expect((await admitted).status()).toBe(201);
  // Full success closes the dialog and refreshes the roster.
  await expect(dialog).toHaveCount(0);
}

/** A prefilled entry opens Pick with exactly one reviewer checked. */
async function expectPrefilledPick(
  f: ReviewFixture,
  definition: Definition,
  recipient: Actor,
) {
  const dialog = requestDialog(f.owner.page);
  await expect(dialog).toBeVisible();
  await expect(requestModeTrigger(dialog)).toHaveText(
    "Pick approvals and approvers▾",
  );
  await expect(
    dialog.getByRole("checkbox", {
      name: "Include satisfied approval requirements",
      exact: true,
    }),
  ).toBeDisabled();
  await expect(dialog.getByRole("checkbox", { checked: true })).toHaveCount(1);
  await expect(
    pickGroup(dialog, definition).getByRole("checkbox", {
      name: recipient.name,
      exact: true,
    }),
  ).toBeChecked();
  expect(await selectedPairs(dialog)).toEqual([
    `${definition.name} → ${recipient.name}`,
  ]);
  return dialog;
}

test("RV2/RV3 a pre-PR human approval keeps one status across Approvals, Overview, Home and Prepare to Ship, survives a push, and prefilled requests recover a rejection", async ({
  review: f,
}) => {
  const page = f.owner.page;
  // Prepare to Ship is the Integration Engineering ship action.
  await setPhase(f, "ie");
  const open = await api(
    page,
    `/api/v1/workspaces/${f.workspace.id}/pull-requests?role=source&status=open`,
  );
  expect(open.status, JSON.stringify(open.body)).toBe(200);
  expect(open.body.data).toHaveLength(0);

  // (a) A fresh dialog selects nothing; Pick requests the reviewer with no PR.
  const fresh = await openRequestDialog(f.owner, f);
  await expect(requestModeTrigger(fresh)).toHaveText(/^Select▾$/);
  await expect(fresh.getByTestId("approval-request-selected")).toHaveCount(0);
  await expect(fresh.getByTestId("approval-request-preview")).toBeDisabled();
  await expect(
    fresh.getByRole("checkbox", {
      name: "Include satisfied approval requirements",
      exact: true,
    }),
  ).toBeEnabled();
  await expect(fresh.getByText(/require a submitted, open PR/)).toHaveCount(0);
  await fresh.getByTestId("approval-request-close").click();
  await expect(fresh).toHaveCount(0);
  await pickAndSubmit(f, f.security, f.reviewer, "Approve before the PR.");
  const security = requirementRow(f, f.security);
  const waiting = security
    .getByTestId("workspace-approvals-wait")
    .filter({ hasText: f.reviewer.name });
  await expect(waiting.getByTestId("review-request-status")).toHaveText(
    "Waiting",
  );
  await expect(waiting.getByTestId("review-request-note")).toContainText(
    "Approve before the PR.",
  );
  await expect(
    security.getByTestId("workspace-approvals-requirement-status"),
  ).toHaveText("Pending");
  await expectCountsEverywhere(f, "0 approved, 0 rejected, 2 pending", [
    { definition: f.business, word: "Pending" },
    { definition: f.security, word: "Pending" },
  ]);

  // (b) The reviewer approves through the UI, still with no PR.
  await decide(f.reviewer, f, f.security, "ie", "Approve", "Approved pre-PR");

  // (c) One status everywhere.
  await ownerApprovals(f);
  await expect(
    security.getByTestId("workspace-approvals-requirement-status"),
  ).toHaveText("Approved");
  await expect(
    requirementRow(f, f.business).getByTestId(
      "workspace-approvals-requirement-status",
    ),
  ).toHaveText("Pending");
  await expect(security.getByTestId("workspace-approvals-path")).toHaveCount(0);
  await expectCurrentness(
    f,
    f.security,
    "All approvals are current.",
    "Escape",
  );
  await expectCountsEverywhere(f, "1 approved, 0 rejected, 1 pending", [
    { definition: f.business, word: "Pending" },
    { definition: f.security, word: "Approved" },
  ]);

  // A human decision made with no PR keeps counting once a PR opens
  // (g5.governance.approval.request-pr).
  await submitPr(f);
  expect((await requirementState(f, f.security)).state).toBe("approved");

  // (d) A source push leaves the approval Approved; only the commit
  // evidence says it is not current (IGNORE).
  await revise(f, "docs", "Approval status source move");
  const moved = await requirementState(f, f.security);
  expect(moved.state).toBe("approved");
  expect(moved.currentness?.state).toBe("changed");
  await ownerApprovals(f);
  await expect(
    security.getByTestId("workspace-approvals-requirement-status"),
  ).toHaveText("Approved");
  await expectCurrentness(
    f,
    f.security,
    `Approvals from ${f.reviewer.name} are not current.`,
    "close",
  );
  await landing(f.owner, f);
  await expect(page.getByTestId("landing-summary-approvals")).toContainText(
    "· 1 approved, 0 rejected, 1 pending",
  );

  // (e) Pick reaches the satisfied requirement directly; a human rejection
  // vetoes the Any-of requirement.
  await pickAndSubmit(f, f.security, f.second);
  await decide(
    f.second,
    f,
    f.security,
    "ie",
    "Request changes",
    "Rollback steps are missing",
  );
  await ownerApprovals(f);
  await expect(
    security.getByTestId("workspace-approvals-requirement-status"),
  ).toHaveText("Rejected");
  await expect(security.getByTestId("workspace-approvals-path")).toHaveText(
    `Rejected by ${f.second.name}. ${f.second.name} has to approve (with no other human rejection) to meet this requirement.`,
  );
  const rejected = security
    .getByTestId("workspace-approvals-decision")
    .filter({ hasText: f.second.name });
  await expect(rejected).toContainText("Rejected");
  await expect(rejected.getByTestId("workspace-approvals-summary")).toHaveText(
    "Rollback steps are missing",
  );
  // Request again sits only beside the latest settled non-approval.
  await expect(
    security
      .getByTestId("workspace-approvals-decision")
      .filter({ hasText: f.reviewer.name })
      .getByTestId("workspace-approvals-request-again"),
  ).toHaveCount(0);
  await expect(
    security.getByTestId("workspace-approvals-request-outstanding"),
  ).toBeVisible();
  await expectCountsEverywhere(f, "0 approved, 1 rejected, 1 pending", [
    { definition: f.business, word: "Pending" },
    { definition: f.security, word: "Rejected" },
  ]);

  // Request again opens Pick with exactly that reviewer checked; Submit
  // leaves a Waiting row with the earlier settled response beneath it.
  await ownerApprovals(f);
  await rejected.getByTestId("workspace-approvals-request-again").click();
  const again = await expectPrefilledPick(f, f.security, f.second);
  await previewRequests(again);
  await expect(again.getByTestId("approval-request-recipient")).toHaveText([
    `${f.security.name} → ${f.second.name}`,
  ]);
  await submitRequests(again);
  await expect(again).toHaveCount(0);
  const rerequested = security
    .getByTestId("workspace-approvals-wait")
    .filter({ hasText: f.second.name });
  await expect(rerequested.getByTestId("review-request-status")).toHaveText(
    "Waiting",
  );
  const prior = rerequested.getByTestId("workspace-approvals-prior");
  await expect(prior).toContainText("Rejected");
  await expect(prior).toContainText("Rollback steps are missing");
  await expect(
    rerequested.getByTestId("workspace-approvals-request-again"),
  ).toHaveCount(0);
  // Every needed reviewer now has an active request.
  await expect(
    security.getByTestId("workspace-approvals-request-outstanding"),
  ).toHaveCount(0);
  await expect(
    security.getByTestId("workspace-approvals-requirement-status"),
  ).toHaveText("Rejected");

  // (f) Cancelling the necessary Waiting request reveals Request outstanding
  // approvals, which opens the same dialog in Pick, prefilled.
  const cancelled = page.waitForResponse(
    (r) =>
      r.request().method() === "POST" &&
      /\/approvals\/requests\/[^/]+\/cancel$/.test(r.url()),
  );
  await rerequested.getByTestId("workspace-approvals-cancel-request").click();
  expect((await cancelled).status()).toBe(200);
  const outstanding = security.getByTestId(
    "workspace-approvals-request-outstanding",
  );
  await expect(outstanding).toBeVisible();
  const afterCancel = security
    .getByTestId("workspace-approvals-wait")
    .filter({ hasText: f.second.name });
  await expect(afterCancel.getByTestId("review-request-status")).toHaveText(
    "Cancelled",
  );
  await expect(
    afterCancel
      .getByTestId("workspace-approvals-prior")
      .getByTestId("workspace-approvals-request-again"),
  ).toBeVisible();
  await outstanding.click();
  const prefilled = await expectPrefilledPick(f, f.security, f.second);
  await prefilled.getByTestId("approval-request-close").click();
  await expect(prefilled).toHaveCount(0);
  expect(
    (await requirementState(f, f.security)).requests.filter(
      (r) => r.state === "pending",
    ),
  ).toHaveLength(0);
  // Cancellation and the new request are ordinary entries in the timeline.
  await expect(await approvalTimeline(page, f.security.name)).toContainText(
    "Rollback steps are missing",
  );

  // (g) Still unsatisfied: the generic blocker, with a PR open.
  await expectCountsEverywhere(f, "0 approved, 1 rejected, 1 pending", [
    { definition: f.business, word: "Pending" },
    { definition: f.security, word: "Rejected" },
  ]);
});
