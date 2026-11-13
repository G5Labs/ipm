import { expect, test, type Page } from "@playwright/test";

import {
  assertActiveSubject,
  assertSingleRefreshCookie,
  authedApi,
  cloneWorkspaceById,
  deleteWorkspaceById,
  navigateToWorkspace,
  readCurrentOrgContext,
  recordExpectedSubject,
  resolveWorkspaceId,
  TEST_ORG,
  TEST_WORKSPACE,
  USING_OAUTH_STUB,
} from "../helpers";
import {
  loginWithOAuthStub,
  resetOAuthStubIdentity,
  SEEDED_FORK_MEMBER_IDENTITY,
} from "../oauth";

/**
 * Become a deterministic non-owner member of `orgId` (identity contract Z0).
 *
 * The seeded owner invites the fixed fork-member identity as `member`
 * (an existing user gets the membership at once; a first-time user gets an
 * invitation that login accepts), then the browser logs in as that identity
 * through the OAuth stub. The API's callback issues the single refresh
 * cookie, so no credential is hand-added and the subject is pinned and
 * asserted before anything owner-scoped runs. Test-token provisioning cannot
 * play this part: it provisions Admin, while this fixture exercises Member
 * permissions without changing the shared administrator's membership.
 */
async function useNonOwnerMemberIdentity(page: Page, orgId: string) {
  const invited = await authedApi(page, `/api/v1/orgs/${orgId}/members`, "POST", {
    email: SEEDED_FORK_MEMBER_IDENTITY.email,
    role: "member",
  });
  expect(
    [200, 201],
    `invite fork member: ${JSON.stringify(invited.body)}`,
  ).toContain(invited.status);

  // Leave the seeded owner's session behind: with its cookie and bearer
  // still in the context, /login redirects straight into the app and the
  // stub's "Continue with GitHub" link never appears. The context then holds
  // exactly one identity, the member's, issued by the OAuth callback.
  await page.context().clearCookies();
  await page.evaluate(() => localStorage.clear());
  await loginWithOAuthStub(page, SEEDED_FORK_MEMBER_IDENTITY, {
    targetOrgSlug: TEST_ORG,
  });
  const userId = await recordExpectedSubject(page);
  await assertSingleRefreshCookie(page.context(), "fork member login");
  await assertActiveSubject(page, "fork member login");

  const currentUser = await authedApi(page, "/api/v1/users/me");
  expect(currentUser.status, JSON.stringify(currentUser.body)).toBe(200);
  const user = currentUser.body?.data ?? currentUser.body ?? {};
  expect(String(user.email ?? "").toLowerCase()).toBe(
    SEEDED_FORK_MEMBER_IDENTITY.email.toLowerCase(),
  );
  const membership = (Array.isArray(user.orgs) ? user.orgs : []).find(
    (org: Record<string, unknown>) => org.orgId === orgId,
  );
  expect(membership?.role).toBe("member");

  return { userId };
}

/** Forking goes through the strip's "+" and the fork picker's row. */
async function clickForkInPicker(page: Page, workspaceName: string) {
  const add = page.getByTestId("workspace-tab-add");
  await expect(add).toBeVisible({ timeout: 30_000 });
  await add.click();
  await expect(page.getByTestId("fork-picker")).toBeVisible({
    timeout: 30_000,
  });
  await page.getByRole("button", { name: `Fork ${workspaceName}` }).click();
}

async function openForkDialog(page: Page) {
  await navigateToWorkspace(page, TEST_ORG, TEST_WORKSPACE);
  await clickForkInPicker(page, TEST_WORKSPACE);
  const dialog = page.getByRole("dialog");
  await expect(dialog).toBeVisible();
  return dialog;
}

test.describe("Fork workspace", () => {
  test("lets a non-owner member fork Base into a child they own", async ({
    page,
  }) => {
    test.skip(
      !USING_OAUTH_STUB,
      "Needs the OAuth stub: test-token provisioning grants owner, a real provider cannot mint the fixed member, and a deterministic non-owner member logs in through the stub (Z0)",
    );
    const { orgId } = await readCurrentOrgContext(page);
    const { userId } = await useNonOwnerMemberIdentity(page, orgId);
    const baseId = await resolveWorkspaceId(page, TEST_ORG, "Base");
    const name = `member-base-fork-${Date.now()}`;
    let childId = "";

    try {
      await navigateToWorkspace(page, TEST_ORG, "Base");
      await clickForkInPicker(page, "Base");

      const dialog = page.getByRole("dialog");
      await expect(dialog).toBeVisible();
      await dialog.locator("#clone-name").fill(name);
      const responsePromise = page.waitForResponse(
        (response) =>
          new URL(response.url()).pathname ===
            `/api/v1/workspaces/${baseId}/clone` &&
          response.request().method() === "POST",
      );
      await dialog.getByTestId("clone-submit-button").click();
      const response = await responsePromise;
      const responseText = await response.text();
      expect(response.status(), responseText).toBe(201);
      const responseBody = JSON.parse(responseText);
      const workspace = responseBody.data?.workspace ?? responseBody.workspace;
      childId = String(workspace?.id ?? "");
      expect(childId).toBeTruthy();

      await expect(page).toHaveURL(
        new RegExp(`/${TEST_ORG}/${encodeURIComponent(name)}(?:[/?#]|$)`),
      );
      const child = await authedApi(page, `/api/v1/workspaces/${childId}`);
      expect(child.status, JSON.stringify(child.body)).toBe(200);
      const childData = child.body?.data ?? child.body ?? {};
      expect(childData.ownerUserId ?? childData.owner_user_id).toBe(userId);
    } finally {
      if (childId) {
        await deleteWorkspaceById(page, childId);
      }
      await resetOAuthStubIdentity(page);
    }
  });

  test("uses one exact destination name for every repository", async ({
    page,
  }) => {
    const dialog = await openForkDialog(page);
    const name = "feature/exact-destination";
    await dialog.locator("#clone-name").fill(name);

    // The per-repository branch names live in the Advanced section.
    await dialog.getByTestId("clone-advanced-toggle").click();
    const rows = dialog.getByTestId("clone-member-mapping-row");
    expect(await rows.count()).toBeGreaterThan(0);
    for (let index = 0; index < (await rows.count()); index += 1) {
      await expect(rows.nth(index)).toContainText(name);
    }
    await expect(
      dialog.getByText(/every repository uses the same destination name/i),
    ).toBeVisible();
    await expect(dialog.getByText(/branch name overrides/i)).toHaveCount(0);
  });

  test("submits only the shared name and keeps the dialog open on collision", async ({
    page,
  }) => {
    const sourceId = await resolveWorkspaceId(page, TEST_ORG, TEST_WORKSPACE);
    const name = `fork-collision-${Date.now()}`;
    const seeded = await cloneWorkspaceById(page, sourceId, { name });
    const seededId = seeded.id;

    try {
      const dialog = await openForkDialog(page);
      const input = dialog.locator("#clone-name");
      await input.fill(name);
      const requestPromise = page.waitForRequest(
        (request) =>
          new URL(request.url()).pathname ===
            `/api/v1/workspaces/${sourceId}/clone` &&
          request.method() === "POST",
      );
      const responsePromise = page.waitForResponse(
        (response) =>
          new URL(response.url()).pathname ===
            `/api/v1/workspaces/${sourceId}/clone` &&
          response.request().method() === "POST",
      );
      await dialog.getByTestId("clone-submit-button").click();
      const request = await requestPromise;
      const response = await responsePromise;

      expect(request.postDataJSON()?.name).toBe(name);
      expect(request.postDataJSON()?.branchOverrides).toBeUndefined();
      // The fork service validates the shared name against every member
      // repository's branches and reports a taken name as a 422 on `name`.
      expect(response.status(), await response.text()).toBe(422);
      await expect(dialog).toBeVisible();
      await expect(input).toHaveValue(name);
      await expect(
        dialog.getByText(/unavailable|already exists|conflict|duplicate|failed/i),
      ).toBeVisible();
    } finally {
      await deleteWorkspaceById(page, seededId);
    }
  });

  test("requires a name and supports cancel, close, and Escape", async ({
    page,
  }) => {
    let dialog = await openForkDialog(page);
    await expect(dialog.locator("#clone-name")).toBeFocused();
    await expect(dialog.getByTestId("clone-submit-button")).toBeDisabled();
    await dialog.getByTestId("clone-cancel-button").click();
    await expect(dialog).not.toBeVisible();

    dialog = await openForkDialog(page);
    await dialog.getByTestId("clone-dialog-close").click();
    await expect(dialog).not.toBeVisible();

    dialog = await openForkDialog(page);
    await page.keyboard.press("Escape");
    await expect(dialog).not.toBeVisible();
  });
});
