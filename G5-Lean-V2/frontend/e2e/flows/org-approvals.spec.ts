import { expect, test, type Page } from "@playwright/test";
import { approvalTimeline, createReviewActor } from "../workspace-review";
import {
  assertActiveSubject,
  assertSingleRefreshCookie,
  authedApi,
  cloneWorkspaceById,
  deleteWorkspaceById,
  openWorkspacePath,
  readCurrentOrgContext,
  recordExpectedSubject,
  resolveWorkspaceId,
  TEST_ORG,
  USING_OAUTH_STUB,
} from "../helpers";
import {
  loginWithOAuthStub,
  resetOAuthStubIdentity,
  SEEDED_ORG_ADMIN_IDENTITY,
} from "../oauth";

// =============================================================================
// Org approval-definition library E2E — B6 governance UI PR-1.
//
// Implements test_contract/org_approvals_management D6: the real browser
// round-trip over the org-wide `approval_definitions` library (migration 186)
// against the live stack — flyout entrypoint, Approvals center-panel tab,
// full-page create, card provenance, full-page edit with invalidation, and
// API-verified append-only versioning.
//
// Cleanup mechanism: the e2e stack itself is the disposable fixture. No
// delete path exists for `approval_definitions` (append-only by design,
// tenant SELECT+INSERT only) and the product has no org-delete API either,
// so a per-suite disposable org would strand a whole org cascade — strictly
// more residue. The CI e2e lane provisions a fresh database per run, which
// tears down every row this suite creates; rows are uniquely named so local
// reruns against a persistent dev stack never collide.
// =============================================================================

const APPROVALS_PATH = `/${TEST_ORG}/settings/approvals`;

async function openApprovalsTab(page: Page) {
  await openWorkspacePath(page, APPROVALS_PATH);
  await expect(
    page.getByRole("heading", { name: "Approval definitions", exact: true }),
  ).toBeVisible({ timeout: 30_000 });
}

/**
 * Become the deterministic non-owner organization admin of `orgId`: the
 * seeded owner invites the fixed identity as `admin`, then the browser logs
 * in as it through the OAuth stub with a single, asserted refresh credential.
 */
async function useNonOwnerOrgAdminIdentity(page: Page, orgId: string) {
  const invited = await authedApi(
    page,
    `/api/v1/orgs/${orgId}/members`,
    "POST",
    {
      email: SEEDED_ORG_ADMIN_IDENTITY.email,
      role: "admin",
    },
  );
  expect(
    [200, 201],
    `invite org admin: ${JSON.stringify(invited.body)}`,
  ).toContain(invited.status);

  await page.context().clearCookies();
  await page.evaluate(() => localStorage.clear());
  await loginWithOAuthStub(page, SEEDED_ORG_ADMIN_IDENTITY, {
    targetOrgSlug: TEST_ORG,
  });
  const userId = await recordExpectedSubject(page);
  await assertSingleRefreshCookie(page.context(), "org admin login");
  await assertActiveSubject(page, "org admin login");

  const currentUser = await authedApi(page, "/api/v1/users/me");
  expect(currentUser.status, JSON.stringify(currentUser.body)).toBe(200);
  const user = currentUser.body?.data ?? currentUser.body ?? {};
  const membership = (Array.isArray(user.orgs) ? user.orgs : []).find(
    (org: Record<string, unknown>) => org.orgId === orgId,
  );
  expect(membership?.role).toBe("admin");
  return { userId };
}

function approvalRequestStates(requirement: Record<string, unknown>) {
  return (requirement.requests as Array<Record<string, unknown>>).map(
    (request) => request.state,
  );
}

test.describe("Org approvals — definition library round-trip", () => {
  // The account flyout now shows only Open Home and Show Admin Page per org;
  // every org configuration surface (including Approval Definitions) is reached
  // through the Admin hub page. Verify that path lands on the Approvals tab.
  test("flyout Show Admin Page reaches the Approval Definitions surface", async ({
    page,
  }) => {
    await openWorkspacePath(page, `/${TEST_ORG}/base`);
    await page.getByTestId("org-management-button").click();

    // The former per-org cluster links are gone from the flyout.
    await expect(page.getByTestId("org-manage-approvals-link")).toHaveCount(0);
    await expect(page.getByTestId("org-manage-workers-link")).toHaveCount(0);

    const adminLink = page.getByTestId("org-show-admin-link").first();
    await expect(adminLink).toBeVisible({ timeout: 15_000 });
    await expect(adminLink).toHaveText("Show Admin Page →");

    await adminLink.click();
    // The Admin hub lists every configuration surface.
    await expect(page.getByTestId("org-admin-page-content")).toBeVisible({
      timeout: 30_000,
    });
    await expect(page).toHaveURL(/view=admin/);

    await page.getByTestId("org-admin-link-approvals").click();
    await expect(
      page.getByRole("heading", { name: "Approval definitions", exact: true }),
    ).toBeVisible({ timeout: 30_000 });
    // B6 PR-2 param migration: the definitions LIBRARY lives at
    // ?view=approval-definitions.
    await expect(page).toHaveURL(/view=approval-definitions/);

    // The Workers surface is reachable from the hub and via its URL.
    await openWorkspacePath(page, `/${TEST_ORG}/base?view=workers`);
    await expect(
      page.getByRole("heading", { name: "Hosts", exact: true }),
    ).toBeVisible({ timeout: 30_000 });
  });

  // test_contract/workspace_approvals_management D6 — the workspace Approvals
  // tab (?view=approvals): attach a library definition to Base, see it
  // active, retire it, and verify the durable attachment audit stays out of
  // request and decision timelines.
  test("workspace approvals tab attaches and retires a requirement with history", async ({
    page,
  }) => {
    const definitionName = `E2E attach target ${Date.now()}`;

    // Seed a library definition through the API (owner = e2e user).
    const { orgId } = await (async () => {
      await openWorkspacePath(page, `/${TEST_ORG}/base`);
      return readCurrentOrgContext(page);
    })();
    const baseWorkspaceId = await resolveWorkspaceId(page, TEST_ORG, "base");
    // Resolve identity directly; an Admin role does not identify one person.
    const me = await authedApi(page, "/api/v1/users/me", "GET");
    expect(me.status).toBe(200);
    const myUserId = me.body.data.id;
    expect(myUserId).toBeTruthy();
    const created = await authedApi(
      page,
      `/api/v1/orgs/${orgId}/approval-definitions`,
      "POST",
      { name: definitionName, humanUserIds: [myUserId] },
    );
    expect(created.status).toBe(201);

    // The workspace Approvals tab on Base (?view=approvals). Base is
    // target-less, so management renders as the always-expanded first panel
    // (no Advanced disclosure).
    await openWorkspacePath(page, `/${TEST_ORG}/base?view=approvals`);
    await expect(
      page.getByTestId("workspace-approvals-required-into"),
    ).toBeVisible({ timeout: 30_000 });

    // Attach through the full-page picker.
    await page.getByTestId("workspace-approvals-require").click();
    const pick = page
      .getByTestId("workspace-approvals-pick-card")
      .filter({ hasText: definitionName });
    await expect(pick).toBeVisible({ timeout: 15_000 });
    await pick.click();
    await page.getByTestId("workspace-approvals-picker-submit").click();

    // Active requirement row appears.
    const row = page
      .getByTestId("workspace-approvals-required-row")
      .filter({ hasText: definitionName });
    await expect(row).toBeVisible({ timeout: 15_000 });

    // Retire via the ⋯ menu; confirm copy frames it as a retire event.
    await row.getByTestId("workspace-approvals-required-menu").click();
    await row.getByTestId("workspace-approvals-stop-requiring").click();
    await row.getByTestId("workspace-approvals-confirm-retire").click();
    await expect(row).not.toBeVisible({ timeout: 15_000 });

    // Attachment bookkeeping stays durable in its dedicated API projection,
    // but neither event masquerades as an approval request in the timeline.
    const state = await authedApi(
      page,
      `/api/v1/workspaces/${baseWorkspaceId}/approvals`,
      "GET",
    );
    expect(state.status).toBe(200);
    const attachmentEvents = (
      state.body.data.attachmentHistory as Array<Record<string, unknown>>
    ).filter((event) => event.definitionId === created.body.data.id);
    expect(attachmentEvents).toHaveLength(2);
    expect(attachmentEvents.map((event) => event.retired).sort()).toEqual([
      false,
      true,
    ]);

    await expect(page.getByTestId("workspace-approvals-feed")).toHaveCount(0);
    await expect(
      page.getByRole("heading", { name: "Approval history", exact: true }),
    ).toHaveCount(0);
  });

  // Issue #278 — requirement administration is org-admin authority, not
  // workspace ownership: a non-owner org admin attaches and retires a Base
  // requirement through the same tab, with durable attachment history.
  test("non-owner org admin attaches and retires a Base requirement", async ({
    page,
  }) => {
    test.skip(
      !USING_OAUTH_STUB,
      "Needs the OAuth stub to log in a deterministic non-owner org admin",
    );
    const definitionName = `E2E admin attach ${Date.now()}`;

    await openWorkspacePath(page, `/${TEST_ORG}/base`);
    const { orgId } = await readCurrentOrgContext(page);
    const baseWorkspaceId = await resolveWorkspaceId(page, TEST_ORG, "base");
    const me = await authedApi(page, "/api/v1/users/me", "GET");
    expect(me.status).toBe(200);
    const ownerUserId = me.body.data.id;
    expect(ownerUserId).toBeTruthy();
    const created = await authedApi(
      page,
      `/api/v1/orgs/${orgId}/approval-definitions`,
      "POST",
      { name: definitionName, humanUserIds: [ownerUserId] },
    );
    expect(created.status).toBe(201);

    try {
      const { userId: adminUserId } = await useNonOwnerOrgAdminIdentity(
        page,
        orgId,
      );
      const base = await authedApi(
        page,
        `/api/v1/workspaces/${baseWorkspaceId}`,
      );
      expect(base.status, JSON.stringify(base.body)).toBe(200);
      const baseData = base.body?.data ?? base.body ?? {};
      const baseOwner = baseData.ownerUserId ?? baseData.owner_user_id;
      expect(baseOwner, JSON.stringify(baseData)).toBeTruthy();
      expect(baseOwner).not.toBe(adminUserId);

      await openWorkspacePath(page, `/${TEST_ORG}/base?view=approvals`);
      await expect(
        page.getByTestId("workspace-approvals-required-into"),
      ).toBeVisible({ timeout: 30_000 });

      await page.getByTestId("workspace-approvals-require").click();
      const pick = page
        .getByTestId("workspace-approvals-pick-card")
        .filter({ hasText: definitionName });
      await expect(pick).toBeVisible({ timeout: 15_000 });
      await pick.click();
      const attachResponse = page.waitForResponse(
        (response) =>
          new URL(response.url()).pathname ===
            `/api/v1/workspaces/${baseWorkspaceId}/approvals/requirements` &&
          response.request().method() === "POST",
      );
      await page.getByTestId("workspace-approvals-picker-submit").click();
      const attached = await attachResponse;
      expect(attached.status(), await attached.text()).toBe(200);

      const row = page
        .getByTestId("workspace-approvals-required-row")
        .filter({ hasText: definitionName });
      await expect(row).toBeVisible({ timeout: 15_000 });

      await row.getByTestId("workspace-approvals-required-menu").click();
      await row.getByTestId("workspace-approvals-stop-requiring").click();
      await row.getByTestId("workspace-approvals-confirm-retire").click();
      await expect(row).not.toBeVisible({ timeout: 15_000 });

      const state = await authedApi(
        page,
        `/api/v1/workspaces/${baseWorkspaceId}/approvals`,
        "GET",
      );
      expect(state.status).toBe(200);
      const attachmentEvents = (
        state.body.data.attachmentHistory as Array<Record<string, unknown>>
      ).filter((event) => event.definitionId === created.body.data.id);
      expect(attachmentEvents).toHaveLength(2);
      expect(attachmentEvents.map((event) => event.retired).sort()).toEqual([
        false,
        true,
      ]);
    } finally {
      // Never leave Base requiring this definition for later specs that merge
      // into Base; the retire is idempotent and 404s when never attached.
      await authedApi(
        page,
        `/api/v1/workspaces/${baseWorkspaceId}/approvals/requirements/${created.body.data.id}/retire`,
        "POST",
        {},
      ).catch(() => undefined);
      await resetOAuthStubIdentity(page);
    }
  });

  // D6 — create through the full-page editor, verify the card's v1 "Created
  // by" provenance, edit with invalidation, and verify append-only versioning
  // through the API.
  test("admin creates a definition, sees the card, edits with invalidation, and the API shows v2", async ({
    page,
  }) => {
    const definitionName = `E2E merge sign-off ${Date.now()}`;

    await openApprovalsTab(page);

    // Create subview (full page, not a modal).
    await page.getByTestId("org-approvals-new").click();
    await expect(page.getByTestId("org-approvals-name-input")).toBeVisible({
      timeout: 15_000,
    });
    const submit = page.getByTestId("org-approvals-submit");
    await expect(submit).toBeDisabled();

    await page.getByTestId("org-approvals-name-input").fill(definitionName);
    await page
      .getByTestId("org-approvals-description-input")
      .fill("Seeded by the org-approvals E2E round-trip.");
    await expect(
      page.getByTestId("org-approvals-advanced-content"),
    ).toBeHidden();
    await page.getByTestId("org-approvals-advanced-toggle").click();
    await expect(
      page.getByTestId("org-approvals-human-override"),
    ).toBeChecked();
    await expect(page.getByTestId("org-approvals-quorum")).toHaveValue("any");
    await expect(
      page.getByTestId("org-approvals-self-approval"),
    ).not.toBeChecked();
    // Composition rule: name alone is not enough — select one human approver.
    await expect(submit).toBeDisabled();
    await page
      .locator('[data-testid^="org-approvals-member-pill-"]')
      .first()
      .click();
    await expect(submit).toBeEnabled();
    const phase = page.getByRole("combobox", {
      name: "Earliest relevant phase",
    });
    await expect(phase).toBeVisible();
    await expect(phase).toHaveValue("ie");
    await expect(phase).not.toHaveAttribute("multiple");
    await phase.selectOption("be");
    await expect(submit).toBeEnabled();
    await submit.click();

    // Back on the library: the card exists with v1 creator provenance and no
    // version chrome.
    const card = page
      .getByTestId("org-approvals-card")
      .filter({ hasText: definitionName });
    await expect(card).toBeVisible({ timeout: 15_000 });
    // The card foot shows v1 provenance: "Created by <name> · <date>".
    await expect(card.getByText(/created by .+ · /i)).toBeVisible();
    // New definitions default to one approval with no denial and self off.
    await expect(
      card.getByText("At least one reviewer must approve"),
    ).toBeVisible();
    await expect(card.getByText("1 person", { exact: true })).toBeVisible();
    // The card shows only the selected recommendation.
    await expect(card.getByTestId("org-approvals-card-phase-be")).toBeVisible();
    await expect(card.getByTestId("org-approvals-card-phase-ie")).toHaveCount(
      0,
    );
    await expect(card.getByTestId("org-approvals-card-phase-se")).toHaveCount(
      0,
    );

    // API check: exactly v1.
    const { orgId } = await readCurrentOrgContext(page);
    const listV1 = await authedApi(
      page,
      `/api/v1/orgs/${orgId}/approval-definitions`,
      "GET",
    );
    expect(listV1.status).toBe(200);
    const createdV1 = (listV1.body.data as Array<Record<string, unknown>>).find(
      (d) => d.name === definitionName,
    );
    expect(createdV1).toBeTruthy();
    expect(createdV1?.latestVersion).toBe(1);
    expect(createdV1?.invalidatesPrevious).toBe(false);
    expect(createdV1?.requireAllReviewers).toBe(false);
    expect(createdV1?.allowSelfApproval).toBe(false);
    expect(createdV1?.humanApprovalsOverrideAgent).toBe(true);
    expect(createdV1?.phases).toEqual(["be"]);

    // Edit preserves the saved recommendation; publish a different single phase.
    await card.getByTestId("org-approvals-edit").click();
    await expect(page.getByTestId("org-approvals-keep-decisions")).toBeChecked({
      timeout: 15_000,
    });
    await expect(
      page.getByTestId("org-approvals-advanced-content"),
    ).toBeHidden();
    await page.getByTestId("org-approvals-advanced-toggle").click();
    await expect(
      page.getByTestId("org-approvals-human-override"),
    ).toBeChecked();
    await expect(page.getByTestId("org-approvals-quorum")).toHaveValue("any");
    await expect(
      page.getByTestId("org-approvals-self-approval"),
    ).not.toBeChecked();
    await page.getByTestId("org-approvals-quorum").selectOption("all");
    await page.getByTestId("org-approvals-self-approval").check();
    await page.getByTestId("org-approvals-human-override").uncheck();
    await page.getByTestId("org-approvals-advanced-toggle").click();
    await expect(
      page.getByTestId("org-approvals-advanced-content"),
    ).toBeHidden();
    await expect(phase).toBeVisible();
    await expect(phase).toHaveValue("be");
    await phase.selectOption("se");
    await page.getByTestId("org-approvals-invalidate-decisions").check();
    await expect(
      page.getByTestId("org-approvals-invalidate-decisions"),
    ).toBeChecked();
    await page.getByTestId("org-approvals-submit").click();
    await expect(card).toBeVisible({ timeout: 15_000 });

    // API check (latest-version + provenance): the list endpoint returns only
    // the latest version, so these assertions prove the new latest content and
    // that v1-derived provenance survives — durable append-only ROW proof
    // (v1 untouched beside v2) lives in the real-DB api test
    // (test_contract D3).
    const listV2 = await authedApi(
      page,
      `/api/v1/orgs/${orgId}/approval-definitions`,
      "GET",
    );
    expect(listV2.status).toBe(200);
    const updated = (listV2.body.data as Array<Record<string, unknown>>).find(
      (d) => d.name === definitionName,
    );
    expect(updated).toBeTruthy();
    expect(updated?.latestVersion).toBe(2);
    expect(updated?.invalidatesPrevious).toBe(true);
    expect(updated?.requireAllReviewers).toBe(true);
    expect(updated?.allowSelfApproval).toBe(true);
    expect(updated?.humanApprovalsOverrideAgent).toBe(false);
    await expect(card.getByText("All reviewers must approve")).toBeVisible();
    expect(updated?.phases).toEqual(["se"]);
    await expect(card.getByTestId("org-approvals-card-phase-se")).toBeVisible();
    await expect(card.getByTestId("org-approvals-card-phase-be")).toHaveCount(
      0,
    );
    expect((updated?.firstCreatedBy as Record<string, unknown>)?.userId).toBe(
      (createdV1?.firstCreatedBy as Record<string, unknown>)?.userId,
    );
    // firstCreatedAt is derived from the stored v1 row server-side; equality
    // with the pre-edit value shows the v1 provenance source is intact.
    expect(updated?.firstCreatedAt).toBe(createdV1?.firstCreatedAt);
    await card.getByTestId("org-approvals-edit").click();
    await expect(
      page.getByTestId("org-approvals-advanced-content"),
    ).toBeHidden();
    await page.getByTestId("org-approvals-advanced-toggle").click();
    await expect(page.getByTestId("org-approvals-quorum")).toHaveValue("all");
    await expect(page.getByTestId("org-approvals-self-approval")).toBeChecked();
    await expect(
      page.getByTestId("org-approvals-human-override"),
    ).not.toBeChecked();
  });

  // Explicit requested re-review replaces the old self-approval/automatic
  // successor assumption, retaining the durable multi-decision history proof.
  test("requested re-review keeps independent decisions across reload without an automatic successor", async ({
    page,
    browser,
  }) => {
    await openWorkspacePath(page, `/${TEST_ORG}/base`);
    const { orgId } = await readCurrentOrgContext(page);
    const { actor: reviewer, context } = await createReviewActor(
      page,
      browser,
      orgId,
      "History reviewer",
    );
    let fork: { id: string; path: string } | undefined;
    let definitionId: string | undefined;
    const baseWorkspaceId = await resolveWorkspaceId(page, TEST_ORG, "base");
    try {
      const definitionName = `E2E requested sign-off ${Date.now()}`;
      const created = await authedApi(
        page,
        `/api/v1/orgs/${orgId}/approval-definitions`,
        "POST",
        {
          name: definitionName,
          humanUserIds: [reviewer.id],
        },
      );
      expect(created.status).toBe(201);
      definitionId = created.body.data.id;
      const attach = await authedApi(
        page,
        `/api/v1/workspaces/${baseWorkspaceId}/approvals/requirements`,
        "POST",
        { definitionId },
      );
      expect(attach.status).toBe(200);
      fork = await cloneWorkspaceById(page, baseWorkspaceId, {
        namePrefix: "e2e-rereview",
      });
      const pr = await authedApi(
        page,
        `/api/v1/workspaces/${fork.id}/pull-requests`,
        "POST",
        {
          targetWorkspaceId: baseWorkspaceId,
          description: "Verify independently requested review decisions",
        },
      );
      expect(pr.status, JSON.stringify(pr.body)).toBe(200);
      const decisions = [];
      for (const [outcome, comment] of [
        ["Approve", "Initial approval"],
        ["Request changes", "Follow-up requires a correction"],
      ]) {
        const admission = await authedApi(
          page,
          `/api/v1/workspaces/${fork.id}/approvals/requests`,
          "POST",
          {
            definitionId,
            definitionVersion: 1,
            phase: "ie",
            actorUserIds: [reviewer.id],
          },
        );
        expect(admission.status, JSON.stringify(admission.body)).toBe(201);
        expect(admission.body.data.requests).toHaveLength(1);
        decisions.push(admission.body.data.requests[0].requestId);
        await openWorkspacePath(reviewer.page, `${fork.path}?view=approvals`);
        await reviewer.page
          .getByRole("button", { name: "Approve", exact: true })
          .click();
        const dialog = reviewer.page.getByRole("dialog", {
          name: "Submit review",
          exact: true,
        });
        await dialog.getByRole("radio", { name: outcome, exact: true }).check();
        await dialog.getByLabel("Review comment").fill(comment);
        await dialog
          .getByRole("button", { name: "Submit review", exact: true })
          .click();
        await expect(dialog).toHaveCount(0);
        await expect(reviewer.page).toHaveURL(/view=approvals/);
        await reviewer.page.reload();
        await expect(reviewer.page).toHaveURL(/view=approvals/);
        try {
          const current = (
            await approvalTimeline(reviewer.page, definitionName)
          )
            .getByTestId("workspace-approvals-history-card")
            .filter({ hasText: comment });
          await expect(current).toHaveCount(1);
          await expect(current).toContainText(
            outcome === "Approve" ? "Approved" : "Rejected",
          );
        } catch (error) {
          const state = await authedApi(
            reviewer.page,
            `/api/v1/workspaces/${fork.id}/approvals`,
          );
          throw new Error(
            `${String(error)}\nReload evidence: ${JSON.stringify({ url: reviewer.page.url(), status: state.status, body: await reviewer.page.locator("body").innerText() }).slice(0, 5000)}`,
          );
        }
        await expect(
          reviewer.page.getByRole("button", { name: "Approve", exact: true }),
        ).toHaveCount(0);
      }
      expect(new Set(decisions).size).toBe(2);
      await openWorkspacePath(page, `${fork.path}?view=approvals`);
      await page.reload();
      const durable = await authedApi(
        page,
        `/api/v1/workspaces/${fork.id}/approvals`,
      );
      expect(durable.status).toBe(200);
      const requirement = durable.body.data.outgoing.requirements.find(
        (r: { definition: { id: string } }) => r.definition.id === definitionId,
      );
      expect(requirement.state).toBe("rejected");
      expect(approvalRequestStates(requirement)).toEqual([
        "settled",
        "settled",
      ]);
      expect(requirement.decisionLog).toHaveLength(2);
      expect(
        requirement.decisionLog
          .map((d: { requestId: string }) => d.requestId)
          .sort(),
      ).toEqual(decisions.sort());
      const feed = await approvalTimeline(page, definitionName);
      const cards = feed
        .getByTestId("workspace-approvals-history-card")
        .filter({ hasText: definitionName });
      await expect(cards).toHaveCount(2);
      await expect(cards.filter({ hasText: "Approved" })).toHaveCount(1);
      await expect(cards.filter({ hasText: "Rejected" })).toHaveCount(1);
      await expect(cards.filter({ hasText: "Awaiting decision" })).toHaveCount(
        0,
      );
      const retired = await authedApi(
        page,
        `/api/v1/workspaces/${baseWorkspaceId}/approvals/requirements/${definitionId}/retire`,
        "POST",
        {},
      );
      expect(retired.status).toBe(200);
      definitionId = undefined;
      await page.reload();
      const legacy = page.getByTestId("workspace-approvals-legacy");
      await expect(legacy).toBeVisible();
      await expect(legacy).not.toHaveAttribute("open");
      await expect(
        page.getByRole("button", {
          name: `Timeline for ${definitionName}`,
          exact: true,
        }),
      ).toHaveCount(0);
      await legacy.getByTestId("workspace-approvals-legacy-toggle").click();
      const historical = page
        .getByTestId("workspace-approvals-historical-requirement")
        .filter({ hasText: definitionName });
      await expect(historical).toContainText("Not currently required");
      const retained = await approvalTimeline(page, definitionName);
      await expect(
        retained.getByTestId("workspace-approvals-history-card"),
      ).toHaveCount(2);
      await expect(
        retained.getByText("Decision finalized", { exact: true }),
      ).toHaveCount(2);
      await legacy.getByTestId("workspace-approvals-legacy-toggle").click();
      await expect(retained).not.toBeVisible();
      await legacy.getByTestId("workspace-approvals-legacy-toggle").click();
      await expect(retained).toBeVisible();
    } finally {
      if (definitionId)
        await authedApi(
          page,
          `/api/v1/workspaces/${baseWorkspaceId}/approvals/requirements/${definitionId}/retire`,
          "POST",
          {},
        );
      if (fork) await deleteWorkspaceById(page, fork.id);
      await context.close();
    }
  });
});
