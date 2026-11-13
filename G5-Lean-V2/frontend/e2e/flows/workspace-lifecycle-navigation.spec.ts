import { expect, test, type Route } from "@playwright/test";
import {
  authedApiViaRequest,
  createWorkspaceClone,
  deleteWorkspaceById,
  openWorkspacePath,
  type WorkspaceClone,
} from "../helpers";

test.describe("Workspace lifecycle navigation", () => {
  test.describe.configure({ timeout: 240_000 });

  test("starts two workspaces independently through tabs with reversed responses", async ({
    page,
  }) => {
    const workspaces: WorkspaceClone[] = [];
    const releases = new Map<string, () => void>();
    const requests: string[] = [];
    const delivered: string[] = [];
    const results = new Map<string, { status: number; outcome: string }>();
    const handlers: Promise<void>[] = [];
    try {
      for (const suffix of ["a", "b"]) {
        workspaces.push(
          await createWorkspaceClone(page, {
            namePrefix: `lifecycle-nav-${suffix}`,
            start: false,
          }),
        );
      }
      const [a, b] = workspaces;
      // Freeze the VM reads at their real stopped snapshots: pending request
      // state must work even before polling observes a backend transition.
      for (const workspace of workspaces) {
        const snapshot = await authedApiViaRequest(
          page,
          `/api/v1/workspaces/${workspace.id}/vm`,
        );
        expect(snapshot.status).toBe(200);
        expect(snapshot.body.data.status).toBe("stopped");
        await page.route(`**/api/v1/workspaces/${workspace.id}/vm`, (route) =>
          route.fulfill({ json: snapshot.body }),
        );
        await page.route(
          `**/api/v1/workspaces/${workspace.id}/resume`,
          (route: Route) => {
            const pending = (async () => {
              requests.push(workspace.id);
              const held = new Promise<void>((resolve) =>
                releases.set(workspace.id, resolve),
              );
              // Both requests reach the real lifecycle service while the first
              // browser request remains pending; only response delivery is held.
              const response = await route.fetch({ timeout: 90_000 });
              const body = await response.json();
              results.set(workspace.id, {
                status: response.status(),
                outcome: body.data?.outcome,
              });
              await held;
              await route.fulfill({ response });
              delivered.push(workspace.id);
            })();
            handlers.push(pending);
            return pending;
          },
        );
      }
      // Register both tabs, then navigate within the reused WorkspacePage.
      await openWorkspacePath(page, b.path);
      await openWorkspacePath(page, a.path);
      const tabs = page.getByTestId("workspace-tab-strip");
      const select = async (workspace: WorkspaceClone) => {
        const tab = tabs.getByRole("tab", {
          name: workspace.name,
          exact: true,
        });
        await tab.click();
        await expect(tab).toHaveAttribute("aria-selected", "true");
        await expect(page.getByTestId("workspace-vm-start")).toHaveAttribute(
          "title",
          "VM: Stopped",
        );
      };
      const start = page.getByTestId("workspace-vm-start");
      const banner = page.getByTestId("workspace-readonly-banner-start");
      await start.click();
      await expect.poll(() => requests).toEqual([a.id]);
      await select(b);
      await expect(start).toBeEnabled();
      await expect(banner).toBeEnabled();
      await banner.click();
      await expect.poll(() => requests).toEqual([a.id, b.id]);
      await select(a);
      await expect(start).toBeDisabled();
      await expect(banner).toBeDisabled();
      await select(b);
      await expect(start).toBeDisabled();
      releases.get(b.id)!();
      await expect.poll(() => delivered, { timeout: 90_000 }).toEqual([b.id]);
      await expect(start).toBeEnabled();
      await select(a);
      await expect(start).toBeDisabled();
      releases.get(a.id)!();
      await expect
        .poll(() => delivered, { timeout: 90_000 })
        .toEqual([b.id, a.id]);
      await expect(start).toBeEnabled();
      expect(requests).toEqual([a.id, b.id]);
      for (const result of results.values()) {
        expect([200, 202]).toContain(result.status);
        expect(["attached", "starting", "queued_for_capacity"]).toContain(
          result.outcome,
        );
      }
    } finally {
      for (const release of releases.values()) release();
      await Promise.allSettled(handlers);
      await page.unrouteAll({ behavior: "wait" });
      for (const workspace of workspaces)
        await deleteWorkspaceById(page, workspace.id).catch(() => {});
    }
  });
});
