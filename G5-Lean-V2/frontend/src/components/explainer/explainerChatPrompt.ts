import {
  explainerTemplateSlug,
  type ExplainerFormat,
} from "../../hooks/useExplainers";

interface ExplainerChatPromptInput {
  workspaceId: string;
  topic: string;
  prompt: string;
  fileName: string;
  format: ExplainerFormat;
}

const FORMAT_INSTRUCTIONS: Record<ExplainerFormat, string> = {
  page: "Create one self-contained HTML page (.html), laid out as a continuous scrolling document.",
  slides:
    "Create one self-contained HTML slide deck (.html), with distinct slides and working navigation.",
  markdown:
    "Create one self-contained Markdown document (.md), using portable Markdown rather than an HTML page or slide deck.",
};

/**
 * Where every chat-created explainer is saved and how it is named, so it lands
 * in the managed explainers root without overwriting an earlier one.
 */
export function explainerSaveInstructions(
  workspaceId: string,
  naming: string,
): string {
  return [
    `Save the file in the docs repository under docs/${workspaceId}/explainers/. When the workspace advertises the validated Project Docs alias, use project_docs/explainers/ for filesystem access to that same directory.`,
    naming,
    "Replace <date> with the current date and time (YYYY-MM-DD-HHMM). Create the directory if needed. If the file already exists, append -2, -3, and so on after the date and before the extension; do not overwrite an earlier explainer.",
    "Write the actual file, then report its workspace path in this chat.",
    EXPLAINER_QUESTION_INSTRUCTIONS,
  ].join("\n");
}

/**
 * Reader questions in an explainer (`g5.documents.question.html`): the
 * question lives in a set file the CLI writes, and the host renders the card
 * for a placeholder.
 */
export const EXPLAINER_QUESTION_INSTRUCTIONS =
  'If the explainer needs a decision from its reader, create the question with `g5d question create --set <document-stem> --file <questions.json>` and put `<div data-g5-question="<set path>#<question id>"></div>` where each card belongs (in Markdown, a [Q<N>. Title] block with a fenced g5-question-ref block {"path": "<set path>", "id": "<question id>"}). Do not render the question, its options or an answer form yourself, and never edit a .g5q.json file; commit and push the set file with the explainer.';

/** Naming for an explainer whose format the agent settles in the chat. */
export const EXPLAINER_CHAT_NAMING =
  "Choose a short descriptive kebab-case filename, followed by -<date>.html, or -<date>.md for a Markdown document.";

/** The form supplies only the first message of an ordinary workspace chat. */
export function explainerChatPrompt(input: ExplainerChatPromptInput): string {
  const extension = input.format === "markdown" ? "md" : "html";
  const slug = explainerTemplateSlug(input.fileName);

  return [
    `Create an explainer: ${input.topic.trim()}`,
    "The instruction, style, scope, and format below are already selected; proceed with generation without asking setup questions.",
    `## Style and additional instructions\n${input.prompt}`,
    "Use the workspace files as context for the requested subject.",
    `## Output\n${FORMAT_INSTRUCTIONS[input.format]}`,
    explainerSaveInstructions(
      input.workspaceId,
      slug
        ? `Use the filename ${slug}-<date>.${extension}.`
        : `Choose a short descriptive kebab-case filename from the instruction, followed by -<date>.${extension}.`,
    ),
  ].join("\n\n");
}
