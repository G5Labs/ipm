// =============================================================================
// First run — a brand-new person joins by invitation and starts personal work
// =============================================================================
// Covers the Core "getting in" stories the loops only touched incidentally
// (audit 2026-10-30; g5.organization.invitation-email, g5.personal.lifecycle,
// g5.personal.start-and-send, g5.personal.home):
//
//   FR1  An admin invites a brand-new email address; the invitation email
//        arrives at the provider stub; the invitee follows its link, signs in
//        through the OAuth stub for the first time and is admitted as a member;
//        they land on Home and run personal Start and send, and the agent
//        replies.
//
// Builds on the invitation-email end-to-end case (invite-members.spec.ts) and
// the Home Start-and-send path (personal-workspace.spec.ts) without
// duplicating them: it is the one loop that walks a new person from an
// invitation all the way to their first agent reply.
//
// Platform-operator organization creation (g5.organization.platform-operator.
// create-org) is a separate registered loop (org-create.spec.ts): a freshly
// created organization has no worker host in this receipt lane, so personal
// Start and send cannot run inside it — the host is bound to the lane's
// organization (scripts/seed-ci-org.sh provisions it per org).
// =============================================================================

import { randomUUID } from "node:crypto";
import { test, expect, type Page } from "@playwright/test";
import {
  APP_BASE_URL,
  TEST_ORG,
  authedApi,
  authedApiViaRequest,
  readCurrentOrgContext,
  runWorkspaceVmAction,
  waitForWorkspaceVmStatus,
  readWorkspaceVm,
} from "../helpers";
import {
  oauthStubBaseUrl,
  resetOAuthStubIdentity,
  setOAuthStubIdentity,
  type OAuthStubIdentity,
} from "../oauth";

test.describe.configure({ timeout: 300_000 });

interface StubEmail {
  to: string[];
  subject: string;
  text: string;
  html: string;
}

async function openInviteDialog(page: Page) {
  await page.goto(`/${TEST_ORG}/settings/members`, {
    waitUntil: "domcontentloaded",
  });
  const inviteBtn = page.getByRole("button", { name: /invite/i });
  await expect(inviteBtn).toBeVisible({ timeout: 30_000 });
  await inviteBtn.click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toBeVisible({ timeout: 5_000 });
  return dialog;
}

test("FR1: an invited newcomer is admitted at first login and gets a reply from personal Start and send", async ({
  page,
  browser,
}) => {
  const stub = oauthStubBaseUrl();
  const health = await page.request.get(`${stub}/healthz`, { timeout: 5_000 });
  expect(
    health.ok(),
    "the receipt stack must include the provider/email stub",
  ).toBeTruthy();

  const { orgId } = await readCurrentOrgContext(page);
  const suffix = `${Date.now()}${Math.floor(Math.random() * 1000)}`;
  const invitee: OAuthStubIdentity = {
    id: 400_000 + Math.floor(Math.random() * 500_000),
    login: `e2e-firstrun-${suffix}`,
    email: `e2e-firstrun-${suffix}@g5.dev`,
    name: "E2E First Run",
  };

  const context = await browser.newContext({
    storageState: { cookies: [], origins: [] },
  });
  const invitee_page = await context.newPage();
  let me:
    | { id: string; email: string; orgs: { orgSlug: string; role: string }[] }
    | undefined;
  let personalWorkspaceId = "";

  try {
    // An admin invites the brand-new address.
    const dialog = await openInviteDialog(page);
    await dialog.locator("#invite-email").fill(invitee.email);
    await dialog.getByRole("button", { name: /send invite/i }).click();
    await expect(page.getByRole("dialog")).not.toBeVisible({ timeout: 15_000 });

    // Exactly one correctly addressed invitation email reaches the stub.
    let message: StubEmail | undefined;
    await expect
      .poll(
        async () => {
          const response = await page.request.get(
            `${stub}/__e2e/emails?to=${encodeURIComponent(invitee.email)}`,
          );
          const body = (await response.json()) as { emails: StubEmail[] };
          message = body.emails[0];
          return body.emails.length;
        },
        { timeout: 45_000, message: "invitation email delivered to the stub" },
      )
      .toBe(1);
    expect(message!.subject).toMatch(/invited you to join .+ on G5/);
    const link = message!.text
      .split("\n")
      .find((line) => line.startsWith("http"));
    expect(link, "the invitation carries a link").toBeTruthy();
    const target = new URL(link!);
    expect(target.origin).toBe(new URL(APP_BASE_URL).origin);
    expect(target.pathname).toBe(`/${TEST_ORG}`);

    // The invitee follows the link in a fresh, signed-out browser and is
    // admitted as a member at first login.
    await setOAuthStubIdentity(invitee_page, invitee);
    await invitee_page.goto(link!, { waitUntil: "domcontentloaded" });
    await invitee_page.waitForURL((url) => url.pathname.startsWith("/login"), {
      timeout: 30_000,
    });
    await invitee_page
      .getByRole("link", { name: /continue with github/i })
      .click();
    const authorize = invitee_page.getByRole("button", { name: /^authorize$/i });
    if (await authorize.isVisible().catch(() => false)) await authorize.click();
    await invitee_page.waitForURL(
      (url) =>
        !url.pathname.startsWith("/login") &&
        !url.pathname.startsWith("/forbidden") &&
        (url.pathname === `/${TEST_ORG}` ||
          url.pathname.startsWith(`/${TEST_ORG}/`)),
      { timeout: 45_000 },
    );
    me = await invitee_page.evaluate(async () => {
      const refresh = await fetch("/api/v1/auth/refresh", {
        method: "POST",
        credentials: "include",
      });
      const jwt = (await refresh.json()).data?.jwt ?? "";
      const response = await fetch("/api/v1/users/me", {
        headers: { Authorization: `Bearer ${jwt}` },
      });
      return (await response.json()).data as {
        id: string;
        email: string;
        orgs: { orgSlug: string; role: string }[];
      };
    });
    expect(me.email.toLowerCase()).toBe(invitee.email.toLowerCase());
    expect(me.orgs).toContainEqual(
      expect.objectContaining({ orgSlug: TEST_ORG, role: "member" }),
    );

    // Finish onboarding for this brand-new account so the guided tour does not
    // auto-start on Home and seize the Home composer (g5.user.onboarding; step
    // 7 is the finished value) — without this the Start-and-send click is
    // swallowed by the tour's seeded scratchpad.
    const onboarded = await authedApi(
      invitee_page,
      "/api/v1/users/me",
      "PATCH",
      { onboardingStep: 7, onboardingSkipped: true },
    );
    expect(onboarded.status, JSON.stringify(onboarded.body)).toBe(200);

    // Ensure the newcomer's personal workspace exists (created on demand by
    // `/me`) and is stopped, so Home's Start-and-send is the cold-start path
    // and the start request fires on click (mirrors personal-workspace.spec).
    const mePersonal = await authedApi(invitee_page, `/api/v1/orgs/${orgId}/me`);
    expect(mePersonal.status, JSON.stringify(mePersonal.body)).toBe(200);
    personalWorkspaceId = (mePersonal.body?.data ?? mePersonal.body).workspaceId;
    expect(personalWorkspaceId).toBeTruthy();
    await waitForWorkspaceVmStatus(
      invitee_page,
      personalWorkspaceId,
      "stopped",
    ).catch(() => {});

    // On Home the newcomer runs personal Start and send; the agent replies.
    await invitee_page.goto(`/${TEST_ORG}/home`, {
      waitUntil: "domcontentloaded",
    });
    const chat = invitee_page.getByTestId("home-personal-chat");
    await expect(chat).toBeVisible({ timeout: 30_000 });
    const composer = chat.getByTestId("chat-input");
    await expect(composer).toBeEnabled({ timeout: 20_000 });
    // `hello` elicits the fake provider's deterministic reply
    // (tools/fake-provider-cli default scenario), so the reply is assertable.
    await composer.fill("hello");
    const startAndSend = chat.getByTestId("chat-panel-no-session-send");
    await expect(startAndSend).toHaveText(/Start and send/);
    const startRequest = invitee_page.waitForRequest(
      (request) =>
        request.method() === "POST" &&
        /\/api\/v1\/workspaces\/[^/]+\/sessions\/start$/.test(
          new URL(request.url()).pathname,
        ),
    );
    await expect(startAndSend).toBeEnabled({ timeout: 20_000 });
    await startAndSend.click();
    const started = await startRequest;
    expect(started.postDataJSON().startCompute).toBe(true);

    // The personal VM comes up and the message is queued and echoed.
    await waitForWorkspaceVmStatus(invitee_page, personalWorkspaceId, "running");
    await expect(chat.getByTestId("chat-messages")).toContainText("hello", {
      timeout: 30_000,
    });

    // The agent's reply crosses the real VM/API/browser path back to Home.
    await expect(
      chat
        .getByTestId("assistant-message")
        .filter({ hasText: /hello from (claude code|codex)/i })
        .first(),
    ).toBeVisible({ timeout: 120_000 });
  } finally {
    if (personalWorkspaceId) {
      await expect
        .poll(
          async () => {
            const { status } = await readWorkspaceVm(
              invitee_page,
              personalWorkspaceId,
            );
            if (status !== "stopped" && status !== "stopping") {
              await runWorkspaceVmAction(
                invitee_page,
                personalWorkspaceId,
                "stop",
              ).catch(() => {});
            }
            return status === "failed" ? "stopped" : status;
          },
          { timeout: 120_000, intervals: [2_000, 5_000] },
        )
        .toBe("stopped")
        .catch(() => {});
    }
    await resetOAuthStubIdentity(invitee_page).catch(() => undefined);
    await context.close();
    // Leave the shared organization as other specs expect. Resolve the member
    // by email when admission never set `me` (a mid-flow failure before line
    // ~155), check the removal, and never let cleanup mask the real failure.
    try {
      let memberId = me?.id ?? "";
      if (!memberId) {
        const members = await authedApiViaRequest(
          page,
          `/api/v1/orgs/${orgId}/members`,
        );
        const list = (members.body?.data ?? members.body ?? []) as {
          userId?: string;
          email?: string;
        }[];
        memberId =
          (Array.isArray(list) ? list : []).find(
            (m) => m.email?.toLowerCase() === invitee.email.toLowerCase(),
          )?.userId ?? "";
      }
      if (memberId) {
        const removed = await authedApiViaRequest(
          page,
          `/api/v1/orgs/${orgId}/members/${memberId}`,
          "DELETE",
        );
        if (![200, 204, 404].includes(removed.status)) {
          console.warn(
            `FR1 cleanup could not remove ${invitee.email}: ${removed.status} ${JSON.stringify(removed.body)}`,
          );
        }
      }
    } catch (error) {
      console.warn(`FR1 cleanup error removing ${invitee.email}: ${error}`);
    }
  }
});
