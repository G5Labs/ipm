import { expect, test } from "@playwright/test";

for (const [name, initial] of [
  ["empty.md", ""],
  ["request.md", "[I1. Uploaded intent]\r\nOriginal café 中文 😀.\r\n"],
  ["legacy.semlift.md", "[D1. Historical design]\nKeep this content.\n"],
]) {
  test(`${name}: upload, Preview, Source, edit, Save and reload without review metadata`, async ({
    page,
  }) => {
    let content = initial;
    let hash = "uploaded";
    const path = `docs/workspace/${name}`;
    const obsolete: string[] = [];
    const errors: string[] = [];
    page.on("pageerror", (error) => errors.push(error.message));
    await page.route("**/api/**", async (route) => {
      const request = route.request();
      const url = new URL(request.url());
      if (/\/grounding\/review-(metadata|actions)/.test(url.pathname)) {
        obsolete.push(request.url());
        await route.fulfill({
          status: 404,
          json: { error: { message: "Resource not found" } },
        });
      } else if (
        request.method() === "POST" &&
        url.pathname.endsWith("/docs")
      ) {
        const upload = request.postDataJSON();
        expect(upload.content).toBe(initial);
        expect(upload.filePath).toBe(path);
        await route.fulfill({
          status: 201,
          json: {
            data: {
              id: "doc",
              workspaceId: "workspace",
              name,
              filePath: path,
              repoId: "docs",
              docType: "design",
              content,
            },
          },
        });
      } else if (url.pathname.includes("/files/")) {
        if (request.method() === "PUT") {
          const saved = request.postDataJSON();
          expect(saved.expectedHash).toBe(hash);
          expect(saved.expectedVmId).toBe("vm");
          content = saved.content;
          hash = "saved";
        }
        await route.fulfill({
          json: {
            data: {
              path,
              content,
              hash,
              size: new TextEncoder().encode(content).length,
              encoding: "utf-8",
              source: "live",
              repoOid: null,
              snapshotAt: null,
              vmId: "vm",
            },
          },
        });
      } else {
        await route.fulfill({ json: { data: [] } });
      }
    });
    await page.goto("/e2e/browser/project-doc.html");
    await page.getByLabel("Upload a doc", { exact: true }).setInputFiles({
      name,
      mimeType: "text/markdown",
      buffer: Buffer.from(initial),
    });
    await page.getByTestId("create-doc-submit").click();
    const view = page.getByTestId("file-view");
    await expect(view.getByTestId("document-preview")).toBeVisible();
    const expectFilters = async () => {
      for (const id of ["toolbar-tag-human", "toolbar-tag-uncertainty"])
        await expect(view.getByTestId(id)).toHaveCount(initial ? 1 : 0);
    };
    await expectFilters();
    await expect(view.getByText(/Review metadata unavailable/)).toHaveCount(0);
    await expect(view.getByTestId("toolbar-tag-unimplemented")).toHaveCount(0);
    await view.getByTestId("file-view-edit").click();
    // Edit opens Rich text; this journey types exact plain text.
    await view.getByTestId("file-view-switch-mode").click();
    const lines = view.locator(".monaco-editor .view-lines").first();
    await expect(lines).toBeVisible();
    await lines.click();
    await page.keyboard.press("ControlOrMeta+End");
    const addition =
      "Typed **content** with  spaces and Unicode: café 中文 😀.";
    await page.keyboard.insertText(addition);
    // Done returns to reading with the unsaved text; Save stays offered there.
    await view.getByTestId("file-view-done").click();
    await expect(view.getByTestId("document-preview")).toContainText(
      "Typed content",
    );
    await expectFilters();
    await view.getByTestId("code-editor-save-button").click();
    await expect.poll(() => content).toBe(initial + addition);
    await page.reload();
    await expect(view.getByTestId("document-preview")).toContainText(
      "Typed content",
    );
    await expectFilters();
    await view.getByTestId("file-view-switch-mode").click();
    await expect(
      view.locator(".monaco-editor .view-lines").first(),
    ).toContainText("Typed **content**");
    expect(obsolete).toEqual([]);
    expect(errors).toEqual([]);
  });
}
