// =============================================================================
// G5 Design System — UI Primitives
// =============================================================================
// Button, IconButton, Chip, SectionTitle, classNames utility, and
// State/ThemeName type re-exports.
//
// All components use CSS custom properties for colors (never hardcoded values)
// to ensure instant theme switching via data-theme/data-mode attributes.
// =============================================================================

import { Check, ChevronDown } from "lucide-react";
import React from "react";
import type { State, ThemeName } from "../../lib/types";

// Re-export types for convenience
export type { State, ThemeName };

// Dialog primitives live in sibling files; re-exported here so consumers
// import every primitive from one module (design-system D6/D14/D15).
export { Modal } from "./Modal";
export type { ModalProps, ModalSize, ModalTier } from "./Modal";
export { useDialogFocus, useCoveringOverlayOpen } from "./useDialogFocus";
export type { UseDialogFocusOptions } from "./useDialogFocus";

// ---------------------------------------------------------------------------
// STATE_MAP — Maps each State to { dot, tint, label }
// ---------------------------------------------------------------------------

/**
 * Mapping from State values to display properties.
 * - dot: CSS color value for the status dot indicator
 * - tint: CSS background color with 10% opacity for pill/chip backgrounds
 * - label: Human-readable display label
 */
export const STATE_MAP: Record<
  State,
  { dot: string; tint: string; label: string }
> = {
  active: {
    dot: "var(--g5-active)",
    tint: "color-mix(in srgb, var(--g5-active) 10%, transparent)",
    label: "Running",
  },
  ok: {
    dot: "var(--g5-ok)",
    tint: "color-mix(in srgb, var(--g5-ok) 10%, transparent)",
    label: "Integrated",
  },
  info: {
    dot: "var(--g5-info)",
    tint: "color-mix(in srgb, var(--g5-info) 10%, transparent)",
    label: "Info",
  },
  warn: {
    dot: "var(--g5-warn)",
    tint: "color-mix(in srgb, var(--g5-warn) 10%, transparent)",
    label: "Review Needed",
  },
  blocked: {
    dot: "var(--g5-blocked)",
    tint: "color-mix(in srgb, var(--g5-blocked) 10%, transparent)",
    label: "Blocked",
  },
  muted: {
    dot: "rgb(255 255 255 / 0.20)",
    tint: "transparent",
    label: "Idle",
  },
};

// ---------------------------------------------------------------------------
// classNames — Falsy-filtering class concatenation utility
// ---------------------------------------------------------------------------

// Lives in its own leaf module so Modal.tsx can import it without a
// UI.tsx ↔ Modal.tsx cycle; re-exported here for existing consumers.
export { classNames } from "./classNames";
import { classNames } from "./classNames";

// ---------------------------------------------------------------------------
// ThemedSelect — In-app popup select
// ---------------------------------------------------------------------------

export interface ThemedSelectOption {
  value: string;
  label: string;
  disabled?: boolean;
  muted?: boolean;
  assistiveDescription?: string;
}

export interface ThemedSelectProps {
  id?: string;
  value: string;
  options: ThemedSelectOption[];
  onChange: (value: string) => void;
  disabled?: boolean;
  testId?: string;
  placeholder?: string;
  size?: "sm" | "md";
  className?: string;
  /** Trigger arrow: the default chevron, or a plain ▾ like text buttons. */
  indicator?: "chevron" | "caret";
  /** Element ids naming the trigger, e.g. its label then the trigger itself. */
  labelledBy?: string;
}

function optionTestId(
  base: string | undefined,
  value: string,
): string | undefined {
  if (!base) return undefined;
  return `${base}-option-${value || "empty"}`;
}

const THEMED_SELECT_MENU_GAP_PX = 4;
const THEMED_SELECT_MENU_MAX_HEIGHT_PX = 240;
const THEMED_SELECT_VIEWPORT_PADDING_PX = 8;

function themedSelectVerticalBounds(root: HTMLElement): {
  top: number;
  bottom: number;
} {
  let top = THEMED_SELECT_VIEWPORT_PADDING_PX;
  let bottom = window.innerHeight - THEMED_SELECT_VIEWPORT_PADDING_PX;
  let ancestor = root.parentElement;

  while (ancestor && ancestor !== document.body) {
    const style = window.getComputedStyle(ancestor);
    if (
      /auto|scroll|hidden|clip/.test(
        `${style.overflow} ${style.overflowX} ${style.overflowY}`,
      )
    ) {
      const rect = ancestor.getBoundingClientRect();
      top = Math.max(top, rect.top);
      bottom = Math.min(bottom, rect.bottom);
    }
    ancestor = ancestor.parentElement;
  }

  return { top, bottom };
}

export const ThemedSelect: React.FC<ThemedSelectProps> = ({
  id,
  value,
  options,
  onChange,
  disabled = false,
  testId,
  placeholder = "Select",
  size = "md",
  className,
  indicator = "chevron",
  labelledBy,
}) => {
  const generatedId = React.useId();
  const listboxId = `${id ?? generatedId}-listbox`;
  const rootRef = React.useRef<HTMLDivElement>(null);
  const buttonRef = React.useRef<HTMLButtonElement>(null);
  const listboxRef = React.useRef<HTMLDivElement>(null);
  const [open, setOpen] = React.useState(false);
  const [listboxPlacement, setListboxPlacement] = React.useState<{
    side: "above" | "below";
    maxHeight: number;
  }>({ side: "below", maxHeight: THEMED_SELECT_MENU_MAX_HEIGHT_PX });
  const selectedOption = options.find((option) => option.value === value);
  const sizeClasses =
    size === "sm" ? "px-2 py-1.5 text-xs" : "px-3 py-2 text-sm";

  React.useEffect(() => {
    if (!open) return;

    const handleMouseDown = (event: MouseEvent) => {
      const target = event.target;
      if (
        target instanceof Node &&
        rootRef.current &&
        !rootRef.current.contains(target)
      ) {
        setOpen(false);
      }
    };

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        // Capture-phase + stopPropagation: an Escape that closes the open
        // listbox must NOT also reach a surrounding dialog's document-level
        // Escape handler (useDialogFocus) and close the modal beneath it.
        event.stopPropagation();
        setOpen(false);
        buttonRef.current?.focus();
      }
    };

    document.addEventListener("mousedown", handleMouseDown);
    document.addEventListener("keydown", handleKeyDown, true);
    return () => {
      document.removeEventListener("mousedown", handleMouseDown);
      document.removeEventListener("keydown", handleKeyDown, true);
    };
  }, [open]);

  React.useEffect(() => {
    if (disabled) {
      setOpen(false);
    }
  }, [disabled]);

  React.useLayoutEffect(() => {
    if (!open) return;

    const updatePlacement = () => {
      const root = rootRef.current;
      const listbox = listboxRef.current;
      if (!root || !listbox) return;

      const triggerRect = root.getBoundingClientRect();
      const bounds = themedSelectVerticalBounds(root);
      const availableAbove = Math.max(
        0,
        triggerRect.top - bounds.top - THEMED_SELECT_MENU_GAP_PX,
      );
      const availableBelow = Math.max(
        0,
        bounds.bottom - triggerRect.bottom - THEMED_SELECT_MENU_GAP_PX,
      );
      const estimatedContentHeight = Math.max(
        listbox.scrollHeight,
        options.length * 36 + 8,
      );
      const desiredHeight = Math.min(
        THEMED_SELECT_MENU_MAX_HEIGHT_PX,
        estimatedContentHeight,
      );
      const side =
        availableBelow < desiredHeight && availableAbove > availableBelow
          ? "above"
          : "below";
      const availableHeight =
        side === "above" ? availableAbove : availableBelow;

      setListboxPlacement({
        side,
        maxHeight: Math.min(THEMED_SELECT_MENU_MAX_HEIGHT_PX, availableHeight),
      });
    };

    updatePlacement();
    window.addEventListener("scroll", updatePlacement, true);
    window.addEventListener("resize", updatePlacement);
    return () => {
      window.removeEventListener("scroll", updatePlacement, true);
      window.removeEventListener("resize", updatePlacement);
    };
  }, [open, options.length]);

  const choose = (option: ThemedSelectOption) => {
    if (option.disabled) return;
    onChange(option.value);
    setOpen(false);
    buttonRef.current?.focus();
  };

  // Keyboard use matches a native select: arrows open the list on the
  // selected option and move between enabled options; Enter/Space choose.
  const focusOnOpen = React.useRef(false);
  const moveFocus = (to: "selected" | "first" | "last" | 1 | -1) => {
    const items = Array.from(
      listboxRef.current?.querySelectorAll<HTMLButtonElement>(
        '[role="option"]:not([disabled])',
      ) ?? [],
    );
    if (items.length === 0) return;
    const current = items.indexOf(document.activeElement as HTMLButtonElement);
    const selected = items.findIndex(
      (item) => item.getAttribute("aria-selected") === "true",
    );
    const index =
      to === "first"
        ? 0
        : to === "last"
          ? items.length - 1
          : to === "selected"
            ? Math.max(selected, 0)
            : current < 0
              ? Math.max(selected, 0)
              : Math.min(items.length - 1, Math.max(0, current + to));
    items[index].focus();
  };

  React.useEffect(() => {
    if (open && focusOnOpen.current) {
      focusOnOpen.current = false;
      moveFocus("selected");
    }
  }, [open]);

  return (
    <div ref={rootRef} className={classNames("relative", className)}>
      <button
        id={id}
        ref={buttonRef}
        type="button"
        data-testid={testId}
        disabled={disabled}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={open ? listboxId : undefined}
        aria-labelledby={labelledBy}
        onClick={() => setOpen((current) => !current)}
        onKeyDown={(event) => {
          if (event.key === "ArrowDown" || event.key === "ArrowUp") {
            event.preventDefault();
            if (open) moveFocus("selected");
            else {
              focusOnOpen.current = true;
              setOpen(true);
            }
          }
        }}
        className={classNames(
          "flex w-full items-center justify-between gap-2 rounded-[var(--g5-radius-sm)]",
          "border border-[var(--g5-stroke)] bg-[var(--g5-bg2)]",
          "text-left text-[var(--g5-text0)]",
          "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]",
          "disabled:cursor-not-allowed disabled:opacity-70",
          sizeClasses,
        )}
      >
        <span
          className="min-w-0 flex-1 truncate"
          title={selectedOption?.label ?? placeholder}
        >
          {selectedOption?.label ?? placeholder}
        </span>
        {indicator === "caret" ? (
          <span aria-hidden="true" className="shrink-0">
            ▾
          </span>
        ) : (
          <ChevronDown
            size={size === "sm" ? 14 : 16}
            className={classNames(
              "shrink-0 text-[var(--g5-text1)] transition-transform",
              open && "rotate-180",
            )}
            aria-hidden="true"
          />
        )}
      </button>
      {open && (
        <>
          <div
            ref={listboxRef}
            id={listboxId}
            role="listbox"
            tabIndex={-1}
            aria-labelledby={labelledBy}
            onKeyDown={(event) => {
              const to =
                event.key === "ArrowDown"
                  ? 1
                  : event.key === "ArrowUp"
                    ? -1
                    : event.key === "Home"
                      ? "first"
                      : event.key === "End"
                        ? "last"
                        : null;
              if (to === null) return;
              event.preventDefault();
              moveFocus(to);
            }}
            data-placement={listboxPlacement.side}
            style={{ maxHeight: `${listboxPlacement.maxHeight}px` }}
            className={classNames(
              // #1370 (design-system D6): the open list grows to its content
              // (bounded) instead of clamping to the trigger width, so options
              // sharing long prefixes stay distinguishable in narrow columns.
              "absolute left-0 z-[120] min-w-full w-max max-w-[22rem] overflow-y-auto",
              listboxPlacement.side === "above"
                ? "bottom-full mb-1"
                : "top-full mt-1",
              "rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)]",
              "bg-[var(--g5-bg1)] py-1 shadow-2xl",
            )}
          >
            {options.map((option) => {
              const selected = option.value === value;
              const assistiveDescriptionId = option.assistiveDescription
                ? `${listboxId}-option-${option.value || "empty"}-description`
                : undefined;
              return (
                <button
                  key={option.value || "empty"}
                  type="button"
                  role="option"
                  aria-selected={selected}
                  aria-describedby={assistiveDescriptionId}
                  disabled={option.disabled}
                  data-muted={option.muted ? "true" : "false"}
                  data-testid={optionTestId(testId, option.value)}
                  title={option.label}
                  onClick={() => choose(option)}
                  className={classNames(
                    "flex w-full items-center gap-2 px-3 py-2 text-left text-sm",
                    "text-[var(--g5-text0)] hover:bg-[var(--g5-bg2)]",
                    "focus:bg-[var(--g5-bg2)] focus-visible:outline-none",
                    selected && "bg-[var(--g5-info)]/15",
                    option.muted && !option.disabled && "opacity-60",
                    option.disabled && "cursor-not-allowed opacity-50",
                  )}
                >
                  <Check
                    size={15}
                    className={classNames(
                      "shrink-0 text-[var(--g5-info)]",
                      selected ? "opacity-100" : "opacity-0",
                    )}
                    aria-hidden="true"
                  />
                  <span
                    className="min-w-0 flex-1 truncate"
                    title={option.label}
                  >
                    {option.label}
                  </span>
                </button>
              );
            })}
          </div>
          {options.map((option) =>
            option.assistiveDescription ? (
              <span
                key={option.value || "empty"}
                id={`${listboxId}-option-${option.value || "empty"}-description`}
                className="sr-only"
              >
                {option.assistiveDescription}
              </span>
            ) : null,
          )}
        </>
      )}
    </div>
  );
};

// ---------------------------------------------------------------------------
// Button — Standard action button
// ---------------------------------------------------------------------------

export interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  kind: "primary" | "secondary" | "ghost";
  children: React.ReactNode;
}

/**
 * Standard action button with three visual variants:
 * - primary: Filled with --g5-info background, white text
 * - secondary: Outlined with --g5-stroke border
 * - ghost: Transparent, text color only, hover shows --g5-bg2
 *
 * Disabled state reduces opacity and prevents pointer events.
 */
export const Button: React.FC<ButtonProps> = ({
  kind,
  disabled = false,
  children,
  className,
  ...props
}) => {
  const base = classNames(
    "inline-flex items-center justify-center gap-2",
    "px-4 py-2 text-sm font-medium",
    "transition-colors duration-150",
    "rounded-[var(--g5-radius-sm)]",
    "cursor-pointer",
    "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)] focus-visible:ring-offset-1",
    disabled && "opacity-40 pointer-events-none",
  );

  const variants: Record<ButtonProps["kind"], string> = {
    primary: classNames(
      "bg-[var(--g5-info)] text-white",
      "hover:brightness-110",
    )!,
    secondary: classNames(
      "border border-[var(--g5-stroke)]",
      "text-[var(--g5-text0)]",
      "hover:bg-[var(--g5-bg2)]",
    )!,
    ghost: classNames(
      "text-[var(--g5-text1)]",
      "hover:bg-[var(--g5-bg2)]",
      "hover:text-[var(--g5-text0)]",
    )!,
  };

  return (
    <button
      type="button"
      data-testid="ui-button"
      className={classNames(base, variants[kind], className)}
      disabled={disabled}
      {...props}
    >
      {children}
    </button>
  );
};

// ---------------------------------------------------------------------------
// IconButton — Square icon-only button
// ---------------------------------------------------------------------------

export interface IconButtonProps extends Omit<
  React.ButtonHTMLAttributes<HTMLButtonElement>,
  "title"
> {
  onClick?: () => void;
  title: string;
  children: React.ReactNode;
}

/**
 * Square icon-only button (32×32px / 8×8 Tailwind units).
 * Hover shows --g5-bg2 background.
 * Requires a title for accessibility.
 */
export const IconButton: React.FC<IconButtonProps> = ({
  onClick,
  title,
  children,
  ...rest
}) => {
  return (
    <button
      type="button"
      data-testid="ui-icon-button"
      className={classNames(
        "inline-flex items-center justify-center",
        "w-8 h-8",
        "rounded-[var(--g5-radius-sm)]",
        "text-[var(--g5-text1)]",
        "hover:bg-[var(--g5-bg2)] hover:text-[var(--g5-text0)]",
        "transition-colors duration-150",
        "cursor-pointer",
        "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]",
      )}
      onClick={onClick}
      title={title}
      aria-label={title}
      {...rest}
    >
      {children}
    </button>
  );
};

// ---------------------------------------------------------------------------
// Chip — Status pill with colored dot + label
// ---------------------------------------------------------------------------

export interface ChipProps extends React.HTMLAttributes<HTMLSpanElement> {
  state: State;
  text?: string;
}

/**
 * Status pill with a small colored dot (from STATE_MAP) followed by label text.
 * Background uses the state's tint color (10% opacity of the state color).
 * If text is not provided, falls back to the STATE_MAP label.
 */
export const Chip: React.FC<ChipProps> = ({ state, text, ...rest }) => {
  const mapping = STATE_MAP[state];
  const label = text ?? mapping.label;

  return (
    <span
      className={classNames(
        "inline-flex items-center gap-1.5",
        "px-2 py-0.5",
        "text-xs font-medium",
        "rounded-[var(--g5-radius-sm)]",
        "text-[var(--g5-text1)]",
      )}
      style={{ backgroundColor: mapping.tint }}
      {...rest}
    >
      <span
        className="w-1.5 h-1.5 rounded-full shrink-0"
        style={{ backgroundColor: mapping.dot }}
        aria-hidden="true"
      />
      {label}
    </span>
  );
};

// ---------------------------------------------------------------------------
// SectionTitle — Uppercase tracking-wide section label
// ---------------------------------------------------------------------------

export interface SectionTitleProps {
  children: React.ReactNode;
}

/**
 * Uppercase, tracking-wide section label for panel headers and list groups.
 * Renders with uppercase, tracking-wide, text-xs, --g5-text2 color.
 */
export const SectionTitle: React.FC<SectionTitleProps> = ({ children }) => {
  return (
    <h3
      className={classNames(
        "text-xs font-semibold uppercase tracking-wide",
        "text-[var(--g5-text2)]",
        "select-none",
      )}
    >
      {children}
    </h3>
  );
};
