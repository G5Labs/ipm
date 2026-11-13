import { expect, test } from "@playwright/test";
import { openWorkspacePath, TEST_ORG } from "../helpers";

test.use({ hasTouch: true });

test("workspace names stay in a fixed-width column after polling", async ({
  page,
  browserName,
}) => {
  // Safari on macOS uses Option-Tab to include buttons in keyboard navigation.
  const nextControl =
    browserName === "webkit" && process.platform === "darwin"
      ? "Alt+Tab"
      : "Tab";
  let workspaceName = "zcl-test";
  let rowCount = 1;
  // Exercise the real Workers tab and CSS without provisioning a cloud host.
  await page.route("**/api/v1/orgs/*/worker-hosts?*", (route) =>
    route.fulfill({
      json: {
        data: {
          hosts: [
            {
              hostId: "45a7d549-1234-4321-8123-123456789abc",
              status: "ready",
              region: "us-central1",
              instanceType: "c3-standard-22",
              createdAt: "2026-09-24T00:00:00Z",
            },
          ],
          total: 1,
          promotedImageTracks: [],
        },
      },
    }),
  );
  await page.route("**/api/v1/orgs/*/worker-hosts/workspaces", (route) =>
    route.fulfill({
      json: {
        data: Array.from({ length: rowCount }, (_, index) => ({
          workspaceId: `workspace-layout-test-${index}`,
          workspaceName:
            index === 0
              ? workspaceName
              : `middle-workspace-${String(index).padStart(2, "0")}`,
          hostId: "45a7d549-1234-4321-8123-123456789abc",
          status: "running",
          ownerUserId: "layout-owner",
          ownerDisplayName: "Workspace Owner",
          ownerEmail: "owner@example.com",
          startedAt: null,
          lastActivityAt: null,
        })),
      },
    }),
  );
  await page.setViewportSize({ width: 1600, height: 1000 });
  await openWorkspacePath(page, `/${TEST_ORG}/base?view=workers`);
  const panel = page.getByTestId("worker-hosts-page-content");
  await expect(
    panel.getByRole("heading", { name: "Hosts", exact: true }),
  ).toBeVisible();
  await expect(
    panel.getByText("Hosts run the workspace VMs for this organization.", {
      exact: true,
    }),
  ).toBeVisible();
  await expect(
    panel.getByText("Workspaces with non-stopped VMs.", { exact: true }),
  ).toBeVisible();
  const table = panel.getByRole("table", {
    name: "Workspaces with non-stopped VMs",
  });
  const nameColumn = table.getByRole("columnheader", {
    name: "Name",
    exact: true,
  });
  const shortName = table.getByRole("link", {
    name: workspaceName,
    exact: true,
  });
  await expect(shortName).toBeVisible();
  const initialWidth = (await table.boundingBox())!.width;
  expect((await nameColumn.boundingBox())!.width).toBe(240);

  for (const nextName of [
    "review-test-2-review-audit-3ea2df2d",
    "workspace".repeat(40),
  ]) {
    workspaceName = nextName;
    const name = table.getByRole("link", {
      name: workspaceName,
      exact: true,
    });
    await expect(name).toBeVisible({ timeout: 15_000 });
    expect((await nameColumn.boundingBox())!.width).toBe(240);
    expect((await table.boundingBox())!.width).toBe(initialWidth);
    expect(
      await name.evaluate((element) => {
        const style = getComputedStyle(element);
        return (
          element.scrollWidth > element.clientWidth &&
          style.textOverflow === "ellipsis" &&
          style.whiteSpace === "nowrap"
        );
      }),
    ).toBe(true);
    // The link is the affordance: no disclosure on hover or focus.
    await name.hover();
    await expect(page.getByRole("tooltip")).toHaveCount(0);
    await panel
      .getByRole("button", { name: "Remove host", exact: true })
      .focus();
    await page.keyboard.press(nextControl);
    await expect(name).toBeFocused();
    await expect(page.getByRole("tooltip")).toHaveCount(0);
    await page.mouse.move(0, 0);
  }

  // Stay above the shell's 1024px mobile breakpoint, where center tabs unmount.
  await page.setViewportSize({ width: 1100, height: 800 });
  expect((await nameColumn.boundingBox())!.width).toBe(240);
  const overflow = await table.evaluate((element) => {
    const container = element.parentElement!;
    return {
      client: container.clientWidth,
      scroll: container.scrollWidth,
      mode: getComputedStyle(container).overflowX,
    };
  });
  expect(overflow.mode).toBe("auto");
  expect(overflow.scroll).toBeGreaterThan(overflow.client);

  rowCount = 30;
  workspaceName = "zz-focus-below-the-fold";
  await expect(table.getByRole("row")).toHaveCount(31, { timeout: 15_000 });
  const belowFold = table.getByRole("link", {
    name: workspaceName,
    exact: true,
  });
  // Programmatic focus below the fold does not scroll the center panel;
  // the link is focused all the same and still shows no disclosure.
  await belowFold.focus();
  await expect(belowFold).toBeFocused();
  await expect(page.getByRole("tooltip")).toHaveCount(0);
  await belowFold.press("Tab");

  rowCount = 2;
  workspaceName = "aaa-focused-workspace";
  await expect(table.getByRole("row")).toHaveCount(3, { timeout: 15_000 });
  const focused = table.getByRole("link", {
    name: workspaceName,
    exact: true,
  });
  await focused.focus();
  workspaceName = "zzz-renamed-focused-workspace";
  const renamed = table.getByRole("link", {
    name: workspaceName,
    exact: true,
  });
  await expect(renamed).toBeVisible({ timeout: 15_000 });
  // Polling renames the row in place; focus stays on the same link.
  await expect(renamed).toBeFocused();
  await expect(page.getByRole("tooltip")).toHaveCount(0);

  // The name is a link: following it opens that workspace.
  await expect(renamed).toHaveAttribute(
    "href",
    `/${TEST_ORG}/${encodeURIComponent(workspaceName)}`,
  );
  await renamed.tap();
  await expect(page).toHaveURL(
    new RegExp(`/${TEST_ORG}/${encodeURIComponent(workspaceName)}(\\?|$)`),
  );
});
