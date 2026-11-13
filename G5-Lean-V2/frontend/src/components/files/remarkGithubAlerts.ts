import { MARKDOWN_ALERTS } from "./markdownAlerts";

// =============================================================================
// G5 Files — remarkGithubAlerts
// =============================================================================
// GitHub alerts in FILES Preview (`g5.documents.files-view.preview`): a block
// quote whose first line is `[!NOTE]`, `[!TIP]`, `[!IMPORTANT]`, `[!WARNING]`
// or `[!CAUTION]` renders as GitHub renders it, a `markdown-alert` box with a
// title, styled by github-markdown-css. Any other block quote is unchanged.
// =============================================================================

interface MdNode {
  type: string;
  value?: string;
  children?: MdNode[];
  data?: {
    hName?: string;
    hProperties?: Record<string, unknown>;
  };
}

const MARKER = /^\[!(note|tip|important|warning|caution)\][ \t]*(?:\r?\n|$)/i;

function convert(blockquote: MdNode): void {
  const first = blockquote.children?.[0];
  if (first?.type !== "paragraph") return;
  const text = first.children?.[0];
  if (text?.type !== "text" || typeof text.value !== "string") return;
  const match = MARKER.exec(text.value);
  if (!match) return;
  const kind = match[1].toLowerCase();

  text.value = text.value.slice(match[0].length);
  // A marker on its own line ends where the alert's text starts; drop the
  // empty text, and a paragraph that held only the marker.
  if (text.value === "") first.children?.shift();
  if ((first.children?.length ?? 0) === 0) blockquote.children?.shift();

  blockquote.data = {
    ...blockquote.data,
    hName: "div",
    hProperties: {
      className: [
        "markdown-alert",
        `markdown-alert-${MARKDOWN_ALERTS[kind].style}`,
      ],
    },
  };
  blockquote.children?.unshift({
    type: "paragraph",
    data: { hName: "p", hProperties: { className: ["markdown-alert-title"] } },
    children: [{ type: "text", value: MARKDOWN_ALERTS[kind].title }],
  });
}

function visit(node: MdNode): void {
  if (node.type === "blockquote") convert(node);
  for (const child of node.children ?? []) visit(child);
}

/** remark plugin: turns GitHub alert block quotes into alert boxes. */
export function remarkGithubAlerts() {
  return (tree: MdNode) => {
    visit(tree);
  };
}
