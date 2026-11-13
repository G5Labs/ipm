import { expect, test, type Page } from "@playwright/test";
import {
  authedApi,
  createWorkspaceClone,
  expandPrimaryIssue,
  deleteWorkspaceById,
  openWorkspacePath,
  readActiveSubject,
  readCurrentOrgContext,
  readWorkspaceVm,
  patchStoredUserSettings,
  TEST_ORG,
  USING_OAUTH_STUB,
  waitForWorkspaceLoad,
} from "../helpers";
import { loginWithOAuthStub, SEEDED_FORK_MEMBER_IDENTITY } from "../oauth";

const FULL_TASK =
  "Task summary for workspace readers.\n\nDetailed acceptance: owners and non-owners can read the entire task without changing ownership or starting stopped compute.";

async function useAlignment(page: Page, path: string) {
  // Fresh role contexts miss auth.setup's full-sidebar preference. These
  // review flows click its stacked sections; pin the same layout for every actor.
  await patchStoredUserSettings(page, {
    showTabBar: true,
    sidebarDisplayMode: "expanded",
  });
  await openWorkspacePath(page, `${path}?view=activity`);
}

async function readTaskAndApprovals(page: Page, phase: string) {
  await expect(page.getByTestId("phase-chevrons")).toHaveAttribute(
    "data-phase",
    phase,
  );
  await expect(
    page.getByRole("heading", { name: "Activity", exact: true }),
  ).toBeVisible();
  await expandPrimaryIssue(page);
  await page.getByTestId("workspace-read-task-description").click();
  const dialog = page.getByRole("dialog", { name: "Workspace task" });
  const description = dialog.getByRole("region", {
    name: "Full task description",
  });
  await expect(description).toContainText(
    "Detailed acceptance: owners and non-owners",
  );
  await expect(dialog.getByRole("button")).toHaveCount(1);
  await description.focus();
  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
  await expect(
    page.getByTestId("workspace-read-task-description"),
  ).toBeFocused();
  await page.getByTestId("ribbon-workspace-approvals").click();
  await expect(
    page.getByRole("heading", { name: /^Approvals Required to merge into / }),
  ).toBeVisible();
  await expect(
    page.getByTestId("workspace-approvals-phase-filter"),
  ).toHaveCount(0);
  await expect(page.getByTestId("ribbon-action-approvals")).toHaveCount(0);
  await expect(page.getByTestId("ribbon-action-approval")).toHaveCount(0);
  // The ribbon's Overview control opens the landing (g5.workbench.landing).
  await page.getByTestId("ribbon-overview").click();
  await expect(page.getByTestId("workspace-landing")).toBeVisible();
}

// Production navigation/task-description coverage only. Formal review journeys
// are separately inventoried in docs/review-flow.md; this test must not stand in
// for PR/request/decision persistence or pushed-vector authorization coverage.
test.describe("Workspace landing for owners and non-owners", () => {
  test.describe.configure({ timeout: 360_000 });
  test.skip(
    !USING_OAUTH_STUB,
    "Requires separately authenticated owner and member identities",
  );
  for (const running of [false, true]) {
    test(`${running ? "running" : "stopped"} workspace: both roles read all phases without giving the reader owner controls`, async ({
      page,
      browser,
    }) => {
      const { orgId } = await readCurrentOrgContext(page);
      const ownerId = await readActiveSubject(page);
      const invited = await authedApi(
        page,
        `/api/v1/orgs/${orgId}/members`,
        "POST",
        {
          email: SEEDED_FORK_MEMBER_IDENTITY.email,
          role: "member",
        },
      );
      expect([200, 201], JSON.stringify(invited.body)).toContain(
        invited.status,
      );
      const workspace = await createWorkspaceClone(page, {
        namePrefix: `landing-roles-${running ? "running" : "stopped"}`,
        description: FULL_TASK,
        start: running,
      });
      const memberContext = await browser.newContext({
        storageState: { cookies: [], origins: [] },
      });
      try {
        const memberPage = await memberContext.newPage();
        await loginWithOAuthStub(memberPage, SEEDED_FORK_MEMBER_IDENTITY, {
          targetOrgSlug: TEST_ORG,
        });
        expect(await readActiveSubject(memberPage)).not.toBe(ownerId);
        await useAlignment(page, workspace.path);
        await useAlignment(memberPage, workspace.path);
        const before = await readWorkspaceVm(page, workspace.id);
        expect(before.status).toBe(running ? "running" : "stopped");
        for (const phase of ["be", "se", "ie"] as const) {
          const ownerPhase = page.getByTestId(`phase-chevron-${phase}`);
          await expect(ownerPhase).not.toHaveAttribute("aria-disabled", "true");
          await ownerPhase.click();
          await expect(page.getByTestId("phase-chevrons")).toHaveAttribute(
            "data-phase",
            phase,
          );
          await readTaskAndApprovals(page, phase);
          await memberPage.reload({ waitUntil: "domcontentloaded" });
          await waitForWorkspaceLoad(memberPage);
          await readTaskAndApprovals(memberPage, phase);
          const nextPhase = phase === "be" ? "se" : "be";
          await expect(
            memberPage.getByTestId(`phase-chevron-${nextPhase}`),
          ).toHaveAttribute("aria-disabled", "true");
          const denied = await authedApi(
            memberPage,
            `/api/v1/workspaces/${workspace.id}`,
            "PATCH",
            { phase: nextPhase },
          );
          expect(denied.status, JSON.stringify(denied.body)).toBe(403);
          const actual = await authedApi(
            page,
            `/api/v1/workspaces/${workspace.id}`,
          );
          expect(actual.status).toBe(200);
          expect(actual.body.data.phase).toBe(phase);
          expect(actual.body.data.ownerUserId).toBe(ownerId);
        }
        const after = await readWorkspaceVm(page, workspace.id);
        expect(after.status).toBe(before.status);
        expect(after.vmId).toBe(before.vmId);
        expect(after.epoch).toBe(before.epoch);
      } finally {
        await memberContext.close();
        await deleteWorkspaceById(page, workspace.id);
      }
    });
  }
});
