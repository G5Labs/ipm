// =============================================================================
// GitHub-rendered HTML
// =============================================================================
// Shows Markdown as GitHub rendered it (a PR's `body_html`, or G5 text through
// GitHub's Markdown API) so repository references such as `#769` link where
// GitHub links them (git g5.git.pr-description). GitHub sanitizes its output;
// it is sanitized again here with GitHub's own allowlist before React renders
// it. The safe link policy for externally authored content holds: links open
// outside the app and images are links, never loaded.
// =============================================================================

import { useMemo } from "react";
import { Fragment, jsx, jsxs } from "react/jsx-runtime";
import { fromHtml } from "hast-util-from-html";
import { defaultSchema, sanitize } from "hast-util-sanitize";
import { toJsxRuntime } from "hast-util-to-jsx-runtime";
import type { Root } from "hast";
import "github-markdown-css/github-markdown.css";

const schema = { ...defaultSchema, clobberPrefix: "user-content-" };

function ExternalAnchor(props: React.ComponentProps<"a">) {
  const { href, children, ...rest } = props;
  // Same-page footnote and heading anchors stay in the reader.
  if (href?.startsWith("#")) {
    return (
      <a href={href} {...rest}>
        {children}
      </a>
    );
  }
  return (
    <a {...rest} href={href} target="_blank" rel="noopener noreferrer">
      {children}
    </a>
  );
}

function ImageLink(props: React.ComponentProps<"img">) {
  const { src, alt } = props;
  if (typeof src !== "string" || !src) return <>{alt ?? ""}</>;
  return (
    <a href={src} target="_blank" rel="noopener noreferrer">
      {alt || "Image"}
    </a>
  );
}

export function GithubRenderedHtml({
  html,
  testId,
}: {
  html: string;
  testId?: string;
}) {
  const content = useMemo(() => {
    const tree = sanitize(fromHtml(html, { fragment: true }), schema) as Root;
    return toJsxRuntime(tree, {
      Fragment,
      jsx,
      jsxs,
      components: { a: ExternalAnchor, img: ImageLink },
    });
  }, [html]);
  return (
    <div
      data-testid={testId}
      className="markdown-body g5-markdown-preview g5-github-html"
    >
      {content}
    </div>
  );
}
