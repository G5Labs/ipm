import { afterEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import React, { useRef } from "react";

import {
  MODAL_DIALOG_PRIORITY,
  REPORT_DRAWER_PRIORITY,
  useCoveringOverlayOpen,
  useDialogFocus,
} from "./useDialogFocus";

function HookPanel({
  onClose,
  closeDisabled = false,
  withInitialFocus = false,
  empty = false,
  enabled = true,
}: {
  onClose: () => void;
  closeDisabled?: boolean;
  withInitialFocus?: boolean;
  empty?: boolean;
  enabled?: boolean;
}) {
  const panelRef = useRef<HTMLDivElement>(null);
  const initialFocusRef = useRef<HTMLButtonElement>(null);
  useDialogFocus(panelRef, {
    onClose,
    closeDisabled,
    initialFocusRef: withInitialFocus ? initialFocusRef : undefined,
    enabled,
  });
  return (
    <div ref={panelRef} tabIndex={-1} data-testid="hook-panel">
      {!empty && (
        <>
          <button type="button">first</button>
          <button type="button" ref={initialFocusRef}>
            middle
          </button>
          <button type="button">last</button>
        </>
      )}
    </div>
  );
}

// Opener buttons appended to <body> are tracked and removed in afterEach so a
// failing assertion cannot leak them into later tests.
const openers: HTMLButtonElement[] = [];
function makeOpener(label: string): HTMLButtonElement {
  const button = document.createElement("button");
  button.textContent = label;
  document.body.appendChild(button);
  openers.push(button);
  return button;
}
afterEach(() => {
  for (const opener of openers.splice(0)) opener.remove();
});

describe("useDialogFocus", () => {
  it("calls onClose on Escape and suppresses it when closeDisabled", async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    const { rerender } = render(<HookPanel onClose={onClose} />);

    await user.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalledTimes(1);

    rerender(<HookPanel onClose={onClose} closeDisabled />);
    await user.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("focuses initialFocusRef when provided, else the first focusable", () => {
    const first = render(<HookPanel onClose={vi.fn()} withInitialFocus />);
    expect(screen.getByRole("button", { name: "middle" })).toHaveFocus();
    first.unmount();

    render(<HookPanel onClose={vi.fn()} />);
    expect(screen.getByRole("button", { name: "first" })).toHaveFocus();
  });

  it("leaves focus on an autoFocus element already focused inside the panel", () => {
    function AutoFocusPanel() {
      const panelRef = useRef<HTMLDivElement>(null);
      useDialogFocus(panelRef, { onClose: vi.fn() });
      return (
        <div ref={panelRef} tabIndex={-1}>
          <button type="button">first</button>
          <input autoFocus aria-label="name" />
        </div>
      );
    }
    render(<AutoFocusPanel />);
    expect(screen.getByRole("textbox", { name: "name" })).toHaveFocus();
  });

  it("falls back to focusing the panel itself when it has no focusables", () => {
    render(<HookPanel onClose={vi.fn()} empty />);
    expect(screen.getByTestId("hook-panel")).toHaveFocus();
  });

  it("wraps Tab from the last focusable to the first, and Shift+Tab back", async () => {
    const user = userEvent.setup();
    render(<HookPanel onClose={vi.fn()} />);

    screen.getByRole("button", { name: "last" }).focus();
    await user.keyboard("{Tab}");
    expect(screen.getByRole("button", { name: "first" })).toHaveFocus();

    await user.keyboard("{Shift>}{Tab}{/Shift}");
    expect(screen.getByRole("button", { name: "last" })).toHaveFocus();
  });

  it("pulls focus back inside the panel when Tab is pressed with focus outside", async () => {
    const user = userEvent.setup();
    render(
      <>
        <button type="button">outside</button>
        <HookPanel onClose={vi.fn()} />
      </>,
    );

    screen.getByRole("button", { name: "outside" }).focus();
    await user.keyboard("{Tab}");
    expect(screen.getByRole("button", { name: "first" })).toHaveFocus();
  });

  it("is inert while enabled=false and runs a full lifecycle per enable cycle", async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    const outside = makeOpener("flyout-opener");
    outside.focus();

    // Mounted closed (always-mounted flyout): no focus capture, no Escape.
    const { rerender } = render(
      <HookPanel onClose={onClose} enabled={false} />,
    );
    expect(outside).toHaveFocus();
    await user.keyboard("{Escape}");
    expect(onClose).not.toHaveBeenCalled();

    // Opening activates the lifecycle: focus moves in, Escape closes.
    rerender(<HookPanel onClose={onClose} enabled />);
    expect(screen.getByRole("button", { name: "first" })).toHaveFocus();
    await user.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalledTimes(1);

    // Closing deactivates and restores focus to the opener.
    rerender(<HookPanel onClose={onClose} enabled={false} />);
    expect(outside).toHaveFocus();
    await user.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("keeps focus off <body> across stacked non-LIFO close and whole-stack unmount", () => {
    const opener = makeOpener("stack-opener");
    opener.focus();

    function NamedPanel({ name }: { name: string }) {
      const panelRef = useRef<HTMLDivElement>(null);
      useDialogFocus(panelRef, { onClose: vi.fn() });
      return (
        <div ref={panelRef} tabIndex={-1}>
          <button type="button">{name}</button>
        </div>
      );
    }
    const scene = (lowerOpen: boolean) => (
      <>
        {lowerOpen && <NamedPanel name="lower-btn" />}
        <NamedPanel name="upper-btn" />
      </>
    );

    const { rerender, unmount } = render(scene(true));
    screen.getByRole("button", { name: "upper-btn" }).focus();

    // Lower closes first (non-LIFO): focus stays in the surviving upper panel.
    rerender(scene(false));
    expect(screen.getByRole("button", { name: "upper-btn" })).toHaveFocus();
    expect(document.body).not.toHaveFocus();

    // Remaining panel unmounts: focus returns to the connected opener.
    unmount();
    expect(opener).toHaveFocus();
    expect(document.body).not.toHaveFocus();
  });

  it("restores focus to the previously focused element on unmount", () => {
    const outside = makeOpener("opener");
    outside.focus();

    const { unmount } = render(<HookPanel onClose={vi.fn()} />);
    expect(outside).not.toHaveFocus();

    unmount();
    expect(outside).toHaveFocus();
  });
});

// platform_reports g5.quality.platform-reports.trigger / workspaces
// g5.workspace.ui.chat-corner-button: the stack reports whether an overlay
// covers the page chrome.
describe("useCoveringOverlayOpen", () => {
  function TierPanel({ priority }: { priority: number }) {
    const panelRef = useRef<HTMLDivElement>(null);
    useDialogFocus(panelRef, { onClose: () => {}, priority });
    return <div ref={panelRef} tabIndex={-1} />;
  }

  function Probe() {
    const covering = useCoveringOverlayOpen();
    return <output data-testid="covering">{String(covering)}</output>;
  }

  function Host({ tiers }: { tiers: number[] }) {
    return (
      <>
        <Probe />
        {tiers.map((priority, index) => (
          <TierPanel key={`${priority}-${index}`} priority={priority} />
        ))}
      </>
    );
  }

  it("is false with no overlay, true for the modal tier, and false again after close", () => {
    const view = render(<Host tiers={[]} />);
    expect(screen.getByTestId("covering")).toHaveTextContent("false");
    view.rerender(<Host tiers={[MODAL_DIALOG_PRIORITY]} />);
    expect(screen.getByTestId("covering")).toHaveTextContent("true");
    view.rerender(<Host tiers={[]} />);
    expect(screen.getByTestId("covering")).toHaveTextContent("false");
  });

  it("ignores tiers under the modal tier and the report drawer's own tier", () => {
    const view = render(<Host tiers={[40]} />);
    expect(screen.getByTestId("covering")).toHaveTextContent("false");
    view.rerender(<Host tiers={[REPORT_DRAWER_PRIORITY]} />);
    expect(screen.getByTestId("covering")).toHaveTextContent("false");
    // A modal under the drawer still covers the chrome.
    view.rerender(
      <Host tiers={[MODAL_DIALOG_PRIORITY, REPORT_DRAWER_PRIORITY]} />,
    );
    expect(screen.getByTestId("covering")).toHaveTextContent("true");
  });
});
