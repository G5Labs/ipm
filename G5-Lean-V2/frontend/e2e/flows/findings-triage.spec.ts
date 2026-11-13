import { randomUUID } from "node:crypto";
import { expect, test } from "@playwright/test";
import {
  authedApiViaRequest,
  createWorkspaceClone,
  deleteWorkspaceById,
  expandOntologyTree,
  openWorkspacePath,
  readCurrentOrgContext,
  runWorkspaceVmAction,
  waitForWorkspaceVmStatus,
} from "../helpers";

test.describe.configure({ timeout: 600_000 });

// Real HTTP, a running workspace and the org's native tracker: a findings
// file opens from Core Ontology, and each decision changes one issue and the
// entry's decision line in the live file (g5.documents.findings). No routes
// are intercepted.
test("findings decisions file, retitle, close and reopen one issue and record it in the file", async ({
  page,
}) => {
  const { orgId } = await readCurrentOrgContext(page);
  const workspace = await createWorkspaceClone(page, {
    namePrefix: "findings-triage",
  });
  const path = ".g5/TLA_verification/bugs_issues.md";
  const token = randomUUID().slice(0, 8);
  const title = `Receipts exceed the ceiling ${token}`;
  try {
    await runWorkspaceVmAction(page, workspace.id, "resume");
    const vm = await waitForWorkspaceVmStatus(page, workspace.id, "running");
    const detail = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${workspace.id}`,
    );
    const data = detail.body?.data ?? detail.body;
    const repoId = data.branches.find(
      (b: { repoId: string }) => b.repoId !== data.docsRepoId,
    )?.repoId;
    expect(repoId).toBeTruthy();
    const content =
      "# Bugs and issues\n\n## Actionable\n\n" +
      `<a id="a-${token}"></a>\n### ${title}\n\nSeverity: High · Decision: Open\n\nTwo lines for one item each pass the ceiling.\n\n` +
      `<a id="b-${token}"></a>\n### Drafts keep a stale order ${token}\n\nSeverity: Low · Decision: Open\n\nSubmit does not recheck.\n\n` +
      "## Other entries\n\n" +
      `<a id="q-${token}"></a>\n### Recall after approval is undecided ${token}\n\nSeverity: Medium\n\nAn open question.\n`;
    const write = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${workspace.id}/files/${path}?repoId=${repoId}`,
      "PUT",
      { content, encoding: "utf-8", expectedHash: null, expectedVmId: vm.vmId },
    );
    expect(write.status, JSON.stringify(write.body)).toBe(200);
    const fileNow = async () => {
      const read = await authedApiViaRequest(
        page,
        `/api/v1/workspaces/${workspace.id}/files/${path}?repoId=${repoId}`,
      );
      return String((read.body?.data ?? read.body).content);
    };
    const issueNow = async (key: string) => {
      const read = await authedApiViaRequest(
        page,
        `/api/v1/orgs/${orgId}/issues/resolve?ref=${encodeURIComponent(key)}`,
      );
      expect(read.status, JSON.stringify(read.body)).toBe(200);
      return (read.body?.data ?? read.body) as {
        title: string;
        state: string;
      };
    };

    await openWorkspacePath(page, workspace.path);
    const tree = await expandOntologyTree(page);
    await tree.getByRole("treeitem", { name: "Bugs and issues" }).click();
    const editor = page.getByTestId("file-view");
    const preview = editor.getByTestId("file-view-preview");
    const row = (id: string) =>
      preview.locator(`[data-testid='finding-row'][data-entry-id='${id}']`);
    const a = row(`a-${token}`);
    await expect(a.getByTestId("finding-severity")).toHaveText("High");
    await expect(row(`q-${token}`).getByTestId("finding-action")).toHaveCount(
      0,
    );

    // Irrelevant without an issue only marks the file.
    const b = row(`b-${token}`);
    await b.getByRole("button", { name: "Irrelevant" }).click();
    await expect(b.getByRole("button", { name: "Irrelevant" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    await expect
      .poll(fileNow)
      .toContain(
        "Severity: Low · Decision: Irrelevant — user decided this issue is not important.",
      );

    // Need fix files a [High] issue and links it in the file.
    await a.getByRole("button", { name: "Need fix" }).click();
    const link = a.getByTestId("finding-decision").getByRole("link");
    await expect(link).toBeVisible({ timeout: 30_000 });
    const key = (await link.textContent())!.trim();
    expect(await issueNow(key)).toMatchObject({
      title: `[High] ${title}`,
      state: "open",
    });
    await expect.poll(fileNow).toContain(`Decision: Need fix — [${key}](`);

    // Defer keeps the issue and lowers its priority.
    await a.getByRole("button", { name: "Defer" }).click();
    await expect(a.getByRole("button", { name: "Defer" })).toHaveAttribute(
      "aria-pressed",
      "true",
      { timeout: 30_000 },
    );
    expect((await issueNow(key)).title).toBe(`[Low] ${title}`);

    // Irrelevant closes it; Need fix reopens the same issue.
    await a.getByRole("button", { name: "Irrelevant" }).click();
    await expect(a).toContainText(`${key} (closed)`, { timeout: 30_000 });
    expect((await issueNow(key)).state).toBe("closed");
    await a.getByRole("button", { name: "Need fix" }).click();
    await expect(a.getByRole("button", { name: "Need fix" })).toHaveAttribute(
      "aria-pressed",
      "true",
      { timeout: 30_000 },
    );
    expect(await issueNow(key)).toMatchObject({
      title: `[High] ${title}`,
      state: "open",
    });
    await expect.poll(fileNow).toContain(`Decision: Need fix — [${key}](`);
  } finally {
    await deleteWorkspaceById(page, workspace.id).catch(() => undefined);
  }
});
