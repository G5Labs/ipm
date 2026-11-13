import React, { createContext, useContext, useMemo } from "react";
import ReactMarkdown, {
  defaultUrlTransform,
  type Components,
} from "react-markdown";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import rehypeRaw from "rehype-raw";
import rehypeSanitize, { defaultSchema } from "rehype-sanitize";
import rehypeKatex from "rehype-katex";
import rehypeSlug from "rehype-slug";
import "github-markdown-css/github-markdown.css";
import "katex/dist/katex.min.css";
import type { PluggableList } from "unified";
import { ReactFlowDiagramWidget } from "../shared/ReactFlowDiagramWidget";
import { SandboxedJsWidgetFrame } from "../shared/SandboxedJsWidgetFrame";
import { QuestionCard } from "../shared/QuestionCard";
import { MermaidDiagram } from "../shared/MermaidDiagram";
import { CodeCopyButton } from "../shared/CodeCopyButton";
import { highlightCode } from "../../lib/utils";
import { useWorkspaceFileContent } from "../../hooks/useFiles";
import { fileBlob, useObjectUrl } from "./fileBytes";
import { imageMimeForPath } from "./fileText";
import { remarkG5Markdown } from "./remarkG5Markdown";
import { remarkGithubAlerts } from "./remarkGithubAlerts";
import { resolveRepoLink } from "./repoLinks";

export interface MarkdownPreviewProps {
  className?: string;
  trustedContent?: boolean;
  historical?: boolean;
  basePath?: string;
  /** Host decorations run after sanitization and never change the source. */
  plugins?: PluggableList;
  components?: Components;
  renderCode?: (
    language: string,
    text: string,
    offset: number,
  ) => React.ReactNode;
  /** The Markdown text to render (the saved file or an unsaved draft). */
  content: string;
  /** The file being shown, for resolving relative links and images. */
  filePath?: string;
  workspaceId?: string;
  repoId?: string;
  /**
   * A host's own rendering of a trusted image (such as a G5 issue's
   * attachment, read through the authenticated client); `null` leaves the
   * image to the default rendering.
   */
  renderImage?: (image: {
    src: string;
    alt: string;
    title?: string;
  }) => React.ReactNode | null;
  /**
   * Opens a repository file (and optionally a line) in a FILES tab; `from`
   * names the statement the link was followed from, for Back.
   */
  onOpenWorkspaceFile?: (
    filePath: string,
    repoRef?: string,
    line?: number,
    from?: string,
  ) => void;
}

/** The prefix GitHub gives every user-supplied id and name. */
const CLOBBER_PREFIX = "user-content-";

const defaultAttributes = defaultSchema.attributes ?? {};

/**
 * GitHub's sanitizer allowlist, plus the classes this renderer's own
 * plugins emit: math placeholders that KaTeX renders after sanitizing, and
 * alert boxes.
 */
const sanitizeSchema = {
  ...defaultSchema,
  clobberPrefix: CLOBBER_PREFIX,
  protocols: {
    ...defaultSchema.protocols,
    href: [...(defaultSchema.protocols?.href ?? []), "g5"],
  },
  attributes: {
    ...defaultAttributes,
    code: [
      ...(defaultAttributes.code ?? []).filter(
        (attribute) =>
          !(Array.isArray(attribute) && attribute[0] === "className"),
      ),
      ["className", /^language-./, "math-inline", "math-display"],
    ],
    div: [
      ...(defaultAttributes.div ?? []),
      ["className", "markdown-alert", /^markdown-alert-[a-z]+$/],
    ],
    p: [...(defaultAttributes.p ?? []), ["className", "markdown-alert-title"]],
  },
};

/**
 * A relative image, read from the same source as the file showing it and
 * decoded as the standalone image viewer decodes it: a typed object URL in
 * an `<img>`, revoked when replaced (`g5.documents.viewers.images`).
 */
const WorkspaceImage: React.FC<{
  workspaceId: string;
  repoId: string;
  path: string;
  alt: string;
  title?: string;
}> = ({ workspaceId, repoId, path, alt, title }) => {
  const { state } = useWorkspaceFileContent(workspaceId, repoId, path);
  const mime = imageMimeForPath(path);
  const file = state.status === "ok" ? state.data : null;
  const content = file?.content ?? null;
  const encoding = file?.encoding ?? null;
  const blob = useMemo(
    () =>
      content !== null && encoding !== null && mime
        ? fileBlob({ content, encoding }, mime)
        : null,
    [content, encoding, mime],
  );
  const src = useObjectUrl(blob);
  if (!file || !mime) {
    return (
      <span
        className="g5-markdown-image-placeholder"
        data-testid="markdown-preview-image-unavailable"
        title={state.status === "error" ? `${path} could not be loaded` : path}
      >
        {alt || path}
      </span>
    );
  }
  if (!src) return null;
  return <img src={src} alt={alt} title={title} loading="lazy" />;
};

/** The text of a hast element, for code blocks. */
function textOf(node: unknown): string {
  if (!node || typeof node !== "object") return "";
  const value = (node as { value?: unknown }).value;
  if (typeof value === "string") return value;
  const children = (node as { children?: unknown[] }).children;
  return Array.isArray(children) ? children.map(textOf).join("") : "";
}

function languageOf(node: unknown): string | null {
  const className = (node as { properties?: { className?: unknown } })
    ?.properties?.className;
  const classes = Array.isArray(className) ? className : [];
  for (const entry of classes) {
    if (typeof entry === "string" && entry.startsWith("language-")) {
      return entry.slice("language-".length);
    }
  }
  return null;
}

type MarkdownRuntime = Required<
  Pick<
    MarkdownPreviewProps,
    "filePath" | "workspaceId" | "repoId" | "trustedContent" | "historical"
  >
> &
  Pick<
    MarkdownPreviewProps,
    "renderCode" | "onOpenWorkspaceFile" | "basePath" | "renderImage"
  >;
const MarkdownContext = createContext<MarkdownRuntime>(null!);
const markdownComponents: Components = {
  pre: function MarkdownCode({ node, children, ...props }) {
    const { trustedContent, historical, renderCode } =
      useContext(MarkdownContext);
    const code = (node as { children?: unknown[] } | undefined)?.children?.[0];
    const language = languageOf(code);
    const body = textOf(code).replace(/\n$/, "");
    const domain =
      trustedContent &&
      !historical &&
      renderCode?.(language ?? "", body, node?.position?.start.offset ?? -1);
    if (domain != null && domain !== false) return <>{domain}</>;
    if (trustedContent && !historical && language === "g5-question")
      return <QuestionCard content={body} />;
    if (
      trustedContent &&
      !historical &&
      ["g5-reactflow", "g5-flow"].includes(language ?? "")
    )
      return <ReactFlowDiagramWidget code={body} />;
    if (
      trustedContent &&
      !historical &&
      ["g5-js", "g5-widget"].includes(language ?? "")
    )
      return <SandboxedJsWidgetFrame code={body} />;
    if (trustedContent && language === "mermaid") {
      return <MermaidDiagram code={textOf(code).replace(/\n$/, "")} />;
    }
    if (language === "math") return <pre {...props}>{children}</pre>;
    const text = textOf(code).replace(/\n$/, "");
    return (
      <div
        className="mb-3 rounded-[var(--g5-radius-sm)] overflow-hidden"
        data-testid="markdown-code-block"
      >
        <div className="flex items-center justify-between gap-2 px-3 py-1 text-[10px] font-medium uppercase tracking-wider text-[var(--g5-text2)] bg-[var(--g5-bg2)] border-b border-[var(--g5-stroke)]">
          <span className="min-w-0 truncate" title={language || undefined}>
            {language}
          </span>
          <CodeCopyButton text={text} />
        </div>
        {language ? (
          <pre {...props}>
            <code
              className={`language-${language}`}
              // highlight.js escapes the text; spans carry only token styles.
              dangerouslySetInnerHTML={{
                __html: highlightCode(text, language),
              }}
            />
          </pre>
        ) : (
          <pre {...props}>{children}</pre>
        )}
      </div>
    );
  },
  a: function MarkdownLink({ node: _node, href, children, ...props }) {
    const {
      trustedContent,
      historical,
      filePath,
      repoId,
      onOpenWorkspaceFile,
    } = useContext(MarkdownContext);
    if (!trustedContent && href && !/^https?:\/\//i.test(href))
      return (
        <span>
          {children} <code>{href}</code>
        </span>
      );
    if (!href || (href.startsWith("g5:") && !href.startsWith("g5://file/")))
      return <span>{children}</span>;
    if (historical)
      return (
        <span
          role="link"
          aria-disabled="true"
          title={`Historical navigation unavailable: ${href ?? ""}`}
        >
          {children}
        </span>
      );
    if (!trustedContent && !/^https?:\/\//i.test(href ?? ""))
      return <span>{children}</span>;
    if (href.startsWith("g5://file/"))
      return (
        <a {...props} href={href} data-g5-file-link="true">
          {children}
        </a>
      );
    const target =
      href && onOpenWorkspaceFile ? resolveRepoLink(filePath, href) : null;
    if (target) {
      return (
        <a
          {...props}
          href={href}
          data-repo-link={target.path}
          onClick={(event) => {
            event.preventDefault();
            onOpenWorkspaceFile?.(target.path, repoId, target.line);
          }}
        >
          {children}
        </a>
      );
    }
    if (href && !href.startsWith("#")) {
      return (
        <a {...props} href={href} target="_blank" rel="noopener noreferrer">
          {children}
        </a>
      );
    }
    // In-page links (headings, footnotes) name ids that sanitizing
    // prefixed with `user-content-`, as GitHub does; follow them inside
    // the preview.
    return (
      <a
        {...props}
        href={href}
        onClick={(event) => {
          if (!href || href.length < 2) return;
          event.preventDefault();
          let id = href.slice(1);
          try {
            id = decodeURIComponent(id);
          } catch {
            // keep the raw fragment
          }
          const container = event.currentTarget.closest(".g5-markdown-preview");
          const target = Array.from(
            container?.querySelectorAll<HTMLElement>("[id]") ?? [],
          ).find(
            (element) =>
              element.id === `${CLOBBER_PREFIX}${id}` || element.id === id,
          );
          target?.scrollIntoView?.({ block: "start" });
        }}
      >
        {children}
      </a>
    );
  },
  img: function MarkdownImage({ node: _node, src, alt, title, ...props }) {
    const {
      trustedContent,
      historical,
      filePath,
      workspaceId,
      repoId,
      basePath,
      renderImage,
    } = useContext(MarkdownContext);
    const source = typeof src === "string" ? src : "";
    if (!source) return <span>{alt || "Image"}</span>;
    if (historical || !trustedContent)
      return (
        <span data-testid="markdown-restricted-image">
          {alt || "Image"}
          {!historical && /^https?:\/\//i.test(source) && (
            <a
              href={source}
              aria-label={`Image: ${alt || "Image"}`}
              target="_blank"
              rel="noopener noreferrer nofollow"
            >
              {source}
            </a>
          )}
        </span>
      );
    const hosted = renderImage?.({ src: source, alt: alt ?? "", title });
    if (hosted) return hosted;
    const target =
      workspaceId && repoId ? resolveRepoLink(filePath, source) : null;
    if (target) {
      return (
        <WorkspaceImage
          workspaceId={workspaceId}
          repoId={repoId}
          path={target.path}
          alt={alt ?? ""}
          title={title}
        />
      );
    }
    return (
      <img
        {...props}
        src={
          basePath && source && !/^(?:[a-z]+:|\/)/i.test(source)
            ? `${basePath.replace(/\/$/, "")}/${source}`
            : source
        }
        alt={alt ?? ""}
        title={title}
        loading="lazy"
        referrerPolicy="no-referrer"
      />
    );
  },
};

export const MarkdownPreview: React.FC<MarkdownPreviewProps> = ({
  content,
  filePath = "",
  workspaceId = "",
  repoId = "",
  onOpenWorkspaceFile,
  className = "",
  trustedContent = true,
  historical = false,
  basePath,
  plugins = [],
  components: hostComponents,
  renderCode,
  renderImage,
}) => {
  return (
    <MarkdownContext.Provider
      value={{
        filePath,
        workspaceId,
        repoId,
        trustedContent,
        historical,
        renderCode,
        onOpenWorkspaceFile,
        basePath,
        renderImage,
      }}
    >
      <div
        className={`markdown-body g5-markdown-preview ${className}`}
        data-testid="markdown-preview"
      >
        <ReactMarkdown
          remarkPlugins={[
            remarkGfm,
            remarkMath,
            remarkGithubAlerts,
            remarkG5Markdown,
            () =>
              (tree: {
                children: { position?: { start: { offset?: number } } }[];
              }) => {
                const frontmatter =
                  /^(?:\uFEFF)?---\r?\n[\s\S]*?\r?\n(?:---|\.\.\.)[^\S\r\n]*(?:\r?\n|$)/.exec(
                    content,
                  );
                if (frontmatter)
                  tree.children = tree.children.filter(
                    (node) =>
                      (node.position?.start.offset ?? 0) >=
                      frontmatter[0].length,
                  );
              },
          ]}
          remarkRehypeOptions={{ clobberPrefix: "" }}
          rehypePlugins={[
            rehypeRaw,
            rehypeSlug,
            [rehypeSanitize, sanitizeSchema],
            rehypeKatex,
            ...plugins,
          ]}
          urlTransform={(url) =>
            url.startsWith("g5:") ? url : defaultUrlTransform(url)
          }
          components={{ ...markdownComponents, ...hostComponents }}
        >
          {content}
        </ReactMarkdown>
      </div>
    </MarkdownContext.Provider>
  );
};
