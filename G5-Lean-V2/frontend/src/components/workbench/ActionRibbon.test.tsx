import { act, fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { WORKBENCH_ACTIONS } from "../../lib/workbenchDefaults";
import type { RibbonStyle, WorkbenchAction } from "../../lib/types";
import {
  ActionRibbon,
  RIBBON_FALLBACK_CHAT_TITLE,
  RIBBON_UNSUPPORTED_TITLE,
  ribbonButtonShape,
} from "./ActionRibbon";

// Workbench g5.workbench.actions: the ribbon renders the served action set per
// phase from data, menus as dropdowns, and hands every click to the page.

function labels(groupId: string): Array<string | undefined> {
  return within(screen.getByTestId(`ribbon-group-${groupId}`))
    .getAllByRole("button")
    .map((button) => button.textContent?.trim());
}

describe("ActionRibbon", () => {
  it.each([
    "extended-icons",
    "minimal-icons",
    "extended-text",
    "simplified",
  ] as const)(
    "keeps the last action with the configured actions before workspace controls in %s",
    (variant) => {
      render(
        <ActionRibbon
          phase="se"
          variant={variant}
          actions={WORKBENCH_ACTIONS}
          configStatus="builtin"
          endActions={
            <button type="button" data-testid="last-action">
              Shared approvals
            </button>
          }
          trailing={
            <button type="button" data-testid="workspace-control">
              Owner
            </button>
          }
        />,
      );
      const approvals = screen.getByTestId("last-action");
      const buttons = within(screen.getByTestId("action-ribbon")).getAllByRole(
        "button",
      );
      expect(buttons.slice(-2)).toEqual([
        approvals,
        screen.getByTestId("workspace-control"),
      ]);
      expect(screen.getByTestId("ribbon-end-actions")).toContainElement(
        approvals,
      );
      expect(screen.getByTestId("ribbon-trailing")).not.toContainElement(
        approvals,
      );
    },
  );

  it("renders a leading shell control before the first group, in the rendered style", () => {
    render(
      <ActionRibbon
        phase="be"
        actions={WORKBENCH_ACTIONS}
        configStatus="builtin"
        leading={(style) => (
          <button type="button" data-testid="ribbon-overview">
            {style}
          </button>
        )}
      />,
    );
    const leading = screen.getByTestId("ribbon-leading");
    expect(leading).toHaveTextContent("extended-icons");
    const author = screen.getByTestId("ribbon-group-author");
    expect(
      leading.compareDocumentPosition(author) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(within(leading).getByTestId("ribbon-overview")).toBeInTheDocument();
  });

  it("renders the Business Engineering V1 button set in its groups", () => {
    render(
      <ActionRibbon
        phase="be"
        actions={WORKBENCH_ACTIONS}
        configStatus="builtin"
      />,
    );

    expect(screen.getByTestId("action-ribbon")).toHaveAttribute(
      "data-phase",
      "be",
    );
    expect(labels("author")).toEqual(["New doc", "Import"]);
    // `extract-decisions` is a hidden action of this group: started by the
    // Review meeting dialog, never drawn (g5.workbench.meeting).
    expect(labels("analysis")).toEqual(["Find gaps", "Review meeting"]);
    expect(screen.queryByTestId("ribbon-action-analyze")).toBeNull();
    expect(
      screen.queryByTestId("ribbon-action-extract-decisions"),
    ).not.toBeInTheDocument();
    // `generate-mock` is a hidden action of this group: started by the
    // Build mock dialog, never drawn (g5.workbench.mock).
    expect(labels("mock")).toEqual(["Build mock"]);
    expect(
      screen.queryByTestId("ribbon-action-generate-mock"),
    ).not.toBeInTheDocument();
    expect(screen.queryByTestId("ribbon-action-preview-mock")).toBeNull();
    expect(labels("alignment")).toEqual(["Approvals"]);
    expect(screen.getByTestId("ribbon-action-import")).toHaveAttribute(
      "aria-haspopup",
      "menu",
    );
    expect(screen.getByTestId("ribbon-action-build-mock")).toHaveAttribute(
      "data-primary",
      "true",
    );
    expect(screen.getByTestId("ribbon-action-build-mock")).toBeEnabled();
  });

  it("renders the System and Integration Engineering V1 button sets", () => {
    const { rerender } = render(
      <ActionRibbon
        phase="se"
        actions={WORKBENCH_ACTIONS}
        configStatus="committed"
      />,
    );
    expect(labels("author")).toEqual(["New doc", "Import"]);
    expect(labels("analysis")).toEqual(["Review meeting"]);
    expect(labels("design")).toEqual([
      "Create plan",
      "Explainer",
      "Diff graph",
    ]);
    expect(labels("build")).toEqual([
      "Generate code",
      "Preview",
      "Code review",
    ]);
    expect(labels("land")).toEqual(["Approval", "Rebase"]);

    rerender(
      <ActionRibbon
        phase="ie"
        actions={WORKBENCH_ACTIONS}
        configStatus="committed"
      />,
    );
    expect(labels("author")).toEqual(["New doc", "Import"]);
    expect(labels("analysis")).toEqual(["Review meeting"]);
    expect(labels("release")).toEqual([
      "Approval",
      "Re-run Verification",
      "Create PR",
      "Get Approvals",
      "Merge to parent",
    ]);
    expect(screen.getByTestId("ribbon-action-merge")).toHaveAttribute(
      "data-primary",
      "true",
    );
  });

  it("opens a menu, dispatches its item and closes again", async () => {
    const onAction = vi.fn();
    render(
      <ActionRibbon
        phase="be"
        actions={WORKBENCH_ACTIONS}
        configStatus="builtin"
        onAction={onAction}
      />,
    );
    expect(screen.queryByRole("menu")).toBeNull();
    await userEvent.click(screen.getByTestId("ribbon-action-import"));
    const menu = screen.getByRole("menu", { name: "Import" });
    expect(
      within(menu)
        .getAllByRole("menuitem")
        .map((item) => item.textContent?.trim()),
    ).toEqual(["Upload file", "Import user stories"]);
    await userEvent.click(
      screen.getByTestId("ribbon-action-import-user-stories"),
    );
    expect(onAction).toHaveBeenCalledTimes(1);
    expect(onAction.mock.calls[0][0]).toMatchObject({
      id: "import-user-stories",
      kind: "chat",
    });
    expect(screen.queryByRole("menu")).toBeNull();

    // Escape closes an open menu without dispatching.
    await userEvent.click(screen.getByTestId("ribbon-action-import"));
    expect(screen.getByRole("menu", { name: "Import" })).toBeInTheDocument();
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).toBeNull();
    expect(onAction).toHaveBeenCalledTimes(1);
  });

  it("dispatches a plain button and disables it while pending", async () => {
    const onAction = vi.fn();
    const { rerender } = render(
      <ActionRibbon
        phase="ie"
        actions={WORKBENCH_ACTIONS}
        configStatus="builtin"
        onAction={onAction}
      />,
    );
    await userEvent.click(screen.getByTestId("ribbon-action-merge"));
    expect(onAction.mock.calls[0][0]).toMatchObject({
      id: "merge",
      target: "merge",
    });
    rerender(
      <ActionRibbon
        phase="ie"
        actions={WORKBENCH_ACTIONS}
        configStatus="builtin"
        onAction={onAction}
        pendingActionId="merge"
      />,
    );
    expect(screen.getByTestId("ribbon-action-merge")).toBeDisabled();
  });

  it("enables the meeting upload and keeps unknown targets disabled with their reasons", () => {
    const actions = {
      ...WORKBENCH_ACTIONS,
      be: [
        {
          name: "Odd",
          actions: [
            {
              id: "meet",
              label: "Upload transcript",
              kind: "open",
              target: "meeting",
            },
            { id: "weird", label: "Weird", kind: "open", target: "teleport" },
            { id: "blind", label: "Blind view", kind: "open", target: "view" },
            { id: "mute", label: "No prompt", kind: "chat" },
          ] as WorkbenchAction[],
        },
      ],
    };
    const onAction = vi.fn();
    render(
      <ActionRibbon
        phase="be"
        actions={actions}
        configStatus="committed"
        onAction={onAction}
      />,
    );
    // Review meeting is live (g5.workbench.meeting, Phase 4).
    expect(screen.getByTestId("ribbon-action-meet")).toBeEnabled();
    fireEvent.click(screen.getByTestId("ribbon-action-meet"));
    expect(onAction).toHaveBeenCalledWith(
      expect.objectContaining({ id: "meet", target: "meeting" }),
    );
    onAction.mockClear();
    for (const id of ["weird", "blind", "mute"]) {
      expect(screen.getByTestId(`ribbon-action-${id}`)).toBeDisabled();
      expect(screen.getByTestId(`ribbon-action-${id}`)).toHaveAttribute(
        "title",
        RIBBON_UNSUPPORTED_TITLE,
      );
    }
    expect(onAction).not.toHaveBeenCalled();
  });

  it("applies the page's enablement over its own rules", () => {
    render(
      <ActionRibbon
        phase="ie"
        actions={WORKBENCH_ACTIONS}
        configStatus="builtin"
        actionState={(action) =>
          action.target === "merge"
            ? { disabled: true, title: "Pull requests are unavailable" }
            : undefined
        }
      />,
    );
    expect(screen.getByTestId("ribbon-action-merge")).toBeDisabled();
    expect(screen.getByTestId("ribbon-action-merge")).toHaveAttribute(
      "title",
      "Pull requests are unavailable",
    );
    expect(screen.getByTestId("ribbon-action-create-pr")).toBeEnabled();
  });

  it("widens Re-run Verification so its long word never breaks (g5.workbench.review-prompts)", () => {
    render(
      <ActionRibbon
        phase="ie"
        actions={WORKBENCH_ACTIONS}
        configStatus="builtin"
        variant="extended-icons"
      />,
    );
    expect(
      screen.getByTestId("ribbon-action-rerun-verification").className,
    ).toContain("w-[96px]");
    expect(screen.getByTestId("ribbon-action-create-pr").className).toContain(
      "w-[76px]",
    );
    // The dynamic style's compact step keeps the wider button.
    expect(
      ribbonButtonShape("compact-icons", { wide: true }).className,
    ).toContain("w-[80px]");
    expect(ribbonButtonShape("compact-icons", {}).className).toContain(
      "w-[60px]",
    );
  });

  it("shows the invalid-configuration banner and keeps the built-ins live", () => {
    render(
      <ActionRibbon
        phase="be"
        actions={WORKBENCH_ACTIONS}
        configStatus="invalid"
        configError=".g5/workbench/actions.yaml: duplicate action id `x`"
      />,
    );
    const banner = screen.getByTestId("ribbon-config-invalid");
    expect(banner).toHaveAttribute("role", "alert");
    expect(banner).toHaveTextContent(
      "Workbench configuration invalid: .g5/workbench/actions.yaml: duplicate action id `x`",
    );
    expect(screen.getByTestId("ribbon-action-build-mock")).toBeEnabled();
  });

  it("labels the client fallback, offers retry and disables templated chats", async () => {
    const onRetry = vi.fn();
    render(
      <ActionRibbon
        phase="se"
        actions={WORKBENCH_ACTIONS}
        configStatus="fallback"
        onRetryConfig={onRetry}
      />,
    );
    expect(screen.getByTestId("ribbon-config-fallback")).toHaveTextContent(
      "Workbench configuration unavailable",
    );
    await userEvent.click(screen.getByTestId("ribbon-config-retry"));
    expect(onRetry).toHaveBeenCalledTimes(1);
    expect(screen.getByTestId("ribbon-action-generate-code")).toBeDisabled();
    expect(screen.getByTestId("ribbon-action-generate-code")).toHaveAttribute(
      "title",
      RIBBON_FALLBACK_CHAT_TITLE,
    );
    // `open` actions still work from the fallback.
    expect(screen.getByTestId("ribbon-action-rebase")).toBeEnabled();
  });

  it("lets the page replace a button (preview actions) and shows the branch chip", () => {
    render(
      <ActionRibbon
        phase="se"
        actions={WORKBENCH_ACTIONS}
        configStatus="builtin"
        variant="minimal-icons"
        renderAction={(action, button) =>
          action.target === "preview" ? (
            <button
              type="button"
              className={button.className}
              data-icon-size={button.iconSize}
              data-label-hidden={String(button.labelHidden)}
            >
              custom {action.label}
            </button>
          ) : undefined
        }
      />,
    );
    const custom = screen.getByTestId("ribbon-action-preview");
    expect(custom).toHaveTextContent("custom Preview");
    // The replaced button receives the style's shape (icon-only here).
    expect(within(custom).getByRole("button")).toHaveAttribute(
      "data-icon-size",
      "18",
    );
    expect(within(custom).getByRole("button")).toHaveAttribute(
      "data-label-hidden",
      "true",
    );
    // No branch chip in any style (g5.workbench.header-styles).
    expect(screen.queryByTestId("ribbon-branch")).not.toBeInTheDocument();
  });

  // g5.workbench.header-styles: every style renders the same action ids and
  // group names; captions only in extended-text; icon-only buttons keep the
  // label as accessible name and tooltip.
  describe("styles", () => {
    const groupNames = () =>
      screen
        .getAllByRole("group")
        .map((group) => group.getAttribute("aria-label"));

    it("defaults to extended icons: labels under glyphs, no group captions", () => {
      render(
        <ActionRibbon
          phase="be"
          actions={WORKBENCH_ACTIONS}
          configStatus="builtin"
        />,
      );
      const ribbon = screen.getByTestId("action-ribbon");
      expect(ribbon).toHaveAttribute("data-variant", "extended-icons");
      expect(groupNames()).toEqual(["Author", "Analysis", "Mock", "Alignment"]);
      expect(screen.queryByText("Author")).not.toBeInTheDocument();
      expect(labels("mock")).toEqual(["Build mock"]);
      expect(
        screen.getByTestId("ribbon-action-build-mock").querySelector("svg"),
      ).not.toBeNull();
      expect(screen.queryByTestId("ribbon-branch")).not.toBeInTheDocument();
    });

    it("minimal icons keep the label as accessible name and tooltip", () => {
      render(
        <ActionRibbon
          phase="se"
          actions={WORKBENCH_ACTIONS}
          configStatus="builtin"
          variant="minimal-icons"
        />,
      );
      expect(screen.getByTestId("action-ribbon")).toHaveAttribute(
        "data-variant",
        "minimal-icons",
      );
      const generate = screen.getByRole("button", { name: "Generate code" });
      expect(generate).toHaveAttribute("title", "Generate code");
      expect(generate).toHaveTextContent("");
      // A menu button keeps its label as the tooltip.
      expect(
        screen.getByRole("button", { name: "Review meeting" }),
      ).toHaveAttribute("title", "Review meeting");
      expect(screen.queryByText("Design")).not.toBeInTheDocument();
    });

    it("extended text shows group captions and leading icons", () => {
      render(
        <ActionRibbon
          phase="be"
          actions={WORKBENCH_ACTIONS}
          configStatus="builtin"
          variant="extended-text"
        />,
      );
      expect(screen.getByText("Author")).toBeInTheDocument();
      expect(screen.getByText("Mock")).toBeInTheDocument();
      expect(labels("author")).toEqual(["New doc", "Import"]);
      expect(
        screen.getByTestId("ribbon-action-new-doc").querySelector("svg"),
      ).not.toBeNull();
    });

    it("simplified renders one row without captions and with every action", () => {
      render(
        <ActionRibbon
          phase="ie"
          actions={WORKBENCH_ACTIONS}
          configStatus="builtin"
          variant="simplified"
        />,
      );
      expect(screen.getByTestId("action-ribbon")).toHaveAttribute(
        "data-variant",
        "simplified",
      );
      expect(screen.queryByText("Release")).not.toBeInTheDocument();
      expect(labels("release")).toEqual([
        "Approval",
        "Re-run Verification",
        "Create PR",
        "Get Approvals",
        "Merge to parent",
      ]);
      expect(groupNames()).toEqual(["Author", "Analysis", "Release"]);
    });

    it("opens a menu and hands the item to the page in the icon style", async () => {
      const onAction = vi.fn();
      render(
        <ActionRibbon
          phase="be"
          actions={WORKBENCH_ACTIONS}
          configStatus="builtin"
          variant="minimal-icons"
          onAction={onAction}
        />,
      );
      await userEvent.click(screen.getByRole("button", { name: "Import" }));
      await userEvent.click(
        screen.getByRole("menuitem", { name: "Import user stories" }),
      );
      expect(onAction).toHaveBeenCalledWith(
        expect.objectContaining({ id: "import-user-stories" }),
      );
    });

    // `dynamic`: extended icons that shrink one step when the groups
    // overflow, then scroll; the ribbon measures its own row.
    describe("dynamic", () => {
      /** Live observers, notified as a browser would on a resize. */
      const observers = new Set<{ notify: () => void }>();
      class NotifyingResizeObserver {
        private readonly entry: { notify: () => void };
        constructor(callback: ResizeObserverCallback) {
          this.entry = {
            notify: () => callback([], this as unknown as ResizeObserver),
          };
        }
        observe() {
          observers.add(this.entry);
        }
        unobserve() {}
        disconnect() {
          observers.delete(this.entry);
        }
      }
      let original: typeof ResizeObserver;
      beforeEach(() => {
        original = window.ResizeObserver;
        window.ResizeObserver =
          NotifyingResizeObserver as unknown as typeof ResizeObserver;
      });
      afterEach(() => {
        window.ResizeObserver = original;
        observers.clear();
      });

      /** jsdom lays nothing out: give the row and its groups widths. */
      function layout(
        outerWidth: number,
        groupsScroll: number,
        groupsClient: number,
      ) {
        const groups = screen.getByTestId("ribbon-groups");
        const outer = groups.parentElement as HTMLElement;
        Object.defineProperty(outer, "clientWidth", {
          configurable: true,
          value: outerWidth,
        });
        Object.defineProperty(groups, "scrollWidth", {
          configurable: true,
          value: groupsScroll,
        });
        Object.defineProperty(groups, "clientWidth", {
          configurable: true,
          value: groupsClient,
        });
        act(() => observers.forEach((observer) => observer.notify()));
      }
      const density = () =>
        screen.getByTestId("action-ribbon").getAttribute("data-density");
      const glyphSize = (testId: string) =>
        screen.getByTestId(testId).querySelector("svg")?.getAttribute("width");

      it("renders extended icons at full size while the row fits", () => {
        render(
          <ActionRibbon
            phase="be"
            actions={WORKBENCH_ACTIONS}
            configStatus="builtin"
            variant="dynamic"
          />,
        );
        const ribbon = screen.getByTestId("action-ribbon");
        expect(ribbon).toHaveAttribute("data-variant", "dynamic");
        expect(density()).toBe("full");
        expect(glyphSize("ribbon-action-build-mock")).toBe("22");
        expect(screen.getByTestId("ribbon-groups").className).toContain(
          "overflow-x-auto",
        );
        expect(screen.queryByText("Author")).not.toBeInTheDocument();
      });

      it("shrinks to compact on overflow, the controls follow, and widening restores full", () => {
        const trailing = vi.fn((style: RibbonStyle) => (
          <span data-testid="trailing-style">{style}</span>
        ));
        const endActions = vi.fn((style: RibbonStyle) => (
          <button data-testid="approvals-style">{style}</button>
        ));
        render(
          <ActionRibbon
            phase="be"
            actions={WORKBENCH_ACTIONS}
            configStatus="builtin"
            variant="dynamic"
            trailing={trailing}
            endActions={endActions}
          />,
        );
        expect(screen.getByTestId("trailing-style")).toHaveTextContent(
          "extended-icons",
        );
        expect(screen.getByTestId("approvals-style")).toHaveTextContent(
          "extended-icons",
        );
        const before = screen
          .getAllByRole("button")
          .map((button) => button.dataset.testid);
        // A label cut by the two-line clamp keeps its full text as tooltip.
        const meetingLabel = screen
          .getByTestId("ribbon-action-review-meeting")
          .querySelector("span") as HTMLElement;
        Object.defineProperty(meetingLabel, "scrollHeight", {
          configurable: true,
          value: 39,
        });
        Object.defineProperty(meetingLabel, "clientHeight", {
          configurable: true,
          value: 26,
        });

        // 300px too wide at an 800px row: full fits again from 1100px.
        layout(800, 900, 600);
        expect(density()).toBe("compact");
        expect(screen.getByTestId("action-ribbon")).toHaveAttribute(
          "data-variant",
          "dynamic",
        );
        expect(glyphSize("ribbon-action-build-mock")).toBe("18");
        expect(
          screen.getByTestId("ribbon-action-build-mock").className,
        ).toContain("w-[60px]");
        expect(
          screen.getAllByRole("button").map((button) => button.dataset.testid),
        ).toEqual(before);
        expect(
          screen
            .getAllByRole("group")
            .map((group) => group.getAttribute("aria-label")),
        ).toEqual(["Author", "Analysis", "Mock", "Alignment"]);
        expect(screen.getByTestId("trailing-style")).toHaveTextContent(
          "compact-icons",
        );
        expect(trailing).toHaveBeenLastCalledWith("compact-icons");
        expect(screen.getByTestId("approvals-style")).toHaveTextContent(
          "compact-icons",
        );
        expect(endActions).toHaveBeenLastCalledWith("compact-icons");
        expect(
          screen.getByTestId("ribbon-action-review-meeting"),
        ).toHaveAttribute("title", "Review meeting");
        expect(screen.getByTestId("ribbon-action-new-doc")).not.toHaveAttribute(
          "title",
        );

        // Wider, but not yet where the full layout fit: stays compact.
        layout(1050, 700, 700);
        expect(density()).toBe("compact");
        layout(1100, 1000, 1000);
        expect(density()).toBe("full");
        expect(glyphSize("ribbon-action-build-mock")).toBe("22");
        expect(screen.getByTestId("trailing-style")).toHaveTextContent(
          "extended-icons",
        );
        expect(screen.getByTestId("approvals-style")).toHaveTextContent(
          "extended-icons",
        );
      });

      it("remeasures the full layout when the final workspace action disappears", () => {
        const { rerender } = render(
          <ActionRibbon
            phase="be"
            actions={WORKBENCH_ACTIONS}
            configStatus="builtin"
            variant="dynamic"
            endActions={<button>Approvals</button>}
          />,
        );
        layout(800, 900, 600);
        expect(density()).toBe("compact");
        layout(800, 600, 600);
        rerender(
          <ActionRibbon
            phase="be"
            actions={WORKBENCH_ACTIONS}
            configStatus="builtin"
            variant="dynamic"
          />,
        );
        expect(density()).toBe("full");
        expect(screen.queryByTestId("ribbon-end-actions")).toBeNull();
      });

      it("goes compact on a 1px overflow and back to full at exactly that width", () => {
        render(
          <ActionRibbon
            phase="se"
            actions={WORKBENCH_ACTIONS}
            configStatus="builtin"
            variant="dynamic"
          />,
        );
        layout(800, 601, 600);
        expect(density()).toBe("compact");
        // One pixel short of where full fits: still compact.
        layout(800, 500, 500);
        expect(density()).toBe("compact");
        // Exactly where full fits (no direction-dependent band).
        layout(801, 600, 600);
        expect(density()).toBe("full");
      });

      it("starts over at full when the phase changes", () => {
        const { rerender } = render(
          <ActionRibbon
            phase="be"
            actions={WORKBENCH_ACTIONS}
            configStatus="builtin"
            variant="dynamic"
          />,
        );
        layout(800, 900, 600);
        expect(density()).toBe("compact");
        // The new phase's groups fit the same row: no stale compact state.
        layout(800, 600, 600);
        expect(density()).toBe("compact");
        rerender(
          <ActionRibbon
            phase="se"
            actions={WORKBENCH_ACTIONS}
            configStatus="builtin"
            variant="dynamic"
          />,
        );
        expect(density()).toBe("full");
        expect(glyphSize("ribbon-action-generate-code")).toBe("22");
      });

      it("opens a fixed menu, dispatches its item, and closes when the row scrolls", async () => {
        const onAction = vi.fn();
        render(
          <ActionRibbon
            phase="be"
            actions={WORKBENCH_ACTIONS}
            configStatus="builtin"
            variant="dynamic"
            onAction={onAction}
          />,
        );
        layout(800, 900, 600);
        const trigger = screen.getByTestId("ribbon-action-import");
        trigger.getBoundingClientRect = () =>
          ({ left: 120, right: 180, top: 10, bottom: 70 }) as DOMRect;
        await userEvent.click(trigger);
        const menu = screen.getByTestId("ribbon-menu-import");
        // Still the trigger's DOM descendant, positioned from its rect.
        expect(trigger.parentElement).toContainElement(menu);
        expect(menu.className).toContain("fixed");
        expect(menu.style.left).toBe("120px");
        expect(menu.style.top).toBe("74px");
        await userEvent.click(
          screen.getByRole("menuitem", { name: "Import user stories" }),
        );
        expect(onAction).toHaveBeenCalledWith(
          expect.objectContaining({ id: "import-user-stories" }),
        );
        expect(screen.queryByRole("menu")).toBeNull();

        await userEvent.click(trigger);
        expect(
          screen.getByRole("menu", { name: "Import" }),
        ).toBeInTheDocument();
        const groups = screen.getByTestId("ribbon-groups");
        groups.scrollLeft = 40;
        fireEvent.scroll(groups);
        expect(screen.queryByRole("menu")).toBeNull();

        await userEvent.click(trigger);
        expect(
          screen.getByRole("menu", { name: "Import" }),
        ).toBeInTheDocument();
        fireEvent(window, new Event("resize"));
        expect(screen.queryByRole("menu")).toBeNull();
        expect(onAction).toHaveBeenCalledTimes(1);
      });
    });
  });
});
