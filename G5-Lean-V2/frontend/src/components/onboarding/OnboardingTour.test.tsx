// The tour controller against fake pages that expose only the anchors the
// real pages carry (ui g5.ui.onboarding-tour): the welcome card on Home, the
// forced Settings press, the Theme and Accounts steps (shared versus own
// account), the viewer closing card, Skip, resume and replay; the issue
// step's manual path, the Start workspace dialog card, the card placed
// beside the chat and the Guided step's compass.

import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { HttpResponse, http } from "msw";
import React from "react";
import {
  Link,
  Navigate,
  Route,
  Routes,
  useLocation,
  useNavigate,
} from "react-router-dom";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createMockUser } from "../../test/fixtures";
import { server } from "../../test/msw/server";
import { renderApp } from "../../test/render";
import { seedUserSettings } from "../../test/userSettings";
import { Modal } from "../ui/UI";
import { OnboardingTour } from "./OnboardingTour";
import { useOnboardingTour } from "./onboardingTourContext";

const ORG = { orgId: "org-g5labs", orgName: "G5 Labs", orgSlug: "g5labs" };

function FakeHome() {
  const navigate = useNavigate();
  const { composerSeed } = useOnboardingTour();
  const [forking, setForking] = React.useState(false);
  return (
    <div>
      {forking && (
        <Modal onClose={() => setForking(false)} labelledBy="fork-title">
          <h2 id="fork-title">Fork base</h2>
          <input data-testid="fork-name" aria-label="Name" />
          <button
            type="button"
            data-testid="clone-cancel-button"
            onClick={() => setForking(false)}
          >
            Cancel
          </button>
          {/* Fork opens the new workspace, as the real dialog does. */}
          <button
            type="button"
            data-testid="clone-submit-button"
            onClick={() => navigate("/g5labs/feature-x")}
          >
            Fork
          </button>
        </Modal>
      )}
      <button
        type="button"
        data-testid="settings-button"
        onClick={() => navigate("/g5labs/base?view=settings")}
      >
        Settings
      </button>
      <button type="button" data-testid="other-button">
        Other
      </button>
      <section data-testid="home-workspaces">Workspaces</section>
      <section data-testid="home-personal-chat">
        <textarea
          data-testid="chat-input"
          readOnly
          value={composerSeed?.text ?? ""}
        />
        <div data-testid="chat-agent-options-row">options</div>
        <button type="button" data-testid="chat-panel-no-session-send">
          Send
        </button>
      </section>
      <section data-testid="home-issues-tile">
        <button type="button" data-testid="home-issues-create">
          Create
        </button>
        <div data-testid="home-issue-900012">
          <a href="https://github.test/x" data-testid="home-issue-title">
            [Test issue] Ada onboarding
          </a>
          <button
            type="button"
            data-testid="home-issue-start-workspace"
            onClick={() => setForking(true)}
          >
            Start
          </button>
        </div>
        <div data-testid="home-issue-900013">
          <a href="https://github.test/y" data-testid="home-issue-title">
            Unrelated issue
          </a>
          <button type="button" data-testid="home-issue-start-workspace">
            Start
          </button>
        </div>
      </section>
      <Link to="/g5labs/feature-x" data-testid="go-workspace">
        workspace
      </Link>
    </div>
  );
}

function ReplayButton() {
  const { replay } = useOnboardingTour();
  return (
    <button type="button" data-testid="replay" onClick={replay}>
      replay
    </button>
  );
}

function FakeSettings() {
  return (
    <div>
      <button type="button" data-testid="settings-button">
        Settings
      </button>
      <ReplayButton />
      <div data-testid="settings-row-theme">theme</div>
      <div data-testid="settings-row-mode">mode</div>
      <section data-testid="settings-section-accounts">
        <button type="button" data-testid="provider-account-add">
          + Add account
        </button>
      </section>
    </div>
  );
}

function FakeWorkspace() {
  return (
    <div>
      <button type="button" data-testid="sidebar-rail-guided">
        Guided
      </button>
      <button type="button" data-testid="guided-step-be-intent-action-new-doc">
        New doc
      </button>
    </div>
  );
}

function FakeApp() {
  return (
    <>
      <OnboardingTour />
      <Routes>
        <Route path="/:org/home" element={<FakeHome />} />
        {/* The real route redirects into Base's settings view. */}
        <Route
          path="/:org/settings"
          element={<Navigate to="/g5labs/base?view=settings" replace />}
        />
        <Route path="/:org/:workspace" element={<WorkspaceOrSettings />} />
      </Routes>
    </>
  );
}

function WorkspaceOrSettings() {
  const { search } = useLocation();
  return new URLSearchParams(search).get("view") === "settings" ? (
    <FakeSettings />
  ) : (
    <FakeWorkspace />
  );
}

type PatchBody = {
  onboardingStep?: number;
  onboardingSkipped?: boolean;
};

function tourUser(
  overrides: Partial<{
    onboardingStep: number | null;
    onboardingSkipped: boolean;
    role: "admin" | "member" | "viewer";
  }> = {},
) {
  const { role = "member", ...rest } = overrides;
  return {
    ...createMockUser({
      id: "user-ada",
      email: "ada@g5labs.dev",
      displayName: "Ada Lovelace",
      orgs: [{ ...ORG, role }],
    }),
    onboardingStep: null,
    onboardingSkipped: false,
    ...rest,
  };
}

function serveAccounts(scopes: string[]) {
  server.use(
    http.get("/api/v1/orgs/:orgId/provider-accounts", () =>
      HttpResponse.json({
        data: scopes.map((scope, index) => ({
          id: `acct-${index}`,
          name: `${scope} account`,
          scope,
          kind: "anthropic_api_key",
          secretHint: "sk-ant",
          createdAt: "2026-01-01T00:00:00Z",
        })),
      }),
    ),
  );
}

function capturePatches(): PatchBody[] {
  const patches: PatchBody[] = [];
  server.use(
    http.patch("/api/v1/users/me", async ({ request }) => {
      const body = (await request.json()) as PatchBody;
      patches.push(body);
      const raw = localStorage.getItem("g5_test_current_user");
      const current = raw ? (JSON.parse(raw) as Record<string, unknown>) : {};
      const stored = Math.max(
        (current.onboardingStep as number | null) ?? -1,
        body.onboardingStep ?? -1,
      );
      const updated = {
        ...current,
        onboardingStep: stored < 0 ? null : stored,
        onboardingSkipped:
          body.onboardingSkipped ??
          (current.onboardingSkipped as boolean) ??
          false,
      };
      localStorage.setItem("g5_test_current_user", JSON.stringify(updated));
      return HttpResponse.json({ data: updated });
    }),
  );
  return patches;
}

beforeEach(() => {
  sessionStorage.clear();
  window.matchMedia = vi.fn().mockImplementation((query: string) => ({
    matches: true,
    media: query,
    onchange: null,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    addListener: vi.fn(),
    removeListener: vi.fn(),
    dispatchEvent: vi.fn(),
  }));
});

afterEach(() => {
  sessionStorage.clear();
});

describe("OnboardingTour", () => {
  it("shows the welcome card on Home to a person never shown it, records step 0 and blocks everything but Settings", async () => {
    // Seven steps with the Guided section on (g5.ui.guided-toggle).
    seedUserSettings({ showGuidedJourney: true });
    const patches = capturePatches();
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    renderApp(<FakeApp />, { route: "/g5labs/home", user: tourUser() });

    const card = await screen.findByTestId("onboarding-step-welcome");
    expect(card).toHaveTextContent("Welcome to G5, Ada.");
    expect(
      within(card).getByTestId("onboarding-card-counter"),
    ).toHaveTextContent("Step 1 of 7");
    await waitFor(() => expect(patches).toEqual([{ onboardingStep: 0 }]));

    // A control outside the spotlight is swallowed at the document.
    const other = screen.getByTestId("other-button");
    const otherClicks = vi.fn();
    other.addEventListener("click", otherClicks);
    await user.click(other);
    expect(otherClicks).not.toHaveBeenCalled();

    // The card offers the same action as the spotlit control, so Skip is
    // never the only button; the pulse marks the control itself.
    expect(within(card).getByTestId("onboarding-next")).toHaveTextContent(
      "Open Settings",
    );
    expect(
      screen.getByTestId("onboarding-spotlight-pulse"),
    ).toBeInTheDocument();

    // Settings is the spotlit control: pressing it moves to the Theme step.
    await user.click(screen.getByTestId("settings-button"));
    expect(
      await screen.findByTestId("onboarding-step-theme"),
    ).toHaveTextContent("Pick a theme and light or dark.");
    await waitFor(() =>
      expect(patches).toEqual([{ onboardingStep: 0 }, { onboardingStep: 1 }]),
    );
  });

  it("does not start anywhere but Home, and never for a person who finished or skipped", async () => {
    const patches = capturePatches();
    renderApp(<FakeApp />, {
      route: "/g5labs/feature-x",
      user: tourUser(),
    });
    await screen.findByTestId("sidebar-rail-guided");
    expect(screen.queryByTestId("onboarding-tour")).toBeNull();
    expect(screen.queryByTestId("onboarding-continue")).toBeNull();
    expect(patches).toEqual([]);

    renderApp(<FakeApp />, {
      route: "/g5labs/home",
      user: tourUser({ onboardingStep: 7 }),
    });
    await screen.findAllByTestId("home-workspaces");
    expect(screen.queryByTestId("onboarding-tour")).toBeNull();

    renderApp(<FakeApp />, {
      route: "/g5labs/home",
      user: tourUser({ onboardingStep: 2, onboardingSkipped: true }),
    });
    await screen.findAllByTestId("home-workspaces");
    expect(screen.queryByTestId("onboarding-tour")).toBeNull();
    expect(screen.queryByTestId("onboarding-continue")).toBeNull();
  });

  it("hosts no step in the operator organization, whose Home is the platform console", async () => {
    const patches = capturePatches();
    const operatorUser = (onboardingStep: number | null) => ({
      ...tourUser({ onboardingStep }),
      orgs: [
        {
          orgId: "org-operator",
          orgName: "Operator",
          orgSlug: "operator",
          role: "admin" as const,
          orgKind: "operator" as const,
        },
        { ...ORG, role: "member" as const, orgKind: "tenant" as const },
      ],
      activeOrgId: "org-operator",
    });
    renderApp(<FakeApp />, {
      route: "/operator/home",
      user: operatorUser(null),
    });
    await screen.findAllByTestId("home-workspaces");
    expect(screen.queryByTestId("onboarding-tour")).toBeNull();
    expect(patches).toEqual([]);

    // A resumed step offers the way back to the tenant organization's Home;
    // its card shows only there.
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    renderApp(<FakeApp />, {
      route: "/operator/home",
      user: operatorUser(3),
    });
    const pill = await screen.findByTestId("onboarding-continue");
    await user.click(pill);
    expect(
      await screen.findByTestId("onboarding-step-scratchpad"),
    ).toBeVisible();
  });

  it("says the organization already provides an account, then navigates Home for the scratchpad step and seeds the composer", async () => {
    capturePatches();
    serveAccounts(["org"]);
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    renderApp(<FakeApp />, {
      route: "/g5labs/base?view=settings",
      user: tourUser({ onboardingStep: 1 }),
    });

    const theme = await screen.findByTestId("onboarding-step-theme");
    await user.click(within(theme).getByTestId("onboarding-next"));
    const accounts = await screen.findByTestId("onboarding-step-accounts");
    await waitFor(() =>
      expect(accounts).toHaveTextContent("G5 Labs already provides one"),
    );
    await waitFor(() =>
      expect(within(accounts).getByTestId("onboarding-next")).toBeEnabled(),
    );
    await user.click(within(accounts).getByTestId("onboarding-next"));

    const scratchpad = await screen.findByTestId("onboarding-step-scratchpad");
    expect(scratchpad).toHaveTextContent("personal scratchpad chat");
    await user.click(within(scratchpad).getByTestId("onboarding-next"));

    const issue = await screen.findByTestId("onboarding-step-create-issue");
    expect(issue).toHaveTextContent("Pick a model above the composer");
    expect(screen.getByTestId("chat-input")).toHaveValue(
      'Create an issue titled "[Test issue] Ada onboarding", assigned to me, with a short body saying it is my onboarding test.',
    );
    expect(within(issue).queryByTestId("onboarding-next")).toBeNull();
  });

  it("asks for the person's own account and holds Next until one exists", async () => {
    capturePatches();
    let scopes: string[] = [];
    server.use(
      http.get("/api/v1/orgs/:orgId/provider-accounts", () =>
        HttpResponse.json({
          data: scopes.map((scope) => ({
            id: `acct-${scope}`,
            name: scope,
            scope,
            kind: "anthropic_api_key",
            secretHint: "sk-ant",
            createdAt: "2026-01-01T00:00:00Z",
          })),
        }),
      ),
    );
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    const { queryClient } = renderApp(<FakeApp />, {
      route: "/g5labs/base?view=settings",
      user: tourUser({ onboardingStep: 2 }),
    });

    const accounts = await screen.findByTestId("onboarding-step-accounts");
    await waitFor(() =>
      expect(accounts).toHaveTextContent("Add yours here before we continue"),
    );
    expect(within(accounts).getByTestId("onboarding-next")).toBeDisabled();

    scopes = ["user"];
    await act(async () => {
      await queryClient.invalidateQueries({
        queryKey: ["scoped-config", "provider-accounts"],
      });
    });
    await waitFor(() =>
      expect(within(accounts).getByTestId("onboarding-next")).toBeEnabled(),
    );
    await user.click(within(accounts).getByTestId("onboarding-next"));
    expect(
      await screen.findByTestId("onboarding-step-scratchpad"),
    ).toBeVisible();
  });

  it("closes a viewer's tour after Accounts and records it finished", async () => {
    const patches = capturePatches();
    // No shared account and no way for a viewer to add one: Next stays open.
    serveAccounts([]);
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    renderApp(<FakeApp />, {
      route: "/g5labs/base?view=settings",
      user: tourUser({ onboardingStep: 2, role: "viewer" }),
    });
    const accounts = await screen.findByTestId("onboarding-step-accounts");
    await waitFor(() =>
      expect(accounts).toHaveTextContent("viewers can't add their own"),
    );
    expect(within(accounts).getByTestId("onboarding-next")).toBeEnabled();
    await user.click(within(accounts).getByTestId("onboarding-next"));
    const closing = await screen.findByTestId("onboarding-step-closing");
    expect(closing).toHaveTextContent("viewers read along");
    await user.click(within(closing).getByTestId("onboarding-next"));
    await waitFor(() =>
      expect(patches).toContainEqual({
        onboardingStep: 7,
        onboardingSkipped: false,
      }),
    );
    expect(screen.queryByTestId("onboarding-tour")).toBeNull();
  });

  it("records a skip at the step shown and offers Continue tour when a resumed step belongs to another page", async () => {
    const patches = capturePatches();
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    renderApp(<FakeApp />, {
      route: "/g5labs/home",
      user: tourUser({ onboardingStep: 3 }),
    });
    const scratchpad = await screen.findByTestId("onboarding-step-scratchpad");
    expect(
      within(scratchpad).getByTestId("onboarding-card-counter"),
    ).toHaveTextContent("Step 4 of 6");
    await user.click(within(scratchpad).getByTestId("onboarding-skip"));
    await waitFor(() =>
      expect(patches).toContainEqual({
        onboardingStep: 3,
        onboardingSkipped: true,
      }),
    );
    expect(screen.queryByTestId("onboarding-tour")).toBeNull();

    // A Theme step paused on Home: the pill leads to Settings.
    renderApp(<FakeApp />, {
      route: "/g5labs/home",
      user: tourUser({ onboardingStep: 1 }),
    });
    const pill = await screen.findByTestId("onboarding-continue");
    expect(pill).toHaveTextContent("Continue tour (2 of 6)");
    await user.click(pill);
    expect(await screen.findByTestId("onboarding-step-theme")).toBeVisible();
  });

  it("opens Settings from the welcome card's own button", async () => {
    capturePatches();
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    renderApp(<FakeApp />, { route: "/g5labs/home", user: tourUser() });
    const card = await screen.findByTestId("onboarding-step-welcome");
    await user.click(within(card).getByTestId("onboarding-next"));
    expect(await screen.findByTestId("onboarding-step-theme")).toBeVisible();
  });

  it("spotlights the matching issue row's own Start action on resume, never the title link", async () => {
    capturePatches();
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    renderApp(<FakeApp />, {
      route: "/g5labs/home",
      user: tourUser({ onboardingStep: 5 }),
    });
    await screen.findByTestId("onboarding-step-start-workspace");
    const rows = screen.getAllByTestId("home-issue-start-workspace");
    const starts = rows.map(() => vi.fn());
    rows.forEach((row, index) => row.addEventListener("click", starts[index]));
    const titles = screen.getAllByTestId("home-issue-title");
    const titleClicks = vi.fn();
    titles[0].addEventListener("click", titleClicks);

    await user.click(titles[0]);
    expect(titleClicks).not.toHaveBeenCalled();
    await user.click(rows[1]);
    expect(starts[1]).not.toHaveBeenCalled();
    await user.click(rows[0]);
    expect(starts[0]).toHaveBeenCalledTimes(1);
  });

  it("ends on the Guided step when the sidebar's compass is pressed, not the checklist's own action", async () => {
    seedUserSettings({ showGuidedJourney: true });
    const patches = capturePatches();
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    renderApp(<FakeApp />, {
      route: "/g5labs/feature-x",
      user: tourUser({ onboardingStep: 6 }),
    });
    const guided = await screen.findByTestId("onboarding-step-guided");
    expect(guided).toHaveTextContent("compass in the sidebar");
    await user.click(
      screen.getByTestId("guided-step-be-intent-action-new-doc"),
    );
    expect(screen.getByTestId("onboarding-step-guided")).toBeInTheDocument();
    expect(patches).not.toContainEqual({
      onboardingStep: 7,
      onboardingSkipped: false,
    });
    await user.click(screen.getByTestId("sidebar-rail-guided"));
    await waitFor(() =>
      expect(patches).toContainEqual({
        onboardingStep: 7,
        onboardingSkipped: false,
      }),
    );
    expect(screen.queryByTestId("onboarding-tour")).toBeNull();
  });

  // ui g5.ui.guided-toggle: the guided experience is off by default, so the
  // tour has six steps and ends when the workspace route loads.
  it("has six steps while the guided experience is off and ends when a workspace loads", async () => {
    const patches = capturePatches();
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    renderApp(<FakeApp />, {
      route: "/g5labs/home",
      user: tourUser({ onboardingStep: 5 }),
    });
    const card = await screen.findByTestId("onboarding-step-start-workspace");
    expect(
      within(card).getByTestId("onboarding-card-counter"),
    ).toHaveTextContent("Step 6 of 6");
    // Start workspace, then Fork: the workspace route loads and the tour
    // ends there instead of moving to a Guided step.
    await user.click(screen.getAllByTestId("home-issue-start-workspace")[0]);
    await screen.findByTestId("onboarding-step-start-workspace-dialog");
    await user.click(screen.getByTestId("clone-submit-button"));
    await waitFor(() =>
      expect(patches).toContainEqual({
        onboardingStep: 7,
        onboardingSkipped: false,
      }),
    );
    expect(screen.queryByTestId("onboarding-tour")).toBeNull();
    expect(screen.queryByTestId("onboarding-step-guided")).toBeNull();
  });

  it("finishes a tour resumed at the Guided step while the guided experience is off", async () => {
    const patches = capturePatches();
    renderApp(<FakeApp />, {
      route: "/g5labs/feature-x",
      user: tourUser({ onboardingStep: 6 }),
    });
    await waitFor(() =>
      expect(patches).toContainEqual({
        onboardingStep: 7,
        onboardingSkipped: false,
      }),
    );
    expect(screen.queryByTestId("onboarding-step-guided")).toBeNull();
  });

  it("ends on the Guided step when the full sidebar's header is activated by keyboard", async () => {
    seedUserSettings({ showGuidedJourney: true });
    const patches = capturePatches();
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    renderApp(
      <>
        <OnboardingTour />
        <Routes>
          <Route
            path="/:org/:workspace"
            element={
              <div data-testid="sidebar-guided-tab">
                {/* The real header toggles on Enter and Space without a click. */}
                <div
                  data-surface="section-header"
                  role="button"
                  tabIndex={0}
                  data-testid="guided-header"
                  onKeyDown={(event) => {
                    if (event.key === "Enter") event.preventDefault();
                  }}
                >
                  Guided
                </div>
              </div>
            }
          />
        </Routes>
      </>,
      { route: "/g5labs/feature-x", user: tourUser({ onboardingStep: 6 }) },
    );
    const card = await screen.findByTestId("onboarding-step-guided");
    // Let the step's initial autofocus finish before moving to its target.
    await waitFor(() => expect(card).toHaveFocus());
    const header = screen.getByTestId("guided-header");
    header.focus();
    expect(header).toHaveFocus();
    await user.keyboard("{Enter}");
    await waitFor(() =>
      expect(patches).toContainEqual({
        onboardingStep: 7,
        onboardingSkipped: false,
      }),
    );
    expect(patches.filter((patch) => patch.onboardingStep === 7)).toHaveLength(
      1,
    );
  });

  it("offers manual creation on the issue step and then spotlights the tile's Create alone", async () => {
    capturePatches();
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    renderApp(<FakeApp />, {
      route: "/g5labs/home",
      user: tourUser({ onboardingStep: 4 }),
    });
    const card = await screen.findByTestId("onboarding-step-create-issue");
    const create = screen.getByTestId("home-issues-create");
    const createClicks = vi.fn();
    create.addEventListener("click", createClicks);
    await user.click(create);
    expect(createClicks).not.toHaveBeenCalled();

    await user.click(
      within(card).getByTestId("onboarding-create-issue-manually"),
    );
    const manual = await screen.findByTestId("onboarding-step-create-issue");
    expect(manual).toHaveTextContent(
      'Press Create in the Issues tile, name the issue "[Test issue] Ada onboarding"',
    );
    expect(
      within(manual).queryByTestId("onboarding-create-issue-manually"),
    ).toBeNull();
    await waitFor(async () => {
      await user.click(create);
      expect(createClicks).toHaveBeenCalled();
    });
    // The composer is no longer the control.
    const send = screen.getByTestId("chat-panel-no-session-send");
    const sendClicks = vi.fn();
    send.addEventListener("click", sendClicks);
    await user.click(send);
    expect(sendClicks).not.toHaveBeenCalled();
  });

  it("keeps a card over the Start workspace dialog that leaves the dialog usable and returns when it closes", async () => {
    capturePatches();
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    renderApp(<FakeApp />, {
      route: "/g5labs/home",
      user: tourUser({ onboardingStep: 5 }),
    });
    await screen.findByTestId("onboarding-step-start-workspace");
    await user.click(screen.getAllByTestId("home-issue-start-workspace")[0]);
    const dialogCard = await screen.findByTestId(
      "onboarding-step-start-workspace-dialog",
    );
    expect(dialogCard).toHaveTextContent("work on the issue later");
    expect(screen.getByTestId("onboarding-spotlight")).toHaveAttribute(
      "data-dim",
      "false",
    );
    // The dialog keeps focus and its fields: nothing is blocked.
    expect(dialogCard).not.toHaveFocus();
    await user.type(screen.getByTestId("fork-name"), "feature-x");
    expect(screen.getByTestId("fork-name")).toHaveValue("feature-x");
    await user.click(screen.getByTestId("clone-cancel-button"));
    expect(
      await screen.findByTestId("onboarding-step-start-workspace"),
    ).toBeVisible();
    expect(
      screen.queryByTestId("onboarding-step-start-workspace-dialog"),
    ).toBeNull();
  });

  it("hides the dialog card when the card fits nowhere outside the dialog, and shows it once it does", async () => {
    capturePatches();
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    const offsetWidth = vi
      .spyOn(HTMLElement.prototype, "offsetWidth", "get")
      .mockReturnValue(360);
    const offsetHeight = vi
      .spyOn(HTMLElement.prototype, "offsetHeight", "get")
      .mockReturnValue(200);
    try {
      renderApp(<FakeApp />, {
        route: "/g5labs/home",
        user: tourUser({ onboardingStep: 5 }),
      });
      await screen.findByTestId("onboarding-step-start-workspace");
      await user.click(screen.getAllByTestId("home-issue-start-workspace")[0]);
      const dialog = await screen.findByRole("dialog", { name: "Fork base" });
      const fork = screen.getByTestId("clone-submit-button");
      const rectOf =
        (left: number, top: number, width: number, height: number) => () =>
          ({
            left,
            top,
            width,
            height,
            right: left + width,
            bottom: top + height,
            x: left,
            y: top,
            toJSON: () => ({}),
          }) as DOMRect;
      // jsdom's viewport is 1024×768: a 576-wide centred dialog that spans
      // the height leaves no strip wide enough on either side.
      dialog.getBoundingClientRect = rectOf(224, 20, 576, 728);
      fork.getBoundingClientRect = rectOf(720, 700, 60, 32);
      await waitFor(() =>
        expect(
          screen.queryByTestId("onboarding-step-start-workspace-dialog"),
        ).toBeNull(),
      );
      // A narrower dialog leaves 240px on the right: the card appears there.
      dialog.getBoundingClientRect = rectOf(100, 20, 640, 728);
      fork.getBoundingClientRect = rectOf(660, 700, 60, 32);
      const card = await screen.findByTestId(
        "onboarding-step-start-workspace-dialog",
      );
      await waitFor(() => expect(parseFloat(card.style.left)).toBe(762));
      expect(parseFloat(card.style.width)).toBe(246);
      expect(screen.getByTestId("onboarding-card-arrow")).toHaveAttribute(
        "data-side",
        "left",
      );
    } finally {
      offsetWidth.mockRestore();
      offsetHeight.mockRestore();
    }
  });

  it("places the chat steps' card beside the composer, not over the transcript", async () => {
    capturePatches();
    renderApp(<FakeApp />, {
      route: "/g5labs/home",
      user: tourUser({ onboardingStep: 3 }),
    });
    const input = await screen.findByTestId("chat-input");
    // The composer sits at the bottom right, as in the Home side panel.
    input.getBoundingClientRect = () =>
      ({
        top: 700,
        left: 700,
        width: 300,
        height: 40,
        right: 1000,
        bottom: 740,
        x: 700,
        y: 700,
        toJSON: () => ({}),
      }) as DOMRect;
    const card = await screen.findByTestId("onboarding-step-scratchpad");
    await waitFor(() => {
      expect(screen.getByTestId("onboarding-card-arrow")).toHaveAttribute(
        "data-side",
        "right",
      );
    });
    expect(parseFloat(card.style.left)).toBeLessThan(700);
  });

  it("replays from the Settings page by starting on Home, not on the Theme step", async () => {
    capturePatches();
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    renderApp(<FakeApp />, {
      route: "/g5labs/base?view=settings",
      user: tourUser({ onboardingStep: 7 }),
    });
    await user.click(await screen.findByTestId("replay"));
    expect(await screen.findByTestId("onboarding-step-welcome")).toBeVisible();
    expect(screen.queryByTestId("onboarding-step-theme")).toBeNull();
    expect(screen.queryByTestId("onboarding-continue")).toBeNull();
    // The welcome card's own action still reaches the Theme step.
    await user.click(screen.getByTestId("onboarding-next"));
    expect(await screen.findByTestId("onboarding-step-theme")).toBeVisible();
  });

  it("replays from the welcome card without lowering the record", async () => {
    const patches = capturePatches();
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    renderApp(
      <>
        <OnboardingTour />
        <Routes>
          <Route
            path="/:org/home"
            element={
              <>
                <FakeHome />
                <ReplayButton />
              </>
            }
          />
        </Routes>
      </>,
      { route: "/g5labs/home", user: tourUser({ onboardingStep: 7 }) },
    );
    await screen.findByTestId("replay");
    expect(screen.queryByTestId("onboarding-tour")).toBeNull();
    await user.click(screen.getByTestId("replay"));
    expect(await screen.findByTestId("onboarding-step-welcome")).toBeVisible();
    expect(sessionStorage.getItem("g5:onboarding:replay-cursor:user-ada")).toBe(
      "0",
    );
    // Step 0 is below the record's 7: nothing is written.
    expect(
      patches.filter((patch) => patch.onboardingStep !== undefined),
    ).toEqual([]);
  });
});
