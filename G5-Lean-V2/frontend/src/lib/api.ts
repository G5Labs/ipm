// =============================================================================
// G5 REST API Client — Fetch-Based HTTP Client with JWT Auth, Token Refresh,
// and Error Handling
// =============================================================================
// This module is the single gateway for all REST API interactions between the
// G5 React frontend and the g5-api backend. It uses only the browser `fetch`
// API (no axios or other HTTP libraries) and has NO React dependency.
//
// Features:
// - JWT authentication with automatic token refresh on 401
// - Request queuing during token refresh (prevents concurrent refresh races)
// - Typed generic methods: get<T>, getRaw<T>, post<T>, patch<T>, put<T>, delete<T>
// - Response envelope parsing: { data: T, meta?: {...} }
// - Typed ApiError class with error code checking helpers
// - Request cancellation via AbortSignal
// - X-Request-ID correlation header on every request
// - Proactive token refresh (every 50 minutes)
// - Query parameter serialization with array support
// =============================================================================

import { rememberPostAuthRedirect } from "./postAuthRedirect";
import { randomUuid } from "./randomUuid";

// ---------------------------------------------------------------------------
// Configuration & Interfaces
// ---------------------------------------------------------------------------

/** Configuration for the API client factory */
export interface ApiClientConfig {
  /** Base URL for API requests (default: window.location.origin) */
  baseUrl?: string;
  /** Callback to get the current JWT access token */
  getToken: () => string | null;
  /** Callback to store a new JWT access token after refresh */
  setToken: (token: string) => void;
  /** Called when token refresh fails — should redirect to login */
  onAuthFailure: () => void;
}

/** Options for individual requests */
export interface RequestOptions {
  /** AbortSignal for request cancellation */
  signal?: AbortSignal;
  /** Additional headers to include */
  headers?: Record<string, string>;
  /** Query parameters to append to the URL */
  params?: Record<string, string | number | boolean | string[] | undefined>;
  /**
   * The principal (`tokenPrincipal`) the request is made for: it is not sent
   * under another principal's token (`g5.security.login.principal`).
   */
  principal?: string | null;
}

/** Full API response envelope (used by getRaw) */
export interface ApiResponse<T> {
  data: T;
  meta?: {
    page?: number;
    perPage?: number;
    total?: number;
    cursor?: string | null;
    hasMore?: boolean;
  };
}

/** The API client instance returned by the factory */
export interface ApiClient {
  /** GET request, extracts data from response envelope */
  get<T>(path: string, options?: RequestOptions): Promise<T>;

  /** GET request, returns full response envelope (for pagination meta) */
  getRaw<T>(path: string, options?: RequestOptions): Promise<ApiResponse<T>>;

  /** GET request returning the raw Response (for downloads/binary payloads) */
  getRawResponse(path: string, options?: RequestOptions): Promise<Response>;

  /** POST request with JSON body */
  post<T>(path: string, body?: unknown, options?: RequestOptions): Promise<T>;

  /** POST request returning the raw Response (for downloads/binary payloads) */
  postRaw(
    path: string,
    body?: unknown,
    options?: RequestOptions,
  ): Promise<Response>;

  /** PATCH request with JSON body */
  patch<T>(path: string, body?: unknown, options?: RequestOptions): Promise<T>;

  /** DELETE request */
  delete<T = void>(path: string, options?: RequestOptions): Promise<T>;

  /** PUT request with JSON body */
  put<T>(path: string, body?: unknown, options?: RequestOptions): Promise<T>;

  /** Start proactive token refresh interval (every 50 minutes).
   *  Returns a cleanup function to stop the interval. */
  startProactiveRefresh(): () => void;
}

// ---------------------------------------------------------------------------
// ApiError Class
// ---------------------------------------------------------------------------

/** Backend error code for a read-only-workspace mutation rejection (HTTP 403). */
export const WORKSPACE_READ_ONLY_CODE = "WORKSPACE_READ_ONLY";

/** Concise user-facing explanation shown when a read-only mutation is blocked. */
export const WORKSPACE_READ_ONLY_MESSAGE =
  "This workspace is read-only. Chat and app edits are disabled; admin settings and Git operations are still available.";

/** Typed API error class carrying structured error information from the backend */
export class ApiError extends Error {
  /** Machine-readable error code from the backend error catalog */
  readonly code: string;
  /** HTTP status code */
  readonly status: number;
  /** Additional error details (e.g., validation errors by field) */
  readonly details: Record<string, unknown>;
  /** Request ID for correlation */
  readonly requestId: string;

  constructor(params: {
    code: string;
    message: string;
    status: number;
    details?: Record<string, unknown>;
    requestId?: string;
  }) {
    super(params.message);
    this.name = "ApiError";
    this.code = params.code;
    this.status = params.status;
    this.details = params.details ?? {};
    this.requestId = params.requestId ?? "";

    // Maintain proper prototype chain for instanceof checks
    Object.setPrototypeOf(this, ApiError.prototype);
  }

  /** Check if this error has a specific code */
  is(code: string): boolean {
    return this.code === code;
  }

  /** Check if this is a network error (no response received) */
  get isNetworkError(): boolean {
    return this.code === "NETWORK_ERROR";
  }

  /** Check if this is an authentication error */
  get isAuthError(): boolean {
    return this.code === "UNAUTHORIZED" || this.status === 401;
  }

  /**
   * Check if this is a read-only-workspace rejection (`WORKSPACE_READ_ONLY`,
   * HTTP 403). Used to surface a consistent read-only toast even when a control
   * was not pre-disabled (stale tab, direct call, race).
   */
  get isReadOnly(): boolean {
    return this.code === WORKSPACE_READ_ONLY_CODE;
  }

  /** Check if this is a client error (4xx) */
  get isClientError(): boolean {
    return this.status >= 400 && this.status < 500;
  }

  /** Check if this is a server error (5xx) */
  get isServerError(): boolean {
    return this.status >= 500 && this.status < 600;
  }
}

// ---------------------------------------------------------------------------
// URL Builder Utility
// ---------------------------------------------------------------------------

/**
 * Build a full URL from base, path, and optional query parameters.
 *
 * - Strips undefined values from params
 * - Serializes arrays as comma-separated values
 * - Encodes all values with encodeURIComponent
 * - Prevents double slashes between base and path
 */
export function buildUrl(
  base: string,
  path: string,
  params?: Record<string, string | number | boolean | string[] | undefined>,
): string {
  // Ensure no double slashes between base and path
  const normalizedBase = base.endsWith("/") ? base.slice(0, -1) : base;
  const normalizedPath = path.startsWith("/") ? path : `/${path}`;
  let url = `${normalizedBase}${normalizedPath}`;

  if (params) {
    const parts: string[] = [];
    for (const [key, value] of Object.entries(params)) {
      if (value === undefined) continue; // Strip undefined values
      if (Array.isArray(value)) {
        // Serialize arrays as comma-separated values
        if (value.length > 0) {
          parts.push(
            `${encodeURIComponent(key)}=${value.map((v) => encodeURIComponent(v)).join(",")}`,
          );
        }
      } else {
        parts.push(
          `${encodeURIComponent(key)}=${encodeURIComponent(String(value))}`,
        );
      }
    }
    const queryString = parts.join("&");
    if (queryString) {
      url += `?${queryString}`;
    }
  }

  return url;
}

// ---------------------------------------------------------------------------
// Factory Function
// ---------------------------------------------------------------------------

/**
 * The principal (person and org) a JWT speaks for, or null when it cannot be
 * read. Requests are bound to it: a refresh that brings another principal's
 * token never sends a request again under it.
 */
export function tokenPrincipal(token: string | null): string | null {
  if (!token) return null;
  try {
    const payload = token.split(".")[1];
    if (!payload) return null;
    const claims = JSON.parse(
      atob(payload.replace(/-/g, "+").replace(/_/g, "/")),
    ) as { sub?: unknown; org?: unknown };
    return typeof claims.sub === "string"
      ? `${claims.sub}:${typeof claims.org === "string" ? claims.org : ""}`
      : null;
  } catch {
    return null;
  }
}

/** Bind access recovery to the person and organization that lost access. */
export function notifyOrganizationAccessChanged(
  principal: string | null,
): void {
  if (principal) {
    window.dispatchEvent(
      new CustomEvent("g5:organization-access-changed", {
        detail: { principal },
      }),
    );
  }
}

function isAbortError(err: unknown): boolean {
  return err instanceof DOMException && err.name === "AbortError";
}

/**
 * No usable answer arrived: the connection failed before or while the
 * response came in. A retry may well succeed (status 0, `NETWORK_ERROR`).
 */
function networkError(err: unknown, requestId: string): ApiError {
  return new ApiError({
    code: "NETWORK_ERROR",
    message: err instanceof Error ? err.message : "Network request failed",
    status: 0,
    requestId,
  });
}

/**
 * Read a JSON answer. An answer cut off while it is read is a network
 * failure; one that is not JSON is the server's answer, and a retry of the
 * same request will not change it.
 */
async function readJson<T>(response: Response, requestId: string): Promise<T> {
  try {
    return (await response.json()) as T;
  } catch (err: unknown) {
    if (isAbortError(err)) throw err;
    if (err instanceof SyntaxError) {
      throw new ApiError({
        code: "INVALID_RESPONSE",
        message: "The server's answer could not be read",
        status: response.status,
        requestId,
      });
    }
    throw networkError(err, requestId);
  }
}

/**
 * Create a configured API client instance.
 *
 * The client uses only the browser `fetch` API and the shared UUID helper.
 * It has NO React dependency — it is a pure TypeScript module.
 */
export function createApiClient(config: ApiClientConfig): ApiClient {
  const baseUrl =
    config.baseUrl ??
    (typeof window !== "undefined" ? window.location.origin : "");
  const { getToken, setToken, onAuthFailure } = config;

  function isCompatibleAbortSignal(signal: AbortSignal): boolean {
    if (typeof Request === "undefined") {
      return false;
    }

    try {
      void new Request("http://localhost/", { signal });
      return true;
    } catch {
      return false;
    }
  }

  function normalizeAbortSignal(signal?: AbortSignal): AbortSignal | undefined {
    if (!signal) {
      return undefined;
    }

    if (isCompatibleAbortSignal(signal)) {
      return signal;
    }

    if (typeof AbortController === "undefined") {
      return undefined;
    }

    const controller = new AbortController();
    const externalSignal = signal as AbortSignal & { reason?: unknown };

    if (externalSignal.aborted) {
      controller.abort(externalSignal.reason);
      return isCompatibleAbortSignal(controller.signal)
        ? controller.signal
        : undefined;
    }

    if (typeof externalSignal.addEventListener === "function") {
      externalSignal.addEventListener(
        "abort",
        () => {
          controller.abort(externalSignal.reason);
        },
        { once: true },
      );
      return isCompatibleAbortSignal(controller.signal)
        ? controller.signal
        : undefined;
    }

    return undefined;
  }

  function isFormDataBody(body: unknown): body is FormData {
    return typeof FormData !== "undefined" && body instanceof FormData;
  }

  // -------------------------------------------------------------------------
  // Token Refresh Coordination
  // -------------------------------------------------------------------------

  // Module-level state for refresh coordination.
  // Only ONE refresh request is in flight at any time.
  let refreshPromise: Promise<void> | null = null;

  async function refreshToken(): Promise<void> {
    // If refresh already in progress, wait for it
    if (refreshPromise) {
      return refreshPromise;
    }

    // Another token installed while this refresh is out (an org switch, the
    // provider's own refresh, a sign-out) is newer than its answer, which
    // then changes nothing, a token, a refusal or a failure alike. The
    // caller sends again only for the principal now in use
    // (`g5.security.login.principal`).
    const installed = getToken();
    const superseded = () => getToken() !== installed;
    refreshPromise = (async () => {
      try {
        let response: Response;
        try {
          response = await fetch(`${baseUrl}/api/v1/auth/refresh`, {
            method: "POST",
            credentials: "include", // Send httpOnly cookie
          });
        } catch (err: unknown) {
          // No answer is not a refused session: report it as a network
          // failure and leave the session alone.
          if (isAbortError(err)) throw err;
          if (superseded()) return;
          throw networkError(err, "");
        }
        if (superseded()) return;

        if (response.ok) {
          const { data } = await readJson<{ data: { jwt: string } }>(
            response,
            "",
          );
          if (superseded()) return;
          setToken(data.jwt);
          // Persist to localStorage so other modules (e.g. live workspace file
          // fetchWsToken) can read the JWT without going through the
          // in-memory ApiClient singleton.
          if (typeof localStorage !== "undefined" && data.jwt) {
            localStorage.setItem("g5_auth_token", data.jwt);
          }
          return;
        }

        // A server that cannot answer now is not a refused session: report
        // the answer and leave the session alone (`g5.workspace.page.refresh`).
        if (
          response.status === 408 ||
          response.status === 429 ||
          response.status >= 500
        ) {
          throw new ApiError({
            code: "REFRESH_UNAVAILABLE",
            message: "The session could not be refreshed right now",
            status: response.status,
            requestId: "",
          });
        }

        // Server refused the refresh — trigger auth failure.
        // (Dev/E2E users re-authenticate via the dev login button.)
        onAuthFailure();
        throw new ApiError({
          code: "UNAUTHORIZED",
          message: "Token refresh failed",
          status: 401,
          requestId: "",
        });
      } finally {
        refreshPromise = null;
      }
    })();

    return refreshPromise;
  }

  /**
   * A refresh for a request answered 401, only while the token it was sent
   * with is still in use. A request answered after another token was
   * installed, or after a sign-out, starts none: it is sent again only for
   * the principal in use, with that principal's token, or not at all
   * (`g5.security.login.principal`).
   */
  async function refreshForAnswerTo(sentWith: string | null): Promise<void> {
    if (getToken() !== sentWith) return;
    await refreshToken();
  }

  function membershipDenialApplies(sentWith: string | null): boolean {
    const current = getToken();
    if (!current || !sentWith) return false;
    const principal = tokenPrincipal(sentWith);
    return (
      current === sentWith ||
      (principal !== null && tokenPrincipal(current) === principal)
    );
  }

  // -------------------------------------------------------------------------
  // Core Request Method
  // -------------------------------------------------------------------------

  async function request<T>(
    method: string,
    path: string,
    body?: unknown,
    options?: RequestOptions,
    _isRetry?: boolean, // prevents infinite refresh loops
  ): Promise<ApiResponse<T>> {
    const url = buildUrl(baseUrl, path, options?.params);
    const token = getToken();

    const requestId = randomUuid();
    refuseForAnotherPrincipal(options, token, requestId);

    const headers: Record<string, string> = {
      Accept: "application/json",
      "X-Request-ID": requestId,
      ...options?.headers,
    };

    if (token) {
      headers["Authorization"] = `Bearer ${token}`;
    }

    if (body !== undefined && !isFormDataBody(body)) {
      headers["Content-Type"] = "application/json";
    }

    let response: Response;
    try {
      const signal = normalizeAbortSignal(options?.signal);
      response = await fetch(url, {
        method,
        headers,
        body:
          body === undefined
            ? undefined
            : isFormDataBody(body)
              ? body
              : JSON.stringify(body),
        signal,
      });
    } catch (err: unknown) {
      // Re-throw AbortError directly (React Query handles it specially)
      if (isAbortError(err)) {
        throw err;
      }
      // Wrap network errors
      throw networkError(err, requestId);
    }

    // Handle 401 Unauthorized — attempt token refresh
    if (response.status === 401 && !_isRetry) {
      console.debug(`[API] 401 on ${method} ${path}, attempting refresh`);
      await refreshForAnswerTo(token);
      refuseReplayAcrossPrincipals(token, requestId);
      // Retry with new token (mark as retry to prevent infinite loop)
      return request<T>(method, path, body, options, true);
    }

    // Handle non-OK responses
    if (!response.ok) {
      const errorBody = await response.json().catch(() => null);
      if (
        response.status === 403 &&
        membershipDenialApplies(token) &&
        errorBody?.error?.details?.missing_scopes?.includes("org_membership")
      ) {
        notifyOrganizationAccessChanged(tokenPrincipal(token));
      }
      throw new ApiError({
        code: errorBody?.error?.code ?? "UNKNOWN_ERROR",
        message:
          errorBody?.error?.message ||
          response.statusText ||
          `The server did not answer (${response.status})`,
        status: response.status,
        details: errorBody?.error?.details ?? {},
        requestId: errorBody?.error?.requestId ?? requestId,
      });
    }

    // Handle 204 No Content
    if (response.status === 204) {
      return { data: undefined as unknown as T };
    }

    return readJson<ApiResponse<T>>(response, requestId);
  }

  /**
   * A request is sent again after a refresh only for the principal that sent
   * it: words one person typed are never sent as another person, nor in
   * another org. A token rotation for the same principal sends it again.
   */
  function refuseReplayAcrossPrincipals(
    sentWith: string | null,
    requestId: string,
  ): void {
    const sentBy = tokenPrincipal(sentWith);
    const now = getToken();
    if (sentBy !== null && tokenPrincipal(now) !== sentBy) {
      throw new ApiError({
        code: "PRINCIPAL_CHANGED",
        message: now
          ? "You are now signed in as someone else, so this was not sent again."
          : "You are signed out, so this was not sent again.",
        status: 401,
        requestId,
      });
    }
  }

  /** A request made for one principal is not sent under another's token. */
  function refuseForAnotherPrincipal(
    options: RequestOptions | undefined,
    token: string | null,
    requestId: string,
  ): void {
    if (options?.principal && tokenPrincipal(token) !== options.principal) {
      throw new ApiError({
        code: "PRINCIPAL_CHANGED",
        message: token
          ? "You are now signed in as someone else, so this was not sent."
          : "You are signed out, so this was not sent.",
        status: 401,
        requestId,
      });
    }
  }

  async function requestRawResponse(
    method: string,
    path: string,
    body?: unknown,
    options?: RequestOptions,
    _isRetry?: boolean,
  ): Promise<Response> {
    const url = buildUrl(baseUrl, path, options?.params);
    const token = getToken();

    const requestId = randomUuid();
    refuseForAnotherPrincipal(options, token, requestId);

    const headers: Record<string, string> = {
      Accept: "*/*",
      "X-Request-ID": requestId,
      ...options?.headers,
    };

    if (token) {
      headers["Authorization"] = `Bearer ${token}`;
    }

    if (body !== undefined && !isFormDataBody(body)) {
      headers["Content-Type"] = "application/json";
    }

    let response: Response;
    try {
      const signal = normalizeAbortSignal(options?.signal);
      response = await fetch(url, {
        method,
        headers,
        body:
          body === undefined
            ? undefined
            : isFormDataBody(body)
              ? body
              : JSON.stringify(body),
        signal,
      });
    } catch (err: unknown) {
      if (isAbortError(err)) {
        throw err;
      }
      throw networkError(err, requestId);
    }

    if (response.status === 401 && !_isRetry) {
      console.debug(`[API] 401 on ${method} ${path}, attempting refresh`);
      await refreshForAnswerTo(token);
      refuseReplayAcrossPrincipals(token, requestId);
      return requestRawResponse(method, path, body, options, true);
    }

    if (!response.ok) {
      const errorBody = await response.json().catch(() => null);
      if (
        response.status === 403 &&
        membershipDenialApplies(token) &&
        errorBody?.error?.details?.missing_scopes?.includes("org_membership")
      ) {
        notifyOrganizationAccessChanged(tokenPrincipal(token));
      }
      throw new ApiError({
        code: errorBody?.error?.code ?? "UNKNOWN_ERROR",
        message:
          errorBody?.error?.message ||
          response.statusText ||
          `The server did not answer (${response.status})`,
        status: response.status,
        details: errorBody?.error?.details ?? {},
        requestId: errorBody?.error?.requestId ?? requestId,
      });
    }

    return response;
  }

  // -------------------------------------------------------------------------
  // Public API Methods
  // -------------------------------------------------------------------------

  return {
    async get<T>(path: string, options?: RequestOptions): Promise<T> {
      const envelope = await request<T>("GET", path, undefined, options);
      return envelope.data;
    },

    async getRaw<T>(
      path: string,
      options?: RequestOptions,
    ): Promise<ApiResponse<T>> {
      return request<T>("GET", path, undefined, options);
    },

    async getRawResponse(
      path: string,
      options?: RequestOptions,
    ): Promise<Response> {
      return requestRawResponse("GET", path, undefined, options);
    },

    async post<T>(
      path: string,
      body?: unknown,
      options?: RequestOptions,
    ): Promise<T> {
      const envelope = await request<T>("POST", path, body, options);
      return envelope.data;
    },

    async postRaw(
      path: string,
      body?: unknown,
      options?: RequestOptions,
    ): Promise<Response> {
      return requestRawResponse("POST", path, body, options);
    },

    async patch<T>(
      path: string,
      body?: unknown,
      options?: RequestOptions,
    ): Promise<T> {
      const envelope = await request<T>("PATCH", path, body, options);
      return envelope.data;
    },

    async delete<T = void>(path: string, options?: RequestOptions): Promise<T> {
      const envelope = await request<T>("DELETE", path, undefined, options);
      return envelope.data;
    },

    async put<T>(
      path: string,
      body?: unknown,
      options?: RequestOptions,
    ): Promise<T> {
      const envelope = await request<T>("PUT", path, body, options);
      return envelope.data;
    },

    startProactiveRefresh(): () => void {
      const REFRESH_INTERVAL_MS = 50 * 60 * 1000; // 50 minutes
      const intervalId = setInterval(async () => {
        try {
          await refreshToken();
        } catch {
          // Proactive refresh failure is non-fatal;
          // the next request will trigger reactive refresh
        }
      }, REFRESH_INTERVAL_MS);

      return () => clearInterval(intervalId);
    },
  };
}

// ---------------------------------------------------------------------------
// Default Singleton API Client
// ---------------------------------------------------------------------------
// A module-level singleton used by all React Query hooks. The token is stored
// in a module-scoped variable (in-memory only, never persisted). The
// AuthProvider sets/clears the token via setApiToken(). On auth failure the
// singleton redirects to /login.
// ---------------------------------------------------------------------------

let _token: string | null = null;

/**
 * Listener invoked BEFORE the module token is swapped, with the previous and
 * next values. Sole consumer today: the telemetry module, which flushes
 * queued events under the OLD credential when the identity/org changes so
 * cross-org event attribution stays correct (api-client [D20]). Listeners
 * receive the values transiently and must not store them.
 */
type ApiTokenChangeListener = (
  previous: string | null,
  next: string | null,
) => void;

const _tokenChangeListeners = new Set<ApiTokenChangeListener>();

/** Subscribe to token changes; returns an unsubscribe function. */
export function subscribeApiTokenChange(
  listener: ApiTokenChangeListener,
): () => void {
  _tokenChangeListeners.add(listener);
  return () => {
    _tokenChangeListeners.delete(listener);
  };
}

/** Set the JWT access token used by the singleton api client. */
export function setApiToken(token: string | null): void {
  if (token !== _token) {
    for (const listener of _tokenChangeListeners) {
      try {
        listener(_token, token);
      } catch {
        // a subscriber bug must never break auth
      }
    }
  }
  _token = token;
}

/** Get the current JWT access token. */
export function getApiToken(): string | null {
  return _token;
}

/**
 * Pre-configured singleton API client instance.
 * All React Query hooks import and use this instance.
 */
export const api: ApiClient = createApiClient({
  getToken: () =>
    _token ??
    (typeof localStorage !== "undefined"
      ? localStorage.getItem("g5_auth_token")
      : null),
  // Route through setApiToken so token-change listeners fire on the
  // refresh/switch-org paths too (api-client [D20]).
  setToken: (t) => setApiToken(t),
  onAuthFailure: () => {
    // In mock mode, never do a hard redirect — it causes an infinite loop
    console.error(
      "[API] onAuthFailure called. VITE_MOCK =",
      import.meta.env.VITE_MOCK,
    );
    if (typeof window !== "undefined" && import.meta.env.VITE_MOCK !== "true") {
      // A refused refresh ends the local session too, including provider-owned
      // retries and drafts, before navigation can unmount the provider.
      setApiToken(null);
      localStorage.removeItem("g5_auth_token");
      console.error("[API] Redirecting to /login");
      rememberPostAuthRedirect(
        `${window.location.pathname}${window.location.search}${window.location.hash}`,
      );
      window.location.href = "/login";
    } else {
      console.warn("[API] Auth failure suppressed in mock mode");
    }
  },
});
