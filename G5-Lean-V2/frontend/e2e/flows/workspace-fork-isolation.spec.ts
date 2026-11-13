import { randomUUID } from "node:crypto";
import { expect, test, type Page } from "@playwright/test";
import {
  authedApiViaRequest,
  cloneWorkspaceById,
  createWorkspaceClone,
  deleteWorkspaceById,
  openRailTerminal,
  openWorkspacePath,
  readWorkspaceVm,
  runWorkspaceVmAction,
  type LiveWorkspaceChangeMember,
  type WorkspaceClone,
  waitForLiveWorkspaceChanges,
  waitForWorkspaceVmStatus,
} from "../helpers";

type Branch = { repoId: string; branchName: string; headCommit: string };
type Detail = { id: string; docsRepoId: string; branches: Branch[] };
type Published = {
  codePath: string;
  codeBytes: string;
  docPath: string;
  docBytes: string;
  heads: Map<string, string>;
};

async function detail(page: Page, workspaceId: string): Promise<Detail> {
  const response = await authedApiViaRequest(
    page,
    `/api/v1/workspaces/${workspaceId}`,
  );
  expect(response.status, JSON.stringify(response.body)).toBe(200);
  return (response.body?.data ?? response.body) as Detail;
}

function headVector(detail: Detail): Map<string, string> {
  expect(detail.branches).toHaveLength(2);
  expect(new Set(detail.branches.map((branch) => branch.branchName)).size).toBe(
    1,
  );
  const heads = new Map(
    detail.branches.map((branch) => [branch.repoId, branch.headCommit]),
  );
  for (const head of heads.values()) {
    expect(head).toMatch(/^([0-9a-f]{40}|[0-9a-f]{64})$/);
  }
  return heads;
}

async function remoteVector(
  page: Page,
  workspaceId: string,
): Promise<Map<string, string>> {
  let report:
    | {
        source: string;
        attached: boolean;
        members: LiveWorkspaceChangeMember[];
      }
    | undefined;
  await expect
    .poll(
      async () => {
        const response = await authedApiViaRequest(
          page,
          `/api/v1/workspaces/${workspaceId}/changes`,
        );
        if (
          response.status === 409 &&
          response.body?.error?.message?.includes(
            "MODULE_CONCURRENCY_EXHAUSTED",
          )
        ) {
          return false;
        }
        expect(response.status, JSON.stringify(response.body)).toBe(200);
        report = response.body?.data ?? response.body;
        return report?.source === "live" && report.attached;
      },
      { timeout: 30_000, intervals: [500, 1_000] },
    )
    .toBe(true);
  const heads = new Map(
    report!.members.map((member) => [
      member.repo_id,
      member.internal_ref ?? "",
    ]),
  );
  expect(heads.size).toBe(2);
  for (const head of heads.values()) {
    expect(head).toMatch(/^([0-9a-f]{40}|[0-9a-f]{64})$/);
  }
  return heads;
}

function quoted(value: string): string {
  return `'${value.replaceAll("'", "'\\''")}'`;
}

async function commitBoth(
  page: Page,
  code: LiveWorkspaceChangeMember,
  docs: LiveWorkspaceChangeMember,
  codePath: string,
  docPath: string,
) {
  await openRailTerminal(page);
  const terminal = page.getByTestId("workspace-terminal");
  await expect(terminal).toBeVisible({ timeout: 30_000 });
  await expect(page.getByText("Workspace terminal · Connected")).toBeVisible({
    timeout: 30_000,
  });
  const input = terminal.locator(".xterm-helper-textarea");
  await input.focus();
  await input.pressSequentially("stty -echo");
  await input.press("Enter");
  const suffix = randomUUID().replaceAll("-", "");
  const marker = `G5_F9_COMMITTED_${suffix}`;
  await input.pressSequentially(
    `git -C ${quoted(code.root_path)} add -- ${quoted(codePath)} && git -C ${quoted(code.root_path)} -c user.email=e2e@g5.dev -c user.name=e2e commit -qm 'F9 code' -- ${quoted(codePath)} && git -C ${quoted(docs.root_path)} add -- ${quoted(docPath)} && git -C ${quoted(docs.root_path)} -c user.email=e2e@g5.dev -c user.name=e2e commit -qm 'F9 docs' -- ${quoted(docPath)} && printf '%s_%s\\n' G5_F9_COMMITTED ${suffix}`,
  );
  await input.press("Enter");
  await expect(terminal.locator(".xterm-accessibility-tree")).toContainText(
    marker,
    { timeout: 30_000 },
  );
}

async function publishWorkspace(
  page: Page,
  workspace: WorkspaceClone,
  label: string,
): Promise<Published> {
  await openWorkspacePath(page, workspace.path);
  const current = await detail(page, workspace.id);
  const before = await remoteVector(page, workspace.id);
  const live = await waitForLiveWorkspaceChanges(page, workspace.id);
  const code = live.members.find(
    (member) => member.repo_id !== current.docsRepoId,
  );
  const docs = live.members.find(
    (member) => member.repo_id === current.docsRepoId,
  );
  expect(code?.root_path).toBeTruthy();
  expect(docs?.root_path).toBeTruthy();
  const token = randomUUID();
  const codePath = `e2e-f9-${label}-${token}.txt`;
  const docPath = `docs/${workspace.id}/e2e-f9-${label}-${token}.md`;
  const codeBytes = `${label} code from ${workspace.id}\n`;
  const docBytes = `# ${label} Project Doc from ${workspace.id}\n`;
  const codeWrite = await authedApiViaRequest(
    page,
    `/api/v1/workspaces/${workspace.id}/files/${codePath}?repoId=${code!.repo_id}`,
    "PUT",
    { content: codeBytes, encoding: "utf-8", expectedHash: null },
  );
  expect([200, 201], JSON.stringify(codeWrite.body)).toContain(
    codeWrite.status,
  );
  const docWrite = await authedApiViaRequest(
    page,
    `/api/v1/workspaces/${workspace.id}/docs`,
    "POST",
    {
      name: docPath.split("/").at(-1),
      docType: "design",
      filePath: docPath,
      content: docBytes,
    },
  );
  expect(docWrite.status, JSON.stringify(docWrite.body)).toBe(201);
  await commitBoth(page, code!, docs!, codePath, docPath);

  const committed = await waitForLiveWorkspaceChanges(page, workspace.id);
  const heads = new Map(
    committed.members.map((member) => [member.repo_id, member.head_oid ?? ""]),
  );
  for (const member of [code!, docs!]) {
    expect(heads.get(member.repo_id)).toMatch(/^([0-9a-f]{40}|[0-9a-f]{64})$/);
    expect(heads.get(member.repo_id)).not.toBe(before.get(member.repo_id));
    expect(
      committed.members.find((entry) => entry.repo_id === member.repo_id)
        ?.against_internal?.ahead,
    ).toBeGreaterThanOrEqual(1);
  }
  const push = await authedApiViaRequest(
    page,
    `/api/v1/workspaces/${workspace.id}/changes/push`,
    "POST",
    {},
  );
  expect(push.status, JSON.stringify(push.body)).toBe(200);
  await expect
    .poll(
      async () => {
        const response = await authedApiViaRequest(
          page,
          `/api/v1/workspaces/${workspace.id}/changes`,
        );
        if (
          response.status === 409 &&
          response.body?.error?.message?.includes(
            "MODULE_CONCURRENCY_EXHAUSTED",
          )
        ) {
          return ["busy", "busy"];
        }
        expect(response.status, JSON.stringify(response.body)).toBe(200);
        const report = response.body?.data ?? response.body;
        return [code!.repo_id, docs!.repo_id].map(
          (repoId) =>
            report.members.find(
              (member: LiveWorkspaceChangeMember) => member.repo_id === repoId,
            )?.internal_ref,
        );
      },
      { timeout: 30_000, intervals: [500, 1_000] },
    )
    .toEqual([heads.get(code!.repo_id), heads.get(docs!.repo_id)]);
  expect(await remoteVector(page, workspace.id)).toEqual(heads);
  return { codePath, codeBytes, docPath, docBytes, heads };
}

async function assertFile(
  page: Page,
  workspaceId: string,
  repoId: string,
  path: string,
  expected: string | null,
) {
  const response = await authedApiViaRequest(
    page,
    `/api/v1/workspaces/${workspaceId}/files/${path}?repoId=${repoId}`,
  );
  if (expected === null) {
    expect(response.status, JSON.stringify(response.body)).toBe(404);
  } else {
    expect(response.status, JSON.stringify(response.body)).toBe(200);
    expect((response.body?.data ?? response.body).content).toBe(expected);
  }
}

test.describe.configure({ timeout: 360_000 });

test("F9 child and parent commits and pushes stay isolated across code and docs", async ({
  page,
}) => {
  const parent = await createWorkspaceClone(page, {
    namePrefix: "f9-parent-child-isolation",
    start: true,
  });
  let child: WorkspaceClone | undefined;
  try {
    const parentBefore = await detail(page, parent.id);
    const parentHeads = await remoteVector(page, parent.id);
    expect(headVector(parentBefore)).toEqual(parentHeads);
    const parentVm = await readWorkspaceVm(page, parent.id);
    child = await cloneWorkspaceById(page, parent.id, {
      namePrefix: "f9-isolated-child",
      source: "durable",
      start: false,
    });
    const childBefore = await detail(page, child.id);
    expect(headVector(childBefore)).toEqual(parentHeads);
    expect(childBefore.branches.map((branch) => branch.repoId).sort()).toEqual(
      parentBefore.branches.map((branch) => branch.repoId).sort(),
    );
    expect(childBefore.branches[0].branchName).not.toBe(
      parentBefore.branches[0].branchName,
    );
    await runWorkspaceVmAction(page, child.id, "resume");
    await waitForWorkspaceVmStatus(page, child.id, "running");

    const childCut = await publishWorkspace(page, child, "child");
    expect(await remoteVector(page, parent.id)).toEqual(parentHeads);
    const codeRepoId = parentBefore.branches.find(
      (branch) => branch.repoId !== parentBefore.docsRepoId,
    )!.repoId;
    await assertFile(page, parent.id, codeRepoId, childCut.codePath, null);
    await assertFile(
      page,
      parent.id,
      parentBefore.docsRepoId,
      childCut.docPath,
      null,
    );
    const parentVmAfterChild = await readWorkspaceVm(page, parent.id);
    expect(parentVmAfterChild.status).toBe("running");
    expect(parentVmAfterChild.vmId).toBe(parentVm.vmId);
    expect(parentVmAfterChild.epoch).toBe(parentVm.epoch);

    const parentCut = await publishWorkspace(page, parent, "parent");
    expect(await remoteVector(page, child.id)).toEqual(childCut.heads);
    await assertFile(page, child.id, codeRepoId, parentCut.codePath, null);
    await assertFile(
      page,
      child.id,
      childBefore.docsRepoId,
      parentCut.docPath,
      null,
    );
    for (const { workspaceId, own, other } of [
      { workspaceId: child.id, own: childCut, other: parentCut },
      { workspaceId: parent.id, own: parentCut, other: childCut },
    ]) {
      await assertFile(
        page,
        workspaceId,
        codeRepoId,
        own.codePath,
        own.codeBytes,
      );
      await assertFile(
        page,
        workspaceId,
        parentBefore.docsRepoId,
        own.docPath,
        own.docBytes,
      );
      expect(own.heads.get(codeRepoId)).not.toBe(other.heads.get(codeRepoId));
      expect(own.heads.get(parentBefore.docsRepoId)).not.toBe(
        other.heads.get(parentBefore.docsRepoId),
      );
    }
  } finally {
    if (child) await deleteWorkspaceById(page, child.id).catch(() => {});
    await deleteWorkspaceById(page, parent.id).catch(() => {});
  }
});
