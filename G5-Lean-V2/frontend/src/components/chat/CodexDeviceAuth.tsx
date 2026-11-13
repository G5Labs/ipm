import { useEffect, useLayoutEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Copy } from "lucide-react";
import { api } from "../../lib/api";
import { writeClipboard } from "../../lib/clipboard";
import type { CodexAuthFailure } from "../../lib/types";

export interface CodexAuthView {
  taskId: string;
  attempt: number;
  phase: "checking" | "waiting" | "ready" | "superseded" | "ended";
  deadlineUnixMs: number | null;
  verificationUrl: string | null;
  userCode: string | null;
  message: string | null;
}

const AUTO_COPIED_PREFIX = "g5:codex-auth-auto-copied:";

interface AutoCopy {
  copied: Promise<boolean>;
  settled: boolean;
}
const autoCopies = new Map<string, AutoCopy>();

/**
 * This tab's one automatic copy of a challenge (chat
 * g5.chat.ui.device-code-copy). A remount, refetch or reload of the same
 * challenge must not overwrite the clipboard again. The session record names
 * only the session, task and attempt (one challenge per attempt), never the
 * code. A write still in flight is shared, so a panel remounted meanwhile
 * (Dock, Pop out) still reports its outcome; a settled one is not reported
 * again. Returns null when there is nothing to report.
 */
function autoCopyOnce(challengeKey: string, code: string): AutoCopy | null {
  const existing = autoCopies.get(challengeKey);
  if (existing) return existing.settled ? null : existing;
  try {
    const storageKey = AUTO_COPIED_PREFIX + challengeKey;
    if (window.sessionStorage.getItem(storageKey)) return null;
    window.sessionStorage.setItem(storageKey, "1");
  } catch {
    // Without session storage the in-memory record still covers remounts.
  }
  const copy: AutoCopy = { copied: writeClipboard(code), settled: false };
  void copy.copied.then(() => {
    copy.settled = true;
  });
  autoCopies.set(challengeKey, copy);
  return copy;
}

type CopyOutcome = "auto-copied" | "copied" | "auto-failed" | "failed";

/** Mounted only for the workspace owner. The API independently checks owner
 * identity; codes are never taken from the shared timeline or persisted here. */
export function CodexDeviceAuth({
  sessionId,
  terminalFailures,
  onLayoutChange,
}: {
  sessionId: string;
  terminalFailures?: readonly CodexAuthFailure[];
  onLayoutChange?: (visible: boolean) => void;
}) {
  const query = useQuery({
    queryKey: ["codex-device-auth", sessionId],
    queryFn: ({ signal }) =>
      api.get<CodexAuthView | null>(
        `/api/v1/sessions/${sessionId}/codex-auth`,
        { signal },
      ),
    refetchInterval: (query) => {
      const phase = query.state.data?.phase;
      // Keep discovering new turns while idle without polling normal owner
      // chats every second. Only an in-progress login needs the faster cadence.
      return phase === "checking" || phase === "waiting" ? 1_000 : 3_000;
    },
    staleTime: 0,
    gcTime: 0,
    retry: false,
  });
  const [now, setNow] = useState(Date.now);
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 1_000);
    return () => window.clearInterval(timer);
  }, []);
  const auth = query.data;
  // Terminal explanations live in the durable transcript. The private
  // query can lag that notice, so also settle a cached active card by identity.
  const visible =
    !!auth &&
    (auth.phase === "checking" || auth.phase === "waiting") &&
    !terminalFailures?.some(
      (failure) =>
        failure.taskId === auth.taskId && failure.attempt === auth.attempt,
    );
  const expired = auth?.deadlineUnixMs != null && now >= auth.deadlineUnixMs;
  const waiting =
    visible && auth?.phase === "waiting" && !expired && !query.isError;
  const challenge =
    waiting &&
    auth.userCode &&
    auth.verificationUrl === "https://auth.openai.com/codex/device"
      ? {
          key: `${sessionId}:${auth.taskId}:${auth.attempt}`,
          userCode: auth.userCode,
        }
      : null;
  const challengeKey = challenge?.key ?? null;
  const challengeCode = challenge?.userCode ?? null;
  const [copy, setCopy] = useState<{
    challengeKey: string;
    outcome: CopyOutcome;
  } | null>(null);
  const copyOutcome =
    copy && copy.challengeKey === challengeKey ? copy.outcome : null;
  // Copy a newly shown code once. A browser refuses clipboard writes from an
  // unfocused page, so a code that arrives while the person is elsewhere is
  // copied when they return.
  useEffect(() => {
    if (!challengeKey || !challengeCode) return;
    let active = true;
    const autoCopy = () => {
      const copy = autoCopyOnce(challengeKey, challengeCode);
      if (!copy) return;
      void copy.copied.then((copied) => {
        if (!active) return;
        // A manual copy made meanwhile is the newer outcome.
        setCopy((current) =>
          current?.challengeKey === challengeKey &&
          (current.outcome === "copied" || current.outcome === "failed")
            ? current
            : {
                challengeKey,
                outcome: copied ? "auto-copied" : "auto-failed",
              },
        );
      });
    };
    if (document.hasFocus()) {
      autoCopy();
    } else {
      window.addEventListener("focus", autoCopy, { once: true });
    }
    return () => {
      active = false;
      window.removeEventListener("focus", autoCopy);
    };
  }, [challengeKey, challengeCode]);
  // The confirmation is brief; a failure stays until the person copies.
  useEffect(() => {
    if (copyOutcome !== "auto-copied" && copyOutcome !== "copied") return;
    const timer = window.setTimeout(() => setCopy(null), 4_000);
    return () => window.clearTimeout(timer);
  }, [copy, copyOutcome]);
  async function copyCode() {
    if (!challengeKey || !challengeCode) return;
    const copied = await writeClipboard(challengeCode);
    setCopy({ challengeKey, outcome: copied ? "copied" : "failed" });
  }
  // Auth is deliberately outside the shared transcript. Notify its container
  // when its own query changes the layout so empty-state and sticky scrolling
  // do not depend on a provider event that cannot arrive until login finishes.
  useLayoutEffect(() => {
    onLayoutChange?.(visible);
  }, [
    onLayoutChange,
    visible,
    auth?.taskId,
    auth?.attempt,
    auth?.phase,
    auth?.userCode,
    auth?.verificationUrl,
    auth?.message,
    expired,
    query.isError,
  ]);
  if (!auth || !visible) return null;
  const remaining = Math.max(
    0,
    Math.ceil(((auth.deadlineUnixMs ?? now) - now) / 1_000),
  );
  return (
    <div
      role="status"
      data-testid="codex-device-auth"
      className="m-3 rounded-md border border-[var(--g5-stroke)] bg-[var(--g5-bg2)] p-3 text-sm text-[var(--g5-text0)]"
    >
      {challenge ? (
        <>
          <p>Sign in to Codex to continue with your message.</p>
          <p className="mt-2">
            1.{" "}
            <a
              href="https://auth.openai.com/codex/device"
              target="_blank"
              rel="noopener noreferrer"
              className="text-[var(--g5-info)] underline"
            >
              Open this link in your browser and sign in
            </a>
            .
          </p>
          <p className="mt-2">
            2. Enter this one-time code after you are signed in:
          </p>
          <div className="my-3 flex flex-wrap items-center gap-3">
            <p
              data-testid="codex-device-auth-code"
              className="font-mono text-lg font-semibold select-all"
            >
              {challenge.userCode}
            </p>
            <button
              type="button"
              data-testid="codex-device-auth-copy"
              onClick={() => void copyCode()}
              className="inline-flex items-center gap-1 rounded border border-[var(--g5-stroke)] px-2 py-1 text-xs text-[var(--g5-text1)] hover:bg-[var(--g5-bg1)]"
            >
              <Copy size={12} aria-hidden="true" />
              {copyOutcome === "auto-copied" || copyOutcome === "copied"
                ? "Copied"
                : "Copy code"}
            </button>
          </div>
          {copyOutcome && (
            <p
              data-testid="codex-device-auth-copy-status"
              className={
                copyOutcome === "auto-failed" || copyOutcome === "failed"
                  ? "mb-2 text-xs text-[var(--g5-warn)]"
                  : "mb-2 text-xs text-[var(--g5-ok)]"
              }
            >
              {copyOutcome === "auto-copied"
                ? "Code copied to your clipboard. Paste it after you sign in."
                : copyOutcome === "copied"
                  ? "Code copied to your clipboard."
                  : copyOutcome === "auto-failed"
                    ? "Couldn't copy the code automatically. Use Copy code, or select the code and copy it."
                    : "Couldn't copy the code. Select it and copy it manually."}
            </p>
          )}
          <p className="text-xs text-[var(--g5-text2)]">
            Waiting for sign-in — {Math.floor(remaining / 60)}:
            {String(remaining % 60).padStart(2, "0")} remaining. Messages you
            send meanwhile stay queued and run after this one.
          </p>
        </>
      ) : (
        <p>
          {auth.message ??
            (expired
              ? "Finishing Codex sign-in…"
              : query.isError
                ? "Reconnecting to Codex sign-in…"
                : "Checking Codex sign-in…")}
        </p>
      )}
    </div>
  );
}
