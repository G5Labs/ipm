import { randomUUID } from "node:crypto";
import { expect, test } from "@playwright/test";
import {
  authedApiViaRequest,
  createWorkspaceClone,
  deleteWorkspaceById,
  expandOntologyTree,
  openWorkspacePath,
  runWorkspaceVmAction,
  waitForWorkspaceVmStatus,
} from "../helpers";

test.describe.configure({ timeout: 600_000 });

// Real HTTP and a running workspace: statement folds and verification status
// read from files written to the checkout (g5.documents.ontology.disclosure,
// g5.documents.ontology.verification). No routes are intercepted.
test("ontology statements fold and show verification read from saved artifacts", async ({
  page,
}) => {
  const workspace = await createWorkspaceClone(page, {
    namePrefix: "ontology-verification",
  });
  try {
    await runWorkspaceVmAction(page, workspace.id, "resume");
    const vm = await waitForWorkspaceVmStatus(page, workspace.id, "running");
    const detail = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${workspace.id}`,
    );
    expect(detail.status).toBe(200);
    const data = detail.body?.data ?? detail.body;
    const repoId = data.branches.find(
      (b: { repoId: string }) => b.repoId !== data.docsRepoId,
    )?.repoId;
    expect(repoId).toBeTruthy();
    const stem = `verification-${randomUUID()}`;
    const tla = ".g5/TLA_verification";
    const files: Record<string, string> = {
      [`.g5/ontology/${stem}.g5.md`]:
        `# Receiving\n\n## Numbers\n\n<a id="${stem}.number"></a>\n[I · ${stem}.number]\n\nEvery receipt needs a number.\n\n` +
        `<a id="${stem}.number.unique"></a>\n[I · ${stem}.number.unique]\n\nUnique per vendor.\n\n- Suggestions reserve nothing.\n\n` +
        `<a id="${stem}.plain"></a>\n[I · ${stem}.plain]\n\nNothing recorded.\n\n## Metadata\n\n` +
        `### ${stem}.number\n\n- agent\n\n` +
        `### ${stem}.number.unique\n\n- agent\n- design-verified\n\nState machines: [Numbering](../TLA_verification/models/${stem}/manifest.json)\n\nExpands: [number](#${stem}.number)\n\n` +
        `### ${stem}.plain\n\n- agent\n`,
      [`${tla}/models/${stem}/manifest.json`]: JSON.stringify({
        concern: stem,
        question: "Is the receipt number unique per vendor?",
        ontology_ids: [`${stem}.number.unique`],
        runs: [
          {
            label: "safety",
            spec: "Numbering.tla",
            cfg: "safety.cfg",
            kind: "safety",
            variant: "as-built",
            expect: "pass",
          },
        ],
      }),
      [`${tla}/models/${stem}/BINDINGS.md`]:
        "# Numbering bindings\n\nSuggest maps to the query handler.\n",
      [`${tla}/results/index.jsonl`]:
        JSON.stringify({
          run_id: `20261101T000000Z-${stem}`,
          concern: stem,
          label: "safety",
          kind: "safety",
          variant: "as-built",
          expect: "pass",
          outcome: "PASS",
          distinct_states: 4242,
        }) + "\n",
    };
    for (const [path, content] of Object.entries(files)) {
      const write = await authedApiViaRequest(
        page,
        `/api/v1/workspaces/${workspace.id}/files/${path}?repoId=${repoId}`,
        "PUT",
        {
          content,
          encoding: "utf-8",
          expectedHash: null,
          expectedVmId: vm.vmId,
        },
      );
      expect(write.status, `${path}: ${JSON.stringify(write.body)}`).toBe(200);
    }
    await openWorkspacePath(page, workspace.path);
    const tree = await expandOntologyTree(page);
    await tree.getByText(stem, { exact: true }).click();
    const editor = page.getByTestId("file-view");
    // The hidden source view also holds the text; read the rendered preview.
    const preview = editor.getByTestId("file-view-preview");
    await expect(
      preview.getByText("Every receipt needs a number."),
    ).toBeVisible();
    // Compare starts on, and every statement of this new file is a change
    // that opens its fold; turn it off to see folds start closed.
    const compare = editor.getByRole("button", {
      name: "Compare",
      exact: true,
    });
    await expect(compare).toHaveAttribute("aria-pressed", "true");
    await compare.click();
    await expect(compare).toHaveAttribute("aria-pressed", "false");
    // Closed fold: the child statement is hidden until the triangle opens it.
    await expect(preview.getByText("Unique per vendor.")).toBeHidden();
    const statuses = editor.getByTestId("verification-status");
    await expect(statuses.first()).toHaveText("Design verified");
    await expect(statuses.last()).toHaveText("Unverified");
    await editor.getByTestId("ontology-statement-toggle").click();
    await expect(preview.getByText("Unique per vendor.")).toBeVisible();
    await expect(
      preview.getByText("Suggestions reserve nothing."),
    ).toBeVisible();
    await statuses.nth(1).click();
    const panel = editor.getByTestId("verification-panel");
    await expect(panel).toContainText(
      "Is the receipt number unique per vendor?",
    );
    await panel.getByText("State machine and verification details").click();
    await expect(panel.getByText("Numbering bindings")).toBeVisible();
    await expect(panel.getByText("PASS", { exact: true })).toBeVisible();
    await expect(panel.getByText("4,242")).toBeVisible();
  } finally {
    await deleteWorkspaceById(page, workspace.id).catch(() => undefined);
  }
});
