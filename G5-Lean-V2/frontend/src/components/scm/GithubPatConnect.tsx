// =============================================================================
// GithubPatConnect — guided fine-grained Personal Access Token entry
// =============================================================================
// Replaces the SCM OAuth App redirect for GitHub repo-access connect. The user
// generates a fine-grained PAT on GitHub (guided), pastes it here, and the
// widget POSTs it to /api/v1/scm/github/connect for server-side validation and
// encrypted storage.
//
// SOT: scm_pat_connect.pm_view.md I3/I4/I5; g5/client/data-layer/scm-oauth.
// =============================================================================

import { useState, type ReactElement } from "react";
import { ExternalLink, Loader2 } from "lucide-react";

import { api, ApiError } from "../../lib/api";
import { Button } from "../ui/UI";

/** Result of a successful connect (POST /api/v1/scm/github/connect). */
export interface ScmConnectResult {
  provider: string;
  providerUsername: string;
  scopes: string[];
  connectedAt: string;
  returnTo: string;
}

/** GitHub's fine-grained PAT creation page (opened in a new tab). */
const FINE_GRAINED_PAT_URL =
  "https://github.com/settings/personal-access-tokens/new";

export interface GithubPatConnectProps {
  /** Called after a successful connect so the parent can refresh its state. */
  onConnected: (result: ScmConnectResult) => void | Promise<void>;
  className?: string;
}

/** Map a connect failure to an actionable, secret-free message. */
function patErrorMessage(err: unknown): string {
  if (err instanceof ApiError) {
    const detail = err.details?.message;
    if (typeof detail === "string" && detail.length > 0) {
      return detail;
    }
    if (err.is("VALIDATION_ERROR") || err.isAuthError) {
      return "GitHub rejected that token. Paste a valid, unexpired fine-grained PAT with Contents: Read and write.";
    }
    return err.message;
  }
  return err instanceof Error ? err.message : "Failed to connect GitHub.";
}

/**
 * Guided fine-grained PAT entry widget for connecting GitHub repo access.
 */
export function GithubPatConnect({
  onConnected,
  className,
}: GithubPatConnectProps): ReactElement {
  const [token, setToken] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const handleConnect = async () => {
    const trimmed = token.trim();
    if (!trimmed) {
      setError("Paste a fine-grained Personal Access Token to connect.");
      return;
    }
    setError(null);
    setNotice(null);
    setSubmitting(true);
    try {
      const result = await api.post<ScmConnectResult>(
        "/api/v1/scm/github/connect",
        { token: trimmed },
      );
      setToken("");
      try {
        await onConnected(result);
      } catch (callbackErr) {
        // The connection itself succeeded; a failure in the parent's refresh
        // callback must not be reported as "Failed to connect GitHub" — but
        // it must not vanish either, or the surrounding UI looks stale for
        // no visible reason.
        console.warn(
          "GithubPatConnect: onConnected callback failed",
          callbackErr,
        );
        setNotice(
          "Connected, but refreshing the connection list failed — reload the page to see it.",
        );
      }
    } catch (err) {
      setError(patErrorMessage(err));
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div
      className={`space-y-3 rounded-md border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] p-4 ${
        className ?? ""
      }`}
    >
      <div className="space-y-1">
        <p className="text-sm font-medium text-[var(--g5-text0)]">
          Connect GitHub with a fine-grained token
        </p>
        <p className="text-xs text-[var(--g5-text1)]">
          G5 uses a fine-grained Personal Access Token to clone, fetch, and push
          your tracked branches. Generate one on GitHub, then paste it below.
        </p>
      </div>

      <ol className="list-decimal space-y-1 pl-5 text-xs text-[var(--g5-text1)]">
        <li>
          <a
            href={FINE_GRAINED_PAT_URL}
            data-testid="github-pat-generate-token-link"
            target="_blank"
            rel="noopener noreferrer"
            className="inline-flex items-center gap-1 text-[var(--g5-info)] hover:underline"
          >
            Generate token on GitHub
            <ExternalLink className="h-3 w-3" aria-hidden="true" />
          </a>
        </li>
        <li>
          Set <strong>Resource owner</strong> to the org/user that owns the
          repos, and choose the repositories to grant.
        </li>
        <li>
          Under <strong>Permissions</strong>, grant{" "}
          <strong>Contents: Read and write</strong> and{" "}
          <strong>Metadata: Read</strong>.
        </li>
        <li>
          Copy the token (starts with <code>github_pat_</code>) and paste it
          here.
        </li>
      </ol>

      <input
        type="password"
        value={token}
        onChange={(e) => setToken(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && !submitting) {
            void handleConnect();
          }
        }}
        placeholder="github_pat_…"
        autoComplete="off"
        spellCheck={false}
        aria-label="GitHub fine-grained personal access token"
        className="w-full rounded-md border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-3 py-2 font-mono text-sm text-[var(--g5-text0)] placeholder:text-[var(--g5-text2)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]"
      />

      {error && (
        <p className="text-xs text-[var(--g5-blocked)]" role="alert">
          {error}
        </p>
      )}

      {notice && (
        <p className="text-xs text-[var(--g5-warn)]" role="status">
          {notice}
        </p>
      )}

      <Button
        kind="primary"
        data-testid="github-pat-connect"
        disabled={submitting || token.trim().length === 0}
        onClick={() => void handleConnect()}
      >
        {submitting ? (
          <span className="flex items-center gap-2">
            <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" />
            Connecting…
          </span>
        ) : (
          "Connect GitHub"
        )}
      </Button>
    </div>
  );
}
