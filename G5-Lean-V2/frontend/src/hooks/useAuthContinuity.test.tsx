import React from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { http, HttpResponse } from "msw";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { AuthProvider, RequireAuth, useAuth } from "./useAuth";
import { api, getApiToken, setApiToken } from "../lib/api";
import { server } from "../test/msw/server";
import {
  fileDraftKey,
  getFileDraft,
  setFileDraft,
} from "../components/files/fileDrafts";
import type { CurrentUserDTO } from "../lib/types";

const user: CurrentUserDTO = {
  id: "alex",
  email: "alex@example.com",
  displayName: "Alex",
  avatarUrl: null,
  oauthProvider: "github",
  activeOrgId: "org-a",
  orgs: [],
  scmConnections: [],
  createdAt: "2026-01-01",
};
function jwt(org = "org-a", sub = "alex", exp = 9999999999) {
  return `e30.${btoa(JSON.stringify({ sub, org, scopes: ["workspaces:read"], exp }))}.sig`;
}
const draftKey = fileDraftKey({
  person: "alex",
  workspaceId: "ws-a",
  repoId: "repo",
  path: "draft.md",
});
function Page() {
  const auth = useAuth();
  return (
    <>
      <p>
        {auth.user?.id}:{auth.user?.activeOrgId}
      </p>
      <p>{auth.user?.displayName}</p>
      <textarea aria-label="Chat" />
    </>
  );
}
function Controls() {
  const auth = useAuth();
  return <button onClick={() => void auth.logout()}>Sign out</button>;
}
function mount() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter>
        <AuthProvider>
          <Controls />
          <Routes>
            <Route
              path="/"
              element={
                <RequireAuth>
                  <Page />
                </RequireAuth>
              }
            />
            <Route path="/login" element={<p>Sign in</p>} />
          </Routes>
        </AuthProvider>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}
const unavailable = (status: number) =>
  status === 0 ? HttpResponse.error() : new HttpResponse(null, { status });
const tick = (ms = 1100) => act(() => vi.advanceTimersByTimeAsync(ms));

describe("browser authentication continuity", () => {
  beforeEach(() => {
    setApiToken(null);
    localStorage.clear();
    vi.useFakeTimers({ shouldAdvanceTime: true });
    server.use(
      http.post("/api/v1/auth/refresh", () =>
        HttpResponse.json({ data: { jwt: jwt() } }),
      ),
      http.get("/api/v1/users/me", () => HttpResponse.json({ data: user })),
      http.post(
        "/api/v1/auth/logout",
        () => new HttpResponse(null, { status: 503 }),
      ),
    );
  });
  afterEach(() => vi.useRealTimers());

  it.each([0, 408, 429, 503])(
    "retries initial refresh %i without visiting sign-in",
    async (status) => {
      let requests = 0;
      server.use(
        http.post("/api/v1/auth/refresh", () =>
          ++requests === 1
            ? unavailable(status)
            : HttpResponse.json({ data: { jwt: jwt() } }),
        ),
      );
      mount();
      await tick(100);
      expect(requests).toBe(1);
      expect(screen.queryByText("Sign in")).toBeNull();
      expect(
        screen.getByRole("status", { name: "Loading" }),
      ).toBeInTheDocument();
      await tick();
      expect(await screen.findByLabelText("Chat")).toBeInTheDocument();
      expect(requests).toBe(2);
    },
  );

  it.each([0, 408, 429, 503])(
    "retries an initial profile %i after partial recovery",
    async (status) => {
      let profiles = 0;
      let refreshes = 0;
      server.use(
        http.post("/api/v1/auth/refresh", () => {
          refreshes++;
          return HttpResponse.json({ data: { jwt: jwt() } });
        }),
        http.get("/api/v1/users/me", () =>
          ++profiles === 1
            ? unavailable(status)
            : HttpResponse.json({ data: user }),
        ),
      );
      mount();
      await tick(100);
      expect(screen.queryByText("Sign in")).toBeNull();
      expect(
        screen.getByRole("status", { name: "Loading" }),
      ).toBeInTheDocument();
      await tick();
      expect(await screen.findByLabelText("Chat")).toBeInTheDocument();
      expect(profiles).toBe(2);
      expect(refreshes).toBe(1);
    },
  );

  it.each(["silent", "periodic"])(
    "preserves drafts and the mounted composer through %s organization adoption and profile recovery",
    async (path) => {
      mount();
      const chat = await screen.findByLabelText("Chat");
      fireEvent.change(chat, { target: { value: "Unsent chat" } });
      setFileDraft(draftKey, {
        id: "draft",
        role: "owner",
        content: "Unsent file",
        baseline: { content: "", hash: null, vmId: null },
      });
      let profiles = 0;
      let recovering = true;
      server.use(
        http.post("/api/v1/auth/refresh", () =>
          HttpResponse.json({ data: { jwt: jwt("org-b") } }),
        ),
        http.get("/api/v1/users/me", () => {
          profiles++;
          return recovering
            ? unavailable(503)
            : HttpResponse.json({ data: { ...user, activeOrgId: "org-b" } });
        }),
        http.get(
          "/api/v1/continuity-probe",
          () => new HttpResponse(null, { status: 401 }),
        ),
      );
      if (path === "silent") {
        await act(async () => {
          await expect(api.get("/api/v1/continuity-probe")).rejects.toThrow();
        });
      } else await tick(50 * 60 * 1000);
      await waitFor(() => expect(profiles).toBeGreaterThan(0));
      expect(screen.queryByText("Sign in")).toBeNull();
      expect(screen.getByLabelText("Chat")).toBe(chat);
      expect(chat).toHaveValue("Unsent chat");
      expect(screen.getByText("alex:org-b")).toBeInTheDocument();
      expect(getFileDraft(draftKey)?.content).toBe("Unsent file");
      recovering = false;
      await tick(30_100);
      expect(profiles).toBeGreaterThan(1);
      expect(screen.getByLabelText("Chat")).toBe(chat);
      expect(getFileDraft(draftKey)?.content).toBe("Unsent file");
    },
  );

  it("renews an unauthorized initial profile, but signs in after a refused refresh", async () => {
    let refreshes = 0;
    server.use(
      http.post("/api/v1/auth/refresh", () =>
        ++refreshes === 1
          ? HttpResponse.json({ data: { jwt: jwt() } })
          : new HttpResponse(null, { status: 401 }),
      ),
      http.get(
        "/api/v1/users/me",
        () => new HttpResponse(null, { status: 401 }),
      ),
    );
    mount();
    await tick();
    expect(await screen.findByText("Sign in")).toBeInTheDocument();
    expect(getApiToken()).toBeNull();
    expect(refreshes).toBe(2);
  });

  it("cancels initial refresh backoff on sign-out even with no installed token", async () => {
    let requests = 0;
    server.use(
      http.post("/api/v1/auth/refresh", () => {
        requests++;
        return unavailable(503);
      }),
    );
    mount();
    await tick(100);
    fireEvent.click(screen.getByRole("button", { name: "Sign out" }));
    await tick(60_000);
    expect(await screen.findByText("Sign in")).toBeInTheDocument();
    expect(requests).toBe(1);
  });

  it.each([0, 408, 429, 503])(
    "remembers a renewed credential across profile failure %i before refusal",
    async (status) => {
      localStorage.setItem("g5_auth_token", jwt());
      const renewed = jwt().replace(".sig", ".renewed");
      let refreshes = 0;
      let renewedProfiles = 0;
      server.use(
        http.post("/api/v1/auth/refresh", () => {
          refreshes++;
          return HttpResponse.json({ data: { jwt: renewed } });
        }),
        http.get("/api/v1/users/me", ({ request }) => {
          if (request.headers.get("authorization") !== `Bearer ${renewed}`)
            return new HttpResponse(null, { status: 401 });
          return ++renewedProfiles % 2 === 1
            ? unavailable(status)
            : new HttpResponse(null, { status: 401 });
        }),
      );
      mount();
      await tick(100);
      expect(screen.queryByText("Sign in")).toBeNull();
      await tick();
      expect(screen.getByText("Sign in")).toBeInTheDocument();
      await tick(60_000);
      expect(refreshes).toBe(1);
      expect(renewedProfiles).toBe(2);
      expect(getApiToken()).toBeNull();
    },
  );

  it("renews again if a renewed credential expires during profile backoff", async () => {
    localStorage.setItem("g5_auth_token", jwt());
    const shortLived = jwt("org-a", "alex", Math.floor(Date.now() / 1000) + 1);
    const latest = jwt().replace(".sig", ".latest");
    let refreshes = 0;
    let shortProfiles = 0;
    server.use(
      http.post("/api/v1/auth/refresh", () =>
        HttpResponse.json({
          data: { jwt: ++refreshes === 1 ? shortLived : latest },
        }),
      ),
      http.get("/api/v1/users/me", ({ request }) => {
        const bearer = request.headers.get("authorization");
        if (bearer === `Bearer ${latest}`)
          return HttpResponse.json({ data: user });
        if (bearer === `Bearer ${shortLived}` && ++shortProfiles === 1)
          return unavailable(503);
        return new HttpResponse(null, { status: 401 });
      }),
    );
    mount();
    await tick(100);
    expect(screen.queryByText("Sign in")).toBeNull();
    await tick();
    expect(screen.getByLabelText("Chat")).toBeInTheDocument();
    expect(refreshes).toBe(2);
    expect(getApiToken()).toBe(latest);
  });

  it("ends the session if renewal immediately returns a refused expired token", async () => {
    localStorage.setItem("g5_auth_token", jwt());
    let refreshes = 0;
    server.use(
      http.post("/api/v1/auth/refresh", () => {
        refreshes++;
        return HttpResponse.json({ data: { jwt: jwt("org-a", "alex", 1) } });
      }),
      http.get("/api/v1/users/me", () => new HttpResponse(null, { status: 401 })),
    );
    mount();
    await tick(100);
    expect(screen.getByText("Sign in")).toBeInTheDocument();
    expect(refreshes).toBe(1);
    expect(getApiToken()).toBeNull();
  });

  it.each([401, 403])(
    "ends initial authentication on refresh refusal %i",
    async (status) => {
      server.use(
        http.post(
          "/api/v1/auth/refresh",
          () => new HttpResponse(null, { status }),
        ),
      );
      mount();
      await tick(100);
      expect(screen.getByText("Sign in")).toBeInTheDocument();
      expect(getApiToken()).toBeNull();
    },
  );

  it("ignores a successful initial refresh that arrives after sign-out", async () => {
    let release!: () => void;
    const held = new Promise<void>((resolve) => {
      release = resolve;
    });
    server.use(
      http.post("/api/v1/auth/refresh", async () => {
        await held;
        return HttpResponse.json({ data: { jwt: jwt() } });
      }),
    );
    mount();
    await tick(100);
    fireEvent.click(screen.getByRole("button", { name: "Sign out" }));
    await tick(100);
    release();
    await tick(100);
    expect(screen.getByText("Sign in")).toBeInTheDocument();
    expect(getApiToken()).toBeNull();
  });

  it("recovers a stored token's profile 401 through a temporarily unavailable refresh", async () => {
    const renewed = jwt().replace(".sig", ".renewed");
    localStorage.setItem("g5_auth_token", jwt());
    let refreshes = 0;
    server.use(
      http.post("/api/v1/auth/refresh", () =>
        ++refreshes === 1
          ? unavailable(429)
          : HttpResponse.json({ data: { jwt: renewed } }),
      ),
      http.get("/api/v1/users/me", ({ request }) =>
        request.headers.get("authorization") === `Bearer ${renewed}`
          ? HttpResponse.json({ data: user })
          : new HttpResponse(null, { status: 401 }),
      ),
    );
    mount();
    await tick(100);
    expect(screen.queryByText("Sign in")).toBeNull();
    await tick();
    expect(await screen.findByLabelText("Chat")).toBeInTheDocument();
    expect(getApiToken()).toBe(renewed);
    expect(localStorage.getItem("g5_auth_token")).toBeNull();
    expect(refreshes).toBe(2);
  });

  it("ends the session when the profile refuses a freshly renewed credential", async () => {
    localStorage.setItem("g5_auth_token", jwt());
    let refreshes = 0;
    server.use(
      http.post("/api/v1/auth/refresh", () => {
        refreshes++;
        return HttpResponse.json({
          data: { jwt: jwt().replace(".sig", ".renewed") },
        });
      }),
      http.get(
        "/api/v1/users/me",
        () => new HttpResponse(null, { status: 401 }),
      ),
    );
    mount();
    await tick(100);
    expect(screen.getByText("Sign in")).toBeInTheDocument();
    await tick(60_000);
    expect(refreshes).toBe(1);
    expect(getApiToken()).toBeNull();
  });

  it.each([401, 403])(
    "ignores a profile %i for a renewed token that was superseded again",
    async (status) => {
      localStorage.setItem("g5_auth_token", jwt());
      const renewed = jwt().replace(".sig", ".renewed");
      const latest = jwt().replace(".sig", ".latest");
      let release!: () => void;
      const held = new Promise<void>((resolve) => {
        release = resolve;
      });
      let renewedProfileStarted = false;
      server.use(
        http.post("/api/v1/auth/refresh", () =>
          HttpResponse.json({ data: { jwt: renewed } }),
        ),
        http.get("/api/v1/users/me", async ({ request }) => {
          const bearer = request.headers.get("authorization");
          if (bearer === `Bearer ${latest}`)
            return HttpResponse.json({ data: user });
          if (bearer === `Bearer ${renewed}`) {
            renewedProfileStarted = true;
            await held;
            return new HttpResponse(null, { status });
          }
          return new HttpResponse(null, { status: 401 });
        }),
      );
      mount();
      await tick(100);
      expect(renewedProfileStarted).toBe(true);
      act(() => setApiToken(latest));
      release();
      await tick(100);
      expect(screen.getByLabelText("Chat")).toBeInTheDocument();
      expect(getApiToken()).toBe(latest);
      expect(screen.queryByText("Sign in")).toBeNull();
    },
  );

  it.each(["sign-out", "unmount", "another person"])(
    "stops profile backoff after %s",
    async (transition) => {
      let profiles = 0;
      server.use(
        http.get("/api/v1/users/me", ({ request }) => {
          if (
            request.headers.get("authorization") ===
            `Bearer ${jwt("org-b", "bob")}`
          ) {
            return HttpResponse.json({
              data: { ...user, id: "bob", activeOrgId: "org-b" },
            });
          }
          profiles++;
          return unavailable(503);
        }),
      );
      const view = mount();
      await tick(100);
      expect(profiles).toBe(1);
      if (transition === "unmount") view.unmount();
      else if (transition === "sign-out")
        fireEvent.click(screen.getByRole("button", { name: "Sign out" }));
      else act(() => setApiToken(jwt("org-b", "bob")));
      await tick(60_000);
      expect(profiles).toBe(1);
      if (transition === "another person")
        expect(screen.getByText("bob:org-b")).toBeInTheDocument();
      if (transition === "sign-out")
        expect(screen.getByText("Sign in")).toBeInTheDocument();
    },
  );

  it.each(["success", "refusal"])(
    "ignores an old profile %s after leaving and returning to the same token",
    async (answer) => {
      let release!: () => void;
      const held = new Promise<void>((resolve) => {
        release = resolve;
      });
      let profiles = 0;
      server.use(
        http.get("/api/v1/users/me", async ({ request }) => {
          if (++profiles === 1) {
            await held;
            return answer === "success"
              ? HttpResponse.json({
                  data: { ...user, displayName: "Obsolete profile" },
                })
              : new HttpResponse(null, { status: 403 });
          }
          return HttpResponse.json({
            data: {
              ...user,
              activeOrgId:
                request.headers.get("authorization") ===
                `Bearer ${jwt("org-b")}`
                  ? "org-b"
                  : "org-a",
            },
          });
        }),
      );
      mount();
      await tick(100);
      expect(profiles).toBe(1);
      act(() => setApiToken(jwt("org-b")));
      await tick(100);
      expect(screen.getByText("alex:org-b")).toBeInTheDocument();
      act(() => setApiToken(jwt()));
      await tick(100);
      release();
      await tick(100);
      expect(screen.getByText("alex:org-a")).toBeInTheDocument();
      expect(screen.queryByText("Obsolete profile")).toBeNull();
      expect(screen.queryByText("Sign in")).toBeNull();
    },
  );

  it("clears file drafts immediately when another person's profile is unavailable", async () => {
    mount();
    await screen.findByLabelText("Chat");
    setFileDraft(draftKey, {
      id: "draft",
      role: "owner",
      content: "Private",
      baseline: { content: "", hash: null, vmId: null },
    });
    server.use(http.get("/api/v1/users/me", () => unavailable(503)));
    act(() => setApiToken(jwt("org-b", "bob")));
    await tick(100);
    expect(getFileDraft(draftKey)).toBeNull();
    expect(screen.queryByLabelText("Chat")).toBeNull();
    expect(screen.getByRole("status", { name: "Loading" })).toBeInTheDocument();
  });
});
