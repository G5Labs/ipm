// =============================================================================
// G5 Design System — SearchField
// =============================================================================
// A search input with its leading glyph and a clear control that appears
// while there is text. Shared by Home's workspace directory, the Issues tile
// (its compact form) and the organization fork picker (workbench
// g5.workbench.shell-chrome).
// =============================================================================

import { Search, X } from "lucide-react";
import React from "react";
import { classNames } from "./UI";

export interface SearchFieldProps {
  value: string;
  onChange: (value: string) => void;
  /** Accessible name; also the placeholder unless one is given. */
  label: string;
  placeholder?: string;
  testId: string;
  className?: string;
  /** The clear control's test id; defaults to `<testId>-clear`. */
  clearTestId?: string;
  inputRef?: React.Ref<HTMLInputElement>;
  inputClassName?: string;
  /** Chip-height field for a tile header; the default suits a toolbar row. */
  compact?: boolean;
  onKeyDown?: React.KeyboardEventHandler<HTMLInputElement>;
}

export const SearchField: React.FC<SearchFieldProps> = ({
  value,
  onChange,
  label,
  placeholder,
  testId,
  className,
  clearTestId,
  inputRef,
  inputClassName,
  compact = false,
  onKeyDown,
}) => (
  <label className={classNames("relative flex items-center", className)}>
    <Search
      size={compact ? 12 : 14}
      className={classNames(
        "pointer-events-none absolute text-[var(--g5-text2)]",
        compact ? "left-2" : "left-2.5",
      )}
      aria-hidden="true"
    />
    <input
      ref={inputRef}
      type="search"
      data-testid={testId}
      aria-label={label}
      placeholder={placeholder ?? label}
      value={value}
      onChange={(event) => onChange(event.target.value)}
      onKeyDown={onKeyDown}
      className={classNames(
        "w-full rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] text-[var(--g5-text0)]",
        // The field's own clear control stands in for the browser's.
        "[&::-webkit-search-cancel-button]:appearance-none",
        compact ? "h-6 pl-6 pr-6 text-[11px]" : "h-8 pl-8 pr-7 text-sm",
        "placeholder:text-[var(--g5-text2)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]",
        inputClassName,
      )}
    />
    {value && (
      <button
        type="button"
        data-testid={clearTestId ?? `${testId}-clear`}
        aria-label="Clear search"
        className="absolute right-1.5 inline-flex h-5 w-5 items-center justify-center rounded text-[var(--g5-text2)] hover:text-[var(--g5-text0)]"
        onClick={() => onChange("")}
      >
        <X size={12} />
      </button>
    )}
  </label>
);
