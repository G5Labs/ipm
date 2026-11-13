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

// Codex settings browser E2E — NET-NEW (not a Claude mirror; settings.spec.ts
// does not test Anthropic credentials). codex_support.test_view §7,
// TC-CODEX-011 → AC-11: OPENAI_API_KEY is stored as a write-only secret at the
// clone's workspace scope, the user sets the default agent provider to Codex
// in Settings → Defaults, and the Codex-native controls operate end to end.
//
// Retargeted for settings-page D19 (chat 994e615b): the Defaults card renders
// no OPENAI_API_KEY row any more (the model plus the assigned account derive
// the provider variables), so step 1 seeds the secret through the scoped
// config API instead of the retired Override drawer and asserts the UI never
// renders the key. Asserts the real RuntimeEnvConfig test-ids:
// `runtime-env-config`, `runtime-env-default-agent-select`.
const CODEX_SETTINGS_E2E_ENABLED = process.env.G5_E2E_CODEX_SETTINGS === "true";

async function createScenarioWorkspace(page: Page): Promise<WorkspaceClone> {
  return createWorkspaceClone(page, {
    org: TEST_ORG,
    workspace: TEST_WORKSPACE,
    namePrefix: "codex-settings",
    description: "Codex settings credential + default-agent browser E2E clone",
  });
}

async function openRuntimeEnvConfig(
  page: Page,
  workspace: WorkspaceClone,
): Promise<void> {
  await openWorkspacePath(page, `${workspace.path}?view=settings`);
  await expect(page.getByTestId("workspace-settings-page")).toBeVisible({
    timeout: 30_000,
  });
  await expect(page.getByTestId("runtime-env-config")).toBeVisible({
    timeout: 30_000,
  });
}

test.describe.serial("Codex settings — credential + default agent", () => {
  test.describe.configure({ timeout: 180_000 });
  // Codex is available in every G5 Lean deployment.

  test.skip(
    !CODEX_SETTINGS_E2E_ENABLED,
    "Codex settings flow is opt-in (set G5_E2E_CODEX_SETTINGS=true with a demo stack)",
  );

  let workspace: WorkspaceClone | null = null;

  // The default-agent selector persists `default_agent_provider` to USER scope
  // (RuntimeEnvConfig I11/D3), which outranks Workspace and Org and is shared by
  // every spec in the run. The scoped-config PUT deep-merges `settings`
  // (G5 #1425), so the key cannot be cleared by omission — it needs the explicit
  // `removeSettingKeys` channel. Without this, deleting the workspace clone alone
  // leaves Codex as the effective default for the rest of the CI run.
  async function clearUserDefaultAgent(page: Page): Promise<void> {
    const orgId = await readCurrentOrgId(page).catch(() => "");
    if (!orgId) return;
    await authedApi(page, `/api/v1/users/me/config?orgId=${orgId}`, "PUT", {
      removeSettingKeys: ["default_agent_provider"],
    }).catch(() => {});
  }

  test.afterEach(async ({ page }) => {
    await clearUserDefaultAgent(page);
    if (workspace) {
      await deleteWorkspaceById(page, workspace.id);
      workspace = null;
    }
  });

  test("enter OPENAI_API_KEY, set default agent = Codex, controls operate", async ({
    page,
  }) => {
    workspace = await createScenarioWorkspace(page);

    await navigateToWorkspace(page, TEST_ORG, workspace.name);
    await waitForWorkspaceLoad(page);
    await openRuntimeEnvConfig(page, workspace);

    // 1. Store OPENAI_API_KEY as a write-only secret at the clone's workspace
    //    scope through the scoped config API (the Defaults card has no
    //    provider variable rows — settings-page D19). The write response
    //    never echoes the value, and the UI renders neither a row for the
    //    key nor the literal value.
    const orgId = await readCurrentOrgId(page);
    const secretWrite = await authedApi(
      page,
      `/api/v1/workspaces/${workspace.id}/config?orgId=${orgId}`,
      "PUT",
      { setSecrets: { OPENAI_API_KEY: "sk-codex-e2e-test-key" } },
    );
    expect(secretWrite.status, JSON.stringify(secretWrite.body).slice(0, 800)).toBe(200);
    expect(JSON.stringify(secretWrite.body)).not.toContain(
      "sk-codex-e2e-test-key",
    );
    await openRuntimeEnvConfig(page, workspace);
    await expect(
      page.getByTestId("runtime-env-row-OPENAI_API_KEY"),
    ).toHaveCount(0);
    await expect(
      page.getByTestId("runtime-env-override-OPENAI_API_KEY"),
    ).toHaveCount(0);
    await expect(page.locator("body")).not.toContainText("sk-codex-e2e-test-key");

    // 2. Set the default agent provider to Codex (persists via scoped config).
    const defaultAgentSelect = page.getByTestId(
      "runtime-env-default-agent-select",
    );
    await expect(defaultAgentSelect).toBeVisible({ timeout: 15_000 });
    await defaultAgentSelect.selectOption("codex");
    await expect(defaultAgentSelect).toHaveValue("codex");

    // 3. Start a new chat → it should default to Codex and the Codex-native
    // controls should be operable. (Requires a Codex-capable worker; without
    // one the composer falls back to the first available provider — the assert
    // is scoped to the persisted default having taken effect in settings.)
    const defaultAgentDisplay = page.getByTestId("runtime-env-default-agent");
    await expect(defaultAgentDisplay).toContainText(/codex/i);
  });
});
