// =============================================================================
// G5 Client — Root Application Component
// =============================================================================
// BrowserRouter + AuthProvider wrapping all route definitions.
// Defines the complete route table, LoginPage, ForbiddenPage, and
// LandingRedirect components.
//
// All routes use path-based component_id identifiers (not UUIDs).
// The URL hierarchy reflects the data model: org → workspace.
// =============================================================================

import { useQueryClient } from "@tanstack/react-query";
import { Check, X } from "lucide-react";
import React, { useEffect, useMemo, useRef, useState } from "react";
import {
  BrowserRouter,
  Navigate,
  Route,
  Routes,
  useLocation,
  useNavigate,
  useParams,
} from "react-router-dom";
import {
  AppShell,
  SelfHostedPreProductionBannerProvider,
  TopBar,
} from "./components/layout/Layout";
import { Button } from "./components/ui/UI";
import { AuthProvider, RequireAuth, useAuth } from "./hooks/useAuth";
import { useInstanceInfo } from "./hooks/useSettings";
import { WebSocketManager, WebSocketProvider } from "./lib/ws";
import { ApiError, api, getApiToken, tokenPrincipal } from "./lib/api";
import {
  getPostAuthRedirect,
  rememberPostAuthRedirect,
} from "./lib/postAuthRedirect";
import {
  AUTH_PROVIDER_ORDER,
  AuthProviderIcon,
  getAuthProviderHref,
  getAuthProviderLabel,
  isAuthProviderId,
} from "./lib/authProviders";
import { isDevMode } from "./lib/devMode";
import { isGithubAppEnabled } from "./lib/githubAppFlags";
import { GithubConnectionCard } from "./components/settings/GithubConnectionCard";
import { useGithubAfterLoginOffer } from "./hooks/useGithubConnection";
import { GithubConnectionPage } from "./pages/GithubConnectionPage";
import { isGithubConnectionPath } from "./lib/githubConnection";
import { preferenceTelemetryValues } from "./lib/preferenceTelemetry";
import {
  recordNavigation,
  startTelemetry,
  telemetryRoute,
} from "./lib/telemetry";
import { readUserSettings } from "./hooks/useUserSettings";
import { routes } from "./lib/routes";
import {
  FORK_PICKER_ROUTE,
  PERSONAL_SHELL_ROUTE_PATTERNS,
  WORKSPACE_SHELL_ROUTE_PATTERNS,
  isWorkspaceShellRoute,
} from "./lib/shellRoutes";
import { WorkspaceTabsProvider } from "./hooks/useWorkspaceTabs";
import { ForkPickerPage } from "./pages/ForkPickerPage";

// ---------------------------------------------------------------------------
// Lazy-loaded page components from other modules
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Page components — direct imports (lazy loading removed for React 19 compat)
// ---------------------------------------------------------------------------

import { HomePage } from "./pages/HomePage";
import { IssuePage } from "./pages/IssuePage";
import { ISSUE_PAGE_PATH, NATIVE_ISSUE_PAGE_PATH } from "./lib/issuePage";
import { PlatformIssueReportProvider } from "./components/platform-issues/PlatformIssueReportButton";
import { ProjectsPage } from "./pages/ProjectsPage";
import { ApprovalReportPage } from "./pages/ApprovalReportPage";
import { APPROVAL_REPORT_PATH } from "./lib/approvalReports";
import { WorkspacePage } from "./pages/WorkspacePage";
import { OnboardingTour } from "./components/onboarding/OnboardingTour";
import type { CurrentUserDTO } from "./lib/types";

export { isWorkspaceShellRoute } from "./lib/shellRoutes";

// ---------------------------------------------------------------------------
// Legacy `/settings` redirect — the standalone settings page was replaced by
// the org-scoped WorkspaceSettingsPage (`/:org/settings`). Redirect to the
// active org's settings, preserving the query string so SCM OAuth callbacks
// that land on `/settings` (no initiating workspace route) still reach a page
// that consumes `scm_connected` / `scm_error`.
// ---------------------------------------------------------------------------

const SettingsRedirect: React.FC = () => {
  const { user } = useAuth();
  const location = useLocation();
  if (!user || user.orgs.length === 0) {
    return <Navigate to="/" replace />;
  }
  const activeOrg = user.activeOrgId
    ? (user.orgs.find((o) => o.orgId === user.activeOrgId) ?? user.orgs[0])
    : user.orgs[0];
  return (
    <Navigate to={`/${activeOrg.orgSlug}/settings${location.search}`} replace />
  );
};

// Normalize only the organization index; explicit child destinations keep their routes.
const OrganizationHomeRedirect: React.FC = () => {
  const { org } = useParams<{ org: string }>();
  const location = useLocation();
  return (
    <Navigate to={`/${org}/home${location.search}${location.hash}`} replace />
  );
};

// ---------------------------------------------------------------------------
// Org admin redirects — the Settings, Worker Hosts, and Org Members surfaces
// now render INSIDE the workspace chrome as center-panel tabs. The standalone
// `/:org/settings*` routes therefore redirect into the org's `base` workspace
// with `?view=settings|workers|members`, which `WorkspacePage` consumes on
// mount to open the matching tab. Any existing query (e.g. SCM OAuth callbacks
// landing on `/:org/settings` with `scm_connected`/`scm_error`) is preserved by
// merging it into the redirect's search string. The `base` workspace is the
// org's default landing workspace (see `getDefaultAuthenticatedPath`).
// ---------------------------------------------------------------------------

/** Build `/:org/base?view=<view>&<preserved-query>` from a route's location. */
function buildWorkspaceViewRedirect(
  org: string | undefined,
  view:
    | "settings"
    | "workers"
    | "members"
    | "approvals"
    | "approval-definitions"
    | "github-app",
  search: string,
): string {
  // `:org` is always present on these routes; fall back defensively to root.
  if (!org) return "/";
  const params = new URLSearchParams(search);
  // The `view` param is reserved for tab selection — overwrite any stale value.
  params.set("view", view);
  return `/${org}/base?${params.toString()}`;
}

const WorkspaceSettingsRedirect: React.FC = () => {
  const { org } = useParams<{ org: string }>();
  const location = useLocation();
  return (
    <Navigate
      to={buildWorkspaceViewRedirect(org, "settings", location.search)}
      replace
    />
  );
};

const WorkerHostsRedirect: React.FC = () => {
  const { org } = useParams<{ org: string }>();
  const location = useLocation();
  return (
    <Navigate
      to={buildWorkspaceViewRedirect(org, "workers", location.search)}
      replace
    />
  );
};

const OrgMembersRedirect: React.FC = () => {
  const { org } = useParams<{ org: string }>();
  const location = useLocation();
  return (
    <Navigate
      to={buildWorkspaceViewRedirect(org, "members", location.search)}
      replace
    />
  );
};

const OrgApprovalsRedirect: React.FC = () => {
  const { org } = useParams<{ org: string }>();
  const location = useLocation();
  // The definitions LIBRARY moved to `?view=approval-definitions` when the
  // workspace Approvals tab took `?view=approvals` (B6 PR-2, user decision).
  return (
    <Navigate
      to={buildWorkspaceViewRedirect(
        org,
        "approval-definitions",
        location.search,
      )}
      replace
    />
  );
};

const OrgGithubAppRedirect: React.FC = () => {
  const { org } = useParams<{ org: string }>();
  const location = useLocation();
  // Preserves `githubAppConnection=connected|failed` from the GitHub callback.
  return (
    <Navigate
      to={buildWorkspaceViewRedirect(org, "github-app", location.search)}
      replace
    />
  );
};

// ---------------------------------------------------------------------------
// Loading Fallback
// ---------------------------------------------------------------------------

const PageLoadingFallback: React.FC = () => (
  <div className="h-screen w-full flex items-center justify-center bg-[var(--g5-bg0)]">
    <div className="flex flex-col items-center gap-3">
      <div
        className="w-8 h-8 border-2 border-[var(--g5-text2)] border-t-[var(--g5-info)] rounded-full animate-spin"
        role="status"
        aria-label="Loading page"
      />
      <span className="text-sm text-[var(--g5-text2)]">Loading…</span>
    </div>
  </div>
);

const NO_ORGANIZATION_PATH = "/no-organization";

/**
 * The organization the user is entering: the active organization from the JWT
 * session, falling back to their first membership, or null when the account
 * has no organization. This is the org whose Home sign-in lands on, and the one
 * the after-login Connect GitHub offer is decided against
 * (g5.settings.github-connection).
 */
function enteredOrg(
  user: CurrentUserDTO,
): CurrentUserDTO["orgs"][number] | null {
  if (user.orgs.length === 0) return null;
  return user.activeOrgId
    ? (user.orgs.find((o) => o.orgId === user.activeOrgId) ?? user.orgs[0])
    : user.orgs[0];
}

function getDefaultAuthenticatedPath(user: CurrentUserDTO): string {
  const postAuthRedirect = getPostAuthRedirect();
  if (postAuthRedirect) {
    return postAuthRedirect;
  }

  const org = enteredOrg(user);
  if (org) {
    // Sign-in lands on the organization Home (ui g5.ui.sign-in-landing),
    // where the onboarding tour starts; a preserved route still wins above.
    return `/${org.orgSlug}/home`;
  }

  // Organizations are created only by platform operators
  // (g5.organization.creation), so an account without one needs an invitation.
  return NO_ORGANIZATION_PATH;
}

/** The dedicated app origin opens login or the user's existing app destination. */
const AppRootRedirect: React.FC = () => {
  const { user, isLoading } = useAuth();
  if (isLoading) return <PageLoadingFallback />;
  return (
    <Navigate
      to={user ? getDefaultAuthenticatedPath(user) : "/login"}
      replace
    />
  );
};

// ---------------------------------------------------------------------------
// LoginPage
// ---------------------------------------------------------------------------

// Dev-login personas, shown only when the instance enables dev login
// (G5_E2E_AUTH). They mirror the OAuth stub roster in
// infra/docker/docker-compose.dev.yml, whose names and org roles
// scripts/seed-dev-org.sh seeds.
const DEV_LOGIN_USERS = [
  {
    email: "demo@g5labs.ai",
    label: "Dev Login as Alex (Admin)",
    testId: "login-dev-login",
  },
  {
    email: "member@g5labs.ai",
    label: "Dev Login as Jordan (Member)",
    testId: "login-dev-login-member",
  },
  {
    email: "viewer@g5labs.ai",
    label: "Dev Login as Vera (Viewer)",
    testId: "login-dev-login-viewer",
  },
] as const;

/**
 * The after-login Connect GitHub offer (g5.settings.github-connection). The
 * server decides it for the organization being entered: offered only to a
 * disconnected person who is more than a viewer of an organization that uses
 * GitHub. Any other answer, including an error, continues to the destination,
 * so sign-in never waits on GitHub. "Continue without connecting" preserves the
 * intended destination.
 */
const AfterLoginGithubOffer: React.FC<{
  orgId: string;
  destination: string;
}> = ({ orgId, destination }) => {
  const offer = useGithubAfterLoginOffer(orgId);
  if (offer.isPending) return <PageLoadingFallback />;
  if (offer.data?.offerAfterLogin !== true) {
    return <Navigate to={destination} replace />;
  }
  return (
    <main className="mx-auto max-w-lg px-6 py-12">
      <GithubConnectionCard afterLogin returnTo={destination} />
    </main>
  );
};

/**
 * Login page at /login showing G5 brand and only the OAuth providers
 * configured on the current instance.
 * If the user is already authenticated, redirects to a preserved app route
 * or their default workspace.
 */
export const LoginPage: React.FC = () => {
  const { user } = useAuth();
  const location = useLocation();
  const instanceInfo = useInstanceInfo();
  const from = (
    location.state as {
      from?: { pathname: string; search?: string; hash?: string };
    }
  )?.from;
  const returnTo = from
    ? `${from.pathname}${from.search ?? ""}${from.hash ?? ""}`
    : null;
  const preservedReturnTo = getPostAuthRedirect();
  const effectiveReturnTo = returnTo ?? preservedReturnTo;

  const authProviders =
    instanceInfo.status === "ok"
      ? AUTH_PROVIDER_ORDER.filter((provider) =>
          instanceInfo.data.authProviders.some(
            (candidate) =>
              isAuthProviderId(candidate) && candidate === provider,
          ),
        )
      : [];
  const devLoginEnabled =
    instanceInfo.status === "ok"
      ? instanceInfo.data.devLoginEnabled
      : isDevMode();

  // If already authenticated, redirect to the original page or /
  if (user) {
    // Resume an explicitly requested connection page itself. Wrapping that
    // page in another connection offer would lose its nested returnTo.
    if (isGithubConnectionPath(effectiveReturnTo)) {
      return <Navigate to={effectiveReturnTo!} replace />;
    }
    if (instanceInfo.status === "loading") return <PageLoadingFallback />;
    const destination = effectiveReturnTo ?? getDefaultAuthenticatedPath(user);
    // Ask about the shared GitHub connection only when the deployment
    // advertises the App and the user is entering an organization
    // (g5.settings.github-connection); no organization means no offer.
    if (instanceInfo.status === "ok" && isGithubAppEnabled(instanceInfo.data)) {
      const org = enteredOrg(user);
      if (org) {
        return (
          <AfterLoginGithubOffer
            key={org.orgId}
            orgId={org.orgId}
            destination={destination}
          />
        );
      }
    }
    return <Navigate to={destination} replace />;
  }

  return (
    <AppShell>
      <div className="flex-1 flex items-center justify-center">
        <div className="w-full max-w-sm space-y-8 px-6">
          {/* Brand */}
          <div className="text-center">
            <h1 className="text-4xl font-bold text-[var(--g5-text0)]">G5</h1>
            <p className="mt-2 text-sm text-[var(--g5-text2)]">
              Ontology-centric development environment
            </p>
          </div>

          {/* OAuth Provider Buttons */}
          <div className="space-y-3">
            {authProviders.map((provider) => (
              <a
                key={provider}
                data-testid="login-provider-link"
                href={getAuthProviderHref(provider)}
                onClick={() => rememberPostAuthRedirect(effectiveReturnTo)}
                className="w-full flex items-center justify-center gap-3 px-4 py-3 text-sm font-medium rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] text-[var(--g5-text0)] hover:bg-[var(--g5-bg2)] transition-colors cursor-pointer"
                style={{ borderWidth: "var(--g5-border-w)" }}
              >
                <AuthProviderIcon provider={provider} className="w-5 h-5" />
                {`Continue with ${getAuthProviderLabel(provider)}`}
              </a>
            ))}

            {instanceInfo.status === "ok" &&
              authProviders.length === 0 &&
              !devLoginEnabled && (
                <div
                  className="rounded-[var(--g5-radius-sm)] border px-4 py-3 text-sm"
                  style={{
                    borderColor: "var(--g5-stroke)",
                    backgroundColor: "var(--g5-bg1)",
                    color: "var(--g5-text2)",
                  }}
                >
                  No sign-in providers are configured for this instance yet.
                  Contact your administrator to request access.
                </div>
              )}
          </div>

          {/* Dev Login — only rendered when the instance explicitly enables it */}
          {devLoginEnabled && (
            <div className="pt-4 border-t border-[var(--g5-stroke)] flex flex-col gap-2">
              {DEV_LOGIN_USERS.map(({ email, label, testId }) => (
                <button
                  key={email}
                  type="button"
                  data-testid={testId}
                  onClick={async () => {
                    try {
                      const res = await fetch("/api/v1/auth/test-token", {
                        method: "POST",
                        headers: { "Content-Type": "application/json" },
                        body: JSON.stringify({ email }),
                      });
                      if (!res.ok)
                        throw new Error("test-token endpoint not available");
                      const json = await res.json();
                      if (!json.data.refreshToken)
                        throw new Error("Dev login session unavailable");
                      // Redeem the fixture refresh token through the ordinary
                      // refresh endpoint, which installs the HttpOnly session
                      // cookie used by org switches and periodic renewal.
                      const refresh = await fetch("/api/v1/auth/refresh", {
                        method: "POST",
                        credentials: "include",
                        headers: { "Content-Type": "application/json" },
                        body: JSON.stringify({
                          refreshToken: json.data.refreshToken,
                        }),
                      });
                      if (!refresh.ok)
                        throw new Error("Dev login session unavailable");
                      const session = await refresh.json();
                      const jwt = session.data.jwt;
                      if (jwt) {
                        rememberPostAuthRedirect(effectiveReturnTo);
                        localStorage.setItem("g5_auth_token", jwt);
                        localStorage.setItem("g5_auth_email", email);
                        window.location.href = "/login";
                      }
                    } catch (e) {
                      console.error("Dev login failed:", e);
                    }
                  }}
                  className="w-full flex items-center justify-center gap-2 px-4 py-3 text-sm font-medium rounded-[var(--g5-radius-sm)] border border-dashed border-[var(--g5-stroke)] bg-transparent text-[var(--g5-text2)] hover:text-[var(--g5-text0)] hover:bg-[var(--g5-bg1)] transition-colors cursor-pointer"
                  style={{ borderWidth: "var(--g5-border-w)" }}
                >
                  🔧 {label}
                </button>
              ))}
            </div>
          )}

          {/* Footer. Terms of Service and Privacy Policy are the live g5labs.ai
              pages; they open in a new tab so the sign-in flow isn't lost. */}
          <p className="text-center text-xs text-[var(--g5-text2)]">
            By continuing, you agree to our{" "}
            <a
              href="https://g5labs.ai/terms"
              target="_blank"
              rel="noopener noreferrer"
              className="underline hover:text-[var(--g5-text0)]"
            >
              Terms of Service
            </a>{" "}
            and{" "}
            <a
              href="https://g5labs.ai/privacy"
              target="_blank"
              rel="noopener noreferrer"
              className="underline hover:text-[var(--g5-text0)]"
            >
              Privacy Policy
            </a>
            .
          </p>
        </div>
      </div>
    </AppShell>
  );
};

// ---------------------------------------------------------------------------
// ForbiddenPage
// ---------------------------------------------------------------------------

/**
 * Styled error page shown when the user lacks a required scope.
 * Displays "Access Denied" heading, message, and "Go Back" button.
 */
const ForbiddenPage: React.FC = () => {
  const navigate = useNavigate();

  const handleGoBack = () => {
    // Navigate to previous page, or fall back to /
    if (window.history.length > 1) {
      navigate(-1);
    } else {
      navigate("/");
    }
  };

  return (
    <AppShell>
      <TopBar>
        <span className="text-sm font-medium text-[var(--g5-text0)]">G5</span>
      </TopBar>
      <div className="flex-1 flex items-center justify-center">
        <div className="text-center space-y-4 px-6">
          <div
            className="inline-flex items-center justify-center w-16 h-16 rounded-full mb-2"
            style={{
              backgroundColor:
                "color-mix(in srgb, var(--g5-blocked) 10%, transparent)",
            }}
          >
            <svg
              className="w-8 h-8"
              style={{ color: "var(--g5-blocked)" }}
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
              strokeWidth={2}
              aria-hidden="true"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                d="M18.364 18.364A9 9 0 005.636 5.636m12.728 12.728A9 9 0 015.636 5.636m12.728 12.728L5.636 5.636"
              />
            </svg>
          </div>
          <h1 className="text-2xl font-bold text-[var(--g5-text0)]">
            Access Denied
          </h1>
          <p className="text-sm text-[var(--g5-text2)] max-w-md">
            You don't have permission to access this page. Contact your
            organization administrator if you believe this is an error.
          </p>
          <Button
            kind="secondary"
            data-testid="access-denied-go-back"
            onClick={handleGoBack}
          >
            Go Back
          </Button>
        </div>
      </div>
    </AppShell>
  );
};

/**
 * Read and clear the short-lived `g5_login_denied` context cookie set by the
 * OAuth callback (issue #1517). The value is the email the visitor just proved
 * control of; it is rendered as escaped text only (never as markup), and the
 * cookie is cleared on read so a later visit does not show a stale address.
 */
function readAndClearDeniedEmail(): string | null {
  const match = document.cookie
    .split("; ")
    .find((c) => c.startsWith("g5_login_denied="));
  if (!match) {
    return null;
  }
  // Clear it immediately (Path must match the cookie the server set).
  document.cookie = "g5_login_denied=; Path=/login; Max-Age=0; SameSite=Lax";
  const raw = match.slice("g5_login_denied=".length);
  if (!raw) {
    return null;
  }
  try {
    const decoded = decodeURIComponent(raw.replace(/\+/g, " ")).trim();
    return decoded.length > 0 ? decoded : null;
  } catch {
    return null;
  }
}

const LoginDeniedPage: React.FC = () => {
  const [deniedEmail, setDeniedEmail] = useState<string | null>(null);
  // `readAndClearDeniedEmail` is destructive: it deletes the cookie on the
  // first call and returns null afterwards. React.StrictMode double-invokes
  // effects in development (main.tsx, routing D1/D16), so guard the read with
  // a ref to run it exactly once per mounted page — otherwise the second
  // invocation clears the state the first one captured. (issue #1517)
  const hasReadDeniedEmail = useRef(false);

  useEffect(() => {
    if (hasReadDeniedEmail.current) {
      return;
    }
    hasReadDeniedEmail.current = true;
    setDeniedEmail(readAndClearDeniedEmail());
  }, []);

  return (
    <AppShell>
      <div className="flex-1 flex items-center justify-center">
        <div className="w-full max-w-lg space-y-6 px-6 text-center">
          <div
            className="mx-auto inline-flex h-16 w-16 items-center justify-center rounded-full"
            style={{
              backgroundColor:
                "color-mix(in srgb, var(--g5-blocked) 10%, transparent)",
            }}
          >
            <svg
              className="h-8 w-8"
              style={{ color: "var(--g5-blocked)" }}
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
              strokeWidth={2}
              aria-hidden="true"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                d="M12 9v4m0 4h.01M4.93 19h14.14c1.54 0 2.5-1.67 1.73-3L13.73 3c-.77-1.33-2.69-1.33-3.46 0L3.2 16c-.77 1.33.19 3 1.73 3z"
              />
            </svg>
          </div>
          <div className="space-y-3">
            <h1 className="text-2xl font-bold text-[var(--g5-text0)]">
              Invitation Required
            </h1>
            {/* Sign-in admits only an invited account or an organization
                member (g5.user.admission). */}
            {deniedEmail ? (
              <p
                className="text-sm leading-6 text-[var(--g5-text2)]"
                data-testid="login-denied-email"
              >
                You signed in as{" "}
                <span className="font-semibold text-[var(--g5-text0)]">
                  {deniedEmail}
                </span>
                , which has no G5 invitation or organization membership. If that
                isn&apos;t the account you meant to use, sign in with a
                different one.
              </p>
            ) : (
              <p className="text-sm leading-6 text-[var(--g5-text2)]">
                Your account authenticated successfully, but it has no G5
                invitation or organization membership.
              </p>
            )}
            <p className="text-sm leading-6 text-[var(--g5-text2)]">
              Ask an organization administrator to invite this email address,
              then sign in again.
            </p>
          </div>
          <div className="flex items-center justify-center gap-3">
            <Button
              kind="primary"
              data-testid="no-org-back-to-sign-in"
              onClick={() => {
                window.location.href = "/login";
              }}
            >
              Back to Sign In
            </Button>
            <Button
              kind="secondary"
              data-testid="login-denied-different-account"
              onClick={() => {
                window.location.href = "/login";
              }}
            >
              Sign in with a different account
            </Button>
          </div>
        </div>
      </div>
    </AppShell>
  );
};

const CliAuthorizePage: React.FC = () => {
  const location = useLocation();
  const userCode = useMemo(() => {
    const value = new URLSearchParams(location.search).get("userCode");
    return value?.trim() ?? "";
  }, [location.search]);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [decision, setDecision] = useState<"approved" | "denied" | null>(null);
  const [error, setError] = useState<string | null>(null);

  const submitDecision = async (approve: boolean) => {
    if (!userCode || isSubmitting) {
      return;
    }

    setIsSubmitting(true);
    setError(null);
    try {
      const response = await api.post<{ status: string }>(
        routes.auth.cliApprove,
        { userCode, approve },
      );
      if (response.status === "approved") {
        setDecision("approved");
      } else if (response.status === "denied") {
        setDecision("denied");
      } else {
        setError(
          `This CLI authorization is ${response.status}. Return to your terminal and run auth login again if needed.`,
        );
      }
    } catch (err) {
      if (err instanceof ApiError) {
        if (err.isAuthError) {
          rememberPostAuthRedirect(
            `${location.pathname}${location.search}${location.hash}`,
          );
          setError(
            "Your browser session expired. Sign in again, then return to approve this CLI request.",
          );
          return;
        }
        setError(err.message);
      } else if (err instanceof Error) {
        setError(err.message);
      } else {
        setError("Unable to authorize this CLI request.");
      }
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <AppShell>
      <div className="flex-1 flex items-center justify-center px-6 py-12">
        <div
          className="w-full max-w-md rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] p-6 shadow-sm"
          style={{ borderWidth: "var(--g5-border-w)" }}
        >
          <div className="space-y-5">
            <div className="space-y-2">
              <h1 className="text-xl font-semibold text-[var(--g5-text0)]">
                Authorize G5 CLI
              </h1>
              <p className="text-sm leading-6 text-[var(--g5-text2)]">
                Compare this code with the one shown in your terminal before
                approving.
              </p>
            </div>

            {userCode ? (
              <div className="rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-4 py-3">
                <div className="text-xs uppercase text-[var(--g5-text2)]">
                  Code
                </div>
                <div className="mt-1 font-mono text-2xl font-semibold tracking-normal text-[var(--g5-text0)]">
                  {userCode}
                </div>
              </div>
            ) : (
              <div
                role="alert"
                className="rounded-[var(--g5-radius-sm)] border border-[var(--g5-blocked)] px-4 py-3 text-sm text-[var(--g5-text0)]"
                style={{
                  backgroundColor:
                    "color-mix(in srgb, var(--g5-blocked) 10%, transparent)",
                }}
              >
                This authorization link is missing a user code.
              </div>
            )}

            {decision && (
              <div
                role="status"
                className="rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-4 py-3 text-sm text-[var(--g5-text1)]"
              >
                {decision === "approved"
                  ? "CLI authorization approved. Return to your terminal."
                  : "CLI authorization denied. Return to your terminal."}
              </div>
            )}

            {error && (
              <div
                role="alert"
                className="rounded-[var(--g5-radius-sm)] border border-[var(--g5-blocked)] px-4 py-3 text-sm text-[var(--g5-text0)]"
                style={{
                  backgroundColor:
                    "color-mix(in srgb, var(--g5-blocked) 10%, transparent)",
                }}
              >
                {error}
              </div>
            )}

            <div className="flex flex-col gap-2 sm:flex-row">
              <Button
                kind="primary"
                data-testid="cli-authorize-approve"
                onClick={() => void submitDecision(true)}
                disabled={!userCode || isSubmitting || decision !== null}
                className="flex-1"
              >
                <Check className="h-4 w-4" aria-hidden="true" />
                Approve
              </Button>
              <Button
                kind="secondary"
                data-testid="cli-authorize-deny"
                onClick={() => void submitDecision(false)}
                disabled={!userCode || isSubmitting || decision !== null}
                className="flex-1"
              >
                <X className="h-4 w-4" aria-hidden="true" />
                Deny
              </Button>
            </div>
          </div>
        </div>
      </div>
    </AppShell>
  );
};

// ---------------------------------------------------------------------------
// NoOrganizationPage
// ---------------------------------------------------------------------------

/**
 * Landing for a signed-in account with no organization: people cannot create
 * their own (g5.organization.creation), so it names the account and asks for
 * an invitation. Only an account whose memberships are all inactive reaches
 * it, since sign-in admits only invited people and members (g5.user.admission).
 */
const NoOrganizationPage: React.FC = () => {
  const { user, logout } = useAuth();
  if (user && user.orgs.length > 0) {
    // An organization arrived since the redirect; open it, as the root does.
    const activeOrg =
      user.orgs.find((o) => o.orgId === user.activeOrgId) ?? user.orgs[0];
    return <Navigate to={`/${activeOrg.orgSlug}/base`} replace />;
  }

  return (
    <AppShell>
      <div className="flex-1 flex items-center justify-center">
        <div
          className="w-full max-w-lg space-y-6 px-6 text-center"
          data-testid="no-organization"
        >
          <div className="space-y-3">
            <h1 className="text-2xl font-bold text-[var(--g5-text0)]">
              No Organization Yet
            </h1>
            <p
              className="text-sm leading-6 text-[var(--g5-text2)]"
              data-testid="no-organization-email"
            >
              You&apos;re signed in as{" "}
              <span className="font-semibold text-[var(--g5-text0)]">
                {user?.email ?? "this account"}
              </span>
              , which isn&apos;t an active member of any organization.
            </p>
            <p className="text-sm leading-6 text-[var(--g5-text2)]">
              Ask an organization administrator for an invitation, then sign in
              again. If this isn&apos;t the account you meant to use, sign in
              with a different one.
            </p>
          </div>
          <div className="flex items-center justify-center gap-3">
            <Button
              kind="secondary"
              data-testid="no-organization-different-account"
              onClick={() => void logout()}
            >
              Sign in with a different account
            </Button>
          </div>
        </div>
      </div>
    </AppShell>
  );
};

// ---------------------------------------------------------------------------
// LandingRedirect
// ---------------------------------------------------------------------------

/**
 * Redirect /landing to the static coming-soon page so public traffic sees the
 * launch page while the authenticated app remains unchanged.
 */
const LandingRedirect: React.FC = () => {
  useEffect(() => {
    window.location.replace("/coming-soon.html");
  }, []);

  return <PageLoadingFallback />;
};

// ---------------------------------------------------------------------------
// WebSocket-enabled route wrapper
// ---------------------------------------------------------------------------

/**
 * Inner component that has access to QueryClient (from QueryClientProvider
 * mounted in main.tsx) and sets up the WebSocketProvider for all routes.
 */
const AppRoutes: React.FC = () => {
  const queryClient = useQueryClient();
  const location = useLocation();
  const workspaceRoute = isWorkspaceShellRoute(location.pathname);
  const instanceInfo = useInstanceInfo();
  const showPreProductionBanner =
    instanceInfo.status === "ok" && instanceInfo.data.selfHosted;

  const wsManager = useMemo(() => {
    const apiUrl =
      (import.meta.env.VITE_API_URL as string | undefined)?.trim() ||
      (typeof window !== "undefined" ? window.location.origin : "");
    return new WebSocketManager({
      getBaseUrl: () => apiUrl,
      getWsTicket: async () => {
        // Reuse token refresh; a failed request reaches the socket's backoff.
        const { ticket } = await api.post<{ ticket: string }>(
          "/api/v1/auth/ws-ticket",
          {},
          { principal: tokenPrincipal(getApiToken()) },
        );
        return ticket;
      },
      queryClient,
    });
  }, [queryClient]);

  return (
    <WebSocketProvider manager={wsManager}>
      <PlatformIssueReportProvider
        // platform_reports g5.quality.platform-reports.trigger: the floating
        // fallback is offered everywhere but renders only while no chrome
        // trigger is mounted or an overlay at the modal tier covers it.
        // `isWorkspaceRoute` governs the privacy-safe slug context.
        showShellTrigger
        isWorkspaceRoute={workspaceRoute}
      >
        <SelfHostedPreProductionBannerProvider
          enabled={showPreProductionBanner}
        >
          {/* ui g5.ui.onboarding-tour: one controller beside the routes
              follows the new person across pages. */}
          <OnboardingTour />
          <Routes>
            {/* Public routes */}
            <Route path="/login" element={<LoginPage />} />
            <Route path="/login/denied" element={<LoginDeniedPage />} />
            <Route path="/landing" element={<LandingRedirect />} />

            {/* Authenticated routes */}
            <Route
              path="/account/github"
              element={
                <RequireAuth>
                  <GithubConnectionPage />
                </RequireAuth>
              }
            />
            <Route
              path="/cli/authorize"
              element={
                <RequireAuth>
                  <CliAuthorizePage />
                </RequireAuth>
              }
            />
            <Route
              path="/forbidden"
              element={
                <RequireAuth>
                  <ForbiddenPage />
                </RequireAuth>
              }
            />

            {/* App entry: resolve the session before choosing its destination. */}
            <Route path="/" element={<AppRootRedirect />} />

            <Route
              path="/:org"
              element={
                <RequireAuth>
                  <OrganizationHomeRedirect />
                </RequireAuth>
              }
            />

            {/* Organization Home — the page synchronizes the route organization
              into the JWT before organization-scoped reads. */}
            <Route
              path="/:org/home"
              element={
                <RequireAuth>
                  <HomePage />
                </RequireAuth>
              }
            />

            {/* An issue's own page (Home's Create lands here): GitHub, and
              the native G5 tracker's, where members also change it. */}
            <Route
              path={ISSUE_PAGE_PATH}
              element={
                <RequireAuth>
                  <IssuePage />
                </RequireAuth>
              }
            />
            <Route
              path={NATIVE_ISSUE_PAGE_PATH}
              element={
                <RequireAuth>
                  <IssuePage />
                </RequireAuth>
              }
            />

            <Route
              path={APPROVAL_REPORT_PATH}
              element={
                <RequireAuth>
                  <ApprovalReportPage />
                </RequireAuth>
              }
            />

            {/* Personal workspace routes (personal_workspace
            g5.personal.routes): `/:org/me` and `/:org/users/:handle` open
            the workspace page resolved by registry kind, never by name.
            Registered before the name routes so they win equal ranks. */}
            {PERSONAL_SHELL_ROUTE_PATTERNS.map(({ family, path }) => (
              <Route
                key={path}
                path={path}
                element={
                  <RequireAuth>
                    <WorkspacePage key={family} routeFamily={family} />
                  </RequireAuth>
                }
              />
            ))}

            {/* Workspace routes: the page's routes come from one registry
            (lib/shellRoutes.ts), shared with the shell classification. */}
            {WORKSPACE_SHELL_ROUTE_PATTERNS.map((path) => (
              <Route
                key={path}
                path={path}
                element={
                  <RequireAuth>
                    <WorkspacePage />
                  </RequireAuth>
                }
              />
            ))}

            {/* Org members — redirects into the workspace; the Members surface now
            renders as a center-panel tab (see OrgMembersRedirect). */}
            <Route
              path="/:org/settings/members"
              element={
                <RequireAuth>
                  <OrgMembersRedirect />
                </RequireAuth>
              }
            />

            {/* Org worker hosts — redirects into the workspace; the Workers surface
            now renders as a center-panel tab (see WorkerHostsRedirect). */}
            <Route
              path="/:org/settings/workers"
              element={
                <RequireAuth>
                  <WorkerHostsRedirect />
                </RequireAuth>
              }
            />

            {/* Org approval definitions — redirects into the workspace; the
            Approvals surface renders as a center-panel tab (see
            OrgApprovalsRedirect). */}
            <Route
              path="/:org/settings/approvals"
              element={
                <RequireAuth>
                  <OrgApprovalsRedirect />
                </RequireAuth>
              }
            />

            {/* Org GitHub App binding — redirects into the workspace; the GitHub
            App surface renders as a center-panel tab. The API's GitHub
            callback returns the browser here. */}
            <Route
              path="/:org/settings/github-app"
              element={
                <RequireAuth>
                  <OrgGithubAppRedirect />
                </RequireAuth>
              }
            />

            {/* Workspace settings — theme/editor/model/SCM + cloud-gated org config.
            Now renders as a center-panel tab inside the workspace chrome; this
            standalone route redirects into the org's base workspace with
            `?view=settings` (see WorkspaceSettingsRedirect), preserving any SCM
            OAuth callback query.
            NOTE: `settings` is a reserved org-level route segment — it is ranked
            above `/:org/:workspace`, so a workspace literally named `settings`
            would be shadowed here. This matches the
            `/:org/settings/members` and `/:org/settings/workers` routes, which
            reserve the same segment. The collision cannot occur: the org-level
            segments are reserved server-side (`is_reserved_workspace_name` in
            g5-api workspace_service) and rejected as workspace names at create,
            init, and rename. */}
            <Route
              path="/:org/settings"
              element={
                <RequireAuth>
                  <WorkspaceSettingsRedirect />
                </RequireAuth>
              }
            />

            {/* Legacy `/settings` — redirect to active org's settings page */}
            <Route
              path="/settings"
              element={
                <RequireAuth>
                  <SettingsRedirect />
                </RequireAuth>
              }
            />

            {/* Projects / workspace listing */}
            <Route
              path="/projects"
              element={
                <RequireAuth>
                  <ProjectsPage />
                </RequireAuth>
              }
            />

            <Route
              path={NO_ORGANIZATION_PATH}
              element={
                <RequireAuth>
                  <NoOrganizationPage />
                </RequireAuth>
              }
            />

            {/* Organization fork picker — the tab strip's "+" (workbench
              g5.workbench.tabs), scoped to the organization in its route;
              `fork` is a reserved segment like `home`. */}
            <Route
              path={FORK_PICKER_ROUTE}
              element={
                <RequireAuth>
                  <ForkPickerPage />
                </RequireAuth>
              }
            />

            {/* Unknown routes fall back to the public launch surface */}
            <Route path="*" element={<LandingRedirect />} />
          </Routes>
        </SelfHostedPreProductionBannerProvider>
      </PlatformIssueReportProvider>
    </WebSocketProvider>
  );
};

// ---------------------------------------------------------------------------
// TelemetryBridge
// ---------------------------------------------------------------------------

/**
 * Thin wiring for the UI interaction telemetry stream (issue #1552,
 * `software_architecture/client/telemetry` [D4]). Mounted inside both
 * BrowserRouter (for useLocation) and AuthProvider (so it lives with the
 * authenticated app lifecycle): starts/stops the telemetry module, hands it
 * the Settings-page preference values for the per-identity snapshot
 * (operations g5.platform.client-telemetry) and forwards route changes —
 * pathname or `view` query param — as navigation events. Renders nothing.
 */
const TelemetryBridge: React.FC = () => {
  const location = useLocation();

  // startTelemetry is idempotent under StrictMode double-invoke and returns
  // its own teardown.
  useEffect(
    () =>
      startTelemetry({
        preferences: () => preferenceTelemetryValues(readUserSettings()),
      }),
    [],
  );

  useEffect(() => {
    const route = telemetryRoute(location.pathname, location.search);
    recordNavigation(route.path, route.view);
  }, [location.pathname, location.search]);

  return null;
};

// ---------------------------------------------------------------------------
// App Component
// ---------------------------------------------------------------------------

/**
 * Root application component.
 * Wraps the entire app in BrowserRouter → AuthProvider → AppRoutes.
 * QueryClientProvider is mounted in main.tsx (above this component).
 */
const App: React.FC = () => {
  return (
    <BrowserRouter>
      <AuthProvider>
        <TelemetryBridge />
        {/* Open workspaces as tabs above every shell page (workbench
            g5.workbench.tabs); inert outside the alignment layout. */}
        <WorkspaceTabsProvider>
          <AppRoutes />
        </WorkspaceTabsProvider>
      </AuthProvider>
    </BrowserRouter>
  );
};

export default App;
