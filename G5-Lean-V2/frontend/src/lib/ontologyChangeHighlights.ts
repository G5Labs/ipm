/**
 * What changed inside a modified statement (g5.documents.ontology.change-highlights).
 *
 * Both versions arrive as the rendered text a reader sees, one string per
 * block. Sentences are aligned first; a removed and an added sentence that
 * still share most of their words are a rewritten pair, inside which only the
 * changed words are marked, unless the sentence was replaced or its changes
 * would scatter into fragments, when the pair is clearer marked whole. Other
 * unmatched sentences are marked whole.
 * `null` means show the pair without highlights: the text is identical, too
 * little of it survives for highlights to help, or it is too large to compare.
 * The Before sentences that lost text also come as an excerpt, so the pair
 * can show only those.
 */

/** Characters `[start, end)` of one block's text to highlight. */
export interface HighlightRange {
  block: number;
  start: number;
  end: number;
}

/** One Before sentence in an excerpt, as kept and removed text. */
export interface ExcerptSentence {
  /** Sentences were left out between this one and the one before it, or,
   * for the first, before it. */
  gapBefore: boolean;
  parts: { text: string; removed: boolean }[];
}

export interface BeforeExcerpt {
  sentences: ExcerptSentence[];
  /** Sentences were left out after the last one. */
  gapAfter: boolean;
}

export interface ChangeHighlights {
  /** Removed text, in the Before version. */
  before: HighlightRange[];
  /** Added text, in the Current version. */
  current: HighlightRange[];
  /** The Before sentences that lost text, in order; `null` when none did or
   * none would be left out. */
  beforeExcerpt: BeforeExcerpt | null;
}

/** Larger versions show without highlights (the ontology's operating limit). */
export const MAX_HIGHLIGHT_WORDS = 2_000;
/** Comparison work allowed per statement: alignment table cells plus words
 * compared for overlap. The word limit alone does not bound it, because
 * punctuation is compared too, so punctuation-heavy text under that limit
 * could block the page. One 2,000-word sentence without punctuation fits. */
export const MAX_COMPARISON_WORK = 5_000_000;
/** A removed and an added sentence sharing at least this word overlap (Dice)
 * are one rewritten sentence. Provisional; see the design plan. */
export const REWRITE_OVERLAP = 0.5;
/** Show no highlights when fewer than this share of the shorter version's
 * words are kept: the text was replaced rather than extended or trimmed.
 * See the design plan. */
export const MIN_KEPT_SHARE = 1 / 3;
/** A rewritten sentence keeping, in order, fewer than this share of its
 * shorter version's words is marked whole: it was replaced, not edited. */
export const MIN_SENTENCE_KEPT_SHARE = 0.5;
/** Each change run beyond the first two needs this many kept words, or the
 * sentence is marked whole rather than in scattered fragments. An inserted
 * clause or one replaced word is a single run, however long. */
export const KEPT_WORDS_PER_EXTRA_RUN = 4;

interface Token {
  start: number;
  end: number;
  text: string;
  word: boolean;
}

interface Sentence {
  block: number;
  start: number;
  end: number;
  /** Whitespace-normalized text, the alignment key. */
  key: string;
  tokens: Token[];
  /** Lower-cased word tokens, for overlap. */
  words: string[];
}

/** Work left for one comparison. */
interface Budget {
  left: number;
}

/** False once the comparison has used up its work. */
function spend(budget: Budget, work: number) {
  budget.left -= work;
  return budget.left >= 0;
}

let sentenceSegmenter: Intl.Segmenter | undefined;
let wordSegmenter: Intl.Segmenter | undefined;

function segmenters() {
  if (typeof Intl === "undefined" || !("Segmenter" in Intl)) return null;
  sentenceSegmenter ??= new Intl.Segmenter(undefined, {
    granularity: "sentence",
  });
  wordSegmenter ??= new Intl.Segmenter(undefined, { granularity: "word" });
  return { sentences: sentenceSegmenter, words: wordSegmenter };
}

function sentencesOf(
  blocks: string[],
  seg: NonNullable<ReturnType<typeof segmenters>>,
): Sentence[] {
  const out: Sentence[] = [];
  blocks.forEach((text, block) => {
    // Rendered text keeps soft line breaks, which the segmenter would take
    // for sentence ends; a same-length copy keeps every offset.
    for (const s of seg.sentences.segment(text.replace(/\s/g, " "))) {
      const tokens: Token[] = [];
      for (const w of seg.words.segment(s.segment)) {
        if (!w.segment.trim()) continue;
        const start = s.index + w.index;
        tokens.push({
          start,
          end: start + w.segment.length,
          text: w.segment,
          word: w.isWordLike ?? /\w/.test(w.segment),
        });
      }
      if (!tokens.length) continue;
      out.push({
        block,
        start: tokens[0].start,
        end: tokens[tokens.length - 1].end,
        key: tokens.map((t) => t.text).join(" "),
        tokens,
        words: tokens.filter((t) => t.word).map((t) => t.text.toLowerCase()),
      });
    }
  });
  return out;
}

/** Index pairs of a longest common subsequence, in order; `null` when its
 * table would exceed the budget. */
function lcs<T>(
  a: T[],
  b: T[],
  same: (x: T, y: T) => boolean,
  budget: Budget,
): [number, number][] | null {
  let lo = 0;
  while (lo < a.length && lo < b.length && same(a[lo], b[lo])) lo++;
  let hiA = a.length,
    hiB = b.length;
  while (hiA > lo && hiB > lo && same(a[hiA - 1], b[hiB - 1])) {
    hiA--;
    hiB--;
  }
  const n = hiA - lo,
    m = hiB - lo;
  if (!spend(budget, n * m)) return null;
  const pairs: [number, number][] = [];
  for (let i = 0; i < lo; i++) pairs.push([i, i]);
  if (n && m) {
    // table[i][j]: LCS length of a[lo+i..] and b[lo+j..].
    const w = m + 1;
    const table = new Uint32Array((n + 1) * w);
    for (let i = n - 1; i >= 0; i--)
      for (let j = m - 1; j >= 0; j--)
        table[i * w + j] = same(a[lo + i], b[lo + j])
          ? table[(i + 1) * w + j + 1] + 1
          : Math.max(table[(i + 1) * w + j], table[i * w + j + 1]);
    let i = 0,
      j = 0;
    while (i < n && j < m) {
      if (same(a[lo + i], b[lo + j])) {
        pairs.push([lo + i, lo + j]);
        i++;
        j++;
      } else if (table[(i + 1) * w + j] >= table[i * w + j + 1]) i++;
      else j++;
    }
  }
  for (let k = 0; k < a.length - hiA; k++) pairs.push([hiA + k, hiB + k]);
  return pairs;
}

function overlap(a: string[], b: string[]) {
  if (!a.length || !b.length) return 0;
  const counts = new Map<string, number>();
  for (const w of a) counts.set(w, (counts.get(w) ?? 0) + 1);
  let shared = 0;
  for (const w of b) {
    const c = counts.get(w);
    if (c) {
      shared++;
      counts.set(w, c - 1);
    }
  }
  return (2 * shared) / (a.length + b.length);
}

/** A sentence's text split at its removed ranges, whitespace collapsed. */
function excerptSentence(
  text: string,
  s: Sentence,
  removed: HighlightRange[],
  gapBefore: boolean,
): ExcerptSentence {
  const parts: ExcerptSentence["parts"] = [];
  const add = (start: number, end: number, isRemoved: boolean) => {
    if (end > start)
      parts.push({
        text: text.slice(start, end).replace(/\s+/g, " "),
        removed: isRemoved,
      });
  };
  let at = s.start;
  for (const r of removed) {
    add(at, r.start, false);
    add(r.start, r.end, true);
    at = r.end;
  }
  add(at, s.end, false);
  return { gapBefore, parts };
}

function whole(s: Sentence): HighlightRange {
  return { block: s.block, start: s.start, end: s.end };
}

/** Runs of unmatched tokens as ranges; a run spans the spaces inside it. */
function runs(s: Sentence, matched: Set<number>): HighlightRange[] {
  const out: HighlightRange[] = [];
  let open: HighlightRange | null = null;
  s.tokens.forEach((t, i) => {
    if (matched.has(i)) open = null;
    else if (open) open.end = t.end;
    else {
      open = { block: s.block, start: t.start, end: t.end };
      out.push(open);
    }
  });
  return out;
}

export function changeHighlights(
  before: string[],
  current: string[],
): ChangeHighlights | null {
  const seg = segmenters();
  if (!seg) return null;
  const old = sentencesOf(before, seg),
    now = sentencesOf(current, seg);
  const wordCount = (list: Sentence[]) =>
    list.reduce((n, s) => n + s.words.length, 0);
  const oldWords = wordCount(old),
    nowWords = wordCount(now);
  if (oldWords > MAX_HIGHLIGHT_WORDS || nowWords > MAX_HIGHLIGHT_WORDS)
    return null;
  if (old.length === now.length && old.every((s, i) => s.key === now[i].key))
    return null;

  const budget: Budget = { left: MAX_COMPARISON_WORK };
  const result: ChangeHighlights = {
    before: [],
    current: [],
    beforeExcerpt: null,
  };
  // Removed text by the Before sentence it is in.
  const removedIn = new Map<Sentence, HighlightRange[]>();
  const remove = (s: Sentence, ranges: HighlightRange[]) => {
    result.before.push(...ranges);
    removedIn.set(s, ranges);
  };
  let kept = 0;
  // Settle one gap of removed and added sentences between kept ones; false
  // when that runs out of budget.
  const settle = (removed: Sentence[], added: Sentence[]) => {
    let from = 0;
    const paired = new Set<number>();
    for (const r of removed) {
      let best = -1,
        bestScore = REWRITE_OVERLAP;
      for (let j = from; j < added.length; j++) {
        if (!spend(budget, r.words.length + added[j].words.length))
          return false;
        const score = overlap(r.words, added[j].words);
        if (score >= bestScore && (best < 0 || score > bestScore)) {
          best = j;
          bestScore = score;
        }
      }
      if (best < 0) {
        remove(r, [whole(r)]);
        continue;
      }
      const a = added[best];
      paired.add(best);
      from = best + 1;
      const tokenPairs = lcs(
        r.tokens,
        a.tokens,
        (x, y) => x.text === y.text,
        budget,
      );
      if (!tokenPairs) return false;
      const keptWords = tokenPairs.filter(([i]) => r.tokens[i].word).length;
      const removedRuns = runs(r, new Set(tokenPairs.map(([i]) => i)));
      const addedRuns = runs(a, new Set(tokenPairs.map(([, j]) => j)));
      const replaced =
        keptWords <
        Math.min(r.words.length, a.words.length) * MIN_SENTENCE_KEPT_SHARE;
      const scattered =
        Math.max(removedRuns.length, addedRuns.length) >
        2 + keptWords / KEPT_WORDS_PER_EXTRA_RUN;
      if (replaced || scattered) {
        remove(r, [whole(r)]);
        result.current.push(whole(a));
        continue;
      }
      kept += keptWords;
      if (removedRuns.length) remove(r, removedRuns);
      result.current.push(...addedRuns);
    }
    added.forEach((a, j) => {
      if (!paired.has(j)) result.current.push(whole(a));
    });
    return true;
  };

  const keptSentences = lcs(old, now, (x, y) => x.key === y.key, budget);
  if (!keptSentences) return null;
  let i = 0,
    j = 0;
  for (const [oi, ni] of [...keptSentences, [old.length, now.length]]) {
    if (!settle(old.slice(i, oi), now.slice(j, ni))) return null;
    if (oi < old.length) kept += old[oi].words.length;
    i = oi + 1;
    j = ni + 1;
  }
  if (kept < Math.min(oldWords, nowWords) * MIN_KEPT_SHARE) return null;
  if (!result.before.length && !result.current.length) return null;
  if (removedIn.size < old.length) {
    const sentences: ExcerptSentence[] = [];
    let last = -1;
    old.forEach((s, i) => {
      const removed = removedIn.get(s);
      if (!removed) return;
      sentences.push(
        excerptSentence(before[s.block], s, removed, i > last + 1),
      );
      last = i;
    });
    if (sentences.length)
      result.beforeExcerpt = { sentences, gapAfter: last < old.length - 1 };
  }
  return result;
}
