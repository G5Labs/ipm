// The API client's answers to failures (`g5.workspace.page.refresh`): no
// usable answer is a network failure a retry may fix (status 0), an answer
// that is not JSON is not, and a request is sent again after a token
// refresh only for the principal that sent it.

import { http, HttpResponse } from "msw";
import { describe, expect, it, vi } from "vitest";
import { server } from "../test/msw/server";
import { ApiError, createApiClient, tokenPrincipal } from "./api";
import { isTransientFailure } from "../hooks/hookUtils";

function jwtFor(sub: string, org: string, signature = "signature"): string {
  const encode = (value: object) =>
    btoa(JSON.stringify(value))
      .replace(/\+/g, "-")
      .replace(/\//g, "_")
      .replace(/=+$/, "");
  return `${encode({ alg: "HS256" })}.${encode({ sub, org, scopes: [] })}.${signature}`;
}

function clientWith(token: string) {
  let current: string | null = token;
  const onAuthFailure = vi.fn();
  const client = createApiClient({
    getToken: () => current,
    setToken: (next) => {
      current = next;
    },
    onAuthFailure,
  });
  return {
    client,
    onAuthFailure,
    token: () => current,
    /** Another path (an org switch, the provider, a sign-out) installs one. */
    install: (next: string | null) => {
      current = next;
    },
  };
}

async function failureOf(promise: Promise<unknown>): Promise<ApiError> {
  try {
    await promise;
  } catch (error) {
    if (error instanceof ApiError) return error;
    throw new Error(`not an ApiError: ${String(error)}`);
  }
  throw new Error("the request succeeded");
}

describe("API client failures", () => {
  it.each(["get", "getRawResponse", "postRaw"] as const)(
    "%s preserves a structured server error's reason and details",
    async (method) => {
      server.use(
        http.all("*/api/v1/forbidden", () =>
          HttpResponse.json(
            {
              error: {
                code: "FORBIDDEN",
                message: "Workspace access was removed",
                details: { workspaceId: "source" },
              },
            },
            { status: 403 },
          ),
        ),
      );
      const { client } = clientWith(jwtFor("user-a", "org-1"));
      const error = await failureOf(client[method]("/api/v1/forbidden"));
      expect(error).toMatchObject({
        code: "FORBIDDEN",
        message: "Workspace access was removed",
        details: { workspaceId: "source" },
        status: 403,
      });
      expect(isTransientFailure(error)).toBe(false);
    },
  );

  it.each(["get", "getRawResponse", "postRaw"] as const)(
    "%s explains a non-JSON 502 with empty HTTP/2 status text",
    async (method) => {
      const fetch = vi.spyOn(globalThis, "fetch").mockResolvedValue(
        new Response("<html>failed_to_pick_backend</html>", {
          status: 502,
          statusText: "",
        }),
      );
      try {
        const { client } = clientWith(jwtFor("user-a", "org-1"));
        const error = await failureOf(client[method]("/api/v1/unavailable"));
        expect(error.message).toBe("The server did not answer (502)");
        expect(error.status).toBe(502);
        expect(isTransientFailure(error)).toBe(true);
      } finally {
        fetch.mockRestore();
      }
    },
  );

  it("reports an answer cut off while it is read as a network failure", async () => {
    server.use(
      http.get("*/api/v1/cut-off", () => {
        const body = new ReadableStream({
          start(controller) {
            controller.enqueue(new TextEncoder().encode('{"data":'));
            controller.error(new TypeError("terminated"));
          },
        });
        return new HttpResponse(body, {
          headers: { "Content-Type": "application/json" },
        });
      }),
    );
    const { client } = clientWith(jwtFor("user-a", "org-1"));
    const error = await failureOf(client.get("/api/v1/cut-off"));
    expect(error.code).toBe("NETWORK_ERROR");
    expect(error.status).toBe(0);
  });

  it("reports an answer that is not JSON as the server's answer, not a network failure", async () => {
    server.use(
      http.get("*/api/v1/not-json", () =>
        HttpResponse.text("<html>proxy</html>", {
          headers: { "Content-Type": "application/json" },
        }),
      ),
    );
    const { client } = clientWith(jwtFor("user-a", "org-1"));
    const error = await failureOf(client.get("/api/v1/not-json"));
    expect(error.code).toBe("INVALID_RESPONSE");
    expect(error.status).toBe(200);
  });

  it("reports a token refresh with no answer as a network failure, keeping the session", async () => {
    server.use(
      http.get("*/api/v1/expired", () =>
        HttpResponse.json({ error: { code: "UNAUTHORIZED" } }, { status: 401 }),
      ),
      http.post("*/api/v1/auth/refresh", () => HttpResponse.error()),
    );
    const { client, onAuthFailure } = clientWith(jwtFor("user-a", "org-1"));
    const error = await failureOf(client.get("/api/v1/expired"));
    expect(error.code).toBe("NETWORK_ERROR");
    expect(error.status).toBe(0);
    expect(onAuthFailure).not.toHaveBeenCalled();
  });

  it.each([408, 429, 503])(
    "keeps the session when the token refresh is answered %i, for plain and raw requests",
    async (status) => {
      server.use(
        http.get("*/api/v1/expired", () =>
          HttpResponse.json(
            { error: { code: "UNAUTHORIZED" } },
            { status: 401 },
          ),
        ),
        http.post("*/api/v1/auth/refresh", () =>
          HttpResponse.json({ error: { code: "BUSY" } }, { status }),
        ),
      );
      const { client, onAuthFailure } = clientWith(jwtFor("user-a", "org-1"));
      const plain = await failureOf(client.get("/api/v1/expired"));
      expect(plain.status).toBe(status);
      const raw = await failureOf(client.getRawResponse("/api/v1/expired"));
      expect(raw.status).toBe(status);
      expect(onAuthFailure).not.toHaveBeenCalled();
    },
  );

  it("signs out when the token refresh is refused", async () => {
    server.use(
      http.get("*/api/v1/expired", () =>
        HttpResponse.json({ error: { code: "UNAUTHORIZED" } }, { status: 401 }),
      ),
      http.post("*/api/v1/auth/refresh", () =>
        HttpResponse.json({ error: { code: "UNAUTHORIZED" } }, { status: 401 }),
      ),
    );
    const { client, onAuthFailure } = clientWith(jwtFor("user-a", "org-1"));
    const error = await failureOf(client.get("/api/v1/expired"));
    expect(error.status).toBe(401);
    expect(onAuthFailure).toHaveBeenCalledTimes(1);
  });

  it("never sends a request again after a refresh brings another principal's token", async () => {
    const posted: unknown[] = [];
    server.use(
      http.post("*/api/v1/replies", async ({ request }) => {
        posted.push(await request.json());
        return HttpResponse.json(
          { error: { code: "UNAUTHORIZED" } },
          { status: 401 },
        );
      }),
      // Another tab signed in as B: the shared cookie now mints B's token.
      http.post("*/api/v1/auth/refresh", () =>
        HttpResponse.json({ data: { jwt: jwtFor("user-b", "org-1") } }),
      ),
    );
    const { client, token } = clientWith(jwtFor("user-a", "org-1"));
    const error = await failureOf(
      client.post("/api/v1/replies", { content: "A's words" }),
    );
    expect(error.code).toBe("PRINCIPAL_CHANGED");
    expect(posted).toHaveLength(1);
    expect(tokenPrincipal(token())).toBe("user-b:org-1");
  });

  it("sends a request again after a refresh for the same principal", async () => {
    let calls = 0;
    server.use(
      http.post("*/api/v1/replies", () => {
        calls += 1;
        return calls === 1
          ? HttpResponse.json(
              { error: { code: "UNAUTHORIZED" } },
              { status: 401 },
            )
          : HttpResponse.json({ data: { id: "reply-1" } }, { status: 201 });
      }),
      http.post("*/api/v1/auth/refresh", () =>
        HttpResponse.json({
          data: { jwt: jwtFor("user-a", "org-1", "rotated") },
        }),
      ),
    );
    const { client } = clientWith(jwtFor("user-a", "org-1"));
    await expect(
      client.post("/api/v1/replies", { content: "A's words" }),
    ).resolves.toEqual({ id: "reply-1" });
    expect(calls).toBe(2);
  });

  it("says a request made for a principal was not sent because nobody is signed in", async () => {
    let calls = 0;
    server.use(
      http.post("*/api/v1/comments", () => {
        calls += 1;
        return HttpResponse.json({ data: { id: "c-1" } }, { status: 201 });
      }),
    );
    const { client, install } = clientWith(jwtFor("user-a", "org-1"));
    install(null);
    const refused = await failureOf(
      client.post(
        "/api/v1/comments",
        { content: "A's words" },
        { principal: "user-a:org-1" },
      ),
    );
    expect(refused.code).toBe("PRINCIPAL_CHANGED");
    expect(refused.message).toBe("You are signed out, so this was not sent.");
    expect(calls).toBe(0);
  });

  it("does not send a request made for one principal under another's token", async () => {
    let calls = 0;
    server.use(
      http.post("*/api/v1/comments", () => {
        calls += 1;
        return HttpResponse.json({ data: { id: "c-1" } }, { status: 201 });
      }),
    );
    const { client } = clientWith(jwtFor("user-b", "org-1"));
    const refused = await failureOf(
      client.post(
        "/api/v1/comments",
        { content: "A's words" },
        {
          principal: "user-a:org-1",
        },
      ),
    );
    expect(refused.code).toBe("PRINCIPAL_CHANGED");
    expect(calls).toBe(0);
    await expect(
      client.post(
        "/api/v1/comments",
        { content: "B's words" },
        {
          principal: "user-b:org-1",
        },
      ),
    ).resolves.toEqual({ id: "c-1" });
    expect(calls).toBe(1);
  });
});

describe("a token refresh answered after another token is in use", () => {
  const cases = [
    {
      installed: "another principal's token",
      next: jwtFor("user-b", "org-2"),
      sentAgain: false,
    },
    { installed: "a sign-out", next: null, sentAgain: false },
    {
      installed: "the same principal's renewed token",
      next: jwtFor("user-a", "org-1", "renewed"),
      sentAgain: true,
    },
  ] as const;

  it.each(
    cases.flatMap((each) => [
      { ...each, answer: "a token", refused: false },
      { ...each, answer: "a refusal", refused: true },
    ]),
  )(
    "changes nothing when answered with $answer after $installed",
    async ({ next, sentAgain, refused }) => {
      localStorage.removeItem("g5_auth_token");
      const sentWith: string[] = [];
      let askRefresh!: () => void;
      const refreshAsked = new Promise<void>((resolve) => {
        askRefresh = resolve;
      });
      let answerRefresh!: () => void;
      const refreshHeld = new Promise<void>((resolve) => {
        answerRefresh = resolve;
      });
      server.use(
        http.post("*/api/v1/replies", ({ request }) => {
          sentWith.push(request.headers.get("authorization") ?? "");
          return sentWith.length === 1
            ? HttpResponse.json(
                { error: { code: "UNAUTHORIZED" } },
                { status: 401 },
              )
            : HttpResponse.json({ data: { id: "reply-1" } }, { status: 201 });
        }),
        // The refresh went out for the token first in use, and is answered
        // after another was installed.
        http.post("*/api/v1/auth/refresh", async () => {
          askRefresh();
          await refreshHeld;
          return refused
            ? HttpResponse.json(
                { error: { code: "UNAUTHORIZED" } },
                { status: 401 },
              )
            : HttpResponse.json({
                data: { jwt: jwtFor("user-a", "org-1", "late") },
              });
        }),
      );
      const { client, onAuthFailure, token, install } = clientWith(
        jwtFor("user-a", "org-1"),
      );
      const sent = client
        .post("/api/v1/replies", { content: "A's words" })
        .then(
          (value) => ({ value }),
          (error: unknown) => ({ error }),
        );
      await refreshAsked;
      install(next);
      answerRefresh();
      const outcome = await sent;

      expect(token()).toBe(next);
      expect(localStorage.getItem("g5_auth_token")).toBeNull();
      expect(onAuthFailure).not.toHaveBeenCalled();
      if (sentAgain) {
        expect(outcome).toEqual({ value: { id: "reply-1" } });
        expect(sentWith).toEqual([
          `Bearer ${jwtFor("user-a", "org-1")}`,
          `Bearer ${next}`,
        ]);
      } else {
        expect(sentWith).toHaveLength(1);
        const error = (outcome as { error: unknown }).error;
        expect(error).toBeInstanceOf(ApiError);
        expect((error as ApiError).code).toBe("PRINCIPAL_CHANGED");
        expect((error as ApiError).message).toBe(
          next
            ? "You are now signed in as someone else, so this was not sent again."
            : "You are signed out, so this was not sent again.",
        );
      }
    },
  );

  it("changes nothing for a raw request's refresh answered after another principal's token", async () => {
    localStorage.removeItem("g5_auth_token");
    let calls = 0;
    let askRefresh!: () => void;
    const refreshAsked = new Promise<void>((resolve) => {
      askRefresh = resolve;
    });
    let answerRefresh!: () => void;
    const refreshHeld = new Promise<void>((resolve) => {
      answerRefresh = resolve;
    });
    server.use(
      http.get("*/api/v1/raw", () => {
        calls += 1;
        return HttpResponse.json(
          { error: { code: "UNAUTHORIZED" } },
          { status: 401 },
        );
      }),
      http.post("*/api/v1/auth/refresh", async () => {
        askRefresh();
        await refreshHeld;
        return HttpResponse.json({
          data: { jwt: jwtFor("user-a", "org-1", "late") },
        });
      }),
    );
    const { client, token, install } = clientWith(jwtFor("user-a", "org-1"));
    const sent = failureOf(client.getRawResponse("/api/v1/raw"));
    await refreshAsked;
    install(jwtFor("user-b", "org-2"));
    answerRefresh();
    expect((await sent).code).toBe("PRINCIPAL_CHANGED");
    expect(token()).toBe(jwtFor("user-b", "org-2"));
    expect(calls).toBe(1);
  });
});

describe("a request answered 401 after another token is in use", () => {
  it.each([
    {
      installed: "another principal's token",
      next: jwtFor("user-b", "org-2"),
      sentAgain: false,
    },
    { installed: "a sign-out", next: null, sentAgain: false },
    {
      installed: "the same principal's renewed token",
      next: jwtFor("user-a", "org-1", "renewed"),
      sentAgain: true,
    },
  ] as const)(
    "starts no refresh, and is sent again only for the same principal, after $installed",
    async ({ next, sentAgain }) => {
      localStorage.removeItem("g5_auth_token");
      const sentWith: string[] = [];
      let refreshes = 0;
      let answerFirst!: () => void;
      const firstHeld = new Promise<void>((resolve) => {
        answerFirst = resolve;
      });
      let askedFirst!: () => void;
      const firstAsked = new Promise<void>((resolve) => {
        askedFirst = resolve;
      });
      server.use(
        http.post("*/api/v1/replies", async ({ request }) => {
          sentWith.push(request.headers.get("authorization") ?? "");
          if (sentWith.length === 1) {
            // The first answer arrives after the token changed.
            askedFirst();
            await firstHeld;
            return HttpResponse.json(
              { error: { code: "UNAUTHORIZED" } },
              { status: 401 },
            );
          }
          return HttpResponse.json(
            { data: { id: "reply-1" } },
            { status: 201 },
          );
        }),
        // A refresh would sign this tab back in through a cookie the server
        // did not revoke.
        http.post("*/api/v1/auth/refresh", () => {
          refreshes += 1;
          return HttpResponse.json({
            data: { jwt: jwtFor("user-a", "org-1", "restored") },
          });
        }),
      );
      const { client, onAuthFailure, token, install } = clientWith(
        jwtFor("user-a", "org-1"),
      );
      const sent = client
        .post("/api/v1/replies", { content: "A's words" })
        .then(
          (value) => ({ value }),
          (error: unknown) => ({ error }),
        );
      await firstAsked;
      install(next);
      answerFirst();
      const outcome = await sent;

      expect(refreshes).toBe(0);
      expect(token()).toBe(next);
      expect(localStorage.getItem("g5_auth_token")).toBeNull();
      expect(onAuthFailure).not.toHaveBeenCalled();
      if (sentAgain) {
        expect(outcome).toEqual({ value: { id: "reply-1" } });
        expect(sentWith[1]).toBe(`Bearer ${next}`);
      } else {
        expect(sentWith).toHaveLength(1);
        expect(((outcome as { error: unknown }).error as ApiError).code).toBe(
          "PRINCIPAL_CHANGED",
        );
      }
    },
  );

  it("starts no refresh for a raw request answered 401 after a sign-out", async () => {
    let refreshes = 0;
    let calls = 0;
    let answerFirst!: () => void;
    const firstHeld = new Promise<void>((resolve) => {
      answerFirst = resolve;
    });
    let askedFirst!: () => void;
    const firstAsked = new Promise<void>((resolve) => {
      askedFirst = resolve;
    });
    server.use(
      http.get("*/api/v1/raw", async () => {
        calls += 1;
        askedFirst();
        await firstHeld;
        return HttpResponse.json(
          { error: { code: "UNAUTHORIZED" } },
          { status: 401 },
        );
      }),
      http.post("*/api/v1/auth/refresh", () => {
        refreshes += 1;
        return HttpResponse.json({
          data: { jwt: jwtFor("user-a", "org-1", "restored") },
        });
      }),
    );
    const { client, token, install } = clientWith(jwtFor("user-a", "org-1"));
    const sent = failureOf(client.getRawResponse("/api/v1/raw"));
    await firstAsked;
    install(null);
    answerFirst();
    expect((await sent).code).toBe("PRINCIPAL_CHANGED");
    expect(refreshes).toBe(0);
    expect(token()).toBeNull();
    expect(calls).toBe(1);
  });
});
