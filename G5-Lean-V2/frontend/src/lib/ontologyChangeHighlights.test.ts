import { describe, expect, it } from "vitest";
import {
  changeHighlights,
  MAX_HIGHLIGHT_WORDS,
  type HighlightRange,
} from "./ontologyChangeHighlights";

/** Each block with its highlighted ranges in [brackets]. */
function marked(blocks: string[], ranges: HighlightRange[]) {
  return blocks.map((text, block) => {
    let out = "",
      at = 0;
    for (const r of ranges
      .filter((r) => r.block === block)
      .sort((a, b) => a.start - b.start)) {
      out += `${text.slice(at, r.start)}[${text.slice(r.start, r.end)}]`;
      at = r.end;
    }
    return out + text.slice(at);
  });
}

function show(before: string[], current: string[]) {
  const result = changeHighlights(before, current);
  if (!result) return null;
  return {
    before: marked(before, result.before),
    current: marked(current, result.current),
  };
}

describe("changeHighlights (g5.documents.ontology.change-highlights)", () => {
  it("marks a replaced sentence whole and only the added words of an edited one", () => {
    expect(
      show(
        [
          "Agents push to the workspace branch. Each push needs a fresh lease from the server. The server rejects stale leases.",
        ],
        [
          "Agents push to the workspace branch. Only the owner may force-push to the branch. The server rejects stale leases with a clear error.",
        ],
      ),
    ).toEqual({
      before: [
        "Agents push to the workspace branch. [Each push needs a fresh lease from the server.] The server rejects stale leases.",
      ],
      current: [
        "Agents push to the workspace branch. [Only the owner may force-push to the branch.] The server rejects stale leases [with a clear error].",
      ],
    });
  });

  it("marks removed and changed words in the Before version", () => {
    expect(
      show(
        ["Keep the first sentence. Retry three times before failing."],
        ["Keep the first sentence. Retry five times before failing."],
      ),
    ).toEqual({
      before: ["Keep the first sentence. Retry [three] times before failing."],
      current: ["Keep the first sentence. Retry [five] times before failing."],
    });
  });

  it("marks wholly added and removed sentences", () => {
    expect(
      show(
        ["One rule applies. A second rule applied once. A third rule stays."],
        ["One rule applies. A third rule stays. Owners may waive it."],
      ),
    ).toEqual({
      before: [
        "One rule applies. [A second rule applied once.] A third rule stays.",
      ],
      current: ["One rule applies. A third rule stays. [Owners may waive it.]"],
    });
  });

  it("marks a heavily reworded sentence whole instead of in scattered words", () => {
    expect(
      show(
        [
          "Keep this sentence as it is. Another sentence stays the same here too. Show a question answer's acceptance and its delivery separately while the answer travels to the saved session.",
        ],
        [
          "Keep this sentence as it is. Another sentence stays the same here too. Show the owner's answer to a native question as accepted separately from delivered to the session.",
        ],
      ),
    ).toEqual({
      before: [
        "Keep this sentence as it is. Another sentence stays the same here too. [Show a question answer's acceptance and its delivery separately while the answer travels to the saved session.]",
      ],
      current: [
        "Keep this sentence as it is. Another sentence stays the same here too. [Show the owner's answer to a native question as accepted separately from delivered to the session.]",
      ],
    });
  });

  it("shows no highlights when the text a reader sees is identical", () => {
    expect(
      changeHighlights(
        ["Keep it.   Wrapped\nacross lines."],
        ["Keep it. Wrapped across lines."],
      ),
    ).toBeNull();
    expect(changeHighlights(["Same text."], ["Same text."])).toBeNull();
  });

  it("marks only a clause inserted into a sentence, even one that doubles it", () => {
    expect(
      show(
        [
          "Streams stop after deactivation. Recheck activity at each protected input or output batch, discard unsent data on denial, and close the stream.",
        ],
        [
          "Streams stop after deactivation. Recheck activity at each protected input or output batch (browser delivery: per batch of at most 256 queued events, at least once a second), discard unsent data on denial, and close the stream.",
        ],
      ),
    ).toEqual({
      before: [
        "Streams stop after deactivation. Recheck activity at each protected input or output batch, discard unsent data on denial, and close the stream.",
      ],
      current: [
        "Streams stop after deactivation. Recheck activity at each protected input or output batch [(browser delivery: per batch of at most 256 queued events, at least once a second)], discard unsent data on denial, and close the stream.",
      ],
    });
  });

  it("highlights added sentences when a statement mostly grew", () => {
    // Kept text is measured against the shorter version, so a statement
    // extended to several times its length still shows what was added.
    expect(
      show(
        ["External synchronization binds a repository to a provider branch."],
        [
          "External synchronization binds a repository to a provider branch. The default branch is reserved for Base. A writable destination cannot be shared by different workspaces. Destinations resolve by the provider repository's canonical identity and full branch name, never by display names.",
        ],
      ),
    ).toEqual({
      before: [
        "External synchronization binds a repository to a provider branch.",
      ],
      current: [
        "External synchronization binds a repository to a provider branch. [The default branch is reserved for Base.] [A writable destination cannot be shared by different workspaces.] [Destinations resolve by the provider repository's canonical identity and full branch name, never by display names.]",
      ],
    });
  });

  it("falls back when too little of the statement survives", () => {
    expect(
      changeHighlights(
        ["The cache stores builds. Entries expire daily."],
        ["Deployments need two approvals from owners."],
      ),
    ).toBeNull();
  });

  it("falls back above the word limit", () => {
    const long = Array.from(
      { length: MAX_HIGHLIGHT_WORDS / 4 + 1 },
      (_, i) => `Rule ${i} holds here.`,
    ).join(" ");
    expect(changeHighlights([long], [`${long} One more.`])).toBeNull();
  });

  it("falls back when punctuation makes a statement under the word limit too large to compare", () => {
    // Code-like text: 1,000 words but 11,000 tokens, edited at both ends, so
    // its comparison table would have about 121 million cells (2 s, 484 MB).
    const code = Array.from(
      { length: 1_000 },
      (_, i) => `w${i % 50}${"(".repeat(10)}`,
    ).join(" ");
    expect(changeHighlights([`x ${code} y`], [`z ${code} q`])).toBeNull();
  });

  it("still compares one long sentence at the word limit", () => {
    const words = Array.from(
      { length: MAX_HIGHLIGHT_WORDS - 2 },
      (_, i) => `w${i}`,
    ).join(" ");
    expect(
      show([`Old ${words} end`], [`New ${words} stop`])?.current[0],
    ).toMatch(/^\[New\] .* \[stop\]$/);
  });

  it("ends a sentence at each block boundary", () => {
    expect(
      show(
        ["Items:", "first item", "second item"],
        ["Items:", "first item", "second changed item"],
      ),
    ).toEqual({
      before: ["Items:", "first item", "second item"],
      current: ["Items:", "first item", "second [changed] item"],
    });
  });

  it("does not treat a sentence moved across a kept one as kept", () => {
    expect(
      show(
        ["Alpha is first. Beta is second. Gamma is third."],
        ["Alpha is first. Gamma is third. Beta is second."],
      ),
    ).toEqual({
      before: ["Alpha is first. [Beta is second.] Gamma is third."],
      current: ["Alpha is first. Gamma is third. [Beta is second.]"],
    });
  });

  it("segments words in text without spaces", () => {
    const result = show(
      ["日本語の文です。二つ目の文です。"],
      ["日本語の文です。三つ目の文です。"],
    );
    expect(result?.before[0]).toMatch(/^日本語の文です。/);
    expect(result?.current[0]).toContain("[");
    expect(result?.current[0]).not.toContain("[日本語");
  });

  it("excerpts only the Before sentences that lost text, marking where others were left out", () => {
    const excerpt = (before: string[], current: string[]) => {
      const result = changeHighlights(before, current)?.beforeExcerpt;
      if (!result) return result;
      return (
        result.sentences
          .map(
            (s, i) =>
              (s.gapBefore ? "… " : i ? "" : "") +
              s.parts.map((p) => (p.removed ? `[${p.text}]` : p.text)).join(""),
          )
          .join(" ") + (result.gapAfter ? " …" : "")
      );
    };
    // A removed sentence and removed words, apart and across blocks, with
    // line wrapping collapsed.
    expect(
      excerpt(
        [
          "Agents push to the workspace branch. Each push\nneeds a fresh lease. The server rejects stale leases.",
          "Retry three times before failing. Then report.",
        ],
        [
          "Agents push to the workspace branch. The server rejects stale leases.",
          "Retry before failing. Then report.",
        ],
      ),
    ).toBe(
      "… [Each push needs a fresh lease.] … Retry [three times] before failing. …",
    );
    // Adjacent sentences need no gap between them.
    expect(
      excerpt(
        ["Keep this one. Drop the second sentence. Trim the old third one."],
        ["Keep this one. Trim the third one."],
      ),
    ).toBe("… [Drop the second sentence.] Trim the [old] third one.");
    // Nothing removed: no Before excerpt.
    expect(
      excerpt(
        ["Keep the first sentence. Then fail."],
        ["Keep the first sentence. Then fail loudly."],
      ),
    ).toBeNull();
    // Every sentence lost text: the whole Before version shows instead.
    expect(
      excerpt(
        ["Retry three times before failing."],
        ["Retry times before failing."],
      ),
    ).toBeNull();
  });

  it("is deterministic", () => {
    const before = ["A b c. D e f. G h i."];
    const current = ["A b x. D e f. G y i."];
    expect(changeHighlights(before, current)).toEqual(
      changeHighlights(before, current),
    );
  });
});
