import { test, expect } from "@playwright/test";
import type {
  CommitHistoryEntry,
  CommitHistoryPage,
} from "../../src/lib/types";
import {
  activateSidebarSection,
  authedApiViaRequest as api,
  createWorkspaceClone,
  deleteWorkspaceById,
  openWorkspacePath,
} from "../helpers";

// The checklist replaces the History picker; immutable historical links and
// bounded history reads remain supported for existing PR/comment consumers.
test("repository checklist replaces the picker while immutable history links still work", async ({
  page,
}) => {
  const workspace = await createWorkspaceClone(page, {
    namePrefix: "source-control-history",
  });
  try {
    const branches = await api(
      page,
      `/api/v1/workspaces/${workspace.id}/branches`,
    );
    expect(branches.status).toBe(200);
    const repos = branches.body.data as { repoId: string; repoName: string }[];
    expect(repos.length).toBeGreaterThanOrEqual(2);
    const repo = repos.find((r) => r.repoName === "code") ?? repos[0];
    let cursor: string | null = null;
    const loaded: CommitHistoryEntry[] = [];
    for (let attempt = 0; attempt < 40; attempt++) {
      const result = await api(
        page,
        `/api/v1/workspaces/${workspace.id}/repos/${repo.repoId}/commits/history?limit=10${cursor ? `&cursor=${cursor}` : ""}`,
      );
      expect(result.status, JSON.stringify(result.body)).toBe(200);
      const history = result.body.data as CommitHistoryPage;
      expect(history.commits.length).toBeLessThanOrEqual(10);
      loaded.push(...history.commits);
      cursor = history.nextCursor;
      if (!cursor) break;
    }
    expect(cursor).toBeNull();
    expect(loaded.length).toBeGreaterThan(10);
    expect(new Set(loaded.map((c) => c.sha)).size).toBe(loaded.length);
    for (const [commit, text] of [
      [
        loaded.find(
          (c) => c.summary === "Merge source control history fixture",
        )!,
        "Merge commit · first-parent diff against",
      ],
      [
        loaded.find((c) => c.parentCount === 0)!,
        "Root commit · compared with empty tree",
      ],
    ] as const) {
      expect(commit).toBeTruthy();
      await openWorkspacePath(
        page,
        `${workspace.path}/commit/${repo.repoId}/${commit.sha}`,
      );
      await expect(
        page.getByTestId("workspace-commit-diff-view"),
      ).toContainText(text);
      await expect(page.getByTestId("workspace-commit-diff-view")).toHaveCount(
        1,
      );
    }
    await activateSidebarSection(page, "sidebar-source-control-tab");
    const menu = page.getByTestId("source-control-menu");
    await expect(menu).toBeVisible();
    for (const r of repos)
      await expect(
        menu.getByLabel(`All changes in ${r.repoName}`, { exact: true }),
      ).toBeVisible();
    await expect(
      page.getByTestId("source-control-history-repository"),
    ).toHaveCount(0);
    await expect(page.getByTestId("source-control-tab-history")).toHaveCount(0);
  } finally {
    await deleteWorkspaceById(page, workspace.id);
  }
});
