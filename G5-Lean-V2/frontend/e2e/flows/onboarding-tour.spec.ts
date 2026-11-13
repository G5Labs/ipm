import { expect, test, type Page } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import { authedApi } from '../helpers';
import { createOrganizationFor } from '../organization';
import {
  loginWithOAuthStub,
  resetOAuthStubIdentity,
  type OAuthStubIdentity,
} from '../oauth';

// The onboarding tour (ui g5.ui.onboarding-tour) for a person who has never
// seen it: the welcome card on Home, the forced Settings press, the Theme and
// Accounts steps (no shared account in a fresh organization, so the person's
// own account unlocks Next after a resume), the scratchpad step, the seeded
// issue message, Skip, the All-before-completion Home defaults (users
// g5.user.onboarding) and Replay. The agent-driven issue creation is left to
// manual inspection: its outcome depends on a live agent.

let counter = 0;
function freshIdentity(): OAuthStubIdentity {
  const suffix = `${Date.now()}-${process.pid}-${counter++}`;
  return {
    id: 300_000_000 + (Date.now() % 100_000_000) + counter,
    login: `e2e-tour-${suffix}`,
    email: `e2e-tour-${suffix}@g5.dev`,
    name: 'Tour Person',
  };
}

interface MeDTO {
  onboardingStep: number | null;
  onboardingSkipped: boolean;
  orgs: Array<{ orgId: string; orgSlug: string }>;
}

async function readMe(page: Page): Promise<MeDTO> {
  const result = await authedApi(page, '/api/v1/users/me', 'GET');
  expect(result.status).toBe(200);
  return (result.body?.data ?? result.body) as MeDTO;
}

test.describe('Onboarding tour', () => {
  test.describe.configure({ timeout: 180_000 });
  test.use({ storageState: { cookies: [], origins: [] } });
  test.afterEach(async ({ page }) => {
    await resetOAuthStubIdentity(page);
  });

  test('walks a new person from the welcome card to the issue step, records progress, skips and replays', async ({
    page,
  }) => {
    const identity = freshIdentity();
    const { slug } = await createOrganizationFor(identity.email, `tour-${Date.now()}`);
    // Sign-in lands on the organization's Home (ui g5.ui.sign-in-landing).
    await loginWithOAuthStub(page, identity);

    const me = await readMe(page);
    const org = me.orgs.find((membership) => membership.orgSlug === slug);
    if (!org) throw new Error(`the person is not a member of ${slug}`);

    // Step 1: the welcome card on Home, recorded as reached.
    await page.goto(`/${org.orgSlug}/home`, { waitUntil: 'domcontentloaded' });
    const welcome = page.getByTestId('onboarding-step-welcome');
    await expect(welcome).toBeVisible({ timeout: 30_000 });
    await expect(welcome).toContainText('Welcome to G5, Tour.');
    // A new person has the Guided section off (ui g5.ui.guided-toggle), so
    // the tour has six steps.
    await expect(welcome.getByTestId('onboarding-card-counter')).toHaveText(
      'Step 1 of 6',
    );
    await expect
      .poll(async () => (await readMe(page)).onboardingStep, { timeout: 15_000 })
      .toBe(0);

    // Everything but Settings is blocked: New workspace does nothing.
    await page.getByTestId('home-new-workspace').click({ force: true });
    await expect(page.getByRole('dialog', { name: /create workspace/i })).toHaveCount(0);

    // Step 2: Settings opens Base's settings view with the Theme step.
    await page.getByTestId('settings-button').click();
    await page.waitForURL((url) => url.searchParams.get('view') === 'settings', {
      timeout: 30_000,
    });
    const theme = page.getByTestId('onboarding-step-theme');
    await expect(theme).toBeVisible({ timeout: 30_000 });
    await expect(page.getByTestId('settings-row-theme')).toBeVisible();
    await theme.getByTestId('onboarding-next').click();

    // Step 3: a fresh organization provides no account, so Next waits.
    const accounts = page.getByTestId('onboarding-step-accounts');
    await expect(accounts).toBeVisible({ timeout: 30_000 });
    await expect(accounts).toContainText('Add yours here before we continue');
    await expect(accounts.getByTestId('onboarding-next')).toBeDisabled();
    await expect
      .poll(async () => (await readMe(page)).onboardingStep, { timeout: 15_000 })
      .toBe(2);

    // Leaving the step offers an accessible way back to the unfinished tour.
    await page.goto(`/${org.orgSlug}/home`, { waitUntil: 'domcontentloaded' });
    const continueTour = page.getByTestId('onboarding-continue');
    await expect(continueTour).toBeVisible({ timeout: 15_000 });
    const contrast = await new AxeBuilder({ page })
      .include('[data-testid="onboarding-continue"]')
      .withRules(['color-contrast'])
      .analyze();
    expect(contrast.violations).toEqual([]);
    await continueTour.click();
    await expect(accounts).toBeVisible({ timeout: 15_000 });

    const created = await authedApi(
      page,
      `/api/v1/orgs/${org.orgId}/provider-accounts`,
      'POST',
      {
        name: `tour-account-${Date.now()}`,
        kind: 'anthropic_api_key',
        scope: 'user',
        secret: 'sk-ant-api03-e2e-onboarding-tour-0123456789',
      },
    );
    expect(created.status).toBe(201);

    // A reload resumes the unfinished tour at the Accounts step, now unlocked.
    await page.reload({ waitUntil: 'domcontentloaded' });
    const resumed = page.getByTestId('onboarding-step-accounts');
    await expect(resumed).toBeVisible({ timeout: 30_000 });
    await expect(resumed.getByTestId('onboarding-next')).toBeEnabled({
      timeout: 15_000,
    });
    await resumed.getByTestId('onboarding-next').click();

    // Step 4: the tour navigates Home to the scratchpad.
    await page.waitForURL(new RegExp(`/${org.orgSlug}/home`), { timeout: 30_000 });
    const scratchpad = page.getByTestId('onboarding-step-scratchpad');
    await expect(scratchpad).toBeVisible({ timeout: 30_000 });
    await expect(scratchpad).toContainText('personal scratchpad chat');
    // The tiles open on All before the tour is finished.
    await expect(page.getByTestId('home-workspaces-filter-all')).toHaveAttribute(
      'aria-pressed',
      'true',
    );
    await scratchpad.getByTestId('onboarding-next').click();

    // Step 5: the composer carries the issue message.
    const createIssue = page.getByTestId('onboarding-step-create-issue');
    await expect(createIssue).toBeVisible({ timeout: 30_000 });
    await expect(createIssue).toContainText('Pick a model above the composer');
    const composer = page
      .getByTestId('home-personal-chat')
      .getByTestId('chat-input');
    await expect(composer).toHaveValue(/\[Test issue\] Tour onboarding/, {
      timeout: 30_000,
    });
    // The card sits beside the chat, never over its transcript.
    await expect(createIssue.getByTestId('onboarding-card-arrow')).toHaveAttribute(
      'data-side',
      /left|right/,
    );

    // The manual path: the card's own button moves the spotlight to the
    // Issues tile's Create.
    await createIssue.getByTestId('onboarding-create-issue-manually').click();
    await expect(createIssue).toContainText('Press Create in the Issues tile');
    await expect(createIssue.getByTestId('onboarding-create-issue-manually')).toHaveCount(
      0,
    );

    // Skip records the step shown and the skip; nothing shows afterwards.
    await createIssue.getByTestId('onboarding-skip').click();
    await expect(page.getByTestId('onboarding-tour')).toHaveCount(0);
    await expect
      .poll(async () => {
        const record = await readMe(page);
        return `${record.onboardingStep}/${record.onboardingSkipped}`;
      }, { timeout: 15_000 })
      .toBe('4/true');
    await page.reload({ waitUntil: 'domcontentloaded' });
    await expect(page.getByTestId('home-workspaces')).toBeVisible({
      timeout: 30_000,
    });
    await expect(page.getByTestId('onboarding-tour')).toHaveCount(0);
    await expect(page.getByTestId('onboarding-continue')).toHaveCount(0);
    await expect(page.getByTestId('home-workspaces-filter-all')).toHaveAttribute(
      'aria-pressed',
      'true',
    );

    // Finished: the record only rises, and Home returns to the person's own.
    const finished = await authedApi(page, '/api/v1/users/me', 'PATCH', {
      onboardingStep: 7,
    });
    expect(finished.status).toBe(200);
    const lowered = await authedApi(page, '/api/v1/users/me', 'PATCH', {
      onboardingStep: 1,
    });
    expect(lowered.status).toBe(200);
    expect((await readMe(page)).onboardingStep).toBe(7);
    await page.reload({ waitUntil: 'domcontentloaded' });
    await expect(page.getByTestId('home-workspaces-filter-mine')).toHaveAttribute(
      'aria-pressed',
      'true',
      { timeout: 30_000 },
    );

    // Replay from Settings starts the welcome card again without lowering it.
    await page.getByTestId('settings-button').click();
    await page.waitForURL((url) => url.searchParams.get('view') === 'settings', {
      timeout: 30_000,
    });
    await page.getByTestId('onboarding-replay').click();
    await page.waitForURL(new RegExp(`/${org.orgSlug}/home`), { timeout: 30_000 });
    await expect(page.getByTestId('onboarding-step-welcome')).toBeVisible({
      timeout: 30_000,
    });
    expect((await readMe(page)).onboardingStep).toBe(7);
  });
});
