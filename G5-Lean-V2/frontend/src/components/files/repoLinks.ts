// =============================================================================
// G5 Files — repoLinks
// =============================================================================
// Relative links and image sources in a FILES Markdown file, resolved to a
// path in the same repository, as GitHub resolves them
// (`g5.documents.files-view.preview`).
// =============================================================================

/** A repository path and optional line, resolved from a relative link. */
export interface RepoLinkTarget {
  path: string;
  line?: number;
}

const SCHEME = /^[a-zA-Z][a-zA-Z0-9+.-]*:/;

/**
 * The line a `#L<n>` or `#L<n>-L<m>` fragment cites, as GitHub writes them;
 * undefined for any other fragment. Relative links and `g5://file` links
 * share this grammar (`g5.documents.file-links.line`).
 */
export function lineFromFragment(fragment: string): number | undefined {
  const match = /^L(\d+)(?:-L\d+)?$/.exec(fragment);
  if (!match) return undefined;
  const line = Number(match[1]);
  return Number.isSafeInteger(line) && line > 0 ? line : undefined;
}

/**
 * Resolve a relative link or image source in `fromFile` to a repository
 * path. A leading `/` starts at the repository root, as on GitHub. Returns
 * null for external, protocol-relative and in-page links, and for paths
 * that climb above the root.
 */
export function resolveRepoLink(
  fromFile: string,
  href: string,
): RepoLinkTarget | null {
  if (!href || href.startsWith("#") || href.startsWith("//")) return null;
  if (SCHEME.test(href)) return null;
  const [beforeHash, hash = ""] = href.split("#", 2);
  const [rawPath] = beforeHash.split("?", 1);
  if (!rawPath) return null;
  let decoded: string;
  try {
    decoded = decodeURIComponent(rawPath);
  } catch {
    decoded = rawPath;
  }
  const parts = decoded.startsWith("/")
    ? []
    : fromFile.split("/").slice(0, -1).filter(Boolean);
  for (const segment of decoded.split("/")) {
    if (segment === "" || segment === ".") continue;
    if (segment === "..") {
      if (parts.length === 0) return null;
      parts.pop();
      continue;
    }
    parts.push(segment);
  }
  if (parts.length === 0) return null;
  const line = lineFromFragment(hash);
  return line ? { path: parts.join("/"), line } : { path: parts.join("/") };
}
