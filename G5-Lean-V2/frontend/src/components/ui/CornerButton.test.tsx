import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { firePointer } from "../../test/pointer";
import { CornerButton, CornerStack } from "./CornerButton";

const STORAGE_KEY = "test:corner-button-offset";

function press(
  el: Element,
  from: [number, number],
  to: [number, number],
): void {
  firePointer(el, "pointerdown", from[0], from[1]);
  firePointer(el, "pointermove", to[0], to[1]);
  firePointer(el, "pointerup", to[0], to[1]);
  // The browser dispatches a click after every completed press.
  fireEvent.click(el);
}

function renderButton(
  props: Partial<React.ComponentProps<typeof CornerButton>> = {},
) {
  const onClick = vi.fn();
  render(
    <CornerButton
      corner="right"
      label="Open chat"
      onClick={onClick}
      testId="corner"
      {...props}
    >
      x
    </CornerButton>,
  );
  return { onClick, button: screen.getByTestId("corner") };
}

// workspaces g5.workspace.ui.chat-corner-button: a movable corner button can
// be dragged out of the way of what it covers and remembers the spot per
// browser; a short press is still a click, a drag never is.
describe("CornerButton", () => {
  let innerWidth = 0;
  let innerHeight = 0;
  beforeEach(() => {
    innerWidth = window.innerWidth;
    innerHeight = window.innerHeight;
    window.innerWidth = 1280;
    window.innerHeight = 800;
    localStorage.removeItem(STORAGE_KEY);
  });
  afterEach(() => {
    window.innerWidth = innerWidth;
    window.innerHeight = innerHeight;
  });

  it("pins to the corner with classes and no inline position when not movable", () => {
    const { button } = renderButton();
    expect(button.className).toContain("right-4");
    expect(button.className).toContain("bottom-4");
    expect(button.style.right).toBe("");
    expect(button.style.bottom).toBe("");
  });

  // ui g5.workspace.ui.chat-bubble-stack: the stack sits above the button
  // and moves with it.
  it("carries a stack above the button that follows a drag", () => {
    const { button } = renderButton({
      movable: { storageKey: STORAGE_KEY },
      stack: <span data-testid="stacked">bubbles</span>,
    });
    const stack = screen.getByTestId("corner-stack");
    expect(stack).toContainElement(screen.getByTestId("stacked"));
    expect(stack.style.right).toBe("16px");
    expect(stack.style.bottom).toBe("64px");

    press(button, [100, 100], [0, 0]);
    expect(button.style.right).toBe("116px");
    expect(button.style.bottom).toBe("116px");
    expect(stack.style.right).toBe("116px");
    expect(stack.style.bottom).toBe("164px");
  });

  // ui g5.workspace.ui.chat-bubble-stack: any bubble is a handle for the
  // pair; the drag's own click never opens the pressed chat.
  it("drags the button and stack together from a bubble in the stack, without clicking it", () => {
    const onBubble = vi.fn();
    const { button, onClick } = renderButton({
      movable: { storageKey: STORAGE_KEY },
      stack: (
        <button type="button" data-testid="bubble" onClick={onBubble}>
          bubble
        </button>
      ),
    });
    const bubble = screen.getByTestId("bubble");
    const stack = screen.getByTestId("corner-stack");
    expect(stack.style.touchAction).toBe("none");

    press(bubble, [1250, 700], [1050, 600]);
    expect(button.style.right).toBe("216px");
    expect(button.style.bottom).toBe("116px");
    expect(stack.style.right).toBe("216px");
    expect(stack.style.bottom).toBe("164px");
    expect(onBubble).not.toHaveBeenCalled();
    expect(onClick).not.toHaveBeenCalled();
    expect(JSON.parse(localStorage.getItem(STORAGE_KEY)!)).toEqual({
      x: 216,
      y: 116,
    });

    // A short press on the bubble is still its click.
    press(bubble, [1050, 600], [1051, 601]);
    expect(onBubble).toHaveBeenCalledTimes(1);
    expect(button.style.right).toBe("216px");
  });

  // A browser retargets the pointerup and the click to the element holding
  // pointer capture: the pressed bubble must hold it, not the stack, or a
  // short press's click never reaches the bubble; and it must hold it from
  // the pointerdown, or a brisk drag leaves the handle before the threshold.
  it("captures the pointer on the pressed bubble at once, not on the stack", () => {
    renderButton({
      movable: { storageKey: STORAGE_KEY },
      stack: (
        <button type="button" data-testid="bubble">
          bubble
        </button>
      ),
    });
    const stack = screen.getByTestId("corner-stack");
    const bubble = screen.getByTestId("bubble");
    const stackCapture = vi.fn();
    const bubbleCapture = vi.fn();
    const bubbleRelease = vi.fn();
    Object.defineProperty(stack, "setPointerCapture", { value: stackCapture });
    Object.defineProperty(bubble, "setPointerCapture", {
      value: bubbleCapture,
    });
    Object.defineProperty(bubble, "hasPointerCapture", { value: () => true });
    Object.defineProperty(bubble, "releasePointerCapture", {
      value: bubbleRelease,
    });

    firePointer(bubble, "pointerdown", 1250, 700);
    expect(bubbleCapture).toHaveBeenCalledWith(1);
    expect(stackCapture).not.toHaveBeenCalled();
    firePointer(bubble, "pointerup", 1250, 700);
    expect(bubbleRelease).toHaveBeenCalledWith(1);
  });

  it("drags the lone stack from a bubble to a spot the button later takes", () => {
    const onBubble = vi.fn();
    render(
      <CornerStack corner="right" storageKey={STORAGE_KEY} testId="alone">
        <button type="button" data-testid="bubble" onClick={onBubble}>
          bubble
        </button>
      </CornerStack>,
    );
    const alone = screen.getByTestId("alone");
    expect(alone.style.touchAction).toBe("none");
    const bubble = screen.getByTestId("bubble");

    press(bubble, [1250, 780], [1050, 680]);
    expect(alone.style.right).toBe("216px");
    expect(alone.style.bottom).toBe("116px");
    expect(onBubble).not.toHaveBeenCalled();
    expect(JSON.parse(localStorage.getItem(STORAGE_KEY)!)).toEqual({
      x: 216,
      y: 116,
    });

    press(bubble, [1050, 680], [1050, 680]);
    expect(onBubble).toHaveBeenCalledTimes(1);

    cleanup();
    const { button } = renderButton({ movable: { storageKey: STORAGE_KEY } });
    expect(button.style.right).toBe("216px");
    expect(button.style.bottom).toBe("116px");
  });

  it("keeps the lone stack still without a storage key", () => {
    render(
      <CornerStack corner="right" testId="alone">
        <span data-testid="bubble">bubble</span>
      </CornerStack>,
    );
    const alone = screen.getByTestId("alone");
    press(screen.getByTestId("bubble"), [1250, 780], [1050, 680]);
    expect(alone.style.right).toBe("16px");
    expect(alone.style.bottom).toBe("16px");
    expect(alone.style.touchAction ?? "").toBe("");
  });

  it("places a stack alone at the button's remembered spot, clamped to the viewport", () => {
    localStorage.setItem(STORAGE_KEY, JSON.stringify({ x: 300, y: 5000 }));
    render(
      <CornerStack corner="right" storageKey={STORAGE_KEY} testId="alone">
        <span data-testid="stacked">bubbles</span>
      </CornerStack>,
    );
    const stack = screen.getByTestId("alone");
    expect(stack).toContainElement(screen.getByTestId("stacked"));
    expect(stack.style.right).toBe("300px");
    // Clamped to the top edge, the stack hangs down from the spot.
    expect(stack).toHaveAttribute("data-stack-direction", "down");
    expect(stack.style.top).toBe("0px");
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("hangs the stack below a button dragged into the top half, and the lone stack too", () => {
    const { button } = renderButton({
      movable: { storageKey: STORAGE_KEY },
      stack: <span>bubbles</span>,
    });
    // Drag the button up to 100px from the top edge of an 800px viewport.
    press(button, [100, 700], [100, 100]);
    expect(button.style.bottom).toBe("616px");
    const stack = screen.getByTestId("corner-stack");
    expect(stack).toHaveAttribute("data-stack-direction", "down");
    expect(stack.style.bottom).toBe("");
    // Just under the button: its top (800 - 616 - 40) plus its size and gap.
    expect(stack.style.top).toBe(`${144 + 40 + 8}px`);

    cleanup();
    render(
      <CornerStack corner="right" storageKey={STORAGE_KEY} testId="alone">
        <span>bubbles</span>
      </CornerStack>,
    );
    const alone = screen.getByTestId("alone");
    expect(alone).toHaveAttribute("data-stack-direction", "down");
    expect(alone.style.top).toBe("144px");
  });

  it("places a stack alone at the home corner without a stored spot", () => {
    render(
      <CornerStack corner="left" testId="alone">
        <span>bubbles</span>
      </CornerStack>,
    );
    const stack = screen.getByTestId("alone");
    expect(stack.style.left).toBe("16px");
    expect(stack.style.bottom).toBe("16px");
  });

  it("renders no stack element without a stack", () => {
    renderButton();
    expect(screen.queryByTestId("corner-stack")).toBeNull();
  });

  it("starts at the home offset and clicks without moving", async () => {
    const user = userEvent.setup();
    const { button, onClick } = renderButton({
      movable: { storageKey: STORAGE_KEY },
    });
    expect(button.style.right).toBe("16px");
    expect(button.style.bottom).toBe("16px");
    expect(button.className).not.toContain("right-4");

    await user.click(button);
    expect(onClick).toHaveBeenCalledTimes(1);
    expect(localStorage.getItem(STORAGE_KEY)).toBeNull();
  });

  it("moves with a drag, remembers the spot, and does not click", () => {
    const { button, onClick } = renderButton({
      movable: { storageKey: STORAGE_KEY },
    });

    // Up and to the left, away from the home corner: both offsets grow.
    press(button, [1250, 780], [1050, 680]);
    expect(button.style.right).toBe("216px");
    expect(button.style.bottom).toBe("116px");
    expect(onClick).not.toHaveBeenCalled();
    expect(JSON.parse(localStorage.getItem(STORAGE_KEY)!)).toEqual({
      x: 216,
      y: 116,
    });

    // Only the drag's own click is swallowed; the next press clicks again.
    fireEvent.click(button);
    expect(onClick).toHaveBeenCalledTimes(1);
  });

  it("disarms the swallowed click once the press's own click has had its turn", async () => {
    const { button, onClick } = renderButton({
      movable: { storageKey: STORAGE_KEY },
    });
    firePointer(button, "pointerdown", 1250, 780);
    firePointer(button, "pointermove", 1150, 780);
    firePointer(button, "pointerup", 1150, 780);
    // A touch drag dispatches no click; a later Enter must still act.
    await new Promise((resolve) => setTimeout(resolve, 0));
    fireEvent.click(button);
    expect(onClick).toHaveBeenCalledTimes(1);
  });

  it("keeps a press within the drag threshold as a click", () => {
    const { button, onClick } = renderButton({
      movable: { storageKey: STORAGE_KEY },
    });
    press(button, [1250, 780], [1252, 782]);
    expect(button.style.right).toBe("16px");
    expect(onClick).toHaveBeenCalledTimes(1);
    expect(localStorage.getItem(STORAGE_KEY)).toBeNull();
  });

  it("stays inside the viewport, however far the drag goes", () => {
    const { button } = renderButton({ movable: { storageKey: STORAGE_KEY } });
    press(button, [1250, 780], [-500, -500]);
    expect(button.style.right).toBe(`${1280 - 40}px`);
    expect(button.style.bottom).toBe(`${800 - 40}px`);
    press(button, [20, 20], [2000, 2000]);
    expect(button.style.right).toBe("0px");
    expect(button.style.bottom).toBe("0px");
  });

  it("opens at the remembered spot, clamped to this viewport", () => {
    localStorage.setItem(STORAGE_KEY, JSON.stringify({ x: 300, y: 5000 }));
    const { button } = renderButton({ movable: { storageKey: STORAGE_KEY } });
    expect(button.style.right).toBe("300px");
    expect(button.style.bottom).toBe(`${800 - 40}px`);
  });

  it("ignores a remembered spot it cannot read", () => {
    localStorage.setItem(STORAGE_KEY, "nonsense");
    const { button } = renderButton({ movable: { storageKey: STORAGE_KEY } });
    expect(button.style.right).toBe("16px");
    expect(button.style.bottom).toBe("16px");
  });

  it("follows a viewport that shrinks under it", () => {
    localStorage.setItem(STORAGE_KEY, JSON.stringify({ x: 1000, y: 700 }));
    const { button } = renderButton({ movable: { storageKey: STORAGE_KEY } });
    expect(button.style.right).toBe("1000px");
    window.innerWidth = 600;
    window.innerHeight = 400;
    fireEvent(window, new Event("resize"));
    expect(button.style.right).toBe("560px");
    expect(button.style.bottom).toBe("360px");
  });

  it("can be moved while disabled and still never clicks", () => {
    const { button, onClick } = renderButton({
      movable: { storageKey: STORAGE_KEY },
      disabledReason: "Read-only workspace.",
    });
    expect(button).toHaveAttribute("aria-disabled", "true");
    expect(button).toHaveAttribute("title", "Read-only workspace.");
    press(button, [1250, 780], [1150, 780]);
    expect(button.style.right).toBe("116px");
    fireEvent.click(button);
    expect(onClick).not.toHaveBeenCalled();
  });

  it("drops a press the browser cancels without moving further, and still clicks next", () => {
    const { button, onClick } = renderButton({
      movable: { storageKey: STORAGE_KEY },
    });
    firePointer(button, "pointerdown", 1250, 780);
    firePointer(button, "pointermove", 1150, 780);
    firePointer(button, "pointercancel", 1150, 780);
    firePointer(button, "pointermove", 900, 780);
    expect(button.style.right).toBe("116px");
    expect(JSON.parse(localStorage.getItem(STORAGE_KEY)!)).toEqual({
      x: 116,
      y: 16,
    });
    // No click follows a cancelled press, so the next one (keyboard or
    // pointer) must act.
    fireEvent.click(button);
    expect(onClick).toHaveBeenCalledTimes(1);
  });

  it("ignores a second pointer while one press is active", () => {
    const { button, onClick } = renderButton({
      movable: { storageKey: STORAGE_KEY },
    });
    firePointer(button, "pointerdown", 1250, 780, 1);
    firePointer(button, "pointerdown", 600, 400, 2);
    firePointer(button, "pointermove", 500, 400, 2);
    expect(button.style.right).toBe("16px");
    firePointer(button, "pointermove", 1150, 780, 1);
    firePointer(button, "pointerup", 600, 400, 2);
    firePointer(button, "pointerup", 1150, 780, 1);
    fireEvent.click(button);
    expect(button.style.right).toBe("116px");
    expect(onClick).not.toHaveBeenCalled();
  });
});
