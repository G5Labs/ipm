import type { ReactElement } from "react";
import { SandboxedHtmlFrame } from "../shared/SandboxedHtmlFrame";
import {
  HtmlDocAnswerBar,
  useHtmlDocAnswerBridge,
  type HtmlDocAnswerBridgeConfig,
} from "../shared/htmlDocAnswerBridge";
import {
  QuestionAnswerBar,
  useQuestionCardBridge,
} from "../shared/questionCardBridge";

interface HtmlPreviewShellProps {
  displayPath: string;
  title: string;
  html: string;
  /**
   * Answer-persistence bridge config. Writes use the shared file session
   * and its current Save capability.
   */
  answerBridge?: HtmlDocAnswerBridgeConfig;
  /**
   * Managed explainer documents only (`g5.documents.question.html`): the
   * workspace whose question sets fill the document's `data-g5-question`
   * placeholders, regardless of the file's opener.
   */
  questionWorkspaceId?: string;
  onOpenWorkspaceFile?: (
    filePath: string,
    repoRef?: string,
    line?: number,
  ) => void;
}

/** Sandboxed HTML projection; FileView owns Source/Preview and source editing. */
export function HtmlPreviewShell({
  displayPath,
  title,
  html,
  answerBridge,
  questionWorkspaceId,
  onOpenWorkspaceFile,
}: HtmlPreviewShellProps): ReactElement {
  const bridge = useHtmlDocAnswerBridge(answerBridge ?? null);
  const questions = useQuestionCardBridge(
    questionWorkspaceId ?? null,
    html,
    Boolean(questionWorkspaceId),
  );
  return (
    <div className="flex h-full min-h-0 flex-col bg-[var(--g5-bg0)]">
      <div className="flex shrink-0 items-center justify-between gap-3 border-b border-[var(--g5-stroke)] px-3 py-2">
        <div className="min-w-0 truncate text-xs text-[var(--g5-text2)]">
          {displayPath}
        </div>
      </div>
      {bridge.bar && (
        <HtmlDocAnswerBar
          state={bridge.bar}
          onApply={bridge.applyPending}
          onDismiss={bridge.dismissPending}
        />
      )}
      {questions.bar && (
        <QuestionAnswerBar
          state={questions.bar}
          onConfirm={questions.confirm}
          onDismiss={questions.dismiss}
        />
      )}
      <SandboxedHtmlFrame
        html={html}
        title={title}
        className="min-h-0 flex-1"
        iframeClassName="h-full w-full border-0 bg-white"
        iframeRef={bridge.frameProps?.iframeRef}
        onLoad={bridge.frameProps?.onLoad}
        onOpenWorkspaceFile={onOpenWorkspaceFile}
        questionFrame={questions.frame}
      />
    </div>
  );
}
