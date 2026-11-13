// =============================================================================
// G5 Client — Onboarding tour (ui g5.ui.onboarding-tour)
// =============================================================================
// One controller above the routes walks a new person through seven steps,
// each a card beside a spotlit control. It reads the person's record (users
// g5.user.onboarding) through the auth context, decides the active step from
// the record, the route and the controls the page has mounted, records every
// step reached and the skip, and follows the person from Home to Base's
// settings view, back to Home, to the issue page and into the new workspace.
// Pages expose only stable anchors (`data-testid`) and the composer seed the
// context carries; no page knows the step list.
//
// Blocking: while a step shows, pointer events outside the spotlit controls
// and the card are swallowed at the document (capture phase), so the page
// still scrolls with the wheel but nothing else is pressable; an open modal
// (the Create issue dialog, the account form's confirmations) hides the
// tour until it closes, so the dialog stays usable. The one exception is
// step 6's Start workspace dialog, which gets its own card without a dim.
// =============================================================================

import { useQueryClient } from "@tanstack/react-query";
import React, {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { useAuth } from "../../hooks/useAuth";
import { useUserSettings } from "../../hooks/useUserSettings";
import { externalIssueKeys } from "../../hooks/useExternalIssues";
import { useProviderAccounts } from "../../hooks/useProviderAccounts";
import { api } from "../../lib/api";
import {
  fillOnboardingCopy,
  ONBOARDING_ACCOUNTS_OWN_COPY,
  ONBOARDING_ACCOUNTS_VIEWER_COPY,
  ONBOARDING_COPY,
  ONBOARDING_ISSUE_FALLBACK_COPY,
  ONBOARDING_ISSUE_MANUAL_COPY,
  ONBOARDING_ISSUE_POLL_MS,
  ONBOARDING_ISSUE_WAIT_MS,
  ONBOARDING_STEP_COUNT,
  ONBOARDING_STEP_IDS,
  ONBOARDING_START_DIALOG_COPY,
  ONBOARDING_VIEWER_CLOSING_COPY,
  ONBOARDING_WORKSPACE_PROMPT_COPY,
  onboardingFirstName,
  onboardingIssueMessage,
  onboardingIssueTitle,
  onboardingNeverShown,
  onboardingResumeStep,
  stepPage,
  type OnboardingPage,
  type OnboardingStepCopy,
} from "../../lib/onboarding";
import { routes } from "../../lib/routes";
import { classifyShellRoute } from "../../lib/shellRoutes";
import type {
  CurrentUserDTO,
  ExternalIssueSearchResponseDTO,
} from "../../lib/types";
import { useCoveringOverlayOpen } from "../ui/useDialogFocus";
import { Button, classNames } from "../ui/UI";
import {
  publishOnboardingTour,
  type OnboardingTourValue,
} from "./onboardingTourContext";

// ---------------------------------------------------------------------------
// Browser state: the replay cursor
// ---------------------------------------------------------------------------

/**
 * A replay runs from step 0 while the record already holds a higher step, so
 * its position lives in this browser tab; a reload keeps it, a new tab does
 * not (the record then resumes or stays finished).
 */
const REPLAY_CURSOR_KEY = "g5:onboarding:replay-cursor";
/** The issue step's baseline (step 5), kept across the Create fallback's
 * navigation, a reload and this device's other tabs, so an issue created
 * meanwhile still counts. Another device starts its own baseline. */
const ISSUE_BASELINE_KEY = "g5:onboarding:issue-baseline";
/** The issue the run detected, so step 6 points at that row, not a namesake. */
const ISSUE_FOUND_KEY = "g5:onboarding:issue-found";

interface FoundIssue {
  externalIssueId: string;
  displayKey: string;
}

function readReplayCursor(userId: string): number | null {
  try {
    const raw = sessionStorage.getItem(`${REPLAY_CURSOR_KEY}:${userId}`);
    if (raw === null) return null;
    const value = Number(raw);
    return Number.isInteger(value) &&
      value >= 0 &&
      value < ONBOARDING_STEP_COUNT
      ? value
      : null;
  } catch {
    return null;
  }
}

function writeReplayCursor(userId: string, value: number | null): void {
  try {
    const key = `${REPLAY_CURSOR_KEY}:${userId}`;
    if (value === null) sessionStorage.removeItem(key);
    else sessionStorage.setItem(key, String(value));
  } catch {
    // The tour still runs without browser storage.
  }
}

function readIssueBaseline(userId: string): string[] | null {
  try {
    const raw = localStorage.getItem(`${ISSUE_BASELINE_KEY}:${userId}`);
    if (raw === null) return null;
    const parsed: unknown = JSON.parse(raw);
    return Array.isArray(parsed) && parsed.every((k) => typeof k === "string")
      ? (parsed as string[])
      : null;
  } catch {
    return null;
  }
}

function writeIssueBaseline(userId: string, keys: string[] | null): void {
  try {
    const key = `${ISSUE_BASELINE_KEY}:${userId}`;
    if (keys === null) localStorage.removeItem(key);
    else localStorage.setItem(key, JSON.stringify(keys));
  } catch {
    // Without storage the baseline lives for this mount only.
  }
}

function readFoundIssue(userId: string): FoundIssue | null {
  try {
    const raw = localStorage.getItem(`${ISSUE_FOUND_KEY}:${userId}`);
    if (raw === null) return null;
    const parsed = JSON.parse(raw) as Partial<FoundIssue>;
    return typeof parsed.externalIssueId === "string" &&
      typeof parsed.displayKey === "string"
      ? {
          externalIssueId: parsed.externalIssueId,
          displayKey: parsed.displayKey,
        }
      : null;
  } catch {
    return null;
  }
}

function writeFoundIssue(userId: string, issue: FoundIssue | null): void {
  try {
    const key = `${ISSUE_FOUND_KEY}:${userId}`;
    if (issue === null) localStorage.removeItem(key);
    else localStorage.setItem(key, JSON.stringify(issue));
  } catch {
    // Without storage the step falls back to the title.
  }
}

// ---------------------------------------------------------------------------
// Where the person is
// ---------------------------------------------------------------------------

interface TourLocation {
  page: OnboardingPage | "other";
  orgSlug: string | null;
}

function locateTour(pathname: string, search: string): TourLocation {
  const route = classifyShellRoute(pathname);
  if (route.kind === "home") return { page: "home", orgSlug: route.orgSlug };
  if (route.kind === "workspace") {
    const view = new URLSearchParams(search).get("view");
    if (view === "settings")
      return { page: "settings", orgSlug: route.orgSlug };
    // Base is not where work happens; the Guided step waits for a fork.
    if (route.workspaceName.toLowerCase() === "base" || route.personal) {
      return { page: "other", orgSlug: route.orgSlug };
    }
    return { page: "workspace", orgSlug: route.orgSlug };
  }
  const issue = pathname.match(/^\/([^/]+)\/issues\//);
  if (issue) return { page: "issue", orgSlug: decodeURIComponent(issue[1]) };
  const org = pathname.match(/^\/([^/]+)/);
  return { page: "other", orgSlug: org ? decodeURIComponent(org[1]) : null };
}

/** The phone layout has no strip, so the tour is a desktop arrangement. */
const DESKTOP_QUERY = "(min-width: 1024px)";

function useIsDesktop(): boolean {
  const [desktop, setDesktop] = useState(() =>
    typeof window === "undefined" || typeof window.matchMedia !== "function"
      ? true
      : window.matchMedia(DESKTOP_QUERY).matches,
  );
  useEffect(() => {
    if (typeof window.matchMedia !== "function") return;
    const query = window.matchMedia(DESKTOP_QUERY);
    const update = () => setDesktop(query.matches);
    update();
    query.addEventListener("change", update);
    return () => query.removeEventListener("change", update);
  }, []);
  return desktop;
}

// ---------------------------------------------------------------------------
// Spotlight geometry
// ---------------------------------------------------------------------------

interface Rect {
  top: number;
  left: number;
  width: number;
  height: number;
}

const SPOTLIGHT_PAD = 6;
/** The card's natural width, and the narrowest it shrinks to beside a dialog. */
const CARD_WIDTH = 360;
const CARD_MIN_WIDTH = 240;

function sameRect(a: Rect | null, b: Rect | null): boolean {
  if (!a || !b) return a === b;
  return (
    Math.round(a.top) === Math.round(b.top) &&
    Math.round(a.left) === Math.round(b.left) &&
    Math.round(a.width) === Math.round(b.width) &&
    Math.round(a.height) === Math.round(b.height)
  );
}

/** A spotlit control: selectors tried in order; `text` narrows to a match. */
interface TourTarget {
  selectors: readonly string[];
  text?: string;
  /**
   * Resolve `selectors` inside one scope (the first visible element matching
   * `selector`, its test id `testIdPattern` and containing `text`), falling
   * back to the scope itself; without a scope, `fallbackSelectors` apply.
   */
  within?: {
    selector: string;
    testIdPattern?: RegExp;
    text?: string;
    fallbackSelectors?: readonly string[];
  };
}

function visible(element: Element): boolean {
  // Browsers answer display/visibility here; a layout-less DOM (tests) has
  // no checkVisibility and counts every element.
  if (typeof (element as HTMLElement).checkVisibility === "function") {
    return (element as HTMLElement).checkVisibility();
  }
  return true;
}

function resolveWithin(
  root: ParentNode,
  selectors: readonly string[],
  text: string | undefined,
): Element[] {
  for (const selector of selectors) {
    let found = Array.from(root.querySelectorAll(selector)).filter(visible);
    if (text) {
      found = found.filter((element) =>
        (element.textContent ?? "").includes(text),
      );
    }
    if (found.length > 0) return found;
  }
  return [];
}

function resolveTarget(target: TourTarget): Element[] {
  if (target.within) {
    const { selector, testIdPattern, text, fallbackSelectors } = target.within;
    const scope = Array.from(document.querySelectorAll(selector))
      .filter(visible)
      .filter(
        (element) =>
          !testIdPattern ||
          testIdPattern.test(element.getAttribute("data-testid") ?? ""),
      )
      .find((element) => !text || (element.textContent ?? "").includes(text));
    if (!scope) {
      return fallbackSelectors
        ? resolveWithin(document, fallbackSelectors, undefined)
        : [];
    }
    const inside = resolveWithin(scope, target.selectors, undefined);
    return inside.length > 0 ? inside : [scope];
  }
  return resolveWithin(document, target.selectors, target.text);
}

function unionRect(elements: Element[]): Rect | null {
  if (elements.length === 0) return null;
  let top = Number.POSITIVE_INFINITY;
  let left = Number.POSITIVE_INFINITY;
  let right = Number.NEGATIVE_INFINITY;
  let bottom = Number.NEGATIVE_INFINITY;
  for (const element of elements) {
    const rect = element.getBoundingClientRect();
    top = Math.min(top, rect.top);
    left = Math.min(left, rect.left);
    right = Math.max(right, rect.right);
    bottom = Math.max(bottom, rect.bottom);
  }
  return { top, left, width: right - left, height: bottom - top };
}

/**
 * Follow the targets' on-screen rectangle: scroll, resize, layout changes
 * and late mounts (the rail overlay, the Guided panel, a loaded issue row).
 */
function useTargetRect(
  targets: readonly TourTarget[],
  avoidAncestor?: string,
): {
  rect: Rect | null;
  /** The rectangle to keep clear of: the avoided ancestor, else the targets'. */
  obstacle: Rect | null;
  elements: Element[];
  /**
   * The targets as last measured, current before the render that carries
   * `elements` commits: event handlers read this so a press or a click on
   * a control that just mounted counts straight away.
   */
  latest: React.MutableRefObject<Element[]>;
} {
  const latest = useRef<Element[]>([]);
  const [state, setState] = useState<{
    rect: Rect | null;
    obstacle: Rect | null;
    elements: Element[];
  }>({ rect: null, obstacle: null, elements: [] });
  const key = JSON.stringify([targets, avoidAncestor ?? null]);
  useEffect(() => {
    if (targets.length === 0) {
      latest.current = [];
      setState((current) =>
        current.rect === null && current.elements.length === 0
          ? current
          : { rect: null, obstacle: null, elements: [] },
      );
      return;
    }
    let frame = 0;
    let disposed = false;
    const measure = () => {
      if (disposed) return;
      const elements = targets.flatMap((target) => resolveTarget(target));
      latest.current = elements;
      const rect = unionRect(elements);
      const ancestor = avoidAncestor
        ? (elements[0]?.closest(avoidAncestor) ?? null)
        : null;
      const obstacle = ancestor ? unionRect([ancestor]) : rect;
      setState((current) =>
        sameRect(current.rect, rect) &&
        sameRect(current.obstacle, obstacle) &&
        current.elements.length === elements.length &&
        current.elements.every((element, index) => element === elements[index])
          ? current
          : { rect, obstacle, elements },
      );
    };
    const schedule = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(measure);
    };
    measure();
    const interval = setInterval(measure, 250);
    window.addEventListener("scroll", schedule, true);
    window.addEventListener("resize", schedule);
    return () => {
      disposed = true;
      clearInterval(interval);
      cancelAnimationFrame(frame);
      window.removeEventListener("scroll", schedule, true);
      window.removeEventListener("resize", schedule);
    };
    // The targets' identity is their content.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);
  return { ...state, latest };
}

// ---------------------------------------------------------------------------
// Step resolution: what the active step shows on this page
// ---------------------------------------------------------------------------

interface StepView {
  /** The record step this view belongs to (0–6), or 7 for the closing card. */
  step: number;
  copy: OnboardingStepCopy;
  targets: readonly TourTarget[];
  /** Pointer events outside the targets and the card are swallowed. */
  block: boolean;
  /** The primary control is disabled until this clears. */
  nextDisabled?: boolean;
  /** Pressing one of the targets finishes the step (the Guided hand-off). */
  advanceOnTargetPress?: boolean;
  /** The controls the pulse ring marks; the whole spotlight by default. */
  pulse?: readonly TourTarget[];
  /** The secondary control: another way to take the step. */
  secondary?: { label: string; testId: string };
  /** Where the card goes, first fit wins; below then above by default. */
  placement?: readonly CardSide[];
  /** The card shows above an open modal instead of hiding behind it. */
  overOverlay?: boolean;
  /** No dim and no blocking: only the ring marks the control. */
  dim?: boolean;
  /** The card does not take focus (a dialog's own focus wins). */
  autoFocus?: boolean;
  /** Nothing shows until a target resolves. */
  requireTarget?: boolean;
  /**
   * The card keeps clear of the first target's closest ancestor matching
   * this selector (a dialog), while the arrow still points at the target.
   */
  avoidAncestor?: string;
  testId: string;
}

/** The Issues tile's Create, the manual path and the timeout fallback. */
const ISSUE_CREATE_TARGET: TourTarget = {
  selectors: [
    '[data-testid="home-issues-create"]',
    '[data-testid="home-issues-tile"]',
  ],
};

type CardSide = "below" | "above" | "left" | "right";

/** The chat lives in a side panel: a card above its composer would cover
 * the transcript, so these steps sit beside it. */
const BESIDE_CHAT: readonly CardSide[] = ["left", "right", "below", "above"];
/** The Start workspace dialog is centered: outside it when there is room. */
const BESIDE_DIALOG: readonly CardSide[] = ["below", "right", "left", "above"];

const PERSONAL_CHAT = '[data-testid="home-personal-chat"]';

// ---------------------------------------------------------------------------
// The controller
// ---------------------------------------------------------------------------

export function OnboardingTour() {
  const { user, applyProfile, principalKey } = useAuth();
  // The Guided step's hand-off needs the Guided section, which the person's
  // setting hides by default (ui g5.ui.guided-toggle): while it is off the
  // tour has six steps and ends when the workspace route loads.
  const { settings: userSettings } = useUserSettings();
  const guidedExperienceOn = userSettings.showGuidedJourney;
  const guidedExperienceOnRef = useRef(guidedExperienceOn);
  guidedExperienceOnRef.current = guidedExperienceOn;
  const stepTotal = guidedExperienceOn
    ? ONBOARDING_STEP_COUNT
    : ONBOARDING_STEP_COUNT - 1;
  const location = useLocation();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const isDesktop = useIsDesktop();
  const coveringOverlayOpen = useCoveringOverlayOpen();

  // The operator organization's Home is the platform console (organizations
  // g5.organization.platform-operator.home), not a development Home: none of
  // the tour's anchors live there, so its pages host no step.
  const operatorSlug =
    user?.orgs.find((membership) => membership.orgKind === "operator")
      ?.orgSlug ?? null;
  const here = useMemo((): TourLocation => {
    const located = locateTour(location.pathname, location.search);
    return located.orgSlug !== null && located.orgSlug === operatorSlug
      ? { ...located, page: "other" }
      : located;
  }, [location.pathname, location.search, operatorSlug]);
  // The organization the tour addresses: the route's, else the token's,
  // else the first; always a tenant organization.
  const org = useMemo(() => {
    if (!user) return null;
    const tenants = user.orgs.filter(
      (membership) => membership.orgKind !== "operator",
    );
    return (
      tenants.find((membership) => membership.orgSlug === here.orgSlug) ??
      tenants.find((membership) => membership.orgId === user.activeOrgId) ??
      tenants[0] ??
      null
    );
  }, [user, here.orgSlug]);
  const isViewer = org?.role === "viewer";

  // The active step (null: nothing shows). `replaying` marks a run from
  // Settings whose position the record does not carry.
  const [cursor, setCursor] = useState<number | null>(null);
  const [replaying, setReplaying] = useState(false);
  const [viewerClosing, setViewerClosing] = useState(false);
  const startedForRef = useRef<string | null>(null);
  /** The person who skipped or finished in this tab: no auto-start for them
   * while the server's answer is still on its way. */
  const dismissedForRef = useRef<string | null>(null);
  /** The highest step this mount already wrote, so a write is not repeated. */
  const recordedRef = useRef<number>(-1);

  // Forget the run when the person changes: nothing of one person's tour
  // reaches the next one in this tab. Declared before the start effect so a
  // change resets first and the start effect then decides afresh.
  useEffect(() => {
    const id = user?.id ?? null;
    if (personRef.current === id) return;
    const previous = personRef.current;
    personRef.current = id;
    if (previous === null) return;
    setCursor(null);
    setReplaying(false);
    setViewerClosing(false);
    startedForRef.current = null;
    dismissedForRef.current = null;
    recordedRef.current = -1;
  }, [user?.id]);

  // Start: a replay in progress, else an unfinished unskipped tour, else the
  // welcome card for a person it was never shown to, on Home only.
  useEffect(() => {
    if (!user || cursor !== null) return;
    if (dismissedForRef.current === user.id) return;
    if (startedForRef.current === user.id) {
      // Already decided for this person; a never-shown person waits for Home.
      if (!onboardingNeverShown(user) || here.page !== "home") return;
    }
    startedForRef.current = user.id;
    const replayCursor = readReplayCursor(user.id);
    if (replayCursor !== null) {
      setReplaying(true);
      setCursor(replayCursor);
      return;
    }
    const resume = onboardingResumeStep(user);
    if (resume !== null) {
      setCursor(resume);
      return;
    }
    if (onboardingNeverShown(user) && here.page === "home" && isDesktop) {
      setCursor(0);
    }
  }, [user, cursor, here.page, isDesktop]);

  // Persistence: every step reached raises the record; the step never goes
  // down server-side, so a replay's writes are harmless.
  // A write is bound to the person and token it was made for: the API
  // refuses it under another principal's token, and a retry after the
  // person changed is dropped rather than sent as them.
  const principalRef = useRef(principalKey);
  principalRef.current = principalKey;
  const personRef = useRef<string | null>(null);
  const userRef = useRef(user);
  userRef.current = user;
  const patchRecord = useCallback(
    async (
      body: { onboardingStep?: number; onboardingSkipped?: boolean },
      attempts = 2,
    ): Promise<boolean> => {
      const principal = principalRef.current;
      const person = personRef.current;
      for (let attempt = 1; attempt <= attempts; attempt += 1) {
        if (personRef.current !== person) return false;
        try {
          const updated = await api.patch<CurrentUserDTO>(
            routes.users.me,
            body,
            { principal },
          );
          // A response that arrives after the person changed, or after a
          // later write already answered, never moves the record backwards.
          if (personRef.current !== person) return false;
          const current = userRef.current;
          if (
            current &&
            current.id === updated.id &&
            (updated.onboardingStep ?? -1) >= (current.onboardingStep ?? -1)
          ) {
            applyProfile(updated);
          }
          return true;
        } catch (error) {
          console.warn("[onboarding] could not record tour progress", error);
          if (attempt < attempts) {
            await new Promise((resolve) => setTimeout(resolve, 1500));
          }
        }
      }
      return false;
    },
    [applyProfile],
  );
  useEffect(() => {
    if (cursor === null || !user) return;
    if (replaying) {
      writeReplayCursor(user.id, cursor);
    }
    const known = Math.max(user.onboardingStep ?? -1, recordedRef.current);
    if (cursor > known) {
      // Count the write only once the server answered; a failed one is
      // retried by the next step's write, which carries a higher value.
      const step = cursor;
      const person = user.id;
      void patchRecord({ onboardingStep: step }).then((ok) => {
        if (ok && personRef.current === person) {
          recordedRef.current = Math.max(recordedRef.current, step);
        }
      });
    }
  }, [cursor, user, replaying, patchRecord]);

  const userId = user?.id ?? null;
  const clearRun = useCallback(() => {
    setCursor(null);
    setReplaying(false);
    setViewerClosing(false);
    dismissedForRef.current = userId;
    if (userId) {
      writeReplayCursor(userId, null);
      writeIssueBaseline(userId, null);
      writeFoundIssue(userId, null);
    }
  }, [userId]);

  const finish = useCallback(() => {
    clearRun();
    void patchRecord({
      onboardingStep: ONBOARDING_STEP_COUNT,
      onboardingSkipped: false,
    });
  }, [clearRun, patchRecord]);

  const skip = useCallback(() => {
    const at = cursor ?? 0;
    clearRun();
    void patchRecord({ onboardingStep: at, onboardingSkipped: true });
  }, [cursor, clearRun, patchRecord]);

  const replay = useCallback(() => {
    if (userId) {
      writeReplayCursor(userId, 0);
      writeIssueBaseline(userId, null);
      writeFoundIssue(userId, null);
    }
    dismissedForRef.current = null;
    setReplaying(true);
    setViewerClosing(false);
    setCursor(0);
    if (org) navigate(`/${org.orgSlug}/home`);
    if (user?.onboardingSkipped) void patchRecord({ onboardingSkipped: false });
  }, [navigate, org, patchRecord, user?.onboardingSkipped, userId]);

  // ----- Facts the steps branch on --------------------------------------
  const accountsNeeded = cursor === 2 && !!org;
  const accounts = useProviderAccounts(accountsNeeded ? org!.orgId : "");
  const orgAccountExists =
    accounts.status === "ok" &&
    accounts.data.some((account) => account.scope === "org");
  const ownAccountExists =
    accounts.status === "ok" &&
    accounts.data.some((account) => account.scope === "user");

  const firstName = user ? onboardingFirstName(user) : "";
  const issueTitle = onboardingIssueTitle(firstName);

  // ----- Route-driven advances ----------------------------------------
  // A step advances on the page change its action causes, never merely
  // because the person is already on the next page: Replay pressed on the
  // settings view sets step 0 there and must still begin on Home.
  const cursorRef = useRef<number | null>(null);
  cursorRef.current = cursor;
  const pageRef = useRef<TourLocation["page"]>(here.page);
  useEffect(() => {
    const from = pageRef.current;
    const to = here.page;
    pageRef.current = to;
    if (from === to) return;
    const at = cursorRef.current;
    // Settings is reached through the `/:org/settings` redirect, so the
    // page before it is that redirect's, not Home.
    if (at === 0 && from !== "settings" && to === "settings") setCursor(1);
    // The Create fallback lands on the new issue's page: that is the issue.
    if (at === 4 && from === "home" && to === "issue") {
      if (userId) writeIssueBaseline(userId, null);
      setCursor(5);
    }
    // Reaching a workspace moves to the Guided step, or ends the tour while
    // the Guided section is off (ui g5.ui.guided-toggle).
    const reachGuided = () => {
      if (guidedExperienceOnRef.current) setCursor(6);
      else finish();
    };
    // Create & Fork Workspace from the fallback reaches a workspace at once.
    if (at === 4 && from === "home" && to === "workspace") {
      if (userId) writeIssueBaseline(userId, null);
      reachGuided();
    }
    if (
      at === 5 &&
      (from === "issue" || from === "home") &&
      to === "workspace"
    ) {
      reachGuided();
    }
  }, [here.page, userId, finish]);

  // A tour resumed at the Guided step while the section is off has no step
  // left: it is finished.
  useEffect(() => {
    if (cursor === 6 && !guidedExperienceOn && !viewerClosing) finish();
  }, [cursor, guidedExperienceOn, viewerClosing, finish]);

  // ----- Step 5: the issue the agent creates --------------------------
  const [issueFallback, setIssueFallback] = useState(false);
  /** "Create issue manually" pressed: the tile's Create is the control. */
  const [issueManual, setIssueManual] = useState(false);
  const [foundIssue, setFoundIssue] = useState<FoundIssue | null>(null);
  useEffect(() => {
    setFoundIssue(userId ? readFoundIssue(userId) : null);
  }, [userId]);
  const [composerSeed, setComposerSeed] = useState<{
    token: number;
    text: string;
  } | null>(null);
  useEffect(() => {
    if (cursor !== 4 || here.page !== "home" || !org || !user) {
      setComposerSeed(null);
      setIssueFallback(false);
      setIssueManual(false);
      return;
    }
    setComposerSeed({
      token: Date.now(),
      text: onboardingIssueMessage(firstName),
    });
    const startedAt = Date.now();
    let disposed = false;
    const stored = readIssueBaseline(user.id);
    let baseline: Set<string> | null = stored ? new Set(stored) : null;
    const orgId = org.orgId;
    const personId = user.id;
    const poll = async () => {
      try {
        const response = await api.get<ExternalIssueSearchResponseDTO>(
          routes.orgs.issueSearch(orgId),
          { params: { q: issueTitle, state: "open" } },
        );
        if (disposed) return;
        const matching = response.results.filter(
          (issue) => issue.title.trim() === issueTitle,
        );
        if (baseline === null) {
          // Issues a replay left behind do not count; the baseline survives
          // the Create fallback's navigation and a reload.
          baseline = new Set(matching.map((issue) => issue.displayKey));
          writeIssueBaseline(personId, [...baseline]);
          return;
        }
        const created = matching.find(
          (issue) => !baseline!.has(issue.displayKey),
        );
        if (created) {
          void queryClient.invalidateQueries({
            queryKey: externalIssueKeys.searchAll,
          });
          const found = {
            externalIssueId: created.externalIssueId,
            displayKey: created.displayKey,
          };
          writeIssueBaseline(personId, null);
          writeFoundIssue(personId, found);
          setFoundIssue(found);
          setCursor(5);
          return;
        }
      } catch (error) {
        console.warn("[onboarding] issue search failed", error);
      }
      if (!disposed && Date.now() - startedAt >= ONBOARDING_ISSUE_WAIT_MS) {
        setIssueFallback(true);
      }
    };
    void poll();
    const timer = setInterval(() => void poll(), ONBOARDING_ISSUE_POLL_MS);
    return () => {
      disposed = true;
      clearInterval(timer);
    };
    // `firstName`/`issueTitle` derive from `user`.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [cursor, here.page, org?.orgId, user?.id, queryClient]);

  // ----- The view -------------------------------------------------------
  const view = useMemo<StepView | null>(() => {
    if (cursor === null || !user) return null;
    if (viewerClosing) {
      return {
        step: ONBOARDING_STEP_COUNT,
        copy: ONBOARDING_VIEWER_CLOSING_COPY,
        targets: [],
        block: true,
        testId: "onboarding-step-closing",
      };
    }
    const id = ONBOARDING_STEP_IDS[cursor];
    if (!id) return null;
    const expected = stepPage(cursor);
    const onPage =
      here.page === expected ||
      (cursor === 5 && here.page === "issue") ||
      (cursor === 6 && here.page === "home");
    if (!onPage) return null;
    switch (id) {
      case "welcome":
        return {
          step: 0,
          copy: ONBOARDING_COPY.welcome,
          targets: [{ selectors: ['[data-testid="settings-button"]'] }],
          block: true,
          testId: "onboarding-step-welcome",
        };
      case "theme":
        return {
          step: 1,
          copy: ONBOARDING_COPY.theme,
          targets: [
            {
              selectors: [
                '[data-testid="settings-row-theme"]',
                '[data-testid="theme-selector"]',
              ],
            },
            {
              selectors: [
                '[data-testid="settings-row-mode"]',
                '[data-testid="mode-toggle"]',
              ],
            },
          ],
          block: true,
          testId: "onboarding-step-theme",
        };
      case "accounts":
        return {
          step: 2,
          copy: orgAccountExists
            ? ONBOARDING_COPY.accounts
            : isViewer
              ? ONBOARDING_ACCOUNTS_VIEWER_COPY
              : ONBOARDING_ACCOUNTS_OWN_COPY,
          targets: [
            {
              selectors: [
                '[data-testid="settings-section-accounts"]',
                '[data-testid="provider-accounts-section"]',
              ],
            },
          ],
          block: true,
          // A viewer cannot add an account (admission needs workspace
          // write), so the step only informs them.
          nextDisabled:
            !isViewer &&
            (accounts.status !== "ok" ||
              (!orgAccountExists && !ownAccountExists)),
          testId: "onboarding-step-accounts",
        };
      case "scratchpad":
        return {
          step: 3,
          copy: ONBOARDING_COPY.scratchpad,
          targets: [
            {
              selectors: [
                `${PERSONAL_CHAT} [data-testid="chat-input"]`,
                PERSONAL_CHAT,
                '[data-testid="home-personal-chat-collapsed"]',
              ],
            },
          ],
          block: true,
          placement: BESIDE_CHAT,
          testId: "onboarding-step-scratchpad",
        };
      case "create-issue":
        if (issueManual) {
          return {
            step: 4,
            copy: ONBOARDING_ISSUE_MANUAL_COPY,
            targets: [ISSUE_CREATE_TARGET],
            block: true,
            testId: "onboarding-step-create-issue",
          };
        }
        return {
          step: 4,
          copy: issueFallback
            ? ONBOARDING_ISSUE_FALLBACK_COPY
            : ONBOARDING_COPY["create-issue"],
          targets: issueFallback
            ? [
                ISSUE_CREATE_TARGET,
                { selectors: [PERSONAL_CHAT] },
                { selectors: ['[data-testid="home-personal-chat-collapsed"]'] },
              ]
            : [
                {
                  selectors: [
                    `${PERSONAL_CHAT} [data-testid="chat-agent-options-row"]`,
                  ],
                },
                { selectors: [`${PERSONAL_CHAT} [data-testid="chat-input"]`] },
                {
                  selectors: [
                    `${PERSONAL_CHAT} [data-testid="chat-panel-no-session-send"]`,
                    `${PERSONAL_CHAT} [data-testid="chat-panel-send"]`,
                  ],
                },
                { selectors: [PERSONAL_CHAT] },
                { selectors: ['[data-testid="home-personal-chat-collapsed"]'] },
              ],
          block: true,
          // The whole chat stays usable; the pulse marks the pickers to set.
          pulse: issueFallback
            ? undefined
            : [
                {
                  selectors: [
                    `${PERSONAL_CHAT} [data-testid="chat-agent-options-row"]`,
                  ],
                },
              ],
          secondary: issueFallback
            ? undefined
            : {
                label: ONBOARDING_COPY["create-issue"].secondary ?? "",
                testId: "onboarding-create-issue-manually",
              },
          placement: issueFallback ? undefined : BESIDE_CHAT,
          testId: "onboarding-step-create-issue",
        };
      case "start-workspace": {
        if (coveringOverlayOpen) {
          // Start pressed: the fork dialog is open. The card says the person
          // can fork now or close it and come back; the dialog stays fully
          // usable (no dim, no blocking, focus left to the dialog) and the
          // card only shows once the dialog's own Fork button is on screen.
          return {
            step: 5,
            copy: ONBOARDING_START_DIALOG_COPY,
            targets: [
              {
                selectors: [
                  '[data-testid="clone-submit-button"]',
                  '[data-testid="clone-open-workspace-button"]',
                ],
              },
            ],
            block: false,
            overOverlay: true,
            dim: false,
            autoFocus: false,
            requireTarget: true,
            avoidAncestor: '[role="dialog"]',
            placement: BESIDE_DIALOG,
            testId: "onboarding-step-start-workspace-dialog",
          };
        }
        if (here.page === "issue") {
          // An issue already attached to a workspace has no Start control;
          // the page's own links stay usable.
          return {
            step: 5,
            copy: ONBOARDING_COPY["start-workspace"],
            targets: [
              { selectors: ['[data-testid="issue-page-start-workspace"]'] },
            ],
            block: false,
            testId: "onboarding-step-start-workspace",
          };
        }
        // The row's own Start action (a GitHub title link would leave G5):
        // the row of the issue this run detected when known, else the row
        // whose text carries the title; without that row (paged away), the
        // whole tile stays usable so Show more and search can find it.
        return {
          step: 5,
          copy: ONBOARDING_COPY["start-workspace"],
          targets: [
            {
              selectors: ['[data-testid="home-issue-start-workspace"]'],
              within: foundIssue
                ? {
                    selector: `[data-testid="home-issue-${foundIssue.externalIssueId}"]`,
                    fallbackSelectors: ['[data-testid="home-issues-tile"]'],
                  }
                : {
                    selector: '[data-testid^="home-issue-"]',
                    testIdPattern: /^home-issue-\d+$/,
                    text: issueTitle,
                    fallbackSelectors: ['[data-testid="home-issues-tile"]'],
                  },
            },
          ],
          block: true,
          testId: "onboarding-step-start-workspace",
        };
      }
      case "guided":
        if (here.page === "home") {
          return {
            step: 6,
            copy: ONBOARDING_WORKSPACE_PROMPT_COPY,
            targets: [{ selectors: ['[data-testid="home-workspaces"]'] }],
            block: false,
            testId: "onboarding-step-guided-prompt",
          };
        }
        // The sidebar's Guided entry: the rail's compass icon, else the
        // full sidebar's section header.
        return {
          step: 6,
          copy: ONBOARDING_COPY.guided,
          targets: [
            {
              selectors: [
                '[data-testid="sidebar-rail-guided"]',
                '[data-testid="sidebar-guided-tab"] [data-surface="section-header"]',
                '[data-testid="sidebar-guided-tab"]',
              ],
            },
          ],
          block: false,
          advanceOnTargetPress: true,
          testId: "onboarding-step-guided",
        };
      default:
        return null;
    }
  }, [
    cursor,
    user,
    viewerClosing,
    here.page,
    orgAccountExists,
    ownAccountExists,
    accounts.status,
    isViewer,
    issueFallback,
    issueManual,
    issueTitle,
    foundIssue,
    coveringOverlayOpen,
  ]);

  const secondary = useCallback(() => {
    if (cursor === 4) setIssueManual(true);
  }, [cursor]);

  const next = useCallback(() => {
    if (cursor === null) return;
    if (viewerClosing) {
      finish();
      return;
    }
    switch (ONBOARDING_STEP_IDS[cursor]) {
      case "welcome":
        // The card's own button does what the spotlit control does; the
        // route change then advances the step.
        if (org) navigate(`/${org.orgSlug}/settings`);
        return;
      case "theme":
        setCursor(2);
        return;
      case "accounts":
        if (isViewer) {
          setViewerClosing(true);
          return;
        }
        setCursor(3);
        if (org) navigate(`/${org.orgSlug}/home`);
        return;
      case "scratchpad":
        setCursor(4);
        return;
      case "guided":
        finish();
        return;
      default:
        return;
    }
  }, [cursor, viewerClosing, finish, isViewer, org, navigate]);

  // Off-page: a paused step on another page offers the way there.
  const continueTarget = useMemo(() => {
    if (cursor === null || view || !org || viewerClosing) return null;
    if (coveringOverlayOpen) return null;
    const page = stepPage(cursor);
    if (page === "settings") return `/${org.orgSlug}/settings`;
    return `/${org.orgSlug}/home`;
  }, [cursor, view, org, viewerClosing, coveringOverlayOpen]);

  const published = useMemo<OnboardingTourValue>(
    () => ({ composerSeed, replay }),
    [composerSeed, replay],
  );
  useEffect(() => {
    publishOnboardingTour(published);
    return () => publishOnboardingTour(null);
  }, [published]);

  const showView =
    !!view && isDesktop && (!coveringOverlayOpen || !!view.overOverlay);
  const stepNumber =
    cursor === null ? 0 : Math.min(cursor, ONBOARDING_STEP_COUNT - 1) + 1;

  return (
    <>
      {showView && view && user && (
        <TourStep
          key={view.testId}
          view={view}
          stepNumber={stepNumber}
          stepTotal={stepTotal}
          values={{
            name: firstName,
            org: org?.orgName ?? "Your organization",
            issue: issueTitle,
          }}
          onNext={next}
          onSecondary={secondary}
          onSkip={skip}
          onTargetPress={view.advanceOnTargetPress ? finish : undefined}
        />
      )}
      {!showView && continueTarget && isDesktop && (
        <button
          type="button"
          data-testid="onboarding-continue"
          onClick={() => navigate(continueTarget)}
          className={classNames(
            "fixed bottom-4 left-1/2 z-[65] -translate-x-1/2 rounded-full px-4 py-2 text-sm font-medium shadow-lg",
            "border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] text-[var(--g5-text0)] cursor-pointer",
            "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)] focus-visible:ring-offset-2",
          )}
        >
          Continue tour ({stepNumber} of {stepTotal})
        </button>
      )}
    </>
  );
}

// ---------------------------------------------------------------------------
// One step on screen: dim, spotlight, card, blocking
// ---------------------------------------------------------------------------

interface TourStepProps {
  view: StepView;
  stepNumber: number;
  /** Seven steps, or six while the Guided section is off. */
  stepTotal: number;
  values: { name: string; org: string; issue: string };
  onNext: () => void;
  onSecondary: () => void;
  onSkip: () => void;
  onTargetPress?: () => void;
}

const NO_TARGETS: readonly TourTarget[] = [];

const FOCUSABLE =
  'a[href], button, input, select, textarea, [tabindex]:not([tabindex="-1"])';

const BLOCKED_EVENTS = [
  "pointerdown",
  "pointerup",
  "mousedown",
  "mouseup",
  "click",
  "dblclick",
  "contextmenu",
  "touchstart",
] as const;

function TourStep({
  view,
  stepNumber,
  stepTotal,
  values,
  onNext,
  onSecondary,
  onSkip,
  onTargetPress,
}: TourStepProps) {
  const {
    rect,
    obstacle,
    elements,
    latest: liveElementsRef,
  } = useTargetRect(view.targets, view.avoidAncestor);
  // Whether an event landed on a spotlit control: the last measurement
  // first, then a fresh resolution for a target that mounted since (the
  // rail overlay, the Guided panel), so no press or click is lost to the
  // measurement interval.
  const onTarget = useCallback(
    (target: EventTarget | null): boolean => {
      if (!(target instanceof Node)) return false;
      const within = (element: Element) => element.contains(target);
      if (liveElementsRef.current.some(within)) return true;
      const fresh = view.targets.flatMap((item) => resolveTarget(item));
      if (fresh.length > 0) liveElementsRef.current = fresh;
      return fresh.some(within);
    },
    [liveElementsRef, view.targets],
  );
  const pulseOwn = useTargetRect(view.pulse ?? NO_TARGETS);
  const pulseRect = view.pulse ? pulseOwn.rect : rect;
  const cardRef = useRef<HTMLDivElement | null>(null);
  const [cardSize, setCardSize] = useState({ width: 360, height: 160 });

  // Bring the first target into view when the step starts and when it
  // changes shape (a late-mounted panel).
  const firstTarget = elements[0] ?? null;
  const scrolledForRef = useRef<Element | null>(null);
  useEffect(() => {
    if (!firstTarget || scrolledForRef.current === firstTarget) return;
    scrolledForRef.current = firstTarget;
    try {
      firstTarget.scrollIntoView({ block: "center", inline: "nearest" });
    } catch {
      // Older browsers: the person can scroll.
    }
  }, [firstTarget]);

  // The card mounts late when the step waits for its target (the dialog
  // card), so the measurement attaches whenever it appears.
  const cardShown = !(view.requireTarget && !rect);
  // `noRoom` below also unmounts the card; its size is then last measured.
  useEffect(() => {
    const card = cardRef.current;
    if (!card || !cardShown) return;
    const measure = () =>
      setCardSize({ width: card.offsetWidth, height: card.offsetHeight });
    measure();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(measure);
    observer.observe(card);
    return () => observer.disconnect();
  }, [cardShown]);

  // Swallow pointer events outside the spotlit controls and the card.
  useEffect(() => {
    if (!view.block) return;
    const root = document.getElementById("root");
    const allowed = (target: EventTarget | null): boolean => {
      if (!(target instanceof Node)) return false;
      if (cardRef.current?.contains(target)) return true;
      if (onTarget(target)) return true;
      // The model and account pickers render into document.body, outside
      // the app root; what the spotlit control opens stays usable.
      return !!root && !root.contains(target);
    };
    const handler = (event: Event) => {
      if (allowed(event.target)) return;
      event.preventDefault();
      event.stopPropagation();
    };
    for (const type of BLOCKED_EVENTS) {
      document.addEventListener(type, handler, true);
    }
    return () => {
      for (const type of BLOCKED_EVENTS) {
        document.removeEventListener(type, handler, true);
      }
    };
  }, [view.block, onTarget]);

  // The Guided hand-off: pressing the spotlit control ends the tour. The
  // full sidebar's section header is a div that activates on Enter and
  // Space without a click, so those keys count too; one press ends once.
  useEffect(() => {
    if (!onTargetPress) return;
    let fired = false;
    const pressed = onTarget;
    const onClick = (event: Event) => {
      if (fired || !pressed(event.target)) return;
      fired = true;
      onTargetPress();
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Enter" && event.key !== " ") return;
      if (fired || !pressed(event.target)) return;
      fired = true;
      onTargetPress();
    };
    document.addEventListener("click", onClick, true);
    document.addEventListener("keydown", onKey, true);
    return () => {
      document.removeEventListener("click", onClick, true);
      document.removeEventListener("keydown", onKey, true);
    };
  }, [onTargetPress, onTarget]);

  // Keyboard: focus lands on the card when a step shows; Tab cycles through
  // the card's controls and the spotlit controls, nothing else.
  const autoFocus = view.autoFocus ?? true;
  useEffect(() => {
    if (autoFocus) cardRef.current?.focus({ preventScroll: true });
  }, [autoFocus]);
  useEffect(() => {
    if (!view.block) return;
    const handler = (event: KeyboardEvent) => {
      if (event.key !== "Tab") return;
      const card = cardRef.current;
      if (!card) return;
      // What a spotlit control opened (the pickers' popovers) portals to
      // the body outside the app root; its controls join the ring.
      const root = document.getElementById("root");
      const portalled = root
        ? Array.from(
            document.body.querySelectorAll<HTMLElement>(FOCUSABLE),
          ).filter(
            (element) => !root.contains(element) && !card.contains(element),
          )
        : [];
      // A control reached both on its own and through an enclosing target
      // enters the ring once, or Tab would loop over the first copies.
      const ring: HTMLElement[] = Array.from(
        new Set<HTMLElement>([
          ...Array.from(card.querySelectorAll<HTMLElement>(FOCUSABLE)),
          ...liveElementsRef.current.flatMap((element) =>
            element.matches(FOCUSABLE)
              ? [element as HTMLElement]
              : Array.from(element.querySelectorAll<HTMLElement>(FOCUSABLE)),
          ),
          ...portalled,
        ]),
      ).filter(
        (element) =>
          !element.hasAttribute("disabled") &&
          (typeof element.checkVisibility !== "function" ||
            element.checkVisibility()),
      );
      if (ring.length === 0) return;
      event.preventDefault();
      const index = ring.indexOf(document.activeElement as HTMLElement);
      const step = event.shiftKey ? -1 : 1;
      const next =
        index === -1 ? 0 : (index + step + ring.length) % ring.length;
      ring[next].focus();
    };
    document.addEventListener("keydown", handler, true);
    return () => document.removeEventListener("keydown", handler, true);
  }, [view.block, liveElementsRef]);

  const dim = view.dim ?? true;
  const copyText = fillOnboardingCopy(view.copy.text, values);
  const copyTitle = view.copy.title
    ? fillOnboardingCopy(view.copy.title, values)
    : null;

  // Card placement: the first side of the step's order where the card fits
  // (below then above by default), else above clamped to the viewport;
  // centered when nothing is spotlit. The card keeps clear of the obstacle
  // (the target itself, or the dialog around it) while the arrow points at
  // the target; a dialog card that fits nowhere stays hidden.
  const viewportWidth =
    typeof window === "undefined" ? 1280 : window.innerWidth;
  const viewportHeight =
    typeof window === "undefined" ? 800 : window.innerHeight;
  const margin = 16;
  let cardStyle: React.CSSProperties;
  // The card's arrow points at the spotlight from the edge facing it.
  let arrow: { side: "top" | "bottom" | "left" | "right"; at: number } | null =
    null;
  let noRoom = false;
  if (rect && obstacle) {
    const box = obstacle;
    const clampLeft = (left: number) =>
      Math.min(
        Math.max(margin, left),
        Math.max(margin, viewportWidth - cardSize.width - margin),
      );
    const clampTop = (top: number) =>
      Math.min(
        Math.max(margin, top),
        Math.max(margin, viewportHeight - cardSize.height - margin),
      );
    const arrowAt = (centre: number, extent: number) =>
      Math.min(Math.max(16, centre), Math.max(16, extent - 16));
    // A card beside the obstacle: vertically centred on the target, clamped
    // to the viewport, its arrow on the facing edge pointing at the target.
    const beside = (
      left: number,
      width: number,
      arrowSide: "left" | "right",
    ): Placed => {
      const top = clampTop(rect.top + rect.height / 2 - cardSize.height / 2);
      return {
        style: { top, left, width },
        arrow: {
          side: arrowSide,
          at: arrowAt(rect.top + rect.height / 2 - top, cardSize.height),
        },
      };
    };
    const sideWidth = (avail: number): number | null => {
      if (avail >= CARD_WIDTH) return CARD_WIDTH;
      // Only a dialog card narrows; beside its own target the card keeps
      // its width and the next side is tried.
      if (box === rect || avail < CARD_MIN_WIDTH) return null;
      return Math.floor(avail);
    };
    type Placed = {
      style: React.CSSProperties;
      arrow: { side: "top" | "bottom" | "left" | "right"; at: number };
    };
    const place = (side: CardSide): Placed | null => {
      switch (side) {
        case "below": {
          const top = box.top + box.height + SPOTLIGHT_PAD + margin;
          if (top + cardSize.height > viewportHeight - margin) return null;
          const left = clampLeft(rect.left);
          return {
            style: { top, left },
            arrow: {
              side: "top",
              at: arrowAt(rect.left + rect.width / 2 - left, cardSize.width),
            },
          };
        }
        case "above": {
          const top = box.top - SPOTLIGHT_PAD - margin - cardSize.height;
          if (top < margin) return null;
          const left = clampLeft(rect.left);
          return {
            style: { top, left },
            arrow: {
              side: "bottom",
              at: arrowAt(rect.left + rect.width / 2 - left, cardSize.width),
            },
          };
        }
        case "left": {
          const avail = box.left - SPOTLIGHT_PAD - margin - margin;
          const width = sideWidth(avail);
          if (width === null) return null;
          return beside(
            box.left - SPOTLIGHT_PAD - margin - width,
            width,
            "right",
          );
        }
        case "right": {
          const left = box.left + box.width + SPOTLIGHT_PAD + margin;
          const width = sideWidth(viewportWidth - margin - left);
          if (width === null) return null;
          return beside(left, width, "left");
        }
      }
    };
    let placed: Placed | null = null;
    for (const side of view.placement ?? ["below", "above"]) {
      placed = place(side);
      if (placed) break;
    }
    if (placed) {
      cardStyle = placed.style;
      arrow = placed.arrow;
    } else if (box !== rect) {
      // Nowhere outside the dialog: the card would cover its fields, so it
      // stays away and the dialog is used as without the tour.
      noRoom = true;
      cardStyle = {};
    } else {
      const top = Math.max(
        margin,
        rect.top - SPOTLIGHT_PAD - margin - cardSize.height,
      );
      const left = clampLeft(rect.left);
      cardStyle = { top, left };
      arrow = {
        side: "bottom",
        at: arrowAt(rect.left + rect.width / 2 - left, cardSize.width),
      };
    }
  } else {
    cardStyle = {
      top: Math.max(margin, (viewportHeight - cardSize.height) / 2),
      left: Math.max(margin, (viewportWidth - cardSize.width) / 2),
    };
  }

  if ((view.requireTarget && !rect) || noRoom) return null;

  return (
    <div data-testid="onboarding-tour" className="contents">
      {/* The dim: one hole whose shadow covers everything else; a dialog
          card keeps the ring and drops the shadow. */}
      <div
        aria-hidden="true"
        data-testid="onboarding-spotlight"
        data-dim={dim ? "true" : "false"}
        className="pointer-events-none fixed z-[64] rounded-[8px] ring-2 ring-[var(--g5-info)]"
        style={{
          ...(rect
            ? {
                top: rect.top - SPOTLIGHT_PAD,
                left: rect.left - SPOTLIGHT_PAD,
                width: rect.width + SPOTLIGHT_PAD * 2,
                height: rect.height + SPOTLIGHT_PAD * 2,
              }
            : { top: 0, left: 0, width: 0, height: 0 }),
          ...(dim ? { boxShadow: "0 0 0 9999px rgba(0, 0, 0, 0.45)" } : {}),
        }}
      />
      {pulseRect && (
        <div
          aria-hidden="true"
          data-testid="onboarding-spotlight-pulse"
          className="g5-tour-pulse pointer-events-none fixed z-[64] rounded-[10px] border-2 border-[var(--g5-info)]"
          style={{
            top: pulseRect.top - SPOTLIGHT_PAD - 2,
            left: pulseRect.left - SPOTLIGHT_PAD - 2,
            width: pulseRect.width + SPOTLIGHT_PAD * 2 + 4,
            height: pulseRect.height + SPOTLIGHT_PAD * 2 + 4,
          }}
        />
      )}
      <div
        ref={cardRef}
        tabIndex={-1}
        role="dialog"
        aria-modal="false"
        aria-labelledby="onboarding-card-title"
        aria-describedby="onboarding-card-text"
        data-testid={view.testId}
        className={classNames(
          "fixed z-[66] w-[360px] max-w-[calc(100vw-32px)] rounded-[var(--g5-radius-md)] border border-[var(--g5-stroke)]",
          "bg-[var(--g5-bg1)] p-4 text-[var(--g5-text0)] shadow-2xl",
        )}
        style={cardStyle}
      >
        {arrow && (
          <span
            aria-hidden="true"
            data-testid="onboarding-card-arrow"
            data-side={arrow.side}
            className={classNames(
              "absolute h-3 w-3 rotate-45 border-[var(--g5-stroke)] bg-[var(--g5-bg1)]",
              arrow.side === "top" && "-top-[7px] border-l border-t",
              arrow.side === "bottom" && "-bottom-[7px] border-b border-r",
              arrow.side === "left" && "-left-[7px] border-l border-b",
              arrow.side === "right" && "-right-[7px] border-r border-t",
            )}
            style={
              arrow.side === "top" || arrow.side === "bottom"
                ? { left: arrow.at - 6 }
                : { top: arrow.at - 6 }
            }
          />
        )}
        <p
          className="text-[11px] font-medium uppercase tracking-wide text-[var(--g5-text2)]"
          data-testid="onboarding-card-counter"
        >
          Step {stepNumber} of {stepTotal}
        </p>
        <h2 id="onboarding-card-title" className="mt-1 text-base font-semibold">
          {copyTitle ?? "G5 tour"}
        </h2>
        <p
          id="onboarding-card-text"
          aria-live="polite"
          className="mt-2 text-sm leading-relaxed"
        >
          {copyText}
        </p>
        <div className="mt-4 flex items-center justify-between gap-2">
          <button
            type="button"
            data-testid="onboarding-skip"
            onClick={onSkip}
            className="text-sm text-[var(--g5-text2)] underline-offset-2 hover:underline cursor-pointer"
          >
            Skip tour
          </button>
          {view.secondary && (
            <Button
              kind="secondary"
              data-testid={view.secondary.testId}
              onClick={onSecondary}
            >
              {view.secondary.label}
            </Button>
          )}
          {view.copy.next && (
            <Button
              kind="primary"
              data-testid="onboarding-next"
              disabled={view.nextDisabled}
              onClick={onNext}
            >
              {view.copy.next}
            </Button>
          )}
        </div>
      </div>
    </div>
  );
}
