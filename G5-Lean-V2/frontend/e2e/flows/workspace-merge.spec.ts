import { test, expect } from "@playwright/test";
import {
  authedApi,
  cloneWorkspaceById,
  createWorkspaceClone,
  deleteWorkspaceById,
  navigateToWorkspace,
  TEST_ORG,
  TEST_WORKSPACE,
} from "../helpers";

test.describe("Workspace merge", () => {
  test.beforeEach(async ({ page }) => {
    await navigateToWorkspace(page, TEST_ORG, TEST_WORKSPACE);
  });
  test("clone workspace can be merged back into parent", async ({ page }) => {
    const parent = await createWorkspaceClone(page, {
      namePrefix: "workspace-merge-parent",
    });
    let cloneId = "";
    try {
      // Both source and destination are owned by this test user. Merge
      // re-authorizes the destination owner at execution time.
      const clone = await cloneWorkspaceById(page, parent.id, {
        name: `workspace-merge-clone-${Date.now()}`,
      });
      cloneId = clone.id;

      const mergeResult = await authedApi(
        page,
        `/api/v1/workspaces/${cloneId}/merge`,
        "POST",
        {},
      );

      expect(mergeResult.status).toBe(200);
      const mergeData = mergeResult.body.data ?? mergeResult.body;
      expect(
        mergeData.branchesMerged ?? mergeData.branches_merged,
      ).toBeDefined();
      expect(
        mergeData.branchesFailed ?? mergeData.branches_failed,
      ).toBeDefined();
    } finally {
      await deleteWorkspaceById(page, cloneId);
      await deleteWorkspaceById(page, parent.id);
    }
  });
});
