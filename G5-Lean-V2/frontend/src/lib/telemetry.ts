// =============================================================================
// UI Interaction Telemetry — capture, batching, and ingest (issue #1552)
// =============================================================================
// Pure TypeScript module (no React dependency — same rule as lib/api.ts).
// Captures element-level clicks, form submits, route navigations and
// Settings-page preference values (lib/preferenceTelemetry.ts) with enough
// stable identity to reconstruct ordered per-user flows
// ("button A → button B → button C"), batches them, and posts them to
// POST /api/v1/telemetry/ui-events. The server re-emits each event on the
// g5_audit tracing target bound to the authenticated user.
//
// Privacy rules (system_invariants/observability_audit D6): free-text input
// VALUES are never read; identity comes from data-testid (primary),
// aria-label, or truncated visible text of the activated control. A
// preference value is always one of the preference's offered choices (the
// encoder reports anything else as `other`). Events inside sandboxed iframes
// never reach these listeners (separate documents).
//
// The stream is best-effort by contract: fire-and-forget sends, no retries,
// bounded queue. A lost batch is acceptable; blocking UI work is not.

import { getApiToken, subscribeApiTokenChange } from "./api";
import { randomUuid } from "./randomUuid";
import { routes } from "./routes";

// ─── Wire types (server contract: routes/telemetry [D2]) ────────────────────

export type UiTelemetryKind = "click" | "navigation" | "submit" | "setting";

export interface UiTelemetryEvent {
  seq: number;
  tsMs: number;
  kind: UiTelemetryKind;
  target?: string;
  label?: string;
  /** Strictly pathname-only — the server rejects any '?' or '#'. */
  path?: string;
  /** Sanitized `view` query-param dimension, separated from `path`. */
  view?: string;
  detail?: string;
}

/** One `[preference key, encoded value]` pair (lib/preferenceTelemetry.ts). */
export type PreferenceEntry = readonly [key: string, value: string];

export interface TelemetryOptions {
  /**
   * Current Settings-page preference values. Reported as `setting` events
   * with `detail = "snapshot"` once per signed-in identity per tab (the
   * marker survives reloads), so the steady state of every active person is
   * measurable, not only their changes (`recordSettingChange`).
   */
  preferences?: () => readonly PreferenceEntry[];
}

/** `detail` of a preference snapshot event. */
export const SETTING_SNAPSHOT_DETAIL = "snapshot";

// ─── Tuning constants ────────────────────────────────────────────────────────

/** Server-side per-field length bound. */
const MAX_FIELD_LEN = 200;
/** Visible-text labels are capped tighter (identity, not prose). */
const MAX_LABEL_LEN = 80;
/** Queue length that triggers an immediate flush. */
const FLUSH_THRESHOLD = 50;
/** Interval flush period; the timer only runs while the tab is visible. */
const FLUSH_INTERVAL_MS = 10_000;
/** Server-side batch bound. */
const MAX_BATCH_EVENTS = 200;
/**
 * Byte cap per keepalive request: the Fetch standard rejects keepalive
 * bodies once in-flight keepalive bytes exceed 64 KiB — chunk conservatively
 * by serialized UTF-8 size (codex review 2026-07-29 finding 8).
 */
const MAX_KEEPALIVE_BYTES = 48 * 1024;
/** Hard queue bound — oldest events drop first (telemetry never queues unboundedly). */
const MAX_QUEUE = 500;

const SESSION_STORAGE_KEY = "g5_telemetry_session";
const SEQ_STORAGE_KEY = "g5_telemetry_seq";
/** Identity (`sub|org`) whose preference snapshot this tab already reported. */
const PREFERENCES_STORAGE_KEY = "g5_telemetry_prefs";

/**
 * Interactive elements worth recording when they carry no data-testid.
 * Plain text/body clicks are dropped as noise.
 */
const INTERACTIVE_SELECTOR = [
  "button",
  "a[href]",
  '[role="button"]',
  '[role="tab"]',
  '[role="menuitem"]',
  '[role="option"]',
  '[role="switch"]',
  'input[type="button"]',
  'input[type="submit"]',
  'input[type="checkbox"]',
  'input[type="radio"]',
  "select",
  "summary",
].join(", ");

// ─── Module state ────────────────────────────────────────────────────────────

let active = false;
let queue: UiTelemetryEvent[] = [];
let flushTimer: ReturnType<typeof setInterval> | null = null;
let keepaliveInFlight = false;
let lastNavigationKey: string | null = null;
let lastNavigationDisplay: string | null = null;
let unsubscribeTokenChange: (() => void) | null = null;
// In-memory fallbacks: every sessionStorage operation can throw (private
// mode, quota) and must never surface (codex impl review finding 5).
let memSessionId: string | null = null;
let memSeq = 0;
let memPreferencesIdentity: string | null = null;
let preferenceSource: (() => readonly PreferenceEntry[]) | null = null;

function storageGet(key: string): string | null {
  try {
    return window.sessionStorage.getItem(key);
  } catch {
    return null;
  }
}

function storageSet(key: string, value: string): void {
  try {
    window.sessionStorage.setItem(key, value);
  } catch {
    // in-memory fallbacks carry the state instead
  }
}

/** Per-tab telemetry session id — survives reloads, not new tabs. */
function telemetrySessionId(): string {
  const existing = storageGet(SESSION_STORAGE_KEY) ?? memSessionId;
  if (existing) {
    memSessionId = existing;
    return existing;
  }
  const fresh = randomUuid();
  memSessionId = fresh;
  storageSet(SESSION_STORAGE_KEY, fresh);
  return fresh;
}

/** Monotonic per-tab sequence, persisted so ordering survives reloads. */
function nextSeq(): number {
  const stored = Number(storageGet(SEQ_STORAGE_KEY) ?? "0") || 0;
  const next = Math.max(stored, memSeq) + 1;
  memSeq = next;
  storageSet(SEQ_STORAGE_KEY, String(next));
  return next;
}

/**
 * Rotates the per-tab telemetry session (auth-context change — client
 * telemetry [D4]). The seq counter keeps running; only the session id
 * changes.
 */
function rotateTelemetrySession(): void {
  const fresh = randomUuid();
  memSessionId = fresh;
  storageSet(SESSION_STORAGE_KEY, fresh);
  lastNavigationKey = null;
  lastNavigationDisplay = null;
}

/**
 * Minimal JWT payload decode for identity comparison; null on any failure.
 * The active organization rides the `org` claim (useAuth `authClaimsChanged`
 * compares the same pair); `orgId` is accepted for tokens that spell it so.
 */
function tokenIdentity(
  token: string | null,
): { sub?: string; orgId?: string } | null {
  if (!token) return null;
  try {
    const payload = token.split(".")[1];
    const decoded = JSON.parse(
      atob(payload.replace(/-/g, "+").replace(/_/g, "/")),
    ) as { sub?: string; org?: string; orgId?: string };
    return { sub: decoded.sub, orgId: decoded.org ?? decoded.orgId };
  } catch {
    return null;
  }
}

/**
 * Token-change handler (codex impl review finding 2): invoked BEFORE the
 * API client swaps its in-memory JWT. On a real identity/org change, flush
 * the pending queue while `getApiToken()` still returns the OLD credential
 * — org-A events must never be posted under org-B claims — then rotate the
 * telemetry session. Routine same-identity refreshes do nothing.
 */
function onTokenChange(previous: string | null, next: string | null): void {
  try {
    const prevId = tokenIdentity(previous);
    const nextId = tokenIdentity(next);
    if (prevId?.sub === nextId?.sub && prevId?.orgId === nextId?.orgId) {
      return;
    }
    flushTelemetry("interval");
    // Anything the flush could not send belongs to the old context; drop it
    // rather than misattribute it to the new one.
    queue = [];
    rotateTelemetrySession();
    // The new identity's steady state, queued now and sent once the API
    // client holds the new credential.
    recordPreferenceSnapshot(next);
  } catch {
    // telemetry must never break auth
  }
}

// ─── Field hygiene ───────────────────────────────────────────────────────────

/** Strips control characters and truncates to the server field bound. */
function cleanField(value: string, maxLen: number = MAX_FIELD_LEN): string {
  let out = "";
  for (const ch of value.trim()) {
    const code = ch.codePointAt(0) ?? 0;
    if (code < 0x20 || code === 0x7f) continue;
    out += ch;
    if (out.length >= maxLen) break;
  }
  return out;
}

// ─── Element identity (client/telemetry [D1]) ───────────────────────────────

interface ClickIdentity {
  target: string;
  label?: string;
  detail?: string;
}

function isFormField(el: Element): boolean {
  const tag = el.tagName.toLowerCase();
  return tag === "input" || tag === "textarea" || tag === "select";
}

/**
 * Resolves the identity of a clicked element, or `null` when the click is
 * noise (no data-testid carrier and no interactive ancestor). Never reads
 * input values.
 */
export function describeClickTarget(el: Element): ClickIdentity | null {
  const interactive = el.closest(INTERACTIVE_SELECTOR);
  const testidCarrier = el.closest("[data-testid]");
  if (!interactive && !testidCarrier) return null;

  const testid = testidCarrier?.getAttribute("data-testid") ?? null;
  const identityEl = interactive ?? testidCarrier!;

  const target = testid
    ? cleanField(testid)
    : identityEl.id
      ? cleanField(identityEl.id)
      : identityEl.tagName.toLowerCase();

  // Label priority: aria-label → visible text (interactive controls only,
  // never form-field values) → link pathname → input name/placeholder.
  let label: string | undefined;
  const ariaLabel = identityEl.getAttribute("aria-label");
  if (ariaLabel) {
    label = cleanField(ariaLabel, MAX_LABEL_LEN);
  } else if (isFormField(identityEl)) {
    const name =
      identityEl.getAttribute("name") ?? identityEl.getAttribute("placeholder");
    if (name) label = cleanField(name, MAX_LABEL_LEN);
  } else {
    const text = identityEl.textContent?.trim();
    if (text) {
      // The visible-text fallback is a documented product tradeoff (D1);
      // redact the one PII shape controls realistically render (member-row
      // emails) before it becomes a label.
      label = cleanField(
        text.replace(/\S+@\S+\.\S+/g, "<email>"),
        MAX_LABEL_LEN,
      );
    } else if (identityEl instanceof HTMLAnchorElement && identityEl.href) {
      try {
        label = cleanField(new URL(identityEl.href).pathname, MAX_LABEL_LEN);
      } catch {
        // ignore unparsable hrefs
      }
    }
  }

  // Container context: nearest ANCESTOR data-testid that differs from the
  // target (e.g. which panel the button lives in).
  const contextCarrier = (testidCarrier ?? identityEl).parentElement?.closest(
    "[data-testid]",
  );
  const contextTestid = contextCarrier?.getAttribute("data-testid");
  const detail =
    contextTestid && contextTestid !== testid
      ? cleanField(contextTestid)
      : undefined;

  return { target, label: label || undefined, detail };
}

// ─── Route context (client/telemetry [D4]) ──────────────────────────────────

export interface TelemetryRoute {
  /** Pathname only — the server rejects any '?' or '#' in `path`. */
  path: string;
  /** Sanitized `view` query param (admin surfaces); all other params dropped as potential PII. */
  view?: string;
}

/**
 * Telemetry route context: pathname plus the sanitized `view` query param as
 * a SEPARATE field — never composed into the path (codex impl review
 * finding 1: a composite would re-open the `?token=...` smuggling channel).
 */
export function telemetryRoute(
  pathname: string,
  search: string,
): TelemetryRoute {
  const path = cleanField(pathname);
  let view: string | null = null;
  try {
    view = new URLSearchParams(search).get("view");
  } catch {
    view = null;
  }
  if (!view) return { path };
  let cleanView = cleanField(view, 40).replace(/[/?#]/g, "");
  // The server bound is 40 BYTES; multi-byte view values must be trimmed by
  // UTF-8 length, not JS string length, or the whole batch gets rejected.
  while (cleanView && utf8.encode(cleanView).length > 40) {
    cleanView = cleanView.slice(0, -1);
  }
  return cleanView ? { path, view: cleanView } : { path };
}

function currentRoute(): TelemetryRoute {
  return telemetryRoute(window.location.pathname, window.location.search);
}

/** Human-readable `pathname(?view=...)` form, used only in `detail`. */
function routeDisplay(route: TelemetryRoute): string {
  return route.view ? `${route.path}?view=${route.view}` : route.path;
}

// ─── Queue / flush ───────────────────────────────────────────────────────────

function record(event: Omit<UiTelemetryEvent, "seq" | "tsMs">): void {
  if (!active) return;
  queue.push({ seq: nextSeq(), tsMs: Date.now(), ...event });
  if (queue.length > MAX_QUEUE) {
    queue = queue.slice(queue.length - MAX_QUEUE);
  }
  if (queue.length >= FLUSH_THRESHOLD) {
    flushTelemetry("threshold");
  }
}

const utf8 = new TextEncoder();

/** Serialized batch respecting both the event-count and keepalive byte caps. */
function takeBatch(byteCap: number | null): UiTelemetryEvent[] {
  const batch: UiTelemetryEvent[] = [];
  let bytes = 0;
  while (queue.length > 0 && batch.length < MAX_BATCH_EVENTS) {
    const next = queue[0];
    if (byteCap !== null) {
      const nextBytes = utf8.encode(JSON.stringify(next)).length + 1;
      if (bytes + nextBytes > byteCap && batch.length > 0) break;
      bytes += nextBytes;
    }
    batch.push(next);
    queue.shift();
  }
  return batch;
}

/**
 * Flushes queued events. `hidden`/`teardown` flushes use `keepalive` fetches
 * (byte-capped, at most one in flight) so tab close loses at most the tail;
 * ordinary flushes use plain fetches. Fire-and-forget: failures drop the
 * batch, telemetry never retries and never throws into caller frames.
 */
export function flushTelemetry(
  reason: "interval" | "threshold" | "hidden" | "teardown",
): void {
  try {
    const token = getApiToken();
    if (!token) return; // unauthenticated: keep queue, send nothing
    const useKeepalive = reason === "hidden" || reason === "teardown";
    if (useKeepalive && keepaliveInFlight) return;

    while (queue.length > 0) {
      const batch = takeBatch(useKeepalive ? MAX_KEEPALIVE_BYTES : null);
      if (batch.length === 0) return;
      const body = JSON.stringify({
        sessionId: telemetrySessionId(),
        events: batch,
      });
      if (useKeepalive) keepaliveInFlight = true;
      let pending: Promise<Response>;
      try {
        pending = fetch(routes.telemetry.uiEvents, {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            Authorization: `Bearer ${token}`,
          },
          body,
          keepalive: useKeepalive,
        });
      } catch {
        // A synchronous fetch throw must not latch the in-flight flag
        // (codex impl review finding 8); the batch is dropped.
        if (useKeepalive) keepaliveInFlight = false;
        return;
      }
      pending
        .catch(() => {
          // best-effort stream: a lost batch is acceptable
        })
        .finally(() => {
          if (useKeepalive) keepaliveInFlight = false;
        });
      // Keepalive path sends a single byte-capped batch; the rest of the
      // queue waits for the next opportunity (at most one in flight).
      if (useKeepalive) return;
    }
  } catch {
    // telemetry must never break the app
  }
}

// ─── Preference values (operations g5.platform.client-telemetry) ────────────

function settingEvent(
  key: string,
  value: string,
  detail: string,
): Omit<UiTelemetryEvent, "seq" | "tsMs"> {
  const route = currentRoute();
  return {
    kind: "setting",
    target: cleanField(key),
    label: cleanField(value, MAX_LABEL_LEN),
    detail: cleanField(detail),
    path: route.path,
    view: route.view,
  };
}

/**
 * Emits one `setting` event for a preference whose value changed: `target`
 * is the preference key, `label` the new value, `detail` the previous one
 * as `from:<previous>`. Callers pass encoded values (lib/preferenceTelemetry)
 * — never raw stored input.
 */
export function recordSettingChange(
  key: string,
  value: string,
  previous: string,
): void {
  try {
    if (!active) return;
    record(settingEvent(key, value, `from:${previous}`));
  } catch {
    // a telemetry bug must never break a settings update
  }
}

/**
 * Emits the preference snapshot (one `setting` event per preference,
 * `detail = "snapshot"`) for the identity carried by `token`, unless this
 * tab already reported that identity. Unauthenticated starts report nothing:
 * the sign-in's token change reports under the signed-in identity instead.
 */
function recordPreferenceSnapshot(token: string | null): void {
  if (!active || !preferenceSource) return;
  const identity = tokenIdentity(token);
  if (!identity?.sub) return;
  const marker = `${identity.sub}|${identity.orgId ?? ""}`;
  const reported =
    storageGet(PREFERENCES_STORAGE_KEY) ?? memPreferencesIdentity;
  if (reported === marker) return;
  memPreferencesIdentity = marker;
  storageSet(PREFERENCES_STORAGE_KEY, marker);
  let entries: readonly PreferenceEntry[];
  try {
    entries = preferenceSource();
  } catch {
    return; // a preference-store bug must never break telemetry start
  }
  for (const [key, value] of entries) {
    record(settingEvent(key, value, SETTING_SNAPSHOT_DETAIL));
  }
}

/** Emits a navigation event when the telemetry route actually changed. */
export function recordNavigation(path: string, view?: string): void {
  if (!active) return;
  const key = `${path}|${view ?? ""}`;
  if (key === lastNavigationKey) return;
  const from = lastNavigationDisplay ?? undefined;
  lastNavigationKey = key;
  lastNavigationDisplay = routeDisplay({ path, view });
  record({ kind: "navigation", target: "router", path, view, detail: from });
}

// ─── Listeners / lifecycle ───────────────────────────────────────────────────

function onClick(event: MouseEvent): void {
  try {
    const el = event.target;
    if (!(el instanceof Element)) return;
    const identity = describeClickTarget(el);
    if (!identity) return;
    const route = currentRoute();
    record({ kind: "click", path: route.path, view: route.view, ...identity });
  } catch {
    // a telemetry bug must never break a user click
  }
}

function onSubmit(event: Event): void {
  try {
    const form = event.target;
    if (!(form instanceof Element)) return;
    const testid = form.closest("[data-testid]")?.getAttribute("data-testid");
    const target = testid
      ? cleanField(testid)
      : form.id
        ? cleanField(form.id)
        : "form";
    const route = currentRoute();
    record({ kind: "submit", target, path: route.path, view: route.view });
  } catch {
    // never break a submit
  }
}

function startFlushTimer(): void {
  if (flushTimer !== null) return;
  flushTimer = setInterval(() => flushTelemetry("interval"), FLUSH_INTERVAL_MS);
}

function stopFlushTimer(): void {
  if (flushTimer !== null) {
    clearInterval(flushTimer);
    flushTimer = null;
  }
}

function onVisibilityChange(): void {
  if (document.visibilityState === "hidden") {
    // Hidden tabs do no background posting; flush the tail with keepalive.
    stopFlushTimer();
    flushTelemetry("hidden");
  } else {
    startFlushTimer();
  }
}

function onPageHide(): void {
  flushTelemetry("hidden");
}

/**
 * Installs the delegated listeners and the visible-only flush timer, and
 * reports the preference snapshot when a signed-in identity is already
 * known. Idempotent under React StrictMode double-invoke; returns a teardown.
 */
export function startTelemetry(options: TelemetryOptions = {}): () => void {
  if (active) {
    return () => {};
  }
  active = true;
  lastNavigationKey = null;
  lastNavigationDisplay = null;
  preferenceSource = options.preferences ?? null;

  document.addEventListener("click", onClick, true);
  document.addEventListener("submit", onSubmit, true);
  document.addEventListener("visibilitychange", onVisibilityChange);
  window.addEventListener("pagehide", onPageHide);
  unsubscribeTokenChange = subscribeApiTokenChange(onTokenChange);
  if (document.visibilityState !== "hidden") {
    startFlushTimer();
  }
  recordPreferenceSnapshot(getApiToken());

  return () => {
    if (!active) return;
    active = false;
    document.removeEventListener("click", onClick, true);
    document.removeEventListener("submit", onSubmit, true);
    document.removeEventListener("visibilitychange", onVisibilityChange);
    window.removeEventListener("pagehide", onPageHide);
    unsubscribeTokenChange?.();
    unsubscribeTokenChange = null;
    stopFlushTimer();
    flushTelemetry("teardown");
  };
}

// ─── Test hooks (not part of the app API) ────────────────────────────────────

/** Test-only: inspect the pending queue. */
export function pendingTelemetryCount(): number {
  return queue.length;
}

/** Test-only: reset all module state between tests. */
export function resetTelemetryForTests(): void {
  active = false;
  queue = [];
  stopFlushTimer();
  keepaliveInFlight = false;
  lastNavigationKey = null;
  lastNavigationDisplay = null;
  unsubscribeTokenChange?.();
  unsubscribeTokenChange = null;
  memSessionId = null;
  memSeq = 0;
  memPreferencesIdentity = null;
  preferenceSource = null;
}
