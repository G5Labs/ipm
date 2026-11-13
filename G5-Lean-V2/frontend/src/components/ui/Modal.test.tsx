import { afterEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import React from "react";

import { Modal } from "./Modal";
import { ThemedSelect } from "./UI";
import { useDialogFocus } from "./useDialogFocus";

function renderModal(
  props: Partial<React.ComponentProps<typeof Modal>> = {},
  children?: React.ReactNode,
) {
  const onClose = vi.fn();
  // The spread defeats the labelledBy-XOR-ariaLabel union, so merge first
  // and cast; every call site still supplies exactly one accessible name.
  const merged = {
    ariaLabel: "Test dialog",
    onClose,
    ...props,
  } as React.ComponentProps<typeof Modal>;
  const utils = render(
    <Modal {...merged}>
      {children ?? (
        <>
          <button type="button">alpha</button>
          <button type="button">omega</button>
        </>
      )}
    </Modal>,
  );
  return { onClose, ...utils };
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

describe("Modal", () => {
  it("renders a portaled dialog with role, aria-modal, and an accessible name", () => {
    const { container } = renderModal();
    const dialog = screen.getByRole("dialog", { name: "Test dialog" });
    expect(dialog).toHaveAttribute("aria-modal", "true");
    // Portaled to document.body — not inside the render container.
    expect(container.querySelector('[role="dialog"]')).toBeNull();
    expect(document.body.contains(dialog)).toBe(true);
  });

  it("supports aria-labelledby via labelledBy", () => {
    renderModal(
      { ariaLabel: undefined, labelledBy: "modal-test-title" },
      <>
        <h2 id="modal-test-title">Visible Title</h2>
        <button type="button">ok</button>
      </>,
    );
    expect(
      screen.getByRole("dialog", { name: "Visible Title" }),
    ).toBeInTheDocument();
  });

  it("closes on Escape", async () => {
    const user = userEvent.setup();
    const { onClose } = renderModal();
    await user.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("closes on an overlay press that starts and releases on the overlay", () => {
    const { onClose } = renderModal();
    const overlay = screen.getByTestId("modal-overlay");
    fireEvent.mouseDown(overlay);
    fireEvent.mouseUp(overlay);
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("does not close for presses inside the panel, nor for drags crossing the boundary in either direction", () => {
    const { onClose } = renderModal();
    const overlay = screen.getByTestId("modal-overlay");
    const inside = screen.getByRole("button", { name: "alpha" });

    fireEvent.mouseDown(inside);
    fireEvent.mouseUp(inside);
    // Drag: press inside the panel, release over the overlay.
    fireEvent.mouseDown(inside);
    fireEvent.mouseUp(overlay);
    // Drag: press on the overlay, release over the panel.
    fireEvent.mouseDown(overlay);
    fireEvent.mouseUp(inside);
    expect(onClose).not.toHaveBeenCalled();
  });

  it("suspends Escape and overlay dismissal while closeDisabled", async () => {
    const user = userEvent.setup();
    const { onClose } = renderModal({ closeDisabled: true });
    await user.keyboard("{Escape}");
    const overlay = screen.getByTestId("modal-overlay");
    fireEvent.mouseDown(overlay);
    fireEvent.mouseUp(overlay);
    expect(onClose).not.toHaveBeenCalled();
  });

  it("traps Tab within the panel and restores focus to the opener on unmount", async () => {
    const user = userEvent.setup();
    const opener = makeOpener("opener");
    opener.focus();

    const { unmount } = renderModal();
    // Initial focus moved inside the dialog.
    expect(screen.getByRole("button", { name: "alpha" })).toHaveFocus();

    screen.getByRole("button", { name: "omega" }).focus();
    await user.keyboard("{Tab}");
    expect(screen.getByRole("button", { name: "alpha" })).toHaveFocus();

    unmount();
    expect(opener).toHaveFocus();
  });

  it("honors initialFocusRef", () => {
    function Host() {
      const ref = React.useRef<HTMLButtonElement>(null);
      return (
        <Modal
          ariaLabel="With initial"
          onClose={() => {}}
          initialFocusRef={ref}
        >
          <button type="button">alpha</button>
          <button type="button" ref={ref}>
            target
          </button>
        </Modal>
      );
    }
    render(<Host />);
    expect(screen.getByRole("button", { name: "target" })).toHaveFocus();
  });

  it("closes only the top-most dialog on Escape when two modals are stacked", async () => {
    const user = userEvent.setup();
    const onCloseBase = vi.fn();
    const onCloseTop = vi.fn();
    const stacked = (topOpen: boolean) => (
      <>
        <Modal ariaLabel="Base" onClose={onCloseBase}>
          <button type="button">base-btn</button>
        </Modal>
        {topOpen && (
          <Modal ariaLabel="Top" tier="top" onClose={onCloseTop}>
            <button type="button">top-btn</button>
          </Modal>
        )}
      </>
    );
    const { rerender } = render(stacked(true));

    await user.keyboard("{Escape}");
    expect(onCloseTop).toHaveBeenCalledTimes(1);
    expect(onCloseBase).not.toHaveBeenCalled();

    // Top modal closes; the STILL-MOUNTED base instance returns to the top
    // of the stack and receives the next Escape.
    rerender(stacked(false));
    await user.keyboard("{Escape}");
    expect(onCloseBase).toHaveBeenCalledTimes(1);
    expect(onCloseTop).toHaveBeenCalledTimes(1);
  });

  it("keeps focus in the surviving top dialog when a lower dialog closes first (non-LIFO)", () => {
    const opener = makeOpener("base-opener");
    opener.focus();

    const stacked = (baseOpen: boolean) => (
      <>
        {baseOpen && (
          <Modal ariaLabel="Base" onClose={() => {}}>
            <button type="button">base-btn</button>
          </Modal>
        )}
        <Modal ariaLabel="Top" tier="top" onClose={() => {}}>
          <button type="button">top-btn</button>
        </Modal>
      </>
    );
    const { rerender } = render(stacked(true));
    const topBtn = screen.getByRole("button", { name: "top-btn" });
    topBtn.focus();

    // Close the BASE dialog while the SAME top instance stays mounted.
    rerender(stacked(false));
    // Focus must not be yanked to the base dialog's opener behind the modal.
    expect(screen.getByRole("button", { name: "top-btn" })).toHaveFocus();
  });

  it("does not let a modal steal Escape from a higher-priority overlay mounted earlier", async () => {
    const user = userEvent.setup();
    const onCloseDrawer = vi.fn();
    const onCloseModal = vi.fn();

    function Drawer() {
      const panelRef = React.useRef<HTMLDivElement>(null);
      // Mirrors the issue-report drawer: z-[130], mounted BEFORE the modal.
      useDialogFocus(panelRef, { onClose: onCloseDrawer, priority: 130 });
      return (
        <div ref={panelRef} tabIndex={-1}>
          <button type="button">drawer-btn</button>
        </div>
      );
    }

    render(
      <>
        <Drawer />
        <Modal ariaLabel="Under drawer" onClose={onCloseModal}>
          <button type="button">modal-btn</button>
        </Modal>
      </>,
    );

    // The drawer (priority 130) is visually on top even though the modal
    // (priority 50) mounted later — the modal must not have stolen focus
    // behind the drawer, and Escape must close the drawer, not the modal.
    expect(screen.getByRole("button", { name: "drawer-btn" })).toHaveFocus();
    await user.keyboard("{Escape}");
    expect(onCloseDrawer).toHaveBeenCalledTimes(1);
    expect(onCloseModal).not.toHaveBeenCalled();
  });

  it("hands focus to the new top overlay when a higher-priority one closes over it, then back out", () => {
    const opener = makeOpener("outside-opener");
    opener.focus();

    function Drawer() {
      const panelRef = React.useRef<HTMLDivElement>(null);
      useDialogFocus(panelRef, { onClose: () => {}, priority: 130 });
      return (
        <div ref={panelRef} tabIndex={-1}>
          <button type="button">drawer-btn</button>
        </div>
      );
    }
    const scene = (drawerOpen: boolean, modalOpen: boolean) => (
      <>
        {drawerOpen && <Drawer />}
        {modalOpen && (
          <Modal ariaLabel="Under drawer" onClose={() => {}}>
            <button type="button">modal-btn</button>
          </Modal>
        )}
      </>
    );

    // Drawer opens (takes focus), then a modal mounts beneath it.
    const { rerender, unmount } = render(scene(true, true));
    expect(screen.getByRole("button", { name: "drawer-btn" })).toHaveFocus();

    // Drawer closes: its opener (outside) lies behind the surviving modal,
    // so focus must land INSIDE the modal instead.
    rerender(scene(false, true));
    expect(screen.getByRole("button", { name: "modal-btn" })).toHaveFocus();

    // Modal closes last: focus falls back out to the original opener.
    unmount();
    expect(opener).toHaveFocus();
  });

  it("lets an open ThemedSelect consume Escape without closing the modal", async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    render(
      <Modal ariaLabel="With select" onClose={onClose}>
        <ThemedSelect
          value="a"
          options={[
            { value: "a", label: "Alpha" },
            { value: "b", label: "Beta" },
          ]}
          onChange={() => {}}
          testId="modal-select"
        />
      </Modal>,
    );

    // Open the listbox, then Escape: only the listbox closes.
    fireEvent.click(screen.getByTestId("modal-select"));
    expect(screen.getByRole("listbox")).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("listbox")).toBeNull();
    expect(onClose).not.toHaveBeenCalled();

    // With the listbox closed, Escape reaches the modal.
    await user.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("restores the external opener when the entire stack unmounts at once", () => {
    const opener = makeOpener("stack-opener");
    opener.focus();

    const { unmount } = render(
      <>
        <Modal ariaLabel="Base" onClose={() => {}}>
          <button type="button">base-btn</button>
        </Modal>
        <Modal ariaLabel="Top" tier="top" onClose={() => {}}>
          <button type="button">top-btn</button>
        </Modal>
      </>,
    );
    // The top dialog's saved focus target lives inside the base dialog.
    expect(screen.getByRole("button", { name: "top-btn" })).toBeInTheDocument();

    unmount();
    // Neither dialog survives; focus must land back on the external opener,
    // not on <body>.
    expect(opener).toHaveFocus();
  });

  it("traps Tab in the top-most dialog when two modals are stacked", async () => {
    const user = userEvent.setup();
    render(
      <>
        <Modal ariaLabel="Base" onClose={() => {}}>
          <button type="button">base-btn</button>
        </Modal>
        <Modal ariaLabel="Top" tier="top" onClose={() => {}}>
          <button type="button">top-a</button>
          <button type="button">top-b</button>
        </Modal>
      </>,
    );

    screen.getByRole("button", { name: "top-b" }).focus();
    await user.keyboard("{Tab}");
    expect(screen.getByRole("button", { name: "top-a" })).toHaveFocus();
  });

  it("applies the size map and the top z tier", () => {
    renderModal({ size: "lg", tier: "top" });
    const overlay = screen.getByTestId("modal-overlay");
    const panel = screen.getByRole("dialog");
    expect(overlay.className).toContain("z-[60]");
    expect(panel.className).toContain("max-w-3xl");
  });

  it("defaults to the modal z tier, md size, and the shared overlay treatment", () => {
    renderModal();
    const overlay = screen.getByTestId("modal-overlay");
    const panel = screen.getByRole("dialog");
    expect(overlay.className).toContain("z-50");
    expect(overlay.className).toContain("bg-black/50");
    expect(overlay.className).toContain("px-4");
    expect(panel.className).toContain("max-w-xl");
  });

  it("styles the panel with the theme foreground as well as its background, whatever className replaces", () => {
    // The panel is portaled onto <body>, outside #root, so text inherits
    // body's color unless the panel sets the theme foreground itself (#763).
    for (const className of [undefined, "p-0"]) {
      const { unmount } = renderModal({ className });
      const classes = screen.getByRole("dialog").className.split(/\s+/);
      expect(classes).toContain("bg-[var(--g5-bg0)]");
      expect(classes).toContain("text-[var(--g5-text0)]");
      unmount();
    }
  });
});
