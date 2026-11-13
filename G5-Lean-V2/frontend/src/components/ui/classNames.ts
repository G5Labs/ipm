// =============================================================================
// G5 Design System — classNames
// =============================================================================
// Leaf module so both UI.tsx and Modal.tsx can import it without forming a
// module cycle (UI.tsx re-exports Modal; Modal must not import from UI.tsx).
// =============================================================================

/**
 * Concatenates class name strings, filtering out all falsy values
 * (false, null, undefined, empty string '').
 *
 * @example
 * classNames('foo', false, 'bar', undefined, 'baz')
 * // => 'foo bar baz'
 */
export function classNames(
  ...args: (string | false | null | undefined)[]
): string {
  return args.filter(Boolean).join(" ");
}
