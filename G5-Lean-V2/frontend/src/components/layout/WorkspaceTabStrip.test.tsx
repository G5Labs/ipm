import {
  act,
  cleanup,
  fireEvent,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { HttpResponse, http } from "msw";
import { Route, Routes, useLocation, useNavigate } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  WorkspaceTabsProvider,
  useWorkspaceTabs,
} from "../../hooks/useWorkspaceTabs";
import {
  readWorkspaceTabs,
  workspaceTabsKey,
  type WorkspaceTab,
} from "../../lib/workspaceTabs";
import {
  createMockUser,
  mockInstanceInfo,
  mockWorkspaces,
} from "../../test/fixtures";
import { server } from "../../test/msw/server";
import { firePointer } from "../../test/pointer";
import { renderApp } from "../../test/render";
import { AppShell } from "./Layout";

const USER = createMockUser();

function installLayout(features: string[] = []) {
  server.use(
    http.get("/api/v1/instance/info", () =>
      HttpResponse.json({
        data: {
          ...mockInstanceInfo,
          features: [...mockInstanceInfo.features, ...features],
        },
      }),
    ),
  );
}

const ALPHA: WorkspaceTab = { orgSlug: "org-g5labs", name: "alpha" };
const BETA: WorkspaceTab = { orgSlug: "org-g5labs", name: "beta" };
const GAMMA: WorkspaceTab = { orgSlug: "org-g5labs", name: "gamma" };

const RouteEcho = ({ label }: { label: string }) => {
  const { pathname, search } = useLocation();
  const navigate = useNavigate();
  const { active, rememberSnapshot } = useWorkspaceTabs();
  return (
    <AppShell>
      <div data-testid="page">
        {label}:{pathname}
        {search}
      </div>
      {/* What a page sees on a workspace route. */}
      <div data-testid="visit">
        {active.kind === "workspace"
          ? `returned:${active.returned} snapshot:${active.snapshot !== null}`
          : "none"}
      </div>
      {/* Stands in for the page's persistence writing alpha's snapshot. */}
      <button
        type="button"
        data-testid="snapshot-alpha"
        onClick={() =>
          rememberSnapshot(ALPHA, {
            workspaceId: "alpha-id",
            tabs: [],
            activeTabId: "pull-requests",
            chatCenter: false,
          })
        }
      >
        snapshot alpha
      </button>
      {/* Stands in for a Home row's Open link. */}
      <button
        type="button"
        data-testid="open-alpha"
        onClick={() => navigate("/org-g5labs/alpha")}
      >
        open alpha
      </button>
      {/* Stands in for a link naming alpha under another spelling. */}
      <button
        type="button"
        data-testid="open-alpha-upper"
        onClick={() => navigate("/org-g5labs/ALPHA")}
      >
        open ALPHA
      </button>
      {/* Stands in for a link into another organization's workspace. */}
      <button
        type="button"
        data-testid="open-other-beta"
        onClick={() => navigate("/other/beta")}
      >
        open other beta
      </button>
      {/* Stands in for the page opening a doc and a `?view=` surface. */}
      <button
        type="button"
        data-testid="deepen-alpha"
        onClick={() =>
          navigate("/org-g5labs/alpha/doc/notes.md?view=details", {
            replace: true,
          })
        }
      >
        deepen alpha
      </button>
    </AppShell>
  );
};

function renderShell(route: string) {
  const rendered = renderApp(
    <WorkspaceTabsProvider>
      <Routes>
        <Route path="/:org/home" element={<RouteEcho label="home" />} />
        <Route path="/:org/fork" element={<RouteEcho label="fork" />} />
        <Route path="/projects" element={<RouteEcho label="projects" />} />
        <Route path="/:org/:workspace/*" element={<RouteEcho label="ws" />} />
      </Routes>
    </WorkspaceTabsProvider>,
    { route, user: USER },
  );
  /** The routed page rendered: a negative assertion after this is meaningful. */
  const ready = () =>
    waitFor(() => {
      expect(screen.getByTestId("page")).toBeInTheDocument();
    });
  return { ...rendered, ready };
}

function seedTabs(tabs: WorkspaceTab[]) {
  localStorage.setItem(workspaceTabsKey(USER.id), JSON.stringify(tabs));
}

/** Another browser tab of the same person writes the list. */
function storeFromAnotherBrowserTab(tabs: WorkspaceTab[]) {
  act(() => {
    seedTabs(tabs);
    window.dispatchEvent(
      new StorageEvent("storage", { key: workspaceTabsKey(USER.id) }),
    );
  });
}

function tabNames(): string[] {
  return within(screen.getByTestId("workspace-tab-strip"))
    .queryAllByTestId("workspace-tab")
    .map((tab) => tab.getAttribute("aria-label") ?? "");
}

/**
 * The strip once it shows the stored tabs, `names`. The strip's first render
 * can precede the provider's effects — the one reading the stored list and,
 * in the same flush, the one subscribing to other browser tabs' writes — so
 * it may show the routed tab alone. A storage event dispatched before that
 * flush reaches no listener, and the arrival effect then opens the routed
 * tab again: finding the strip alone is not enough before
 * `storeFromAnotherBrowserTab`.
 */
async function findStripShowing(names: string[]): Promise<HTMLElement> {
  const strip = await screen.findByTestId("workspace-tab-strip");
  await waitFor(() => expect(tabNames()).toEqual(names));
  return strip;
}

function page() {
  return screen.getByTestId("page");
}

// Open workspaces are tabs (workbench g5.workbench.tabs): the route is the
// active tab, the list is per-user browser state, Home is pinned.
describe("WorkspaceTabStrip", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("stays hidden on routes outside the shell and records nothing there", async () => {
    installLayout();
    const { ready } = renderShell("/projects");
    await ready();
    expect(page()).toHaveTextContent("projects:/projects");
    expect(screen.queryByTestId("workspace-tab-strip")).toBeNull();
    expect(readWorkspaceTabs(USER.id)).toEqual([]);
  });

  it("opens a tab for the routed workspace, pins Home and persists per user", async () => {
    installLayout();
    renderShell("/org-g5labs/alpha");
    const strip = await screen.findByTestId("workspace-tab-strip");
    expect(within(strip).getByTestId("workspace-tab-home")).toHaveAttribute(
      "aria-selected",
      "false",
    );
    const tab = await within(strip).findByRole("tab", { name: "alpha" });
    expect(tab).toHaveAttribute("aria-selected", "true");
    expect(readWorkspaceTabs(USER.id)).toEqual([
      { orgSlug: "org-g5labs", name: "alpha" },
    ]);
  });

  it("shows each tab's workspace phase icon from the organization's list", async () => {
    installLayout();
    // `g5labs` is a membership of the mock user (org-g5labs): the strip reads
    // that organization's workspace list, the one Home loads.
    server.use(
      http.get("/api/v1/orgs/org-g5labs/workspaces", () =>
        HttpResponse.json({
          data: [
            {
              ...mockWorkspaces[0],
              id: "alpha-id",
              name: "alpha",
              phase: "se",
            },
            // An older server omits the phase: the DTO default is `be`.
            { ...mockWorkspaces[0], id: "beta-id", name: "beta" },
          ],
        }),
      ),
    );
    seedTabs([
      { orgSlug: "g5labs", name: "alpha" },
      { orgSlug: "g5labs", name: "beta" },
      { orgSlug: "g5labs", name: "gamma" },
    ]);
    renderShell("/g5labs/alpha");
    const strip = await screen.findByTestId("workspace-tab-strip");
    const alpha = within(strip).getByRole("tab", { name: "alpha" });
    await waitFor(() =>
      expect(within(alpha).getByTestId("workspace-tab-phase")).toHaveAttribute(
        "data-phase",
        "se",
      ),
    );
    expect(
      within(alpha).getByTestId("workspace-tab-phase").querySelector("svg"),
    ).toHaveClass("lucide-blocks");
    expect(within(alpha).getByTestId("workspace-tab-phase")).toHaveAttribute(
      "title",
      "System Engineering",
    );
    const beta = within(strip).getByRole("tab", { name: "beta" });
    expect(within(beta).getByTestId("workspace-tab-phase")).toHaveAttribute(
      "data-phase",
      "be",
    );
    expect(
      within(beta).getByTestId("workspace-tab-phase").querySelector("svg"),
    ).toHaveClass("lucide-briefcase");
    // A tab the list lacks (deleted or renamed elsewhere) shows no phase.
    const gamma = within(strip).getByRole("tab", { name: "gamma" });
    expect(within(gamma).queryByTestId("workspace-tab-phase")).toBeNull();
    expect(strip.querySelector("svg.lucide-layers")).toBeNull();
  });

  it("matches a tab to its listed workspace case-insensitively (issue #736)", async () => {
    installLayout();
    // The settings redirects and the default landing route `/:org/base`,
    // while the workspace is listed as `Base`; the server resolves the name
    // case-insensitively, so the tab is Base's: its phase glyph and name.
    server.use(
      http.get("/api/v1/orgs/org-g5labs/workspaces", () =>
        HttpResponse.json({
          data: [
            {
              ...mockWorkspaces[0],
              id: "base-id",
              name: "Base",
              isBase: true,
              phase: "be",
            },
          ],
        }),
      ),
    );
    seedTabs([{ orgSlug: "g5labs", name: "Base" }]);
    renderShell("/g5labs/base?view=settings");
    const strip = await screen.findByTestId("workspace-tab-strip");
    // Arriving by the lower-case route opens no second tab for Base.
    const base = await within(strip).findByRole("tab", { name: "Base" });
    expect(tabNames()).toEqual(["Base"]);
    expect(base).toHaveAttribute("aria-selected", "true");
    expect(readWorkspaceTabs(USER.id)).toEqual([
      { orgSlug: "g5labs", name: "Base" },
    ]);
    await waitFor(() =>
      expect(within(base).getByTestId("workspace-tab-phase")).toHaveAttribute(
        "data-phase",
        "be",
      ),
    );
    expect(
      within(base).getByTestId("workspace-tab-phase").querySelector("svg"),
    ).toHaveClass("lucide-briefcase");
    expect(
      within(strip).getByRole("button", { name: "Close Base" }),
    ).toBeInTheDocument();
  });

  it("shows a lower-case routed tab under its listed name once listed", async () => {
    installLayout();
    server.use(
      http.get("/api/v1/orgs/org-g5labs/workspaces", () =>
        HttpResponse.json({
          data: [
            { ...mockWorkspaces[0], id: "base-id", name: "Base", isBase: true },
          ],
        }),
      ),
    );
    // The only arrival is the lower-case route: the stored tab keeps the
    // routed name, the strip shows the listed one with Base's glyph.
    renderShell("/g5labs/base");
    const strip = await screen.findByTestId("workspace-tab-strip");
    const base = await within(strip).findByRole("tab", { name: "Base" });
    expect(tabNames()).toEqual(["Base"]);
    expect(base).toHaveTextContent("Base");
    expect(within(base).getByTestId("workspace-tab-phase")).toHaveAttribute(
      "data-phase",
      "be",
    );
    expect(readWorkspaceTabs(USER.id)).toEqual([
      { orgSlug: "g5labs", name: "base" },
    ]);
  });

  it("switches between open tabs and Home by navigation only", async () => {
    installLayout();
    seedTabs([
      { orgSlug: "org-g5labs", name: "alpha" },
      { orgSlug: "org-g5labs", name: "beta" },
    ]);
    const user = userEvent.setup();
    renderShell("/org-g5labs/beta");
    const strip = await screen.findByTestId("workspace-tab-strip");
    // The stored list is read once the signed-in person is known.
    await waitFor(() => expect(tabNames()).toEqual(["alpha", "beta"]));

    await user.click(within(strip).getByRole("tab", { name: "alpha" }));
    expect(page()).toHaveTextContent("ws:/org-g5labs/alpha");
    expect(within(strip).getByRole("tab", { name: "alpha" })).toHaveAttribute(
      "aria-selected",
      "true",
    );

    await user.click(within(strip).getByTestId("workspace-tab-home"));
    expect(page()).toHaveTextContent("home:/org-g5labs/home");
    expect(within(strip).getByTestId("workspace-tab-home")).toHaveAttribute(
      "aria-selected",
      "true",
    );
    // Both workspace tabs stay open.
    expect(tabNames()).toEqual(["alpha", "beta"]);
  });

  it("returns to the URL a tab was last on; a closed tab forgets it", async () => {
    installLayout();
    seedTabs([
      { orgSlug: "org-g5labs", name: "alpha" },
      { orgSlug: "org-g5labs", name: "beta" },
    ]);
    const user = userEvent.setup();
    renderShell("/org-g5labs/alpha");
    const strip = await screen.findByTestId("workspace-tab-strip");
    const tab = (name: string) => within(strip).getByRole("tab", { name });
    const deep = "ws:/org-g5labs/alpha/doc/notes.md?view=details";

    await user.click(screen.getByTestId("deepen-alpha"));
    expect(page()).toHaveTextContent(deep);

    // Away and back by the strip: the page alpha was on, not its root.
    await user.click(tab("beta"));
    expect(page()).toHaveTextContent("ws:/org-g5labs/beta");
    expect(page()).not.toHaveTextContent("notes.md");
    await user.click(tab("alpha"));
    expect(page()).toHaveTextContent(deep);

    // Through Home as well.
    await user.click(within(strip).getByTestId("workspace-tab-home"));
    expect(page()).toHaveTextContent("home:/org-g5labs/home");
    await user.click(tab("alpha"));
    expect(page()).toHaveTextContent(deep);

    // Closing the active beta shows alpha where it was.
    await user.click(tab("beta"));
    await user.click(
      within(tab("beta").parentElement as HTMLElement).getByTestId(
        "workspace-tab-close",
      ),
    );
    expect(page()).toHaveTextContent(deep);
    expect(tabNames()).toEqual(["alpha"]);

    // A closed tab forgets its page: reopened, alpha is a fresh open.
    await user.click(
      within(tab("alpha").parentElement as HTMLElement).getByTestId(
        "workspace-tab-close",
      ),
    );
    expect(page()).toHaveTextContent("home:/org-g5labs/home");
    await user.click(screen.getByTestId("open-alpha"));
    expect(page()).toHaveTextContent("ws:/org-g5labs/alpha");
    expect(page()).not.toHaveTextContent("notes.md");
  });

  it("a link to the open workspace under another spelling is a fresh open of its tab", async () => {
    installLayout();
    seedTabs([ALPHA, { orgSlug: "org-g5labs", name: "beta" }]);
    const user = userEvent.setup();
    renderShell("/org-g5labs/alpha");
    const strip = await screen.findByTestId("workspace-tab-strip");
    const tab = (name: string) => within(strip).getByRole("tab", { name });
    const visit = () => screen.getByTestId("visit");
    await user.click(screen.getByTestId("snapshot-alpha"));
    await user.click(tab("beta"));
    await user.click(tab("alpha"));
    expect(visit()).toHaveTextContent("returned:true snapshot:true");

    // The server resolves `ALPHA` to alpha: the link lands in alpha's tab
    // (no second tab), as an opening — what the link names is shown — and
    // the tab's memory is kept.
    await user.click(screen.getByTestId("open-alpha-upper"));
    expect(page()).toHaveTextContent("ws:/org-g5labs/ALPHA");
    expect(tabNames()).toEqual(["alpha", "beta"]);
    expect(tab("alpha")).toHaveAttribute("aria-selected", "true");
    expect(visit()).toHaveTextContent("returned:false snapshot:true");
    expect(readWorkspaceTabs(USER.id)).toEqual([
      ALPHA,
      { orgSlug: "org-g5labs", name: "beta" },
    ]);

    // Returning through the strip restores where the tab was last shown.
    await user.click(tab("beta"));
    await user.click(tab("alpha"));
    expect(page()).toHaveTextContent("ws:/org-g5labs/ALPHA");
    expect(visit()).toHaveTextContent("returned:true snapshot:true");
  });

  it("a return is the strip's own navigation; a link to the workspace is an opening", async () => {
    installLayout();
    seedTabs([ALPHA, { orgSlug: "org-g5labs", name: "beta" }]);
    const user = userEvent.setup();
    renderShell("/org-g5labs/alpha");
    const strip = await screen.findByTestId("workspace-tab-strip");
    const tab = (name: string) => within(strip).getByRole("tab", { name });
    const visit = () => screen.getByTestId("visit");
    expect(visit()).toHaveTextContent("returned:false snapshot:false");

    await user.click(screen.getByTestId("snapshot-alpha"));
    expect(visit()).toHaveTextContent("snapshot:true");
    await user.click(tab("beta"));
    expect(visit()).toHaveTextContent("returned:false snapshot:false");
    // By the strip: a return, with the snapshot.
    await user.click(tab("alpha"));
    expect(visit()).toHaveTextContent("returned:true snapshot:true");

    // By a link (a Home row, a recent task): an opening, the memory kept.
    await user.click(within(strip).getByTestId("workspace-tab-home"));
    await user.click(screen.getByTestId("open-alpha"));
    expect(page()).toHaveTextContent("ws:/org-g5labs/alpha");
    expect(visit()).toHaveTextContent("returned:false snapshot:true");
    await user.click(tab("beta"));
    await user.click(tab("alpha"));
    expect(visit()).toHaveTextContent("returned:true snapshot:true");

    // A closed tab keeps no memory, even from a write that lands after the
    // close (the departing page's flush).
    await user.click(
      within(tab("alpha").parentElement as HTMLElement).getByTestId(
        "workspace-tab-close",
      ),
    );
    expect(page()).toHaveTextContent("home:/org-g5labs/home");
    await user.click(screen.getByTestId("snapshot-alpha"));
    await user.click(screen.getByTestId("open-alpha"));
    expect(visit()).toHaveTextContent("returned:false snapshot:false");
    await user.click(tab("beta"));
    await user.click(tab("alpha"));
    expect(visit()).toHaveTextContent("returned:true snapshot:false");
  });

  it("closing the active tab shows its left neighbour, the last one goes Home", async () => {
    installLayout();
    seedTabs([
      { orgSlug: "org-g5labs", name: "alpha" },
      { orgSlug: "org-g5labs", name: "beta" },
    ]);
    const user = userEvent.setup();
    renderShell("/org-g5labs/beta");
    const strip = await screen.findByTestId("workspace-tab-strip");

    await user.click(within(strip).getByRole("button", { name: "Close beta" }));
    expect(page()).toHaveTextContent("ws:/org-g5labs/alpha");
    expect(tabNames()).toEqual(["alpha"]);

    await user.click(
      within(strip).getByRole("button", { name: "Close alpha" }),
    );
    expect(page()).toHaveTextContent("home:/org-g5labs/home");
    expect(tabNames()).toEqual([]);
    expect(readWorkspaceTabs(USER.id)).toEqual([]);
  });

  it("reopening a workspace after closing its tab from Home adds the tab again", async () => {
    installLayout();
    seedTabs([{ orgSlug: "org-g5labs", name: "alpha" }]);
    const user = userEvent.setup();
    renderShell("/org-g5labs/alpha");
    const strip = await screen.findByTestId("workspace-tab-strip");
    await user.click(
      within(strip).getByRole("button", { name: "Close alpha" }),
    );
    expect(page()).toHaveTextContent("home:/org-g5labs/home");
    expect(tabNames()).toEqual([]);

    await user.click(screen.getByTestId("open-alpha"));
    expect(page()).toHaveTextContent("ws:/org-g5labs/alpha");
    await waitFor(() => expect(tabNames()).toEqual(["alpha"]));
    expect(readWorkspaceTabs(USER.id)).toEqual([
      { orgSlug: "org-g5labs", name: "alpha" },
    ]);
  });

  it("closing the first tab goes Home even when tabs remain to its right", async () => {
    installLayout();
    seedTabs([
      { orgSlug: "org-g5labs", name: "alpha" },
      { orgSlug: "org-g5labs", name: "beta" },
    ]);
    const user = userEvent.setup();
    renderShell("/org-g5labs/alpha");
    const strip = await screen.findByTestId("workspace-tab-strip");

    await user.click(
      within(strip).getByRole("button", { name: "Close alpha" }),
    );
    expect(page()).toHaveTextContent("home:/org-g5labs/home");
    expect(tabNames()).toEqual(["beta"]);
  });

  it("shows the route organization's tabs only; another organization's tabs stay open for its own pages", async () => {
    installLayout();
    seedTabs([
      { orgSlug: "org-g5labs", name: "alpha" },
      { orgSlug: "other", name: "beta" },
      { orgSlug: "other", name: "gamma" },
    ]);
    const user = userEvent.setup();
    renderShell("/org-g5labs/alpha");
    await screen.findByTestId("workspace-tab-strip");
    expect(tabNames()).toEqual(["alpha"]);

    // Into the other organization: its tabs, its Home, its picker.
    await user.click(screen.getByTestId("open-other-beta"));
    expect(page()).toHaveTextContent("ws:/other/beta");
    expect(tabNames()).toEqual(["beta", "gamma"]);
    expect(readWorkspaceTabs(USER.id)).toEqual([
      { orgSlug: "org-g5labs", name: "alpha" },
      { orgSlug: "other", name: "beta" },
      { orgSlug: "other", name: "gamma" },
    ]);
    await user.click(screen.getByTestId("workspace-tab-add"));
    expect(page()).toHaveTextContent("fork:/other/fork");
    expect(tabNames()).toEqual(["beta", "gamma"]);
    await user.click(screen.getByTestId("workspace-tab-home"));
    expect(page()).toHaveTextContent("home:/other/home");
    expect(tabNames()).toEqual(["beta", "gamma"]);

    // Closing the active tab shows its neighbour in the strip, never a tab
    // of another organization.
    await user.click(screen.getByTestId("open-other-beta"));
    await user.click(
      within(screen.getByTestId("workspace-tab-strip")).getByRole("button", {
        name: "Close beta",
      }),
    );
    expect(page()).toHaveTextContent("home:/other/home");
    expect(tabNames()).toEqual(["gamma"]);

    // Back in the first organization its tab is still open.
    await user.click(screen.getByTestId("open-alpha"));
    expect(tabNames()).toEqual(["alpha"]);
  });

  it("closing an inactive tab keeps the page; middle-click closes too", async () => {
    installLayout();
    seedTabs([
      { orgSlug: "org-g5labs", name: "alpha" },
      { orgSlug: "org-g5labs", name: "beta" },
    ]);
    const user = userEvent.setup();
    renderShell("/org-g5labs/beta");
    const strip = await screen.findByTestId("workspace-tab-strip");

    await user.click(
      await within(strip).findByRole("button", { name: "Close alpha" }),
    );
    expect(page()).toHaveTextContent("ws:/org-g5labs/beta");
    expect(tabNames()).toEqual(["beta"]);

    await user.pointer({
      keys: "[MouseMiddle]",
      target: within(strip).getByRole("tab", { name: "beta" }),
    });
    expect(page()).toHaveTextContent("home:/org-g5labs/home");
    expect(tabNames()).toEqual([]);
  });

  it("keeps showing the routed workspace without reopening a tab closed in another browser tab", async () => {
    installLayout();
    seedTabs([ALPHA, BETA]);
    renderShell("/org-g5labs/beta");
    const strip = await findStripShowing(["alpha", "beta"]);
    storeFromAnotherBrowserTab([ALPHA]);
    // beta stays on screen as a transient tab; the close sticks in storage,
    // so the other browser tab does not get it back.
    await waitFor(() => expect(tabNames()).toEqual(["alpha", "beta"]));
    expect(page()).toHaveTextContent("ws:/org-g5labs/beta");
    expect(within(strip).getByRole("tab", { name: "beta" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(readWorkspaceTabs(USER.id)).toEqual([ALPHA]);
  });

  it("navigating within a workspace closed in another browser tab does not reopen its tab", async () => {
    installLayout();
    seedTabs([ALPHA, BETA]);
    const user = userEvent.setup();
    renderShell("/org-g5labs/alpha");
    await findStripShowing(["alpha", "beta"]);
    storeFromAnotherBrowserTab([BETA]);
    await user.click(screen.getByTestId("deepen-alpha"));
    expect(page()).toHaveTextContent("ws:/org-g5labs/alpha/doc/notes.md");
    expect(tabNames()).toEqual(["beta", "alpha"]);
    expect(readWorkspaceTabs(USER.id)).toEqual([BETA]);
  });

  it("leaving a workspace closed in another browser tab drops it; opening it again reopens its tab", async () => {
    installLayout();
    seedTabs([BETA, ALPHA]);
    const user = userEvent.setup();
    renderShell("/org-g5labs/alpha");
    const strip = await findStripShowing(["beta", "alpha"]);
    storeFromAnotherBrowserTab([BETA]);
    await user.click(within(strip).getByRole("tab", { name: "Home" }));
    expect(page()).toHaveTextContent("home:/org-g5labs/home");
    expect(tabNames()).toEqual(["beta"]);
    expect(readWorkspaceTabs(USER.id)).toEqual([BETA]);
    await user.click(screen.getByTestId("open-alpha"));
    await waitFor(() => expect(tabNames()).toEqual(["beta", "alpha"]));
    expect(readWorkspaceTabs(USER.id)).toEqual([BETA, ALPHA]);
  });

  it("closing the transient tab shows its left neighbour and leaves the list unchanged", async () => {
    installLayout();
    seedTabs([BETA, ALPHA]);
    const user = userEvent.setup();
    renderShell("/org-g5labs/alpha");
    const strip = await findStripShowing(["beta", "alpha"]);
    storeFromAnotherBrowserTab([BETA]);
    await waitFor(() => expect(tabNames()).toEqual(["beta", "alpha"]));
    await user.click(
      within(strip).getByRole("button", { name: "Close alpha" }),
    );
    expect(page()).toHaveTextContent("ws:/org-g5labs/beta");
    expect(tabNames()).toEqual(["beta"]);
    expect(readWorkspaceTabs(USER.id)).toEqual([BETA]);
  });

  it("follows another browser tab's additions to the list", async () => {
    installLayout();
    seedTabs([{ orgSlug: "org-g5labs", name: "alpha" }]);
    renderShell("/org-g5labs/alpha");
    await screen.findByTestId("workspace-tab-strip");
    storeFromAnotherBrowserTab([
      { orgSlug: "org-g5labs", name: "alpha" },
      { orgSlug: "org-g5labs", name: "beta" },
    ]);
    await waitFor(() => expect(tabNames()).toEqual(["alpha", "beta"]));
  });

  it("a tab closed by another browser tab loses its memory", async () => {
    installLayout();
    seedTabs([ALPHA, { orgSlug: "org-g5labs", name: "beta" }]);
    const user = userEvent.setup();
    renderShell("/org-g5labs/alpha");
    const strip = await screen.findByTestId("workspace-tab-strip");
    const tab = (name: string) => within(strip).getByRole("tab", { name });
    await user.click(screen.getByTestId("deepen-alpha"));
    await user.click(screen.getByTestId("snapshot-alpha"));
    await user.click(tab("beta"));
    // Another browser tab closes alpha, then opens it again.
    storeFromAnotherBrowserTab([{ orgSlug: "org-g5labs", name: "beta" }]);
    await waitFor(() => expect(tabNames()).toEqual(["beta"]));
    storeFromAnotherBrowserTab([
      { orgSlug: "org-g5labs", name: "beta" },
      ALPHA,
    ]);
    await waitFor(() => expect(tabNames()).toEqual(["beta", "alpha"]));
    await user.click(tab("alpha"));
    expect(page()).toHaveTextContent("ws:/org-g5labs/alpha");
    expect(page()).not.toHaveTextContent("notes.md");
    expect(screen.getByTestId("visit")).toHaveTextContent(
      "returned:false snapshot:false",
    );
  });

  it("+ opens the fork picker as a transient New tab that the fork's landing retires", async () => {
    installLayout();
    seedTabs([{ orgSlug: "g5labs", name: "alpha" }]);
    const user = userEvent.setup();
    renderShell("/g5labs/alpha");
    const strip = await screen.findByTestId("workspace-tab-strip");

    await user.click(within(strip).getByTestId("workspace-tab-add"));
    expect(page()).toHaveTextContent("fork:/g5labs/fork");
    expect(within(strip).getByTestId("workspace-tab-new")).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(tabNames()).toEqual(["alpha"]);
    // Home is the picker's organization's.
    await user.click(within(strip).getByTestId("workspace-tab-home"));
    expect(page()).toHaveTextContent("home:/g5labs/home");
    expect(screen.queryByTestId("workspace-tab-new")).toBeNull();
  });

  it("renders the page's global controls at the strip's right end, outside the tablist (g5.workbench.shell-chrome)", async () => {
    installLayout();
    renderApp(
      <WorkspaceTabsProvider>
        <Routes>
          <Route
            path="/:org/home"
            element={
              <AppShell
                shellControls={<button data-testid="shell-probe">probe</button>}
              >
                <div data-testid="page">home</div>
              </AppShell>
            }
          />
        </Routes>
      </WorkspaceTabsProvider>,
      { route: "/org-g5labs/home", user: USER },
    );
    const bar = await screen.findByTestId("workspace-tab-bar");
    const probe = within(bar).getByTestId("shell-probe");
    expect(screen.getByTestId("workspace-tab-strip")).not.toContainElement(
      probe,
    );
    // The controls come after the "+" in reading order.
    expect(
      screen.getByTestId("workspace-tab-add").compareDocumentPosition(probe) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it("moves focus along the tabs with the arrow keys without selecting", async () => {
    installLayout();
    seedTabs([
      { orgSlug: "org-g5labs", name: "alpha" },
      { orgSlug: "org-g5labs", name: "beta" },
    ]);
    const user = userEvent.setup();
    renderShell("/org-g5labs/alpha");
    const strip = await screen.findByTestId("workspace-tab-strip");
    // The persisted tabs arrive with the user id, after the route's own tab:
    // the arrow keys walk the tabs in the DOM at the key press, so wait for
    // beta before stepping, or ArrowRight wraps from alpha to Home.
    await within(strip).findByRole("tab", { name: "beta" });
    within(strip).getByRole("tab", { name: "alpha" }).focus();
    await user.keyboard("{ArrowRight}");
    expect(within(strip).getByRole("tab", { name: "beta" })).toHaveFocus();
    await user.keyboard("{ArrowRight}");
    expect(within(strip).getByTestId("workspace-tab-home")).toHaveFocus();
    await user.keyboard("{ArrowLeft}");
    expect(within(strip).getByRole("tab", { name: "beta" })).toHaveFocus();
    expect(page()).toHaveTextContent("ws:/org-g5labs/alpha");
  });

  // A personal workspace tab shows its owner's avatar in the glyph's place
  // (g5.workbench.tabs): the caller's own from the signed-in person, a
  // member's resolved through the organization roster.
  describe("personal tab avatars", () => {
    const ME: WorkspaceTab = { orgSlug: "g5labs", name: "me", personal: "me" };
    const JORDAN: WorkspaceTab = {
      orgSlug: "g5labs",
      name: "users/jordan",
      personal: "user",
    };
    const G5_ALPHA: WorkspaceTab = { orgSlug: "g5labs", name: "alpha" };

    function renderWithUser(user = USER) {
      return renderApp(
        <WorkspaceTabsProvider>
          <Routes>
            <Route path="/:org/home" element={<RouteEcho label="home" />} />
            <Route
              path="/:org/:workspace/*"
              element={<RouteEcho label="ws" />}
            />
          </Routes>
        </WorkspaceTabsProvider>,
        { route: "/g5labs/alpha", user },
      );
    }

    it("shows the caller's own avatar on their personal tab, keeping the tab label as its accessible name", async () => {
      installLayout();
      seedTabs([ME, G5_ALPHA]);
      renderWithUser();
      const strip = await screen.findByTestId("workspace-tab-strip");
      const me = await within(strip).findByRole("tab", {
        name: "Personal workspace",
      });
      const avatar = within(me).getByTestId("identity-avatar");
      // The OAuth picture, decorative, with the display name as its tooltip.
      expect(avatar.tagName).toBe("IMG");
      expect(avatar).toHaveAttribute("src", USER.avatarUrl);
      expect(avatar).toHaveAttribute("title", USER.displayName);
      expect(avatar).toHaveAttribute("aria-hidden", "true");
      // Not natively draggable, so a tab-reorder drag that starts on the
      // picture is not cancelled by an image drag.
      expect(avatar).toHaveAttribute("draggable", "false");
      // The tab's accessible name stays its label; the avatar names nothing.
      expect(me).toHaveAttribute("aria-label", "Personal workspace");
      // An ordinary workspace tab shows no avatar.
      const alpha = within(strip).getByRole("tab", { name: "alpha" });
      expect(within(alpha).queryByTestId("identity-avatar")).toBeNull();
    });

    it("falls back to the deterministic initial-and-tint when the caller has no picture", async () => {
      installLayout();
      seedTabs([ME, G5_ALPHA]);
      renderWithUser(createMockUser({ avatarUrl: null }));
      const strip = await screen.findByTestId("workspace-tab-strip");
      const me = await within(strip).findByRole("tab", {
        name: "Personal workspace",
      });
      const avatar = within(me).getByTestId("identity-avatar");
      // No image: the initial from the identity (alex@g5labs.dev) and a tint.
      expect(avatar.tagName).toBe("SPAN");
      expect(avatar).toHaveTextContent("A");
      expect(avatar).toHaveAttribute("title", "Alex Chen");
    });

    it("shows another member's avatar on their personal tab, resolved through the roster", async () => {
      installLayout();
      // The owner/administrator resolves a member's personal workspace by
      // handle to its user id; the roster carries that member's avatar.
      server.use(
        http.get("/api/v1/orgs/:orgId/users/:handle/workspace", ({ params }) =>
          HttpResponse.json({
            data: {
              workspaceId: "jordan-personal",
              orgId: String(params.orgId),
              userId: "user-jordan",
              name: "users/jordan",
              handle: "jordan",
            },
          }),
        ),
      );
      seedTabs([JORDAN, G5_ALPHA]);
      renderWithUser();
      const strip = await screen.findByTestId("workspace-tab-strip");
      const jordan = await within(strip).findByRole("tab", {
        name: "users/jordan",
      });
      // Once resolved, Jordan's picture and display name, not the handle.
      await waitFor(() => {
        const avatar = within(jordan).getByTestId("identity-avatar");
        expect(avatar.tagName).toBe("IMG");
        expect(avatar).toHaveAttribute("title", "Jordan Rivera");
      });
      // The tab's accessible name stays the route name.
      expect(jordan).toHaveAttribute("aria-label", "users/jordan");
    });
  });

  // Reordering (g5.workbench.tabs.reorder). jsdom lays nothing out: every
  // workspace tab is 100px wide, side by side from the scroller's left edge,
  // and the scroller shows 1000px.
  describe("reordering", () => {
    const TAB_WIDTH = 100;
    const SCROLLER_WIDTH = 1000;
    let scrollLeft = 0;

    function box(left: number, width: number): DOMRect {
      return {
        left,
        right: left + width,
        width,
        top: 0,
        bottom: 32,
        height: 32,
        x: left,
        y: 0,
        toJSON: () => ({}),
      };
    }

    beforeEach(() => {
      scrollLeft = 0;
      const original = HTMLElement.prototype.getBoundingClientRect;
      vi.spyOn(
        HTMLElement.prototype,
        "getBoundingClientRect",
      ).mockImplementation(function (this: HTMLElement) {
        if (this.dataset.testid === "workspace-tab-scroller") {
          return box(0, SCROLLER_WIDTH);
        }
        if (
          this.dataset.testid === "workspace-tab-group" &&
          this.parentElement
        ) {
          const index = Array.from(
            this.parentElement.querySelectorAll(
              '[data-testid="workspace-tab-group"]',
            ),
          ).indexOf(this);
          return box(index * TAB_WIDTH - scrollLeft, TAB_WIDTH);
        }
        return original.call(this);
      });
    });

    afterEach(() => {
      vi.restoreAllMocks();
    });

    function strip() {
      return screen.getByTestId("workspace-tab-strip");
    }

    function tab(name: string) {
      return within(strip()).getByRole("tab", { name });
    }

    /** Left-button drag of a tab from one x to another, then release. */
    function drag(
      name: string,
      fromX: number,
      toX: number,
      end: "pointerup" | "pointercancel" = "pointerup",
    ) {
      const button = tab(name);
      firePointer(button, "pointerdown", fromX, 10);
      firePointer(button, "pointermove", (fromX + toX) / 2, 10);
      firePointer(button, "pointermove", toX, 10);
      firePointer(button, end, toX, 10);
      // The click a browser dispatches after a completed press.
      if (end === "pointerup") fireEvent.click(button);
    }

    it("drags a tab to the last place and to the first place after Home, persisting the order", async () => {
      installLayout();
      seedTabs([ALPHA, BETA, GAMMA]);
      const { unmount } = renderShell("/org-g5labs/beta");
      await waitFor(() =>
        expect(tabNames()).toEqual(["alpha", "beta", "gamma"]),
      );

      // alpha's right edge passes gamma's midpoint: it goes last.
      drag("alpha", 50, 260);
      expect(tabNames()).toEqual(["beta", "gamma", "alpha"]);
      expect(readWorkspaceTabs(USER.id)).toEqual([BETA, GAMMA, ALPHA]);
      // The click that ended the drag selected nothing.
      expect(page()).toHaveTextContent("ws:/org-g5labs/beta");
      expect(tab("beta")).toHaveAttribute("aria-selected", "true");

      // gamma (now second) to the first workspace place: never before Home.
      drag("gamma", 150, -400);
      expect(tabNames()).toEqual(["gamma", "beta", "alpha"]);
      expect(readWorkspaceTabs(USER.id)).toEqual([GAMMA, BETA, ALPHA]);
      expect(
        within(strip())
          .getAllByRole("tab")
          .map((el) => el.getAttribute("aria-label")),
      ).toEqual(["Home", "gamma", "beta", "alpha"]);

      // The order survives a reload of the page.
      unmount();
      cleanup();
      renderShell("/org-g5labs/home");
      await waitFor(() =>
        expect(tabNames()).toEqual(["gamma", "beta", "alpha"]),
      );
    });

    it("lands one place along when the dragged tab passes a single neighbour's midpoint", async () => {
      installLayout();
      seedTabs([ALPHA, BETA, GAMMA]);
      renderShell("/org-g5labs/alpha");
      await waitFor(() =>
        expect(tabNames()).toEqual(["alpha", "beta", "gamma"]),
      );
      // Short of beta's midpoint: back where it was, nothing written.
      drag("alpha", 50, 95);
      expect(tabNames()).toEqual(["alpha", "beta", "gamma"]);
      drag("alpha", 50, 110);
      expect(tabNames()).toEqual(["beta", "alpha", "gamma"]);
      expect(readWorkspaceTabs(USER.id)).toEqual([BETA, ALPHA, GAMMA]);
    });

    it("keeps the active tab's selection, URL and return state through a reorder", async () => {
      installLayout();
      seedTabs([ALPHA, BETA, GAMMA]);
      const user = userEvent.setup();
      renderShell("/org-g5labs/alpha");
      await waitFor(() =>
        expect(tabNames()).toEqual(["alpha", "beta", "gamma"]),
      );
      const deep = "ws:/org-g5labs/alpha/doc/notes.md?view=details";
      await user.click(screen.getByTestId("deepen-alpha"));
      await user.click(screen.getByTestId("snapshot-alpha"));

      drag("alpha", 50, 260);
      expect(tabNames()).toEqual(["beta", "gamma", "alpha"]);
      expect(page()).toHaveTextContent(deep);
      expect(tab("alpha")).toHaveAttribute("aria-selected", "true");

      // Away and back through the strip: alpha returns where it was.
      await user.click(tab("beta"));
      expect(page()).toHaveTextContent("ws:/org-g5labs/beta");
      await user.click(tab("alpha"));
      expect(page()).toHaveTextContent(deep);
      expect(screen.getByTestId("visit")).toHaveTextContent(
        "returned:true snapshot:true",
      );
    });

    it("closing the active tab after a reorder shows its new left neighbour", async () => {
      installLayout();
      seedTabs([ALPHA, BETA, GAMMA]);
      const user = userEvent.setup();
      renderShell("/org-g5labs/alpha");
      await waitFor(() =>
        expect(tabNames()).toEqual(["alpha", "beta", "gamma"]),
      );
      drag("gamma", 250, -100);
      expect(tabNames()).toEqual(["gamma", "alpha", "beta"]);
      await user.click(
        within(strip()).getByRole("button", { name: "Close alpha" }),
      );
      expect(page()).toHaveTextContent("ws:/org-g5labs/gamma");
      expect(tabNames()).toEqual(["gamma", "beta"]);
      expect(readWorkspaceTabs(USER.id)).toEqual([GAMMA, BETA]);
    });

    it("keeps Home fixed outside the sideways scroller and out of the drag", async () => {
      installLayout();
      seedTabs([ALPHA, BETA]);
      renderShell("/org-g5labs/beta");
      await waitFor(() => expect(tabNames()).toEqual(["alpha", "beta"]));
      const scroller = screen.getByTestId("workspace-tab-scroller");
      const home = screen.getByTestId("workspace-tab-home");
      // Overflowing tabs scroll inside the scroller; Home is beside it, so it
      // stays at the left whatever the scroll position.
      expect(scroller).toHaveClass("overflow-x-auto");
      expect(scroller).not.toContainElement(home);
      expect(strip()).toContainElement(home);
      expect(
        home.compareDocumentPosition(scroller) &
          Node.DOCUMENT_POSITION_FOLLOWING,
      ).toBeTruthy();
      // Pressing and dragging Home moves nothing and still opens Home.
      firePointer(home, "pointerdown", -50, 10);
      firePointer(home, "pointermove", 250, 10);
      firePointer(home, "pointerup", 250, 10);
      fireEvent.click(home);
      expect(page()).toHaveTextContent("home:/org-g5labs/home");
      expect(tabNames()).toEqual(["alpha", "beta"]);
      // The "+" stays outside the sortable tabs, after them.
      expect(
        within(scroller)
          .getAllByTestId("workspace-tab-group")
          .at(-1)!
          .compareDocumentPosition(screen.getByTestId("workspace-tab-add")) &
          Node.DOCUMENT_POSITION_FOLLOWING,
      ).toBeTruthy();
    });

    it("a short press is a click; Escape or a cancelled pointer drops the drag", async () => {
      installLayout();
      seedTabs([ALPHA, BETA, GAMMA]);
      renderShell("/org-g5labs/beta");
      await waitFor(() =>
        expect(tabNames()).toEqual(["alpha", "beta", "gamma"]),
      );

      // Below the threshold it is a click: alpha is selected.
      const alpha = tab("alpha");
      firePointer(alpha, "pointerdown", 50, 10);
      firePointer(alpha, "pointermove", 53, 10);
      firePointer(alpha, "pointerup", 53, 10);
      fireEvent.click(alpha);
      expect(page()).toHaveTextContent("ws:/org-g5labs/alpha");

      // Escape mid-drag: the tabs return, the release neither moves nor selects.
      const gamma = tab("gamma");
      firePointer(gamma, "pointerdown", 250, 10);
      firePointer(gamma, "pointermove", 20, 10);
      expect(
        within(strip())
          .getAllByTestId("workspace-tab-group")
          .find((group) => group.dataset.dragging),
      ).toBeDefined();
      fireEvent.keyDown(window, { key: "Escape" });
      expect(
        within(strip())
          .getAllByTestId("workspace-tab-group")
          .every((group) => !group.dataset.dragging && !group.style.transform),
      ).toBe(true);
      firePointer(gamma, "pointermove", 0, 10);
      firePointer(gamma, "pointerup", 0, 10);
      fireEvent.click(gamma);
      expect(tabNames()).toEqual(["alpha", "beta", "gamma"]);
      expect(page()).toHaveTextContent("ws:/org-g5labs/alpha");

      // A pointer the browser cancels (no release) moves nothing either.
      drag("gamma", 250, -100, "pointercancel");
      expect(tabNames()).toEqual(["alpha", "beta", "gamma"]);
      expect(readWorkspaceTabs(USER.id)).toEqual([ALPHA, BETA, GAMMA]);

      // After a cancelled drag the next click still selects.
      fireEvent.click(tab("gamma"));
      expect(page()).toHaveTextContent("ws:/org-g5labs/gamma");
    });

    it("middle-click still closes and the close button still closes after a drag", async () => {
      installLayout();
      seedTabs([ALPHA, BETA, GAMMA]);
      const user = userEvent.setup();
      renderShell("/org-g5labs/beta");
      await waitFor(() =>
        expect(tabNames()).toEqual(["alpha", "beta", "gamma"]),
      );
      drag("alpha", 50, 260);
      await user.pointer({ keys: "[MouseMiddle]", target: tab("gamma") });
      expect(tabNames()).toEqual(["beta", "alpha"]);
      await user.click(
        within(strip()).getByRole("button", { name: "Close alpha" }),
      );
      expect(tabNames()).toEqual(["beta"]);
      expect(page()).toHaveTextContent("ws:/org-g5labs/beta");
      expect(readWorkspaceTabs(USER.id)).toEqual([BETA]);
    });

    it("reorders within the route organization, leaving other organizations' tabs in place", async () => {
      installLayout();
      const OTHER_X: WorkspaceTab = { orgSlug: "other", name: "x" };
      const OTHER_Y: WorkspaceTab = { orgSlug: "other", name: "y" };
      seedTabs([ALPHA, OTHER_X, BETA, OTHER_Y, GAMMA]);
      renderShell("/org-g5labs/alpha");
      await waitFor(() =>
        expect(tabNames()).toEqual(["alpha", "beta", "gamma"]),
      );
      drag("gamma", 250, -100);
      expect(tabNames()).toEqual(["gamma", "alpha", "beta"]);
      expect(readWorkspaceTabs(USER.id)).toEqual([
        GAMMA,
        ALPHA,
        OTHER_X,
        BETA,
        OTHER_Y,
      ]);
      // Last in the strip is right behind the organization's last tab.
      drag("gamma", 50, 260);
      expect(readWorkspaceTabs(USER.id)).toEqual([
        ALPHA,
        OTHER_X,
        BETA,
        GAMMA,
        OTHER_Y,
      ]);
    });

    it("moves a personal workspace tab by its route identity", async () => {
      installLayout();
      const ME: WorkspaceTab = {
        orgSlug: "org-g5labs",
        name: "me",
        personal: "me",
      };
      seedTabs([ME, ALPHA]);
      renderShell("/org-g5labs/alpha");
      await waitFor(() =>
        expect(tabNames()).toEqual(["Personal workspace", "alpha"]),
      );
      drag("Personal workspace", 50, 160);
      expect(tabNames()).toEqual(["alpha", "Personal workspace"]);
      expect(readWorkspaceTabs(USER.id)).toEqual([ALPHA, ME]);
      expect(page()).toHaveTextContent("ws:/org-g5labs/alpha");
      fireEvent.click(tab("Personal workspace"));
      expect(page()).toHaveTextContent("ws:/org-g5labs/me");
    });

    it("a reorder never resurrects a tab closed in another browser tab", async () => {
      installLayout();
      seedTabs([ALPHA, BETA, GAMMA]);
      renderShell("/org-g5labs/beta");
      await waitFor(() =>
        expect(tabNames()).toEqual(["alpha", "beta", "gamma"]),
      );
      // beta is closed elsewhere: it stays shown here as a transient tab.
      storeFromAnotherBrowserTab([ALPHA, GAMMA]);
      await waitFor(() =>
        expect(tabNames()).toEqual(["alpha", "gamma", "beta"]),
      );

      // alpha dropped behind the transient tab: last among the stored tabs,
      // and beta stays closed.
      drag("alpha", 50, 260);
      expect(readWorkspaceTabs(USER.id)).toEqual([GAMMA, ALPHA]);
      expect(tabNames()).toEqual(["gamma", "alpha", "beta"]);

      // Dragging the transient tab writes nothing.
      drag("beta", 250, -100);
      expect(readWorkspaceTabs(USER.id)).toEqual([GAMMA, ALPHA]);
      expect(tabNames()).toEqual(["gamma", "alpha", "beta"]);
      expect(page()).toHaveTextContent("ws:/org-g5labs/beta");
    });

    it("a reorder applies to the stored list even before another browser tab's storage event arrives", async () => {
      installLayout();
      seedTabs([ALPHA, BETA, GAMMA]);
      renderShell("/org-g5labs/alpha");
      await waitFor(() =>
        expect(tabNames()).toEqual(["alpha", "beta", "gamma"]),
      );
      // Another browser tab closed beta; its storage event has not arrived.
      seedTabs([ALPHA, GAMMA]);
      drag("alpha", 50, 260);
      expect(readWorkspaceTabs(USER.id)).toEqual([GAMMA, ALPHA]);
      expect(tabNames()).toEqual(["gamma", "alpha"]);
      // A tab closed there before the drop moves nothing and is not written.
      seedTabs([ALPHA]);
      drag("gamma", 50, 260);
      expect(readWorkspaceTabs(USER.id)).toEqual([ALPHA]);
    });

    it("a list change from another browser tab mid-drag cancels the drag", async () => {
      installLayout();
      seedTabs([ALPHA, BETA, GAMMA]);
      renderShell("/org-g5labs/beta");
      await waitFor(() =>
        expect(tabNames()).toEqual(["alpha", "beta", "gamma"]),
      );
      const alpha = tab("alpha");
      firePointer(alpha, "pointerdown", 50, 10);
      firePointer(alpha, "pointermove", 260, 10);
      storeFromAnotherBrowserTab([ALPHA, BETA]);
      await waitFor(() => expect(tabNames()).toEqual(["alpha", "beta"]));
      expect(
        within(strip())
          .getAllByTestId("workspace-tab-group")
          .every((group) => !group.dataset.dragging && !group.style.transform),
      ).toBe(true);
      firePointer(alpha, "pointermove", 270, 10);
      firePointer(alpha, "pointerup", 270, 10);
      fireEvent.click(alpha);
      // The other browser tab's list stands; the release moved and selected
      // nothing.
      expect(readWorkspaceTabs(USER.id)).toEqual([ALPHA, BETA]);
      expect(tabNames()).toEqual(["alpha", "beta"]);
      expect(page()).toHaveTextContent("ws:/org-g5labs/beta");
    });

    it("follows another browser tab's reorder", async () => {
      installLayout();
      seedTabs([ALPHA, BETA, GAMMA]);
      renderShell("/org-g5labs/beta");
      await waitFor(() =>
        expect(tabNames()).toEqual(["alpha", "beta", "gamma"]),
      );
      storeFromAnotherBrowserTab([GAMMA, ALPHA, BETA]);
      await waitFor(() =>
        expect(tabNames()).toEqual(["gamma", "alpha", "beta"]),
      );
      expect(tab("beta")).toHaveAttribute("aria-selected", "true");
    });

    it("scrolls the strip while a drag holds the pointer at its edge", async () => {
      installLayout();
      seedTabs(
        Array.from({ length: 15 }, (_, i) => ({
          orgSlug: "org-g5labs",
          name: `ws-${i}`,
        })),
      );
      renderShell("/org-g5labs/home");
      await waitFor(() => expect(tabNames()).toHaveLength(15));
      // 1500px of tabs in a 1000px scroller.
      const scroller = screen.getByTestId("workspace-tab-scroller");
      Object.defineProperty(scroller, "scrollLeft", {
        configurable: true,
        get: () => scrollLeft,
        set: (value: number) => {
          scrollLeft = Math.max(0, Math.min(500, value));
        },
      });
      const first = tab("ws-0");
      firePointer(first, "pointerdown", 50, 10);
      firePointer(first, "pointermove", 990, 10);
      await waitFor(() => expect(scrollLeft).toBe(500));
      // Scrolled to the end, the tab can land last.
      firePointer(first, "pointermove", 995, 10);
      firePointer(first, "pointerup", 995, 10);
      fireEvent.click(first);
      expect(tabNames().at(-1)).toBe("ws-0");
      expect(tabNames()).toHaveLength(15);
      expect(page()).toHaveTextContent("home:/org-g5labs/home");
    });

    it("moves the focused tab with Ctrl/Cmd+Shift+arrows, keeping its focus", async () => {
      installLayout();
      seedTabs([ALPHA, BETA, GAMMA]);
      const user = userEvent.setup();
      renderShell("/org-g5labs/beta");
      await waitFor(() =>
        expect(tabNames()).toEqual(["alpha", "beta", "gamma"]),
      );
      tab("alpha").focus();
      await user.keyboard("{Control>}{Shift>}{ArrowRight}{/Shift}{/Control}");
      expect(tabNames()).toEqual(["beta", "alpha", "gamma"]);
      expect(tab("alpha")).toHaveFocus();
      await user.keyboard("{Meta>}{Shift>}{ArrowRight}{/Shift}{/Meta}");
      expect(tabNames()).toEqual(["beta", "gamma", "alpha"]);
      expect(tab("alpha")).toHaveFocus();
      // At the end it stays; it never passes Home on the left either.
      await user.keyboard("{Control>}{Shift>}{ArrowRight}{/Shift}{/Control}");
      expect(tabNames()).toEqual(["beta", "gamma", "alpha"]);
      tab("beta").focus();
      await user.keyboard("{Control>}{Shift>}{ArrowLeft}{/Shift}{/Control}");
      expect(tabNames()).toEqual(["beta", "gamma", "alpha"]);
      expect(readWorkspaceTabs(USER.id)).toEqual([BETA, GAMMA, ALPHA]);
      // Plain arrows still only move focus; nothing was selected.
      await user.keyboard("{ArrowRight}");
      expect(tab("gamma")).toHaveFocus();
      expect(tabNames()).toEqual(["beta", "gamma", "alpha"]);
      expect(page()).toHaveTextContent("ws:/org-g5labs/beta");
    });
  });
});

// A workspace deleted elsewhere sends no event: the provider reconciles the
// tabs not shown with the organization's workspace list
// (g5.workbench.tabs.deleted). Only a workspace once seen listed counts.
describe("WorkspaceTabStrip — workspaces deleted elsewhere", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  const listed = (entries: Array<{ id: string; name: string }>) =>
    entries.map((entry) => ({ ...mockWorkspaces[0], ...entry }));

  function serveList(initial: Array<{ id: string; name: string }>) {
    let current = listed(initial);
    server.use(
      http.get("/api/v1/orgs/org-g5labs/workspaces", () =>
        HttpResponse.json({ data: current }),
      ),
    );
    return (next: Array<{ id: string; name: string }>) => {
      current = listed(next);
    };
  }

  it("closes a tab whose workspace a later list lacks; keeps one never listed and the routed one", async () => {
    installLayout();
    const setList = serveList([
      { id: "alpha-id", name: "alpha" },
      { id: "beta-id", name: "beta" },
    ]);
    seedTabs([
      { orgSlug: "g5labs", name: "alpha" },
      { orgSlug: "g5labs", name: "beta" },
      // Never listed (a review copy, say): the list cannot say it is gone.
      { orgSlug: "g5labs", name: "gamma" },
    ]);
    const { queryClient } = renderShell("/g5labs/alpha");
    const strip = await screen.findByTestId("workspace-tab-strip");
    await waitFor(() =>
      expect(
        within(within(strip).getByRole("tab", { name: "beta" })).getByTestId(
          "workspace-tab-phase",
        ),
      ).toBeInTheDocument(),
    );

    // beta and the routed alpha are deleted elsewhere.
    setList([]);
    await act(async () => {
      await queryClient.invalidateQueries({ queryKey: ["workspaces"] });
    });

    await waitFor(() => expect(tabNames()).toEqual(["alpha", "gamma"]));
    expect(readWorkspaceTabs(USER.id)).toEqual([
      { orgSlug: "g5labs", name: "alpha" },
      { orgSlug: "g5labs", name: "gamma" },
    ]);
    expect(page()).toHaveTextContent("ws:/g5labs/alpha");
  });

  it("a tab follows its workspace renamed elsewhere, keeping its place", async () => {
    installLayout();
    const setList = serveList([
      { id: "alpha-id", name: "alpha" },
      { id: "beta-id", name: "beta" },
      { id: "gamma-id", name: "gamma" },
    ]);
    seedTabs([
      { orgSlug: "g5labs", name: "alpha" },
      { orgSlug: "g5labs", name: "beta" },
      { orgSlug: "g5labs", name: "gamma" },
    ]);
    const { queryClient } = renderShell("/g5labs/alpha");
    const strip = await screen.findByTestId("workspace-tab-strip");
    await waitFor(() =>
      expect(
        within(within(strip).getByRole("tab", { name: "gamma" })).getByTestId(
          "workspace-tab-phase",
        ),
      ).toBeInTheDocument(),
    );

    setList([
      { id: "alpha-id", name: "alpha" },
      { id: "beta-id", name: "beta-renamed" },
      { id: "gamma-id", name: "gamma" },
    ]);
    await act(async () => {
      await queryClient.invalidateQueries({ queryKey: ["workspaces"] });
    });

    await waitFor(() =>
      expect(tabNames()).toEqual(["alpha", "beta-renamed", "gamma"]),
    );
    expect(readWorkspaceTabs(USER.id)).toEqual([
      { orgSlug: "g5labs", name: "alpha" },
      { orgSlug: "g5labs", name: "beta-renamed" },
      { orgSlug: "g5labs", name: "gamma" },
    ]);
  });
});
