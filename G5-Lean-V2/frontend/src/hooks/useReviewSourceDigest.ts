import { useEffect, useRef, useState } from "react";
import { sha256Utf8 } from "../lib/commentAnchors";
import type { ReviewSourceDigest } from "../lib/reviewSuggestionEdits";

/** Content-bound review proof; late crypto can never describe a newer buffer. */
export function useReviewSourceDigest(
  key: string | null,
  content: string,
  enabled: boolean,
  loadedSource: boolean,
): ReviewSourceDigest | undefined {
  const [selected, setSelected] = useState<
    (ReviewSourceDigest & { key: string | null }) | null
  >(null);
  const loaded = useRef<typeof selected>(null);
  useEffect(() => {
    if (!enabled) return;
    const cached = loaded.current;
    if (cached?.key === key && cached.content === content) {
      setSelected(cached);
      return;
    }
    let cancelled = false;
    const compute = () => {
      void sha256Utf8(content)
        .then((digest) => {
          if (cancelled) return;
          const next = { key, content, digest };
          if (loadedSource) loaded.current = next;
          setSelected(next);
        })
        .catch(() => {
          if (!cancelled) setSelected(null);
        });
    };
    // Preserve the editor's loaded-buffer cache and dirty-buffer debounce.
    const timer = loadedSource ? undefined : setTimeout(compute, 250);
    if (loadedSource) compute();
    return () => {
      cancelled = true;
      if (timer !== undefined) clearTimeout(timer);
    };
  }, [key, content, enabled, loadedSource]);
  return enabled && selected?.key === key && selected.content === content
    ? selected
    : undefined;
}
