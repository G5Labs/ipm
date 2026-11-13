import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { createMockChatSession } from "../../test/fixtures";
import type { ChatSessionActivityDTO, ChatSessionDTO } from "../../lib/types";
import { ChatBubbleStack, chatBubbles } from "./ChatBubbleStack";

// The 3D head draws on a canvas jsdom does not implement; the stack only has
// to hand it the right state and pose.
vi.mock("robot-heads", () => ({
  RobotHead: (props: { state: string; paused?: boolean; size: number }) => (
    <canvas
      data-testid="robot-head"
      data-state={props.state}
      data-paused={String(props.paused ?? false)}
      data-size={props.size}
    />
  ),
}));

function activity(
  overrides: Partial<ChatSessionActivityDTO>,
): ChatSessionActivityDTO {
  return {
    activeTurnStartedAt: null,
    lastFinishedAt: null,
    latestResult: null,
    unseenResult: false,
    ...overrides,
  };
}

const WORKING = activity({ activeTurnStartedAt: "2024-11-15T10:00:00Z" });
const WAITING = activity({
  activeTurnStartedAt: "2024-11-15T10:01:00Z",
  awaitingInput: true,
});
const IDLE = activity({
  lastFinishedAt: "2024-11-15T09:00:00Z",
  latestResult: { taskId: "t-idle", attempt: 1, hasOutput: true },
  unseenResult: true,
});
const QUIET = activity({
  lastFinishedAt: "2024-11-15T08:00:00Z",
  latestResult: { taskId: "t-quiet", attempt: 1, hasOutput: true },
});

function session(
  id: string,
  title: string,
  sessionActivity: ChatSessionActivityDTO,
): ChatSessionDTO {
  return createMockChatSession({ id, title, activity: sessionActivity });
}

/** The server's list order, most recent first: the quiet chat is last. */
const SESSIONS = [
  session("s-working", "Fix the flaky test", WORKING),
  session("s-waiting", "Add the run log", WAITING),
  session("s-idle", "Rename the title bar", IDLE),
  session("s-quiet", "Spend breakdown", QUIET),
];

function reducedMotion(matches: boolean) {
  vi.mocked(window.matchMedia).mockImplementation((query: string) => ({
    matches: matches && query === "(prefers-reduced-motion: reduce)",
    media: query,
    onchange: null,
    addListener: vi.fn(),
    removeListener: vi.fn(),
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    dispatchEvent: vi.fn(),
  }));
}

function renderStack(
  props: Partial<React.ComponentProps<typeof ChatBubbleStack>> = {},
) {
  const onOpen = vi.fn();
  render(
    <ChatBubbleStack
      sessions={SESSIONS}
      style="still"
      onOpen={onOpen}
      {...props}
    />,
  );
  return { onOpen };
}

function bubbleIds(): string[] {
  return screen
    .getAllByTestId("chat-bubble")
    .map((bubble) => bubble.dataset.sessionId ?? "");
}

// ui g5.workspace.ui.chat-bubble-stack: one bubble per listed chat, its face
// from the server's activity, the most recent nearest the button.
describe("ChatBubbleStack", () => {
  afterEach(() => {
    reducedMotion(false);
  });

  it("keeps the list's order and gives every chat a bubble, the quiet one too", () => {
    expect(chatBubbles(SESSIONS).map((bubble) => bubble.session.id)).toEqual([
      "s-working",
      "s-waiting",
      "s-idle",
      "s-quiet",
    ]);
  });

  it("shows a bubble per chat, the first rendered last, with its state", () => {
    renderStack();
    // Rendered top to bottom; the first listed chat sits just above the button.
    expect(bubbleIds()).toEqual([
      "s-quiet",
      "s-idle",
      "s-waiting",
      "s-working",
    ]);
    const waiting = screen.getByRole("button", {
      name: "Add the run log: Waiting for your answer",
    });
    expect(waiting).toHaveAttribute("data-state", "waiting");
    // The name shows beside the bubble on hover, not as a native tooltip.
    expect(waiting).not.toHaveAttribute("title");
    const label = within(waiting).getByTestId("chat-bubble-label");
    expect(label).toHaveTextContent("Add the run log");
    expect(label).toHaveTextContent("Waiting for your answer");
    expect(label.className).toContain("group-hover/bubble:opacity-100");
    expect(
      screen.getByRole("button", { name: "Fix the flaky test: Working" }),
    ).toHaveAttribute("data-state", "working");
    const idle = screen.getByRole("button", {
      name: "Rename the title bar: Idle, new result",
    });
    expect(idle).toHaveAttribute("data-state", "idle");
    expect(within(idle).getByTestId("chat-bubble-unseen")).toBeInTheDocument();
    expect(within(waiting).queryByTestId("chat-bubble-unseen")).toBeNull();
    // A chat at rest: the idle face, no dot.
    const quiet = screen.getByRole("button", { name: "Spend breakdown: Idle" });
    expect(quiet).toHaveAttribute("data-state", "idle");
    expect(within(quiet).queryByTestId("chat-bubble-unseen")).toBeNull();
    // Still faces: one image, no alternating frame.
    expect(waiting.querySelectorAll("img")).toHaveLength(1);
    expect(screen.queryByTestId("chat-bubble-frame")).toBeNull();
    expect(screen.queryByTestId("chat-bubble-count")).toBeNull();
  });

  it("opens the pressed chat", async () => {
    const user = userEvent.setup();
    const { onOpen } = renderStack();
    await user.click(
      screen.getByRole("button", { name: "Fix the flaky test: Working" }),
    );
    expect(onOpen).toHaveBeenCalledWith("s-working");
  });

  it("renders nothing while the style is off or the workspace has no chat", () => {
    const { container } = render(
      <ChatBubbleStack sessions={SESSIONS} style="off" onOpen={vi.fn()} />,
    );
    expect(container).toBeEmptyDOMElement();
    const empty = render(
      <ChatBubbleStack sessions={[]} style="still" onOpen={vi.fn()} />,
    );
    expect(empty.container).toBeEmptyDOMElement();
  });

  it("shows five by default, and the count unfolds every chat until folded back", async () => {
    const user = userEvent.setup();
    const many = Array.from({ length: 7 }, (_, i) =>
      session(`s-${i}`, `Chat ${i}`, i % 2 === 0 ? WORKING : IDLE),
    );
    const { onOpen } = renderStack({ sessions: many });
    expect(bubbleIds()).toEqual(["s-4", "s-3", "s-2", "s-1", "s-0"]);
    const more = screen.getByTestId("chat-bubble-more");
    expect(more).toHaveTextContent("+2");
    expect(more).toHaveAccessibleName("2 more chats; show all");
    expect(more).toHaveAttribute("aria-expanded", "false");

    await user.click(more);
    expect(onOpen).not.toHaveBeenCalled();
    expect(screen.getAllByTestId("chat-bubble")).toHaveLength(7);
    expect(screen.getByTestId("chat-bubble-stack")).toHaveAttribute(
      "data-expanded",
      "true",
    );
    const fewer = screen.getByTestId("chat-bubble-fewer");
    expect(fewer).toHaveAccessibleName("Show the 5 most recent chats only");
    expect(fewer).toHaveAttribute("aria-expanded", "true");
    expect(screen.queryByTestId("chat-bubble-more")).toBeNull();

    await user.click(fewer);
    expect(screen.getAllByTestId("chat-bubble")).toHaveLength(5);
    expect(screen.getByTestId("chat-bubble-more")).toHaveTextContent("+2");
  });

  it("shows as many bubbles as the limit setting asks, with no count when all fit", () => {
    renderStack({ limit: 2 });
    expect(bubbleIds()).toEqual(["s-waiting", "s-working"]);
    expect(screen.getByTestId("chat-bubble-more")).toHaveTextContent("+2");
    cleanup();
    renderStack({ limit: 4 });
    expect(screen.getAllByTestId("chat-bubble")).toHaveLength(4);
    expect(screen.queryByTestId("chat-bubble-more")).toBeNull();
    cleanup();
    // A limit below one still shows the nearest chat.
    renderStack({ limit: 0 });
    expect(bubbleIds()).toEqual(["s-working"]);
    expect(screen.getByTestId("chat-bubble-more")).toHaveTextContent("+3");
  });

  it("animates a second frame per state unless motion is reduced", () => {
    const { unmount } = render(
      <ChatBubbleStack sessions={SESSIONS} style="animated" onOpen={vi.fn()} />,
    );
    const frames = screen.getAllByTestId("chat-bubble-frame");
    expect(frames.map((frame) => frame.dataset.period)).toEqual([
      "idle",
      "idle",
      "waiting",
      "working",
    ]);
    expect(frames[0]).toHaveClass("g5-chat-bubble-frame-alt");
    unmount();

    reducedMotion(true);
    renderStack({ style: "animated" });
    expect(screen.getAllByTestId("chat-bubble")).toHaveLength(4);
    expect(screen.queryByTestId("chat-bubble-frame")).toBeNull();
  });

  it("draws a robot head per state in 3D and holds the pose under reduced motion", async () => {
    reducedMotion(true);
    renderStack({ style: "3d" });
    const heads = await screen.findAllByTestId("robot-head");
    expect(heads.map((head) => head.dataset.state)).toEqual([
      "sleeping",
      "sleeping",
      "happy",
      "working",
    ]);
    expect(heads.every((head) => head.dataset.paused === "true")).toBe(true);
    expect(heads[0]).toHaveAttribute("data-size", "40");
    expect(document.querySelectorAll("img")).toHaveLength(0);
  });
});
