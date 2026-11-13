import type { Artifact } from "./types";

type ContentBlock =
  | { type: "text"; content: string }
  | { type: "artifact"; artifact: Artifact };

/**
 * Parse assistant message content into alternating text and artifact blocks.
 * Artifact syntax: :::artifact{type="..." title="..." language="..."}...:::
 */
export function parseMessageContent(
  content: string,
  messageArtifacts: Artifact[],
): ContentBlock[] {
  const blocks: ContentBlock[] = [];
  const artifactRegex = /:::artifact\{([^}]*)\}([\s\S]*?):::/g;
  let lastIndex = 0;
  let match: RegExpExecArray | null;

  while ((match = artifactRegex.exec(content)) !== null) {
    // Text before this artifact
    if (match.index > lastIndex) {
      const textBefore = content.slice(lastIndex, match.index).trim();
      if (textBefore) {
        blocks.push({ type: "text", content: textBefore });
      }
    }

    // Parse artifact attributes
    const attrsStr = match[1];
    const artifactContent = match[2].trim();

    const typeMatch = attrsStr.match(/type="([^"]*)"/);
    const titleMatch = attrsStr.match(/title="([^"]*)"/);
    const languageMatch = attrsStr.match(/language="([^"]*)"/);

    const artifact: Artifact = {
      type: (typeMatch?.[1] as Artifact["type"]) || "code",
      title: titleMatch?.[1],
      content: artifactContent,
      language: languageMatch?.[1],
    };

    blocks.push({ type: "artifact", artifact });
    lastIndex = match.index + match[0].length;
  }

  // Remaining text after last artifact
  if (lastIndex < content.length) {
    const remaining = content.slice(lastIndex).trim();
    if (remaining) {
      blocks.push({ type: "text", content: remaining });
    }
  }

  // If no inline artifacts were found but message has content, add as text
  if (blocks.length === 0 && content.trim()) {
    blocks.push({ type: "text", content: content.trim() });
  }

  // Append DTO artifacts not found inline
  for (const artifact of messageArtifacts) {
    const alreadyInline = blocks.some(
      (b) => b.type === "artifact" && b.artifact.content === artifact.content,
    );
    if (!alreadyInline) {
      blocks.push({ type: "artifact", artifact });
    }
  }

  return blocks;
}
