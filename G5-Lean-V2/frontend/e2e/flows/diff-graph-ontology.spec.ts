import { expect, test, type Page } from "@playwright/test";
import {
  authedApiViaRequest,
  cloneWorkspaceById,
  createWorkspaceClone,
  deleteWorkspaceById,
  openWorkspacePath,
  runWorkspaceVmAction,
  waitForWorkspaceVmStatus,
  type WorkspaceClone,
} from "../helpers";

// Diff Graph over current-format ontology (.g5/ontology/features/diff_graph.g5.md):
// real API, Git and worker persistence in disposable parent and child workspaces.

// Declared at load time so the CI timeout backstop sees the budget.
test.describe.configure({ timeout: 900_000 });

const ROOT = ".g5/ontology/diff-graph-root.g5.md";
const OTHER = ".g5/ontology/features/diff-graph-other.g5.md";
const NOTES = "diff-graph-notes.txt";

function ontologyPage(
  title: string,
  statements: [id: string, kind: string, body: string, provenance: string][],
): string {
  const body = statements
    .map(([id, kind, text]) => `<a id="${id}"></a>\n[${kind} · ${id}]\n\n${text}\n`)
    .join("\n");
  const footer = statements
    .map(([id, , , provenance]) => `### ${id}\n\n- ${provenance}\n`)
    .join("\n");
  return `# ${title}\n\n${body}\n## Metadata\n\n${footer}`;
}

async function codeRepoId(page: Page, workspaceId: string): Promise<string> {
  const detail = await authedApiViaRequest(page, `/api/v1/workspaces/${workspaceId}`);
  const data = detail.body?.data ?? detail.body;
  const repoId = data.branches.find(
    (branch: { repoId: string }) => branch.repoId !== data.docsRepoId,
  )?.repoId;
  expect(repoId).toBeTruthy();
  return repoId;
}

async function save(
  page: Page,
  workspaceId: string,
  repoId: string,
  vmId: string,
  path: string,
  content: string,
  commitAndPush: boolean,
) {
  const current = await authedApiViaRequest(
    page,
    `/api/v1/workspaces/${workspaceId}/files/${path}?repoId=${repoId}`,
  );
  const expectedHash =
    current.status === 200 ? ((current.body?.data ?? current.body).hash as string) : null;
  const response = await authedApiViaRequest(
    page,
    `/api/v1/workspaces/${workspaceId}/files/${path}?repoId=${repoId}`,
    "PUT",
    { content, encoding: "utf-8", expectedHash, expectedVmId: vmId, commitAndPush },
  );
  expect(response.status, JSON.stringify(response.body)).toBe(200);
}

test("Diff Graph shows current-format ontology, typed links and opens statements in the editor", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1600, height: 1000 });
  const parent = await createWorkspaceClone(page, { namePrefix: "diff-graph-parent" });
  let child: WorkspaceClone | undefined;
  try {
    await runWorkspaceVmAction(page, parent.id, "resume");
    const parentVm = await waitForWorkspaceVmStatus(page, parent.id, "running");
    const parentRepo = await codeRepoId(page, parent.id);
    await save(
      page,
      parent.id,
      parentRepo,
      parentVm.vmId!,
      ROOT,
      ontologyPage("Diff graph root", [
        ["dg.a", "I", "Alpha.", "agent"],
        ["dg.b", "D", "Beta, deleted in the child.", "agent"],
      ]),
      true,
    );
    await save(
      page,
      parent.id,
      parentRepo,
      parentVm.vmId!,
      OTHER,
      ontologyPage("Diff graph other", [["dg.o", "I", "Other.", "agent"]]),
      true,
    );
    await runWorkspaceVmAction(page, parent.id, "stop");
    await waitForWorkspaceVmStatus(page, parent.id, "stopped");
    child = await cloneWorkspaceById(page, parent.id, {
      namePrefix: "diff-graph-child",
      source: "durable",
    });
    await runWorkspaceVmAction(page, child.id, "resume");
    const childVm = await waitForWorkspaceVmStatus(page, child.id, "running");
    const childRepo = await codeRepoId(page, child.id);
    await save(
      page,
      child.id,
      childRepo,
      childVm.vmId!,
      ROOT,
      ontologyPage("Diff graph root", [["dg.a", "I", "Alpha, edited.", "human-provided"]]),
      true,
    );
    await save(
      page,
      child.id,
      childRepo,
      childVm.vmId!,
      OTHER,
      ontologyPage("Diff graph other", [
        ["dg.o", "I", "Other.", "agent"],
        [
          "dg.o2",
          "I",
          "Cites [beta](../diff-graph-root.g5.md#dg.b) and [nowhere](missing.g5.md).",
          "agent",
        ],
      ]),
      true,
    );
    // Saved but not committed: All changes includes it for the owner.
    await save(page, child.id, childRepo, childVm.vmId!, NOTES, "saved only\n", false);

    await openWorkspacePath(page, `${child.path}?view=diff-graph`);
    const graph = page.getByTestId("source-control-change-graph");
    await expect(graph.getByTestId("change-graph-live-content")).toHaveText(
      "Includes saved uncommitted edits.",
    );
    await expect(graph.getByTestId("change-graph-legend")).toBeVisible();
    await expect(
      graph.locator('[data-testid="change-graph-column"][data-column="ontology"]'),
    ).toContainText("Ontology");
    await expect(
      graph.getByTestId("change-graph-group-label").filter({ hasText: "features" }),
    ).toBeVisible();
    await expect(graph.getByRole("button", { name: /diff-graph-notes\.txt/ })).toBeVisible();
    const other = graph.getByRole("button", {
      name: /features\/diff-graph-other, Ontology, Modified/,
    });
    await expect(other.getByTestId("change-graph-card-notice")).toHaveText(
      "⚠ 1 missing link target",
    );
    await expect(
      graph.locator('[data-testid="change-graph-edge"][data-relationship="reference"]'),
    ).toHaveCount(1);

    // Linked target deleted in this comparison: the edge points at the deleted
    // statement's file, never at a guessed location.
    await other.click();
    const details = page.getByTestId("change-graph-node-details");
    await expect(details.getByTestId("change-graph-missing-targets")).toContainText(
      "missing.g5.md",
    );
    await details.getByTestId("change-graph-changed-grounding-edges-toggle").click();
    await expect(
      details.getByTestId("change-graph-evidence-target-deleted"),
    ).toBeVisible();

    // A changed statement opens the ontology editor with Compare on; a deleted
    // statement opens at its read-only Before row.
    await graph
      .getByRole("button", { name: /^diff-graph-root, Ontology, Modified/ })
      .click();
    const statements = details.getByTestId("change-graph-changed-semantic-blocks-section");
    await expect(statements).toContainText("dg.a");
    await expect(statements).toContainText("dg.b");
    await statements
      .getByRole("button", { name: "Open dg.b in the ontology editor" })
      .click();
    const editor = page.getByTestId("file-view");
    await expect(page).toHaveURL(
      (url) =>
        decodeURIComponent(url.pathname).endsWith(`/file/${ROOT}`) &&
        url.searchParams.get("branch") === childRepo &&
        !url.searchParams.has("editor"),
    );
    await expect(
      editor.getByRole("button", { name: "Compare", exact: true }),
    ).toHaveAttribute("aria-pressed", "true");
    await expect(
      editor.getByRole("region", { name: "Deleted statement dg.b", exact: true }),
    ).toBeInViewport();

    // Human intent keeps the file with a changed human statement only.
    await openWorkspacePath(page, `${child.path}?view=diff-graph`);
    await graph.getByRole("button", { name: "Human intent" }).click();
    await expect(
      graph.getByRole("button", { name: /^diff-graph-root, Ontology, Modified/ }),
    ).toBeVisible();
    await expect(
      graph.getByRole("button", { name: /features\/diff-graph-other, Ontology/ }),
    ).toHaveCount(0);
  } finally {
    if (child) await deleteWorkspaceById(page, child.id);
    await deleteWorkspaceById(page, parent.id);
  }
});
