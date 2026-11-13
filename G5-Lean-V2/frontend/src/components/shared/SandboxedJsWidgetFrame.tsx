import React, { useEffect, useMemo, useRef, useState } from "react";
import { classNames } from "../../lib/utils";

const WIDGET_MESSAGE_SOURCE = "g5-js-widget";
const MIN_WIDGET_HEIGHT = 180;
const MAX_WIDGET_HEIGHT = 3200;
const THEME_TOKEN_NAMES = [
  "--g5-bg0",
  "--g5-bg1",
  "--g5-bg2",
  "--g5-stroke",
  "--g5-text0",
  "--g5-text1",
  "--g5-text2",
  "--g5-info",
  "--g5-ok",
  "--g5-warn",
  "--g5-blocked",
  "--g5-radius-sm",
  "--g5-radius-md",
  "--g5-font-sans",
  "--g5-font-mono",
] as const;

interface ThemeSnapshot {
  mode: string;
  theme: string;
  vars: Record<string, string>;
}

type WidgetStatus = "booting" | "ready" | "error";

interface WidgetLogEntry {
  id: number;
  kind: "ready" | "resize" | "event" | "error";
  label: string;
  detail?: unknown;
}

interface WidgetMessage {
  source?: string;
  widgetId?: string;
  type?: string;
  height?: unknown;
  name?: unknown;
  payload?: unknown;
  message?: unknown;
  stack?: unknown;
  deltaX?: unknown;
  deltaY?: unknown;
}

export interface SandboxedJsWidgetFrameProps {
  code: string;
  widgetId?: string;
  propsData?: unknown;
  className?: string;
}

function clampHeight(value: number): number {
  if (!Number.isFinite(value)) return MIN_WIDGET_HEIGHT;
  return Math.min(MAX_WIDGET_HEIGHT, Math.max(MIN_WIDGET_HEIGHT, value));
}

function jsonForInlineScript(value: unknown): string {
  return JSON.stringify(value)
    .replace(/</g, "\\u003c")
    .replace(/>/g, "\\u003e")
    .replace(/&/g, "\\u0026")
    .replace(/\u2028/g, "\\u2028")
    .replace(/\u2029/g, "\\u2029");
}

function escapeInlineScript(code: string): string {
  return code.replace(/<\/script/gi, "<\\/script");
}

function readThemeSnapshot(): ThemeSnapshot {
  if (typeof document === "undefined") {
    return { mode: "dark", theme: "editor", vars: {} };
  }

  const root = document.documentElement;
  const styles = window.getComputedStyle(root);
  const vars: Record<string, string> = {};
  for (const name of THEME_TOKEN_NAMES) {
    const value = styles.getPropertyValue(name).trim();
    if (value) vars[name] = value;
  }
  return {
    mode: root.dataset.mode ?? "dark",
    theme: root.dataset.theme ?? "editor",
    vars,
  };
}

function themeVarsToCss(theme: ThemeSnapshot): string {
  return THEME_TOKEN_NAMES.map((name) => {
    const value = theme.vars[name] || "";
    return value ? `${name}: ${value};` : "";
  })
    .filter(Boolean)
    .join("\n      ");
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value && typeof value === "object" && !Array.isArray(value));
}

function useWidgetThemeSnapshot(): ThemeSnapshot {
  const [theme, setTheme] = useState(readThemeSnapshot);

  useEffect(() => {
    if (typeof document === "undefined") return;

    const updateTheme = () => setTheme(readThemeSnapshot());
    const observer = new MutationObserver(updateTheme);
    observer.observe(document.documentElement, {
      attributeFilter: ["data-theme", "data-mode", "style", "class"],
      attributes: true,
    });
    return () => observer.disconnect();
  }, []);

  return theme;
}

function buildWidgetSrcDoc(
  code: string,
  widgetId: string,
  propsData: unknown,
  theme: ThemeSnapshot,
) {
  const widgetScript = escapeInlineScript(code);
  const widgetIdJson = jsonForInlineScript(widgetId);
  const propsJson = jsonForInlineScript(
    isRecord(propsData)
      ? { ...propsData, theme }
      : { value: propsData ?? null, theme },
  );
  const themeCss = themeVarsToCss(theme);

  return `<!doctype html>
<html>
<head>
  <meta charset="utf-8" />
  <meta
    http-equiv="Content-Security-Policy"
    content="default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data: blob:; connect-src 'none'; font-src 'none'; base-uri 'none'; form-action 'none'; frame-src 'none'; object-src 'none'"
  />
  <style>
    :root {
      ${themeCss}
      color-scheme: ${theme.mode === "light" ? "light" : "dark"};
      font-family: var(--g5-font-sans);
      background: var(--g5-bg0);
      color: var(--g5-text0);
    }
    * { box-sizing: border-box; }
    html {
      overflow: hidden;
    }
    body {
      margin: 0;
      min-height: 100vh;
      background: var(--g5-bg0);
      color: var(--g5-text0);
      overflow: hidden;
    }
    #mount {
      min-height: 100vh;
      padding: 16px;
    }
    button, input, select, textarea {
      font: inherit;
    }
    button {
      border: 1px solid var(--g5-stroke);
      border-radius: var(--g5-radius-sm);
      background: var(--g5-bg1);
      color: var(--g5-text0);
      padding: 8px 10px;
      cursor: pointer;
    }
    button:hover {
      background: var(--g5-bg2);
    }
    .g5-widget-shell {
      display: grid;
      gap: 12px;
    }
    .g5-widget-row {
      display: flex;
      gap: 8px;
      align-items: center;
      flex-wrap: wrap;
    }
  </style>
</head>
<body>
  <div id="mount"></div>
  <script>
    (() => {
      const WIDGET_ID = ${widgetIdJson};
      const PROPS = ${propsJson};
      const SOURCE = ${JSON.stringify(WIDGET_MESSAGE_SOURCE)};
      const MIN_HEIGHT = ${MIN_WIDGET_HEIGHT};
      const MAX_HEIGHT = ${MAX_WIDGET_HEIGHT};
      const mount = document.getElementById("mount");

      const clampHeight = (value) => {
        const next = Number(value);
        if (!Number.isFinite(next)) return MIN_HEIGHT;
        return Math.min(MAX_HEIGHT, Math.max(MIN_HEIGHT, Math.ceil(next)));
      };

      const post = (message) => {
        parent.postMessage({ source: SOURCE, widgetId: WIDGET_ID, ...message }, "*");
      };

      const requestMeasuredHeight = () => {
        const height = Math.max(
          document.documentElement.scrollHeight,
          document.body.scrollHeight,
          mount ? mount.scrollHeight : 0,
        );
        post({ type: "resize", height: clampHeight(height) });
      };

      const disabledNetwork = () => Promise.reject(new Error("Network access is disabled for g5-js widgets."));
      try { window.fetch = disabledNetwork; } catch {}
      try { window.XMLHttpRequest = function DisabledXMLHttpRequest() { throw new Error("Network access is disabled for g5-js widgets."); }; } catch {}
      try { window.WebSocket = function DisabledWebSocket() { throw new Error("Network access is disabled for g5-js widgets."); }; } catch {}
      try { window.EventSource = function DisabledEventSource() { throw new Error("Network access is disabled for g5-js widgets."); }; } catch {}
      try { navigator.sendBeacon = () => false; } catch {}

      const g5 = Object.freeze({
        props: PROPS,
        setHtml(html) {
          if (mount) mount.innerHTML = String(html ?? "");
          requestAnimationFrame(requestMeasuredHeight);
        },
        setText(text) {
          if (mount) mount.textContent = String(text ?? "");
          requestAnimationFrame(requestMeasuredHeight);
        },
        emit(name, payload) {
          post({ type: "event", name: String(name), payload: payload ?? null });
        },
        requestHeight(height) {
          if (height == null) {
            requestMeasuredHeight();
            return;
          }
          post({ type: "resize", height: clampHeight(height) });
        },
        openFile(path) {
          post({
            type: "event",
            name: "openFileRequest",
            payload: { path: String(path ?? "") },
          });
        },
      });

      window.g5 = g5;
      window.mount = mount;
      window.props = PROPS;
      window.__g5RequestMeasuredHeight = requestMeasuredHeight;

      window.addEventListener("error", (event) => {
        post({
          type: "error",
          message: event.message || "Widget error",
          stack: event.error && event.error.stack ? String(event.error.stack) : "",
        });
      });

      window.addEventListener("unhandledrejection", (event) => {
        const reason = event.reason;
        post({
          type: "error",
          message: reason && reason.message ? String(reason.message) : String(reason),
          stack: reason && reason.stack ? String(reason.stack) : "",
        });
      });

      window.addEventListener("wheel", (event) => {
        event.preventDefault();
        post({
          type: "wheel",
          deltaX: event.deltaX,
          deltaY: event.deltaY,
        });
      }, { passive: false });

      if (typeof ResizeObserver !== "undefined" && mount) {
        const resizeObserver = new ResizeObserver(() => requestMeasuredHeight());
        resizeObserver.observe(mount);
      }

      post({ type: "ready" });
    })();
  </script>
  <script>
${widgetScript}
  </script>
  <script>
    requestAnimationFrame(window.__g5RequestMeasuredHeight);
  </script>
</body>
</html>`;
}

function isWidgetMessage(value: unknown): value is WidgetMessage {
  return Boolean(value && typeof value === "object");
}

function scrollNearestDocumentSurface(
  source: HTMLElement | null,
  deltaX: number,
  deltaY: number,
) {
  let current = source?.parentElement ?? null;
  while (current && current !== document.body) {
    const style = window.getComputedStyle(current);
    const canScrollY =
      /(auto|scroll|overlay)/.test(style.overflowY) &&
      current.scrollHeight > current.clientHeight;
    const canScrollX =
      /(auto|scroll|overlay)/.test(style.overflowX) &&
      current.scrollWidth > current.clientWidth;
    if (canScrollY || canScrollX) {
      current.scrollBy({ left: deltaX, top: deltaY });
      return;
    }
    current = current.parentElement;
  }
  window.scrollBy({ left: deltaX, top: deltaY });
}

export const SandboxedJsWidgetFrame: React.FC<SandboxedJsWidgetFrameProps> = ({
  code,
  widgetId = "inline-g5-js-widget",
  propsData,
  className,
}) => {
  const theme = useWidgetThemeSnapshot();
  const srcDoc = useMemo(
    () => buildWidgetSrcDoc(code, widgetId, propsData, theme),
    [code, propsData, theme, widgetId],
  );

  return (
    <SandboxedJsWidgetFrameRun
      key={srcDoc}
      className={className}
      srcDoc={srcDoc}
      widgetId={widgetId}
    />
  );
};

interface SandboxedJsWidgetFrameRunProps {
  className?: string;
  srcDoc: string;
  widgetId: string;
}

const SandboxedJsWidgetFrameRun: React.FC<SandboxedJsWidgetFrameRunProps> = ({
  className,
  srcDoc,
  widgetId,
}) => {
  const iframeRef = useRef<HTMLIFrameElement>(null);
  const [height, setHeight] = useState(MIN_WIDGET_HEIGHT);
  const [status, setStatus] = useState<WidgetStatus>("booting");
  const [logs, setLogs] = useState<WidgetLogEntry[]>([]);
  const nextLogIdRef = useRef(1);

  useEffect(() => {
    const pushLog = (
      kind: WidgetLogEntry["kind"],
      label: string,
      detail?: unknown,
    ) => {
      setLogs((current) =>
        [{ id: nextLogIdRef.current++, kind, label, detail }, ...current].slice(
          0,
          6,
        ),
      );
    };

    const handleMessage = (event: MessageEvent) => {
      if (event.source !== iframeRef.current?.contentWindow) return;
      if (!isWidgetMessage(event.data)) return;
      if (
        event.data.source !== WIDGET_MESSAGE_SOURCE ||
        event.data.widgetId !== widgetId
      ) {
        return;
      }

      if (event.data.type === "ready") {
        setStatus("ready");
        pushLog("ready", "Widget ready");
        return;
      }

      if (event.data.type === "resize") {
        const nextHeight = clampHeight(Number(event.data.height));
        setHeight(nextHeight);
        pushLog("resize", `Resize requested: ${nextHeight}px`);
        return;
      }

      if (event.data.type === "wheel") {
        const deltaX = Number(event.data.deltaX);
        const deltaY = Number(event.data.deltaY);
        scrollNearestDocumentSurface(
          iframeRef.current,
          Number.isFinite(deltaX) ? deltaX : 0,
          Number.isFinite(deltaY) ? deltaY : 0,
        );
        return;
      }

      if (event.data.type === "event") {
        const name =
          typeof event.data.name === "string" ? event.data.name : "event";
        pushLog("event", name, event.data.payload);
        return;
      }

      if (event.data.type === "error") {
        setStatus("error");
        const message =
          typeof event.data.message === "string"
            ? event.data.message
            : "Widget error";
        pushLog("error", message, event.data.stack);
      }
    };

    window.addEventListener("message", handleMessage);
    return () => window.removeEventListener("message", handleMessage);
  }, [widgetId]);

  return (
    <div
      data-testid="sandboxed-js-widget"
      className={classNames(
        "my-4 overflow-hidden rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg1)]",
        className,
      )}
      style={{ borderWidth: "var(--g5-border-w)" }}
    >
      <div className="flex items-center justify-between gap-3 border-b border-[var(--g5-stroke)] px-3 py-2 text-xs">
        <div className="min-w-0">
          <div className="font-medium text-[var(--g5-text0)]">
            Sandboxed JS widget
          </div>
          <div className="truncate text-[var(--g5-text2)]">
            sandbox=allow-scripts, no same-origin
          </div>
        </div>
        <span
          className={classNames(
            "shrink-0 rounded-full px-2 py-0.5 font-medium",
            status === "ready" &&
              "bg-[color-mix(in_srgb,var(--g5-ok)_14%,transparent)] text-[var(--g5-ok)]",
            status === "error" &&
              "bg-[color-mix(in_srgb,var(--g5-blocked)_14%,transparent)] text-[var(--g5-blocked)]",
            status === "booting" &&
              "bg-[color-mix(in_srgb,var(--g5-warn)_14%,transparent)] text-[var(--g5-warn)]",
          )}
        >
          {status}
        </span>
      </div>

      <iframe
        ref={iframeRef}
        data-testid="sandboxed-js-widget-frame"
        title={`Sandboxed JS widget ${widgetId}`}
        sandbox="allow-scripts"
        referrerPolicy="no-referrer"
        srcDoc={srcDoc}
        scrolling="no"
        className="block w-full bg-[var(--g5-bg0)]"
        style={{ height, overflow: "hidden" }}
      />

      {logs.length > 0 && (
        <div className="border-t border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-3 py-2">
          <div className="mb-1 text-[10px] font-semibold uppercase tracking-normal text-[var(--g5-text2)]">
            Bridge events
          </div>
          <ol className="space-y-1 text-xs text-[var(--g5-text1)]">
            {logs.map((entry) => (
              <li key={entry.id} className="min-w-0">
                <span className="font-medium">{entry.label}</span>
                {entry.detail !== undefined && (
                  <code className="ml-2 break-all rounded bg-[var(--g5-bg2)] px-1 py-0.5 text-[10px] text-[var(--g5-text2)]">
                    {JSON.stringify(entry.detail)}
                  </code>
                )}
              </li>
            ))}
          </ol>
        </div>
      )}
    </div>
  );
};
