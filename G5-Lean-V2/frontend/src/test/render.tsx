// =============================================================================
// Test Render Helper — renderApp() with All Required Providers
// =============================================================================
// Wraps React components in all required providers for integration testing:
// QueryClientProvider → MemoryRouter → AuthProvider → MockWebSocketProvider → {ui}
//
// Returns the standard @testing-library/react RenderResult plus additional
// test utilities (ws controller, queryClient).
// =============================================================================

import React from "react";
import { render } from "@testing-library/react";
import type { RenderResult } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, parsePath } from "react-router-dom";
import { AuthProvider } from "../hooks/useAuth";
import { MockWebSocketProvider, MockWsController } from "./msw/ws-mock";
import type { OrgKind } from "../lib/types";

// ---------------------------------------------------------------------------
// MockUser Interface
// ---------------------------------------------------------------------------

export interface MockUser {
  id: string;
  email: string;
  displayName: string;
  avatarUrl: string | null;
  orgs: Array<{
    orgId: string;
    orgName: string;
    orgSlug: string;
    role: "admin" | "member" | "viewer";
    /** Defaults to `tenant` in the mocked `/users/me` response. */
    orgKind?: OrgKind;
  }>;
  scopes: string[];
}

// ---------------------------------------------------------------------------
// RenderApp Options & Result
// ---------------------------------------------------------------------------

export interface RenderAppOptions {
  /** Initial route for MemoryRouter (default: '/') */
  route?: string;
  routeState?: unknown;
  /** Mock user to inject into AuthProvider (default: mockCurrentUser from fixtures) */
  user?: MockUser | null;
  /** Pre-configured MockWsController (default: new instance created) */
  ws?: MockWsController;
  /** Pre-configured QueryClient (default: new test-configured instance) */
  queryClient?: QueryClient;
}

export interface RenderAppResult extends RenderResult {
  /** MockWsController for pushing WebSocket events */
  ws: MockWsController;
  /** QueryClient for cache inspection/manipulation */
  queryClient: QueryClient;
}

const TEST_CURRENT_USER_KEY = "g5_test_current_user";
const TEST_DISABLE_AUTH_KEY = "g5_test_disable_auth";

/**
 * An unsigned test JWT for `user`, scoped to `orgId` (default: their first
 * organization). Tests that answer `/auth/refresh` for an organization
 * switch build the switched token with the same shape.
 */
export function createTestJwt(user: MockUser, orgId?: string): string {
  const now = Math.floor(Date.now() / 1000);
  const header = btoa(JSON.stringify({ alg: "HS256", typ: "JWT" }));
  const payload = btoa(
    JSON.stringify({
      sub: user.id,
      email: user.email,
      org: orgId ?? user.orgs[0]?.orgId ?? "org-test",
      scopes: user.scopes,
      iat: now,
      exp: now + 3600,
    }),
  );

  return `${header}.${payload}.test-signature`;
}

// ---------------------------------------------------------------------------
// Test QueryClient Factory
// ---------------------------------------------------------------------------

/**
 * Creates a QueryClient configured for deterministic testing:
 * - No retries (failed queries fail immediately)
 * - Infinite staleTime (no auto-refetch)
 * - Infinite gcTime (no garbage collection during test)
 * - Suppressed error logging (prevents noisy console output)
 *
 * The client is tracked in `_activeTestClients` so the global `afterEach`
 * can cancel in-flight queries before jsdom tears down the DOM.
 */

/** Active QueryClients created during the current test. */
export const _activeTestClients: QueryClient[] = [];

function createTestQueryClient(): QueryClient {
  const qc = new QueryClient({
    defaultOptions: {
      queries: {
        retry: false,
        staleTime: Infinity,
        gcTime: Infinity,
      },
      mutations: {
        retry: false,
      },
    },
  });
  _activeTestClients.push(qc);
  return qc;
}

// ---------------------------------------------------------------------------
// renderApp Function
// ---------------------------------------------------------------------------

/**
 * Wraps a React component in all required providers for integration testing.
 *
 * Provider wrapping order (outermost to innermost):
 * 1. QueryClientProvider — with test-configured QueryClient
 * 2. MemoryRouter — with initialEntries set to [route]
 * 3. AuthProvider — with mock user injected
 * 4. MockWebSocketProvider — with the MockWsController instance
 * 5. The actual ui component
 *
 * @param ui - React element to render
 * @param options - Configuration options
 * @returns RenderResult extended with ws controller and queryClient
 */
export function renderApp(
  ui: React.ReactElement,
  options: RenderAppOptions = {},
): RenderAppResult {
  const {
    route = "/",
    routeState,
    user,
    ws: wsController = new MockWsController(),
    queryClient = createTestQueryClient(),
  } = options;

  if (typeof globalThis.localStorage !== "undefined") {
    globalThis.localStorage.removeItem(TEST_CURRENT_USER_KEY);
    globalThis.localStorage.removeItem(TEST_DISABLE_AUTH_KEY);

    if (user === null) {
      globalThis.localStorage.removeItem("g5_auth_token");
      globalThis.localStorage.setItem(TEST_DISABLE_AUTH_KEY, "1");
    } else if (user) {
      globalThis.localStorage.setItem("g5_auth_token", createTestJwt(user));
      globalThis.localStorage.setItem(
        TEST_CURRENT_USER_KEY,
        JSON.stringify(user),
      );
    }
  }

  // Wire the queryClient to the mock controller so that ws.emit() can
  // mirror the real WebSocketManager's cache invalidation/update logic.
  wsController.queryClient = queryClient;

  const renderResult = render(
    <QueryClientProvider client={queryClient}>
      <MemoryRouter
        initialEntries={[{ ...parsePath(route), state: routeState }]}
      >
        <AuthProvider>
          <MockWebSocketProvider controller={wsController}>
            {ui}
          </MockWebSocketProvider>
        </AuthProvider>
      </MemoryRouter>
    </QueryClientProvider>,
  );

  return {
    ...renderResult,
    ws: wsController,
    queryClient,
  };
}

// ---------------------------------------------------------------------------
// renderWithWs Convenience Function
// ---------------------------------------------------------------------------

/**
 * Convenience wrapper that creates a fresh MockWsController and passes it
 * through, returning it in the result for event simulation.
 *
 * @param ui - React element to render
 * @param options - Configuration options (ws is auto-created)
 * @returns RenderResult extended with ws controller and queryClient
 */
export function renderWithWs(
  ui: React.ReactElement,
  options?: Omit<RenderAppOptions, "ws">,
): RenderAppResult {
  const controller = new MockWsController();
  return renderApp(ui, { ...options, ws: controller });
}
