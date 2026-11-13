import {
  createContext,
  memo,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
  type ReactNode,
  type RefObject,
} from "react";
import {
  DiffHunksView,
  MAX_RENDERED_DIFF_LINES,
  type DiffHunksViewProps,
} from "../editor/DiffHunksView";

type Observe = (
  target: Element,
  listener: (entry: IntersectionObserverEntry) => void,
) => () => void;
const Viewport = createContext<Observe | null>(null);

/** One observer serves every text body in the existing diff scroll pane. */
export function SourceControlDiffViewport({
  root,
  children,
}: {
  root: RefObject<HTMLDivElement | null>;
  children: ReactNode;
}) {
  const targets = useRef(new Map<Element, Parameters<Observe>[1]>());
  const observer = useRef<IntersectionObserver | null>(null);
  const observe = useCallback<Observe>((target, listener) => {
    targets.current.set(target, listener);
    observer.current?.observe(target);
    return () => {
      targets.current.delete(target);
      observer.current?.unobserve(target);
    };
  }, []);
  useEffect(() => {
    if (typeof IntersectionObserver === "undefined") return;
    const current = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) targets.current.get(entry.target)?.(entry);
      },
      { root: root.current, rootMargin: "600px 0px" },
    );
    observer.current = current;
    for (const target of targets.current.keys()) current.observe(target);
    return () => {
      current.disconnect();
      observer.current = null;
    };
  }, [root]);
  return <Viewport.Provider value={observe}>{children}</Viewport.Provider>;
}

type Props = Pick<DiffHunksViewProps, "hunks" | "additions" | "deletions">;

function estimatedHeight(hunks: Props["hunks"]) {
  const lines = hunks.reduce((count, hunk) => count + hunk.lines.length, 0);
  // DiffHunksView uses 20px rows, 26px hunk headers and 8px hunk padding.
  return lines > MAX_RENDERED_DIFF_LINES ? 68 : lines * 20 + hunks.length * 34;
}

export const SourceControlDiffBody = memo(
  function SourceControlDiffBody(props: Props) {
    const observe = useContext(Viewport);
    const element = useRef<HTMLDivElement>(null);
    const estimate = estimatedHeight(props.hunks);
    const [window, setWindow] = useState(() => ({
      estimate,
      visible: !observe || typeof IntersectionObserver === "undefined",
      height: estimate,
    }));
    if (window.estimate !== estimate)
      setWindow({
        ...window,
        estimate,
        height: estimate,
      });
    const onIntersection = useCallback((entry: IntersectionObserverEntry) => {
      setWindow((previous) => {
        const height =
          previous.visible && !entry.isIntersecting
            ? Math.ceil(entry.boundingClientRect.height)
            : previous.height;
        return previous.visible === entry.isIntersecting &&
          previous.height === height
          ? previous
          : { ...previous, visible: entry.isIntersecting, height };
      });
    }, []);
    useEffect(() => {
      if (element.current && observe)
        return observe(element.current, onIntersection);
    }, [observe, onIntersection]);
    return (
      <div
        ref={element}
        data-testid="source-control-diff-body"
        data-rendered={window.visible}
        role="region"
        aria-label="File diff"
        // eslint-disable-next-line jsx-a11y/no-noninteractive-tabindex
        tabIndex={0}
        className="source-control-horizontal-diff"
        style={window.visible ? undefined : { height: window.height }}
      >
        {window.visible && <DiffHunksView {...props} />}
      </div>
    );
  },
  (before, after) =>
    before.additions === after.additions &&
    before.deletions === after.deletions &&
    (before.hunks === after.hunks ||
      JSON.stringify(before.hunks) === JSON.stringify(after.hunks)),
);
