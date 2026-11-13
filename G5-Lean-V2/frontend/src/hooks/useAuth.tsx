// =============================================================================
// G5 Client — Authentication System
// =============================================================================
// AuthProvider context, useAuth hook, and RequireAuth route guard.
//
// Production: JWT refreshed via httpOnly cookie (server-set, not client-managed).
// Dev/E2E: JWT may be persisted in localStorage via dev-login flow.
// Automatic JWT refresh every 50 minutes via setInterval.
//
// Also provides React Query hooks for user data:
//   useCurrentUser, useUser, useMyScopes
// =============================================================================

import {
  draftPerson,
  retainFileDraftsFor,
} from "../components/files/fileDrafts";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import type { ReactElement, ReactNode } from "react";
import {
  createContext,
  Fragment,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { Navigate, useLocation, useNavigate } from "react-router-dom";
import {
  api,
  getApiToken,
  setApiToken,
  subscribeApiTokenChange,
  tokenPrincipal,
} from "../lib/api";
import { PlatformIssueReportChromePending } from "../components/platform-issues/PlatformIssueReportChromePending";
import type { AuthProviderId } from "../lib/authProviders";
import { clearChatPromptHistory } from "../lib/chatPromptHistory";
import { clearIssueCommentDrafts } from "../lib/issueCommentDrafts";
import { abortScopedConfigRequests } from "../lib/scopedConfigAbort";
import {
  clearPostAuthRedirect,
  rememberPostAuthRedirect,
} from "../lib/postAuthRedirect";
import type { AsyncState, CurrentUserDTO, UserRef } from "../lib/types";
import { toAsyncState } from "./hookUtils";

// ---------------------------------------------------------------------------
// JWT Payload Interface
// ---------------------------------------------------------------------------

/**
 * Shape of the decoded JWT payload. The JWT is decoded client-side
 * (without verification — the server already verified it) to extract
 * scopes for fast hasScope() checks.
 */
export interface JWTPayload {
  // Mirrors the server's JwtClaims (g5-common auth/jwt.rs) exactly — the
  // JWT payload uses these claim names verbatim. There is no email/role
  // claim, and the active org rides `org`, not `orgId`.
  sub: string; // user ID (UUID string)
  org: string; // currently-active org ID (UUID string)
  scopes: string[]; // effective scopes for this org
  iat: number;
  exp: number; // 1 hour
  iss?: string;
  aud?: string;
  jti?: string;
}

// ---------------------------------------------------------------------------
// AuthContextValue Interface
// ---------------------------------------------------------------------------

export interface AuthContextValue {
  /** Current authenticated user, or null if not logged in */
  user: CurrentUserDTO | null;
  /** Authentication is unresolved; a same-person page can remain mounted. */
  isLoading: boolean;
  /** Effective scopes for the current org (extracted from JWT) */
  scopes: string[];
  /** Initiate OAuth login with a provider */
  login: (provider: AuthProviderId) => void;
  /** Logout and revoke refresh token */
  logout: () => void;
  /** Switch active org — re-issues JWT with new org context and scopes */
  switchOrg: (orgId: string) => Promise<void>;
  /** Fast client-side scope check */
  hasScope: (scope: string) => boolean;
  /** Check if user has ALL specified scopes */
  hasAllScopes: (scopes: string[]) => boolean;
  /**
   * The person and org the token in use speaks for, changed the moment a
   * token is adopted, before its profile loads; null without a readable token.
   */
  principalKey: string | null;
  /**
   * Adopt a profile the server just returned (the PATCH on the "me" route
   * answers with the full profile), keeping the token's organization.
   */
  applyProfile: (profile: CurrentUserDTO) => void;
}

// Re-export CurrentUserDTO for convenience
export type { CurrentUserDTO };

// ---------------------------------------------------------------------------
// Internal Helpers
// ---------------------------------------------------------------------------

/** Refresh interval: 50 minutes (before 60-minute JWT expiry) */
const REFRESH_INTERVAL_MS = 50 * 60 * 1000;

/**
 * Decode a JWT payload without verification.
 * Uses base64url decoding (atob) — no external library needed.
 */
/**
 * True when two tokens differ in ANY authorization-bearing claim — subject
 * (`sub`), active org (`org`), or the normalized scope set (settings-page
 * D9; claim names match the server's JwtClaims exactly). A same-user scope
 * downgrade also invalidates the right to cached scoped-config data, so
 * principal identity alone is not enough. An undecodable token
 * conservatively counts as changed.
 */
function authClaimsChanged(previous: string | null, next: string): boolean {
  try {
    const n = decodeJWT(next);
    if (!previous) {
      return true;
    }
    const p = decodeJWT(previous);
    const normalize = (s: string[] | undefined): string =>
      [...(s ?? [])].sort().join(" ");
    return (
      p.sub !== n.sub ||
      p.org !== n.org ||
      normalize(p.scopes) !== normalize(n.scopes)
    );
  } catch {
    return true;
  }
}

/**
 * The token in use when it carries the same authorization claims as
 * `acting` (it, or a renewal of it); otherwise null. A step finishing for
 * `acting` completes with that credential, and stops after a sign-out or
 * once another context is in use (`g5.security.login.principal`).
 */
function sameContextInUse(acting: string): string | null {
  const installed = getApiToken();
  return installed && !authClaimsChanged(acting, installed) ? installed : null;
}

function decodeJWT(token: string): JWTPayload {
  const parts = token.split(".");
  if (parts.length !== 3) {
    throw new Error("Invalid JWT format");
  }
  // base64url → base64 → decode
  const payload = parts[1].replace(/-/g, "+").replace(/_/g, "/");
  const jsonStr = atob(payload);
  return JSON.parse(jsonStr) as JWTPayload;
}

/** A fresh token, a refusal (the session is over), or no answer (try later). */
type TokenRefresh =
  | { outcome: "token"; jwt: string }
  | { outcome: "refused" }
  | { outcome: "unavailable" };

async function refreshToken(orgId?: string): Promise<string | null> {
  const refreshed = await requestToken(orgId);
  return refreshed.outcome === "token" ? refreshed.jwt : null;
}

/**
 * Ask for a fresh token. No answer, or a server that cannot answer now, is
 * not a refused session: the caller keeps the session it has and tries
 * again (`g5.workspace.page.refresh`).
 */
async function requestToken(
  orgId?: string,
  forceRefresh = false,
): Promise<TokenRefresh> {
  try {
    // Check for locally-stored JWT first (set by dev login or E2E setup).
    // If it hasn't expired, return it directly.
    const storedJwt = forceRefresh
      ? null
      : localStorage.getItem("g5_auth_token");
    if (storedJwt && !orgId) {
      try {
        const claims = decodeJWT(storedJwt);
        const nowSec = Math.floor(Date.now() / 1000);
        // Allow 60s buffer to avoid race conditions near expiry
        if (claims.exp && claims.exp > nowSec + 60) {
          return { outcome: "token", jwt: storedJwt }; // Still valid
        }
      } catch {
        // decodeJWT failed — fall through to clear
      }
      // Token expired or invalid — clear it and fall through to
      // cookie-based refresh so the normal auth flow takes over.
      localStorage.removeItem("g5_auth_token");
    }

    const body = orgId ? JSON.stringify({ orgId }) : undefined;
    const headers: Record<string, string> = {};
    if (body) {
      headers["Content-Type"] = "application/json";
    }

    const res = await fetch("/api/v1/auth/refresh", {
      method: "POST",
      credentials: "include",
      headers,
      body,
    });

    if (!res.ok) {
      return res.status === 408 || res.status === 429 || res.status >= 500
        ? { outcome: "unavailable" }
        : { outcome: "refused" };
    }

    const json = await res.json();
    return typeof json.data?.jwt === "string" && tokenPrincipal(json.data.jwt)
      ? { outcome: "token", jwt: json.data.jwt }
      : { outcome: "unavailable" };
  } catch (err) {
    console.warn("[auth] refreshToken failed:", err);
    return { outcome: "unavailable" };
  }
}

/**
 * Fetch the current user profile using the JWT.
 */
type ProfileResult =
  | { outcome: "profile"; user: CurrentUserDTO }
  | { outcome: "unauthorized" }
  | { outcome: "refused" }
  | { outcome: "unavailable" };

async function fetchCurrentUser(jwt: string): Promise<ProfileResult> {
  try {
    const res = await fetch("/api/v1/users/me", {
      method: "GET",
      credentials: "include",
      headers: {
        Authorization: `Bearer ${jwt}`,
      },
    });

    if (!res.ok) {
      return {
        outcome:
          res.status === 401
            ? "unauthorized"
            : res.status === 408 || res.status === 429 || res.status >= 500
              ? "unavailable"
              : "refused",
      };
    }

    const json = await res.json();
    return json.data?.id
      ? { outcome: "profile", user: json.data as CurrentUserDTO }
      : { outcome: "unavailable" };
  } catch (err) {
    console.warn("[auth] fetchCurrentUser failed:", err);
    return { outcome: "unavailable" };
  }
}

// ---------------------------------------------------------------------------
// Auth Context
// ---------------------------------------------------------------------------

const AuthContext = createContext<AuthContextValue | null>(null);

// ---------------------------------------------------------------------------
// AuthProvider Component
// ---------------------------------------------------------------------------

export interface AuthProviderProps {
  children: ReactNode;
}

/**
 * React context provider managing the complete JWT lifecycle:
 * - Initial auth check on mount via POST /api/v1/auth/refresh
 * - Automatic JWT refresh every 50 minutes
 * - User state, scopes, and scope check helpers
 * - Login, logout, and org switching
 */
export function AuthProvider({ children }: AuthProviderProps): ReactElement {
  const queryClient = useQueryClient();
  const jwtRef = useRef<string | null>(null);
  const [user, setUser] = useState<CurrentUserDTO | null>(null);
  const [principalKey, setPrincipalKey] = useState<string | null>(() =>
    tokenPrincipal(getApiToken()),
  );
  const adoptionEpochRef = useRef(0);
  const profileAttemptRef = useRef(0);
  const profileRetryRef = useRef<ReturnType<typeof setTimeout> | undefined>(
    undefined,
  );
  // Every adopted token, from any path (sign-in, periodic refresh, org
  // switch, a request's own refresh), names its principal at once.
  useEffect(
    () =>
      subscribeApiTokenChange((previous, next) => {
        const principal = tokenPrincipal(next);
        setPrincipalKey(principal);
        jwtRef.current = next;
        if (next && !authClaimsChanged(previous, next)) return;
        ++adoptionEpochRef.current;
        clearTimeout(profileRetryRef.current);
        abortScopedConfigRequests();
        // Publish the new context synchronously, before cache eviction or a
        // profile request can yield. Organization changes keep this person.
        const person = draftPerson(principal);
        if (!next || person !== draftPerson(tokenPrincipal(previous))) {
          ++switchAttemptRef.current;
        }
        setUser((current) =>
          current && person === current.id
            ? { ...current, activeOrgId: principal!.slice(person.length + 1) }
            : null,
        );
        setIsLoading(next !== null);
        try {
          setScopes(next ? decodeJWT(next).scopes : []);
        } catch {
          setScopes([]);
        }
        if (!next) {
          clearIssueCommentDrafts();
          clearChatPromptHistory();
        }
      }),
    [],
  );
  const [scopes, setScopes] = useState<string[]>([]);
  const [isLoading, setIsLoading] = useState<boolean>(true);
  // Token the provider itself is about to install via an explicit path
  // (initial auth, periodic refresh, org switch) — the token-change
  // listener skips its silent-adoption sync for these, because the
  // explicit path owns cache eviction and profile loading.
  const expectedAdoptionRef = useRef<string | null>(null);
  // The latest org switch asked for: an earlier one answered later is older.
  const switchAttemptRef = useRef(0);

  const navigate = useNavigate();
  // Sign-in and the periodic refresh run once per mount. `navigate` changes
  // with every route change, and a run per navigation signed this tab back
  // in after a sign-out through a refresh cookie the server had not revoked
  // (`g5.security.login.principal`).
  const navigateRef = useRef(navigate);
  useEffect(() => {
    navigateRef.current = navigate;
  }, [navigate]);

  const evictPrincipalCaches = useCallback(async () => {
    // Scoped-config responses carry secret key names and bounded display
    // hints (settings-page D9): evict them on every auth-principal
    // transition so a later principal in the same tab cannot read the
    // previous principal's entries from the in-memory query cache.
    // Abort in-flight scoped-config writes FIRST — clearing the mutation
    // cache alone would still let a running request settle its
    // hint-bearing response into hook state after the transition.
    abortScopedConfigRequests();
    await queryClient.cancelQueries({ queryKey: ["scoped-config"] });
    // RESET (not merely remove) scoped-config queries: removeQueries leaves
    // mounted observers rendering their previous data, while resetQueries
    // drops them to the initial empty state and refetches active ones under
    // whatever principal is installed by then (settings-page D9).
    void queryClient.resetQueries({ queryKey: ["scoped-config"] });
    // Completed scoped-config PUT results also carry hint maps in the
    // mutation cache (settings-page D9); an auth-principal transition
    // invalidates every pending/settled mutation of the old principal.
    queryClient.getMutationCache().clear();
  }, [queryClient]);

  const endSession = useCallback(() => {
    // Also fence a first-load refresh when no token has been installed yet.
    ++adoptionEpochRef.current;
    ++switchAttemptRef.current;
    clearTimeout(profileRetryRef.current);
    jwtRef.current = null;
    setApiToken(null);
    localStorage.removeItem("g5_auth_token");
    clearIssueCommentDrafts();
    clearChatPromptHistory();
    retainFileDraftsFor(null);
    setUser(null);
    setScopes([]);
    setIsLoading(false);
  }, []);

  // One profile recovery for the current transition, shared by every token
  // producer. The first attempt can be awaited by an explicit org switch;
  // unavailable answers continue in the provider's bounded-backoff lifecycle.
  const loadProfile = useCallback(
    async (jwt: string, epoch: number) => {
      if (epoch !== adoptionEpochRef.current || !sameContextInUse(jwt)) return;
      const attempt = ++profileAttemptRef.current;
      clearTimeout(profileRetryRef.current);
      const current = () =>
        epoch === adoptionEpochRef.current &&
        attempt === profileAttemptRef.current &&
        sameContextInUse(jwt);
      let renewedCredential: string | null = null;
      const run = async (delay = 1000, renew = false): Promise<void> => {
        let inUse = current();
        if (!inUse) return;
        const retry = (refresh: boolean) => {
          profileRetryRef.current = setTimeout(() => {
            void run(Math.min(delay * 2, 30_000), refresh);
          }, delay);
        };
        if (renew) {
          const stored = localStorage.getItem("g5_auth_token");
          if (stored && !authClaimsChanged(inUse, stored)) {
            localStorage.removeItem("g5_auth_token");
          }
          const refreshed = await requestToken(undefined, true);
          if (!current()) return;
          // Even a same-context renewal supersedes a refused older exchange.
          if (getApiToken() !== inUse) return run(delay);
          if (refreshed.outcome === "unavailable") return retry(true);
          if (refreshed.outcome === "refused") {
            endSession();
            await evictPrincipalCaches();
            return;
          }
          setApiToken(refreshed.jwt);
          renewedCredential = refreshed.jwt;
          inUse = current();
          // A different context is synchronized by the adoption listener.
          if (!inUse) return;
        }
        const profile = await fetchCurrentUser(inUse);
        if (!current()) return;
        if (profile.outcome === "unavailable") return retry(false);
        if (profile.outcome !== "profile" && getApiToken() !== inUse) {
          // A refusal of an older credential says nothing about a renewal
          // installed while this profile request was pending.
          return run(delay);
        }
        if (profile.outcome === "unauthorized") {
          // Temporary profile failures do not erase this credential's renewal.
          // Expiry during backoff can still require another exchange; a token
          // refused immediately after renewal must not start a refresh loop.
          const expiredWhileWaiting =
            !renew && decodeJWT(inUse).exp <= Math.floor(Date.now() / 1000);
          if (inUse !== renewedCredential || expiredWhileWaiting)
            return run(delay, true);
          endSession();
          await evictPrincipalCaches();
          return;
        }
        if (profile.outcome === "refused") {
          endSession();
          await evictPrincipalCaches();
          return;
        }
        if (profile.user.id !== draftPerson(tokenPrincipal(inUse))) {
          return retry(false);
        }
        // Profile data may lag the cookie's selected organization. The adopted
        // token remains the owner of the active authorization context.
        setUser({ ...profile.user, activeOrgId: decodeJWT(inUse).org });
        setIsLoading(false);
      };
      await run();
    },
    [endSession, evictPrincipalCaches],
  );

  useEffect(() => {
    let pending = false;
    let attempt = 0;
    let retryTimer: ReturnType<typeof setTimeout> | undefined;
    const cancelRecovery = () => {
      attempt += 1;
      pending = false;
      clearTimeout(retryTimer);
      retryTimer = undefined;
    };
    const unsubscribe = subscribeApiTokenChange((previous, next) => {
      if (!next || authClaimsChanged(previous, next)) cancelRecovery();
    });
    const recover = (event: Event) => {
      if (pending) return;
      const acting = getApiToken();
      const principal = (event as CustomEvent<{ principal?: string }>).detail
        ?.principal;
      if (!acting || !principal || principal !== tokenPrincipal(acting)) return;
      pending = true;
      const currentAttempt = ++attempt;
      const isCurrent = () =>
        currentAttempt === attempt && sameContextInUse(acting);
      const stored = localStorage.getItem("g5_auth_token");
      if (stored && !authClaimsChanged(acting, stored)) {
        localStorage.removeItem("g5_auth_token");
      }
      abortScopedConfigRequests();
      const refresh = async (retryDelay = 1000) => {
        if (!isCurrent()) return;
        const refreshed = await requestToken(undefined, true);
        if (!isCurrent()) return;
        if (refreshed.outcome === "unavailable") {
          // Keep one recovery in flight, including its backoff. A context
          // switch or sign-out cancels the retry and fences late responses.
          retryTimer = setTimeout(() => {
            void refresh(Math.min(retryDelay * 2, 30_000));
          }, retryDelay);
          return;
        }
        pending = false;
        if (refreshed.outcome === "token") {
          setApiToken(refreshed.jwt);
        } else {
          // An implicit refresh can select another org or account-only access.
          // A refusal means the refresh session itself is no longer valid.
          jwtRef.current = null;
          setApiToken(null);
          setUser(null);
          setScopes([]);
          navigateRef.current("/login", { replace: true });
        }
      };
      void (async () => {
        // Remove the disabled organization's live views before selecting an
        // active organization or the account-only destination.
        await queryClient.cancelQueries();
        if (!isCurrent()) return;
        queryClient.clear();
        navigateRef.current("/", { replace: true });
        await refresh();
      })();
    };
    window.addEventListener("g5:organization-access-changed", recover);
    return () => {
      cancelRecovery();
      unsubscribe();
      window.removeEventListener("g5:organization-access-changed", recover);
    };
  }, [queryClient]);

  // Every token adoption must pass the authorization-claims check,
  // including the api client's silent 401-refresh retry path, which never
  // goes through the explicit logout/switch-org/periodic-refresh handlers:
  // another tab can rotate the shared session cookie to a different user,
  // org, role, or scope set, and the silently adopted JWT would otherwise
  // leave stale cached hints readable and the UI rendering the previous
  // identity (settings-page D9). The api client fires token-change
  // listeners before each swap. Explicit provider paths pre-announce their
  // adoption via expectedAdoptionRef and own their eviction/profile sync;
  // everything else is a silent adoption handled here: abort old-principal
  // requests immediately, then — after the swap completes, so
  // reset-triggered refetches run under the NEW token — evict caches and
  // reload the profile, guarded against superseded adoptions. The earlier
  // subscription already published identity and scopes synchronously.
  useEffect(() => {
    return subscribeApiTokenChange((previous, next) => {
      if (!next) {
        // Token clears ride the explicit logout/expiry paths, which evict.
        return;
      }
      const expected = expectedAdoptionRef.current === next;
      if (expected) {
        expectedAdoptionRef.current = null;
      }
      if (!authClaimsChanged(previous, next)) {
        jwtRef.current = next;
        return;
      }
      if (expected) {
        // Explicit path (initial auth / periodic refresh / org switch)
        // performs its own eviction and profile loading.
        return;
      }
      abortScopedConfigRequests();
      const epoch = adoptionEpochRef.current;
      queueMicrotask(() => {
        void (async () => {
          if (epoch !== adoptionEpochRef.current) return;
          await evictPrincipalCaches();
          if (epoch !== adoptionEpochRef.current) return;
          // A sign-out, or another context, installed meanwhile owns what
          // is shown now; a renewal of this one finishes this adoption with
          // the renewed credential (`g5.security.login.principal`).
          const inUse = sameContextInUse(next);
          if (!inUse) return;
          await loadProfile(inUse, epoch);
        })();
      });
    });
  }, [evictPrincipalCaches, loadProfile]);

  // -----------------------------------------------------------------------
  // Logout
  // -----------------------------------------------------------------------

  const logout = useCallback(async () => {
    const acting = jwtRef.current;
    endSession();
    const epoch = adoptionEpochRef.current;
    await evictPrincipalCaches();
    try {
      await fetch("/api/v1/auth/logout", {
        method: "POST",
        credentials: "include",
        headers: acting ? { Authorization: `Bearer ${acting}` } : undefined,
      });
    } catch {
      // Best-effort logout — local state was already cleared.
    }

    if (epoch !== adoptionEpochRef.current) return;

    // The eviction's reset may have started refetches under the pre-logout
    // token; sweep scoped-config entries once more now that the token is
    // cleared so a late refetch cannot repopulate them (settings-page D9).
    void queryClient.cancelQueries({ queryKey: ["scoped-config"] });
    queryClient.removeQueries({ queryKey: ["scoped-config"] });
    navigate("/login", { replace: true });
  }, [endSession, evictPrincipalCaches, navigate, queryClient]);

  // -----------------------------------------------------------------------
  // Initial auth check + periodic refresh
  // -----------------------------------------------------------------------

  useEffect(() => {
    let cancelled = false;
    let initialRetry: ReturnType<typeof setTimeout> | undefined;
    const initialEpoch = adoptionEpochRef.current;

    // Initial auth check
    async function initialAuth(retryDelay = 1000) {
      if (cancelled || initialEpoch !== adoptionEpochRef.current) return;
      // In mock mode, bypass network auth entirely and load mock user directly
      if (import.meta.env.VITE_MOCK === "true") {
        try {
          const { MOCK_CURRENT_USER } = await import("../data/mock");
          const { setApiToken } = await import("../lib/api");
          if (cancelled) return;
          jwtRef.current = "mock-token";
          setApiToken("mock-token");
          setScopes([
            "workspaces:read",
            "workspaces:write",
            "tasks:execute",
            "workers:admin",
            "org:admin",
            "sessions:write",
          ]);
          setUser(MOCK_CURRENT_USER);
          setIsLoading(false);
          return;
        } catch (e) {
          console.warn(
            "[MockAuth] Failed to load mock user, falling back to network auth:",
            e,
          );
        }
      }

      const installed = getApiToken();
      const refreshed = await requestToken();
      if (
        cancelled ||
        initialEpoch !== adoptionEpochRef.current ||
        getApiToken() !== installed
      )
        return;
      if (refreshed.outcome === "unavailable") {
        initialRetry = setTimeout(() => {
          void initialAuth(Math.min(retryDelay * 2, 30_000));
        }, retryDelay);
        return;
      }
      if (refreshed.outcome === "refused") {
        endSession();
        return;
      }
      const jwt = refreshed.jwt;
      jwtRef.current = jwt;
      expectedAdoptionRef.current = jwt;
      setApiToken(jwt);
      expectedAdoptionRef.current = null;
      try {
        setScopes(decodeJWT(jwt).scopes);
      } catch {
        setScopes([]);
      }
      await loadProfile(jwt, adoptionEpochRef.current);
    }

    initialAuth();

    // Periodic refresh every 50 minutes
    const interval = setInterval(async () => {
      // The token in use when this refresh goes out. Another adoption while
      // it is out (an org switch, a request's own refresh, a sign-out) is
      // newer than its answer, which is then dropped, a token or a refusal
      // alike; the next interval refreshes again
      // (`g5.security.login.principal`).
      const previousJwt = getApiToken();
      const startedAt = adoptionEpochRef.current;
      // Signed out: there is no session to keep, and a refresh cookie the
      // server failed to revoke must not sign this tab back in.
      if (previousJwt === null) return;
      const refreshed = await requestToken();
      if (cancelled) return;
      if (
        startedAt !== adoptionEpochRef.current ||
        getApiToken() !== previousJwt
      )
        return;
      // No answer is not an expired session: the session, and the page with
      // it, stay; the next interval, or a request's own refresh on a 401,
      // tries again.
      if (refreshed.outcome === "unavailable") return;
      const jwt = refreshed.outcome === "token" ? refreshed.jwt : null;

      if (jwt) {
        // The refresh cookie is shared across tabs: another tab may have
        // re-authenticated as a different user, switched org, or changed
        // role/scopes. Detect any authorization-claims change before
        // adopting the token and evict principal-scoped caches — including
        // scoped-config secret hints (settings-page D9) — so stale data
        // cannot render under the new context. Undecodable tokens count as
        // a change.
        const claimsChanged = authClaimsChanged(previousJwt, jwt);
        if (claimsChanged) {
          // Kill the outgoing context's in-flight scoped-config writes
          // before anything else; the full eviction runs AFTER the new
          // token installs so reset-triggered refetches never replay
          // under the outgoing credential (settings-page D9).
          abortScopedConfigRequests();
        }

        jwtRef.current = jwt;
        expectedAdoptionRef.current = jwt;
        setApiToken(jwt);
        expectedAdoptionRef.current = null;
        const epoch = adoptionEpochRef.current;
        if (claimsChanged) {
          await evictPrincipalCaches();
          if (cancelled) return;
          await loadProfile(jwt, epoch);
        }
      } else {
        // Refresh failed — session expired
        await evictPrincipalCaches();
        // A token installed while evicting is newer than this refusal.
        if (
          cancelled ||
          startedAt !== adoptionEpochRef.current ||
          getApiToken() !== previousJwt
        )
          return;
        endSession();
        // The eviction's reset may have started refetches under the
        // pre-expiry token; sweep scoped-config entries once more now that
        // the token is cleared (settings-page D9).
        void queryClient.cancelQueries({ queryKey: ["scoped-config"] });
        queryClient.removeQueries({ queryKey: ["scoped-config"] });
        navigateRef.current("/login", { replace: true });
      }
    }, REFRESH_INTERVAL_MS);

    return () => {
      cancelled = true;
      clearInterval(interval);
      clearTimeout(initialRetry);
      clearTimeout(profileRetryRef.current);
      ++adoptionEpochRef.current;
    };
  }, [endSession, evictPrincipalCaches, loadProfile, queryClient]);

  // -----------------------------------------------------------------------
  // Login
  // -----------------------------------------------------------------------

  const login = useCallback((provider: AuthProviderId) => {
    window.location.href = `/api/v1/auth/login/${provider}`;
    // eslint-disable-next-line no-console
    console.error("[Auth] Redirect to login provider:", provider);
  }, []);

  // -----------------------------------------------------------------------
  // Switch Org
  // -----------------------------------------------------------------------

  const switchOrg = useCallback(
    async (orgId: string) => {
      const startedWith = getApiToken();
      const attempt = ++switchAttemptRef.current;
      const jwt = await refreshToken(orgId);
      // The answer is for the session the switch was asked in: not after a
      // sign-out, another person's sign-in or a later switch. A renewal of
      // the same person's token meanwhile does not change that
      // (`g5.security.login.principal`).
      const installed = getApiToken();
      if (
        attempt !== switchAttemptRef.current ||
        installed === null ||
        tokenPrincipal(installed)?.split(":")[0] !==
          tokenPrincipal(startedWith)?.split(":")[0]
      ) {
        throw new Error(
          "The session changed before the organization switch finished",
        );
      }
      if (!jwt) {
        throw new Error("Unable to switch organization");
      }
      let payload: JWTPayload;
      try {
        payload = decodeJWT(jwt);
      } catch (err) {
        console.warn("[Auth] JWT decode failed during org switch:", err);
        throw new Error("Unable to switch organization");
      }
      if (payload.org !== orgId) {
        throw new Error(
          "Organization switch returned the wrong authorization context",
        );
      }
      // Abort outgoing writes now; evict AFTER the new token installs so
      // reset refetches run under the new org context (settings-page D9).
      abortScopedConfigRequests();
      jwtRef.current = jwt;
      expectedAdoptionRef.current = jwt;
      setApiToken(jwt);
      expectedAdoptionRef.current = null;
      const epoch = adoptionEpochRef.current;
      await evictPrincipalCaches();
      // A sign-out, or another context, installed meanwhile owns what is
      // shown now; a renewal of this one finishes the switch with the
      // renewed credential, never the older one.
      const inUse = sameContextInUse(jwt);
      if (!inUse || epoch !== adoptionEpochRef.current) return;
      if (
        typeof localStorage !== "undefined" &&
        localStorage.getItem("g5_auth_token")
      ) {
        localStorage.setItem("g5_auth_token", inUse);
      }
      setScopes(payload.scopes);

      await loadProfile(inUse, epoch);
    },
    [evictPrincipalCaches, loadProfile],
  );

  // -----------------------------------------------------------------------
  // Scope Checks
  // -----------------------------------------------------------------------

  const hasScope = useCallback(
    (scope: string): boolean => scopes.includes(scope),
    [scopes],
  );

  const hasAllScopes = useCallback(
    (requiredScopes: string[]): boolean =>
      requiredScopes.every((s) => scopes.includes(s)),
    [scopes],
  );

  // -----------------------------------------------------------------------
  // Context Value
  // -----------------------------------------------------------------------

  const applyProfile = useCallback((profile: CurrentUserDTO) => {
    setUser((current) =>
      current && current.id === profile.id
        ? { ...profile, activeOrgId: current.activeOrgId }
        : current,
    );
  }, []);

  const value = useMemo<AuthContextValue>(
    () => ({
      user,
      isLoading,
      scopes,
      login,
      logout,
      switchOrg,
      hasScope,
      hasAllScopes,
      principalKey,
      applyProfile,
    }),
    [
      user,
      isLoading,
      scopes,
      login,
      logout,
      switchOrg,
      hasScope,
      hasAllScopes,
      principalKey,
      applyProfile,
    ],
  );

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}

// ---------------------------------------------------------------------------
// useAuth Hook
// ---------------------------------------------------------------------------

/**
 * Hook to access the authentication context.
 * Must be used within an AuthProvider.
 *
 * @returns AuthContextValue with user, scopes, and auth actions
 * @throws Error if used outside of AuthProvider
 */
export function useAuth(): AuthContextValue {
  const context = useContext(AuthContext);
  if (!context) {
    throw new Error("useAuth must be used within an AuthProvider");
  }
  return context;
}

/**
 * The principal this render was made for (`principalKey`), or null outside
 * an AuthProvider. A handler created in the render keeps it, so a write it
 * starts is sent only for that principal, even once another one's token has
 * arrived and before the page has caught up with it
 * (`g5.security.login.principal`).
 */
export function useRenderedPrincipal(): string | null {
  return useContext(AuthContext)?.principalKey ?? null;
}

// ---------------------------------------------------------------------------
// RequireAuth Component
// ---------------------------------------------------------------------------

export interface RequireAuthProps {
  children: ReactNode;
  requiredScope?: string;
}

/**
 * Route guard component that wraps authenticated routes.
 *
 * Behavior:
 * - If isLoading → render a loading spinner (centered)
 * - If user is null and not loading → redirect to /login with state.from
 * - If user present but requiredScope fails → redirect to /forbidden
 * - If user present and scope check passes → render children
 */
export function RequireAuth({
  children,
  requiredScope,
}: RequireAuthProps): ReactElement {
  const { user, isLoading, hasScope, principalKey } = useAuth();
  const location = useLocation();

  useEffect(() => {
    if (user) {
      clearPostAuthRedirect();
    }
  }, [user]);

  // Unsaved FILES drafts belong to one person, like what this guards:
  // another person, or signing out, drops them; an org switch keeps them
  // (`g5.documents.files-view.drafts`).
  const person = draftPerson(principalKey) ?? user?.id ?? null;
  useEffect(() => {
    // Still loading is not signed out.
    if (isLoading && !person) return;
    retainFileDraftsFor(person);
  }, [isLoading, person]);

  useEffect(() => {
    if (!isLoading && !user) {
      rememberPostAuthRedirect(
        `${location.pathname}${location.search}${location.hash}`,
      );
    }
  }, [isLoading, location.hash, location.pathname, location.search, user]);

  if (isLoading && !user) {
    // Loading spinner — centered, using design system tokens. The page that
    // replaces it usually carries the report trigger in its chrome, so the
    // floating fallback waits (ui g5.quality.platform-reports.trigger).
    return (
      <div className="h-screen w-full flex items-center justify-center bg-[var(--g5-bg0)]">
        <PlatformIssueReportChromePending />
        <div className="flex flex-col items-center gap-3">
          <div
            className="w-8 h-8 border-2 border-[var(--g5-text2)] border-t-[var(--g5-info)] rounded-full animate-spin"
            role="status"
            aria-label="Loading"
          />
          <span className="text-sm text-[var(--g5-text2)]">Loading…</span>
        </div>
      </div>
    );
  }

  if (!user) {
    // Redirect to the public landing route, preserving original URL for
    // post-login redirect. The landing route forwards to the static
    // coming-soon experience.
    return <Navigate to="/login" state={{ from: location }} replace />;
  }

  if (requiredScope && !hasScope(requiredScope)) {
    // User lacks required scope — redirect to forbidden
    return <Navigate to="/forbidden" replace />;
  }

  // What this guards belongs to one person. Another person gets it afresh
  // the moment their token is adopted, so nothing the previous one typed or
  // was sending stays on screen under the new identity. An org switch keeps
  // it, so a flow that switched org (creating one) goes on; the comment and
  // review composers below start empty for the new org on their own
  // (`useScopedComposerStore`). A token rotation keeps it too.
  return <Fragment key={person ?? user.id}>{children}</Fragment>;
}

// ---------------------------------------------------------------------------
// React Query Auth Hooks
// ---------------------------------------------------------------------------

/**
 * Query hook for the current authenticated user.
 * Uses a long staleTime (5 minutes) since user data rarely changes.
 */
export function useCurrentUser(): AsyncState<CurrentUserDTO> {
  const query = useQuery<CurrentUserDTO, Error>({
    queryKey: ["current-user"],
    queryFn: ({ signal }) =>
      api.get<CurrentUserDTO>("/api/v1/users/me", { signal }),
    staleTime: 300_000, // 5 minutes
  });
  return toAsyncState(query);
}

/**
 * Query hook for a user by ID.
 */
export function useUser(userId: string): AsyncState<UserRef> {
  const query = useQuery<UserRef, Error>({
    queryKey: ["user", userId],
    queryFn: ({ signal }) =>
      api.get<UserRef>(`/api/v1/users/${userId}`, { signal }),
    enabled: !!userId,
  });
  return toAsyncState(query);
}
