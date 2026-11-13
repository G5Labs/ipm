/** Real Git history supplies the expected identities/endpoints; no API stubs. */
import { randomUUID } from "node:crypto";
import { test, expect, api, write, push, shell } from "../workspace-review";
import {
  activateSidebarSection,
  runWorkspaceVmAction,
  waitForWorkspaceVmStatus,
  waitForLiveWorkspaceChanges,
} from "../helpers";
import {
  quote,
  gitHead,
  openChecklist,
  applyChecklist,
  expectFiles,
  added,
  readAppliedDiff,
  type Chosen,
} from "../source-control";

test.describe.configure({ timeout: 600_000 });

test("commit lists and per-repository unpushed choices retain their exact comparison meaning", async ({
  review: f,
}) => {
  const page = f.owner.page,
    ws = f.workspace;
  const agent = await page.context().newPage();
  const live = await waitForLiveWorkspaceChanges(agent, ws.id);
  const root = live.members.find((m) => m.repo_id === f.codeRepo)!.root_path;
  const git = `git -C ${quote(root)}`;
  const prefix = `sc-${randomUUID().slice(0, 8)}`;
  const shared = `${prefix}-shared.txt`,
    skipped = `${prefix}-skipped.txt`;
  const renamed = `${prefix}-renamed.txt`,
    binary = `${prefix}-binary.dat`;
  const newFile = `${prefix}-new.txt`,
    side = `${prefix}-side.txt`;
  const deleted = ".g5/e2e-history/commit-1.txt",
    reverted = ".g5/e2e-history/commit-2.txt";
  const original = await api(
    page,
    `/api/v1/workspaces/${ws.id}/files/${reverted}?repoId=${f.codeRepo}`,
  );
  expect(original.status, JSON.stringify(original.body)).toBe(200);
  const commit = async (label: string) => {
    await shell(
      agent,
      ws,
      `${git} add -A && ${git} -c user.name=Acceptance -c user.email=e2e@g5.dev commit -qm ${quote(label)}`,
    );
    return gitHead(agent, ws, root);
  };
  await write(page, ws, f.codeRepo, shared, "anchor\nSELECTED ONE\nend\n");
  const one = await commit("Selected first patch");
  await write(page, ws, f.codeRepo, shared, "anchor\nSKIPPED SECOND\nend\n");
  await write(page, ws, f.codeRepo, skipped, "ONLY IN SKIPPED COMMIT\n");
  const two = await commit("Skipped middle patch");
  await write(page, ws, f.codeRepo, shared, "anchor\nSELECTED THREE\nend\n");
  const three = await commit("Selected third patch");
  await write(page, ws, f.codeRepo, reverted, "TEMPORARY REVERTED EDIT\n");
  await shell(
    agent,
    ws,
    `${git} mv -- ${quote(f.codePath)} ${quote(renamed)} && ${git} rm -- ${quote(deleted)} && printf '\\000\\377binary' > ${quote(`${root}/${binary}`)}`,
  );
  await commit("Rename, deletion, binary and temporary edit");
  await write(page, ws, f.codeRepo, reverted, original.body.data.content);
  const beforeMerge = await commit("Restore exact base bytes");
  await shell(
    agent,
    ws,
    `${git} switch -c ${quote(prefix)} ${three} && printf 'MERGED SIDE PATCH\\n' > ${quote(`${root}/${side}`)} && ${git} add -- ${quote(side)} && ${git} -c user.name=Acceptance -c user.email=e2e@g5.dev commit -qm 'Side commit' && ${git} switch - && ${git} -c user.name=Acceptance -c user.email=e2e@g5.dev merge --no-ff -qm 'Selected merge' ${quote(prefix)}`,
  );
  const merged = await gitHead(agent, ws, root);
  const pushed = await push(page, ws);
  expect(pushed[f.codeRepo]).toBe(merged);
  await write(page, ws, f.codeRepo, shared, "anchor\nLOCAL COMMIT\nend\n");
  const local = await commit("Local commit stays unpushed");
  await write(page, ws, f.codeRepo, shared, "anchor\nSTAGED EDIT\nend\n");
  await shell(agent, ws, `${git} add -- ${quote(shared)}`);
  await write(page, ws, f.codeRepo, shared, "anchor\nUNSTAGED EDIT\nend\n");
  await write(page, ws, f.codeRepo, newFile, "NONIGNORED NEW FILE\n");
  await write(page, ws, f.docsRepo, f.docPath, "# DOCS PRIVATE EDIT\n");
  const menu = await openChecklist(page, ws);
  const code = menu.repos.find((r) => r.repoId === f.codeRepo)!;
  const docs = menu.repos.find((r) => r.repoId === f.docsRepo)!;
  expect(code.live).toEqual({ selectable: true });
  expect(code.commits.map((c) => c.sha)).not.toContain(local);
  expect(code.commits.find((c) => c.sha === merged)!.parents).toEqual([
    beforeMerge,
    expect.any(String),
  ]);
  const all = (r: typeof code, unpushed: boolean) => ({
    commits: r.commits.map((c) => c.sha),
    unpushed,
  });
  const codeNet = [renamed, deleted, shared, skipped, binary, side].map(
    (p) => [f.codeRepo, p] as [string, string],
  );
  const codeLive: [string, string][] = [
    [f.codeRepo, shared],
    [f.codeRepo, newFile],
  ];
  const doc: [string, string] = [f.docsRepo, f.docPath];
  const cases: {
    name: string;
    chosen: Chosen;
    files: [string, string][];
    mode: string;
  }[] = [
    {
      name: "all current vs base",
      chosen: { [f.codeRepo]: all(code, true), [f.docsRepo]: all(docs, true) },
      files: [...codeNet, [f.codeRepo, newFile], doc],
      mode: "all",
    },
    {
      name: "pushed vs base",
      chosen: {
        [f.codeRepo]: all(code, false),
        [f.docsRepo]: all(docs, false),
      },
      files: [...codeNet, doc],
      mode: "pushed",
    },
    {
      name: "unpushed vs pushed",
      chosen: {
        [f.codeRepo]: { commits: [], unpushed: true },
        [f.docsRepo]: { commits: [], unpushed: true },
      },
      files: [...codeLive, doc],
      mode: "unpushed",
    },
    {
      name: "nonconsecutive commits",
      chosen: { [f.codeRepo]: { commits: [one, three], unpushed: false } },
      files: [[f.codeRepo, shared]],
      mode: "subset",
    },
    {
      name: "subset plus same-repo unpushed",
      chosen: { [f.codeRepo]: { commits: [one, three], unpushed: true } },
      files: codeLive,
      mode: "subset-live",
    },
    {
      name: "subset plus other-repo unpushed",
      chosen: {
        [f.codeRepo]: { commits: [one, three], unpushed: false },
        [f.docsRepo]: { commits: [], unpushed: true },
      },
      files: [[f.codeRepo, shared], doc],
      mode: "subset-docs",
    },
    {
      name: "code live and docs pushed",
      chosen: { [f.codeRepo]: all(code, true), [f.docsRepo]: all(docs, false) },
      files: [...codeNet, [f.codeRepo, newFile], doc],
      mode: "mixed",
    },
    {
      name: "merge and nonconsecutive list",
      chosen: {
        [f.codeRepo]: { commits: [one, three, merged], unpushed: false },
      },
      files: [
        [f.codeRepo, shared],
        [f.codeRepo, side],
      ],
      mode: "merge",
    },
  ];
  for (const scenario of cases)
    await test.step(scenario.name, async () => {
      const snapshot = await applyChecklist(page, menu, scenario.chosen);
      expectFiles(snapshot, scenario.files);
      expect(snapshot.private).toBe(
        Object.values(scenario.chosen).some((c) => c.unpushed),
      );
      expect(snapshot.patches.some((p) => p.file.path === reverted)).toBe(
        false,
      );
      if (scenario.mode.startsWith("subset") || scenario.mode === "merge") {
        const selected = snapshot.patches
          .filter((p) => p.after === one || p.after === three)
          .sort(
            (a, b) =>
              [one, three].indexOf(a.after!) - [one, three].indexOf(b.after!),
          );
        expect(selected).toHaveLength(2);
        expect(selected.map((p) => [p.before, p.after])).toEqual([
          [f.pushed[f.codeRepo], one],
          [two, three],
        ]);
        expect(selected.every((p) => p.file.path === shared)).toBe(true);
        expect(
          added({ ...snapshot, patches: [selected[0]] }, shared),
        ).toContain("SELECTED ONE");
        expect(
          added({ ...snapshot, patches: [selected[1]] }, shared),
        ).toContain("SELECTED THREE");
        expect(added(snapshot, shared)).not.toContain("SKIPPED SECOND");
        expect(snapshot.patches.some((p) => p.file.path === skipped)).toBe(
          false,
        );
        if (scenario.mode === "merge") {
          const merge = snapshot.patches.find((p) => p.after === merged)!;
          expect(merge.before).toBe(beforeMerge);
          expect(merge.label).toContain("merge: first parent");
          expect(merge.file.path).toBe(side);
        }
      }
      if (["all", "mixed", "pushed"].includes(scenario.mode)) {
        expect(
          snapshot.patches.find((p) => p.file.path === renamed)!.file.oldPath,
        ).toBe(f.codePath);
        expect(
          snapshot.patches.find((p) => p.file.path === deleted)!.file.change,
        ).toBe("deleted");
        expect(
          snapshot.patches.find((p) => p.file.path === binary)!.file.diff
            .status,
        ).toBe("binary");
        expect(added(snapshot, shared)).toContain(
          scenario.mode === "pushed" ? "SELECTED THREE" : "UNSTAGED EDIT",
        );
      }
      for (const patch of snapshot.patches.filter((p) => p.after === null)) {
        const repo = menu.repos.find((r) => r.repoId === patch.repoId)!;
        expect(patch.before).toBe(
          scenario.chosen[patch.repoId].commits.length === repo.commits.length
            ? repo.base
            : repo.pushed,
        );
      }
      if (scenario.chosen[f.codeRepo]?.unpushed) {
        expect(added(snapshot, shared)).toContain("UNSTAGED EDIT");
        expect(added(snapshot, shared)).not.toContain("LOCAL COMMIT");
        expect(added(snapshot, newFile)).toContain("NONIGNORED NEW FILE");
      }
      if (scenario.chosen[f.docsRepo]?.unpushed)
        expect(added(snapshot, f.docPath)).toContain("DOCS PRIVATE EDIT");
      else
        expect(added(snapshot, f.docPath)).not.toContain("DOCS PRIVATE EDIT");
    });
});

/** Real commits establish counts independently of the UI/category implementation. */
test("file-type filters keep matching filenames and diffs current across net and custom comparisons", async ({
  review: f,
}, testInfo) => {
  const page = f.owner.page,
    ws = f.workspace;
  const actor = await page.context().newPage();
  try {
    const live = await waitForLiveWorkspaceChanges(actor, ws.id);
    const codeRoot = live.members.find(
      (member) => member.repo_id === f.codeRepo,
    )!.root_path;
    const docsRoot = live.members.find(
      (member) => member.repo_id === f.docsRepo,
    )!.root_path;
    const prefix = `types-${randomUUID().slice(0, 8)}`;
    const ordinary = `${prefix}/README.md`,
      former = `${prefix}/renamed.md`;
    const ontology = `.g5/ontology/${prefix}.g5.md`,
      lookalike = `.g5/ontology/${prefix}.md`;
    const reverted = `${prefix}/temporary.txt`,
      local = `${prefix}/local.ts`;
    const branching = `${prefix}/guide`,
      nested = `${branching}/index.txt`;
    const docsOutside = `assets/${prefix}.txt`,
      docsOntology = `.g5/ontology/${prefix}-docs.g5.md`;
    const docsLookalike = `.g5/ontology/${prefix}-notes.md`;
    const commitRepo = async (root: string, label: string) => {
      const git = `git -C ${quote(root)}`;
      await shell(
        actor,
        ws,
        `${git} add -A && ${git} -c user.name=Acceptance -c user.email=e2e@g5.dev commit -qm ${quote(label)}`,
      );
      return gitHead(actor, ws, root);
    };
    for (const [path, bytes] of [
      [ordinary, "ORDINARY CODE DOCUMENTATION"],
      [former, "UNIQUE CROSS CATEGORY RENAME\nunchanged content"],
      [reverted, "TEMPORARY NET REVERSION"],
      [lookalike, "ORDINARY MARKDOWN UNDER ONTOLOGY"],
      [branching, "ORIGINAL STANDALONE LEAF"],
    ])
      await write(page, ws, f.codeRepo, path, `${bytes}\n`);
    const first = await commitRepo(codeRoot, "First type comparison patch");
    await shell(
      actor,
      ws,
      `mkdir -p ${quote(`${codeRoot}/.g5/ontology`)} && git -C ${quote(codeRoot)} mv -- ${quote(former)} ${quote(ontology)} && git -C ${quote(codeRoot)} rm -- ${quote(reverted)} ${quote(f.codePath)} ${quote(branching)}`,
    );
    await write(page, ws, f.codeRepo, nested, "UNRELATED CHILD VALUE\n");
    const second = await commitRepo(
      codeRoot,
      "Rename into ontology and remove files",
    );
    for (const [path, bytes] of [
      [docsOutside, "OUTSIDE MANAGED DOCS"],
      [docsOntology, "ONTOLOGY IN DOCS ROLE"],
      [docsLookalike, "ORDINARY DOCS UNDER ONTOLOGY"],
    ])
      await write(page, ws, f.docsRepo, path, `${bytes}\n`);
    const docsCommit = await commitRepo(docsRoot, "Docs and ontology roles");
    await push(page, ws);
    await write(page, ws, f.codeRepo, local, "PRIVATE CODE TYPE\n");
    await write(page, ws, f.docsRepo, f.docPath, "# PRIVATE DOCS TYPE\n");
    const menu = await openChecklist(page, ws);
    const control = page.getByTestId("source-control-menu");
    const summary = page.getByTestId("source-control-type-summary");
    await page.getByTestId("landing-summary-changed").click();
    await expect(summary).toHaveText(
      "Code 4 · Ontology 2 · Docs 3 — 9 matching of 9 changed files",
    );
    await expect(
      control.getByRole("button", { name: "Show", exact: true }),
    ).toBeEnabled();
    const all = Object.fromEntries(
      menu.repos.map((repo) => [
        repo.repoId,
        { commits: repo.commits.map((commit) => commit.sha), unpushed: true },
      ]),
    );
    let reads = 0;
    page.on("request", (request) => {
      if (
        /\/source-control\/(apply|pushed|menu|check)$/.test(
          new URL(request.url()).pathname,
        )
      )
        reads++;
    });
    const complete = await applyChecklist(page, menu, all);
    expect(complete.fileCount).toBe(10);
    const global = control.getByRole("checkbox", {
      name: "All changes",
      exact: true,
    });
    const show = control.getByRole("button", {
      name: "Show",
      exact: true,
    });
    const capturedReads = reads;
    const capturedDiff = await readAppliedDiff(page);
    await global.uncheck();
    for (const name of ["Code", "Ontology", "Docs"])
      await expect(
        control.getByRole("checkbox", { name, exact: true }),
      ).not.toBeChecked();
    await expect(show).toBeDisabled();
    await expect(global).not.toBeChecked({ indeterminate: true });
    await expect(page.getByTestId("source-control-file")).toHaveCount(10);
    expect(await readAppliedDiff(page)).toBe(capturedDiff);
    await control.getByRole("checkbox", { name: "Code", exact: true }).check();
    await expect(global).toBeChecked({ indeterminate: true });
    await expect(show).toBeDisabled();
    await global.check();
    for (const name of ["Code", "Ontology", "Docs"])
      await expect(
        control.getByRole("checkbox", { name, exact: true }),
      ).toBeChecked();
    await expect(show).toBeEnabled();
    expect(reads).toBe(capturedReads);
    expect(complete.patches.some((patch) => patch.file.path === reverted)).toBe(
      false,
    );
    expect(
      complete.patches.find((patch) => patch.fileKey === f.codePath)!.file
        .change,
    ).toBe("deleted");
    const expectedKinds = new Map([
      [`${f.codeRepo}:${f.codePath}`, "code"],
      [`${f.codeRepo}:${ordinary}`, "code"],
      [`${f.codeRepo}:${ontology}`, "ontology"],
      [`${f.codeRepo}:${lookalike}`, "code"],
      [`${f.codeRepo}:${local}`, "code"],
      [`${f.codeRepo}:${nested}`, "code"],
      [`${f.docsRepo}:${f.docPath}`, "docs"],
      [`${f.docsRepo}:${docsOutside}`, "docs"],
      [`${f.docsRepo}:${docsOntology}`, "ontology"],
      [`${f.docsRepo}:${docsLookalike}`, "docs"],
    ]);
    expect(
      new Map(
        complete.patches.map((patch) => [
          `${patch.repoId}:${patch.fileKey}`,
          patch.kind,
        ]),
      ),
    ).toEqual(expectedKinds);
    await expect(summary).toHaveText(
      "Code 5 · Ontology 2 · Docs 3 — 10 matching of 10 changed files",
    );
    const help = page.getByLabel("About changed file types", { exact: true });
    await help.focus();
    await page.keyboard.press("Enter");
    await expect(help.locator("..")).toHaveAttribute("open", "");
    await expect(
      page.getByText(/Only files matching the applied types appear/),
    ).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(help.locator("..")).not.toHaveAttribute("open", "");
    await expect(help).toBeFocused();
    const filterTypes = async (
      comparison: typeof complete,
      counts: { Code: number; Ontology: number; Docs: number },
      total: number,
    ) => {
      const stream = page.getByTestId("source-control-diff-stream");
      for (const types of [
        ["Code"],
        ["Ontology"],
        ["Docs"],
        [],
        ["Code", "Docs"],
        ["Ontology", "Docs"],
        ["Code", "Ontology"],
        ["Code", "Ontology", "Docs"],
      ] as (keyof typeof counts)[][]) {
        await stream.evaluate((el) => {
          el.scrollTop = 160;
          el.dispatchEvent(new Event("scroll"));
        });
        await page.evaluate(
          () =>
            new Promise<void>((resolve) =>
              requestAnimationFrame(() =>
                requestAnimationFrame(() => resolve()),
              ),
            ),
        );
        const bytes = await readAppliedDiff(page);
        const position = await stream.evaluate((el) => el.scrollTop);
        const before = await summary.innerText(),
          beforeReads = reads;
        await activateSidebarSection(page, "sidebar-source-control-tab");
        for (const name of ["Code", "Ontology", "Docs"] as const)
          await control
            .getByRole("checkbox", { name, exact: true })
            .setChecked(types.includes(name));
        await expect(summary).toHaveText(before);
        expect(await readAppliedDiff(page)).toBe(bytes);
        expect(await stream.evaluate((el) => el.scrollTop)).toBe(position);
        expect(reads).toBe(beforeReads);
        await control
          .getByRole("button", { name: "Show", exact: true })
          .click();
        const matching = types.reduce((sum, type) => sum + counts[type], 0);
        await expect(summary).toHaveText(
          `${types.map((type) => `${type} ${counts[type]}`).join(" · ")}${types.length ? " — " : ""}${matching} matching of ${total} changed files`,
        );
        await expect(page.getByTestId("source-control-file")).toHaveCount(
          matching,
        );
        await expect(stream.locator("article")).toHaveCount(matching);
        const expected = new Map<string, string[]>();
        const expectedTitles = new Map<string, string>();
        for (const patch of comparison.patches) {
          if (!types.some((type) => type.toLowerCase() === patch.kind))
            continue;
          const key = `${patch.repoId}\0${patch.fileKey ?? patch.file.path}`;
          expectedTitles.set(
            key,
            `${patch.fileKey ?? patch.file.path} ${expected.has(key) ? "changed in multiple commits" : patch.file.change}`,
          );
          const lines = expected.get(key) ?? [];
          lines.push(
            ...patch.file.diff.hunks.flatMap((hunk) =>
              hunk.lines.map((line) => line.content),
            ),
          );
          expected.set(key, lines);
        }
        expect(
          await stream
            .locator("article")
            .evaluateAll((cards) =>
              cards
                .map((card) =>
                  decodeURIComponent(card.id.slice("source-file-".length)),
                )
                .sort(),
            ),
        ).toEqual([...expected.keys()].sort());
        for (const [id, lines] of expected) {
          const card = stream.locator(
            `[id="source-file-${encodeURIComponent(id)}"]`,
          );
          await card.scrollIntoViewIfNeeded();
          await expect(card.getByTestId("diff-line-code")).toHaveText(lines);
        }
        expect(
          await page
            .getByTestId("source-control-file")
            .evaluateAll((rows) =>
              rows.map((row) => row.getAttribute("title")).sort(),
            ),
        ).toEqual([...expectedTitles.values()].sort());
        // Show validates current tips and captures selected live work anew.
        expect(reads).toBe(beforeReads + 1);
        if (!matching)
          await expect(stream).toHaveText(
            "No changed files match the selected file types.",
          );
        if (types.length === 1 && types[0] === "Code") {
          await page.screenshot({
            path: testInfo.outputPath(`code-filter-${total}.png`),
          });
          await expect(
            page
              .getByRole("navigation", { name: "Changed files" })
              .getByRole("button", {
                name: docsOutside.split("/").at(-1)!,
                exact: true,
              }),
          ).toHaveCount(0);
          const row = page
            .getByRole("navigation", { name: "Changed files" })
            .getByRole("button", {
              name: ordinary.split("/").at(-1)!,
              exact: true,
            });
          await row.focus();
          await page.keyboard.press("Enter");
          const card = stream.locator("article").filter({ hasText: ordinary });
          const visiblePosition = await stream.evaluate((el) => el.scrollTop);
          await card
            .getByLabel(`Actions for ${ordinary}`, { exact: true })
            .click();
          await card
            .getByRole("button", { name: "View file", exact: true })
            .click();
          await expect(
            page.getByTestId("center-panel").getByTestId("code-editor"),
          ).toHaveAttribute("data-file-path", ordinary);
          const refreshedMenu = page.waitForResponse(
            (response) =>
              response.url().endsWith("/source-control/menu") &&
              response.status() === 200,
          );
          const refreshedComparison = page.waitForResponse(
            (response) =>
              response.url().endsWith("/source-control/apply") &&
              response.status() === 200,
          );
          await page
            .getByRole("tab", { name: "Source Control", exact: true })
            .click();
          await Promise.all([refreshedMenu, refreshedComparison]);
          await expect(show).toBeEnabled();
          await expect(summary).toContainText(
            `${matching} matching of ${total}`,
          );
          expect(await stream.evaluate((el) => el.scrollTop)).toBe(
            visiblePosition,
          );
        }
      }
    };
    await filterTypes(complete, { Code: 5, Ontology: 2, Docs: 3 }, 10);
    await page.getByRole("tab", { name: "Workspace", exact: true }).click();
    const activity = page.getByTestId("landing-summary-changed");
    await expect(activity).toHaveText("9 changed files");
    await expect(page.getByTestId("landing-summary-code")).toHaveText("Code 4");
    await expect(page.getByTestId("landing-summary-ontology")).toHaveText(
      "Ontology 2",
    );
    await expect(page.getByTestId("landing-summary-docs")).toHaveText("Docs 3");
    const link = new URL((await activity.getAttribute("href"))!, page.url());
    expect(link.searchParams.get("view")).toBe("source-control");
    expect(link.searchParams.has("heads")).toBe(false);
    expect(link.searchParams.has("scope")).toBe(false);
    await activity.click();
    await expect(summary).toHaveText(
      "Code 4 · Ontology 2 · Docs 3 — 9 matching of 9 changed files",
    );
    await expect(show).toBeEnabled();
    // Reload retains the applied selection while recapturing current heads.
    await page.reload();
    await expect(summary).toHaveText(
      "Code 4 · Ontology 2 · Docs 3 — 9 matching of 9 changed files",
    );
    await activateSidebarSection(page, "sidebar-source-control-tab");
    for (const repo of menu.repos) {
      await expect(
        control.getByLabel(`Not yet pushed in ${repo.name}`, { exact: true }),
      ).toBeEnabled();
      await expect(
        control.getByLabel(`Not yet pushed in ${repo.name}`, { exact: true }),
      ).not.toBeChecked();
    }
    await control
      .getByRole("checkbox", { name: "Code", exact: true })
      .uncheck();
    const pushedReads = reads;
    await control.getByRole("button", { name: "Show", exact: true }).click();
    await expect(summary).toHaveText(
      "Ontology 2 · Docs 3 — 5 matching of 9 changed files",
    );
    await expect(
      page.getByText(
        "Selection changed. Showing the previous selection until you click Show.",
        { exact: true },
      ),
    ).toHaveCount(0);
    expect(reads).toBe(pushedReads + 1);
    for (const name of ["Code", "Ontology", "Docs"])
      await control.getByRole("checkbox", { name, exact: true }).check();
    const custom = await applyChecklist(page, menu, {
      [f.codeRepo]: { commits: [first, second], unpushed: false },
      [f.docsRepo]: { commits: [docsCommit], unpushed: false },
    });
    expect(custom.fileCount).toBe(10);
    const renamed = custom.patches.filter(
      (patch) => patch.fileKey === ontology,
    );
    expect(renamed).toHaveLength(2);
    expect(renamed.some((patch) => patch.file.path === former)).toBe(true);
    expect(renamed.every((patch) => patch.kind === "ontology")).toBe(true);
    expect(
      custom.patches.filter((patch) => patch.fileKey === reverted),
    ).toHaveLength(2);
    expect(
      custom.patches.filter((patch) => patch.fileKey === branching),
    ).toHaveLength(2);
    expect(
      custom.patches.filter((patch) => patch.fileKey === nested),
    ).toHaveLength(1);
    await expect(summary).toHaveText(
      "Code 6 · Ontology 2 · Docs 2 — 10 matching of 10 changed files",
    );
    await filterTypes(custom, { Code: 6, Ontology: 2, Docs: 2 }, 10);
    for (const name of ["Code", "Ontology", "Docs"])
      await control.getByRole("checkbox", { name, exact: true }).uncheck();
    const filteredReads = reads;
    await show.click();
    await expect(summary).toHaveText("0 matching of 10 changed files");
    await expect(page.getByTestId("source-control-file")).toHaveCount(0);
    expect(reads).toBe(filteredReads + 1);
    const privateAgain = await applyChecklist(page, menu, all);
    expect(privateAgain.private).toBe(true);
    await expect(summary).toHaveText("0 matching of 10 changed files");
    await runWorkspaceVmAction(page, ws.id, "stop");
    await waitForWorkspaceVmStatus(page, ws.id, "stopped");
    await expect(summary).toBeHidden();
    await expect(page.getByTestId("source-control-diff-stream")).toBeHidden();
  } finally {
    await actor.close();
  }
});
