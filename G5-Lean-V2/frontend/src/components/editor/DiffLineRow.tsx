import type React from "react";

import type { DiffLine } from "../../lib/types";
import { classNames } from "../ui/UI";
import { DIFF_LINE_ROW_STYLE } from "./diffRowStyles";

export const DiffLineRow: React.FC<{ line: DiffLine }> = ({ line }) => {
  const symbol =
    line.type === "addition" ? "+" : line.type === "deletion" ? "−" : " ";
  return (
    <div
      data-testid="diff-line-row"
      data-line-type={line.type}
      style={DIFF_LINE_ROW_STYLE}
      className={classNames(
        "grid min-h-5",
        line.type === "addition" &&
          "bg-[color-mix(in_srgb,var(--g5-ok)_13%,transparent)]",
        line.type === "deletion" &&
          "bg-[color-mix(in_srgb,var(--g5-blocked)_13%,transparent)]",
      )}
    >
      <span className="px-1 text-right text-[var(--g5-text2)] select-none">
        {line.oldLine ?? ""}
      </span>
      <span className="px-1 text-right text-[var(--g5-text2)] select-none border-r border-[var(--g5-stroke)]/40">
        {line.newLine ?? ""}
      </span>
      <span
        className={classNames(
          "px-1 select-none",
          line.type === "addition" && "text-[var(--g5-ok)]",
          line.type === "deletion" && "text-[var(--g5-blocked)]",
          line.type === "context" && "text-[var(--g5-text2)]",
        )}
      >
        {symbol}
      </span>
      <span
        data-testid="diff-line-code"
        className="px-2 whitespace-pre text-[var(--g5-text0)]"
      >
        {line.content}
      </span>
    </div>
  );
};
