import { act, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { useRef } from "react";
import type { DiffHunk } from "../../lib/types";
import {
  SourceControlDiffBody,
  SourceControlDiffViewport,
} from "./SourceControlDiffBody";

let observers: TestObserver[];
const originalIntersectionObserver = globalThis.IntersectionObserver;
class TestObserver {
  targets = new Set<Element>();
  constructor(
    public callback: IntersectionObserverCallback,
    public options: IntersectionObserverInit,
  ) {
    observers.push(this);
  }
  observe = vi.fn((element: Element) => this.targets.add(element));
  unobserve = vi.fn((element: Element) => this.targets.delete(element));
  disconnect = vi.fn(() => this.targets.clear());
  intersect(element: Element, visible: boolean, height = 74) {
    act(() =>
      this.callback(
        [
          {
            target: element,
            isIntersecting: visible,
            boundingClientRect: { height },
          } as IntersectionObserverEntry,
        ],
        this as unknown as IntersectionObserver,
      ),
    );
  }
}
beforeEach(() => {
  observers = [];
  globalThis.IntersectionObserver =
    TestObserver as unknown as typeof IntersectionObserver;
});
afterEach(() => {
  globalThis.IntersectionObserver = originalIntersectionObserver;
});
const hunks: DiffHunk[] = [
  {
    startLine: 1,
    header: "@@ -1 +1 @@",
    lines: [
      { type: "deletion", oldLine: 1, content: "previous" },
      { type: "addition", newLine: 1, content: "current" },
    ],
  },
];
function Harness({
  value = hunks,
  second = false,
}: {
  value?: DiffHunk[];
  second?: boolean;
}) {
  const root = useRef<HTMLDivElement>(null);
  return (
    <div ref={root} data-testid="viewport">
      <SourceControlDiffViewport root={root}>
        <SourceControlDiffBody hunks={value} additions={1} deletions={1} />
        {second && (
          <SourceControlDiffBody hunks={hunks} additions={1} deletions={1} />
        )}
      </SourceControlDiffViewport>
    </div>
  );
}

it("shares one pane observer, renders nearby lines and releases distant lines with measured height", () => {
  const mounted = render(<Harness second />);
  expect(observers).toHaveLength(1);
  const observer = observers[0];
  expect(observer.options).toEqual({
    root: screen.getByTestId("viewport"),
    rootMargin: "600px 0px",
  });
  const [first, second] = screen.getAllByTestId("source-control-diff-body");
  expect(observer.targets.size).toBe(2);
  expect(first).toHaveStyle({ height: "74px" });
  expect(screen.queryAllByTestId("diff-line-code")).toHaveLength(0);
  observer.intersect(first, true);
  expect(
    screen.getAllByTestId("diff-line-code").map((line) => line.textContent),
  ).toEqual(["previous", "current"]);
  expect(second).toHaveAttribute("data-rendered", "false");
  observer.intersect(first, false, 81);
  expect(first).toHaveStyle({ height: "81px" });
  expect(screen.queryAllByTestId("diff-line-code")).toHaveLength(0);
  observer.intersect(second, true);
  expect(screen.getAllByTestId("diff-line-code")).toHaveLength(2);
  mounted.unmount();
  expect(observer.disconnect).toHaveBeenCalledTimes(1);
  expect(observer.targets.size).toBe(0);
  observer.intersect(first, true);
  expect(screen.queryAllByTestId("diff-line-code")).toHaveLength(0);
});

it("reuses unchanged hunk bodies from a new response and updates changed bytes", async () => {
  const renderer = vi.spyOn(
    await import("../editor/DiffHunksView"),
    "DiffHunksView",
  );
  try {
    const mounted = render(<Harness />);
    const body = screen.getByTestId("source-control-diff-body");
    observers[0].intersect(body, true);
    const calls = renderer.mock.calls.length;
    expect(calls).toBeGreaterThan(0);
    mounted.rerender(<Harness value={structuredClone(hunks)} />);
    expect(renderer).toHaveBeenCalledTimes(calls);
    const changed = structuredClone(hunks);
    changed[0].lines[1].content = "UPDATED";
    mounted.rerender(<Harness value={changed} />);
    expect(screen.getByText("UPDATED")).toBeVisible();
    expect(screen.queryByText("current")).toBeNull();
    expect(renderer.mock.calls.length).toBeGreaterThan(calls);
  } finally {
    renderer.mockRestore();
  }
});

it("recalculates offscreen height when a comparison adds lines", () => {
  const mounted = render(<Harness />);
  const changed = structuredClone(hunks);
  changed[0].lines.push({ type: "addition", newLine: 2, content: "extra" });
  mounted.rerender(<Harness value={changed} />);
  const body = screen.getByTestId("source-control-diff-body");
  expect(body).toHaveStyle({ height: "94px" });
  observers[0].intersect(body, true);
  expect(screen.getByText("extra")).toBeVisible();
});

it("preserves measured offscreen height when changed content has the same dimensions", () => {
  const mounted = render(<Harness />);
  const body = screen.getByTestId("source-control-diff-body");
  observers[0].intersect(body, true);
  observers[0].intersect(body, false, 81);
  const changed = structuredClone(hunks);
  changed[0].lines[1].content = "new content";
  mounted.rerender(<Harness value={changed} />);
  expect(body).toHaveStyle({ height: "81px" });
  observers[0].intersect(body, true);
  expect(screen.getByText("new content")).toBeVisible();
  expect(screen.queryByText("current")).toBeNull();
});

it("releases removed targets across repeated filtering without accumulating observers", () => {
  const mounted = render(<Harness second />);
  const observer = observers[0];
  const first = screen.getAllByTestId("source-control-diff-body")[0];
  for (let cycle = 0; cycle < 30; cycle++) {
    const removed = screen.getAllByTestId("source-control-diff-body")[1];
    mounted.rerender(<Harness value={structuredClone(hunks)} />);
    expect(observer.unobserve).toHaveBeenLastCalledWith(removed);
    expect(observer.targets).toEqual(new Set([first]));
    observer.intersect(removed, true);
    expect(screen.queryAllByTestId("diff-line-code")).toHaveLength(0);
    mounted.rerender(<Harness value={structuredClone(hunks)} second />);
    expect(observer.targets.size).toBe(2);
    expect(observers).toHaveLength(1);
  }
  mounted.unmount();
  expect(observer.targets.size).toBe(0);
  expect(observer.disconnect).toHaveBeenCalledTimes(1);
});

it("renders eagerly when IntersectionObserver is unavailable", () => {
  globalThis.IntersectionObserver =
    undefined as unknown as typeof IntersectionObserver;
  render(<Harness />);
  expect(screen.getByText("current")).toBeVisible();
  expect(screen.getByTestId("source-control-diff-body")).toHaveAttribute(
    "data-rendered",
    "true",
  );
});
