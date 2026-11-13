// =============================================================================
// E2E: Connect GitHub via fine-grained Personal Access Token
// =============================================================================
// Validates the scm_pat_connect.test_view scenarios through workspace
// settings' SCM Connections section (WorkspaceSettingsContent hosts
// GithubPatConnect). Organization onboarding no longer offers it: only platform
// operators create organizations (g5.organization.creation), and a new org's
// owner connects repositories later from Base's settings. The connect flow is
// hermetic: the api validates the PAT against the oauth-stub's /user
// (G5_SCM_GITHUB_API_BASE_URL), which accepts E2E_VALID_PAT and 401s anything
// else. No real github.com, no secret.
//
// Grounds to scm_pat_connect.test_view.md I3 (success), I4 (invalid), I7
// (guidance); I8's entry point is now the settings page.
// =============================================================================

import { test, expect, type Page, type Locator } from '@playwright/test';
import { TEST_SELECTED_ORG, openWorkspacePath } from '../helpers';
import { connectGithubPat, E2E_VALID_PAT } from '../oauth';

const PAT_STUB_ENABLED =
  process.env.G5_E2E_SCM_PAT_STUB_ENABLED === 'true' ||
  process.env.CI === 'true';

/** Open the seeded org's Base settings and return its SCM Connections section. */
async function openScmConnections(page: Page): Promise<Locator> {
  await openWorkspacePath(page, `/${TEST_SELECTED_ORG}/base?view=settings`);
  const settingsPage = page.getByTestId('workspace-settings-page');
  await expect(settingsPage).toBeVisible({ timeout: 15_000 });
  const section = settingsPage.getByTestId('settings-scm-connections');
  await expect(section).toBeVisible({ timeout: 15_000 });
  await expect(section.getByLabel(/personal access token/i)).toBeVisible({
    timeout: 15_000,
  });
  return section;
}

test.describe('SCM connect — fine-grained PAT', () => {
  test.skip(
    !PAT_STUB_ENABLED,
    'requires G5_SCM_GITHUB_API_BASE_URL to target the deterministic OAuth stub',
  );
  // These cases share one user-scoped GitHub connection: the I3/I8 success case
  // upserts a row in user_scm_connections, after which the PAT widget no longer
  // renders for that seeded user — so I7/I4 must run BEFORE I3/I8. With the suite
  // now running fullyParallel, pin this block serial to preserve that ordering
  // (no tests skipped; they just run in order on one worker).
  test.describe.configure({ mode: 'serial' });

  test('I7: shows generation guidance with the fine-grained token link and required permissions', async ({
    page,
  }) => {
    const section = await openScmConnections(page);

    const link = section.getByRole('link', {
      name: /generate token on github/i,
    });
    await expect(link).toBeVisible();
    await expect(link).toHaveAttribute(
      'href',
      'https://github.com/settings/personal-access-tokens/new',
    );
    await expect(
      section.getByText(/Contents: Read and write/i),
    ).toBeVisible();
    await expect(section.getByText(/Metadata: Read/i)).toBeVisible();
  });

  test('I4: an invalid token is rejected with an actionable error and no connection', async ({
    page,
  }) => {
    const section = await openScmConnections(page);

    await connectGithubPat(section, 'not-a-real-token');

    await expect(section.getByText(/GitHub rejected the token/i)).toBeVisible({
      timeout: 15_000,
    });
    // The token field remains — nothing was connected.
    await expect(
      section.getByLabel(/personal access token/i),
    ).toBeVisible();
  });

  test('I3/I8: connects with a valid fine-grained PAT from workspace settings', async ({
    page,
  }) => {
    const section = await openScmConnections(page);

    await connectGithubPat(section, E2E_VALID_PAT);

    // On success the GitHub connect widget unmounts (provider now connected),
    // so the token field disappears.
    await expect(
      section.getByLabel(/personal access token/i),
    ).toBeHidden({ timeout: 15_000 });
  });
});
