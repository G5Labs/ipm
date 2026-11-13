import { randomUUID } from "node:crypto";

import {
  Page,
  expect,
  Locator,
  type BrowserContext,
  type Response,
} from "@playwright/test";

import { recordTiming, timed } from "./timings";
import { requestWorkspaceVmAction } from "./workspace-vm-action";

const DEFAULT_BASE_URL = process.env.CI
  ? "http://localhost:3000"
  : "http://localhost:5173";
export const APP_BASE_URL = process.env.E2E_BASE_URL || DEFAULT_BASE_URL;
const APP_BOOTSTRAP_URL = new URL("/projects", APP_BASE_URL).toString();

// ---------------------------------------------------------------------------
// Identity contract (Z0)
//
// One browser context acts as exactly one platform user. The API issues its
// refresh cookie with the API's name/path for this deployment. Secure cookies
// use a host prefix; ambiguous credentials fail closed at the API. Every
// identity goes through `installIdentity`, which leaves one refresh cookie at the API's own
// path, records the intended subject, and every owner-only operation asserts
// the active subject before acting.
// ---------------------------------------------------------------------------
const REFRESH_COOKIE_BASE_NAME =
  process.env.G5_REFRESH_COOKIE_NAME ?? "g5_refresh";
const LEGACY_REFRESH_COOKIE_NAME = REFRESH_COOKIE_BASE_NAME.replace(
  /^__Host-/,
  "",
);
const SECURE_REFRESH_COOKIE =
  process.env.G5_COOKIE_SECURE === undefined
    ? new URL(APP_BASE_URL).protocol === "https:"
    : process.env.G5_COOKIE_SECURE === "true";
export const REFRESH_COOKIE_NAME =
  SECURE_REFRESH_COOKIE && !REFRESH_COOKIE_BASE_NAME.startsWith("__Host-")
    ? `__Host-${REFRESH_COOKIE_BASE_NAME}`
    : REFRESH_COOKIE_BASE_NAME;
export const REFRESH_COOKIE_PATH = SECURE_REFRESH_COOKIE ? "/" : "/api/v1/auth";
export const AUTH_TOKEN_KEY = "g5_auth_token";
export const AUTH_EMAIL_KEY = "g5_auth_email";
export const EXPECTED_SUBJECT_KEY = "g5_e2e_expected_subject";

function jwtPayload(jwt: string): Record<string, unknown> {
  const encoded = jwt.split(".")[1] ?? "";
  if (!encoded) {
    return {};
  }
  try {
    return JSON.parse(
      Buffer.from(encoded, "base64url").toString("utf8"),
    ) as Record<string, unknown>;
  } catch {
    return {};
  }
}

/** `sub` claim of a platform JWT; throws when the token carries none. */
export function jwtSubject(jwt: string): string {
  const sub = jwtPayload(jwt).sub;
  if (typeof sub !== "string" || !sub) {
    throw new Error("platform JWT carries no subject");
  }
  return sub;
}

/**
 * `sub` claim of a platform JWT, or an empty string when the value is not a
 * decodable token (a deliberately aged-out fixture value, for instance).
 */
export function jwtSubjectOrEmpty(jwt: string): string {
  const sub = jwtPayload(jwt).sub;
  return typeof sub === "string" ? sub : "";
}

/** `org` claim of a platform JWT, or an empty string. */
export function jwtOrg(jwt: string): string {
  const org = jwtPayload(jwt).org;
  return typeof org === "string" ? org : "";
}

/** Every effective or legacy refresh cookie, on any domain or path. */
export async function refreshCookies(
  context: BrowserContext,
): Promise<Array<{ domain: string; path: string; value: string }>> {
  const cookies = await context.cookies();
  return cookies
    .filter(
      (cookie) =>
        cookie.name === REFRESH_COOKIE_NAME ||
        cookie.name === LEGACY_REFRESH_COOKIE_NAME,
    )
    .map((cookie) => ({
      domain: cookie.domain,
      path: cookie.path,
      value: cookie.value,
    }));
}

/**
 * Replace every refresh cookie with one for `refreshToken` at the API's own
 * cookie path on the app host, so the app, the browser-side refresh and
 * `page.request` all refresh through the same credential.
 */
export async function installRefreshCookie(
  context: BrowserContext,
  refreshToken: string,
): Promise<void> {
  expect(refreshToken, "refresh token required").toBeTruthy();
  await context.clearCookies({ name: REFRESH_COOKIE_NAME });
  if (LEGACY_REFRESH_COOKIE_NAME !== REFRESH_COOKIE_NAME) {
    await context.clearCookies({ name: LEGACY_REFRESH_COOKIE_NAME });
  }
  const app = new URL(APP_BASE_URL);
  await context.addCookies([
    {
      name: REFRESH_COOKIE_NAME,
      value: refreshToken,
      domain: app.hostname,
      path: REFRESH_COOKIE_PATH,
      httpOnly: true,
      secure: SECURE_REFRESH_COOKIE,
      sameSite: "Lax",
    },
  ]);
  await assertSingleRefreshCookie(context, "installRefreshCookie");
}

/** Exactly one refresh cookie, at the API's path: the only effective one. */
export async function assertSingleRefreshCookie(
  context: BrowserContext,
  where: string,
): Promise<void> {
  const cookies = await refreshCookies(context);
  expect(
    cookies.map((cookie) => `${cookie.domain}${cookie.path}`),
    `${where}: expected exactly one ${REFRESH_COOKIE_NAME} cookie at ${REFRESH_COOKIE_PATH}`,
  ).toEqual([`${new URL(APP_BASE_URL).hostname}${REFRESH_COOKIE_PATH}`]);
}

async function readLocalStorageValue(page: Page, key: string): Promise<string> {
  if (page.isClosed()) {
    const state = await page.context().storageState();
    const appOrigin = new URL(APP_BASE_URL).origin;
    return (
      state.origins
        .find((origin) => origin.origin === appOrigin)
        ?.localStorage.find((entry) => entry.name === key)?.value ?? ""
    );
  }
  await ensureAppOrigin(page);
  return page.evaluate((k) => localStorage.getItem(k) ?? "", key);
}

/** The bearer the browser currently holds, decoded to its subject. */
export async function readActiveSubject(page: Page): Promise<string> {
  const token = await readLocalStorageValue(page, AUTH_TOKEN_KEY);
  expect(token, "no platform token in localStorage").toBeTruthy();
  return jwtSubject(token);
}

/**
 * Record which user this context is meant to act as. Defaults to the subject
 * of the token currently held, so a context bootstrapped through OAuth (whose
 * subject is only known after login) can pin itself once.
 */
export async function recordExpectedSubject(
  page: Page,
  subject?: string,
): Promise<string> {
  const expected = subject ?? (await readActiveSubject(page));
  await ensureAppOrigin(page);
  await page.evaluate(({ key, value }) => localStorage.setItem(key, value), {
    key: EXPECTED_SUBJECT_KEY,
    value: expected,
  });
  return expected;
}

/** The recorded intended subject; pins the current one when none is recorded. */
export async function expectedSubject(page: Page): Promise<string> {
  const recorded = await readLocalStorageValue(page, EXPECTED_SUBJECT_KEY);
  if (recorded) {
    return recorded;
  }
  if (page.isClosed()) {
    return readActiveSubject(page);
  }
  return recordExpectedSubject(page);
}

/**
 * Assert that the bearer this context holds and the user the API resolves it
 * to are both the intended subject. Call before every owner-only operation
 * (fork, lifecycle, live Changes, Save) so a drifted identity fails here,
 * with a message naming the operation, instead of as a bare 403 later.
 */
export async function assertActiveSubject(
  page: Page,
  operation: string,
): Promise<string> {
  const expected = await expectedSubject(page);
  const active = await readActiveSubject(page);
  expect(
    active,
    `${operation}: bearer subject ${active} drifted from the intended identity ${expected}`,
  ).toBe(expected);
  const me = await authedApiViaRequest(page, "/api/v1/users/me");
  expect(me.status, `${operation}: /users/me ${JSON.stringify(me.body)}`).toBe(
    200,
  );
  const resolved = String(me.body?.data?.id ?? me.body?.id ?? "");
  expect(
    resolved,
    `${operation}: API resolves the bearer to ${resolved}, not the intended ${expected}`,
  ).toBe(expected);
  return expected;
}

/**
 * Install a complete identity: one refresh cookie at the API's path, the
 * bearer and login email the app reads, and the intended subject.
 */
export async function installIdentity(
  page: Page,
  identity: { jwt: string; refreshToken: string; email: string },
): Promise<string> {
  const subject = jwtSubject(identity.jwt);
  await installRefreshCookie(page.context(), identity.refreshToken);
  await ensureAppOrigin(page);
  await page.evaluate(
    ({ tokenKey, token, emailKey, email, subjectKey, sub }) => {
      localStorage.setItem(tokenKey, token);
      localStorage.setItem(emailKey, email);
      localStorage.setItem(subjectKey, sub);
    },
    {
      tokenKey: AUTH_TOKEN_KEY,
      token: identity.jwt,
      emailKey: AUTH_EMAIL_KEY,
      email: identity.email,
      subjectKey: EXPECTED_SUBJECT_KEY,
      sub: subject,
    },
  );
  return subject;
}

function assertRefreshedSubject(jwt: string, expected: string, where: string) {
  const refreshed = jwtSubject(jwt);
  if (refreshed !== expected) {
    throw new Error(
      `${where}: refresh minted a token for ${refreshed}, not the intended identity ${expected}; a second refresh credential is in effect`,
    );
  }
}

function assertBearerSubject(jwt: string, expected: string, where: string) {
  // An undecodable stored value (an aged-out fixture token) carries no
  // identity to drift; the server rejects it and the refresh path recovers.
  const subject = jwtSubjectOrEmpty(jwt);
  if (subject && subject !== expected) {
    throw new Error(
      `${where}: the stored bearer belongs to ${subject}, not the intended identity ${expected}`,
    );
  }
}

function pageNeedsAppOrigin(page: Page): boolean {
  try {
    return new URL(page.url()).origin !== new URL(APP_BASE_URL).origin;
  } catch {
    return true;
  }
}

async function ensureAppOrigin(page: Page): Promise<void> {
  if (!pageNeedsAppOrigin(page)) {
    return;
  }

  await page.goto(APP_BOOTSTRAP_URL, { waitUntil: "domcontentloaded" });
}

function authShellUrl(url: string): boolean {
  try {
    const { pathname } = new URL(url);
    return (
      pathname === "/" ||
      pathname.startsWith("/login") ||
      pathname.startsWith("/landing") ||
      pathname === "/coming-soon.html"
    );
  } catch {
    return false;
  }
}

async function pageShowsAuthShell(page: Page): Promise<boolean> {
  if (authShellUrl(page.url())) {
    return true;
  }

  return await softVisible(
    page.getByRole("link", { name: /^login$/i }).first(),
    1_000,
  );
}

/**
 * Authenticated API call through Playwright's REQUEST CONTEXT (`page.request`),
 * not the in-browser `page.evaluate` path that `authedApi` prefers.
 *
 * Use this for pre-test SETUP lookups (resolve/list/file probes) that run
 * before the workspace UI is even open: the request-context path reads the JWT
 * from the context's storage state, does not navigate, and cannot hang on an
 * in-browser `fetch` the way `page.evaluate` can under CI load. It refreshes
 * only after a 401 and preserves the expired JWT's org during that refresh.
 */
export async function authedApiViaRequest(
  page: Page,
  path: string,
  method: "GET" | "POST" | "PUT" | "PATCH" | "DELETE" = "GET",
  body?: Record<string, unknown>,
  requestOptions: { timeout?: number } = {},
): Promise<ApiResult> {
  const requestWithToken = async (token: string): Promise<ApiResult> => {
    const headers: Record<string, string> = {
      Authorization: `Bearer ${token}`,
    };
    if (method !== "GET") {
      headers["Content-Type"] = "application/json";
    }
    const response = await page.request.fetch(
      new URL(path, APP_BASE_URL).toString(),
      {
        method,
        headers,
        data: body,
        ...requestOptions,
        failOnStatusCode: false,
      },
    );
    return {
      status: response.status(),
      body: await response.json().catch(() => ({})),
    };
  };

  const storageState = await page
    .context()
    .storageState()
    .catch(() => null);
  const appOrigin = new URL(APP_BASE_URL).origin;
  const appStorage = storageState?.origins.find(
    (origin) => origin.origin === appOrigin,
  )?.localStorage;
  let token =
    appStorage?.find((entry) => entry.name === AUTH_TOKEN_KEY)?.value ?? "";
  // The subject this context is meant to act as: the recorded one, else the
  // one the current bearer carries. A refresh that yields any other subject
  // is an identity drift and must fail loudly, never proceed as that user.
  const expectedSub =
    appStorage?.find((entry) => entry.name === EXPECTED_SUBJECT_KEY)?.value ??
    (token ? jwtSubjectOrEmpty(token) : "");
  if (token) {
    // Never dispatch under a bearer that is not the intended identity.
    if (expectedSub) {
      assertBearerSubject(token, expectedSub, "authedApiViaRequest");
    }
    const result = await requestWithToken(token);
    if (result.status !== 401) {
      return result;
    }
  }

  // Preserve the browser JWT's org when an expired token needs refreshing.
  // Parallel contexts share the saved refresh cookie; an org switch in one
  // context must not make another context's request run under that org.
  let refreshOrgId = "";
  try {
    const payload = JSON.parse(
      Buffer.from(token.split(".")[1] ?? "", "base64url").toString("utf8"),
    ) as { org?: unknown };
    refreshOrgId = typeof payload.org === "string" ? payload.org : "";
  } catch {
    // An absent or deliberately aged-out test token falls back to the refresh
    // session's org, matching the browser's recovery path.
  }
  token = "";

  // Bounded retry on the cookie refresh (issue #1853). Network-level and 5xx
  // failures are transient; a definitive 401/403 means the cookie is dead.
  let refreshFailure = "";
  for (let attempt = 0; attempt < 3 && !token; attempt += 1) {
    if (attempt > 0) {
      await new Promise((resolve) => setTimeout(resolve, 250 * attempt));
    }
    const refreshRes = await page.request
      .post(new URL("/api/v1/auth/refresh", APP_BASE_URL).toString(), {
        failOnStatusCode: false,
        ...(refreshOrgId
          ? {
              headers: { "Content-Type": "application/json" },
              data: { orgId: refreshOrgId },
            }
          : {}),
      })
      .catch((error: unknown) => {
        refreshFailure = error instanceof Error ? error.message : String(error);
        return null;
      });
    if (!refreshRes) {
      continue;
    }
    if (refreshRes.ok()) {
      const refreshJson = await refreshRes.json().catch(() => ({}));
      token = refreshJson.data?.jwt ?? refreshJson.jwt ?? "";
      if (!token) {
        refreshFailure = "refresh returned ok without a jwt";
      } else if (expectedSub) {
        assertRefreshedSubject(token, expectedSub, "authedApiViaRequest");
      }
      continue;
    }
    refreshFailure = `refresh returned ${refreshRes.status()}`;
    if (refreshRes.status() === 401 || refreshRes.status() === 403) {
      // Terminal: the g5_refresh cookie is invalid or revoked.
      break;
    }
  }

  if (!token) {
    // Never issue the target request unauthenticated: a bare 401 from the
    // endpoint would masquerade as an API authorization failure and hide
    // the real cause (the session refresh is broken).
    throw new Error(
      `authedApiViaRequest could not obtain a JWT for ${method} ${path}: ${
        refreshFailure || "no refresh attempt succeeded"
      }`,
    );
  }
  return requestWithToken(token);
}

/**
 * Wait up to `timeout` ms for the locator to become visible.
 *
 * Playwright's `locator.isVisible()` returns **immediately** — its `timeout`
 * option is deprecated and ignored.  Many E2E guards were written as
 *
 *     if (await el.isVisible({ timeout: 5_000 })) { … }
 *
 * which resolves to `false` instantly if the element hasn't rendered yet,
 * silently skipping all assertions.  `softVisible` does a real wait via
 * `waitFor`, then returns `true` / `false` without throwing.
 */
export async function softVisible(
  locator: Locator,
  timeout = 5_000,
): Promise<boolean> {
  try {
    await locator.waitFor({ state: "visible", timeout });
    return true;
  } catch {
    return false;
  }
}

/** The switch at the foot of a file view between its rendered and plain text modes. */
function fileModeSwitch(page: Page): Locator {
  return page.getByTestId("center-panel").getByTestId("file-view-switch-mode");
}

/**
 * Open the shared Monaco surface, optionally entering owner edit mode. Plain
 * text is selected first so Edit starts there rather than in Rich text
 * (`g5.documents.source-editing`).
 */
export async function openFileSource(
  page: Page,
  editable = true,
): Promise<Locator> {
  const view = page.getByTestId("center-panel").getByTestId("file-view");
  await expect(view).toBeVisible();
  const toggle = fileModeSwitch(page);
  if (
    (await toggle.count()) &&
    ((await toggle.textContent()) ?? "").includes("plain text")
  )
    await toggle.click();
  if (editable) {
    // Reading shows Edit (with Discard beside it while a draft waits);
    // editing shows Done.
    const edit = view.getByTestId("file-view-edit");
    await expect(
      edit.or(view.getByTestId("file-view-done")).first(),
    ).toBeVisible();
    if (await edit.isVisible()) await edit.click();
  }
  const editor = view.getByTestId("code-editor");
  await expect(editor.locator(".monaco-editor .view-lines")).toBeVisible();
  await editor.locator(".monaco-editor .view-lines").click();
  return editor;
}

/** Replace exact file source through native keyboard input, without serialization. */
export async function replaceFileSource(
  page: Page,
  content: string,
): Promise<Locator> {
  const editor = await openFileSource(page);
  await editor.locator(".monaco-editor .view-lines").click();
  await page.keyboard.press("ControlOrMeta+A");
  if (content) await page.keyboard.insertText(content);
  else await page.keyboard.press("Backspace");
  return editor;
}

/**
 * The rendered reading view of the file, with any unsaved draft applied:
 * Done ends editing, and the switch leaves plain text
 * (`g5.documents.preview.rich-editing`).
 */
export async function openFilePreview(page: Page): Promise<Locator> {
  const view = page.getByTestId("center-panel").getByTestId("file-view");
  const done = view.getByTestId("file-view-done");
  if (await done.isVisible()) {
    await done.click();
    await expect(done).toHaveCount(0);
  }
  const toggle = fileModeSwitch(page);
  if (
    (await toggle.count()) &&
    !((await toggle.textContent()) ?? "").includes("plain text")
  )
    await toggle.click();
  const preview = view.getByTestId("file-view-preview");
  await expect(preview).toBeVisible();
  return preview;
}

export interface ApiResult {
  status: number;
  body: any;
}

type ApiEnvelope<T> = { data?: T } & T;

type WaitPollOptions = {
  timeout?: number;
  intervals?: number[];
};

function unwrapApiData<T>(body: ApiEnvelope<T> | undefined): T {
  return (body?.data ?? body ?? {}) as T;
}

function waitOptions(
  options?: number | WaitPollOptions,
  fallbackTimeout = 15_000,
  fallbackIntervals: number[] = [250, 500, 1_000],
): Required<WaitPollOptions> {
  if (typeof options === "number") {
    return { timeout: options, intervals: fallbackIntervals };
  }
  return {
    timeout: options?.timeout ?? fallbackTimeout,
    intervals: options?.intervals ?? fallbackIntervals,
  };
}

export async function waitForTaskStatus(
  page: Page,
  taskId: string,
  expected: string,
  options?: number | WaitPollOptions,
): Promise<void> {
  const { timeout, intervals } = waitOptions(options);
  await expect
    .poll(
      async () => {
        const response = await authedApi(page, `/api/v1/tasks/${taskId}`);
        const task = unwrapApiData<{ status?: string }>(response.body);
        return task.status ?? `http-${response.status}`;
      },
      {
        timeout,
        intervals,
        message: `task ${taskId} must become ${expected}`,
      },
    )
    .toBe(expected);
}

type OrgContext = { orgId: string; orgSlug: string };

/** Common test data IDs used across E2E tests */
export const USING_TEST_TOKEN_AUTH =
  !process.env.VITE_E2E_USER && process.env.E2E_USE_OAUTH !== "true";
/** The local OAuth stub is the identity provider (CI's browser lanes). */
export const USING_OAUTH_STUB =
  process.env.E2E_USE_OAUTH === "true" && !process.env.VITE_E2E_USER;
export const TEST_SELECTED_ORG = process.env.VITE_E2E_SELECTED_ORG || "g5labs";
export const TEST_MEMBER_ORG =
  process.env.VITE_E2E_MEMBER_ORG || "g5labs-member";
export const TEST_UNASSIGNED_ORG =
  process.env.VITE_E2E_UNASSIGNED_ORG || "g5labs-unassigned";
export const TEST_ORG =
  process.env.VITE_E2E_ORG ||
  process.env.VITE_E2E_TARGET_ORG ||
  (USING_TEST_TOKEN_AUTH ? TEST_MEMBER_ORG : TEST_SELECTED_ORG);
export const TEST_WORKSPACE = process.env.VITE_E2E_WORKSPACE || "e2e-test";
export const TEST_BRANCH = process.env.VITE_E2E_BRANCH || "main";

const orgContextCache = new WeakMap<Page, Map<string, OrgContext>>();
const selectedOrgCache = new WeakMap<Page, string>();

export interface WorkspaceClone {
  id: string;
  name: string;
  path: string;
}

export interface WorkspaceCreateOptions {
  orgId?: string;
  orgSlug?: string;
  namePrefix?: string;
  description?: string;
  branches?: Array<{
    repoId: string;
    branchName?: string;
    trackingMode?: string;
  }>;
}

type WorkspaceBranchMaterializationRow = {
  workspaceId?: string;
  repoId?: string;
  repoName?: string;
  repo_name?: string;
  branchName?: string;
  branch_name?: string;
  materializationStatus?: string;
  materialization_status?: string;
  headCommit?: string;
  head_commit?: string;
  trackingMode?: string;
  tracking_mode?: string;
};

/**
 * Issue an authenticated API request from the browser context.
 *
 * Legacy E2E specs used a JWT copied into localStorage during auth setup.
 * After the auth refresh changes, that cached token can drift from the live
 * session and protected routes will return 401. Refresh on demand with the
 * browser's httpOnly cookie, persist the fresh JWT back to localStorage, and
 * retry once.
 */
export async function authedApi(
  page: Page,
  path: string,
  method: "GET" | "POST" | "PUT" | "PATCH" | "DELETE" = "GET",
  body?: Record<string, unknown>,
): Promise<ApiResult> {
  for (let attempt = 0; attempt < 3; attempt += 1) {
    try {
      if (page.isClosed()) {
        return await authedApiViaRequest(page, path, method, body);
      }

      await ensureAppOrigin(page);

      // Real auth-shell pages can still redirect or tear down while browser-
      // context fetches are in flight. Keep the request-context fallback for
      // that case, but preserve the browser's existing token on normal app
      // pages so org-scoped setup calls all share the same auth context.
      if (await pageShowsAuthShell(page)) {
        return await authedApiViaRequest(page, path, method, body);
      }

      const browserResult = await page.evaluate(
        async ({ path: apiPath, method: apiMethod, body: apiBody }) => {
          async function refreshJwt(staleToken: string): Promise<string> {
            let orgId = "";
            try {
              const encoded = staleToken.split(".")[1] ?? "";
              const base64 = encoded.replace(/-/g, "+").replace(/_/g, "/");
              const padded = base64.padEnd(
                base64.length + ((4 - (base64.length % 4)) % 4),
                "=",
              );
              const payload = JSON.parse(atob(padded)) as { org?: unknown };
              orgId = typeof payload.org === "string" ? payload.org : "";
            } catch {
              // Let an undecodable test token recover from the refresh
              // session's current org.
            }
            const refreshRes = await fetch("/api/v1/auth/refresh", {
              method: "POST",
              credentials: "include",
              ...(orgId
                ? {
                    headers: { "Content-Type": "application/json" },
                    body: JSON.stringify({ orgId }),
                  }
                : {}),
            });
            if (!refreshRes.ok) {
              return "";
            }

            const refreshJson = await refreshRes.json().catch(() => ({}));
            const refreshed = refreshJson.data?.jwt ?? refreshJson.jwt ?? "";
            if (refreshed) {
              // Identity contract (Z0): a refresh that yields another user is
              // an identity drift. Never store it and never retry under it;
              // fail here, before any mutation runs as that user.
              assertIntendedSubject(refreshed, "refreshed bearer", true);
              localStorage.setItem("g5_auth_token", refreshed);
            }
            return refreshed;
          }

          function subjectOf(jwt: string): string {
            try {
              const encoded = jwt.split(".")[1] ?? "";
              const base64 = encoded.replace(/-/g, "+").replace(/_/g, "/");
              const padded = base64.padEnd(
                base64.length + ((4 - (base64.length % 4)) % 4),
                "=",
              );
              const payload = JSON.parse(atob(padded)) as { sub?: unknown };
              return typeof payload.sub === "string" ? payload.sub : "";
            } catch {
              return "";
            }
          }

          function assertIntendedSubject(
            jwt: string,
            what: string,
            requireDecodable: boolean,
          ): void {
            const expected =
              localStorage.getItem("g5_e2e_expected_subject") ?? "";
            if (!expected) {
              return;
            }
            const subject = subjectOf(jwt);
            // A stored value that is not a decodable token (an aged-out
            // fixture value) carries no identity to drift; the server
            // rejects it and the refresh path recovers. A refreshed bearer
            // must always decode to the intended subject.
            if (!subject && !requireDecodable) {
              return;
            }
            if (subject !== expected) {
              throw new Error(
                `authedApi identity drift: ${what} belongs to ${subject || "<undecodable>"}, not the intended identity ${expected}`,
              );
            }
          }

          async function send(token: string): Promise<ApiResult> {
            const headers: Record<string, string> = {};
            if (apiMethod !== "GET") {
              headers["Content-Type"] = "application/json";
            }
            if (token) {
              headers.Authorization = `Bearer ${token}`;
            }

            const res = await fetch(apiPath, {
              method: apiMethod,
              credentials: "include",
              headers,
              body: apiBody ? JSON.stringify(apiBody) : undefined,
            });
            const json = await res.json().catch(() => ({}));
            return { status: res.status, body: json };
          }

          let token = localStorage.getItem("g5_auth_token") ?? "";
          if (token) {
            assertIntendedSubject(token, "stored bearer", false);
          }
          let result = await send(token);

          if (result.status === 401) {
            token = await refreshJwt(token);
            if (token) {
              result = await send(token);
            }
          }

          return result;
        },
        { path, method, body },
      );

      // A 401 after an in-browser refresh that could not produce a token
      // (transient fetch failure, SPA boot race) does not mean the session
      // is dead: the context's g5_refresh cookie stays valid across
      // sequential test contexts. Recover through the request-context path,
      // which retries the cookie refresh with backoff (issue #1853),
      // instead of surfacing an unretried 401 to the caller.
      if (browserResult.status === 401) {
        return await authedApiViaRequest(page, path, method, body);
      }
      // The browser-side refresh may have replaced the bearer; it must still
      // belong to the intended identity.
      const expected = await readLocalStorageValue(page, EXPECTED_SUBJECT_KEY);
      if (expected) {
        const current = await readLocalStorageValue(page, AUTH_TOKEN_KEY);
        if (current) {
          assertRefreshedSubject(current, expected, "authedApi");
        }
      }
      return browserResult;
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      const pageLifecycleError =
        message.includes("Test ended") ||
        message.includes("Target page, context or browser has been closed") ||
        message.includes("has been closed");
      const transientNavigationError =
        message.includes("Execution context was destroyed") ||
        message.includes("Cannot find context with specified id") ||
        message.includes("ERR_ADDRESS_UNREACHABLE");
      const transientFetchError =
        message.includes("Failed to fetch") ||
        message.includes("NetworkError when attempting to fetch resource");
      const retryable = transientNavigationError || transientFetchError;

      if (pageLifecycleError || message.includes("ERR_ADDRESS_UNREACHABLE")) {
        return await authedApiViaRequest(page, path, method, body);
      }

      if (!retryable || attempt === 2) {
        throw error;
      }

      await page.waitForTimeout(250 * (attempt + 1));
      if (!page.isClosed()) {
        await page.waitForLoadState("domcontentloaded").catch(() => {});
        await ensureAppOrigin(page).catch(() => {});
      }
    }
  }

  throw new Error(`authedApi failed unexpectedly for ${method} ${path}`);
}

export type ChatTurnIdentity = {
  sessionId: string;
  inputId: string;
  taskId: string;
  workspaceId: string;
};

type ChatSessionTask = {
  id: string;
  status: string;
};

function unwrapResponseData<T>(body: unknown): T {
  if (body !== null && typeof body === "object" && "data" in body) {
    return (body as { data: T }).data;
  }
  return body as T;
}

async function enrichChatTurnIdentity(
  page: Page,
  turn: ChatTurnIdentity,
): Promise<ChatTurnIdentity> {
  if (!turn.workspaceId) {
    const sessionResult = await authedApi(
      page,
      `/api/v1/sessions/${turn.sessionId}`,
    );
    expect(
      sessionResult.status,
      `session detail: ${JSON.stringify(sessionResult.body)}`,
    ).toBe(200);
    turn.workspaceId = String(
      unwrapResponseData<{ workspaceId?: string }>(sessionResult.body)
        .workspaceId ?? "",
    );
    expect(
      turn.workspaceId,
      "chat session must expose its workspace id",
    ).not.toBe("");
  }

  if (!turn.taskId) {
    await expect
      .poll(
        async () => {
          const result = await authedApi(
            page,
            `/api/v1/sessions/${turn.sessionId}/task`,
          );
          if (result.status !== 200) return "";
          turn.taskId =
            unwrapResponseData<ChatSessionTask | null>(result.body)?.id ?? "";
          return turn.taskId;
        },
        { timeout: 15_000, intervals: [250, 500, 1_000] },
      )
      .not.toBe("");
  }

  return turn;
}

/**
 * Retain the exact durable identities created by one successful chat send.
 * The callback runs as soon as the response exposes the session/input pair,
 * before fallible task/workspace enrichment, so cleanup always owns the turn.
 */
export async function captureChatTurn(
  page: Page,
  sendResponse: Response,
  onTurnCreated: (turn: ChatTurnIdentity) => void = () => {},
): Promise<ChatTurnIdentity> {
  expect(
    sendResponse.ok(),
    `chat send failed with HTTP ${sendResponse.status()}: ${await sendResponse.text()}`,
  ).toBeTruthy();

  const sendPath = new URL(sendResponse.url()).pathname;
  const sendBody = unwrapResponseData<Record<string, unknown>>(
    await sendResponse.json(),
  );
  let sessionId: string;
  let inputId: string;
  let taskId = "";
  let workspaceId = "";
  if (sendPath.endsWith("/sessions/start")) {
    const session = sendBody.session as
      | { id?: string; workspaceId?: string }
      | undefined;
    sessionId = session?.id ?? "";
    workspaceId = session?.workspaceId ?? "";
    const message = sendBody.message as Record<string, unknown> | undefined;
    inputId = String(
      message?.inputId ??
        message?.messageId ??
        sendBody.assistantMessageId ??
        "",
    );
    taskId = String(sendBody.taskId ?? "");
  } else {
    const match = sendPath.match(/\/sessions\/([^/]+)\/messages$/);
    sessionId = match?.[1] ?? "";
    inputId = String(sendBody.inputId ?? sendBody.messageId ?? "");
    taskId = String(sendBody.taskId ?? "");
  }
  expect(sessionId, "chat send must expose its exact session id").not.toBe("");
  expect(inputId, "chat send must expose its exact queued input id").not.toBe(
    "",
  );
  const turn = { sessionId, inputId, taskId, workspaceId };
  onTurnCreated(turn);
  return enrichChatTurnIdentity(page, turn);
}

type TranscriptMessage = {
  id: string;
  role: string;
  messageIndex: number;
  taskId?: string | null;
};

class TransientTranscriptReadError extends Error {
  constructor(status: number) {
    super(`Chat transcript read failed with HTTP ${status}`);
  }
}

async function readTranscriptMessages(
  page: Page,
  sessionId: string,
): Promise<TranscriptMessage[]> {
  const messages: TranscriptMessage[] = [];
  let cursor: number | undefined;
  for (;;) {
    const query = new URLSearchParams({ limit: "500" });
    if (cursor !== undefined) query.set("cursor", String(cursor));
    const result = await authedApi(
      page,
      `/api/v1/sessions/${sessionId}/messages?${query}`,
    );
    if (result.status >= 500 || result.status === 408)
      throw new TransientTranscriptReadError(result.status);
    expect(result.status, "chat transcript must be readable").toBe(200);
    const batch = unwrapResponseData<{
      messages: TranscriptMessage[];
      cursor: number | null;
      hasMore: boolean;
    }>(result.body);
    messages.push(...batch.messages);
    if (!batch.hasMore) return messages;
    expect(batch.cursor, "chat transcript pagination must advance").not.toBe(
      cursor ?? null,
    );
    expect(batch.cursor).not.toBeNull();
    cursor = batch.cursor!;
  }
}

/**
 * A send returns the queued input ID, not the native assistant row ID.
 * Acceptance persists that input's user/system message immediately followed by
 * its assistant placeholder, including steered replies. Read the actual reply
 * IDs for this task after that input, never by recency or identical content.
 * A native question can resume in another attempt of the same task. Stop at
 * the next human input so another input's continuation cannot satisfy this wait.
 * Fail at once on a confirmed failed/cancelled task before acceptance, or an
 * error in that exact reply (g5.quality.delivery.ci.provider-failure-bound).
 */
export async function expectAssistantReply(
  page: Page,
  turn: Pick<ChatTurnIdentity, "taskId" | "sessionId" | "inputId">,
  text: string,
  {
    timeout,
    readTaskStatus = () => readTaskStatusById(page, turn.taskId),
    readMessages = () => readTranscriptMessages(page, turn.sessionId),
  }: {
    timeout: number;
    readTaskStatus?: () => Promise<string | undefined>;
    readMessages?: () => Promise<TranscriptMessage[]>;
  },
): Promise<void> {
  let assistantMessageIds: string[] = [];
  let consecutiveReadFailures = 0;
  let lastReadError: TransientTranscriptReadError | undefined;
  let message = page.locator(
    '[data-testid="assistant-message"][data-message-id=""]',
  );
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    let messages: TranscriptMessage[] | undefined;
    try {
      messages = await readMessages();
      consecutiveReadFailures = 0;
      lastReadError = undefined;
    } catch (error) {
      // authedApi already bounds network retries. Only transient HTTP reads
      // recover here; auth, schema and assertion failures still fail at once.
      if (!(error instanceof TransientTranscriptReadError)) throw error;
      lastReadError = error;
      consecutiveReadFailures += 1;
    }
    const input = messages?.find(
      (row) => row.id === turn.inputId && row.role !== "assistant",
    );
    if (input && messages) {
      const nextHumanIndex = Math.min(
        ...messages
          .filter(
            (row) =>
              row.role === "user" && row.messageIndex > input.messageIndex,
          )
          .map((row) => row.messageIndex),
      );
      assistantMessageIds = messages
        .filter(
          (row) =>
            row.role === "assistant" &&
            row.taskId === turn.taskId &&
            row.messageIndex > input.messageIndex &&
            row.messageIndex < nextHumanIndex,
        )
        .sort((a, b) => a.messageIndex - b.messageIndex)
        .map((row) => row.id);
    }
    if (assistantMessageIds.length) {
      const selector = (id: string) =>
        `[data-testid="assistant-message"][data-message-id="${id}"]`;
      const content = await page
        .locator(assistantMessageIds.map(selector).join(","))
        .allTextContents();
      if (content.some((body) => body.includes(text))) return;
      message = page.locator(selector(assistantMessageIds.at(-1)!));
    }
    const failure = message.locator(
      '[data-testid="assistant-status-block"][data-status-kind="error"]',
    );
    if (!assistantMessageIds.length || (await failure.count()) > 0) {
      const status = await readTaskStatus();
      if (status === "failed" || status === "cancelled") {
        const reported =
          (await failure.allInnerTexts()).join(" | ") ||
          "input was not accepted";
        throw new Error(
          `The provider turn ${status} before replying "${text}": ${reported}`,
        );
      }
    }
    if (lastReadError && consecutiveReadFailures >= 3) throw lastReadError;
    await page.waitForTimeout(1_000);
  }
  if (lastReadError) throw lastReadError;
  expect(
    assistantMessageIds.length,
    `accepted reply for input ${turn.inputId}`,
  ).toBeTruthy();
  await expect(message).toContainText(text, { timeout: 1_000 });
}

async function readTaskStatusById(
  page: Page,
  taskId: string,
): Promise<string | undefined> {
  if (!taskId) return undefined;
  const result = await authedApi(page, `/api/v1/tasks/${taskId}`);
  if (result.status !== 200) return undefined;
  return unwrapResponseData<{ status?: string } | null>(result.body)?.status;
}

/**
 * Stop one exact chat turn and wait until neither its live task nor its sticky
 * capacity occupant remains. Assistant completion alone does not release a VM.
 */
export async function releaseChatTurn(
  page: Page,
  turn: ChatTurnIdentity,
): Promise<void> {
  const stop = await authedApi(
    page,
    `/api/v1/sessions/${turn.sessionId}/messages/${turn.inputId}/stop`,
    "POST",
    {},
  );
  expect(
    [200, 409].includes(stop.status),
    `chat stop cleanup returned HTTP ${stop.status}: ${JSON.stringify(stop.body)}`,
  ).toBeTruthy();

  await enrichChatTurnIdentity(page, turn);
  // Stop ends the turn (`g5.agents.lifecycle.chat-stop`): a queued or held turn
  // at once, a running one once its attempt stops. Wait for that, so the VM's
  // sticky capacity is released.
  await expect
    .poll(
      async () => {
        const result = await authedApi(
          page,
          `/api/v1/sessions/${turn.sessionId}/task`,
        );
        if (result.status === 404) return true;
        expect(result.status).toBe(200);
        const task = unwrapResponseData<ChatSessionTask | null>(result.body);
        return (
          task === null ||
          ["succeeded", "failed", "cancelled"].includes(task.status)
        );
      },
      { timeout: 120_000, intervals: [500, 1_000, 2_000] },
    )
    .toBe(true);
}

export async function resolveWorkspaceId(
  page: Page,
  org = TEST_ORG,
  workspace = TEST_WORKSPACE,
): Promise<string> {
  await ensureAuthOrgForSlug(page, org);

  const resolvePath = `/api/v1/resolve?org=${encodeURIComponent(org)}&workspace=${encodeURIComponent(workspace)}`;

  let result = await authedApi(page, resolvePath);

  // A freshly created/cloned workspace can transiently 404 here while the
  // clone-propagation/projection catches up (the resolve row is not yet
  // visible to this session). Bounded poll-retry on 404 so propagation lag is
  // retried rather than asserted; the happy path (200) returns immediately and
  // is unchanged, so sibling specs are unaffected. Non-404 failures and an
  // exhausted budget fall through to the strict assertion below.
  const resolveDeadline = Date.now() + 20_000;
  let resolveBackoff = 500;
  while (result.status === 404 && Date.now() < resolveDeadline) {
    await page.waitForTimeout(resolveBackoff);
    resolveBackoff = Math.min(resolveBackoff * 2, 2_000);
    result = await authedApi(page, resolvePath);
  }

  expect(result.status).toBe(200);
  const data = result.body?.data ?? result.body ?? {};
  const workspaceId = data.workspaceId ?? data.workspace_id ?? "";
  expect(workspaceId).toBeTruthy();
  return workspaceId;
}

export async function switchAuthOrg(page: Page, orgId: string): Promise<void> {
  expect(orgId, "org id required for auth switch").toBeTruthy();
  selectedOrgCache.delete(page);

  if (page.isClosed()) {
    throw new Error(`Cannot switch auth org to ${orgId} on a closed page`);
  }

  const refreshRes = await page.request.post(
    new URL("/api/v1/auth/refresh", APP_BASE_URL).toString(),
    {
      failOnStatusCode: false,
      headers: {
        "Content-Type": "application/json",
      },
      data: { orgId },
    },
  );

  if (!refreshRes.ok()) {
    throw new Error(`org switch refresh failed with ${refreshRes.status()}`);
  }

  const refreshJson = await refreshRes.json().catch(() => ({}));
  const jwt = refreshJson.data?.jwt ?? refreshJson.jwt ?? "";
  if (!jwt) {
    throw new Error("org switch refresh response did not include jwt");
  }
  // Switching org must never switch user.
  assertRefreshedSubject(jwt, await expectedSubject(page), "switchAuthOrg");

  let lastError: unknown = null;

  for (let attempt = 0; attempt < 3; attempt += 1) {
    if (page.isClosed()) {
      throw new Error(
        `Cannot persist switched-org auth on a closed page for ${orgId}`,
      );
    }

    await ensureAppOrigin(page);

    try {
      await page.evaluate((token) => {
        localStorage.setItem("g5_auth_token", token);
      }, jwt);
      return;
    } catch (error) {
      lastError = error;
      const message = error instanceof Error ? error.message : String(error);
      const retryable =
        message.includes("Execution context was destroyed") ||
        message.includes("Cannot find context with specified id") ||
        message.includes("interrupted by another navigation");

      if (!retryable || attempt === 2) {
        throw error;
      }

      await page.waitForTimeout(250 * (attempt + 1));
      await page.waitForLoadState("domcontentloaded").catch(() => {});
    }
  }

  throw lastError instanceof Error
    ? lastError
    : new Error(`Failed to persist switched-org auth for ${orgId}`);
}

export async function waitForWorkspaceResolution(
  page: Page,
  orgSlug: string,
  workspaceName: string,
  options?: {
    expectedWorkspaceId?: string;
    timeout?: number;
  },
): Promise<string> {
  await ensureAuthOrgForSlug(page, orgSlug);

  const timeout = options?.timeout ?? 15_000;
  const deadline = Date.now() + timeout;
  let lastStatus = 0;
  let lastWorkspaceId = "";

  while (Date.now() < deadline) {
    const result = await authedApi(
      page,
      `/api/v1/resolve?org=${encodeURIComponent(orgSlug)}&workspace=${encodeURIComponent(workspaceName)}`,
    );
    lastStatus = result.status;
    const data = result.body?.data ?? result.body ?? {};
    lastWorkspaceId = data.workspaceId ?? data.workspace_id ?? "";
    if (
      result.status === 200 &&
      lastWorkspaceId &&
      (!options?.expectedWorkspaceId ||
        lastWorkspaceId === options.expectedWorkspaceId)
    ) {
      return lastWorkspaceId;
    }
    await page.waitForTimeout(500);
  }

  throw new Error(
    `Workspace ${orgSlug}/${workspaceName} did not resolve in time. ` +
      `Last status=${lastStatus}, last workspace id=${lastWorkspaceId || "(none)"}, ` +
      `expected=${options?.expectedWorkspaceId || "(any)"}`,
  );
}

export async function waitForWorkspaceMaterialization(
  page: Page,
  workspaceId: string,
  options?: {
    timeout?: number;
    minimumBranches?: number;
  },
): Promise<void> {
  const timeout = options?.timeout ?? 15_000;
  const minimumBranches = options?.minimumBranches ?? 1;
  const deadline = Date.now() + timeout;
  let lastBranchStatus = 0;
  let lastFileStatuses = "";
  let lastBranchCount = 0;
  let lastMaterializedBranchCount = 0;
  let lastBranchStates = "";

  while (Date.now() < deadline) {
    const branchesRes = await authedApi(
      page,
      `/api/v1/workspaces/${workspaceId}/branches`,
    );
    const branchRows = (
      Array.isArray(branchesRes.body?.data)
        ? branchesRes.body.data
        : Array.isArray(branchesRes.body)
          ? branchesRes.body
          : []
    ) as WorkspaceBranchMaterializationRow[];
    lastBranchStatus = branchesRes.status;
    lastBranchCount = branchRows.length;
    lastMaterializedBranchCount = branchRows.filter((branch) => {
      const status =
        branch.materializationStatus ?? branch.materialization_status ?? "";
      return status === "materialized";
    }).length;
    lastBranchStates = branchRows
      .map((branch) => {
        const name = branch.branchName ?? branch.branch_name ?? "(unnamed)";
        const status =
          branch.materializationStatus ??
          branch.materialization_status ??
          "(none)";
        return `${name}:${status}`;
      })
      .join(", ");
    const materialized = branchRows.filter(
      (branch) =>
        (branch.materializationStatus ?? branch.materialization_status) ===
        "materialized",
    );
    const fileTrees = await Promise.all(
      materialized.map((branch) =>
        authedApi(
          page,
          `/api/v1/workspaces/${workspaceId}/repos/${branch.repoId}/files`,
        ),
      ),
    );
    lastFileStatuses = fileTrees.map((response) => response.status).join(", ");

    if (
      branchesRes.status === 200 &&
      branchRows.length >= minimumBranches &&
      lastMaterializedBranchCount >= minimumBranches &&
      fileTrees.length >= minimumBranches &&
      fileTrees.every((response) => response.status === 200)
    ) {
      return;
    }

    await page.waitForTimeout(500);
  }

  throw new Error(
    `Workspace ${workspaceId} did not finish materialization in time. ` +
      `Last branches status=${lastBranchStatus} ` +
      `(count=${lastBranchCount}, materialized=${lastMaterializedBranchCount}, states=${lastBranchStates || "(none)"}), ` +
      `last Files tree statuses=${lastFileStatuses || "(none)"}.`,
  );
}

export function workspaceCloneName(prefix: string): string {
  const normalized = prefix
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 32);
  const safePrefix = normalized || "e2e-clone";
  return `${safePrefix}-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

export async function cloneWorkspaceById(
  page: Page,
  parentWorkspaceId: string,
  options?: {
    name?: string;
    namePrefix?: string;
    orgSlug?: string;
    description?: string;
    start?: boolean;
    source?: "durable" | "live";
    interrupt?: boolean;
  },
): Promise<WorkspaceClone> {
  const cloneName =
    options?.name ?? workspaceCloneName(options?.namePrefix ?? "e2e-clone");
  // A retry resumes the same durable fork operation. Keep one key for this
  // helper invocation instead of minting a new operation on every poll.
  const idempotencyKey = randomUUID();
  // Forking is owner-scoped on the child: pin who is about to own it.
  await assertActiveSubject(page, `fork ${parentWorkspaceId}`);
  const clonePayload = () => ({
    name: cloneName,
    idempotencyKey,
    ...(options?.source ? { source: options.source } : {}),
    ...(options?.interrupt !== undefined
      ? { interrupt: options.interrupt }
      : {}),
    description:
      options?.description ??
      `Isolated browser E2E clone of ${parentWorkspaceId}`,
  });

  // The clone endpoint can transiently return 404 while the parent projection
  // catches up, or 503 when a retryable database conflict aborts an attempt.
  // Both are safe to retry only because every attempt carries the same key.
  const cloneDeadline = Date.now() + 20_000;
  // `vm.api.create` is the server-side clone (the POST round trips, retries
  // included); `vm.clone-materialize` is the wait for the child to resolve
  // and materialize. Both are evidence apart from the boot wait
  // (g5.quality.delivery.ci.timing-evidence).
  const createStarted = Date.now();
  let result = await authedApi(
    page,
    `/api/v1/workspaces/${parentWorkspaceId}/clone`,
    "POST",
    clonePayload(),
  );
  while ([404, 503].includes(result.status) && Date.now() < cloneDeadline) {
    await page.waitForTimeout(500);
    result = await authedApi(
      page,
      `/api/v1/workspaces/${parentWorkspaceId}/clone`,
      "POST",
      clonePayload(),
    );
  }
  recordTiming("vm.api.create", Date.now() - createStarted);
  expect([200, 201], JSON.stringify(result.body)).toContain(result.status);

  const data = result.body?.data ?? result.body ?? {};
  const workspaceId = data.workspace?.id ?? data.id ?? "";
  const clonedBranches = Array.isArray(data.workspace?.branches)
    ? data.workspace.branches
    : Array.isArray(data.branches)
      ? data.branches
      : [];
  expect(workspaceId).toBeTruthy();

  await timed("vm.clone-materialize", async () => {
    if (options?.orgSlug) {
      await waitForWorkspaceResolution(page, options.orgSlug, cloneName, {
        expectedWorkspaceId: workspaceId,
      });
    }
    await waitForWorkspaceMaterialization(page, workspaceId, {
      minimumBranches: Math.max(1, clonedBranches.length),
    });
  });
  if (options?.start) {
    await runWorkspaceVmAction(page, workspaceId, "resume");
    await waitForWorkspaceVmStatus(page, workspaceId, "running");
  }

  return {
    id: workspaceId,
    name: cloneName,
    path: `/${options?.orgSlug ?? TEST_ORG}/${cloneName}`,
  };
}

export async function createWorkspace(
  page: Page,
  options?: WorkspaceCreateOptions,
): Promise<WorkspaceClone> {
  const targetOrg = options?.orgSlug
    ? await readOrgContext(page, options.orgSlug)
    : await readCurrentOrgContext(page);
  if (options?.orgId && options.orgId !== targetOrg.orgId) {
    throw new Error(
      `createWorkspace org identity mismatch: slug '${targetOrg.orgSlug}' resolves to '${targetOrg.orgId}', not '${options.orgId}'`,
    );
  }
  const orgId = options?.orgId ?? targetOrg.orgId;
  const orgSlug = options?.orgSlug ?? targetOrg.orgSlug;
  const workspaceName = workspaceCloneName(
    options?.namePrefix ?? "e2e-workspace",
  );
  if (options?.branches?.length) {
    throw new Error(
      "createWorkspace cannot select branch members after the base-fork cutover",
    );
  }

  await switchAuthOrg(page, orgId);
  const listed = await authedApi(page, `/api/v1/orgs/${orgId}/workspaces`);
  expect(
    listed.status,
    `list workspaces for '${orgSlug}' failed: ${JSON.stringify(listed.body)}`,
  ).toBe(200);
  const rows = listed.body?.data ?? listed.body ?? [];
  const base = Array.isArray(rows)
    ? rows.find(
        (workspace: Record<string, unknown>) =>
          workspace.isBase === true || workspace.is_base === true,
      )
    : undefined;
  const baseId = String(base?.id ?? "");
  expect(
    baseId,
    `organization '${orgSlug}' has no base workspace`,
  ).toBeTruthy();

  return cloneWorkspaceById(page, baseId, {
    name: workspaceName,
    orgSlug,
    description: options?.description,
    start: true,
  });
}

export async function createWorkspaceClone(
  page: Page,
  options?: {
    org?: string;
    workspace?: string;
    namePrefix?: string;
    description?: string;
    start?: boolean;
  },
): Promise<WorkspaceClone> {
  const org = options?.org ?? TEST_ORG;
  const workspace = options?.workspace ?? TEST_WORKSPACE;
  const parentWorkspaceId = await resolveWorkspaceId(page, org, workspace);
  return cloneWorkspaceById(page, parentWorkspaceId, {
    namePrefix: options?.namePrefix,
    orgSlug: org,
    description:
      options?.description ?? `Isolated browser E2E clone of ${workspace}`,
    start: options?.start ?? false,
  });
}

export interface WorkspaceVmState {
  status: string;
  vmId?: string | null;
  epoch?: number | null;
  lastFlushId?: string | null;
  unflushedLossAt?: string | null;
  /** The pending start (`WorkspaceVmView.start_intent`): its `status` moves
   * pending → queued_for_capacity → consumed; `created_at` is when the API
   * admitted the resume. */
  startIntent?: { status?: string; created_at?: string } | null;
  /** Set from the host's `ready` event timestamp when the API flips the VM
   * to `running`. */
  startedAt?: string | null;
}

export async function readWorkspaceVm(
  page: Page,
  workspaceId: string,
): Promise<WorkspaceVmState> {
  const response = await authedApiViaRequest(
    page,
    `/api/v1/workspaces/${workspaceId}/vm`,
  );
  expect(response.status, JSON.stringify(response.body)).toBe(200);
  return (response.body?.data ?? response.body) as WorkspaceVmState;
}

export async function waitForWorkspaceVmStatus(
  page: Page,
  workspaceId: string,
  expectedStatus: string,
  timeout = 180_000,
): Promise<WorkspaceVmState> {
  let latest: WorkspaceVmState = { status: "unknown" };
  // Capacity waits are reported apart from execution
  // (g5.quality.delivery.ci.timing-evidence); the wait itself is unchanged.
  let capacityWaitMs = 0;
  let queuedSince: number | undefined;
  const settleCapacityWait = () => {
    if (queuedSince === undefined) return;
    capacityWaitMs += Date.now() - queuedSince;
    queuedSince = undefined;
  };
  // Transition evidence from the fields the poll already reads: each row
  // status and start-intent sub-status first seen after the first poll is
  // recorded as `vm.<status>` / `vm.intent.<status>` with its offset from
  // the start of this wait (the first poll only reports the status the wait
  // began in, so it is recorded only when it already is the target). When
  // the target is `running`, the server's own intent-admitted → host-ready
  // interval and the lag from host-ready to this poll seeing `running` are
  // recorded beside it, which separates the boot from the poll overshoot.
  const waitStarted = Date.now();
  const seen = new Set<string>();
  const noteTransitions = (state: WorkspaceVmState, firstPoll: boolean) => {
    try {
      const now = Date.now();
      const marks = [`vm.${state.status}`];
      const intentStatus = state.startIntent?.status;
      if (typeof intentStatus === "string")
        marks.push(`vm.intent.${intentStatus}`);
      for (const mark of marks) {
        if (seen.has(mark)) continue;
        seen.add(mark);
        if (!firstPoll || state.status === expectedStatus)
          recordTiming(mark, now - waitStarted);
      }
      if (
        expectedStatus === "running" &&
        state.status === "running" &&
        !seen.has("vm.server")
      ) {
        seen.add("vm.server");
        const readyAt = Date.parse(state.startedAt ?? "");
        const admittedAt = Date.parse(state.startIntent?.created_at ?? "");
        if (Number.isFinite(readyAt)) {
          if (Number.isFinite(admittedAt))
            recordTiming("vm.server.intent->ready", readyAt - admittedAt);
          recordTiming("vm.server.ready->running-seen", now - readyAt);
        }
      }
    } catch {
      // Evidence only: never fail or slow the wait.
    }
  };
  let polls = 0;
  try {
    await expect
      .poll(
        async () => {
          latest = await readWorkspaceVm(page, workspaceId);
          noteTransitions(latest, polls++ === 0);
          if (latest.status === "queued_for_capacity")
            queuedSince ??= Date.now();
          else settleCapacityWait();
          return latest.status;
        },
        { timeout, intervals: [500, 1_000, 2_000] },
      )
      .toBe(expectedStatus);
  } catch (error) {
    settleCapacityWait();
    if (error instanceof Error && capacityWaitMs > 0) {
      error.message += `\nWorkspace ${workspaceId} was queued_for_capacity for ${Math.round(capacityWaitMs / 1000)} s of this ${Math.round(timeout / 1000)} s wait; last status ${latest.status}.`;
    }
    throw error;
  } finally {
    settleCapacityWait();
    if (capacityWaitMs > 0) recordTiming("vm.capacity-wait", capacityWaitMs);
  }
  return latest;
}

/**
 * The preview dialog's picker auto-selects only when one application is
 * configured; the seeded repository also carries the `noisy-build` profile,
 * so flows that want the workspace default choose it explicitly.
 */
export async function selectWorkspaceDefaultPreview(page: Page): Promise<void> {
  const picker = page.getByLabel("Preview application");
  await expect(picker).toBeVisible({ timeout: 30_000 });
  const option = picker.locator("option", { hasText: "Workspace default" });
  await expect(option).toHaveCount(1, { timeout: 30_000 });
  await picker.selectOption((await option.getAttribute("value")) ?? "");
}

export async function runWorkspaceVmAction(
  page: Page,
  workspaceId: string,
  action: "resume" | "stop",
): Promise<void> {
  // `vm.api.start` / `vm.api.stop`: the lifecycle POST round trips (count =
  // attempts). A resume reply waits for the host to answer the start command,
  // which it does once the VM is ready, so most of a boot lands here rather
  // than in the following status wait (g5.quality.delivery.ci.timing-evidence).
  const phase = action === "resume" ? "vm.api.start" : "vm.api.stop";
  await requestWorkspaceVmAction(
    async (operationId) => {
      // Every attempt retains the intended actor and lifecycle operation.
      await assertActiveSubject(page, `workspace ${action} ${workspaceId}`);
      return timed(phase, () =>
        authedApiViaRequest(
          page,
          `/api/v1/workspaces/${workspaceId}/${action}`,
          "POST",
          { operationId },
          // Stop can spend 120 s saving, then two 60 s host waits. The
          // request must outlive those server bounds plus admission overhead;
          // transport errors still fail instead of replaying an unknown result.
          action === "stop" ? { timeout: 300_000 } : {},
        ),
      );
    },
    (milliseconds) =>
      timed("vm.database-retry", () => page.waitForTimeout(milliseconds)),
  );
}

export interface LiveWorkspaceChangeMember {
  repo_id: string;
  root_path: string;
  shared_branch: string;
  head: string | null;
  head_oid: string | null;
  internal_ref: string | null;
  errors: string[];
  dirty: Array<{ status: string; path: string }> | null;
  against_internal?: { ahead: number | null; behind: number | null } | null;
}

export async function readWorkspaceChanges(
  page: Page,
  workspaceId: string,
): Promise<{
  source: "live" | "durable";
  attached: boolean;
  members: LiveWorkspaceChangeMember[];
}> {
  // The live Changes report is served only to the workspace's current owner.
  await assertActiveSubject(page, `workspace changes ${workspaceId}`);
  const response = await authedApiViaRequest(
    page,
    `/api/v1/workspaces/${workspaceId}/changes`,
  );
  expect(response.status, JSON.stringify(response.body)).toBe(200);
  return response.body?.data ?? response.body;
}

export async function waitForLiveWorkspaceChanges(
  page: Page,
  workspaceId: string,
  timeout = 60_000,
): Promise<Awaited<ReturnType<typeof readWorkspaceChanges>>> {
  // `vm.live-changes`: first poll to the live report being served
  // (g5.quality.delivery.ci.timing-evidence).
  const started = Date.now();
  let latest = await readWorkspaceChanges(page, workspaceId);
  await expect
    .poll(
      async () => {
        latest = await readWorkspaceChanges(page, workspaceId);
        return `${latest.source}:${latest.attached}`;
      },
      { timeout, intervals: [500, 1_000] },
    )
    .toBe("live:true");
  recordTiming("vm.live-changes", Date.now() - started);
  return latest;
}

export async function deleteWorkspaceById(
  page: Page,
  workspaceId: string,
): Promise<void> {
  if (!workspaceId) {
    return;
  }
  const operationId = randomUUID();
  const deadline = Date.now() + 90_000;
  let latestStatus = 0;
  let latestBody: unknown;
  while (Date.now() < deadline) {
    const result = await authedApi(
      page,
      `/api/v1/workspaces/${workspaceId}/delete`,
      "POST",
      { force: true, operationId },
    );
    latestStatus = result.status;
    latestBody = result.body;
    if (result.status === 204 || result.status === 404) {
      return;
    }
    expect(
      [409, 503],
      `workspace cleanup failed: ${JSON.stringify(result.body)}`,
    ).toContain(result.status);
    await page.waitForTimeout(1_000);
  }
  throw new Error(
    `workspace cleanup timed out with ${latestStatus}: ${JSON.stringify(latestBody)}`,
  );
}

export async function readCurrentOrgId(page: Page): Promise<string> {
  const context = await readCurrentOrgContext(page);
  return context.orgId;
}

export async function seedWorkspaceCodexCredential(
  page: Page,
  workspaceId: string,
): Promise<void> {
  const orgId = await readCurrentOrgId(page);
  const result = await authedApi(
    page,
    `/api/v1/workspaces/${workspaceId}/config?orgId=${orgId}`,
    "PUT",
    {
      setSecrets: {
        OPENAI_API_KEY: "sk-codex-e2e-test-key",
      },
      // Codex subscription defaults ON (external_agent_provider_contract D11),
      // and an enabled subscription is *preferred over* a configured API key on
      // the direct OpenAI route. These fake-worker positive flows exercise the
      // deterministic OPENAI_API_KEY path, so opt out of the subscription route
      // explicitly — otherwise the task resolves to interactive subscription
      // auth and the fake worker is never assigned.
      settings: {
        codex_subscription_enabled: false,
      },
    },
  );
  expect(result.status, JSON.stringify(result.body).slice(0, 800)).toBe(200);
}

/** Seed a non-live workspace credential for positive-path Claude Code E2E tasks. */
export async function seedWorkspaceClaudeCredential(
  page: Page,
  workspaceId: string,
): Promise<void> {
  const orgId = await readCurrentOrgId(page);
  const result = await authedApi(
    page,
    `/api/v1/workspaces/${workspaceId}/config?orgId=${orgId}`,
    "PUT",
    {
      setSecrets: {
        ANTHROPIC_API_KEY: "sk-ant-e2e-test-key",
      },
    },
  );
  expect(result.status, JSON.stringify(result.body).slice(0, 800)).toBe(200);
}

async function readOrgContext(
  page: Page,
  orgSlug = TEST_ORG,
): Promise<OrgContext> {
  const cached = orgContextCache.get(page)?.get(orgSlug);
  if (cached) {
    return cached;
  }

  const result = await authedApi(page, "/api/v1/users/me");
  expect(result.status).toBe(200);
  const data = result.body?.data ?? result.body ?? {};
  const orgs = Array.isArray(data.orgs) ? data.orgs : [];
  const targetOrg = orgs.find((org: Record<string, unknown>) => {
    const slug = org.orgSlug ?? org.org_slug ?? org.slug;
    return typeof slug === "string" && slug === orgSlug;
  });
  expect(
    targetOrg,
    `Authenticated E2E user is not a member of target org ${orgSlug}`,
  ).toBeTruthy();
  const resolvedOrgId = targetOrg.orgId ?? targetOrg.org_id ?? "";
  const resolvedOrgSlug =
    targetOrg.orgSlug ?? targetOrg.org_slug ?? targetOrg.slug ?? "";
  expect(resolvedOrgId).toBeTruthy();
  expect(resolvedOrgSlug).toBeTruthy();
  const context = {
    orgId: String(resolvedOrgId),
    orgSlug: String(resolvedOrgSlug),
  };

  const pageCache = orgContextCache.get(page) ?? new Map<string, OrgContext>();
  pageCache.set(context.orgSlug, context);
  orgContextCache.set(page, pageCache);

  return context;
}

export async function readCurrentOrgContext(page: Page): Promise<OrgContext> {
  return readOrgContext(page, TEST_ORG);
}

async function ensureAuthOrgForSlug(
  page: Page,
  orgSlug?: string,
): Promise<void> {
  if (!orgSlug || selectedOrgCache.get(page) === orgSlug) {
    return;
  }

  const context = await readOrgContext(page, orgSlug);
  await switchAuthOrg(page, context.orgId);
  selectedOrgCache.set(page, context.orgSlug);
}

function orgSlugFromPath(path: string): string | null {
  try {
    const url = new URL(path, APP_BASE_URL);
    if (url.origin !== new URL(APP_BASE_URL).origin) {
      return null;
    }

    const [orgSlug] = url.pathname.split("/").filter(Boolean);
    if (
      !orgSlug ||
      orgSlug === "projects" ||
      orgSlug === "login" ||
      orgSlug === "landing"
    ) {
      return null;
    }
    return orgSlug;
  } catch {
    return null;
  }
}

/** Navigate to a specific workspace within an org */
export async function navigateToWorkspace(
  page: Page,
  org: string,
  workspace: string,
): Promise<void> {
  await ensureAuthOrgForSlug(page, org);
  await openWorkspacePath(page, `/${org}/${workspace}`);
}

export async function openWorkspacePath(
  page: Page,
  workspacePath: string,
): Promise<void> {
  const orgSlug = orgSlugFromPath(workspacePath);
  if (orgSlug) {
    await ensureAuthOrgForSlug(page, orgSlug);
  }

  let lastError: unknown = null;

  for (let attempt = 0; attempt < 3; attempt += 1) {
    if (authShellUrl(page.url())) {
      await ensureAppOrigin(page);
    }

    try {
      // Use 'domcontentloaded' so goto() returns as soon as the HTML is
      // parsed instead of waiting for every sub-resource (fonts, analytics,
      // slow API prefetches). waitForWorkspaceLoad() below handles waiting
      // for the React shell to render.
      await page.goto(workspacePath, { waitUntil: "domcontentloaded" });
      await waitForWorkspaceLoad(page);
      return;
    } catch (error) {
      lastError = error;
      const message = error instanceof Error ? error.message : String(error);
      if (
        !message.includes("interrupted by another navigation") &&
        !message.includes("/coming-soon.html") &&
        !authShellUrl(page.url())
      ) {
        throw error;
      }
      await ensureAppOrigin(page);
      await page.waitForTimeout(250);
    }
  }

  throw lastError instanceof Error
    ? lastError
    : new Error(`Failed to open workspace path ${workspacePath}`);
}

/** Wait for the workspace page to fully load (sidebar + center panel rendered) */
export async function waitForWorkspaceLoad(page: Page): Promise<void> {
  if (await pageShowsAuthShell(page)) {
    throw new Error(
      `Workspace load aborted on unauthenticated shell: ${page.url()}`,
    );
  }
  // Use 30 s — the Docker CI environment is CPU-constrained so the React
  // shell and initial API fetches can take longer than the default
  // actionTimeout (15 s).  This explicit timeout overrides expect.timeout.
  await expect(page.getByTestId("left-sidebar")).toBeVisible({
    timeout: 30_000,
  });
  await expect(page.getByTestId("center-panel")).toBeVisible({
    timeout: 30_000,
  });
  // The workspace shell can render before slug resolution finishes. Treat the
  // route as loaded only once the top-bar workspace switcher has resolved away
  // from its placeholder state.
  await expect(
    page.getByRole("button", { name: /^Select project$/ }).first(),
  ).not.toBeVisible({ timeout: 30_000 });
}

/** Open the command palette via keyboard shortcut */
/** Close any open overlay (command palette, search, dialogs) */
export async function closeOverlay(page: Page): Promise<void> {
  await page.keyboard.press("Escape");
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function sidebarSectionHeader(section: Locator): Locator {
  return section.locator('[role="button"][aria-expanded]').first();
}

/**
 * Expand a stacked sidebar section before reading its content. No-op when the
 * section is already expanded (`aria-expanded="true"`).
 */
export async function activateSidebarSection(
  page: Page,
  testId:
    | "sidebar-docs-tab"
    | "sidebar-ontology-tab"
    | "sidebar-all-docs-tab"
    | "sidebar-source-control-tab"
    | "sidebar-tasks-tab"
    | "sidebar-files-tab",
): Promise<void> {
  const rail = page.getByTestId(testId);
  await expect(rail).toBeVisible({ timeout: 30_000 });
  const button = sidebarSectionHeader(rail);
  await expect(button).toBeVisible({ timeout: 30_000 });
  if ((await button.getAttribute("aria-expanded")) !== "true") {
    await button.click();
  }
  await expect(button).toHaveAttribute("aria-expanded", "true", {
    timeout: 5_000,
  });
}

/**
 * Expand one repository root inside the Files section. Repository roots are
 * collapsed independently from the section itself, so file-row tests must
 * cross both disclosure boundaries explicitly.
 */
export async function expandRepositoryFileRoot(
  page: Page,
  repoId?: string,
): Promise<Locator> {
  await activateSidebarSection(page, "sidebar-files-tab");
  const filesSection = page.getByTestId("sidebar-files-tab");
  const tree = filesSection.getByRole("tree", {
    name: "Workspace repository files",
  });
  await expect(tree).toBeVisible({ timeout: 30_000 });
  const root = repoId
    ? tree.locator(`[data-g5-node-id="${repoId}"]`).first()
    : tree.getByRole("treeitem").first();
  await expect(root).toBeVisible({ timeout: 30_000 });
  if ((await root.getAttribute("aria-expanded")) !== "true") {
    await root.click();
  }
  await expect(root).toHaveAttribute("aria-expanded", "true", {
    timeout: 5_000,
  });
  return root;
}

export async function clickSidebarSectionAction(
  page: Page,
  testId: string,
  actionLabel: string | RegExp,
): Promise<void> {
  const section = page.getByTestId(testId);
  await expect(section).toBeVisible({ timeout: 30_000 });

  const actionName =
    typeof actionLabel === "string"
      ? new RegExp(`^${escapeRegExp(actionLabel)}$`, "i")
      : actionLabel;
  const isDirectCreateAction =
    typeof actionLabel === "string" && actionLabel === "Create project doc";

  if (isDirectCreateAction) {
    const actionButton = section
      .getByRole("button", { name: actionName })
      .first();
    await expect(actionButton).toBeVisible({ timeout: 5_000 });
    await actionButton.click();
    return;
  }

  const menuItem = page.getByRole("menuitem", { name: actionName }).first();
  const sectionHeader = sidebarSectionHeader(section);

  await sectionHeader.click({ button: "right" });
  if (!(await softVisible(menuItem, 1_000))) {
    const actionButton = section
      .getByRole("button", { name: /actions$/i })
      .first();
    if (await softVisible(actionButton, 500)) {
      await actionButton.click();
    }
  }
  if (!(await softVisible(menuItem, 1_000))) {
    await sectionHeader.focus();
    await page.keyboard.press("Shift+F10");
  }

  await expect(menuItem).toBeVisible({ timeout: 5_000 });
  await menuItem.click();
}

/** Wait for a toast notification with specific text */
export async function waitForToast(
  page: Page,
  text: string | RegExp,
  options?: { timeout?: number },
): Promise<Locator> {
  const timeout = options?.timeout ?? 10_000;
  const toast = page.getByTestId("toast").filter({ hasText: text });
  await expect(toast).toBeVisible({ timeout });
  return toast;
}

/** Get the current theme name from the HTML attribute */
export async function getCurrentTheme(page: Page): Promise<string> {
  const theme = await page.locator("html").getAttribute("data-theme");
  return theme ?? "";
}

/** Get the current mode (light/dark) from the HTML attribute */
export async function getCurrentMode(page: Page): Promise<string> {
  const mode = await page.locator("html").getAttribute("data-mode");
  return mode ?? "";
}

/**
 * Records whether the server has acknowledged the `workspace:<id>`
 * subscription on the event socket.
 *
 * DOM readiness is not subscription readiness: `waitForWorkspaceLoad` returns
 * once React has rendered, but the socket may still be connecting or the
 * `subscribe` frame still unacknowledged. A publish that lands in that window
 * is delivered to nobody, so a test that skips this wait is racy — and so is
 * any real client that mutates immediately after load. Attach BEFORE
 * navigating: the socket is opened during page load, so a listener registered
 * afterwards misses the `websocket` event entirely.
 */
export function watchWorkspaceSubscription(
  page: Page,
  workspaceId: string,
): { acknowledged: () => boolean } {
  const channel = `workspace:${workspaceId}`;
  let acknowledged = false;

  page.on("websocket", (ws) => {
    ws.on("framereceived", (frame) => {
      const payload =
        typeof frame.payload === "string"
          ? frame.payload
          : frame.payload.toString("utf8");
      if (!payload.includes("subscribed")) {
        return;
      }
      try {
        const parsed = JSON.parse(payload) as {
          type?: string;
          channel?: string;
        };
        if (parsed.type === "subscribed" && parsed.channel === channel) {
          acknowledged = true;
        }
      } catch {
        // Domain event frames are not control messages; ignore non-JSON noise.
      }
    });
  });

  return { acknowledged: () => acknowledged };
}

/** Wait for a WebSocket connection to be established */
export async function waitForWebSocket(
  page: Page,
  urlPattern?: string | RegExp,
): Promise<void> {
  await page.waitForEvent("websocket", {
    predicate: (ws) => {
      if (!urlPattern) return true;
      if (typeof urlPattern === "string") return ws.url().includes(urlPattern);
      return urlPattern.test(ws.url());
    },
    timeout: 15_000,
  });
}

/** Send a chat message and wait for the response to start streaming */
export async function sendChatMessage(
  page: Page,
  message: string,
): Promise<void> {
  await openChatComposer(page);
  const chatInput = page
    .getByTestId("chat-input")
    .or(page.getByPlaceholder(/message|ask|type/i))
    .first();
  await expect(chatInput).toBeVisible({ timeout: 10_000 });
  await chatInput.fill(message);
  await chatInput.press("Enter");
}

// ---------------------------------------------------------------------------
// CRUD / Dialog helpers (used by workspace-create, org-create, invite, clone tests)
// ---------------------------------------------------------------------------

/** Navigate to the Projects page with auth-shell-aware retries */
export async function navigateToProjects(page: Page): Promise<void> {
  let lastError: unknown = null;

  for (let attempt = 0; attempt < 3; attempt += 1) {
    if (authShellUrl(page.url())) {
      await ensureAppOrigin(page);
    }

    try {
      await page.goto("/projects", { waitUntil: "domcontentloaded" });
      await expect(
        page
          .getByRole("heading", { name: /projects/i })
          .or(page.getByRole("button", { name: /new project/i }))
          .first(),
      ).toBeVisible({ timeout: 30_000 });
      return;
    } catch (error) {
      lastError = error;
      const message = error instanceof Error ? error.message : String(error);
      if (
        !message.includes("interrupted by another navigation") &&
        !message.includes("/coming-soon.html") &&
        !authShellUrl(page.url())
      ) {
        throw error;
      }
      await ensureAppOrigin(page);
      await page.waitForTimeout(250);
    }
  }

  throw lastError instanceof Error
    ? lastError
    : new Error("Failed to open /projects");
}

/** Navigate to the Org Members settings page */
export async function navigateToOrgMembers(page: Page): Promise<void> {
  await ensureAuthOrgForSlug(page, TEST_ORG);
  await page.goto(`/${TEST_ORG}/settings/members`, {
    waitUntil: "domcontentloaded",
  });
  await expect(
    page
      .getByRole("button", { name: /invite/i })
      .or(page.getByText(/members/i))
      .first(),
  ).toBeVisible({ timeout: 30_000 });
}

/** Open a dialog and wait for it to be visible. Returns the dialog locator. */
export async function openDialog(
  page: Page,
  triggerLocator: Locator,
): Promise<Locator> {
  await triggerLocator.click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toBeVisible({ timeout: 5_000 });
  return dialog;
}

/** Close any open dialog via Escape key and verify it's closed */
export async function closeDialogViaEscape(page: Page): Promise<void> {
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).not.toBeVisible({ timeout: 5_000 });
}

/**
 * Reveal the Project Docs sidebar section and return its locator.
 *
 * Expands the Project Docs stacked section if it is collapsed. Shared across
 * the Project Doc specs so each spec does not re-declare its own copy.
 */
export async function expandProjectDocs(page: Page): Promise<Locator> {
  await activateSidebarSection(page, "sidebar-docs-tab");
  return page.getByTestId("sidebar-docs-tab");
}

/** Count non-overlapping occurrences of `needle` in `haystack`. */
export function countOccurrences(haystack: string, needle: string): number {
  if (!needle) return 0;
  return haystack.split(needle).length - 1;
}

// ---------------------------------------------------------------------------
// Agent selection via the settings gear (#1370, right-panel D25/D27): the
// composer "AGENT:" cards row is removed — the settings popover's compact
// Agent dropdown is the sole selector, and its shared open state survives an
// agent
// switch (only outside click / Escape / re-clicking the gear dismisses it).
// ---------------------------------------------------------------------------

/** The visible provider-settings gear (exactly one provider block renders). */
export function providerSettingsGear(page: Page): Locator {
  return page
    .getByTestId("claude-settings-control")
    .or(page.getByTestId("codex-settings-control"));
}

/** Wait until the chat's direct launch choices have resolved and are editable. */
export async function waitForDirectAgentLaunchChoices(
  page: Page,
): Promise<void> {
  await expect(page.getByTestId("chat-agent-select")).toBeEnabled({
    timeout: 15_000,
  });
}

/** Select a harness on an empty new chat through the settings popover. */
export async function selectAgentProviderViaSettings(
  page: Page,
  provider: "claude_code" | "codex",
): Promise<void> {
  await openChatComposer(page);
  await providerSettingsGear(page).click();
  await waitForDirectAgentLaunchChoices(page);
  await page.getByTestId("chat-agent-select").click();
  await page.getByTestId(`chat-agent-select-option-${provider}`).click();
  await applyChatAgentLaunchSettings(page);
}

/** Wait for Chat's automatic confirmation, then close the settings popup. */
export async function applyChatAgentLaunchSettings(page: Page): Promise<void> {
  await expect(page.getByTestId("chat-resolution-loading")).toHaveCount(0, {
    timeout: 15_000,
  });
  await expect(page.getByTestId("chat-resolution-error")).toHaveCount(0);
  await expect(page.getByTestId("chat-provider-settings-apply")).toHaveCount(0);
  await expect(page.getByTestId("chat-provider-settings-cancel")).toHaveCount(
    0,
  );
  await page.mouse.click(8, 8);
  await expect(page.getByTestId("claude-settings-modal")).toHaveCount(0);
  await expect(page.getByTestId("codex-settings-modal")).toHaveCount(0);
}

/** Assert the composer's displayed provider (data-selected-provider marker). */
export async function expectSelectedAgentProvider(
  page: Page,
  provider: string,
): Promise<void> {
  await expect(page.getByTestId("chat-agent-options-row")).toHaveAttribute(
    "data-selected-provider",
    provider,
  );
}

/** Open the settings popover, run assertions on the direct fields, close it. */
export async function withHarnessGroupOpen(
  page: Page,
  assertions: () => Promise<void>,
): Promise<void> {
  await providerSettingsGear(page).click();
  await assertions();
  await page.keyboard.press("Escape");
}

const USER_SETTINGS_KEY = "g5-user-settings";

/**
 * Turn the center tab bar on before every document load in this page. The
 * strip is hidden by default (workspaces g5.workspace.ui.center-tabs), so a
 * flow that clicks the Chat or content tabs opts in. The script merges into
 * the stored settings, so values a flow changes later (e.g. the workspace
 * layout) survive a reload.
 */
export async function showWorkspaceTabs(page: Page): Promise<void> {
  await patchStoredUserSettings(page, { showTabBar: true });
}

/**
 * Merge a patch into the browser-stored user settings before every
 * navigation (the same store `hooks/useUserSettings.ts` reads), e.g. the
 * sidebar display mode (left-sidebar D38).
 */
export async function patchStoredUserSettings(
  page: Page,
  patch: Record<string, unknown>,
): Promise<void> {
  await page.addInitScript(
    ({ key, patch }: { key: string; patch: Record<string, unknown> }) => {
      let settings: Record<string, unknown> = {};
      try {
        settings = JSON.parse(localStorage.getItem(key) ?? "{}") ?? {};
      } catch {
        settings = {};
      }
      const merged = { ...settings, ...patch };
      if (JSON.stringify(merged) !== JSON.stringify(settings)) {
        localStorage.setItem(key, JSON.stringify(merged));
      }
    },
    { key: USER_SETTINGS_KEY, patch },
  );
}

/**
 * Open the ribbon's Preview control (ui g5.ui.workbench-shell): Preview is a
 * System Engineering action, so the owner's workspace is moved there first.
 */
export async function openRibbonPreview(page: Page): Promise<void> {
  const chevrons = page.getByTestId("phase-chevrons");
  await expect(chevrons).toBeVisible({ timeout: 30_000 });
  if ((await chevrons.getAttribute("data-phase")) !== "se") {
    await page.getByTestId("phase-chevron-se").click();
    await expect(chevrons).toHaveAttribute("data-phase", "se", {
      timeout: 15_000,
    });
  }
  const trigger = page.getByTestId("ribbon-preview-preview");
  await expect(trigger).toBeVisible({ timeout: 30_000 });
  await expect(trigger).toBeEnabled({ timeout: 30_000 });
  await trigger.click();
}

/** The owner's Terminal action on the rail's Files header. */
/**
 * The rail's Terminal action: the Files section header's button, shown to
 * the owner of a non-Base workspace, disabled with the VM state's reason
 * while the VM is not running (`g5.workspace.ui.unavailable`). Activates the
 * section so the header renders; returns the locator (count 0 for others).
 */
export async function railTerminalButton(page: Page): Promise<Locator> {
  await activateSidebarSection(page, "sidebar-files-tab");
  return page
    .getByTestId("sidebar-files-tab")
    .getByRole("button", { name: "Terminal" });
}

export async function openRailTerminal(page: Page): Promise<void> {
  await (await railTerminalButton(page)).click();
}

/**
 * Make the Chat composer visible. A workspace route lands on the timeline
 * (g5.workbench.landing); the ribbon's chat toggle then puts Chat in the
 * center. No-op when a composer is already showing.
 */
export async function openChatComposer(page: Page): Promise<void> {
  const input = page.getByTestId("chat-input").first();
  if (await input.isVisible().catch(() => false)) return;
  // With the tab bar shown the Chat tab stands in for the ribbon's toggle.
  const chatTab = page.getByTestId("center-chat-tab");
  if (await chatTab.isVisible().catch(() => false)) {
    await chatTab.click();
  } else {
    const toggle = page.getByTestId("toggle-right-panel");
    if (
      (await toggle.getAttribute("title").catch(() => null)) === "Open chat"
    ) {
      await toggle.click();
    }
  }
  await expect(input).toBeVisible({ timeout: 30_000 });
}

/** Open the workspace Settings page from the strip and return it. */
export async function openWorkspaceSettingsPage(page: Page) {
  await page.getByTestId("settings-button").click();
  const settingsPage = page.getByTestId("workspace-settings-page");
  await expect(settingsPage).toBeVisible({ timeout: 10_000 });
  return settingsPage;
}

/** The workspace docs list entry for `filePath`, when listed. */
export async function listedDoc(
  page: Page,
  workspaceId: string,
  filePath: string,
): Promise<{ id: string; source: string } | undefined> {
  const list = await authedApi(
    page,
    `/api/v1/workspaces/${workspaceId}/docs?surface=workspace`,
  );
  type Entry = { id: string; filePath?: string; source?: string };
  const body = list.body as { data?: Entry[] | { docs: Entry[] } };
  const data = body?.data ?? [];
  const docs = Array.isArray(data) ? data : (data?.docs ?? []);
  const doc = docs.find((entry) => entry.filePath === filePath);
  return doc
    ? { id: String(doc.id), source: String(doc.source ?? "") }
    : undefined;
}

/** The document's id once it is listed (live or committed). */
export async function docIdForPath(
  page: Page,
  workspaceId: string,
  filePath: string,
  timeout = 30_000,
): Promise<string> {
  let docId = "";
  await expect
    .poll(
      async () => {
        docId = (await listedDoc(page, workspaceId, filePath))?.id ?? "";
        return docId;
      },
      { timeout },
    )
    .not.toBe("");
  return docId;
}

// ---------------------------------------------------------------------------
// Instance feature gates, mirrored from the Node-side env the stack was
// started with (the API reads the same variables).
// ---------------------------------------------------------------------------

/** `guided_journey` is a default-on kill-switch: only an explicit "false" turns it off. */
export const GUIDED_JOURNEY_ENABLED =
  process.env.G5_GUIDED_JOURNEY_ENABLED !== "false";

/** Expand the file-backed Core Ontology section. */
export async function expandOntologyTree(page: Page): Promise<Locator> {
  await activateSidebarSection(page, "sidebar-ontology-tab");
  const tree = page.getByTestId("ontology-tree");
  await expect(tree).toBeVisible({ timeout: 30_000 });
  await expect(tree.getByRole("treeitem").first()).toBeVisible({
    timeout: 30_000,
  });
  const collapsed = tree.locator('[role="treeitem"][aria-expanded="false"]');
  while (await collapsed.count()) await collapsed.first().click();
  return tree;
}

/**
 * Open the Overview's Primary Issue section (g5.workbench.landing.task). It
 * starts collapsed once Work Description presents a PR, so wait for that
 * section to settle before reading the default.
 */
export async function expandPrimaryIssue(page: Page) {
  const work = page.getByTestId("landing-implementation");
  await expect(
    work
      .getByTestId("landing-implementation-title")
      .or(work.getByTestId("landing-implementation-empty")),
  ).toBeVisible();
  const toggle = page.getByTestId("landing-task-toggle");
  if ((await toggle.getAttribute("aria-expanded")) === "false")
    await toggle.click();
  await expect(toggle).toHaveAttribute("aria-expanded", "true");
}
