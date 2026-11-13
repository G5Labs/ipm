// =============================================================================
// G5 Client — Onboarding tour facts (ui g5.ui.onboarding-tour, users
// g5.user.onboarding)
// =============================================================================
// The pure part of the tour: the step list, what each step needs from the
// page, where it lives, and the Home defaults derived from the person's
// record. The controller (`components/onboarding/OnboardingTour.tsx`) owns
// the DOM; pages expose anchors and know nothing of the steps.
// =============================================================================

import type { CurrentUserDTO } from "./types";

/** Steps 0–6; 7 is the finished value (users g5.user.onboarding). */
export const ONBOARDING_STEP_COUNT = 7;

export type OnboardingStepId =
  | "welcome"
  | "theme"
  | "accounts"
  | "scratchpad"
  | "create-issue"
  | "start-workspace"
  | "guided";

export const ONBOARDING_STEP_IDS: readonly OnboardingStepId[] = [
  "welcome",
  "theme",
  "accounts",
  "scratchpad",
  "create-issue",
  "start-workspace",
  "guided",
];

/** Where a step is taken; the controller matches it against the route. */
export type OnboardingPage = "home" | "settings" | "issue" | "workspace";

export function stepPage(step: number): OnboardingPage {
  switch (ONBOARDING_STEP_IDS[step]) {
    case "theme":
    case "accounts":
      return "settings";
    case "start-workspace":
      return "home";
    case "guided":
      return "workspace";
    default:
      return "home";
  }
}

/** Whether the person has finished the tour (step 7). */
export function hasCompletedOnboarding(
  user: Pick<CurrentUserDTO, "onboardingStep"> | null | undefined,
): boolean {
  return (user?.onboardingStep ?? -1) >= ONBOARDING_STEP_COUNT;
}

/**
 * Home tiles open on All until the tour is finished (ui
 * g5.ui.home-workspaces): a newcomer sees the organization's work.
 */
export function homeDefaultsToAll(
  user: Pick<CurrentUserDTO, "onboardingStep"> | null | undefined,
): boolean {
  return !hasCompletedOnboarding(user);
}

/** The welcome card shows itself only to a person it was never shown to. */
export function onboardingNeverShown(
  user: Pick<CurrentUserDTO, "onboardingStep"> | null | undefined,
): boolean {
  return (user?.onboardingStep ?? null) === null;
}

/**
 * An unfinished, unskipped tour resumes at its recorded step (the person
 * chose "resume until completed or skipped").
 */
export function onboardingResumeStep(
  user:
    | Pick<CurrentUserDTO, "onboardingStep" | "onboardingSkipped">
    | null
    | undefined,
): number | null {
  const step = user?.onboardingStep ?? null;
  if (step === null) return null;
  if (step >= ONBOARDING_STEP_COUNT) return null;
  if (user?.onboardingSkipped) return null;
  return step;
}

/** The first name the copy addresses: the display name's first word. */
export function onboardingFirstName(
  user: Pick<CurrentUserDTO, "displayName" | "email">,
): string {
  const name = user.displayName.trim().split(/\s+/)[0];
  if (name) return name;
  return user.email.split("@")[0] || "there";
}

/** The title of the issue the person's agent creates in step 5. */
export function onboardingIssueTitle(firstName: string): string {
  return `[Test issue] ${firstName} onboarding`;
}

/** The message seeded into the scratchpad composer for step 5. */
export function onboardingIssueMessage(firstName: string): string {
  return `Create an issue titled "${onboardingIssueTitle(firstName)}", assigned to me, with a short body saying it is my onboarding test.`;
}

/**
 * The issue step waits this long for the agent before offering the manual
 * fallback (a stopped scratchpad boots first, which takes a minute or so).
 */
export const ONBOARDING_ISSUE_WAIT_MS = 4 * 60_000;
export const ONBOARDING_ISSUE_POLL_MS = 5_000;

/** Copy for each step; `{org}` and `{name}` are filled by the controller. */
export interface OnboardingStepCopy {
  title?: string;
  text: string;
  /** The primary control's label; absent when the step advances on its own. */
  next?: string;
  /** A secondary control's label: another way to take the step. */
  secondary?: string;
}

export const ONBOARDING_COPY: Record<OnboardingStepId, OnboardingStepCopy> = {
  welcome: {
    title: "Welcome to G5, {name}.",
    text: "Before your agents can run, let's set up your account. Press the pulsing Settings button to begin; skip any time and replay later from Settings.",
    next: "Open Settings",
  },
  theme: {
    text: "Pick a theme and light or dark. You can change this any time.",
    next: "Next",
  },
  accounts: {
    text: "Agents bill to an account. {org} already provides one, so you're set. Add your own subscription here if you prefer to use it.",
    next: "Next",
  },
  scratchpad: {
    text: "Back on Home, this is your personal scratchpad chat. It reads the org's code, but nothing changes until work moves to a workspace. Issues are how ideas leave this chat to start work in a writable workspace.",
    next: "Next",
  },
  "create-issue": {
    text: "Let's create your first issue. Pick a model above the composer if you like, then send this message.",
    secondary: "Create issue manually",
  },
  "start-workspace": {
    text: "There it is. Press Start workspace on it: each piece of work gets its own branch, agent and machine.",
  },
  guided: {
    text: "Every workspace has a guided checklist for when you feel lost: open it from the compass in the sidebar. Replay this tour any time from Settings.",
    next: "Done",
  },
};

/** The Accounts step when the organization provides no shared account. */
export const ONBOARDING_ACCOUNTS_OWN_COPY: OnboardingStepCopy = {
  text: "Agents need a subscription or API key to run. Add yours here before we continue; your org admin can also add a shared one.",
  next: "Next",
};

/** The Accounts step for a viewer in an organization without a shared account. */
export const ONBOARDING_ACCOUNTS_VIEWER_COPY: OnboardingStepCopy = {
  text: "Agents bill to an account. {org} has no shared one yet; an admin adds it here, and viewers can't add their own.",
  next: "Next",
};

/** The closing card a viewer sees after the Accounts step. */
export const ONBOARDING_VIEWER_CLOSING_COPY: OnboardingStepCopy = {
  text: "That's it for now: viewers read along, and once {org} makes you a member you'll get your own scratchpad and workspaces. Replay this tour any time from Settings.",
  next: "Done",
};

/** The issue step after the wait or a chat error. */
export const ONBOARDING_ISSUE_FALLBACK_COPY: OnboardingStepCopy = {
  text: 'The agent hasn\'t created the issue yet. Or create it yourself: press Create, name it "{issue}" and assign it to you.',
};

/** The issue step after "Create issue manually". */
export const ONBOARDING_ISSUE_MANUAL_COPY: OnboardingStepCopy = {
  text: 'Press Create in the Issues tile, name the issue "{issue}" and assign it to you.',
};

/** Step 6 while the Start workspace dialog is open. */
export const ONBOARDING_START_DIALOG_COPY: OnboardingStepCopy = {
  text: "Start now: name the workspace and press Fork. Or close this dialog to work on the issue later; the tour waits on it.",
};

/** Step 6 while the person is on Home without the issue (resume). */
export const ONBOARDING_WORKSPACE_PROMPT_COPY: OnboardingStepCopy = {
  text: "Open any workspace to continue the tour there.",
};

export function fillOnboardingCopy(
  text: string,
  values: { name: string; org: string; issue: string },
): string {
  return text
    .replaceAll("{name}", values.name)
    .replaceAll("{org}", values.org)
    .replaceAll("{issue}", values.issue);
}
