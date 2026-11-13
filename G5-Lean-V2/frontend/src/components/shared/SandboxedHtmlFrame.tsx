import {
  useCallback,
  useEffect,
  useImperativeHandle,
  useMemo,
  useRef,
  useState,
  type Ref,
} from "react";
import {
  collectPlatformThemeCss,
  sanitizeWorkspaceFilePath,
  sanitizeWorkspaceRepoRef,
  withSandboxedPlatformTheme,
  withSandboxedQuestionCards,
} from "./sandboxedHtml";

interface SandboxedHtmlFrameProps {
  html: string;
  title: string;
  className?: string;
  iframeClassName?: string;
  /**
   * Optional host access to the iframe element (D59 answer bridge). The frame
   * itself listens for messages only when `onOpenOntologyNode` is provided
   * (D66); surfaces without these props keep the exact prior write-inert
   * behavior.
   */
  iframeRef?: Ref<HTMLIFrameElement>;
  onLoad?: () => void;
  /** Host resolver for `g5://file/<path>` deep links — same D68
   * direct-dispatch model as ontology links (page proposes, host
   * re-validates and dispatches immediately, bounded by the hidden-tab
   * check and the rate limit — no confirm step); the host opens the path
   * through the citation file-open pathway, at the cited line if any. */
  onOpenWorkspaceFile?: (
    filePath: string,
    repoRef?: string,
    line?: number,
  ) => void;
  /**
   * Host-supplied question cards (`g5.documents.question.html`): when given,
   * the card renderer is injected for `data-g5-question` placeholders and the
   * question bridge (`useQuestionCardBridge`) sees this frame element and its
   * loads. Absent ⇒ placeholders stay inert markup.
   */
  questionFrame?: {
    setFrame: (element: HTMLIFrameElement | null) => void;
    onLoad: () => void;
  };
}

// Resolve the active platform theme CSS and follow <html data-theme data-mode>
// switches so an open preview restyles without a remount or tab change.
function usePlatformThemeCss(): string {
  const [themeCss, setThemeCss] = useState(collectPlatformThemeCss);

  useEffect(() => {
    if (typeof document === "undefined" || !document.documentElement) return;
    const observer = new MutationObserver(() =>
      setThemeCss(collectPlatformThemeCss()),
    );
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["data-theme", "data-mode"],
    });
    return () => observer.disconnect();
  }, []);

  return themeCss;
}

export function SandboxedHtmlFrame({
  html,
  title,
  className,
  iframeClassName = "h-full w-full border-0 bg-white",
  iframeRef,
  onLoad,
  onOpenWorkspaceFile,
  questionFrame,
}: SandboxedHtmlFrameProps) {
  const themeCss = usePlatformThemeCss();
  const withQuestionCards = questionFrame != null;
  const srcDoc = useMemo(() => {
    const composed = withSandboxedPlatformTheme(html, themeCss);
    return withQuestionCards ? withSandboxedQuestionCards(composed) : composed;
  }, [html, themeCss, withQuestionCards]);

  // The iframe stays invisible until its document loads: the default white
  // canvas otherwise flashes against the dark platform theme (user report
  // 2026-08-07). The container shows the ambient themed background instead.
  const [frameLoaded, setFrameLoaded] = useState(false);
  useEffect(() => {
    setFrameLoaded(false);
  }, [srcDoc]);

  // Local element handle for D66 source checks, merged with the optional
  // caller-owned ref (D59 bridge) so both observers see the same element.
  const localFrameRef = useRef<HTMLIFrameElement | null>(null);
  useImperativeHandle(
    iframeRef,
    () => localFrameRef.current as HTMLIFrameElement,
    [],
  );

  // D68 direct dispatch (user decision 2026-08-10, superseding the earlier
  // host-owned confirmation): a valid proposal navigates directly — the
  // documents are agent-generated in-workspace artifacts and navigation
  // stays in-app, so the blast radius of a forged proposal is an unwanted
  // tab. Because postMessage carries no trusted-activation bit, two
  // invisible bounds remain: hidden-tab proposals are dropped, and
  // dispatches are rate-limited to one per 750ms per frame so a scripted
  // proposal loop cannot flood navigation.
  const lastNavDispatchRef = useRef(0);

  useEffect(() => {
    if (!onOpenWorkspaceFile) return;
    const onMessage = (event: MessageEvent) => {
      // Only the sandboxed document itself may propose (D59 source rule).
      const frame = localFrameRef.current;
      if (!frame || event.source !== frame.contentWindow) return;
      const data = event.data as Record<string, unknown> | null;
      if (!data || data.source !== "g5-html-doc") return;
      if (document.visibilityState === "hidden") return;
      if (
        onOpenWorkspaceFile &&
        data.type === "open-workspace-file" &&
        typeof data.filePath === "string"
      ) {
        const filePath = sanitizeWorkspaceFilePath(data.filePath);
        if (!filePath) return;
        // The optional ownership qualifier is re-validated with the same
        // rules as the in-page script; an invalid qualifier drops the WHOLE
        // proposal — a link that tried to name an owner is never
        // reinterpreted as ownerless (center-panel D73).
        let repoRef: string | undefined;
        if (data.repoRef !== undefined) {
          if (typeof data.repoRef !== "string") return;
          const sanitizedRepoRef = sanitizeWorkspaceRepoRef(data.repoRef);
          if (!sanitizedRepoRef) return;
          repoRef = sanitizedRepoRef;
        }
        // The in-page script only proposes a positive line; anything else
        // is forged and drops the whole proposal.
        let line: number | undefined;
        if (data.line !== undefined) {
          if (
            typeof data.line !== "number" ||
            !Number.isSafeInteger(data.line) ||
            data.line < 1
          )
            return;
          line = data.line;
        }
        const now = Date.now();
        if (now - lastNavDispatchRef.current < 750) return;
        lastNavDispatchRef.current = now;
        onOpenWorkspaceFile(filePath, repoRef, line);
      }
    };
    window.addEventListener("message", onMessage);
    return () => window.removeEventListener("message", onMessage);
  }, [onOpenWorkspaceFile]);

  return (
    <div className={className ? `relative ${className}` : "relative"}>
      <iframe
        ref={(element) => {
          localFrameRef.current = element;
          questionFrame?.setFrame(element);
        }}
        className={`${iframeClassName} ${frameLoaded ? "" : "opacity-0"}`}
        title={title}
        sandbox="allow-scripts"
        referrerPolicy="no-referrer"
        srcDoc={srcDoc}
        onLoad={() => {
          setFrameLoaded(true);
          onLoad?.();
          questionFrame?.onLoad();
        }}
      />
    </div>
  );
}
