// =============================================================================
// Fetch Interceptor — Mock Mode Without Service Workers
// =============================================================================
// Monkey-patches window.fetch to route /api/* requests through MSW handlers
// inline. This avoids the need for a Service Worker, which doesn't work in
// VS Code Simple Browser or many dev container setups.
//
// Non-API requests (Vite HMR, module loads, etc.) pass through to real fetch.
// Mock mode answers the organization Home's reads from `home-preview` ahead
// of the unit-test fixture, so the alignment Home shows populated tiles.
// =============================================================================

import { http, HttpResponse } from "msw";
import { EXTERNAL_ISSUES_ENABLED_SETTING } from "../lib/externalIssuesFlags";
import { handlers } from "../test/msw/handlers";
import { MOCK_CHAT_MESSAGES } from "../test/fixtures";
import { homePreviewHandlers } from "./home-preview";

/**
 * Routes the composer needs before it can send, which the unit-test fixture
 * leaves to each test: the launch resolution (echoing the configured
 * defaults) and the provider quota. Without them mock mode's Send stays
 * disabled with "The server did not answer (404)".
 */
const composerLaunchHandlers = [
  http.post("/api/v1/workspaces/:id/config/agent-launch/resolve", () =>
    HttpResponse.json({
      data: {
        resolved: {
          schemaVersion: 1,
          resolutionId: "mock-resolved",
          configRevision: "mock",
          catalogVersion: "mock",
          provider: { value: "claude_code", provenance: "configured" },
          providerRoute: "anthropic_direct",
          model: { value: "claude-opus-4-8", provenance: "configured" },
          upstreamModel: "claude-opus-4-8",
          permissionMode: { value: "auto", provenance: "configured" },
          effort: { value: "high", provenance: "configured" },
          authMode: "anthropic_api_key",
          egressMode: "direct_provider",
          credentialReady: true,
          requiresInteractiveAuth: false,
        },
      },
    }),
  ),
  http.get("/api/v1/workspaces/:id/provider-quota", () =>
    HttpResponse.json({ data: null }),
  ),
];

// We parse the handler definitions to build a simple router
interface MockRoute {
  method: string;
  pattern: RegExp;
  paramNames: string[];
  /** Resolves to null when the handler passes, as an MSW resolver may. */
  handler: (
    params: Record<string, string>,
    req: Request,
  ) => Response | null | Promise<Response | null>;
}

/**
 * Convert an MSW path pattern like '/api/v1/workspaces/:id/tasks'
 * into a regex with named capture groups.
 */
function pathToRegex(path: string): { pattern: RegExp; paramNames: string[] } {
  const paramNames: string[] = [];
  // First replace :param patterns, then handle * wildcard
  let regexStr = path.replace(/:([^/]+)/g, (_match, paramName) => {
    paramNames.push(paramName);
    return "([^/]+)";
  });
  // Replace trailing wildcard * with a catch-all group
  regexStr = regexStr.replace(/\/\*$/, "/(.+)");
  // Also handle standalone * (MSW wildcard)
  regexStr = regexStr.replace(/\*/, "(.*)");
  return { pattern: new RegExp(`^${regexStr}$`), paramNames };
}

/**
 * Extract route info from MSW handlers.
 * MSW RequestHandler has info.method, info.path, and a resolver function.
 */
function buildRoutes(): MockRoute[] {
  const routes: MockRoute[] = [];

  for (const h of [
    ...homePreviewHandlers,
    ...composerLaunchHandlers,
    ...handlers,
  ]) {
    const info = (h as any).info;
    if (!info?.path || !info?.method) continue;

    const method = String(info.method).toUpperCase();
    const path = String(info.path);
    const { pattern, paramNames } = pathToRegex(path);

    // The handler's resolver is the second arg to http.get/post/etc
    // We can call the full handler via MSW's internal API, but it's simpler
    // to just invoke the resolver directly. MSW stores it internally.
    const resolver = (h as any).resolver;

    if (!resolver) continue;

    routes.push({
      method,
      pattern,
      paramNames,
      handler: async (params: Record<string, string>, req: Request) => {
        try {
          // Call the MSW resolver with a simplified context
          const result = await resolver({
            request: req,
            params,
            cookies: {},
          });
          // MSW resolvers return Response objects directly
          if (result instanceof Response) {
            return result;
          }
          // Nothing returned: the handler passes, and the next route that
          // matches answers (the fixture's re-verification clone answers
          // only a run's start).
          if (result === undefined) return null;
          // Fallback — shouldn't happen with well-formed handlers
          return new Response(JSON.stringify(result), {
            status: 200,
            headers: { "Content-Type": "application/json" },
          });
        } catch (err) {
          console.error("[MockFetch] Handler error:", path, err);
          return new Response(JSON.stringify({ error: "Mock handler error" }), {
            status: 500,
            headers: { "Content-Type": "application/json" },
          });
        }
      },
    });
  }

  return routes;
}

/**
 * Instance features the dev stack advertises by default
 * (`infra/docker/docker-compose.dev.yml`: `G5_GUIDED_JOURNEY_ENABLED` and
 * `G5_ENABLE_PROVIDER_ACCOUNTS`, both `true`; QA sets the same). The MSW
 * fixture advertises only `chat` and `codex_ui` so unit tests opt into each
 * gated section explicitly; mock mode is a developer's stand-in for that
 * stack and must show the same UI (QA, 2026-10-13).
 */
export const DEV_DEFAULT_FEATURES: readonly string[] = [
  "guided_journey",
  "provider_accounts",
];

/**
 * Features mock mode adds beyond the dev defaults: `external_issues`, which a
 * stack advertises once its GitHub App is configured, so the Home Issues and
 * tasks tile renders against the `home-preview` issues.
 */
export const MOCK_DEMO_FEATURES: readonly string[] = ["external_issues"];

/**
 * The feature list mock mode advertises: the fixture's features plus the dev
 * defaults and the demo features, or plus `VITE_MOCK_FEATURES`
 * (comma-separated, may be empty to see the fixture alone) when that is set.
 */
export function mockInstanceFeatures(
  fixtureFeatures: unknown,
  override: string | undefined,
): string[] {
  const base = Array.isArray(fixtureFeatures)
    ? fixtureFeatures.filter((f): f is string => typeof f === "string")
    : [];
  const extra =
    override === undefined
      ? [...DEV_DEFAULT_FEATURES, ...MOCK_DEMO_FEATURES]
      : override
          .split(",")
          .map((f) => f.trim())
          .filter(Boolean);
  return Array.from(new Set([...base, ...extra]));
}

const INSTANCE_INFO_PATH = "/api/v1/instance/info";
const ORG_CONFIG_PATH = /^\/api\/v1\/orgs\/[^/]+\/config$/;

/** Rewrite a JSON `{ data }` response's payload; other responses pass. */
async function rewriteData(
  response: Response,
  rewrite: (data: Record<string, unknown>) => void,
): Promise<Response> {
  if (!response.ok) return response;
  const body = (await response
    .clone()
    .json()
    .catch(() => null)) as { data?: unknown } | null;
  if (!body?.data || typeof body.data !== "object") return response;
  rewrite(body.data as Record<string, unknown>);
  return new Response(JSON.stringify(body), {
    status: response.status,
    headers: { "Content-Type": "application/json" },
  });
}

function withMockInstanceInfo(response: Response): Promise<Response> {
  return rewriteData(response, (data) => {
    data.features = mockInstanceFeatures(
      data.features,
      import.meta.env.VITE_MOCK_FEATURES,
    );
  });
}

/**
 * The organization's external-issues switch reads as on unless someone set
 * it in this session, so the Issues and tasks tile shows; turning it off in
 * Runtime Config still hides the tile. The verification switch keeps its
 * real default, off, until someone turns it on in Admin.
 */
function withExternalIssuesOn(response: Response): Promise<Response> {
  return rewriteData(response, (data) => {
    for (const key of ["settings", "effectiveSettings"]) {
      const settings = data[key] as Record<string, unknown> | undefined;
      if (settings && !(EXTERNAL_ISSUES_ENABLED_SETTING in settings)) {
        settings[EXTERNAL_ISSUES_ENABLED_SETTING] = true;
      }
    }
  });
}

/** Mock-mode adjustments to the fixture's answers, by request. */
function adjustResponse(
  method: string,
  pathname: string,
  response: Response,
): Promise<Response> | Response {
  if (method !== "GET") return response;
  if (pathname === INSTANCE_INFO_PATH) return withMockInstanceInfo(response);
  if (ORG_CONFIG_PATH.test(pathname)) return withExternalIssuesOn(response);
  return response;
}

/**
 * Install the fetch interceptor. Replaces window.fetch with a version
 * that checks /api/* requests against mock routes, falling through to
 * the real fetch for everything else.
 */
/**
 * The VM state mock mode shows (`?mockVm=stopped|failed|starting|running`
 * on the first page URL, remembered for the tab): lets a reviewer or a
 * screenshot run see the stopped-workspace surfaces (chat
 * g5.chat.start-and-send). Start in the UI moves it to `starting`.
 */
const MOCK_VM_KEY = "g5:mock-vm-status";
function mockVmStatus(): string | null {
  // The URL seeds the state once per tab; Start then moves it on, and a
  // later poll must not reset it from the still-present query parameter.
  const stored = window.sessionStorage.getItem(MOCK_VM_KEY);
  if (stored) return stored;
  const fromUrl = new URLSearchParams(window.location.search).get("mockVm");
  if (fromUrl) window.sessionStorage.setItem(MOCK_VM_KEY, fromUrl);
  return fromUrl;
}
/** The session's last assistant message in the fixture, which the mock question hangs off. */
function mockLastAssistantMessageId(sessionId: string): string {
  const messages =
    MOCK_CHAT_MESSAGES[sessionId] ?? Object.values(MOCK_CHAT_MESSAGES)[0] ?? [];
  const assistant = [...messages].reverse().find((m) => m.role === "assistant");
  return assistant?.id ?? "test-msg-1";
}
function mockFlag(name: string): boolean {
  const key = `g5:mock-${name}`;
  const fromUrl = new URLSearchParams(window.location.search).get(name);
  if (fromUrl) window.sessionStorage.setItem(key, fromUrl);
  return window.sessionStorage.getItem(key) === "1";
}
function withMockVmStatus(
  method: string,
  pathname: string,
  response: Response,
  requestBody: string,
): Promise<Response> | Response {
  const status = mockVmStatus();
  if (!status) return response;
  if (
    method === "POST" &&
    /\/api\/v1\/workspaces\/[^/]+\/resume$/.test(pathname)
  ) {
    window.sessionStorage.setItem(MOCK_VM_KEY, "starting");
  }
  if (
    method === "POST" &&
    response.ok &&
    /"startCompute"\s*:\s*true/.test(requestBody) &&
    (/\/api\/v1\/workspaces\/[^/]+\/sessions\/start$/.test(pathname) ||
      /\/api\/v1\/sessions\/[^/]+\/messages$/.test(pathname) ||
      /\/api\/v1\/tasks\/[^/]+\/provider-requests\/[^/]+\/reply$/.test(
        pathname,
      ))
  ) {
    // A compound Start recorded by a send or an answer that carried the
    // flag and was accepted: the next VM poll shows the Start under way.
    if (status === "stopped" || status === "failed") {
      window.sessionStorage.setItem(MOCK_VM_KEY, "starting");
    }
  }
  // `?mockQuestion=1` adds a pending agent question to the open chat, so the
  // held question's Start and answer can be seen without an agent.
  const questionMatch = pathname.match(
    /^\/api\/v1\/sessions\/([^/]+)\/timeline$/,
  );
  if (method === "GET" && questionMatch && mockFlag("mockQuestion")) {
    return response.json().then((body) => {
      const entries = body?.data?.entries;
      if (Array.isArray(entries)) {
        entries.push({
          id: "mock-question",
          entryType: "checkpoint",
          content: "provider_request.opened",
          sequence: entries.length + 1,
          createdAt: new Date().toISOString(),
          metadata: {
            schema_version: "g5.task-event.v1",
            kind: "provider_request.opened",
            taskId: "task-mock-question",
            attempt: 1,
            sessionId: questionMatch[1],
            messageId: mockLastAssistantMessageId(questionMatch[1]),
            nativeRequestStatus: "pending",
            payload: {
              provider_request_id: "toolu-mock-question",
              request_kind: "question",
              tool_name: "AskUserQuestion",
              deadline_unix_ms: null,
              resume_session_id: "native-session",
              display: {
                questions: [
                  {
                    question: "Which retry strategy should the job use?",
                    options: [
                      { label: "Exponential backoff" },
                      { label: "Fixed delay" },
                    ],
                  },
                ],
              },
            },
          },
        });
      }
      return new Response(JSON.stringify(body), {
        status: response.status,
        headers: { "Content-Type": "application/json" },
      });
    });
  }
  if (method !== "GET" || !/\/api\/v1\/workspaces\/[^/]+\/vm$/.test(pathname)) {
    return response;
  }
  return response.json().then((body) => {
    const data = body?.data;
    if (data && typeof data === "object") {
      data.status = status;
      if (status !== "running") {
        data.vmId = null;
        data.hostId = null;
        data.startedAt = null;
      }
    }
    return new Response(JSON.stringify(body), {
      status: response.status,
      headers: { "Content-Type": "application/json" },
    });
  });
}

export function installFetchInterceptor(): void {
  const routes = buildRoutes();
  const realFetch = window.fetch.bind(window);

  window.fetch = async (
    input: RequestInfo | URL,
    init?: RequestInit,
  ): Promise<Response> => {
    const req = input instanceof Request ? input : new Request(input, init);
    const url = new URL(req.url, window.location.origin);
    const pathname = url.pathname;
    const method = (init?.method ?? req.method ?? "GET").toUpperCase();

    // Only intercept /api/* requests
    if (!pathname.startsWith("/api/")) {
      return realFetch(input, init);
    }

    // Try to match against routes
    for (const route of routes) {
      if (route.method !== method) continue;
      const match = pathname.match(route.pattern);
      if (!match) continue;

      // Extract params
      const params: Record<string, string> = {};
      route.paramNames.forEach((name, i) => {
        params[name] = match[i + 1];
      });

      console.debug(`[MockFetch] ${method} ${pathname}`, params);
      const requestBody =
        method === "POST"
          ? await req
              .clone()
              .text()
              .catch(() => "")
          : "";
      const response = await route.handler(params, req);
      if (!response) continue;
      return withMockVmStatus(
        method,
        pathname,
        await adjustResponse(method, pathname, response),
        requestBody,
      );
    }

    // No route matched — log and return 404
    console.warn(`[MockFetch] No handler for ${method} ${pathname}`);
    return new Response(
      JSON.stringify({ error: "Not found", path: pathname }),
      {
        status: 404,
        headers: { "Content-Type": "application/json" },
      },
    );
  };

  console.log(
    `[MockFetch] Installed fetch interceptor with ${routes.length} routes`,
  );
}
