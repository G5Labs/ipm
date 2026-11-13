import { expect, test, type Page } from "@playwright/test";
import {
  TEST_ORG,
  TEST_WORKSPACE,
  authedApi,
  captureChatTurn,
  createWorkspaceClone,
  deleteWorkspaceById,
  navigateToWorkspace,
  openChatComposer,
  openWorkspacePath,
  providerSettingsGear,
  readCurrentOrgId,
  sendChatMessage,
  type ChatTurnIdentity,
  type WorkspaceClone,
  waitForDirectAgentLaunchChoices,
  waitForWorkspaceLoad,
} from "../helpers";

// Provider accounts browser E2E — test_contract/provider_accounts I6
// (settings-page D2/D16; external_agent_provider_contract D14;
// secret_custody_and_redaction D9). The Settings Accounts section lists named
// provider accounts as expandable rows, creates one with the raw key never
// reaching the DOM, edits it in place sending only the changed field, keeps
// the stored credential on a blank Save, and refuses removal while assigned.
//
// Gate: the provider-accounts kill-switch is DEFAULT-ON server-side
// (`G5_ENABLE_PROVIDER_ACCOUNTS`), but this spec runs only in lanes that
// declare the stack under test has it on (`G5_E2E_PROVIDER_ACCOUNTS_ENABLED`).
// Step 7 (section absent while the switch is off, account still exists via
// the API) is a separate describe gated on
// `G5_E2E_PROVIDER_ACCOUNTS_DISABLED=true`.
// No LLM key is needed: the credential is a shaped test string.
const PROVIDER_ACCOUNTS_E2E_ENABLED =
  process.env.G5_E2E_PROVIDER_ACCOUNTS_ENABLED === "true";
const PROVIDER_ACCOUNTS_E2E_DISABLED =
  process.env.G5_E2E_PROVIDER_ACCOUNTS_DISABLED === "true";

// Eligible (≥ 24 chars, single line) Anthropic-API-key-shaped test value.
const TEST_KEY = "sk-ant-api03-e2e-provider-account-0123456789";
/** A Claude subscription account takes a `claude setup-token` value, not an
 *  API key (contract `validate_secret`). */
const TEST_OAUTH = "sk-ant-oat01-e2e-provider-account-0123456789";
const secretFor = (kind: string, name: string): string =>
  `${kind === "claude_subscription" ? TEST_OAUTH : TEST_KEY}-${name}`;
const HINT_PREFIX = TEST_KEY.slice(0, 12);
const KEY_REMAINDER = TEST_KEY.slice(12);

interface AccountRow {
  id: string;
  name: string;
  scope: string;
}

function accountsPath(orgId: string, workspaceId?: string): string {
  const base = `/api/v1/orgs/${orgId}/provider-accounts`;
  return workspaceId ? `${base}?workspaceId=${workspaceId}` : base;
}

async function listAccounts(
  page: Page,
  orgId: string,
  workspaceId?: string,
): Promise<AccountRow[]> {
  const result = await authedApi(page, accountsPath(orgId, workspaceId), "GET");
  expect(result.status).toBe(200);
  const rows = (result.body?.data ?? result.body ?? []) as AccountRow[];
  return Array.isArray(rows) ? rows : [];
}

async function deleteAccountsNamed(
  page: Page,
  orgId: string,
  namePrefix: string,
): Promise<void> {
  const rows = await listAccounts(page, orgId).catch(() => []);
  for (const row of rows) {
    if (row.name.startsWith(namePrefix)) {
      await authedApi(
        page,
        `/api/v1/orgs/${orgId}/provider-accounts/${row.id}`,
        "DELETE",
      ).catch(() => {});
    }
  }
}

// A seeded account becomes the person's default for its agent (chat
// g5.chat.account-defaults), so both keys are shared-tenant state to clear.
async function clearClaudeAssignment(page: Page, orgId: string): Promise<void> {
  await authedApi(page, `/api/v1/users/me/config?orgId=${orgId}`, "PUT", {
    removeSettingKeys: ["claude_account_id", "codex_account_id"],
  }).catch(() => {});
}

// Legacy Claude provider keys the resolver would still use without an
// account (settings-page D19): while the effective config carries one, the
// account select's unassigned option reads "Runtime variables"; otherwise it
// reads "No account". The stack under test may or may not carry them, so the
// expected label is derived from the effective workspace config.
const LEGACY_CLAUDE_SECRETS = [
  "ANTHROPIC_AUTH_TOKEN",
  "CLAUDE_CODE_OAUTH_TOKEN",
  "ANTHROPIC_API_KEY",
];

async function claudeUnassignedLabel(
  page: Page,
  workspaceId: string,
  orgId: string,
): Promise<"Runtime variables" | "No account"> {
  const result = await authedApi(
    page,
    `/api/v1/workspaces/${workspaceId}/config?orgId=${orgId}`,
    "GET",
  );
  expect(result.status).toBe(200);
  const data = (result.body?.data ?? result.body ?? {}) as {
    effectiveSecretKeys?: string[];
    effectiveSettings?: Record<string, unknown>;
  };
  // Precondition: no Org/Workspace assignment shadows the user scope — with
  // an inherited assignment the unassigned option is omitted or reads
  // "Use <scope> default" instead (settings-page D17).
  expect(
    data.effectiveSettings?.claude_account_id,
    "fixture org/workspace must not assign a Claude account",
  ).toBeUndefined();
  const secrets = data.effectiveSecretKeys ?? [];
  const baseUrl = data.effectiveSettings?.ANTHROPIC_BASE_URL;
  const legacy =
    LEGACY_CLAUDE_SECRETS.some((key) => secrets.includes(key)) ||
    (typeof baseUrl === "string" && baseUrl.trim() !== "");
  return legacy ? "Runtime variables" : "No account";
}

async function openSettings(page: Page, workspace: WorkspaceClone): Promise<void> {
  await openWorkspacePath(page, `${workspace.path}?view=settings`);
  await expect(page.getByTestId("workspace-settings-page")).toBeVisible({
    timeout: 30_000,
  });
}

function sectionHeading(page: Page, name: string) {
  return page.getByRole("heading", { level: 2, name, exact: true });
}

async function assertBefore(
  page: Page,
  first: string,
  second: string,
): Promise<void> {
  const firstBox = await sectionHeading(page, first).boundingBox();
  const secondBox = await sectionHeading(page, second).boundingBox();
  expect(firstBox, `${first} heading box`).not.toBeNull();
  expect(secondBox, `${second} heading box`).not.toBeNull();
  expect(firstBox!.y).toBeLessThan(secondBox!.y);
}

test.describe.serial("Provider accounts — Settings Accounts section", () => {
  test.describe.configure({ timeout: 240_000 });
  test.skip(
    !PROVIDER_ACCOUNTS_E2E_ENABLED,
    "Provider accounts flow is opt-in (set G5_E2E_PROVIDER_ACCOUNTS_ENABLED=true against a stack with the switch on)",
  );

  const NAME_PREFIX = "e2e-acct";
  let workspace: WorkspaceClone | null = null;
  let orgId = "";

  test.afterEach(async ({ page }) => {
    if (orgId) {
      await clearClaudeAssignment(page, orgId);
      await deleteAccountsNamed(page, orgId, NAME_PREFIX);
    }
    if (workspace) {
      await deleteWorkspaceById(page, workspace.id);
      workspace = null;
    }
  });

  test("lists, creates, edits in place, and removes a user account", async ({
    page,
  }) => {

    // Console sweep: the raw key must never be logged either (I6 step 3).
    const consoleMessages: string[] = [];
    page.on("console", (msg) => consoleMessages.push(msg.text()));

    workspace = await createWorkspaceClone(page, {
      org: TEST_ORG,
      workspace: TEST_WORKSPACE,
      namePrefix: "provider-accounts",
      description: "Provider accounts Settings section browser E2E clone",
    });
    await navigateToWorkspace(page, TEST_ORG, workspace.name);
    await waitForWorkspaceLoad(page);
    orgId = await readCurrentOrgId(page);
    await deleteAccountsNamed(page, orgId, NAME_PREFIX);

    // 1. Section order: Appearance, Accounts, Defaults, Editor, Access,
    //    Notifications, Execution policy (ui g5.ui.settings-page; Accounts
    //    second on purpose). The index on the left lists the same sections.
    await openSettings(page, workspace);
    const section = page.getByTestId("provider-accounts-section");
    await expect(section).toBeVisible({ timeout: 30_000 });
    await assertBefore(page, "Appearance", "Accounts");
    await assertBefore(page, "Accounts", "Defaults");
    await assertBefore(page, "Defaults", "Editor");
    await assertBefore(page, "Editor", "Access");
    await assertBefore(page, "Access", "Notifications");
    await assertBefore(page, "Notifications", "Execution policy");
    await expect(
      page.getByTestId("settings-index").getByRole("button"),
    ).toHaveText([
      "Appearance",
      "Accounts",
      "Defaults",
      "Editor",
      "Access",
      "Notifications",
      "Execution policy",
    ]);
    // The index follows the scroll and a click scrolls to the section.
    await page.getByTestId("settings-index-access").click();
    await expect(page).toHaveURL(/section=access/);
    await expect(page.getByTestId("settings-index-access")).toHaveAttribute(
      "aria-current",
      "location",
    );
    await expect(sectionHeading(page, "Provider runtime")).toHaveCount(0);
    // The Accounts section is the list only: no assignment control there.
    await expect(
      section.getByTestId("provider-account-assignment-claude_code"),
    ).toHaveCount(0);

    // 2. "+ Add account": name, kind anthropic_api_key, eligible key, scope
    //    You, Save.
    const accountName = `${NAME_PREFIX}-${Date.now()}`;
    await page.getByTestId("provider-account-add").click();
    const nameInput = page.getByTestId("provider-account-name-input-new");
    await expect(nameInput).toBeFocused();
    await nameInput.fill(accountName);
    await page.getByTestId("provider-account-kind-select").click();
    await page
      .getByTestId("provider-account-kind-select-option-anthropic_api_key")
      .click();
    await page.getByTestId("provider-account-secret-input-new").fill(TEST_KEY);
    await page.getByTestId("provider-account-scope-select").click();
    await page.getByTestId("provider-account-scope-select-option-user").click();

    const createResponsePromise = page.waitForResponse(
      (r) =>
        r.url().includes("/provider-accounts") &&
        r.request().method() === "POST",
      { timeout: 30_000 },
    );
    await page.getByTestId("provider-account-save-new").click();
    const createResponse = await createResponsePromise;
    expect(createResponse.status()).toBe(201);
    expect(createResponse.headers()["cache-control"]).toBe("no-store");
    const createPayload = JSON.stringify(await createResponse.json());
    expect(createPayload).toContain(HINT_PREFIX);
    expect(createPayload).not.toContain(KEY_REMAINDER);

    // 3. Collapsed row: name, "API" type pill, "You" scope pill, 12-char
    //    hint; the raw key appears nowhere in the DOM or console.
    const created = (await listAccounts(page, orgId, workspace.id)).find(
      (row) => row.name === accountName,
    );
    expect(created, "created account visible through the API").toBeTruthy();
    const id = created!.id;
    const row = page.getByTestId(`provider-account-row-${id}`);
    await expect(row).toBeVisible({ timeout: 30_000 });
    await expect(page.getByTestId(`provider-account-name-${id}`)).toHaveText(
      accountName,
    );
    await expect(page.getByTestId(`provider-account-type-pill-${id}`)).toHaveText(
      "API",
    );
    await expect(page.getByTestId(`provider-account-scope-pill-${id}`)).toHaveText(
      "You",
    );
    await expect(page.getByTestId(`provider-account-hint-${id}`)).toHaveText(
      `${HINT_PREFIX}…`,
    );
    await expect(page.locator("body")).not.toContainText(KEY_REMAINDER);
    expect(consoleMessages.join("\n")).not.toContain(KEY_REMAINDER);

    // 4. Expand, rename, header updates live, Save sends only the name.
    await page.getByTestId(`provider-account-toggle-${id}`).click();
    const renamed = `${accountName}-renamed`;
    const editName = page.getByTestId(`provider-account-name-input-${id}`);
    await expect(editName).toBeVisible({ timeout: 15_000 });
    await editName.fill(renamed);
    await expect(page.getByTestId(`provider-account-name-${id}`)).toHaveText(
      renamed,
    );
    const renameRequestPromise = page.waitForRequest(
      (r) =>
        r.url().includes(`/provider-accounts/${id}`) &&
        r.method() === "PATCH",
      { timeout: 30_000 },
    );
    await page.getByTestId(`provider-account-save-${id}`).click();
    const renameRequest = await renameRequestPromise;
    expect(renameRequest.postDataJSON()).toEqual({ name: renamed });
    await expect(page.getByTestId(`provider-account-drawer-${id}`)).toHaveCount(
      0,
      { timeout: 30_000 },
    );
    await expect(page.getByTestId(`provider-account-name-${id}`)).toHaveText(
      renamed,
    );

    // 5. Expand again, leave the credential blank, change the base URL, Save:
    //    no `secret` is sent and the hint is unchanged.
    await page.getByTestId(`provider-account-toggle-${id}`).click();
    const secretInput = page.getByTestId(`provider-account-secret-input-${id}`);
    await expect(secretInput).toBeVisible({ timeout: 15_000 });
    await expect(secretInput).toHaveValue("");
    await expect(secretInput).toHaveAttribute(
      "placeholder",
      new RegExp(`^${HINT_PREFIX}`),
    );
    await page
      .getByTestId(`provider-account-base-url-${id}`)
      .fill("https://api.anthropic.com");
    const blankRequestPromise = page.waitForRequest(
      (r) =>
        r.url().includes(`/provider-accounts/${id}`) &&
        r.method() === "PATCH",
      { timeout: 30_000 },
    );
    await page.getByTestId(`provider-account-save-${id}`).click();
    const blankRequest = await blankRequestPromise;
    const blankBody = blankRequest.postDataJSON() as Record<string, unknown>;
    expect(blankBody).not.toHaveProperty("secret");
    expect(blankBody).toEqual({ baseUrl: "https://api.anthropic.com" });
    await expect(page.getByTestId(`provider-account-hint-${id}`)).toHaveText(
      `${HINT_PREFIX}…`,
    );

    // 6. The person's first Claude account became their Claude Code default
    //    on creation (chat g5.chat.account-defaults): the user config names
    //    it and the row carries the Default pill. Its own scope's assignment
    //    does not block Remove; the removal clears it with the row.
    const defaulted = await authedApi(
      page,
      `/api/v1/users/me/config?orgId=${orgId}`,
      "GET",
    );
    expect(defaulted.status).toBe(200);
    expect(defaulted.body?.data?.settings?.claude_account_id).toBe(id);
    await expect(
      page.getByTestId(`provider-account-default-pill-${id}-claude_code`),
    ).toHaveText("Default · Claude Code", { timeout: 30_000 });
    await page.getByTestId(`provider-account-toggle-${id}`).click();
    const remove = page.getByTestId(`provider-account-remove-${id}`);
    await expect(remove).toBeEnabled({ timeout: 15_000 });
    const deleteResponsePromise = page.waitForResponse(
      (r) =>
        r.url().includes(`/provider-accounts/${id}`) &&
        r.request().method() === "DELETE",
      { timeout: 30_000 },
    );
    await remove.click();
    expect((await deleteResponsePromise).status()).toBe(204);
    await expect(page.getByTestId(`provider-account-row-${id}`)).toHaveCount(0, {
      timeout: 30_000,
    });
    expect(
      (await listAccounts(page, orgId, workspace.id)).some((r) => r.id === id),
    ).toBe(false);
    const released = await authedApi(
      page,
      `/api/v1/users/me/config?orgId=${orgId}`,
      "GET",
    );
    expect(released.body?.data?.settings ?? {}).not.toHaveProperty(
      "claude_account_id",
    );
  });
});

// I6 step 7: run against a stack started with G5_ENABLE_PROVIDER_ACCOUNTS=false
// — the section is absent while a previously created account still exists
// through the API (the kill-switch never deletes; contract D14).
test.describe("Provider accounts — kill-switch off hides the section only", () => {
  test.describe.configure({ timeout: 180_000 });
  test.skip(
    !PROVIDER_ACCOUNTS_E2E_DISABLED,
    "Switch-off flow is opt-in (set G5_E2E_PROVIDER_ACCOUNTS_DISABLED=true against a stack started with G5_ENABLE_PROVIDER_ACCOUNTS=false)",
  );

  const NAME_PREFIX = "e2e-acct-off";
  let workspace: WorkspaceClone | null = null;
  let orgId = "";

  test.afterEach(async ({ page }) => {
    if (orgId) await deleteAccountsNamed(page, orgId, NAME_PREFIX);
    if (workspace) {
      await deleteWorkspaceById(page, workspace.id);
      workspace = null;
    }
  });

  test("section absent, account still exists through the API", async ({
    page,
  }) => {
    workspace = await createWorkspaceClone(page, {
      org: TEST_ORG,
      workspace: TEST_WORKSPACE,
      namePrefix: "provider-accounts-off",
      description: "Provider accounts kill-switch-off browser E2E clone",
    });
    await navigateToWorkspace(page, TEST_ORG, workspace.name);
    await waitForWorkspaceLoad(page);
    orgId = await readCurrentOrgId(page);

    // The API keeps working while the UI is hidden.
    const accountName = `${NAME_PREFIX}-${Date.now()}`;
    const created = await authedApi(page, accountsPath(orgId), "POST", {
      name: accountName,
      kind: "anthropic_api_key",
      scope: "user",
      secret: TEST_KEY,
    });
    expect(created.status).toBe(201);

    await openSettings(page, workspace);
    await expect(sectionHeading(page, "Defaults")).toBeVisible({
      timeout: 30_000,
    });
    await expect(sectionHeading(page, "Access")).toBeVisible();
    await expect(sectionHeading(page, "Accounts")).toHaveCount(0);
    await expect(page.getByTestId("provider-accounts-section")).toHaveCount(0);
    // The kill-switch also hides the account selects in Defaults (D14/D19).
    await expect(
      page.getByTestId("runtime-env-default-model-claude_code"),
    ).toBeVisible();
    await expect(
      page.getByTestId("provider-account-assignment-claude_code"),
    ).toHaveCount(0);

    const rows = await listAccounts(page, orgId);
    expect(rows.some((row) => row.name === accountName)).toBe(true);
  });
});

// I7: per-agent assignment controls persist immediately and show the
// resolved account (settings-page D17 hosted in the D19 Defaults card;
// external_agent_provider_contract D14, D11; config_safe_defaults D5). Two
// Claude-family accounts are seeded through the API; the Claude Code row's
// account select in Settings → Defaults lists both plus the unassigned option
// ("Runtime variables" while legacy keys exist, else "No account") while the
// Codex row's account select lists Codex-family accounts only; selecting
// persists a user-scope
// `claude_account_id` write and the resolution line reflects the backend
// resolution; the unassigned option removes the key; changing the Claude
// Code default model in the same row leaves the assignment untouched.
test.describe.serial("Provider accounts — per-agent assignment controls", () => {
  test.describe.configure({ timeout: 240_000 });
  test.skip(
    !PROVIDER_ACCOUNTS_E2E_ENABLED,
    "Provider accounts flow is opt-in (set G5_E2E_PROVIDER_ACCOUNTS_ENABLED=true against a stack with the switch on)",
  );

  const NAME_PREFIX = "e2e-assign";
  let workspace: WorkspaceClone | null = null;
  let orgId = "";

  test.afterEach(async ({ page }) => {
    if (orgId) {
      // User scope is shared-tenant state across specs: always clear it.
      await clearClaudeAssignment(page, orgId);
      await deleteAccountsNamed(page, orgId, NAME_PREFIX);
    }
    if (workspace) {
      await deleteWorkspaceById(page, workspace.id);
      workspace = null;
    }
  });

  test("assigns an account, shows the resolved route, clears back to runtime variables", async ({
    page,
  }) => {

    workspace = await createWorkspaceClone(page, {
      org: TEST_ORG,
      workspace: TEST_WORKSPACE,
      namePrefix: "provider-accounts-assign",
      description: "Provider accounts assignment controls browser E2E clone",
    });
    await navigateToWorkspace(page, TEST_ORG, workspace.name);
    await waitForWorkspaceLoad(page);
    orgId = await readCurrentOrgId(page);
    await clearClaudeAssignment(page, orgId);
    await deleteAccountsNamed(page, orgId, NAME_PREFIX);

    // The unassigned label is derived before any account exists: seeding
    // makes the first Claude account the person's default (chat
    // g5.chat.account-defaults), which the precondition would otherwise see.
    const unassignedLabel = await claudeUnassignedLabel(
      page,
      workspace.id,
      orgId,
    );

    // Seed two Claude-family accounts through the API; one Codex-family
    // account proves the family filter.
    const stamp = Date.now();
    const firstName = `${NAME_PREFIX}-first-${stamp}`;
    const secondName = `${NAME_PREFIX}-second-${stamp}`;
    const codexName = `${NAME_PREFIX}-codex-${stamp}`;
    for (const [name, kind] of [
      [firstName, "anthropic_api_key"],
      [secondName, "claude_subscription"],
    ] as const) {
      const created = await authedApi(page, accountsPath(orgId), "POST", {
        name,
        kind,
        scope: "user",
        secret: secretFor(kind, name),
      });
      expect(created.status, `${kind} account created`).toBe(201);
    }
    const codexCreated = await authedApi(page, accountsPath(orgId), "POST", {
      name: codexName,
      kind: "chatgpt_subscription",
      scope: "user",
    });
    expect(codexCreated.status).toBe(201);
    const rows = await listAccounts(page, orgId, workspace.id);
    const first = rows.find((r) => r.name === firstName);
    const second = rows.find((r) => r.name === secondName);
    const codex = rows.find((r) => r.name === codexName);
    expect(first && second && codex, "seeded accounts visible").toBeTruthy();

    // 1. The Claude Code row of Settings → Defaults hosts the account select:
    //    it shows the first account, the default its creation made it, and
    //    lists both Claude accounts plus the unassigned option; the Codex
    //    row's select lists Codex-family accounts only.
    await openSettings(page, workspace);
    const claudeRow = page.getByTestId("runtime-env-default-model-claude_code");
    await expect(claudeRow).toBeVisible({ timeout: 30_000 });
    await expect(
      claudeRow.getByTestId("provider-account-assignment-claude_code"),
    ).toBeVisible({ timeout: 30_000 });
    // No assignment group in the Accounts section any more (D16).
    await expect(
      page
        .getByTestId("provider-accounts-section")
        .getByTestId("provider-account-assignment-claude_code"),
    ).toHaveCount(0);
    const claudeSelect = claudeRow.getByTestId(
      "provider-account-assignment-select-claude_code",
    );
    await expect(claudeSelect).toBeEnabled({ timeout: 30_000 });
    await expect(claudeSelect).toContainText(firstName);
    await claudeSelect.click();
    await expect(
      page.getByTestId(
        "provider-account-assignment-select-claude_code-option-empty",
      ),
    ).toHaveText(unassignedLabel);
    await expect(
      page.getByTestId(
        `provider-account-assignment-select-claude_code-option-${first!.id}`,
      ),
    ).toContainText(firstName);
    await expect(
      page.getByTestId(
        `provider-account-assignment-select-claude_code-option-${second!.id}`,
      ),
    ).toContainText(secondName);
    await expect(
      page.getByTestId(
        `provider-account-assignment-select-claude_code-option-${codex!.id}`,
      ),
    ).toHaveCount(0);
    await page.keyboard.press("Escape");

    {
      const codexRow = page.getByTestId("runtime-env-default-model-codex");
      await expect(codexRow).toBeVisible();
      const codexSelect = codexRow.getByTestId(
        "provider-account-assignment-select-codex",
      );
      await codexSelect.click();
      await expect(
        page.getByTestId(
          `provider-account-assignment-select-codex-option-${codex!.id}`,
        ),
      ).toContainText(codexName);
      await expect(
        page.getByTestId(
          `provider-account-assignment-select-codex-option-${first!.id}`,
        ),
      ).toHaveCount(0);
      await page.keyboard.press("Escape");
    }

    // 2. Select the second account: a user-scope write carrying
    //    `claude_account_id`; the select shows the account and the persisted
    //    user config carries it. The row renders no source badge, scope
    //    pill, Override button, or resolution line (settings-page D17).
    const assignRequestPromise = page.waitForRequest(
      (r) => r.url().includes("/users/me/config") && r.method() === "PUT",
      { timeout: 30_000 },
    );
    await claudeSelect.click();
    await page
      .getByTestId(
        `provider-account-assignment-select-claude_code-option-${second!.id}`,
      )
      .click();
    const assignRequest = await assignRequestPromise;
    expect(assignRequest.postDataJSON()).toEqual({
      settings: { claude_account_id: second!.id },
    });
    await expect(claudeSelect).toContainText(secondName);
    await expect(claudeRow.getByText("Default account")).toBeVisible();
    for (const retired of [
      "provider-account-assignment-source-claude_code",
      "provider-account-assignment-pill-claude_code-org",
      "provider-account-assignment-pill-claude_code-workspace",
      "provider-account-assignment-pill-claude_code-user",
      "provider-account-assignment-override-claude_code",
      "provider-account-resolution-claude_code",
    ]) {
      await expect(claudeRow.getByTestId(retired)).toHaveCount(0);
    }
    await expect(claudeRow).not.toContainText("resolves to");
    await expect(claudeRow).not.toContainText(
      "does not change the default model",
    );
    const assignResponse = await assignRequest.response();
    expect(assignResponse?.status()).toBe(200);
    const persisted = await authedApi(
      page,
      `/api/v1/users/me/config?orgId=${orgId}`,
      "GET",
    );
    expect(persisted.status).toBe(200);
    expect(persisted.body?.data?.settings?.claude_account_id).toBe(second!.id);

    // 3. The unassigned option removes the key through `removeSettingKeys`.
    const clearRequestPromise = page.waitForRequest(
      (r) => r.url().includes("/users/me/config") && r.method() === "PUT",
      { timeout: 30_000 },
    );
    await claudeSelect.click();
    await page
      .getByTestId("provider-account-assignment-select-claude_code-option-empty")
      .click();
    const clearRequest = await clearRequestPromise;
    expect(clearRequest.postDataJSON()).toEqual({
      removeSettingKeys: ["claude_account_id"],
    });
    await expect(claudeSelect).toContainText(unassignedLabel);
    const clearResponse = await clearRequest.response();
    expect(clearResponse?.status()).toBe(200);
    const cleared = await authedApi(
      page,
      `/api/v1/users/me/config?orgId=${orgId}`,
      "GET",
    );
    expect(cleared.body?.data?.settings ?? {}).not.toHaveProperty(
      "claude_account_id",
    );

    // 4. Changing the Claude Code default model in the same Defaults row
    //    leaves the assignment untouched (assign again first so there is one).
    const reassignRequestPromise = page.waitForRequest(
      (r) => r.url().includes("/users/me/config") && r.method() === "PUT",
      { timeout: 30_000 },
    );
    await claudeSelect.click();
    await page
      .getByTestId(
        `provider-account-assignment-select-claude_code-option-${first!.id}`,
      )
      .click();
    const reassignResponse = await (await reassignRequestPromise).response();
    expect(reassignResponse?.status()).toBe(200);
    await expect(claudeSelect).toContainText(firstName);
    const modelSelect = claudeRow.getByTestId(
      "runtime-env-default-model-select-claude_code",
    );
    const currentModel = await modelSelect.inputValue();
    const otherModel = await modelSelect
      .locator("option")
      .evaluateAll(
        (options, current) =>
          options
            .map((o) => (o as HTMLOptionElement).value)
            .find((v) => v && v !== current && v !== "__other__") ?? "",
        currentModel,
      );
    expect(otherModel, "an alternative Claude model option").not.toBe("");
    await modelSelect.selectOption(otherModel);
    await expect(modelSelect).toHaveValue(otherModel);
    await expect(claudeSelect).toContainText(firstName);
    const afterModel = await authedApi(
      page,
      `/api/v1/users/me/config?orgId=${orgId}`,
      "GET",
    );
    expect(afterModel.body?.data?.settings?.claude_account_id).toBe(first!.id);
  });
});

// I11 steps 3 and 5, browser half (settings-page D18; contract D13): an
// API-key row renders no usage rings, and a Claude subscription row renders
// the 5h / 7d usage cells exactly when the accounts-list DTO carries `usage`.
// The snapshot is captured passively from OAuth responses, so the browser
// cannot seed one; the API answer is the oracle for the subscription row.
test.describe.serial("Provider accounts — usage rings", () => {
  test.describe.configure({ timeout: 180_000 });
  test.skip(
    !PROVIDER_ACCOUNTS_E2E_ENABLED,
    "Provider accounts flow is opt-in (set G5_E2E_PROVIDER_ACCOUNTS_ENABLED=true against a stack with the switch on)",
  );

  const NAME_PREFIX = "e2e-usage";
  let workspace: WorkspaceClone | null = null;
  let orgId = "";

  test.afterEach(async ({ page }) => {
    if (orgId) {
      await clearClaudeAssignment(page, orgId);
      await deleteAccountsNamed(page, orgId, NAME_PREFIX);
    }
    if (workspace) {
      await deleteWorkspaceById(page, workspace.id);
      workspace = null;
    }
  });

  test("API-key rows show no rings; subscription rows mirror the list DTO usage", async ({
    page,
  }) => {

    workspace = await createWorkspaceClone(page, {
      org: TEST_ORG,
      workspace: TEST_WORKSPACE,
      namePrefix: "provider-accounts-usage",
      description: "Provider accounts usage rings browser E2E clone",
    });
    await navigateToWorkspace(page, TEST_ORG, workspace.name);
    await waitForWorkspaceLoad(page);
    orgId = await readCurrentOrgId(page);
    await deleteAccountsNamed(page, orgId, NAME_PREFIX);

    const stamp = Date.now();
    const apiName = `${NAME_PREFIX}-api-${stamp}`;
    const subName = `${NAME_PREFIX}-sub-${stamp}`;
    for (const [name, kind] of [
      [apiName, "anthropic_api_key"],
      [subName, "claude_subscription"],
    ] as const) {
      const created = await authedApi(page, accountsPath(orgId), "POST", {
        name,
        kind,
        scope: "user",
        secret: secretFor(kind, name),
      });
      expect(created.status, `${kind} account created`).toBe(201);
    }

    interface UsageRow extends AccountRow {
      usage: {
        fiveHourUtilization: number | null;
        sevenDayUtilization: number | null;
      } | null;
    }
    const listed = await authedApi(
      page,
      accountsPath(orgId, workspace.id),
      "GET",
    );
    expect(listed.status).toBe(200);
    const rows = (listed.body?.data ?? []) as UsageRow[];
    const apiRow = rows.find((r) => r.name === apiName);
    const subRow = rows.find((r) => r.name === subName);
    expect(apiRow && subRow, "seeded accounts visible").toBeTruthy();
    // Step 3: an API-key account never carries usage.
    expect(apiRow!.usage).toBeNull();

    await openSettings(page, workspace);
    const apiRowEl = page.getByTestId(`provider-account-row-${apiRow!.id}`);
    await expect(apiRowEl).toBeVisible({ timeout: 30_000 });
    await expect(
      apiRowEl.getByTestId(`provider-account-usage-${apiRow!.id}`),
    ).toHaveCount(0);
    await expect(apiRowEl.locator("svg[data-level]")).toHaveCount(0);
    await apiRowEl
      .getByTestId(`provider-account-toggle-${apiRow!.id}`)
      .click();
    await expect(
      page.getByTestId(`provider-account-usage-none-${apiRow!.id}`),
    ).toHaveText("No usage data");

    // Step 5 (browser half): the subscription row renders the usage cells
    // exactly when its list DTO carries a snapshot.
    const subRowEl = page.getByTestId(`provider-account-row-${subRow!.id}`);
    await expect(subRowEl).toBeVisible();
    const cells = subRowEl.getByTestId(`provider-account-usage-${subRow!.id}`);
    if (subRow!.usage) {
      await expect(cells).toBeVisible();
      for (const [tag, utilization] of [
        ["5h", subRow!.usage.fiveHourUtilization],
        ["7d", subRow!.usage.sevenDayUtilization],
      ] as const) {
        const cell = cells.getByTestId(
          `provider-account-usage-${tag}-${subRow!.id}`,
        );
        await expect(cell).toContainText(tag);
        const expected =
          utilization == null
            ? ""
            : String(Math.round(Math.min(1, Math.max(0, utilization)) * 100));
        await expect(cell).toHaveAttribute("data-percent", expected);
      }
    } else {
      await expect(cells).toHaveCount(0);
      await expect(subRowEl.locator("svg[data-level]")).toHaveCount(0);
    }
  });
});

// ---------------------------------------------------------------------------
// test_contract/provider_accounts I8 — the chat composer offers an Account
// field before the first turn and submits it explicitly (agent_interaction_ux
// D7; right-panel session_provider_controls D30; contract D11/D14).
// ---------------------------------------------------------------------------
test.describe.serial("Provider accounts — chat Account field", () => {
  test.describe.configure({ timeout: 300_000 });
  test.skip(
    !PROVIDER_ACCOUNTS_E2E_ENABLED,
    "Provider accounts flow is opt-in (set G5_E2E_PROVIDER_ACCOUNTS_ENABLED=true against a stack with the switch on)",
  );

  const NAME_PREFIX = "e2e-chat-account";
  let workspace: WorkspaceClone | null = null;
  let orgId = "";
  let turn: ChatTurnIdentity | null = null;

  test.afterEach(async ({ page }) => {
    if (turn) {
      // Release the worker slot before the accounts are removed.
      await authedApi(
        page,
        `/api/v1/sessions/${turn.sessionId}/messages/${turn.inputId}/stop`,
        "POST",
      ).catch(() => {});
      turn = null;
    }
    if (orgId) {
      await clearClaudeAssignment(page, orgId);
      await deleteAccountsNamed(page, orgId, NAME_PREFIX);
    }
    if (workspace) {
      await deleteWorkspaceById(page, workspace.id);
      workspace = null;
    }
  });

  test("shows the assigned account first, resolves a different pick before Send, keeps it across Escape, and freezes it on the session", async ({
    page,
  }) => {

    workspace = await createWorkspaceClone(page, {
      org: TEST_ORG,
      workspace: TEST_WORKSPACE,
      namePrefix: "provider-accounts-chat",
      description: "Provider accounts chat Account field browser E2E clone",
      start: true,
    });
    await navigateToWorkspace(page, TEST_ORG, workspace.name);
    await waitForWorkspaceLoad(page);
    orgId = await readCurrentOrgId(page);
    await clearClaudeAssignment(page, orgId);
    await deleteAccountsNamed(page, orgId, NAME_PREFIX);

    // Whether the composer may offer "Runtime variables" at all (chat
    // g5.chat.account-defaults): only while the stack carries raw Claude
    // keys. Read before seeding, which makes the first account the default.
    const rawKeysOffered =
      (await claudeUnassignedLabel(page, workspace.id, orgId)) ===
      "Runtime variables";

    // 1. Seed two user-scoped Claude accounts through the API and assign the
    //    first at user scope (creation already made it the default).
    const stamp = Date.now();
    const firstName = `${NAME_PREFIX}-first-${stamp}`;
    const secondName = `${NAME_PREFIX}-second-${stamp}`;
    for (const name of [firstName, secondName]) {
      const created = await authedApi(page, accountsPath(orgId), "POST", {
        name,
        kind: "anthropic_api_key",
        scope: "user",
        secret: `${TEST_KEY}-${name}`,
      });
      expect(created.status, `${name} created`).toBe(201);
    }
    const rows = await listAccounts(page, orgId, workspace.id);
    const first = rows.find((r) => r.name === firstName);
    const second = rows.find((r) => r.name === secondName);
    expect(first && second, "seeded accounts visible").toBeTruthy();
    const assigned = await authedApi(
      page,
      `/api/v1/users/me/config?orgId=${orgId}`,
      "PUT",
      { settings: { claude_account_id: first!.id } },
    );
    expect(assigned.status).toBe(200);

    // 2. A fresh new chat: the popover's Account field shows the assigned
    //    account selected, scope descriptions on both rows, and a "Runtime
    //    variables" option only while raw keys exist (with an account
    //    selected and no raw keys there is nothing to fall back to).
    await openWorkspacePath(page, workspace.path);
    await waitForWorkspaceLoad(page);
    await openChatComposer(page);
    await providerSettingsGear(page).click();
    await waitForDirectAgentLaunchChoices(page);
    const accountSelect = page.getByTestId("chat-account-select");
    await expect(accountSelect).toBeEnabled({ timeout: 30_000 });
    await expect(accountSelect).toContainText(firstName);
    // The composer pill's harness segment names the selected account
    // instead of "Claude Code" (session_provider_controls D30).
    await expect(page.getByTestId("claude-harness-control")).toHaveText(
      firstName,
    );
    await accountSelect.click();
    if (rawKeysOffered) {
      await expect(
        page.getByTestId("chat-account-select-option-runtime_config"),
      ).toHaveText("Runtime variables");
    } else {
      await expect(
        page.getByTestId("chat-account-select-option-runtime_config"),
      ).toHaveCount(0);
    }
    const firstOption = page.getByTestId(
      `chat-account-select-option-${first!.id}`,
    );
    const secondOption = page.getByTestId(
      `chat-account-select-option-${second!.id}`,
    );
    await expect(firstOption).toHaveAttribute("aria-selected", "true");
    await expect(secondOption).toHaveAttribute("aria-selected", "false");
    for (const option of [firstOption, secondOption]) {
      const describedBy = await option.getAttribute("aria-describedby");
      expect(describedBy, "scope description wired").toBeTruthy();
      await expect(page.locator(`#${describedBy}`)).toHaveText(
        "Anthropic API key · You",
      );
    }

    // 3. Pick the second account: Send stays disabled until the resolver
    //    confirms, and the confirmed resolution names the second account.
    const resolvePromise = page.waitForResponse(
      (r) =>
        r.url().includes("/config/agent-launch/resolve") &&
        r.request().method() === "POST" &&
        (r.request().postDataJSON()?.preferences?.account?.value?.accountId ??
          "") === second!.id,
      { timeout: 30_000 },
    );
    const chatInput = page.getByTestId("chat-input").first();
    await chatInput.fill("Reply with the single word OK.");
    await secondOption.click();
    await expect(accountSelect).toContainText(secondName);
    const resolveResponse = await resolvePromise;
    expect(resolveResponse.status()).toBe(200);
    const resolved = (await resolveResponse.json()) as {
      data?: { resolved?: { accountId?: string } };
    };
    expect(resolved.data?.resolved?.accountId).toBe(second!.id);
    await expect(page.getByTestId("chat-resolution-loading")).toHaveCount(0, {
      timeout: 15_000,
    });
    await expect(page.getByTestId("chat-resolution-error")).toHaveCount(0);
    await expect(page.getByTestId("chat-panel-no-session-send")).toBeEnabled({
      timeout: 15_000,
    });

    // 4. Escape only closes the popover; reopening shows the kept selection.
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("claude-settings-modal")).toHaveCount(0);
    await openChatComposer(page);
    await providerSettingsGear(page).click();
    await expect(page.getByTestId("chat-account-select")).toContainText(
      secondName,
    );
    await expect(page.getByTestId("claude-harness-control")).toHaveText(
      secondName,
    );
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("claude-settings-modal")).toHaveCount(0);

    // 5. Send: the session detail's frozen account equals the second account.
    const startResponsePromise = page.waitForResponse(
      (r) =>
        r.url().includes("/sessions/start") && r.request().method() === "POST",
      { timeout: 60_000 },
    );
    await sendChatMessage(page, "Reply with the single word OK.");
    const startResponse = await startResponsePromise;
    turn = await captureChatTurn(page, startResponse);
    await expect
      .poll(
        async () => {
          const detail = await authedApi(
            page,
            `/api/v1/sessions/${turn!.sessionId}`,
          );
          const body = (detail.body?.data ?? detail.body) as {
            account?: { id?: string } | null;
          };
          return body?.account?.id ?? "";
        },
        { timeout: 60_000 },
      )
      .toBe(second!.id);
  });
});
