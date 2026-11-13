import { expect, test, type Page } from "@playwright/test";
import {
  authedApi,
  createWorkspaceClone,
  deleteWorkspaceById,
  navigateToWorkspace,
  openWorkspacePath,
  readCurrentOrgId,
  TEST_ORG,
  TEST_WORKSPACE,
  waitForWorkspaceLoad,
  type WorkspaceClone,
} from "../helpers";

// Credential-visibility browser E2E — test_contract/credentials_visibility I2
// (settings-page D16/D17/D19; security_policy secret_custody_and_redaction
// D9): the server-provided 12-character beginning-of-key hint stays visible
// on the three surfaces that survive the Defaults simplification (chat
// 994e615b / a0e02bc4): an Accounts row (and its expanded credential
// placeholder) shows the account's `secretHint`, the Defaults card's Claude
// Code resolution line shows the winning legacy credential's hint with its
// scope badge when the route resolves from Runtime variables, and a custom
// write-only secret row in Other variables shows its hint per scope. The
// raw remainder never renders anywhere, the scoped-config responses stay
// `no-store` and hint-bounded, and console output stays remainder-free.
//
// The Accounts half runs only in lanes that declare the provider-accounts
// switch on (`G5_E2E_PROVIDER_ACCOUNTS_ENABLED`, same convention as
// provider-accounts.spec.ts); the Defaults resolution-line half always runs.
// No LLM key required — every value is a shaped test string, never used for
// a provider call.
const PROVIDER_ACCOUNTS_E2E_ENABLED =
  process.env.G5_E2E_PROVIDER_ACCOUNTS_ENABLED === "true";

// Eligible, subscription-OAuth-shaped test token (≥ 24 chars, single line)
// stored as the legacy ANTHROPIC_AUTH_TOKEN at the clone's workspace scope.
const HINT_TOKEN = "sk-ant-oat01-e2e-credentials-hint-0123456789";
const HINT_PREFIX = "sk-ant-oat01";
const HINT_REMAINDER = HINT_TOKEN.slice(12);
// Below the 24-char eligibility floor: saved, but must keep the masked display.
const SHORT_TOKEN = "sk-short-e2e";
// Eligible API-key-shaped value for the account row.
const ACCOUNT_KEY = "sk-ant-api03-e2e-credentials-hint-9876543210";
const ACCOUNT_HINT_PREFIX = ACCOUNT_KEY.slice(0, 12);
const ACCOUNT_KEY_REMAINDER = ACCOUNT_KEY.slice(12);
const ACCOUNT_NAME_PREFIX = "e2e-hint-acct";
// Custom write-only secret under test in Other variables (settings-page D5/D9).
const CUSTOM_KEY = "E2E_HINT_SECRET";
const CUSTOM_TOKEN = "ghp-e2e-credentials-hint-custom-0123456789abcdef";
const CUSTOM_PREFIX = CUSTOM_TOKEN.slice(0, 12);
const CUSTOM_REMAINDER = CUSTOM_TOKEN.slice(12);

interface AccountRow {
  id: string;
  name: string;
}

async function openSettings(page: Page, workspace: WorkspaceClone): Promise<void> {
  await openWorkspacePath(page, `${workspace.path}?view=settings`);
  await expect(page.getByTestId("workspace-settings-page")).toBeVisible({
    timeout: 30_000,
  });
  await expect(page.getByTestId("runtime-env-config")).toBeVisible({
    timeout: 30_000,
  });
}

async function clearClaudeAssignment(page: Page, orgId: string): Promise<void> {
  await authedApi(page, `/api/v1/users/me/config?orgId=${orgId}`, "PUT", {
    removeSettingKeys: ["claude_account_id"],
  }).catch(() => {});
}

async function deleteHintAccounts(page: Page, orgId: string): Promise<void> {
  const listed = await authedApi(
    page,
    `/api/v1/orgs/${orgId}/provider-accounts`,
    "GET",
  ).catch(() => null);
  const rows = (listed?.body?.data ?? []) as AccountRow[];
  for (const row of Array.isArray(rows) ? rows : []) {
    if (row.name.startsWith(ACCOUNT_NAME_PREFIX)) {
      await authedApi(
        page,
        `/api/v1/orgs/${orgId}/provider-accounts/${row.id}`,
        "DELETE",
      ).catch(() => {});
    }
  }
}

/** Write the legacy secret at the clone's workspace scope; returns the body. */
async function putWorkspaceSecret(
  page: Page,
  workspace: WorkspaceClone,
  orgId: string,
  value: string,
): Promise<string> {
  const result = await authedApi(
    page,
    `/api/v1/workspaces/${workspace.id}/config?orgId=${orgId}`,
    "PUT",
    { setSecrets: { ANTHROPIC_AUTH_TOKEN: value } },
  );
  expect(result.status, JSON.stringify(result.body).slice(0, 800)).toBe(200);
  return JSON.stringify(result.body);
}

// Owning-scenario state shared with the describe-block cleanup below.
let workspace: WorkspaceClone | null = null;
let orgId = "";

// The owning executable scenario for test_contract/credentials_visibility I2.
async function credentialsHintScenario(page: Page): Promise<void> {
  // The remainder must stay out of every browser-visible surface — not just
  // the DOM: collect console output for a final sweep, and inspect the
  // scoped-config responses directly (headers + payload).
  const consoleMessages: string[] = [];
  page.on("console", (msg) => consoleMessages.push(msg.text()));

  workspace = await createWorkspaceClone(page, {
    org: TEST_ORG,
    workspace: TEST_WORKSPACE,
    namePrefix: "credentials-hint",
    description: "Secret beginning-of-key hint browser E2E clone",
  });

  await navigateToWorkspace(page, TEST_ORG, workspace.name);
  await waitForWorkspaceLoad(page);
  orgId = await readCurrentOrgId(page);
  // A user-scope assignment would make the Claude route resolve through an
  // account instead of Runtime variables (shared-tenant state: always clear).
  await clearClaudeAssignment(page, orgId);
  await deleteHintAccounts(page, orgId);

  // 1. Account-row hint (settings-page D16; Accounts half, gated): create an
  //    anthropic_api_key account through the API with an eligible key. The
  //    create response is no-store and carries at most the 12-char hint; the
  //    collapsed row shows the hint, the expanded row's credential
  //    placeholder starts with it, and the remainder never reaches the DOM.
  let accountId: string | null = null;
  if (PROVIDER_ACCOUNTS_E2E_ENABLED) {
    const accountName = `${ACCOUNT_NAME_PREFIX}-${Date.now()}`;
    const createResponsePromise = page.waitForResponse(
      (r) =>
        r.url().includes("/provider-accounts") &&
        r.request().method() === "POST",
      { timeout: 30_000 },
    );
    const created = await authedApi(
      page,
      `/api/v1/orgs/${orgId}/provider-accounts`,
      "POST",
      {
        name: accountName,
        kind: "anthropic_api_key",
        scope: "user",
        secret: ACCOUNT_KEY,
      },
    );
    expect(created.status, JSON.stringify(created.body).slice(0, 800)).toBe(201);
    const createResponse = await createResponsePromise;
    expect(createResponse.headers()["cache-control"]).toBe("no-store");
    const createPayload = JSON.stringify(await createResponse.json());
    expect(createPayload).toContain(ACCOUNT_HINT_PREFIX);
    expect(createPayload).not.toContain(ACCOUNT_KEY_REMAINDER);
    accountId = (created.body?.data?.id ?? created.body?.id ?? null) as
      | string
      | null;
    expect(accountId, "created account id").toBeTruthy();

    await openSettings(page, workspace);
    const hintCell = page.getByTestId(`provider-account-hint-${accountId}`);
    await expect(hintCell).toHaveText(`${ACCOUNT_HINT_PREFIX}…`, {
      timeout: 30_000,
    });
    await page.getByTestId(`provider-account-toggle-${accountId}`).click();
    const secretInput = page.getByTestId(
      `provider-account-secret-input-${accountId}`,
    );
    await expect(secretInput).toBeVisible({ timeout: 15_000 });
    await expect(secretInput).toHaveValue("");
    await expect(secretInput).toHaveAttribute(
      "placeholder",
      new RegExp(`^${ACCOUNT_HINT_PREFIX}`),
    );
    await expect(page.locator("body")).not.toContainText(ACCOUNT_KEY_REMAINDER);
  }

  // 2. Legacy credential at the CLONE's workspace scope through the
  //    scoped-config API (the Defaults card renders no provider secret row
  //    any more — settings-page D19). Workspace scope keeps the mutation
  //    inside the disposable clone (deleted in afterEach) instead of
  //    touching the shared demo user's user-scope secrets. The write
  //    response must be no-store and carry at most the sanctioned hint.
  const putResponsePromise = page.waitForResponse(
    (r) =>
      r.url().includes(`/workspaces/${workspace!.id}/config`) &&
      r.request().method() === "PUT",
    { timeout: 30_000 },
  );
  const putPayload = await putWorkspaceSecret(page, workspace, orgId, HINT_TOKEN);
  const putResponse = await putResponsePromise;
  expect(putResponse.headers()["cache-control"]).toBe("no-store");
  expect(putPayload).toContain(HINT_PREFIX);
  expect(putPayload).not.toContain(HINT_REMAINDER);

  // 3. Settings → Defaults: no provider variable row renders and the Claude
  //    Code row carries no resolution line and no hint segment (the Defaults
  //    card shows no legacy credential hint — settings-page D17/D19, chat
  //    9275474a). The workspace-config GET the page issues obeys the same
  //    no-store contract.
  const getResponsePromise = page.waitForResponse(
    (r) =>
      r.url().includes(`/workspaces/${workspace!.id}/config`) &&
      !r.url().includes("/agent-launch/") &&
      r.request().method() === "GET",
    { timeout: 30_000 },
  );
  await openSettings(page, workspace);
  const getResponse = await getResponsePromise;
  expect(getResponse.headers()["cache-control"]).toBe("no-store");
  const getPayload = JSON.stringify(await getResponse.json());
  expect(getPayload).toContain(HINT_PREFIX);
  expect(getPayload).not.toContain(HINT_REMAINDER);

  await expect(
    page.getByTestId("runtime-env-row-ANTHROPIC_AUTH_TOKEN"),
  ).toHaveCount(0);
  const claudeRow = page.getByTestId("runtime-env-default-model-claude_code");
  await expect(claudeRow).toBeVisible({ timeout: 30_000 });
  await expect(
    claudeRow.getByTestId("runtime-env-default-model-select-claude_code"),
  ).toBeVisible({ timeout: 30_000 });
  if (PROVIDER_ACCOUNTS_E2E_ENABLED) {
    await expect(
      claudeRow.getByTestId("provider-account-assignment-select-claude_code"),
    ).toBeEnabled({ timeout: 30_000 });
  }
  await expect(
    claudeRow.getByTestId("provider-account-resolution-claude_code"),
  ).toHaveCount(0);
  await expect(
    claudeRow.getByTestId("provider-account-resolution-hint-claude_code"),
  ).toHaveCount(0);
  await expect(claudeRow).not.toContainText("resolves to");
  await expect(claudeRow).not.toContainText(HINT_PREFIX);
  await expect(page.locator("body")).not.toContainText(HINT_REMAINDER);
  if (accountId) {
    await expect(page.locator("body")).not.toContainText(ACCOUNT_KEY_REMAINDER);
  }

  // 4. Custom secret row (settings-page D5/D9): add a custom write-only
  //    secret through "+ Add custom variable" (a name prompt, then a confirm
  //    choosing the secret channel; the new row opens with its Override
  //    drawer already expanded), set it at the clone's Workspace scope,
  //    Save, and assert the row shows the server hint in its effective
  //    display and Workspace pill while the unset scope keeps `—`.
  page.on("dialog", (dialog) => {
    if (dialog.type() === "prompt") {
      void dialog.accept(CUSTOM_KEY);
    } else {
      void dialog.accept();
    }
  });
  await page.getByTestId("runtime-env-add-var").click();
  const customRow = page.getByTestId(`runtime-env-row-${CUSTOM_KEY}`);
  await expect(customRow).toBeVisible({ timeout: 15_000 });
  await expect(customRow).toContainText("secret");
  const scopeSelect = page.getByTestId(`runtime-env-scope-${CUSTOM_KEY}`);
  await expect(scopeSelect).toBeVisible({ timeout: 15_000 });
  await scopeSelect.selectOption("workspace");
  const secretInput = page.getByTestId(`runtime-env-secret-input-${CUSTOM_KEY}`);
  await expect(secretInput).toBeVisible({ timeout: 15_000 });
  await secretInput.fill(CUSTOM_TOKEN);
  const applyButton = page.getByTestId(`runtime-env-apply-${CUSTOM_KEY}`);
  await expect(applyButton).toBeEnabled({ timeout: 15_000 });
  await applyButton.click();
  const saveButton = page.getByTestId("runtime-env-save");
  await expect(saveButton).toBeEnabled({ timeout: 15_000 });
  const customPutPromise = page.waitForResponse(
    (r) =>
      r.url().includes(`/workspaces/${workspace!.id}/config`) &&
      r.request().method() === "PUT",
    { timeout: 30_000 },
  );
  await saveButton.click();
  const customPut = await customPutPromise;
  expect(customPut.headers()["cache-control"]).toBe("no-store");
  const customPutPayload = JSON.stringify(await customPut.json());
  expect(customPutPayload).toContain(CUSTOM_PREFIX);
  expect(customPutPayload).not.toContain(CUSTOM_REMAINDER);
  const customHint = page.getByTestId(`runtime-env-hint-${CUSTOM_KEY}`);
  await expect(customHint).toHaveText(`${CUSTOM_PREFIX}…`, { timeout: 30_000 });
  await expect(customRow).toContainText("configured");
  await expect(customRow).toContainText(`Workspace: ${CUSTOM_PREFIX}…`);
  await expect(customRow).toContainText("You: —");
  await expect(page.locator("body")).not.toContainText(CUSTOM_REMAINDER);
  await expect(page.locator("body")).not.toContainText(HINT_REMAINDER);

  // 5. Ineligible (short) values: overwrite the legacy key and the custom
  //    secret at the same clone workspace scope. The Defaults card still
  //    renders no hint (nothing is derived client-side); the custom row
  //    keeps the masked display.
  await putWorkspaceSecret(page, workspace, orgId, SHORT_TOKEN);
  const shortCustom = await authedApi(
    page,
    `/api/v1/workspaces/${workspace.id}/config?orgId=${orgId}`,
    "PUT",
    { setSecrets: { [CUSTOM_KEY]: SHORT_TOKEN } },
  );
  expect(shortCustom.status).toBe(200);
  await openSettings(page, workspace);
  const maskedClaudeRow = page.getByTestId(
    "runtime-env-default-model-claude_code",
  );
  await expect(maskedClaudeRow).toBeVisible({ timeout: 30_000 });
  await expect(
    maskedClaudeRow.getByTestId("provider-account-resolution-claude_code"),
  ).toHaveCount(0);
  await expect(
    page.getByTestId("provider-account-resolution-hint-claude_code"),
  ).toHaveCount(0);
  await expect(
    page.getByTestId("provider-account-resolution-hint-source-claude_code"),
  ).toHaveCount(0);
  const maskedCustomRow = page.getByTestId(`runtime-env-row-${CUSTOM_KEY}`);
  await expect(maskedCustomRow).toContainText("•••• configured", {
    timeout: 30_000,
  });
  await expect(page.getByTestId(`runtime-env-hint-${CUSTOM_KEY}`)).toHaveCount(0);
  await expect(maskedCustomRow).toContainText("Workspace: set");
  await expect(page.locator("body")).not.toContainText(SHORT_TOKEN);

  // 6. Browser console output never carried any remainder or the short
  //    token either (D9 leak sweep, console surface).
  const consoleDump = consoleMessages.join("\n");
  expect(consoleDump).not.toContain(HINT_REMAINDER);
  expect(consoleDump).not.toContain(CUSTOM_REMAINDER);
  expect(consoleDump).not.toContain(SHORT_TOKEN);
  expect(consoleDump).not.toContain(ACCOUNT_KEY_REMAINDER);
}

test.describe
  .serial("Credential visibility — secret beginning-of-key hint", () => {
  test.describe.configure({ timeout: 240_000 });
  // The legacy secret is written at the WORKSPACE scope of the disposable
  // clone, never at the shared demo user's user scope: the secret channel is
  // write-only, so a shared-user override could not be restored afterwards.
  // Deleting the clone in cleanup removes the secret with it; the test
  // account and the (cleared) user-scope assignment are the only shared
  // state touched, and both are cleaned here.
  test.afterEach(async ({ page }) => {
    if (orgId) {
      await clearClaudeAssignment(page, orgId);
      await deleteHintAccounts(page, orgId);
    }
    if (workspace) {
      await deleteWorkspaceById(page, workspace.id);
      workspace = null;
    }
  });

  test("account row and custom secret row show the 12-char hint, the Defaults card shows none; remainder never renders; short secret stays masked", async ({
    page,
  }) => {
    await credentialsHintScenario(page);
  });
});
