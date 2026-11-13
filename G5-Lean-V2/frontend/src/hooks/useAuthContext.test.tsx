// =============================================================================
// useAuthContext.test.tsx — AuthProvider Context Tests with MSW
// =============================================================================

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import React from "react";
import {
  act,
  fireEvent,
  render,
  renderHook,
  screen,
  waitFor,
} from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { delay, http, HttpResponse } from "msw";

import type { CurrentUserDTO } from "../lib/types";
import { server } from "../test/msw/server";
import { AuthProvider, RequireAuth, useAuth } from "./useAuth";
import { useComposerField } from "./useComposerStore";
import {
  ComposerOwner,
  ComposerOwnerIfNone,
} from "../components/comments/ComposerOwner";
import { getApiToken, setApiToken, tokenPrincipal } from "../lib/api";
import {
  fileDraftKey,
  getFileDraft,
  newFileDraftId,
  setFileDraft,
  type FileDraft,
} from "../components/files/fileDrafts";
import { scopedConfigSignal } from "../lib/scopedConfigAbort";
import { useUserScopedConfig } from "./useSettings";

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false, staleTime: Infinity, gcTime: Infinity },
    },
  });
  const wrapper = ({ children }: { children: React.ReactNode }) =>
    React.createElement(
      QueryClientProvider,
      { client: queryClient },
      React.createElement(
        MemoryRouter,
        null,
        React.createElement(AuthProvider, null, children),
      ),
    );

  return { wrapper, queryClient };
}

// Production-shaped JWT payload: claim names match the server's JwtClaims
// exactly (`sub`, `org`, `scopes`, `iat`, `exp`) — a test token with
// invented claims (orgId/role) would mask claims-comparison bugs.
function makeJwt(org: string, scopes: string[], sub = "user-alex"): string {
  const now = Math.floor(Date.now() / 1000);
  const header = btoa(JSON.stringify({ alg: "HS256", typ: "JWT" }));
  const payload = btoa(
    JSON.stringify({
      sub,
      org,
      scopes,
      iat: now,
      exp: now + 3600,
    }),
  );

  return `${header}.${payload}.signature`;
}

const initialUser: CurrentUserDTO = {
  id: "user-alex",
  email: "alex@g5labs.dev",
  displayName: "Alex Chen",
  avatarUrl: null,
  oauthProvider: "github",
  activeOrgId: "org-g5labs",
  orgs: [
    {
      orgId: "org-g5labs",
      orgName: "G5 Labs",
      orgSlug: "g5labs",
      role: "admin",
      orgKind: "tenant",
    },
  ],
  scmConnections: [
    {
      provider: "github",
      providerUsername: "alexchen",
      scopes: ["repo", "read:org"],
      hasInstallation: true,
      connectedAt: "2024-01-01T00:00:00Z",
    },
  ],
  createdAt: "2024-01-01T00:00:00Z",
};

const switchedUser: CurrentUserDTO = {
  ...initialUser,
  orgs: [
    ...initialUser.orgs,
    {
      orgId: "org-new",
      orgName: "New Org",
      orgSlug: "new-org",
      role: "admin",
      orgKind: "tenant",
    },
  ],
};

describe("AuthProvider switchOrg", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("preserves unrelated public cache after logout even when the request fails", async () => {
    const jwt = makeJwt("org-g5labs", ["workspaces:read"]);
    let publicCachePreservedBeforeLogoutRequest = false;
    const { wrapper, queryClient } = createWrapper();
    server.use(
      http.post("/api/v1/auth/refresh", () =>
        HttpResponse.json({ data: { jwt } }),
      ),
      http.get("/api/v1/users/me", () =>
        HttpResponse.json({ data: initialUser }),
      ),
      http.post("/api/v1/auth/logout", () => {
        publicCachePreservedBeforeLogoutRequest =
          queryClient.getQueryData(["unrelated", "public"]) === "preserve-me";
        return HttpResponse.json({ error: "unavailable" }, { status: 503 });
      }),
    );

    const { result } = renderHook(() => useAuth(), { wrapper });
    await waitFor(() => expect(result.current.user).not.toBeNull());

    queryClient.setQueryData(["unrelated", "public"], "preserve-me");

    await act(async () => {
      await result.current.logout();
    });

    expect(publicCachePreservedBeforeLogoutRequest).toBe(true);
    expect(queryClient.getQueryData(["unrelated", "public"])).toBe(
      "preserve-me",
    );
  });

  it("clears issue comment drafts on logout and keeps other local state", async () => {
    const jwt = makeJwt("org-g5labs", ["workspaces:read"]);
    const { wrapper } = createWrapper();
    server.use(
      http.post("/api/v1/auth/refresh", () =>
        HttpResponse.json({ data: { jwt } }),
      ),
      http.get("/api/v1/users/me", () =>
        HttpResponse.json({ data: initialUser }),
      ),
      http.post("/api/v1/auth/logout", () => HttpResponse.json({ data: {} })),
    );
    const { result } = renderHook(() => useAuth(), { wrapper });
    await waitFor(() => expect(result.current.user).not.toBeNull());
    localStorage.setItem(
      "g5:issue-comment-draft:user-1:ws-1:tracker-1:9001",
      JSON.stringify({ body: "unsent", key: "k", savedAt: Date.now() }),
    );
    localStorage.setItem("g5:unrelated-preference", "keep");

    await act(async () => {
      await result.current.logout();
    });

    expect(
      localStorage.getItem("g5:issue-comment-draft:user-1:ws-1:tracker-1:9001"),
    ).toBeNull();
    expect(localStorage.getItem("g5:unrelated-preference")).toBe("keep");
  });

  it("refetches current-user on org switch and keeps SCM connections user-scoped", async () => {
    const initialJwt = makeJwt("org-g5labs", ["workspaces:read"]);
    const switchedJwt = makeJwt("org-new", [
      "workspaces:read",
      "workspaces:write",
    ]);

    let currentUserRequests = 0;
    let publicCachePreservedBeforeNewIdentity = false;
    const { wrapper, queryClient } = createWrapper();

    server.use(
      http.post("/api/v1/auth/refresh", async ({ request }) => {
        const text = await request.text();
        if (text) {
          const body = JSON.parse(text) as { orgId?: string };
          if (body.orgId === "org-new") {
            return HttpResponse.json({ data: { jwt: switchedJwt } });
          }
        }

        return HttpResponse.json({ data: { jwt: initialJwt } });
      }),
      http.get("/api/v1/users/me", ({ request }) => {
        currentUserRequests += 1;
        const authHeader = request.headers.get("authorization");

        if (authHeader === `Bearer ${switchedJwt}`) {
          publicCachePreservedBeforeNewIdentity =
            queryClient.getQueryData(["unrelated", "public"]) === "preserve-me";
          return HttpResponse.json({ data: switchedUser });
        }

        return HttpResponse.json({ data: initialUser });
      }),
    );

    const { result } = renderHook(() => useAuth(), { wrapper });

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
      expect(result.current.user?.orgs).toHaveLength(1);
    });

    queryClient.setQueryData(["unrelated", "public"], "preserve-me");

    await act(async () => {
      await result.current.switchOrg("org-new");
    });

    await waitFor(() => {
      expect(result.current.user?.orgs).toHaveLength(2);
      expect(
        result.current.user?.orgs.some((org) => org.orgId === "org-new"),
      ).toBe(true);
      expect(result.current.user?.scmConnections).toEqual(
        switchedUser.scmConnections,
      );
      expect(result.current.scopes).toEqual([
        "workspaces:read",
        "workspaces:write",
      ]);
    });

    expect(currentUserRequests).toBe(2);
    expect(publicCachePreservedBeforeNewIdentity).toBe(true);
    expect(queryClient.getQueryData(["unrelated", "public"])).toBe(
      "preserve-me",
    );
  });

  it("evicts scoped-config queries and the mutation cache on logout (settings-page D9)", async () => {
    const jwt = makeJwt("org-g5labs", ["workspaces:read"]);
    const { wrapper, queryClient } = createWrapper();
    server.use(
      http.post("/api/v1/auth/refresh", () =>
        HttpResponse.json({ data: { jwt } }),
      ),
      http.get("/api/v1/users/me", () =>
        HttpResponse.json({ data: initialUser }),
      ),
      http.post("/api/v1/auth/logout", () => HttpResponse.json({ data: {} })),
    );

    const { result } = renderHook(() => useAuth(), { wrapper });
    await waitFor(() => expect(result.current.user).not.toBeNull());

    // A cached scoped-config read carrying a secret display hint, plus a
    // settled scoped-config PUT in the mutation cache (its data also holds
    // hint maps).
    queryClient.setQueryData(["scoped-config", "user", "org-g5labs"], {
      scope: "user",
      secretKeyHints: { ANTHROPIC_AUTH_TOKEN: "sk-ant-oat01" },
    });
    queryClient.setQueryData(["unrelated", "public"], "preserve-me");
    const mutation = queryClient.getMutationCache().build(queryClient, {
      mutationFn: async () => ({
        secretKeyHints: { ANTHROPIC_AUTH_TOKEN: "sk-ant-oat01" },
      }),
    });
    await mutation.execute(undefined);
    expect(queryClient.getMutationCache().getAll().length).toBeGreaterThan(0);

    // An IN-FLIGHT scoped-config write attached to the abort seam must be
    // aborted by the transition, never allowed to settle its hint-bearing
    // response afterwards (settings-page D9).
    const inFlight = queryClient.getMutationCache().build(queryClient, {
      mutationFn: () =>
        new Promise((_resolve, reject) => {
          scopedConfigSignal().addEventListener("abort", () =>
            reject(new DOMException("Aborted", "AbortError")),
          );
        }),
    });
    const inFlightResult = inFlight.execute(undefined).then(
      () => "settled",
      () => "aborted",
    );
    // execute() invokes mutationFn on a microtask; yield once so the request
    // is genuinely in flight (signal attached) before the transition fires.
    await Promise.resolve();

    await act(async () => {
      await result.current.logout();
    });

    expect(await inFlightResult).toBe("aborted");
    expect(
      queryClient.getQueryData(["scoped-config", "user", "org-g5labs"]),
    ).toBeUndefined();
    expect(queryClient.getMutationCache().getAll()).toEqual([]);
    expect(queryClient.getQueryData(["unrelated", "public"])).toBe(
      "preserve-me",
    );
  });

  it("evicts principal-scoped caches when a periodic refresh returns a different principal", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const alexJwt = makeJwt("org-g5labs", ["workspaces:read"], "user-alex");
    const bobJwt = makeJwt("org-g5labs", ["workspaces:read"], "user-bob");
    let refreshRequests = 0;
    server.use(
      http.post("/api/v1/auth/refresh", () => {
        refreshRequests += 1;
        // Another tab re-authenticated as a different user: the shared
        // cookie now mints Bob's JWT.
        return HttpResponse.json({
          data: { jwt: refreshRequests === 1 ? alexJwt : bobJwt },
        });
      }),
      http.get("/api/v1/users/me", ({ request }) => {
        const authHeader = request.headers.get("authorization");
        if (authHeader === `Bearer ${bobJwt}`) {
          return HttpResponse.json({
            data: { ...initialUser, id: "user-bob" },
          });
        }
        return HttpResponse.json({ data: initialUser });
      }),
    );

    // Serve scoped-config with a hint marker that identifies WHICH bearer
    // fetched it: after the transition the mounted observer must hold data
    // fetched under the NEW token, never a stale old-token response.
    const scopedConfigFor = (hint: string) => ({
      scope: "user",
      settings: {},
      secretKeys: ["ANTHROPIC_AUTH_TOKEN"],
      secretKeyHints: { ANTHROPIC_AUTH_TOKEN: hint },
      effectiveSettings: {},
      effectiveSecretKeys: ["ANTHROPIC_AUTH_TOKEN"],
      effectiveSecretKeyHints: { ANTHROPIC_AUTH_TOKEN: hint },
    });
    server.use(
      http.get("/api/v1/users/me/config", ({ request }) => {
        const bearer = request.headers.get("authorization");
        return HttpResponse.json({
          data: scopedConfigFor(
            bearer === `Bearer ${bobJwt}` ? "bob-token-hin" : "alex-token-hi",
          ),
        });
      }),
    );

    const { wrapper, queryClient } = createWrapper();
    const { result } = renderHook(
      () => ({
        auth: useAuth(),
        config: useUserScopedConfig("org-g5labs"),
      }),
      { wrapper },
    );
    const configHint = () => {
      const c = result.current.config;
      return c.status === "ok"
        ? c.data.secretKeyHints?.ANTHROPIC_AUTH_TOKEN
        : undefined;
    };
    await waitFor(() => expect(result.current.auth.user).not.toBeNull());
    await waitFor(() => expect(configHint()).toBe("alex-token-hi"));

    queryClient.setQueryData(["unrelated", "public"], "preserve-me");

    await act(async () => {
      await vi.advanceTimersByTimeAsync(50 * 60 * 1000);
    });

    // The mounted observer must end up with data fetched under Bob's
    // token — the old principal's response can neither survive nor be
    // refetched under the outgoing credential.
    await waitFor(() => {
      expect(result.current.auth.user?.id).toBe("user-bob");
      expect(configHint()).toBe("bob-token-hin");
    });
    expect(queryClient.getQueryData(["unrelated", "public"])).toBe(
      "preserve-me",
    );
  });

  it("gives another principal a fresh page, without the previous one's unsent words, but keeps them through a rotation", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const alexJwt = makeJwt("org-g5labs", ["workspaces:read"], "user-alex");
    // Same claims, newly signed: a rotation, not another principal.
    const rotatedJwt = alexJwt.replace(/signature$/, "rotated-signature");
    const bobJwt = makeJwt("org-g5labs", ["workspaces:read"], "user-bob");
    const minted = [alexJwt, rotatedJwt, bobJwt];
    let refreshRequests = 0;
    server.use(
      // Another tab: first the same person again, then Bob signs in there.
      http.post("/api/v1/auth/refresh", () => {
        const jwt = minted[Math.min(refreshRequests, minted.length - 1)];
        refreshRequests += 1;
        return HttpResponse.json({ data: { jwt } });
      }),
      http.get("/api/v1/users/me", ({ request }) =>
        HttpResponse.json({
          data:
            request.headers.get("authorization") === `Bearer ${bobJwt}`
              ? { ...initialUser, id: "user-bob", displayName: "Bob" }
              : initialUser,
        }),
      ),
    );
    const { wrapper: Wrapper } = createWrapper();
    render(
      <Wrapper>
        <RequireAuth>
          <textarea aria-label="Reply" />
        </RequireAuth>
      </Wrapper>,
    );
    const reply = (await screen.findByLabelText(
      "Reply",
    )) as HTMLTextAreaElement;
    fireEvent.change(reply, { target: { value: "Alex's unsent words" } });

    // The same principal, a new token: the page and the words stay.
    await act(async () => {
      await vi.advanceTimersByTimeAsync(50 * 60 * 1000);
    });
    await waitFor(() => expect(refreshRequests).toBe(2));
    expect(screen.getByLabelText("Reply")).toBe(reply);
    expect(reply).toHaveValue("Alex's unsent words");

    // Bob's token is adopted by the next periodic refresh: what Bob sees is
    // new, and holds nothing Alex typed.
    await act(async () => {
      await vi.advanceTimersByTimeAsync(50 * 60 * 1000);
    });
    await waitFor(() => expect(screen.getByLabelText("Reply")).not.toBe(reply));
    expect(screen.getByLabelText("Reply")).toHaveValue("");
  });

  it("retires the previous principal's page the moment another's token is adopted, before its profile loads", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const alexJwt = makeJwt("org-g5labs", ["workspaces:read"], "user-alex");
    const bobJwt = makeJwt("org-g5labs", ["workspaces:read"], "user-bob");
    let refreshRequests = 0;
    server.use(
      http.post("/api/v1/auth/refresh", () => {
        refreshRequests += 1;
        return HttpResponse.json({
          data: { jwt: refreshRequests === 1 ? alexJwt : bobJwt },
        });
      }),
      // Bob's profile never arrives.
      http.get("/api/v1/users/me", async ({ request }) => {
        if (request.headers.get("authorization") === `Bearer ${bobJwt}`) {
          await delay("infinite");
        }
        return HttpResponse.json({ data: initialUser });
      }),
    );
    const { wrapper: Wrapper } = createWrapper();
    render(
      <Wrapper>
        <Routes>
          <Route
            path="/"
            element={
              <RequireAuth>
                <textarea aria-label="Reply" />
              </RequireAuth>
            }
          />
          <Route path="/login" element={<p>Sign in</p>} />
        </Routes>
      </Wrapper>,
    );
    const reply = (await screen.findByLabelText(
      "Reply",
    )) as HTMLTextAreaElement;
    fireEvent.change(reply, { target: { value: "Alex's unsent words" } });

    await act(async () => {
      await vi.advanceTimersByTimeAsync(50 * 60 * 1000);
    });
    await waitFor(() => expect(refreshRequests).toBeGreaterThanOrEqual(2));
    // Nothing of Alex's stays usable under Bob's token while Bob's profile
    // is pending.
    expect(screen.getByRole("status", { name: "Loading" })).toBeInTheDocument();
    expect(screen.queryByText("Sign in")).toBeNull();
    expect(screen.queryByLabelText("Reply")).toBeNull();
    expect(reply).not.toBeInTheDocument();
  });

  it("keeps the page through an org switch, so the flow that switched goes on, but not the comment words typed for the previous org", async () => {
    const initialJwt = makeJwt("org-g5labs", ["workspaces:read"]);
    const switchedJwt = makeJwt("org-new", ["workspaces:read"]);
    server.use(
      http.post("/api/v1/auth/refresh", async ({ request }) => {
        const text = await request.text();
        const body = text ? (JSON.parse(text) as { orgId?: string }) : {};
        return HttpResponse.json({
          data: { jwt: body.orgId === "org-new" ? switchedJwt : initialJwt },
        });
      }),
      // The profile still names the first org: the composers are keyed by
      // the token in use, not by the profile.
      http.get("/api/v1/users/me", () =>
        HttpResponse.json({ data: switchedUser }),
      ),
    );
    // Like the wizard that creates an org: it switches to it, then goes on.
    const CreateOrg = () => {
      const { switchOrg } = useAuth();
      const [step, setStep] = React.useState(1);
      return (
        <>
          <p>Step {step}</p>
          <button
            type="button"
            onClick={async () => {
              await switchOrg("org-new");
              setStep(2);
            }}
          >
            Create org
          </button>
        </>
      );
    };
    const Composer = ({ label }: { label: string }) => {
      const [words, setWords] = useComposerField(
        { kind: "thread", id: "thread-1" },
        "words",
        "",
      );
      return (
        <textarea
          aria-label={label}
          value={words}
          onChange={(event) => setWords(event.target.value)}
        />
      );
    };
    const { wrapper: Wrapper } = createWrapper();
    render(
      <Wrapper>
        <RequireAuth>
          <CreateOrg />
          <ComposerOwner scope="workspace-1">
            <Composer label="Reply" />
          </ComposerOwner>
          <ComposerOwnerIfNone>
            <Composer label="Review reply" />
          </ComposerOwnerIfNone>
        </RequireAuth>
      </Wrapper>,
    );
    const reply = await screen.findByLabelText("Reply");
    fireEvent.change(reply, { target: { value: "Words for the first org" } });
    fireEvent.change(screen.getByLabelText("Review reply"), {
      target: { value: "More words for the first org" },
    });

    fireEvent.click(screen.getByRole("button", { name: "Create org" }));
    expect(await screen.findByText("Step 2")).toBeInTheDocument();
    expect(screen.getByLabelText("Reply")).toBe(reply);
    expect(reply).toHaveValue("");
    expect(screen.getByLabelText("Review reply")).toHaveValue("");
    expect(getApiToken()).toBe(switchedJwt);
  });

  it("keeps a person's unsaved FILES drafts through an org switch, and drops another person's", async () => {
    const initialJwt = makeJwt("org-g5labs", ["workspaces:read"]);
    const switchedJwt = makeJwt("org-new", ["workspaces:read"]);
    server.use(
      http.post("/api/v1/auth/refresh", async ({ request }) => {
        const text = await request.text();
        const body = text ? (JSON.parse(text) as { orgId?: string }) : {};
        return HttpResponse.json({
          data: { jwt: body.orgId === "org-new" ? switchedJwt : initialJwt },
        });
      }),
      http.get("/api/v1/users/me", () =>
        HttpResponse.json({ data: switchedUser }),
      ),
    );
    const draft = (content: string): FileDraft => ({
      id: newFileDraftId(),
      role: "owner",
      content,
      baseline: { content: "base\n", hash: "h1", vmId: "vm-1" },
    });
    const address = { workspaceId: "ws-1", repoId: "repo-1", path: "a.md" };
    const own = fileDraftKey({ person: "user-alex", ...address });
    const theirs = fileDraftKey({ person: "user-bob", ...address });
    setFileDraft(own, draft("Alex's unsaved text\n"));
    setFileDraft(theirs, draft("Bob's unsaved text\n"));
    const SwitchOrg = () => {
      const { switchOrg } = useAuth();
      const [step, setStep] = React.useState(1);
      return (
        <>
          <p>Step {step}</p>
          <button
            type="button"
            onClick={async () => {
              await switchOrg("org-new");
              setStep(2);
            }}
          >
            Switch org
          </button>
        </>
      );
    };
    const { wrapper: Wrapper } = createWrapper();
    render(
      <Wrapper>
        <RequireAuth>
          <SwitchOrg />
        </RequireAuth>
      </Wrapper>,
    );
    expect(await screen.findByText("Step 1")).toBeInTheDocument();
    await waitFor(() => expect(getFileDraft(theirs)).toBeNull());
    expect(getFileDraft(own)?.content).toBe("Alex's unsaved text\n");

    fireEvent.click(screen.getByRole("button", { name: "Switch org" }));
    expect(await screen.findByText("Step 2")).toBeInTheDocument();
    expect(getApiToken()).toBe(switchedJwt);
    expect(getFileDraft(own)?.content).toBe("Alex's unsaved text\n");
  });

  it("keeps the session and the page when the periodic refresh gets no answer", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const alexJwt = makeJwt("org-g5labs", ["workspaces:read"], "user-alex");
    let refreshRequests = 0;
    server.use(
      http.post("/api/v1/auth/refresh", () => {
        refreshRequests += 1;
        return refreshRequests === 1
          ? HttpResponse.json({ data: { jwt: alexJwt } })
          : HttpResponse.error();
      }),
      http.get("/api/v1/users/me", () =>
        HttpResponse.json({ data: initialUser }),
      ),
    );
    const { wrapper: Wrapper } = createWrapper();
    render(
      <Wrapper>
        <Routes>
          <Route
            path="/"
            element={
              <RequireAuth>
                <textarea aria-label="Reply" />
              </RequireAuth>
            }
          />
          <Route path="/login" element={<p>Sign in</p>} />
        </Routes>
      </Wrapper>,
    );
    const reply = (await screen.findByLabelText(
      "Reply",
    )) as HTMLTextAreaElement;
    fireEvent.change(reply, { target: { value: "Alex's unsent words" } });

    await act(async () => {
      await vi.advanceTimersByTimeAsync(50 * 60 * 1000);
    });
    await waitFor(() => expect(refreshRequests).toBe(2));
    // No answer is not an expired session: nothing is signed out.
    await act(async () => {
      await vi.advanceTimersByTimeAsync(100);
    });
    expect(screen.queryByText("Sign in")).toBeNull();
    expect(screen.getByLabelText("Reply")).toBe(reply);
    expect(reply).toHaveValue("Alex's unsent words");
  });

  it.each([
    { path: "a periodic refresh", silent: false },
    { path: "a silent adoption", silent: true },
  ])(
    "keeps the page when $path only changes the same principal's scopes",
    async ({ silent }) => {
      vi.useFakeTimers({ shouldAdvanceTime: true });
      const alexJwt = makeJwt("org-g5labs", ["workspaces:read"], "user-alex");
      const widerJwt = makeJwt(
        "org-g5labs",
        ["workspaces:read", "org:admin"],
        "user-alex",
      );
      let refreshRequests = 0;
      server.use(
        http.post("/api/v1/auth/refresh", () => {
          refreshRequests += 1;
          return HttpResponse.json({
            data: { jwt: refreshRequests === 1 ? alexJwt : widerJwt },
          });
        }),
        // The profile for the new scopes never arrives.
        http.get("/api/v1/users/me", async ({ request }) => {
          if (request.headers.get("authorization") === `Bearer ${widerJwt}`) {
            await delay("infinite");
          }
          return HttpResponse.json({ data: initialUser });
        }),
      );
      const { wrapper: Wrapper } = createWrapper();
      render(
        <Wrapper>
          <Routes>
            <Route
              path="/"
              element={
                <RequireAuth>
                  <textarea aria-label="Reply" />
                </RequireAuth>
              }
            />
            <Route path="/login" element={<p>Sign in</p>} />
          </Routes>
        </Wrapper>,
      );
      const reply = (await screen.findByLabelText(
        "Reply",
      )) as HTMLTextAreaElement;
      fireEvent.change(reply, { target: { value: "Alex's unsent words" } });

      if (silent) {
        act(() => setApiToken(widerJwt));
      } else {
        await act(async () => {
          await vi.advanceTimersByTimeAsync(50 * 60 * 1000);
        });
        await waitFor(() => expect(refreshRequests).toBe(2));
      }
      await act(async () => {
        await vi.advanceTimersByTimeAsync(100);
      });
      expect(screen.queryByText("Sign in")).toBeNull();
      expect(screen.getByLabelText("Reply")).toBe(reply);
      expect(reply).toHaveValue("Alex's unsent words");
    },
  );

  it.each([
    { after: "an org switch", then: "switch" },
    { after: "another person's silent adoption", then: "silent" },
    { after: "nothing else", then: "none" },
  ] as const)(
    "shows a late profile answer only for the principal in use, after $after",
    async ({ then }) => {
      vi.useFakeTimers({ shouldAdvanceTime: true });
      const startJwt = makeJwt("org-g5labs", ["workspaces:read"]);
      // Another tab switched this person to org A: the periodic refresh
      // adopts it, and A's profile is slow to arrive.
      const orgAJwt = makeJwt("org-a", ["workspaces:read"]);
      const orgBJwt = makeJwt("org-b", ["workspaces:read"]);
      const caseyJwt = makeJwt("org-g5labs", ["workspaces:read"], "user-casey");
      const profile = (id: string, activeOrgId: string): CurrentUserDTO => ({
        ...initialUser,
        id,
        activeOrgId,
      });
      let releaseOrgA!: () => void;
      const orgAHeld = new Promise<void>((resolve) => {
        releaseOrgA = resolve;
      });
      let refreshes = 0;
      server.use(
        http.post("/api/v1/auth/refresh", async ({ request }) => {
          const text = await request.text();
          if (text && (JSON.parse(text) as { orgId?: string }).orgId) {
            return HttpResponse.json({ data: { jwt: orgBJwt } });
          }
          refreshes += 1;
          return HttpResponse.json({
            data: { jwt: refreshes === 1 ? startJwt : orgAJwt },
          });
        }),
        http.get("/api/v1/users/me", async ({ request }) => {
          switch (request.headers.get("authorization")) {
            case `Bearer ${orgAJwt}`:
              await orgAHeld;
              return HttpResponse.json({ data: profile("user-alex", "org-a") });
            case `Bearer ${orgBJwt}`:
              return HttpResponse.json({ data: profile("user-alex", "org-b") });
            case `Bearer ${caseyJwt}`:
              return HttpResponse.json({
                data: profile("user-casey", "org-g5labs"),
              });
            default:
              return HttpResponse.json({ data: initialUser });
          }
        }),
      );
      const { wrapper } = createWrapper();
      const { result } = renderHook(() => useAuth(), { wrapper });
      await waitFor(() =>
        expect(result.current.user?.activeOrgId).toBe("org-g5labs"),
      );

      await act(async () => {
        await vi.advanceTimersByTimeAsync(50 * 60 * 1000);
      });
      await waitFor(() => expect(refreshes).toBe(2));
      expect(result.current.principalKey).toBe("user-alex:org-a");

      if (then === "switch") {
        await act(async () => {
          await result.current.switchOrg("org-b");
        });
        expect(result.current.user?.activeOrgId).toBe("org-b");
      } else if (then === "silent") {
        act(() => setApiToken(caseyJwt));
        await waitFor(() => expect(result.current.user?.id).toBe("user-casey"));
      }

      // The older answer arrives.
      await act(async () => {
        releaseOrgA();
        await vi.advanceTimersByTimeAsync(100);
      });
      if (then === "switch") {
        expect(result.current.principalKey).toBe("user-alex:org-b");
        expect(result.current.user?.activeOrgId).toBe("org-b");
      } else if (then === "silent") {
        expect(result.current.principalKey).toBe("user-casey:org-g5labs");
        expect(result.current.user?.id).toBe("user-casey");
      } else {
        expect(result.current.principalKey).toBe("user-alex:org-a");
        expect(result.current.user?.activeOrgId).toBe("org-a");
      }
    },
  );

  describe("an org switch answered late", () => {
    const startJwt = makeJwt("org-g5labs", ["workspaces:read"]);
    const orgBJwt = makeJwt("org-b", ["workspaces:read"]);
    const orgCJwt = makeJwt("org-c", ["workspaces:read", "workspaces:write"]);
    const caseyJwt = makeJwt("org-g5labs", ["workspaces:read"], "user-casey");

    function serve() {
      let answerB!: () => void;
      const bHeld = new Promise<void>((resolve) => {
        answerB = resolve;
      });
      let askB!: () => void;
      const bAsked = new Promise<void>((resolve) => {
        askB = resolve;
      });
      server.use(
        http.post("/api/v1/auth/refresh", async ({ request }) => {
          const text = await request.text();
          const orgId = text
            ? (JSON.parse(text) as { orgId?: string }).orgId
            : undefined;
          if (orgId === "org-b") {
            askB();
            await bHeld;
            return HttpResponse.json({ data: { jwt: orgBJwt } });
          }
          if (orgId === "org-c") {
            return HttpResponse.json({ data: { jwt: orgCJwt } });
          }
          return HttpResponse.json({ data: { jwt: startJwt } });
        }),
        http.get("/api/v1/users/me", ({ request }) => {
          const used = request.headers.get("authorization");
          return HttpResponse.json({
            data:
              used === `Bearer ${caseyJwt}`
                ? { ...initialUser, id: "user-casey" }
                : used === `Bearer ${orgCJwt}`
                  ? { ...switchedUser, activeOrgId: "org-c" }
                  : used === `Bearer ${orgBJwt}`
                    ? { ...switchedUser, activeOrgId: "org-b" }
                    : initialUser,
          });
        }),
        http.post("/api/v1/auth/logout", () => HttpResponse.json({ data: {} })),
      );
      return { answerB, bAsked };
    }

    it.each([
      { after: "a sign-out", then: "logout" },
      { after: "sign-out and the same person's new sign-in", then: "relogin" },
      { after: "another person's sign-in", then: "silent" },
      { after: "a later switch", then: "switch" },
    ] as const)(
      "changes nothing when answered after $after",
      async ({ then }) => {
        const held = serve();
        const { wrapper } = createWrapper();
        const { result } = renderHook(() => useAuth(), { wrapper });
        await waitFor(() =>
          expect(result.current.user?.activeOrgId).toBe("org-g5labs"),
        );
        localStorage.setItem("g5_auth_token", startJwt);

        let switching!: Promise<string>;
        act(() => {
          switching = result.current.switchOrg("org-b").then(
            () => "switched",
            (error: Error) => error.message,
          );
        });
        await held.bAsked;
        if (then === "logout" || then === "relogin") {
          await act(async () => {
            await result.current.logout();
          });
          if (then === "relogin") {
            act(() => setApiToken(startJwt));
            await waitFor(() =>
              expect(result.current.user?.id).toBe("user-alex"),
            );
          }
        } else if (then === "silent") {
          act(() => setApiToken(caseyJwt));
          await waitFor(() =>
            expect(result.current.user?.id).toBe("user-casey"),
          );
        } else {
          await act(async () => {
            await result.current.switchOrg("org-c");
          });
          expect(result.current.user?.activeOrgId).toBe("org-c");
        }

        // The earlier switch is answered now.
        await act(async () => {
          held.answerB();
        });
        expect(await switching).toBe(
          "The session changed before the organization switch finished",
        );
        if (then === "logout") {
          expect(getApiToken()).toBeNull();
          expect(result.current.user).toBeNull();
          expect(result.current.scopes).toEqual([]);
          expect(localStorage.getItem("g5_auth_token")).toBeNull();
        } else if (then === "relogin") {
          expect(getApiToken()).toBe(startJwt);
          expect(result.current.user?.activeOrgId).toBe("org-g5labs");
        } else if (then === "silent") {
          expect(getApiToken()).toBe(caseyJwt);
          expect(result.current.user?.id).toBe("user-casey");
        } else {
          expect(getApiToken()).toBe(orgCJwt);
          expect(result.current.user?.activeOrgId).toBe("org-c");
          expect(result.current.scopes).toEqual([
            "workspaces:read",
            "workspaces:write",
          ]);
          expect(localStorage.getItem("g5_auth_token")).toBe(orgCJwt);
        }
      },
    );

    it("still switches when the same person's token was renewed meanwhile", async () => {
      const held = serve();
      const { wrapper } = createWrapper();
      const { result } = renderHook(() => useAuth(), { wrapper });
      await waitFor(() =>
        expect(result.current.user?.activeOrgId).toBe("org-g5labs"),
      );
      let switching!: Promise<void>;
      act(() => {
        switching = result.current.switchOrg("org-b");
      });
      await held.bAsked;
      // A renewal for the same person and org (another `iat`).
      act(() =>
        setApiToken(
          `${startJwt.split(".")[0]}.${btoa(JSON.stringify({ sub: "user-alex", org: "org-g5labs", scopes: ["workspaces:read"], iat: 1, exp: 9999999999 }))}.renewed`,
        ),
      );
      await act(async () => {
        held.answerB();
        await switching;
      });
      expect(getApiToken()).toBe(orgBJwt);
      await waitFor(() =>
        expect(result.current.user?.activeOrgId).toBe("org-b"),
      );
    });

    it("stores nothing for an org switch when another person signs in while it evicts", async () => {
      const held = serve();
      held.answerB();
      const { wrapper, queryClient } = createWrapper();
      const { result } = renderHook(() => useAuth(), { wrapper });
      await waitFor(() =>
        expect(result.current.user?.activeOrgId).toBe("org-g5labs"),
      );
      localStorage.setItem("g5_auth_token", startJwt);
      // While the switch evicts, another person's token is adopted.
      const cancel = queryClient.cancelQueries.bind(queryClient);
      let adoptDuringEviction = true;
      vi.spyOn(queryClient, "cancelQueries").mockImplementation(
        async (...args) => {
          if (adoptDuringEviction && getApiToken() === orgBJwt) {
            adoptDuringEviction = false;
            setApiToken(caseyJwt);
          }
          return cancel(...args);
        },
      );
      await act(async () => {
        await result.current.switchOrg("org-b");
      });
      expect(adoptDuringEviction).toBe(false);
      expect(getApiToken()).toBe(caseyJwt);
      expect(localStorage.getItem("g5_auth_token")).toBe(startJwt);
      await waitFor(() => expect(result.current.user?.id).toBe("user-casey"));
    });

    it("lets an org switch that completes while a silent adoption evicts stand", async () => {
      const held = serve();
      held.answerB();
      const { wrapper, queryClient } = createWrapper();
      const { result } = renderHook(() => useAuth(), { wrapper });
      await waitFor(() =>
        expect(result.current.user?.activeOrgId).toBe("org-g5labs"),
      );
      // While the silent adoption evicts, the person switches org.
      const cancel = queryClient.cancelQueries.bind(queryClient);
      let switchDuringEviction = true;
      vi.spyOn(queryClient, "cancelQueries").mockImplementation(
        async (...args) => {
          if (switchDuringEviction && getApiToken() === caseyJwt) {
            switchDuringEviction = false;
            await result.current.switchOrg("org-b");
          }
          return cancel(...args);
        },
      );
      await act(async () => {
        setApiToken(caseyJwt);
        await new Promise((resolve) => setTimeout(resolve, 50));
      });
      expect(switchDuringEviction).toBe(false);
      expect(getApiToken()).toBe(orgBJwt);
      await waitFor(() =>
        expect(result.current.user?.activeOrgId).toBe("org-b"),
      );
      expect(result.current.scopes).toEqual(["workspaces:read"]);
    });
  });

  describe("a renewal while an adoption evicts", () => {
    const alexJwt = makeJwt("org-g5labs", ["workspaces:read"]);
    /** The same claims, issued again: another signature. */
    const renewed = (jwt: string) => jwt.replace(/signature$/, "renewed");

    function serveProfiles() {
      server.use(
        http.post("/api/v1/auth/refresh", async ({ request }) => {
          const text = await request.text();
          const orgId = text
            ? (JSON.parse(text) as { orgId?: string }).orgId
            : undefined;
          return HttpResponse.json({
            data: {
              jwt:
                orgId === "org-b"
                  ? makeJwt("org-b", ["workspaces:read", "workspaces:write"])
                  : orgId === "org-casey"
                    ? makeJwt(
                        "org-casey",
                        ["workspaces:read", "tasks:execute"],
                        "user-casey",
                      )
                    : alexJwt,
            },
          });
        }),
        // Each profile is the one its token speaks for.
        http.get("/api/v1/users/me", ({ request }) => {
          const token = (request.headers.get("authorization") ?? "").slice(7);
          const claims = JSON.parse(atob(token.split(".")[1])) as {
            sub: string;
            org: string;
          };
          return HttpResponse.json({
            data: { ...switchedUser, id: claims.sub, activeOrgId: claims.org },
          });
        }),
      );
    }

    /** Holds the next eviction until released. */
    function holdNextEviction(queryClient: QueryClient) {
      const cancel = queryClient.cancelQueries.bind(queryClient);
      let armed = true;
      let release!: () => void;
      const released = new Promise<void>((resolve) => {
        release = resolve;
      });
      let reach!: () => void;
      const reached = new Promise<void>((resolve) => {
        reach = resolve;
      });
      vi.spyOn(queryClient, "cancelQueries").mockImplementation(
        async (...args) => {
          if (armed) {
            armed = false;
            reach();
            await released;
          }
          return cancel(...args);
        },
      );
      return { reached, release };
    }

    it.each([
      {
        what: "another person's silent adoption",
        adopted: makeJwt(
          "org-g5labs",
          ["workspaces:read", "tasks:execute"],
          "user-casey",
        ),
        person: "user-casey",
        scopes: ["workspaces:read", "tasks:execute"],
      },
      {
        what: "a silent change of scopes",
        adopted: makeJwt("org-g5labs", ["workspaces:read", "workspaces:write"]),
        person: "user-alex",
        scopes: ["workspaces:read", "workspaces:write"],
      },
    ])(
      "finishes $what with the renewed token",
      async ({ adopted, person, scopes }) => {
        serveProfiles();
        const { wrapper, queryClient } = createWrapper();
        const { result } = renderHook(() => useAuth(), { wrapper });
        await waitFor(() => expect(result.current.user?.id).toBe("user-alex"));
        const eviction = holdNextEviction(queryClient);
        act(() => setApiToken(adopted));
        await eviction.reached;
        act(() => setApiToken(renewed(adopted)));
        await act(async () => {
          eviction.release();
        });

        await waitFor(() => expect(result.current.scopes).toEqual(scopes));
        await waitFor(() => expect(result.current.user?.id).toBe(person));
        expect(getApiToken()).toBe(renewed(adopted));
      },
    );

    it.each([
      {
        what: "an org switch",
        orgId: "org-b",
        person: "user-alex",
        scopes: ["workspaces:read", "workspaces:write"],
      },
      {
        what: "an org switch answered for another person",
        orgId: "org-casey",
        person: "user-casey",
        scopes: ["workspaces:read", "tasks:execute"],
      },
    ])(
      "finishes $what with the renewed token",
      async ({ orgId, person, scopes }) => {
        serveProfiles();
        const { wrapper, queryClient } = createWrapper();
        const { result } = renderHook(() => useAuth(), { wrapper });
        await waitFor(() => expect(result.current.user?.id).toBe("user-alex"));
        localStorage.setItem("g5_auth_token", alexJwt);
        const eviction = holdNextEviction(queryClient);
        let switching!: Promise<void>;
        act(() => {
          switching = result.current.switchOrg(orgId);
        });
        await eviction.reached;
        const switched = getApiToken()!;
        act(() => setApiToken(renewed(switched)));
        await act(async () => {
          eviction.release();
          await switching;
        });

        expect(getApiToken()).toBe(renewed(switched));
        expect(result.current.scopes).toEqual(scopes);
        await waitFor(() => expect(result.current.user?.id).toBe(person));
        expect(result.current.user?.activeOrgId).toBe(orgId);
        expect(localStorage.getItem("g5_auth_token")).toBe(renewed(switched));
      },
    );
  });

  it("keeps a token installed while a periodic refusal evicts", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const startJwt = makeJwt("org-g5labs", ["workspaces:read"]);
    const caseyJwt = makeJwt("org-g5labs", ["workspaces:read"], "user-casey");
    let refreshes = 0;
    server.use(
      http.post("/api/v1/auth/refresh", () => {
        refreshes += 1;
        return refreshes === 1
          ? HttpResponse.json({ data: { jwt: startJwt } })
          : HttpResponse.json(
              { error: { code: "UNAUTHORIZED" } },
              { status: 401 },
            );
      }),
      http.get("/api/v1/users/me", ({ request }) =>
        HttpResponse.json({
          data:
            request.headers.get("authorization") === `Bearer ${caseyJwt}`
              ? { ...initialUser, id: "user-casey" }
              : initialUser,
        }),
      ),
    );
    const { wrapper, queryClient } = createWrapper();
    const { result } = renderHook(() => useAuth(), { wrapper });
    await waitFor(() => expect(result.current.user?.id).toBe("user-alex"));
    // While the refusal evicts, another path installs a token.
    const cancel = queryClient.cancelQueries.bind(queryClient);
    let installDuringEviction = true;
    vi.spyOn(queryClient, "cancelQueries").mockImplementation(
      async (...args) => {
        if (installDuringEviction && refreshes === 2) {
          installDuringEviction = false;
          setApiToken(caseyJwt);
        }
        return cancel(...args);
      },
    );
    await act(async () => {
      await vi.advanceTimersByTimeAsync(50 * 60 * 1000);
    });
    await waitFor(() => expect(installDuringEviction).toBe(false));
    await waitFor(() => expect(result.current.user?.id).toBe("user-casey"));
    expect(getApiToken()).toBe(caseyJwt);
  });

  it("starts no background refresh once signed out", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const startJwt = makeJwt("org-g5labs", ["workspaces:read"]);
    let refreshes = 0;
    server.use(
      // The server failed to revoke the refresh cookie.
      http.post("/api/v1/auth/refresh", () => {
        refreshes += 1;
        return HttpResponse.json({ data: { jwt: startJwt } });
      }),
      http.get("/api/v1/users/me", () =>
        HttpResponse.json({ data: initialUser }),
      ),
      http.post("/api/v1/auth/logout", () =>
        HttpResponse.json({ error: "unavailable" }, { status: 503 }),
      ),
    );
    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useAuth(), { wrapper });
    await waitFor(() => expect(result.current.user?.id).toBe("user-alex"));
    await act(async () => {
      await result.current.logout();
    });
    const before = refreshes;
    await act(async () => {
      await vi.advanceTimersByTimeAsync(50 * 60 * 1000);
    });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(100);
    });
    expect(refreshes).toBe(before);
    expect(getApiToken()).toBeNull();
    expect(result.current.user).toBeNull();
  });

  it("keeps a token another path installs while the first refresh is out", async () => {
    const startJwt = makeJwt("org-g5labs", ["workspaces:read"]);
    const caseyJwt = makeJwt("org-g5labs", ["workspaces:read"], "user-casey");
    let releaseFirst!: () => void;
    const firstHeld = new Promise<void>((resolve) => {
      releaseFirst = resolve;
    });
    server.use(
      http.post("/api/v1/auth/refresh", async () => {
        await firstHeld;
        return HttpResponse.json({ data: { jwt: startJwt } });
      }),
      http.get("/api/v1/users/me", ({ request }) =>
        HttpResponse.json({
          data:
            request.headers.get("authorization") === `Bearer ${caseyJwt}`
              ? { ...initialUser, id: "user-casey" }
              : initialUser,
        }),
      ),
    );
    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useAuth(), { wrapper });
    // A request's own refresh adopts a token first.
    act(() => setApiToken(caseyJwt));
    await waitFor(() => expect(result.current.user?.id).toBe("user-casey"));

    await act(async () => {
      releaseFirst();
      await new Promise((resolve) => setTimeout(resolve, 20));
    });
    expect(getApiToken()).toBe(caseyJwt);
    expect(result.current.user?.id).toBe("user-casey");
    expect(result.current.isLoading).toBe(false);
  });

  it.each([
    { answer: "a token for the context it started in", refused: false },
    { answer: "a refusal", refused: true },
  ] as const)(
    "drops a periodic refresh answered with $answer once an org switch has put a newer token in use",
    async ({ refused }) => {
      vi.useFakeTimers({ shouldAdvanceTime: true });
      const startJwt = makeJwt("org-g5labs", ["workspaces:read"]);
      const orgBJwt = makeJwt("org-b", ["workspaces:read"]);
      let releasePeriodic!: () => void;
      const periodicHeld = new Promise<void>((resolve) => {
        releasePeriodic = resolve;
      });
      let refreshes = 0;
      server.use(
        http.post("/api/v1/auth/refresh", async ({ request }) => {
          const text = await request.text();
          if (text && (JSON.parse(text) as { orgId?: string }).orgId) {
            return HttpResponse.json({ data: { jwt: orgBJwt } });
          }
          refreshes += 1;
          if (refreshes === 1) {
            return HttpResponse.json({ data: { jwt: startJwt } });
          }
          // The periodic refresh went out before the switch and answers
          // after it, for the context it started in.
          await periodicHeld;
          return refused
            ? HttpResponse.json(
                {
                  error: {
                    code: "UNAUTHORIZED",
                    message: "Session expired",
                    requestId: "request-refused",
                  },
                },
                { status: 401 },
              )
            : HttpResponse.json({
                data: { jwt: makeJwt("org-g5labs", ["workspaces:read"]) },
              });
        }),
        http.get("/api/v1/users/me", ({ request }) =>
          HttpResponse.json({
            data:
              request.headers.get("authorization") === `Bearer ${orgBJwt}`
                ? { ...switchedUser, activeOrgId: "org-b" }
                : initialUser,
          }),
        ),
      );
      const { wrapper } = createWrapper();
      const { result } = renderHook(() => useAuth(), { wrapper });
      await waitFor(() =>
        expect(result.current.user?.activeOrgId).toBe("org-g5labs"),
      );

      await act(async () => {
        await vi.advanceTimersByTimeAsync(50 * 60 * 1000);
      });
      await waitFor(() => expect(refreshes).toBe(2));
      await act(async () => {
        await result.current.switchOrg("org-b");
      });
      expect(result.current.user?.activeOrgId).toBe("org-b");

      // The earlier answer arrives: the switch stays in force.
      await act(async () => {
        releasePeriodic();
        await vi.advanceTimersByTimeAsync(100);
      });
      expect(getApiToken()).toBe(orgBJwt);
      expect(result.current.principalKey).toBe("user-alex:org-b");
      expect(result.current.user?.activeOrgId).toBe("org-b");
    },
  );

  it.each([
    { who: "another person, through the shared session", sub: "user-casey" },
    { who: "the same person", sub: "user-alex" },
  ] as const)(
    "shows no outgoing profile under the new token when an org switch answers for $who",
    async ({ sub }) => {
      const startJwt = makeJwt("org-g5labs", ["workspaces:read"]);
      const switchedJwt = makeJwt("org-b", ["workspaces:read"], sub);
      server.use(
        http.post("/api/v1/auth/refresh", async ({ request }) => {
          const text = await request.text();
          const body = text ? (JSON.parse(text) as { orgId?: string }) : {};
          return HttpResponse.json({
            data: { jwt: body.orgId === "org-b" ? switchedJwt : startJwt },
          });
        }),
        // The new profile never arrives.
        http.get("/api/v1/users/me", async ({ request }) => {
          if (
            request.headers.get("authorization") === `Bearer ${switchedJwt}`
          ) {
            await delay("infinite");
          }
          return HttpResponse.json({ data: initialUser });
        }),
      );
      const { wrapper } = createWrapper();
      const { result } = renderHook(() => useAuth(), { wrapper });
      await waitFor(() => expect(result.current.user?.id).toBe("user-alex"));

      act(() => {
        void result.current.switchOrg("org-b");
      });
      await waitFor(() =>
        expect(result.current.principalKey).toBe(`${sub}:org-b`),
      );
      if (sub === "user-alex") {
        // The same person: the profile names the new org at once.
        expect(result.current.user?.id).toBe("user-alex");
        expect(result.current.user?.activeOrgId).toBe("org-b");
      } else {
        expect(result.current.user).toBeNull();
      }
    },
  );

  it("evicts on ANY cross-principal token adoption (silent 401-refresh path) but not on same-principal rotation", async () => {
    const alexJwt = makeJwt("org-g5labs", ["workspaces:read"], "user-alex");
    const { wrapper, queryClient } = createWrapper();
    server.use(
      http.post("/api/v1/auth/refresh", () =>
        HttpResponse.json({ data: { jwt: alexJwt } }),
      ),
      http.get("/api/v1/users/me", () =>
        HttpResponse.json({ data: initialUser }),
      ),
    );

    // Mount the REAL scoped-config hook next to the provider: eviction must
    // reach mounted observers (hook output), not just the cache collections.
    const { result } = renderHook(
      () => ({
        auth: useAuth(),
        config: useUserScopedConfig("org-g5labs"),
      }),
      { wrapper },
    );
    const configData = () => {
      const c = result.current.config;
      return c.status === "ok" ? c.data : undefined;
    };
    await waitFor(() => expect(result.current.auth.user).not.toBeNull());
    await waitFor(() => expect(configData()).toBeDefined());

    const seedCaches = () => {
      queryClient.setQueryData(["scoped-config", "user", "org-g5labs"], {
        scope: "user",
        secretKeyHints: { ANTHROPIC_AUTH_TOKEN: "sk-ant-oat01" },
      });
      queryClient.setQueryData(["unrelated", "public"], "preserve-me");
    };

    // Same-principal token rotation (fresh JWT, same sub/org — the normal
    // 50-minute refresh) must NOT churn the caches.
    seedCaches();
    await act(async () => {
      setApiToken(makeJwt("org-g5labs", ["workspaces:read"], "user-alex"));
    });
    expect(
      (
        queryClient.getQueryData(["scoped-config", "user", "org-g5labs"]) as {
          secretKeyHints?: Record<string, string>;
        }
      )?.secretKeyHints?.ANTHROPIC_AUTH_TOKEN,
    ).toBe("sk-ant-oat01");

    // The mounted hook sees the seeded hint before the transition.
    await waitFor(() =>
      expect(configData()?.secretKeyHints?.ANTHROPIC_AUTH_TOKEN).toBe(
        "sk-ant-oat01",
      ),
    );

    // A same-principal SCOPE DOWNGRADE must also evict: the reduced token
    // no longer authorizes the cached reads (settings-page D9).
    await act(async () => {
      setApiToken(makeJwt("org-g5labs", [], "user-alex"));
    });
    await waitFor(() => {
      expect(
        configData()?.secretKeyHints?.ANTHROPIC_AUTH_TOKEN,
      ).toBeUndefined();
    });

    // Restore the full-scope session and re-seed.
    await act(async () => {
      setApiToken(makeJwt("org-g5labs", ["workspaces:read"], "user-alex"));
    });
    seedCaches();

    // A cross-tab ORG switch — same user, same scopes, different `org`
    // claim — must also evict (the real JWT carries `org`; a comparison
    // against invented claim names would miss exactly this case).
    await act(async () => {
      setApiToken(makeJwt("org-other", ["workspaces:read"], "user-alex"));
    });
    await waitFor(() => {
      expect(
        configData()?.secretKeyHints?.ANTHROPIC_AUTH_TOKEN,
      ).toBeUndefined();
    });

    // Back to the baseline session and re-seed for the cross-principal
    // case.
    await act(async () => {
      setApiToken(makeJwt("org-g5labs", ["workspaces:read"], "user-alex"));
    });
    seedCaches();

    // A token adoption that changes principal — the api client's silent
    // 401-refresh retry path calls setApiToken directly, bypassing the
    // AuthProvider handlers — must evict scoped-config data, drop it from
    // mounted hook output, AND re-synchronize the displayed identity to
    // the newly adopted principal.
    const bobJwt = makeJwt("org-g5labs", ["workspaces:read"], "user-bob");
    server.use(
      http.get("/api/v1/users/me", ({ request }) => {
        if (request.headers.get("authorization") === `Bearer ${bobJwt}`) {
          return HttpResponse.json({
            data: { ...initialUser, id: "user-bob" },
          });
        }
        return HttpResponse.json({ data: initialUser });
      }),
    );
    await act(async () => {
      setApiToken(bobJwt);
    });
    await waitFor(() => {
      expect(
        configData()?.secretKeyHints?.ANTHROPIC_AUTH_TOKEN,
      ).toBeUndefined();
      expect(result.current.auth.user?.id).toBe("user-bob");
      expect(result.current.auth.scopes).toEqual(["workspaces:read"]);
    });
    expect(queryClient.getQueryData(["unrelated", "public"])).toBe(
      "preserve-me",
    );
  });

  it("preserves unrelated public cache when periodic refresh expires the session", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const jwt = makeJwt("org-g5labs", ["workspaces:read"]);
    let refreshRequests = 0;
    let expireSession = false;
    server.use(
      http.post("/api/v1/auth/refresh", () => {
        refreshRequests += 1;
        if (!expireSession) {
          return HttpResponse.json({ data: { jwt } });
        }
        return HttpResponse.json({ error: "expired" }, { status: 401 });
      }),
      http.get("/api/v1/users/me", () =>
        HttpResponse.json({ data: initialUser }),
      ),
    );

    const { wrapper, queryClient } = createWrapper();
    const { result } = renderHook(() => useAuth(), { wrapper });
    await waitFor(() => expect(result.current.user).not.toBeNull());
    expireSession = true;

    queryClient.setQueryData(["unrelated", "public"], "preserve-me");

    await act(async () => {
      await vi.advanceTimersByTimeAsync(50 * 60 * 1000);
    });
    await waitFor(() => expect(result.current.user).toBeNull());

    expect(refreshRequests).toBeGreaterThanOrEqual(2);
    expect(queryClient.getQueryData(["unrelated", "public"])).toBe(
      "preserve-me",
    );
  });
});

describe("membership revocation recovery", () => {
  afterEach(() => vi.useRealTimers());

  it.each(["other-org", "other-user"])(
    "ignores a delayed socket revocation for %s without clearing the selected organization's queries",
    async (previousContext) => {
      localStorage.clear();
      const current = makeJwt("org-g5labs", ["workspaces:read"]);
      const previous = makeJwt(
        previousContext === "other-org" ? "org-old" : "org-g5labs",
        ["workspaces:read"],
        previousContext === "other-user" ? "user-old" : "user-alex",
      );
      const refresh = vi.fn(() =>
        HttpResponse.json({ data: { jwt: current } }),
      );
      server.use(
        http.post("/api/v1/auth/refresh", refresh),
        http.get("/api/v1/users/me", () =>
          HttpResponse.json({ data: initialUser }),
        ),
      );
      const { wrapper, queryClient } = createWrapper();
      const { result } = renderHook(() => useAuth(), { wrapper });
      await waitFor(() =>
        expect(result.current.user?.activeOrgId).toBe("org-g5labs"),
      );
      refresh.mockClear();
      localStorage.setItem("g5_auth_token", current);
      queryClient.setQueryData(["workspace", "selected-org"], "keep me");
      const cancel = vi.spyOn(queryClient, "cancelQueries");
      const signal = scopedConfigSignal();
      await act(async () => {
        window.dispatchEvent(
          new CustomEvent("g5:organization-access-changed", {
            detail: { principal: tokenPrincipal(previous) },
          }),
        );
      });
      expect(queryClient.getQueryData(["workspace", "selected-org"])).toBe(
        "keep me",
      );
      expect(cancel).not.toHaveBeenCalled();
      expect(signal.aborted).toBe(false);
      expect(refresh).not.toHaveBeenCalled();
      expect(localStorage.getItem("g5_auth_token")).toBe(current);
      expect(getApiToken()).toBe(current);
    },
  );

  it.each([false, true])(
    "clears disabled-organization data and adopts another active organization without logout (stored JWT: %s)",
    async (stored) => {
      localStorage.clear();
      const { wrapper, queryClient } = createWrapper();
      const before = makeJwt("org-g5labs", ["workspaces:read"]);
      const after = makeJwt("org-new", ["workspaces:read"]);
      if (stored) localStorage.setItem("g5_auth_token", before);
      let disabled = false;
      const loggedOut = vi.fn();
      server.use(
        http.post("/api/v1/auth/refresh", () =>
          HttpResponse.json({ data: { jwt: disabled ? after : before } }),
        ),
        http.post("/api/v1/auth/logout", () => {
          loggedOut();
          return HttpResponse.json({ data: {} });
        }),
        http.get("/api/v1/users/me", () =>
          HttpResponse.json({
            data: disabled
              ? {
                  ...initialUser,
                  activeOrgId: "org-new",
                  orgs: [
                    {
                      orgId: "org-new",
                      orgName: "Other",
                      orgSlug: "other",
                      role: "member",
                      orgKind: "tenant",
                    },
                  ],
                }
              : initialUser,
          }),
        ),
      );
      const { result } = renderHook(() => useAuth(), { wrapper });
      await waitFor(() =>
        expect(result.current.user?.activeOrgId).toBe("org-g5labs"),
      );
      queryClient.setQueryData(["workspace", "disabled-workspace"], {
        name: "Old organization",
      });
      disabled = true;
      act(() => {
        window.dispatchEvent(
          new CustomEvent("g5:organization-access-changed", {
            detail: { principal: tokenPrincipal(before) },
          }),
        );
      });
      await waitFor(() =>
        expect(result.current.user?.activeOrgId).toBe("org-new"),
      );
      expect(
        queryClient.getQueryData(["workspace", "disabled-workspace"]),
      ).toBeUndefined();
      expect(getApiToken()).toBe(after);
      expect(localStorage.getItem("g5_auth_token")).toBeNull();
      expect(loggedOut).not.toHaveBeenCalled();
    },
  );

  it.each([false, true])(
    "retries transient recovery with backoff and stops after a context switch (%s)",
    async (switchDuringBackoff) => {
      localStorage.clear();
      vi.useFakeTimers({ shouldAdvanceTime: true });
      const before = makeJwt("org-g5labs", ["workspaces:read"]);
      const after = makeJwt("org-new", ["workspaces:read"]);
      let disabled = false;
      let attempts = 0;
      server.use(
        http.post("/api/v1/auth/refresh", () => {
          if (!disabled) return HttpResponse.json({ data: { jwt: before } });
          attempts += 1;
          return attempts === 1
            ? HttpResponse.json({ error: "unavailable" }, { status: 503 })
            : HttpResponse.json({ data: { jwt: after } });
        }),
        http.get("/api/v1/users/me", ({ request }) =>
          HttpResponse.json({
            data: {
              ...initialUser,
              activeOrgId:
                request.headers.get("authorization") === `Bearer ${after}`
                  ? "org-new"
                  : "org-g5labs",
            },
          }),
        ),
      );
      const { wrapper } = createWrapper();
      const { result } = renderHook(() => useAuth(), { wrapper });
      await waitFor(() =>
        expect(result.current.user?.activeOrgId).toBe("org-g5labs"),
      );
      disabled = true;
      await act(async () => {
        window.dispatchEvent(
          new CustomEvent("g5:organization-access-changed", {
            detail: { principal: tokenPrincipal(before) },
          }),
        );
      });
      await waitFor(() => expect(attempts).toBe(1));
      // Repeated denial events cannot defeat the backoff or create parallel runs.
      act(() => {
        window.dispatchEvent(
          new CustomEvent("g5:organization-access-changed", {
            detail: { principal: tokenPrincipal(before) },
          }),
        );
        if (switchDuringBackoff) setApiToken(after);
      });
      await act(async () => {
        await vi.advanceTimersByTimeAsync(2_000);
      });
      await waitFor(() =>
        expect(result.current.user?.activeOrgId).toBe("org-new"),
      );
      expect(attempts).toBe(switchDuringBackoff ? 1 : 2);
      expect(getApiToken()).toBe(after);
    },
  );
});
