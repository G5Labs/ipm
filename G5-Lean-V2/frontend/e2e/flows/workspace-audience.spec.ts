import { expect, test } from "@playwright/test";
import {
  APP_BASE_URL,
  TEST_ORG,
  USING_OAUTH_STUB,
  createWorkspaceClone,
  deleteWorkspaceById,
  openWorkspacePath,
  readCurrentOrgContext,
} from "../helpers";
import { api, createReviewActor } from "../workspace-review";

// The fixed audience is the same in discovery, direct API reads and the browser.
test("members open ordinary workspaces without invitations while personal work stays private", async ({
  page,
  browser,
}) => {
  test.skip(!USING_OAUTH_STUB, "Requires a distinct organization member");
  const { orgId } = await readCurrentOrgContext(page);
  const me = await api(page, `/api/v1/orgs/${orgId}/me`);
  expect(me.status, JSON.stringify(me.body)).toBe(200);
  const personal = me.body.data;
  const workspace = await createWorkspaceClone(page, {
    namePrefix: "fixed-audience",
  });
  const { actor, context } = await createReviewActor(
    page,
    browser,
    orgId,
    "Audience reader",
  );
  try {
    const detail = await api(actor.page, `/api/v1/workspaces/${workspace.id}`);
    expect(detail.status, JSON.stringify(detail.body)).toBe(200);
    expect(detail.body.data).not.toHaveProperty("visibility");
    const listing = await api(actor.page, `/api/v1/orgs/${orgId}/workspaces`);
    expect(listing.status, JSON.stringify(listing.body)).toBe(200);
    const ids = listing.body.data.map((row: { id: string }) => row.id);
    expect(ids).toContain(workspace.id);
    expect(ids).not.toContain(personal.workspaceId);
    await openWorkspacePath(actor.page, workspace.path);
    await expect(actor.page.getByTestId("workbench-header")).toBeVisible();
    await expect(
      actor.page.getByTestId("workspace-readonly-banner"),
    ).toBeVisible();

    for (const suffix of ["", "/sessions", "/notifications"]) {
      const refused = await api(
        actor.page,
        `/api/v1/workspaces/${personal.workspaceId}${suffix}`,
      );
      expect([403, 404], JSON.stringify(refused.body)).toContain(
        refused.status,
      );
    }
    await actor.page.goto(`${APP_BASE_URL}/${TEST_ORG}/${personal.name}`, {
      waitUntil: "domcontentloaded",
    });
    await expect(
      actor.page.getByTestId("workspace-not-found-message"),
    ).toBeVisible();
    const stillPrivate = await api(
      page,
      `/api/v1/workspaces/${personal.workspaceId}`,
    );
    expect(stillPrivate.status, JSON.stringify(stillPrivate.body)).toBe(200);
  } finally {
    await context.close();
    await deleteWorkspaceById(page, workspace.id);
    const removed = await api(
      page,
      `/api/v1/orgs/${orgId}/members/${actor.id}`,
      "DELETE",
    );
    expect([200, 204], JSON.stringify(removed.body)).toContain(removed.status);
  }
});
