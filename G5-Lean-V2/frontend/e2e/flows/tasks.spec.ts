import { expect, test } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { Pool } from "pg";
import {
  activateSidebarSection,
  openWorkspacePath,
  resolveWorkspaceId,
  showWorkspaceTabs,
  TEST_ORG,
  TEST_WORKSPACE,
} from "../helpers";

const databaseUrl =
  process.env.G5_E2E_DATABASE_URL ??
  process.env.E2E_DATABASE_URL ??
  process.env.DATABASE_URL ??
  "postgresql://g5:g5_test@localhost:5433/g5_test";

test("task history pages and an older task opens directly before its page is loaded", async ({
  page,
  browserName,
}) => {
  test.skip(
    browserName !== "chromium",
    "Task history database fixture runs once.",
  );
  await showWorkspaceTabs(page);
  const workspaceId = await resolveWorkspaceId(page, TEST_ORG, TEST_WORKSPACE);
  const pool = new Pool({ connectionString: databaseUrl });
  const ids = Array.from({ length: 55 }, () => randomUUID());
  const prefix = `Task history ${randomUUID().slice(0, 8)}`;
  const oldest = ids[54]!;
  try {
    await pool.query(
      `INSERT INTO tasks (id, org_id, workspace_id, name, kind, status, created_at, ended_at, created_by)
       SELECT entry.id, workspace.org_id, workspace.id, $3 || ' ' || entry.ordinal,
              'command', 'succeeded', now() + interval '1 day' - entry.ordinal * interval '1 second', now(),
              COALESCE(workspace.owner_user_id, workspace.created_by)
       FROM workspaces workspace CROSS JOIN unnest($2::uuid[]) WITH ORDINALITY AS entry(id, ordinal)
       WHERE workspace.id = $1`,
      [workspaceId, ids, prefix],
    );
    // No list page contains this task at initial load: resolution must use its ID.
    await openWorkspacePath(
      page,
      `/${TEST_ORG}/${TEST_WORKSPACE}/task/${oldest}`,
    );
    await expect(
      page.getByRole("tab", { name: "Workspace changes", exact: true }),
    ).toBeVisible();
    await expect(page).toHaveURL(new RegExp(`/task/${oldest}$`));
    await activateSidebarSection(page, "sidebar-tasks-tab");
    const section = page.getByTestId("sidebar-tasks-tab");
    await expect(
      section.getByText(`${prefix} 1`, { exact: true }),
    ).toBeVisible();
    await expect(
      section.getByText(`${prefix} 55`, { exact: true }),
    ).toHaveCount(0);
    await page.getByTestId("sidebar-tasks-load-more").click();
    const older = section.getByText(`${prefix} 55`, { exact: true });
    await expect(older).toBeVisible();
    await older.click();
    await expect(page).toHaveURL(new RegExp(`/task/${oldest}$`));
    await expect(section.getByText(`${prefix} 1`, { exact: true })).toHaveCount(
      1,
    );
  } finally {
    await pool.query(
      "DELETE FROM tasks WHERE workspace_id = $1 AND id = ANY($2::uuid[])",
      [workspaceId, ids],
    );
    await pool.end();
  }
});
