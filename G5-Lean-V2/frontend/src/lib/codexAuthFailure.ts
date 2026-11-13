import {
  compareTimelineEntries,
  sortedTimelineEntries,
} from "./providerOutput";
import type { ChatMessageDTO, CodexAuthFailure, TimelineEntry } from "./types";

const TERMINAL_CODES = new Set([
  "codex_auth_failed",
  "codex_auth_timeout",
  "codex_auth_disabled",
  "codex_auth_busy",
  "codex_auth_interrupted",
  "codex_auth_superseded",
]);

export function codexAuthFailureKey(failure: CodexAuthFailure): string {
  return `${failure.taskId}:${failure.attempt}`;
}

export function codexAuthFailureFromEntry(
  entry: TimelineEntry,
): CodexAuthFailure | null {
  const meta = entry.metadata;
  if (
    meta.providerEvent !== "provided_agent.error" ||
    meta.visibility === "internal" ||
    meta.visibility === "debug" ||
    (!meta.codexAuthFailure && !TERMINAL_CODES.has(String(meta.code))) ||
    typeof meta.taskId !== "string" ||
    typeof meta.attempt !== "number"
  )
    return null;
  return { taskId: meta.taskId, attempt: meta.attempt };
}

/** One public explanation per login attempt. The saved notice is authoritative
 * after settlement (including owner interruption); before it arrives, use the
 * latest journal notice. Identity, never matching text across turns, reconciles
 * replay and partial pages. Real provider output is left intact. */
export function reconcileCodexAuthFailures(
  messages: ChatMessageDTO[],
  entries: TimelineEntry[],
): TimelineEntry[] {
  const notices = new Map<string, TimelineEntry>();
  for (const entry of sortedTimelineEntries(entries)) {
    const failure = codexAuthFailureFromEntry(entry);
    if (failure) notices.set(codexAuthFailureKey(failure), entry);
  }
  for (const message of messages) {
    if (
      message.role !== "assistant" ||
      message.status !== "error" ||
      !message.codexAuthFailure ||
      !message.content.trim()
    )
      continue;
    const failure = message.codexAuthFailure;
    notices.set(codexAuthFailureKey(failure), {
      id: `codex-auth-failure-${message.id}`,
      entryType: "error",
      content: message.content,
      createdAt: message.createdAt,
      metadata: {
        provider: "provided_agent",
        providerEvent: "provided_agent.error",
        phase: "status",
        messageId: message.id,
        sessionId: message.sessionId,
        ...failure,
        codexAuthFailure: true,
        message: message.content,
      },
    });
  }
  return [
    ...entries.filter((entry) => !codexAuthFailureFromEntry(entry)),
    ...notices.values(),
  ];
}
