// =============================================================================
// SettingsPageShell — one scroll, index on the left (ui g5.ui.settings-page)
// =============================================================================
// The index lists the sections and the separate pages, marks the section in
// view as the scroll moves, scrolls to a section on click and records it in
// `?section=`, and a `?section=` deep link lands on its section.
// =============================================================================

import {
  act,
  fireEvent,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { useLocation } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { renderApp } from "../../test/render";
import {
  SettingsPageShell,
  SettingsRow,
  SettingsSection,
} from "./SettingsPageShell";

// jsdom lays nothing out: give each section a top and the sheet a height.
const OFFSETS: Record<string, number> = {
  one: 0,
  late: 300,
  two: 500,
  three: 1000,
};
const SHEET_CLIENT_HEIGHT = 600;
const SHEET_SCROLL_HEIGHT = 1600;

function LocationProbe() {
  const location = useLocation();
  return <output data-testid="location">{location.search}</output>;
}

function Page({
  view = "settings",
  late = false,
}: {
  view?: string;
  late?: boolean;
}) {
  return (
    <>
      <SettingsPageShell
        testId="shell"
        view={view}
        crumbs={["g5", "base"]}
        title="Settings"
        scopeLine="Your settings."
        indexTitle="Your settings"
        links={{
          title: "Pages",
          items: [
            {
              id: "members",
              label: "Members",
              href: "/g5/settings/members",
              testId: "link-members",
            },
          ],
        }}
        indexFooter={<span>Elsewhere</span>}
      >
        <SettingsSection id="one" title="One" description="First.">
          <SettingsRow title="A row" description="Its label.">
            <input aria-label="A control" />
          </SettingsRow>
        </SettingsSection>
        {late && (
          <SettingsSection id="late" title="Late">
            <p>Arrived later</p>
          </SettingsSection>
        )}
        <SettingsSection id="two" title="Two">
          <p>Second body</p>
        </SettingsSection>
        <SettingsSection id="three" title="Three">
          <p>Third body</p>
        </SettingsSection>
      </SettingsPageShell>
      <LocationProbe />
    </>
  );
}

/** A page whose "Late" section mounts on a click. */
function LatePage() {
  const [late, setLate] = useState(false);
  return (
    <>
      <button
        type="button"
        data-testid="add-late"
        onClick={() => setLate(true)}
      >
        add late
      </button>
      <Page late={late} />
    </>
  );
}

function sheet(): HTMLElement {
  return screen.getByTestId("settings-sheet-scroll");
}

let offsetTopSpy: ReturnType<typeof vi.spyOn>;
let clientHeightSpy: ReturnType<typeof vi.spyOn>;
let scrollHeightSpy: ReturnType<typeof vi.spyOn>;

beforeEach(() => {
  offsetTopSpy = vi
    .spyOn(HTMLElement.prototype, "offsetTop", "get")
    .mockImplementation(function (this: HTMLElement) {
      const id = this.getAttribute("data-settings-section");
      return id ? (OFFSETS[id] ?? 0) : 0;
    });
  clientHeightSpy = vi
    .spyOn(Element.prototype, "clientHeight", "get")
    .mockImplementation(function (this: Element) {
      return this.getAttribute("data-testid") === "settings-sheet-scroll"
        ? SHEET_CLIENT_HEIGHT
        : 0;
    });
  scrollHeightSpy = vi
    .spyOn(Element.prototype, "scrollHeight", "get")
    .mockImplementation(function (this: Element) {
      return this.getAttribute("data-testid") === "settings-sheet-scroll"
        ? SHEET_SCROLL_HEIGHT
        : 0;
    });
});

afterEach(() => {
  offsetTopSpy.mockRestore();
  clientHeightSpy.mockRestore();
  scrollHeightSpy.mockRestore();
});

async function scrollSheetTo(top: number) {
  const el = sheet();
  Object.defineProperty(el, "scrollTop", {
    value: top,
    writable: true,
    configurable: true,
  });
  await act(async () => {
    fireEvent.scroll(el);
    // The spy runs on the next animation frame.
    await new Promise((resolve) => window.requestAnimationFrame(resolve));
  });
}

describe("SettingsPageShell", () => {
  it("lists the sections and the pages in the index, with the first section current", async () => {
    renderApp(<Page />, { route: "/g5/base?view=settings" });

    const index = await screen.findByTestId("settings-index");
    expect(
      within(index)
        .getAllByRole("button")
        .map((button) => button.textContent),
    ).toEqual(["One", "Two", "Three"]);
    expect(screen.getByTestId("settings-index-one")).toHaveAttribute(
      "aria-current",
      "location",
    );
    expect(screen.getByTestId("settings-index-two")).not.toHaveAttribute(
      "aria-current",
    );
    expect(within(index).getByText("Pages")).toBeInTheDocument();
    expect(screen.getByTestId("link-members")).toHaveAttribute(
      "href",
      "/g5/settings/members",
    );
    expect(within(index).getByText("Elsewhere")).toBeInTheDocument();
    // Header and content.
    expect(screen.getByTestId("settings-crumbs")).toHaveTextContent("g5/base");
    expect(
      screen.getByRole("heading", { level: 1, name: "Settings" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { level: 2, name: "One" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { level: 3, name: "A row" }),
    ).toBeInTheDocument();
    expect(screen.getByLabelText("A control")).toBeInTheDocument();
  });

  it("marks the section in view as the sheet scrolls, and the last one at the end", async () => {
    renderApp(<Page />, { route: "/g5/base?view=settings" });
    await screen.findByTestId("settings-index");

    await scrollSheetTo(520);
    await waitFor(() =>
      expect(screen.getByTestId("settings-index-two")).toHaveAttribute(
        "aria-current",
        "location",
      ),
    );
    expect(screen.getByTestId("settings-index-one")).not.toHaveAttribute(
      "aria-current",
    );

    // Scrolled to the end: the last section is current even though its top
    // never reaches the offset line.
    await scrollSheetTo(SHEET_SCROLL_HEIGHT - SHEET_CLIENT_HEIGHT);
    await waitFor(() =>
      expect(screen.getByTestId("settings-index-three")).toHaveAttribute(
        "aria-current",
        "location",
      ),
    );

    await scrollSheetTo(0);
    await waitFor(() =>
      expect(screen.getByTestId("settings-index-one")).toHaveAttribute(
        "aria-current",
        "location",
      ),
    );
  });

  it("scrolls to a section on click and records it in ?section=", async () => {
    renderApp(<Page />, { route: "/g5/base?view=settings" });
    await screen.findByTestId("settings-index");
    const scrollTo = vi.fn();
    Object.defineProperty(sheet(), "scrollTo", {
      value: scrollTo,
      configurable: true,
    });

    await userEvent.click(screen.getByTestId("settings-index-two"));

    expect(scrollTo).toHaveBeenCalledWith(
      expect.objectContaining({ top: OFFSETS.two - 16 }),
    );
    expect(screen.getByTestId("settings-index-two")).toHaveAttribute(
      "aria-current",
      "location",
    );
    await waitFor(() =>
      expect(screen.getByTestId("location")).toHaveTextContent(
        "?view=settings&section=two",
      ),
    );
  });

  it("lands on the section a ?section= deep link names", async () => {
    renderApp(<Page />, { route: "/g5/base?view=settings&section=three" });
    await screen.findByTestId("settings-index");

    await waitFor(() =>
      expect(screen.getByTestId("settings-index-three")).toHaveAttribute(
        "aria-current",
        "location",
      ),
    );
    // Without `scrollTo` (jsdom) the sheet's scrollTop is set directly.
    expect(sheet().scrollTop).toBe(OFFSETS.three - 16);
  });

  it("keeps a deep-linked section aligned while content above it arrives, until the person scrolls", async () => {
    let resize: (() => void) | null = null;
    const observe = vi.fn();
    const disconnect = vi.fn();
    const ResizeObserverSpy = vi
      .spyOn(window, "ResizeObserver")
      .mockImplementation(function (
        this: ResizeObserver,
        callback: ResizeObserverCallback,
      ) {
        resize = () => callback([], this);
        return { observe, unobserve: vi.fn(), disconnect } as never;
      });
    try {
      renderApp(<Page />, { route: "/g5/base?view=settings&section=two" });
      await screen.findByTestId("settings-index");
      expect(observe).toHaveBeenCalled();
      expect(sheet().scrollTop).toBe(OFFSETS.two - 16);

      // A section above grows: the sheet is re-aligned to the target.
      OFFSETS.two = 700;
      await act(async () => {
        resize?.();
      });
      expect(sheet().scrollTop).toBe(700 - 16);

      // The person scrolls: later growth no longer moves the sheet.
      fireEvent.wheel(sheet());
      OFFSETS.two = 900;
      await act(async () => {
        resize?.();
      });
      expect(sheet().scrollTop).toBe(700 - 16);
    } finally {
      OFFSETS.two = 500;
      ResizeObserverSpy.mockRestore();
    }
  });

  it("lists a section that mounts later at its place on the sheet", async () => {
    renderApp(<LatePage />, { route: "/g5/base?view=settings" });
    await screen.findByTestId("settings-index");
    expect(
      within(screen.getByTestId("settings-index"))
        .getAllByRole("button")
        .map((button) => button.textContent),
    ).toEqual(["One", "Two", "Three"]);

    await userEvent.click(screen.getByTestId("add-late"));
    await waitFor(() =>
      expect(
        within(screen.getByTestId("settings-index"))
          .getAllByRole("button")
          .map((button) => button.textContent),
      ).toEqual(["One", "Late", "Two", "Three"]),
    );
  });

  it("consumes ?section= only while the URL names this sheet's view", async () => {
    renderApp(<Page view="admin" />, {
      route: "/g5/base?view=settings&section=three",
    });
    await screen.findByTestId("settings-index");
    expect(screen.getByTestId("settings-index-one")).toHaveAttribute(
      "aria-current",
      "location",
    );
    expect(sheet().scrollTop).toBe(0);
  });

  it("moves focus to the section heading on an index click", async () => {
    renderApp(<Page />, { route: "/g5/base?view=settings" });
    await screen.findByTestId("settings-index");

    await userEvent.click(screen.getByTestId("settings-index-two"));

    expect(
      screen.getByRole("heading", { level: 2, name: "Two" }),
    ).toHaveFocus();
  });

  it("lets a scrollbar drag end a deep link's settling", async () => {
    let resize: (() => void) | null = null;
    const ResizeObserverSpy = vi
      .spyOn(window, "ResizeObserver")
      .mockImplementation(function (
        this: ResizeObserver,
        callback: ResizeObserverCallback,
      ) {
        resize = () => callback([], this);
        return {
          observe: vi.fn(),
          unobserve: vi.fn(),
          disconnect: vi.fn(),
        } as never;
      });
    try {
      renderApp(<Page />, { route: "/g5/base?view=settings&section=two" });
      await screen.findByTestId("settings-index");
      expect(sheet().scrollTop).toBe(OFFSETS.two - 16);

      fireEvent.pointerDown(sheet());
      OFFSETS.two = 800;
      await act(async () => {
        resize?.();
      });
      expect(sheet().scrollTop).toBe(500 - 16);
    } finally {
      OFFSETS.two = 500;
      ResizeObserverSpy.mockRestore();
    }
  });

  it("abandons a deep link whose section has not arrived once the person scrolls", async () => {
    renderApp(<LatePage />, { route: "/g5/base?view=settings&section=late" });
    await screen.findByTestId("settings-index");
    expect(sheet().scrollTop).toBe(0);

    fireEvent.wheel(sheet());
    await userEvent.click(screen.getByTestId("add-late"));
    await screen.findByTestId("settings-index-late");
    expect(sheet().scrollTop).toBe(0);
    expect(screen.getByTestId("settings-index-late")).not.toHaveAttribute(
      "aria-current",
    );
  });

  it("ignores a ?section= that names no section", async () => {
    renderApp(<Page />, { route: "/g5/base?view=settings&section=nope" });
    await screen.findByTestId("settings-index");
    expect(screen.getByTestId("settings-index-one")).toHaveAttribute(
      "aria-current",
      "location",
    );
    expect(sheet().scrollTop).toBe(0);
  });
});
