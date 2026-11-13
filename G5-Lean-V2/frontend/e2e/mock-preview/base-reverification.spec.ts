import { expect, test } from "@playwright/test";

// Base re-verification from Home (ui g5.ui.home-reverification and its
// dialog; ontology_and_harness g5.reverification.workspace) in the
// frontend-only preview: off by default, an administrator turns the
// verification switch on in Admin, then the line reads the fixture's stale
// demo status, the dialog lists its pull requests and runs, and a full run
// lands on the new workspace's chat pre-typed with the run's prompt under the
// cost notice.

const HOME = "/g5labs/home";

test("Home shows Base's re-verification and a start lands on the seeded chat", async ({
  page,
}) => {
  // The routes this journey adds must all be answered (other preview gaps,
  // such as provider accounts, are not this journey's).
  const unhandled: string[] = [];
  page.on("console", (message) => {
    const text = message.text();
    if (
      text.includes("[MockFetch] No handler") &&
      /base-reverification|\/clone/.test(text)
    ) {
      unhandled.push(text);
    }
  });
  // Off by default: Home shows none of it.
  await page.goto(HOME);
  await expect(page.getByTestId("home-columns")).toBeVisible();
  await expect(page.getByTestId("home-reverification")).toHaveCount(0);

  // An administrator turns the switch on in Admin, then returns Home in the
  // app (a reload would reset the mock).
  await page.goto("/g5labs/base?view=admin");
  const toggle = page.getByTestId("org-admin-verification-action-switch");
  await expect(toggle).toBeVisible();
  await toggle.click();
  await expect(toggle).toBeChecked();
  await page.getByTestId("workspace-tab-home").click();

  const line = page.getByTestId("home-reverification-status");
  await expect(line).toHaveText(
    "Base has not been re-verified for 4 merged PRs (+3,410 −1,022 lines)",
  );
  // One line above the tiles.
  const lineBox = await line.boundingBox();
  const columnsBox = await page.getByTestId("home-columns").boundingBox();
  expect(lineBox!.y + lineBox!.height).toBeLessThanOrEqual(columnsBox!.y);

  await page.getByTestId("home-reverification-open").click();
  const dialog = page.getByRole("dialog", {
    name: "Re-run verification of Base",
  });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByTestId("home-reverification-pr")).toHaveCount(4);
  await expect(dialog.getByTestId("home-reverification-run")).toHaveCount(1);
  await expect(
    dialog.getByTestId("home-reverification-incremental"),
  ).toBeEnabled();

  await dialog.getByTestId("home-reverification-full").click();

  await expect(page).toHaveURL(
    /\/g5labs\/base-reverification-\d{4}-\d{2}-\d{2}\?view=chat$/,
  );
  const input = page.getByTestId("chat-input").first();
  await expect(input).toHaveValue(
    /^Base re-verification \(full\) for this workspace, which was forked from Base at g5-client@/,
  );
  // The bold notice speaks about Base, not a pull request.
  await expect(
    page.getByTestId("composer-seed-verification-notice").first(),
  ).toContainText("check Base's behaviour against its intent");

  // Back on Home (history, not a reload, which would reset the mock) the run
  // is in progress and the button greyed for everyone.
  await page.goBack();
  await expect(page.getByTestId("home-reverification-status")).toHaveText(
    /^Verification in progress in workspace base-reverification-\d{4}-\d{2}-\d{2} \(by Alex Chen\)$/,
  );
  await expect(page.getByTestId("home-reverification-open")).toBeDisabled();
  expect(unhandled).toEqual([]);
});
