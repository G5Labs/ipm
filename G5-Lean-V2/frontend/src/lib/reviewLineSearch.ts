/** Exact line-pattern occurrences, using linear KMP instead of repeated slices. */
export function findLineMatches(
  lines: readonly string[],
  pattern: readonly string[],
): number[] {
  if (!pattern.length || pattern.length > lines.length) return [];
  const prefixes = new Uint32Array(pattern.length);
  for (let i = 1, matched = 0; i < pattern.length; i++) {
    while (matched > 0 && pattern[i] !== pattern[matched])
      matched = prefixes[matched - 1];
    if (pattern[i] === pattern[matched]) matched++;
    prefixes[i] = matched;
  }
  const positions: number[] = [];
  for (let i = 0, matched = 0; i < lines.length; i++) {
    while (matched > 0 && lines[i] !== pattern[matched])
      matched = prefixes[matched - 1];
    if (lines[i] === pattern[matched]) matched++;
    if (matched === pattern.length) {
      positions.push(i - matched + 1);
      matched = prefixes[matched - 1];
    }
  }
  return positions;
}

/** One published position, including insertion context, costs only its length. */
export function linePatternAt(
  lines: readonly string[],
  pattern: readonly string[],
  start: number,
): boolean {
  if (start < 0 || start + pattern.length > lines.length) return false;
  for (let i = 0; i < pattern.length; i++)
    if (lines[start + i] !== pattern[i]) return false;
  return true;
}
