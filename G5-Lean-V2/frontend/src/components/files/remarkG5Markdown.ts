import type { Root, RootContent, Paragraph } from "mdast";
import type { VFile } from "vfile";
import { replaceEmojiShortcodes } from "../../lib/utils";

import { MARKDOWN_ALERTS } from "./markdownAlerts";

/** Retain authored callout directives and emoji text in the common Markdown pipeline. */
export function remarkG5Markdown() {
  return (tree: Root, file: VFile) => {
    const source = String(file);
    const ranges = [
      ...source.matchAll(
        /^:::[ \t]*(note|tip|warning|danger)[ \t]*\r?\n[\s\S]*?^:::[ \t]*(?:\r?\n|$)/gm,
      ),
    ];
    for (const match of ranges.reverse()) {
      const start = match.index!;
      const end = start + match[0].length;
      const first = tree.children.findIndex(
        (n) => n.position?.start.offset === start,
      );
      if (first < 0) continue;
      let last = first;
      while (
        last + 1 < tree.children.length &&
        (tree.children[last + 1].position?.start.offset ?? end) < end
      )
        last++;
      const children = tree.children.slice(first, last + 1);
      const trim = (node: RootContent, beginning: boolean) => {
        if (node.type !== "paragraph") return;
        const text = beginning ? node.children[0] : node.children.at(-1);
        if (text?.type === "text")
          text.value = beginning
            ? text.value.replace(/^:::[ \t]*\w+[ \t]*(?:\r?\n|$)/, "")
            : text.value.replace(/(?:\r?\n|^):::[ \t]*$/, "");
      };
      trim(children[0], true);
      trim(children.at(-1)!, false);
      const { title, icon, style } = MARKDOWN_ALERTS[match[1]];
      const heading: Paragraph = {
        type: "paragraph",
        data: { hProperties: { className: ["markdown-alert-title"] } },
        children: [
          {
            type: "strong",
            data: { hName: "span" },
            children: [{ type: "text", value: icon }],
          },
          { type: "text", value: ` ${title}` },
        ],
      };
      tree.children.splice(first, last - first + 1, {
        type: "blockquote",
        data: {
          hName: "div",
          hProperties: {
            className: ["markdown-alert", `markdown-alert-${style}`],
          },
        },
        position: children[0].position,
        children: [
          heading,
          ...children.filter(
            (n) =>
              n.type !== "paragraph" ||
              n.children.some((c) => c.type !== "text" || c.value),
          ),
        ] as never,
      });
    }
    const visit = (node: {
      type: string;
      value?: string;
      children?: unknown[];
    }) => {
      if (node.type === "text" && node.value)
        node.value = replaceEmojiShortcodes(node.value);
      for (const child of node.children ?? []) visit(child as typeof node);
    };
    visit(tree);
  };
}
