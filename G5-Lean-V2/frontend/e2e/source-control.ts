import { randomUUID } from "node:crypto";
import { expect, type Page } from "@playwright/test";
import {
  activateSidebarSection,
  openWorkspacePath,
  captureChatTurn,
  expectAssistantReply,
  releaseChatTurn,
  selectAgentProviderViaSettings,
  type WorkspaceClone,
} from "./helpers";
import { shell } from "./workspace-review";
import type {
  SourceMenu,
  SourceRepo,
  SourceSnapshot,
} from "../src/lib/sourceControlSelection";

export const quote = (value: string) => `'${value.replaceAll("'", "'\\''")}'`;

/** Read every applied patch, including bodies released outside the viewport. */
export async function readAppliedDiff(page: Page): Promise<string> {
  const stream = page.getByTestId("source-control-diff-stream");
  await expect(stream).toBeVisible();
  const position = await stream.evaluate((element) => element.scrollTop);
  const cards = stream.locator("article");
  if (!(await cards.count())) return stream.innerText();
  const result = [];
  try {
    for (const card of await cards.all()) {
      const patches = [];
      for (const section of await card.locator(":scope > section").all()) {
        const body = section.getByTestId("source-control-diff-body");
        if (await body.count()) {
          await body.scrollIntoViewIfNeeded();
          await expect(body).toHaveAttribute("data-rendered", "true");
        }
        patches.push(await section.innerText());
      }
      result.push({
        id: await card.getAttribute("id"),
        heading: await card
          .locator(":scope > .source-control-file-heading")
          .innerText(),
        patches,
      });
    }
    return JSON.stringify(result);
  } finally {
    await stream.evaluate((element, top) => {
      element.scrollTop = top;
    }, position);
    await page.evaluate(
      () =>
        new Promise<void>((resolve) =>
          requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
        ),
    );
  }
}

/** Independent Git evidence, emitted by the real terminal with echo disabled. */
export async function gitHead(page: Page, ws: WorkspaceClone, root: string) {
  const marker = `HEAD_${randomUUID().slice(0, 8)}`;
  await shell(
    page,
    ws,
    `git -C ${quote(root)} rev-parse HEAD | sed 's/^/${marker} /'`,
  );
  const output = await page
    .getByTestId("workspace-terminal")
    .locator(".xterm-accessibility-tree")
    .innerText();
  const sha = output.match(new RegExp(`${marker} ([a-f0-9]{40})`))?.[1];
  expect(sha, output).toMatch(/^[a-f0-9]{40}$/);
  return sha!;
}

function childLabels(repo: SourceRepo) {
  return [
    ...(repo.live && repo.live.selectable !== false
      ? [`Not yet pushed in ${repo.name}`]
      : []),
    ...repo.commits.map(
      (commit) => `${commit.sha.slice(0, 7)} in ${repo.name}`,
    ),
  ];
}

export async function openChecklist(
  page: Page,
  ws: WorkspaceClone,
): Promise<SourceMenu> {
  await openWorkspacePath(page, `${ws.path}?view=activity`);
  const response = page.waitForResponse(
    (r) =>
      r.url().endsWith("/source-control/menu") &&
      r.request().method() === "GET",
  );
  await activateSidebarSection(page, "sidebar-source-control-tab");
  const received = await response;
  expect(received.status(), await received.text()).toBe(200);
  const menu = (await received.json()).data as SourceMenu;
  expect(menu.repos.length).toBeGreaterThanOrEqual(2);
  expect(menu.repos.every((r) => !r.error && !r.truncated)).toBe(true);
  for (const repo of menu.repos) {
    const heading = page.getByLabel(`All changes in ${repo.name}`, {
      exact: true,
    });
    const children = childLabels(repo);
    expect(children.length).toBeGreaterThan(1);
    await expect(heading).toBeChecked();
    await heading.uncheck();
    for (const label of children) {
      const child = page.getByLabel(label, { exact: true });
      await expect(child).not.toBeChecked();
      await expect(child).toBeVisible();
      await expect(child).toBeEnabled();
    }
    await page.getByLabel(children[0], { exact: true }).check();
    await expect(heading).toBeChecked({ indeterminate: true });
    await heading.check();
    for (const label of children)
      await expect(page.getByLabel(label, { exact: true })).toBeChecked();
  }
  return menu;
}

/** A deterministic provider crosses chat dispatch and runs the shipped CLI. */
export async function saveFromChat(page: Page, ws: WorkspaceClone) {
  await openWorkspacePath(page, ws.path);
  await page.getByTestId("center-chat-tab").click();
  await selectAgentProviderViaSettings(page, "claude_code");
  const input = page.getByTestId("chat-input").first();
  await expect(input).toBeVisible();
  const response = page.waitForResponse(
    (r) =>
      r.request().method() === "POST" &&
      /\/api\/v1\/(sessions\/[^/]+\/messages|workspaces\/[^/]+\/sessions\/start)$/.test(
        new URL(r.url()).pathname,
      ),
  );
  await input.fill("Save changes. G5_WORKSPACE_SAVE_E2E");
  await input.press("Enter");
  const turn = await captureChatTurn(page, await response);
  try {
    await expectAssistantReply(
      page,
      turn,
      "Fake workspace Save completed: generation",
      { timeout: 120_000 },
    );
  } finally {
    await releaseChatTurn(page, turn);
  }
}

export type Chosen = Record<string, { commits: string[]; unpushed: boolean }>;
/** Drive checkboxes, then check the actual request, response and visible cards. */
export async function applyChecklist(
  page: Page,
  menu: SourceMenu,
  chosen: Chosen,
): Promise<SourceSnapshot> {
  const control = page.getByTestId("source-control-menu");
  const all = control.getByRole("checkbox", {
    name: "All changes",
    exact: true,
  });
  const selectedTypes: string[] = [];
  for (const name of ["Code", "Ontology", "Docs"])
    if (await control.getByRole("checkbox", { name, exact: true }).isChecked())
      selectedTypes.push(name);
  await all.check();
  await all.uncheck();
  for (const name of ["Code", "Ontology", "Docs"])
    await expect(
      control.getByRole("checkbox", { name, exact: true }),
    ).not.toBeChecked();
  for (const name of selectedTypes)
    await control.getByRole("checkbox", { name, exact: true }).check();
  await expect(
    control.getByRole("button", { name: "Show", exact: true }),
  ).toBeDisabled();
  for (const repo of menu.repos) {
    const choice = chosen[repo.repoId];
    const heading = control.getByLabel(`All changes in ${repo.name}`, {
      exact: true,
    });
    await expect(heading).not.toBeChecked();
    // Clearing a repository must leave every child available.
    for (const commit of repo.commits)
      await expect(
        control.getByLabel(`${commit.sha.slice(0, 7)} in ${repo.name}`, {
          exact: true,
        }),
      ).toBeVisible();
    if (!choice) continue;
    for (const sha of choice.commits)
      await control
        .getByLabel(`${sha.slice(0, 7)} in ${repo.name}`, { exact: true })
        .check();
    if (choice.unpushed)
      await control
        .getByLabel(`Not yet pushed in ${repo.name}`, { exact: true })
        .check();
    const selectedCount = choice.commits.length + Number(choice.unpushed);
    if (selectedCount === childLabels(repo).length)
      await expect(heading).toBeChecked();
    else await expect(heading).toBeChecked({ indeterminate: true });
  }
  const response = page.waitForResponse(
    (r) =>
      r.url().endsWith("/source-control/apply") &&
      r.request().method() === "POST",
  );
  await control.getByRole("button", { name: "Show", exact: true }).click();
  const received = await response;
  expect(received.status(), await received.text()).toBe(200);
  const input = received.request().postDataJSON();
  expect(input.repos.map((r: { repoId: string }) => r.repoId).sort()).toEqual(
    Object.keys(chosen).sort(),
  );
  for (const row of input.repos) {
    expect([...row.commits].sort()).toEqual(
      [...chosen[row.repoId].commits].sort(),
    );
    expect(row.includeUnpushed).toBe(chosen[row.repoId].unpushed);
  }
  const snapshot = (await received.json()).data as SourceSnapshot;
  const types: string[] = [];
  for (const name of ["Code", "Ontology", "Docs"])
    if (await control.getByRole("checkbox", { name, exact: true }).isChecked())
      types.push(name.toLowerCase());
  const matching = new Set(
    snapshot.patches
      .filter((patch) => types.includes(patch.kind))
      .map((patch) => `${patch.repoId}:${patch.fileKey ?? patch.file.path}`),
  );
  await expect(
    page.getByTestId("source-control-diff-stream").locator("article"),
  ).toHaveCount(matching.size);
  return snapshot;
}

export function expectFiles(
  snapshot: SourceSnapshot,
  expected: [string, string][],
) {
  const actual = [
    ...new Set(snapshot.patches.map((p) => `${p.repoId}:${p.file.path}`)),
  ].sort();
  expect(actual).toEqual(
    expected.map(([repo, path]) => `${repo}:${path}`).sort(),
  );
  expect(snapshot.fileCount).toBe(expected.length);
}

export function added(snapshot: SourceSnapshot, path: string) {
  return snapshot.patches
    .filter((p) => p.file.path === path)
    .flatMap((p) =>
      p.file.diff.hunks.flatMap((h) =>
        h.lines.filter((l) => l.type === "addition").map((l) => l.content),
      ),
    )
    .join("\n");
}
