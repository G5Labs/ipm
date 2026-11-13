import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type Ref,
} from "react";
import {
  answersJsonByteLength,
  buildNextDocument,
  MAX_ANSWERS_JSON_BYTES,
} from "./htmlDocAnswerBlock";
import {
  EXPLAINER_CHAT_NAMING,
  explainerSaveInstructions,
} from "../explainer/explainerChatPrompt";
import { randomUuid } from "../../lib/randomUuid";
import { classNames } from "../../lib/utils";

export const HTML_DOC_MSG_SOURCE_CHILD = "g5-html-doc";
export const HTML_DOC_MSG_SOURCE_HOST = "g5-html-doc-host";
export const HTML_DOC_BRIDGE_VERSION = 1;
export const MAX_SECTION_TITLE_CHARS = 200;
export const MAX_SECTION_ID_CHARS = 64;

const APPLIED_BAR_AUTO_HIDE_MS = 5000;
const MAX_REQUEST_IDS_PER_GENERATION = 32;
const MAX_REQUEST_ID_CHARS = 64;

export type HtmlDocBridgeErrorCode =
  | "read-only"
  | "missing-block"
  | "ambiguous-block"
  | "too-large"
  | "bad-request"
  | "busy"
  | "conflict"
  | "internal";

export type ProposalStatus =
  | "received"
  | "applied"
  | "dismissed"
  | "superseded"
  | "error";

interface PendingProposal {
  requestId: string;
  answersJson: string;
  generation: string;
}

interface RequestEntry {
  payload: string;
  status: ProposalStatus;
}

export function sanitizeSectionText(raw: string): string {
  return (
    raw
      // eslint-disable-next-line no-control-regex -- stripping controls is intentional
      .replace(/[\u0000-\u001f\u007f`]/g, " ")
      .replace(/\s+/g, " ")
      .trim()
  );
}

export function explainDocSectionChatPrompt(
  docPath: string,
  sectionTitle: string,
  sectionId: string | null,
  workspaceId: string,
): string {
  const idPart = sectionId ? ` (section id \`${sectionId}\`)` : "";
  return (
    `Create an explainer for the section ` +
    `"${sectionTitle}"${idPart} of the interactive project doc \`${docPath}\`. ` +
    "Read the doc, gather its evidence, ask me the scope/style/format " +
    "questions, then generate the explainer.\n\n" +
    explainerSaveInstructions(workspaceId, EXPLAINER_CHAT_NAMING)
  );
}

export interface HtmlDocAnswerBridgeConfig {
  workspaceId: string;
  filePath: string | null;
  displayed: string;
  canSave: boolean;
  onSave: (content: string) => Promise<void>;
  onSeedChatPrompt?: (text: string) => void;
}

export type HtmlDocAnswerBarState =
  | {
      kind: "proposal";
      filePath: string;
      payloadBytes: number;
      errorCode: HtmlDocBridgeErrorCode | null;
    }
  | { kind: "applying" }
  | { kind: "applied" };

export interface HtmlDocAnswerBridge {
  frameProps?: { iframeRef: Ref<HTMLIFrameElement>; onLoad: () => void };
  bar: HtmlDocAnswerBarState | null;
  applyPending: () => void;
  dismissPending: () => void;
}

function mintGeneration(): string {
  return randomUuid();
}

export function useHtmlDocAnswerBridge(
  config: HtmlDocAnswerBridgeConfig | null,
): HtmlDocAnswerBridge {
  const iframeRef = useRef<HTMLIFrameElement | null>(null);
  const frameLoadedRef = useRef(false);
  const generationRef = useRef(mintGeneration());
  const requestsRef = useRef<Map<string, RequestEntry>>(new Map());
  const [pending, setPendingState] = useState<PendingProposal | null>(null);
  const pendingRef = useRef<PendingProposal | null>(null);
  const [bar, setBar] = useState<HtmlDocAnswerBarState | null>(null);
  const applyingRef = useRef(false);
  const hideTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const configRef = useRef(config);
  const enabled = Boolean(config?.filePath);
  const canSave = Boolean(config?.canSave && config.filePath);
  const canChatPrompt = Boolean(config?.onSeedChatPrompt);

  useEffect(() => {
    configRef.current = config;
  }, [config]);

  const setPending = useCallback((next: PendingProposal | null) => {
    pendingRef.current = next;
    setPendingState(next);
  }, []);

  const postToChild = useCallback((message: Record<string, unknown>) => {
    iframeRef.current?.contentWindow?.postMessage(
      {
        source: HTML_DOC_MSG_SOURCE_HOST,
        version: HTML_DOC_BRIDGE_VERSION,
        generation: generationRef.current,
        ...message,
      },
      "*",
    );
  }, []);

  const postStatus = useCallback(
    (
      requestId: string,
      status: ProposalStatus,
      errorCode?: HtmlDocBridgeErrorCode,
    ) => {
      const entry = requestsRef.current.get(requestId);
      if (entry) entry.status = status;
      postToChild({
        type: "proposal-status",
        requestId,
        status,
        ...(errorCode ? { errorCode } : {}),
      });
    },
    [postToChild],
  );

  const postCapabilities = useCallback(() => {
    const current = configRef.current;
    postToChild({
      type: "bridge-ready",
      canSave: Boolean(current?.canSave && current.filePath),
      canChatPrompt: Boolean(current?.onSeedChatPrompt),
    });
  }, [postToChild]);

  const handleFrameLoad = useCallback(() => {
    frameLoadedRef.current = true;
    generationRef.current = mintGeneration();
    requestsRef.current = new Map();
    setPending(null);
    if (!applyingRef.current) setBar(null);
    postCapabilities();
  }, [postCapabilities, setPending]);

  // A draft refresh can load the frame while Save is pending. Announce the
  // settled capability (and permission changes) without reloading its source
  // or resetting the generation and pending proposal.
  useEffect(() => {
    if (!enabled) {
      frameLoadedRef.current = false;
      return;
    }
    if (frameLoadedRef.current) postCapabilities();
  }, [enabled, canSave, canChatPrompt, postCapabilities]);

  useEffect(() => {
    if (!enabled) return;
    const onMessage = (event: MessageEvent) => {
      if (event.source !== iframeRef.current?.contentWindow) return;
      const data = event.data as Record<string, unknown> | null;
      if (
        !data ||
        data.source !== HTML_DOC_MSG_SOURCE_CHILD ||
        data.version !== HTML_DOC_BRIDGE_VERSION ||
        (data.type !== "propose-answers" &&
          data.type !== "propose-chat-prompt") ||
        data.generation !== generationRef.current
      ) {
        return;
      }
      const requestId = data.requestId;
      if (
        typeof requestId !== "string" ||
        requestId.length === 0 ||
        requestId.length > MAX_REQUEST_ID_CHARS
      ) {
        return;
      }
      const payload =
        data.type === "propose-answers"
          ? data.answersJson
          : `${String(data.sectionTitle)}\u0000${String(data.sectionId ?? "")}`;
      if (typeof payload !== "string") {
        postStatus(requestId, "error", "bad-request");
        return;
      }
      const prior = requestsRef.current.get(requestId);
      if (prior) {
        postStatus(
          requestId,
          prior.payload === payload ? prior.status : "error",
          prior.payload === payload ? undefined : "bad-request",
        );
        return;
      }
      if (
        applyingRef.current ||
        requestsRef.current.size >= MAX_REQUEST_IDS_PER_GENERATION
      ) {
        postStatus(requestId, "error", "busy");
        return;
      }
      const current = configRef.current;
      if (data.type === "propose-answers" && !current?.canSave) {
        postStatus(requestId, "error", "read-only");
        return;
      }
      requestsRef.current.set(requestId, { payload, status: "received" });

      if (data.type === "propose-chat-prompt") {
        if (
          !current?.onSeedChatPrompt ||
          typeof data.sectionTitle !== "string"
        ) {
          postStatus(requestId, "error", "bad-request");
          return;
        }
        const sectionTitle = sanitizeSectionText(data.sectionTitle);
        const sectionId =
          typeof data.sectionId === "string"
            ? sanitizeSectionText(data.sectionId) || null
            : data.sectionId == null
              ? null
              : undefined;
        if (
          !sectionTitle ||
          sectionTitle.length > MAX_SECTION_TITLE_CHARS ||
          sectionId === undefined ||
          (sectionId != null && sectionId.length > MAX_SECTION_ID_CHARS)
        ) {
          postStatus(requestId, "error", "bad-request");
          return;
        }
        current.onSeedChatPrompt(
          explainDocSectionChatPrompt(
            current.filePath ?? "",
            sectionTitle,
            sectionId,
            current.workspaceId,
          ),
        );
        postStatus(requestId, "applied");
        return;
      }

      if (answersJsonByteLength(payload) > MAX_ANSWERS_JSON_BYTES) {
        postStatus(requestId, "error", "too-large");
        return;
      }
      try {
        JSON.parse(payload);
      } catch {
        postStatus(requestId, "error", "bad-request");
        return;
      }
      if (pendingRef.current) {
        postStatus(pendingRef.current.requestId, "superseded");
      }
      const proposal = {
        requestId,
        answersJson: payload,
        generation: generationRef.current,
      };
      setPending(proposal);
      setBar({
        kind: "proposal",
        filePath: current?.filePath ?? "",
        payloadBytes: answersJsonByteLength(payload),
        errorCode: null,
      });
      postStatus(requestId, "received");
    };
    window.addEventListener("message", onMessage);
    return () => window.removeEventListener("message", onMessage);
  }, [enabled, postStatus, setPending]);

  useEffect(
    () => () => {
      if (hideTimerRef.current) clearTimeout(hideTimerRef.current);
    },
    [],
  );

  const applyPending = useCallback(() => {
    const proposal = pending;
    const current = configRef.current;
    if (!proposal || !current || applyingRef.current) return;
    if (proposal.generation !== generationRef.current) return;
    if (!current.canSave || !current.filePath) {
      setBar({
        kind: "proposal",
        filePath: current.filePath ?? "",
        payloadBytes: answersJsonByteLength(proposal.answersJson),
        errorCode: "read-only",
      });
      return;
    }
    const result = buildNextDocument(current.displayed, proposal.answersJson);
    if (result.kind === "error") {
      postStatus(proposal.requestId, "error", result.code);
      setBar({
        kind: "proposal",
        filePath: current.filePath,
        payloadBytes: answersJsonByteLength(proposal.answersJson),
        errorCode: result.code,
      });
      return;
    }
    applyingRef.current = true;
    setBar({ kind: "applying" });
    const next = result.kind === "no-op" ? current.displayed : result.next;
    void current
      .onSave(next)
      .then(() => {
        setPending(null);
        postStatus(proposal.requestId, "applied");
        setBar({ kind: "applied" });
        if (hideTimerRef.current) clearTimeout(hideTimerRef.current);
        hideTimerRef.current = setTimeout(
          () => setBar(null),
          APPLIED_BAR_AUTO_HIDE_MS,
        );
      })
      .catch((error: unknown) => {
        const code: HtmlDocBridgeErrorCode =
          typeof error === "object" &&
          error !== null &&
          "status" in error &&
          error.status === 409
            ? "conflict"
            : "internal";
        postStatus(proposal.requestId, "error", code);
        setBar({
          kind: "proposal",
          filePath: current.filePath ?? "",
          payloadBytes: answersJsonByteLength(proposal.answersJson),
          errorCode: code,
        });
      })
      .finally(() => {
        applyingRef.current = false;
      });
  }, [pending, postStatus, setPending]);

  const dismissPending = useCallback(() => {
    if (!pending) return;
    postStatus(pending.requestId, "dismissed");
    setPending(null);
    setBar(null);
  }, [pending, postStatus, setPending]);

  return useMemo(
    () => ({
      frameProps: enabled ? { iframeRef, onLoad: handleFrameLoad } : undefined,
      bar,
      applyPending,
      dismissPending,
    }),
    [applyPending, bar, dismissPending, enabled, handleFrameLoad],
  );
}

const ERROR_MESSAGES: Record<HtmlDocBridgeErrorCode, string> = {
  "read-only": "This document is read-only here.",
  "missing-block": "The document has no g5-doc-answers block.",
  "ambiguous-block": "The document has more than one g5-doc-answers block.",
  "too-large": "The proposed answers exceed the size limit.",
  "bad-request": "The page sent an invalid answers payload.",
  busy: "Another save is in progress.",
  conflict: "The document changed concurrently. Reload it and try again.",
  internal: "Saving failed unexpectedly.",
};

export function HtmlDocAnswerBar({
  state,
  onApply,
  onDismiss,
  className,
}: {
  state: HtmlDocAnswerBarState;
  onApply: () => void;
  onDismiss: () => void;
  className?: string;
}) {
  return (
    <div
      data-testid="html-doc-answer-bar"
      role="status"
      aria-live="polite"
      className={classNames(
        "flex shrink-0 flex-wrap items-center gap-2 border-b border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-3 py-2 text-xs",
        className,
      )}
    >
      {state.kind === "proposal" && (
        <>
          <span className="min-w-0 flex-1 truncate text-[var(--g5-text1)]">
            This page proposes saving answers into{" "}
            <code className="rounded bg-[var(--g5-bg2)] px-1 py-0.5">
              {state.filePath}
            </code>{" "}
            ({Math.max(1, Math.round(state.payloadBytes / 1024))} KiB).
            {state.errorCode && (
              <span className="ml-2 text-[var(--g5-blocked)]">
                {ERROR_MESSAGES[state.errorCode]}
              </span>
            )}
          </span>
          <button
            type="button"
            data-testid="html-doc-answer-apply"
            onClick={onApply}
            className="shrink-0 rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg2)] px-2 py-1 font-medium text-[var(--g5-text0)] hover:bg-[var(--g5-bg0)]"
          >
            Apply and save
          </button>
          <button
            type="button"
            data-testid="html-doc-answer-dismiss"
            onClick={onDismiss}
            className="shrink-0 rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] px-2 py-1 text-[var(--g5-text2)] hover:text-[var(--g5-text0)]"
          >
            Dismiss
          </button>
        </>
      )}
      {state.kind === "applying" && (
        <span className="text-[var(--g5-text2)]">Saving answers…</span>
      )}
      {state.kind === "applied" && (
        <span className="text-[var(--g5-ok)]">Answers saved.</span>
      )}
    </div>
  );
}
