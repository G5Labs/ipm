/**
 * Whether a path sits under a managed explainer root (core D25 mirror of the
 * server-side `is_managed_explainer_path`): the workspace Project-Docs
 * subfolder `docs/<segment>/explainers/` for generated documents, or the legacy
 * top-level `explainers/` root for pre-cutover documents.
 */
export function isManagedExplainerPath(path: string): boolean {
  // No component may be empty, `.`, or `..` — mirrors the Rust predicate's
  // traversal rejection (review finding 2026-08-10).
  const componentsAreClean = (value: string): boolean =>
    value
      .split("/")
      .every(
        (segment) => segment !== "" && segment !== "." && segment !== "..",
      );
  if (path.startsWith("explainers/")) {
    const rest = path.slice("explainers/".length);
    return rest.length > 0 && componentsAreClean(rest);
  }
  if (!path.startsWith("docs/")) return false;
  const rest = path.slice("docs/".length);
  const slash = rest.indexOf("/");
  if (slash <= 0) return false;
  const folder = rest.slice(0, slash);
  if (folder === "." || folder === "..") return false;
  const tail = rest.slice(slash + 1);
  if (!tail.startsWith("explainers/")) return false;
  const name = tail.slice("explainers/".length);
  return name.length > 0 && componentsAreClean(name);
}
