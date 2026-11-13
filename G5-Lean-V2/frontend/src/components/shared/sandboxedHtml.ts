import { lineFromFragment } from "../files/repoLinks";
import {
  QUESTION_CARD_RENDERER_ATTR,
  SANDBOXED_QUESTION_CARD_JS,
} from "./questionCardRenderer";

export const SANDBOXED_HTML_CSP =
  "default-src 'none'; style-src 'unsafe-inline'; img-src data:; font-src data:; " +
  "script-src 'unsafe-inline'; connect-src 'none'; form-action 'none'; base-uri 'none'";

const CSP_META = `<meta http-equiv="Content-Security-Policy" content="${SANDBOXED_HTML_CSP}">`;
// Leading whitespace is matched as the HTML parser sees it: only ASCII
// whitespace (tab, LF, FF, CR, space) is skipped ahead of a doctype. JS `\s`
// also matches NBSP, vertical tab and other Unicode spaces, which the parser
// treats as text; matching those would keep the doctype "first" while the
// injected policy meta landed after a text node, outside the head.
const LEADING_DOCTYPE_PATTERN = /^(\uFEFF?[\t\n\f\r ]*<!doctype[^>]*>)/i;

const THEME_PROP_PREFIX = "--g5-";

// In-document fragment navigation ([D64]): a srcdoc document's base URL is the
// embedding page, so an untouched fragment click navigates the sandboxed
// iframe to the platform origin (which refuses framing) instead of jumping to
// the section. Interception is scroll-only \u2014 never a `location` write, which
// would itself navigate to the embedding origin. Exported raw so tests can
// execute the exact shipped bytes against a DOM.
export const SANDBOXED_ANCHOR_NAV_JS = `(function () {
  function fragmentTarget(fragment) {
    if (!fragment) return null;
    var decoded = fragment;
    try {
      decoded = decodeURIComponent(fragment);
    } catch (error) {
      // Undecodable fragment: match the raw text.
    }
    return (
      document.getElementById(decoded) ||
      document.getElementsByName(decoded)[0] ||
      null
    );
  }
  function isSelfReference(href, urlPart) {
    // Document-relative reference (no scheme, no authority, not
    // root-relative): the standalone-authored "page.html#section" idiom,
    // which inside the sandbox could only resolve into the refusing
    // embedding origin anyway.
    if (
      urlPart.charAt(0) !== "/" &&
      !/^[a-zA-Z][a-zA-Z0-9+.-]*:/.test(urlPart)
    ) {
      return true;
    }
    // Absolute self-link: resolves (minus fragment) to the document's base
    // URL (for a srcdoc document, the embedding page URL) minus fragment.
    try {
      var resolved = new URL(href, document.baseURI);
      var base = new URL(document.baseURI);
      resolved.hash = "";
      base.hash = "";
      return resolved.href === base.href;
    } catch (error) {
      return false;
    }
  }
  document.addEventListener("click", function (event) {
    if (event.defaultPrevented || event.button !== 0) return;
    if (event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    var anchor =
      event.target && event.target.closest
        ? event.target.closest("a[href], area[href]")
        : null;
    if (!anchor) return;
    var href = anchor.getAttribute("href") || "";
    var hashIndex = href.indexOf("#");
    if (hashIndex < 0) return;
    var fragment = href.slice(hashIndex + 1);
    var target = fragmentTarget(fragment);
    if (hashIndex > 0) {
      // URL part + fragment: only an actual self-reference whose section
      // exists here is treated as in-doc; genuinely external links keep
      // default behavior even when a local id collides with their fragment,
      // as do links requesting another browsing context (target keywords are
      // case-insensitive). Fragment-only hrefs skip both checks ([D64]):
      // without allow-popups a foreign context is a guaranteed-dead click,
      // and "#frag" in another context is self-referential anyway.
      var browsingContext = anchor.getAttribute("target");
      if (
        !target ||
        (browsingContext && browsingContext.toLowerCase() !== "_self")
      ) {
        return;
      }
      if (!isSelfReference(href, href.slice(0, hashIndex))) return;
    }
    event.preventDefault();
    if (target) {
      if (target.scrollIntoView) target.scrollIntoView({ block: "start" });
    } else if (fragment === "" || fragment.toLowerCase() === "top") {
      window.scrollTo(0, 0);
    }
  });
})();`;

const ANCHOR_NAV_SCRIPT = `<script data-g5-anchor-nav>${SANDBOXED_ANCHOR_NAV_JS}</script>`;

/** Longest accepted workspace file path in a `g5://file/` deep link. */
export const MAX_WORKSPACE_FILE_PATH_CHARS = 400;

/**
 * Sanitize the repo-relative path of a `g5://file/<path>` deep link.
 *
 * Accepts forward-slash relative paths in the conservative repo charset;
 * rejects control characters, backslashes, absolute paths, empty/`.`/`..`
 * segments, and over-long paths. Returns null when the path fails — the
 * caller drops the proposal rather than guessing.
 */
export function sanitizeWorkspaceFilePath(raw: string): string | null {
  const stripped = raw
    // eslint-disable-next-line no-control-regex -- stripping control chars is the point
    .replace(/[\u0000-\u001f\u007f]/g, "");
  if (
    stripped.length === 0 ||
    stripped.length > MAX_WORKSPACE_FILE_PATH_CHARS
  ) {
    return null;
  }
  if (stripped.includes("\\")) return null;
  if (stripped.startsWith("/")) return null;
  if (!/^[A-Za-z0-9._\-/ ]+$/.test(stripped)) return null;
  const segments = stripped.split("/");
  for (const segment of segments) {
    if (segment === "" || segment === "." || segment === "..") return null;
  }
  return stripped;
}

/** Longest accepted `?repo=` ownership qualifier in a `g5://file/` deep link. */
export const MAX_WORKSPACE_REPO_REF_CHARS = 100;

/**
 * Sanitize the `?repo=<repoId-or-repoName>` ownership qualifier of a
 * `g5://file/` deep link (center-panel D73, issue #1970).
 *
 * The value is a lossy transport spelling of a workspace repo identity —
 * decoded against the workspace inventory later, never trusted as a route by
 * itself. Accepts the conservative single-segment repo charset; returns null
 * otherwise, and the caller drops the WHOLE proposal — a link that tried to
 * name an owner is never silently reinterpreted as ownerless.
 */
export function sanitizeWorkspaceRepoRef(raw: string): string | null {
  const stripped = raw
    // eslint-disable-next-line no-control-regex -- stripping control chars is the point
    .replace(/[\u0000-\u001f\u007f]/g, "");
  if (stripped.length === 0 || stripped.length > MAX_WORKSPACE_REPO_REF_CHARS) {
    return null;
  }
  return /^[A-Za-z0-9._-]+$/.test(stripped) ? stripped : null;
}

/**
 * Parse a `g5://file/<path>[?repo=<value>][#L<n>[-L<m>]]` href into its
 * sanitized parts.
 *
 * Shared by the markdown deep-link delegate (the sandbox bridge carries the
 * parts as separate message fields instead). The query, when present, must be
 * exactly one `repo=<value>` pair; anything else invalidates the whole link.
 * A line fragment yields `line`; any other fragment is ignored, and no
 * fragment changes which repository or file the link names
 * (`g5.documents.file-links.line`). A `?` after the `#` is a misplaced query:
 * the link is malformed, so a misplaced `?repo=` is never dropped as part of
 * an ignored fragment.
 */
export function parseG5FileHref(
  href: string,
): { filePath: string; repoRef?: string; line?: number } | null {
  const prefix = "g5://file/";
  const trimmed = href.trim();
  if (!trimmed.startsWith(prefix)) return null;
  const hashIndex = trimmed.indexOf("#");
  const fragment = hashIndex >= 0 ? trimmed.slice(hashIndex + 1) : "";
  if (fragment.includes("?")) return null;
  const line = lineFromFragment(fragment);
  const remainder = trimmed.slice(
    prefix.length,
    hashIndex >= 0 ? hashIndex : undefined,
  );
  const queryIndex = remainder.indexOf("?");
  const rawPath = queryIndex >= 0 ? remainder.slice(0, queryIndex) : remainder;
  const rawQuery = queryIndex >= 0 ? remainder.slice(queryIndex + 1) : null;

  let decodedPath = rawPath;
  try {
    decodedPath = decodeURIComponent(rawPath);
  } catch {
    // Undecodable remainder: match the raw text (anchor-nav precedent).
  }
  const filePath = sanitizeWorkspaceFilePath(decodedPath);
  if (!filePath) return null;

  if (rawQuery === null) return line ? { filePath, line } : { filePath };
  if (!rawQuery.startsWith("repo=")) return null;
  let rawValue = rawQuery.slice("repo=".length);
  if (rawValue.includes("&") || rawValue.includes("=")) return null;
  try {
    rawValue = decodeURIComponent(rawValue);
  } catch {
    // Undecodable value: match the raw text.
  }
  const repoRef = sanitizeWorkspaceRepoRef(rawValue);
  if (!repoRef) return null;
  return line ? { filePath, repoRef, line } : { filePath, repoRef };
}

// Workspace-file deep links (same [D66]/[D68] model as ontology links): a
// `g5://file/<path>[?repo=<repoId-or-repoName>][#L<n>[-L<m>]]` href is
// consumed in the sandbox and the sanitized path plus optional ownership
// qualifier and cited line are PROPOSED to the host, which re-validates them
// and resolves ownership (center-panel D73) into the citation file-open
// pathway. A present-but-malformed query, including one after the `#`, drops
// the whole proposal — a link that tried to name an owner is never
// reinterpreted as ownerless. In-page sanitization mirrors
// parseG5FileHref exactly (in-page literal copy; the script cannot import).
// Exported raw so tests can execute the exact shipped bytes.
export const SANDBOXED_FILE_NAV_JS = `(function () {
  var PREFIX = "g5://file/";
  document.addEventListener(
    "click",
    function (event) {
      if (event.defaultPrevented || event.button !== 0) return;
      if (event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
      var anchor =
        event.target && event.target.closest
          ? event.target.closest("a[href]")
          : null;
      if (!anchor) return;
      // Trim BEFORE the scheme check (mirroring parseG5FileHref): browsers
      // strip surrounding whitespace when resolving an href, so a padded
      // custom-scheme link would otherwise escape consume-and-propose and
      // reach the browser's navigation path.
      var href = (anchor.getAttribute("href") || "").replace(/^\\s+|\\s+$/g, "");
      if (href.slice(0, PREFIX.length) !== PREFIX) return;
      // The scheme can never render in the sandbox: consume the click even
      // when the path turns out invalid (drop silently, never navigate).
      event.preventDefault();
      var raw = href.slice(PREFIX.length);
      // A line fragment opens at that line; any other fragment is ignored.
      var line;
      var hashIndex = raw.indexOf("#");
      if (hashIndex >= 0) {
        var fragment = raw.slice(hashIndex + 1);
        if (fragment.indexOf("?") >= 0) return;
        var lineMatch = /^L(\\d+)(?:-L\\d+)?$/.exec(fragment);
        if (lineMatch) {
          var cited = Number(lineMatch[1]);
          if (Number.isSafeInteger(cited) && cited > 0) line = cited;
        }
        raw = raw.slice(0, hashIndex);
      }
      var query = null;
      var queryIndex = raw.indexOf("?");
      if (queryIndex >= 0) {
        query = raw.slice(queryIndex + 1);
        raw = raw.slice(0, queryIndex);
      }
      try {
        raw = decodeURIComponent(raw);
      } catch (error) {
        // Undecodable remainder: match the raw text (anchor-nav precedent).
      }
      raw = raw.replace(/[\\u0000-\\u001f\\u007f]/g, "");
      if (raw.length === 0 || raw.length > 400) return;
      if (raw.indexOf("\\\\") >= 0) return;
      if (raw.charAt(0) === "/") return;
      if (!/^[A-Za-z0-9._\\-/ ]+$/.test(raw)) return;
      var segments = raw.split("/");
      for (var i = 0; i < segments.length; i++) {
        if (segments[i] === "" || segments[i] === "." || segments[i] === "..") return;
      }
      var repoRef;
      if (query !== null) {
        if (query.slice(0, 5) !== "repo=") return;
        var value = query.slice(5);
        if (value.indexOf("&") >= 0 || value.indexOf("=") >= 0) return;
        try {
          value = decodeURIComponent(value);
        } catch (error) {
          // Undecodable value: match the raw text.
        }
        value = value.replace(/[\\u0000-\\u001f\\u007f]/g, "");
        if (value.length === 0 || value.length > 100) return;
        if (!/^[A-Za-z0-9._-]+$/.test(value)) return;
        repoRef = value;
      }
      parent.postMessage(
        {
          source: "g5-html-doc",
          type: "open-workspace-file",
          filePath: raw,
          repoRef: repoRef,
          line: line
        },
        "*"
      );
    },
    true
  );
})();`;

const FILE_NAV_SCRIPT = `<script data-g5-file-nav>${SANDBOXED_FILE_NAV_JS}</script>`;

/**
 * A policy for HTML shown through a blob URL of this origin (a review
 * artifact preview): scripts are forbidden outright, and nothing loads
 * from anywhere but the document itself. `withSandboxedHtmlCsp` is for
 * documents that only ever run inside a sandboxed frame and carries the
 * navigation scripts, which need `'unsafe-inline'`; a blob URL can be
 * opened as a top-level page, where the frame's sandbox no longer applies,
 * so the document must be inert by its own policy.
 */
export const SCRIPTLESS_PREVIEW_CSP =
  "default-src 'none'; img-src data: blob:; style-src 'unsafe-inline'; font-src data:; base-uri 'none'; form-action 'none'";

const SCRIPTLESS_PREVIEW_META = `<meta http-equiv="Content-Security-Policy" content="${SCRIPTLESS_PREVIEW_CSP}">`;

export function withScriptlessPreviewCsp(html: string): string {
  const doctypeMatch = html.match(LEADING_DOCTYPE_PATTERN);
  if (doctypeMatch?.[0]) {
    return `${doctypeMatch[0]}${SCRIPTLESS_PREVIEW_META}${html.slice(doctypeMatch[0].length)}`;
  }
  return `${SCRIPTLESS_PREVIEW_META}${html}`;
}

export function withSandboxedHtmlCsp(html: string): string {
  // Keep a leading doctype first so full documents stay in standards mode, then
  // inject the policy before attacker-authored executable/fetch-capable bytes.
  // The [D64] anchor-nav script and the [D66] ontology-nav script follow the
  // CSP meta (the theme block, when present, is spliced between the CSP and
  // the scripts) so they still precede authored bytes.
  const doctypeMatch = html.match(LEADING_DOCTYPE_PATTERN);
  if (doctypeMatch?.[0]) {
    return `${doctypeMatch[0]}${CSP_META}${ANCHOR_NAV_SCRIPT}${FILE_NAV_SCRIPT}${html.slice(doctypeMatch[0].length)}`;
  }

  // Without a leading doctype, keep the policy before attacker-authored bytes;
  // regex placement can be redirected by comments or decoy tags before the real
  // document head.
  return `${CSP_META}${ANCHOR_NAV_SCRIPT}${FILE_NAV_SCRIPT}${html}`;
}

interface StyleRuleLike {
  selectorText?: string;
  style?: CSSStyleDeclaration;
  cssRules?: CSSRuleList;
  conditionText?: string;
  type?: number;
}

function collectDeclsFromRules(
  rules: ArrayLike<CSSRule>,
  root: Element,
  decls: Map<string, string>,
): void {
  for (const rule of Array.from(rules)) {
    const candidate = rule as StyleRuleLike;
    if (candidate.selectorText && candidate.style) {
      let matches = false;
      try {
        matches = root.matches(candidate.selectorText);
      } catch {
        continue;
      }
      if (!matches) continue;
      const style = candidate.style;
      for (let i = 0; i < style.length; i++) {
        // Indexed access instead of .item(): identical per spec, and jsdom's
        // CSSStyleDeclaration only implements the indexed form.
        const name = style[i];
        if (name.startsWith(THEME_PROP_PREFIX) || name === "color-scheme") {
          decls.set(name, style.getPropertyValue(name).trim());
        }
      }
    } else if (candidate.cssRules) {
      // Grouping rules (@layer, @supports, @media). Skip media groups whose
      // condition does not currently apply, and @supports branches the
      // browser reports as unsupported.
      if (
        candidate.type === CSSRule.MEDIA_RULE &&
        candidate.conditionText &&
        typeof window !== "undefined" &&
        typeof window.matchMedia === "function" &&
        !window.matchMedia(candidate.conditionText).matches
      ) {
        continue;
      }
      if (
        candidate.type === CSSRule.SUPPORTS_RULE &&
        candidate.conditionText &&
        typeof CSS !== "undefined" &&
        typeof CSS.supports === "function" &&
        !CSS.supports(candidate.conditionText)
      ) {
        continue;
      }
      collectDeclsFromRules(candidate.cssRules, root, decls);
    }
  }
}

// Resolve the app's active theme (its `--g5-*` custom properties and
// color-scheme) by walking same-origin stylesheets and keeping, in document
// order, every declaration whose selector matches the current
// `<html data-theme data-mode>` element. Later matches win, mirroring the
// cascade for the equal-specificity palette selectors in index.css.
export function collectPlatformThemeCss(): string {
  if (typeof document === "undefined" || !document.documentElement) return "";
  const root = document.documentElement;
  const decls = new Map<string, string>();
  for (const sheet of Array.from(document.styleSheets)) {
    let rules: CSSRuleList;
    try {
      rules = sheet.cssRules;
    } catch {
      // Cross-origin stylesheet — not ours, skip.
      continue;
    }
    collectDeclsFromRules(rules, root, decls);
  }
  if (decls.size === 0) return "";
  const body = Array.from(decls, ([name, value]) => `${name}: ${value};`).join(
    " ",
  );
  return `:root { ${body} }`;
}

// CSP first, then the platform theme tokens, both ahead of authored bytes. The
// injected block only declares custom properties (plus color-scheme), so a
// document that never references var(--g5-...) renders unchanged, and a
// document's own later :root declarations override the injection.
export function withSandboxedPlatformTheme(
  html: string,
  themeCss: string = collectPlatformThemeCss(),
): string {
  const withCsp = withSandboxedHtmlCsp(html);
  if (!themeCss) return withCsp;
  return withCsp.replace(
    CSP_META,
    `${CSP_META}<style data-g5-platform-theme>${themeCss}</style>`,
  );
}

const QUESTION_CARD_RENDERER_SCRIPT = `<script ${QUESTION_CARD_RENDERER_ATTR}>${SANDBOXED_QUESTION_CARD_JS}</script>`;
const QUESTION_PLACEHOLDER_PRESENT = /data-g5-question\s*=/i;

/** True when the authored HTML carries a `data-g5-question` placeholder. */
export function hasQuestionPlaceholders(html: string): boolean {
  return QUESTION_PLACEHOLDER_PRESENT.test(html);
}

/**
 * Host-supplied question cards (`g5.documents.question.html`): given a
 * document composed by `withSandboxedPlatformTheme`, splice the card renderer
 * script immediately before the injected anchor-nav script — hence after the
 * CSP and theme and before every authored byte — only when the authored HTML
 * holds a placeholder. Card data arrives later by message, so the composed
 * document does not change when an answer does. A document without
 * placeholders is returned byte-identical.
 */
export function withSandboxedQuestionCards(composedHtml: string): string {
  if (!hasQuestionPlaceholders(composedHtml)) return composedHtml;
  const navIndex = composedHtml.indexOf(ANCHOR_NAV_SCRIPT);
  if (navIndex < 0) return composedHtml;
  return (
    composedHtml.slice(0, navIndex) +
    QUESTION_CARD_RENDERER_SCRIPT +
    composedHtml.slice(navIndex)
  );
}
