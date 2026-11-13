// =============================================================================
// G5 Workbench — ActionRibbon
// =============================================================================
// Per-phase action strip under (or beside) the PhaseChevrons in the
// `alignment` layout (workbench g5.workbench.actions): groups of templated
// actions, primary ones tinted with the phase hue, menus as real dropdowns.
// The content is data (the served workbench configuration, or the client
// fallback `WORKBENCH_ACTIONS` when the route is unavailable), not a
// component per button. The ribbon owns no behaviour: `onAction` hands every
// click to the page, which dispatches `chat` actions to the persona start and
// `open` actions to the handlers that already own each surface.
//
// Five styles (g5.workbench.header-styles, the person's "Action bar"
// setting): `extended-icons` — a glyph with the label under it, groups
// separated by dividers, no captions; `minimal-icons` — icon only, the label
// as tooltip and accessible name; `extended-text` — a caption over each
// group of text buttons with a leading icon; `simplified` — one wrapping row
// of icon+label buttons with dividers; `dynamic` — `extended-icons` that
// shrink one step to `compact-icons` when the groups overflow, then scroll
// horizontally. The ribbon owns that decision by measuring its own row; the
// trailing controls follow the rendered style. Every style renders the same
// actions under the same test ids, and every group keeps its accessible name.
// =============================================================================

import React, {
  type CSSProperties,
  type ReactNode,
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import { AlertTriangle, ChevronDown, Loader2 } from "lucide-react";
import type {
  ActionBarStyle,
  RibbonStyle,
  WorkbenchAction,
  WorkbenchActions,
  WorkspacePhase,
} from "../../lib/types";
import { useDismissOnOutside } from "../../hooks/useDismissOnOutside";
import { WORKBENCH_PHASES } from "../../lib/workbenchDefaults";
import { classNames } from "../ui/UI";
import { workbenchActionIcon } from "./workbenchIcons";

/** Why an action whose `target` this client does not know never executes. */
export const RIBBON_UNSUPPORTED_TITLE = "This action is not supported here";
/** Why templated chats cannot start from the client fallback. */
export const RIBBON_FALLBACK_CHAT_TITLE =
  "Workbench configuration unavailable; retry to start templated chats";

const KNOWN_TARGETS = new Set([
  "approvals",
  "ship",
  "rebase",
  "merge",
  "preview",
  "new-doc",
  "upload",
  "meeting",
  "view",
  "explainer",
  "review",
  "verification",
  "mock",
]);

/** Where the rendered action set came from. */
export type RibbonConfigStatus =
  | "loading"
  | "builtin"
  | "committed"
  | "invalid"
  | "fallback";

export interface RibbonActionState {
  disabled?: boolean;
  /** Tooltip; for a disabled action, the reason. */
  title?: string;
}

/** What a replaced button must look like to sit among the ribbon's own. */
export interface RibbonButtonShape {
  className: string;
  style?: CSSProperties;
  /** Glyph size of the style (12 for text styles, 18/22 for icon styles). */
  iconSize: number;
  /** Icon-only style: the label is the accessible name and tooltip only. */
  labelHidden: boolean;
  /**
   * Glyph over label: `full` (`extended-icons`) wraps the label, `compact`
   * (`compact-icons`) clamps it to two lines. Absent: label beside the glyph.
   */
  column?: "full" | "compact";
}

export interface ActionRibbonProps {
  /** Phase whose groups render (the chevrons' active phase). */
  phase: WorkspacePhase;
  /** Resolved action set (served configuration or the client fallback). */
  actions: WorkbenchActions;
  configStatus: RibbonConfigStatus;
  /**
   * The person's "Action bar" setting; default `extended-icons`. `dynamic`
   * renders `extended-icons` or `compact-icons` by the room the row has.
   */
  variant?: ActionBarStyle;
  /** The server's validation error when `configStatus` is `invalid`. */
  configError?: string | null;
  /** Refetch the configuration (fallback banner). */
  onRetryConfig?: () => void;
  /** Page-owned enablement per action, merged over the ribbon's own rules. */
  actionState?: (action: WorkbenchAction) => RibbonActionState | undefined;
  /** Every executable click lands here. */
  onAction?: (action: WorkbenchAction) => void;
  /** Action currently executing (its button shows a spinner). */
  pendingActionId?: string | null;
  /**
   * Replace a button entirely; `preview` actions render a `PreviewControl`
   * whose trigger IS the ribbon button. Return `undefined` to keep the
   * default button.
   */
  renderAction?: (
    action: WorkbenchAction,
    button: RibbonButtonShape,
    /** Test-id prefix of the rendered control (`ribbon` here; the Guided
     * panel passes its own so a control mounted twice stays unique). */
    idPrefix?: string,
  ) => ReactNode | undefined;
  /**
   * Workspace-bound controls at the row's right end, after the last group
   * (g5.workbench.shell-chrome). Rendered even when the phase has no groups
   * (Base). A function receives the rendered style, so the controls follow
   * the dynamic density.
   */
  trailing?: ReactNode | ((style: RibbonStyle) => ReactNode);
  /**
   * A shell control before the phase's first action (the Overview control,
   * workbench g5.workbench.landing), in the row's own button shape. A
   * function receives the rendered style.
   */
  leading?: ReactNode | ((style: RibbonStyle) => ReactNode);
  /** Extra actions after configured groups, using the rendered density. */
  endActions?: ReactNode | ((style: RibbonStyle) => ReactNode);
}

/**
 * Built-in actions whose label has a word too long for the standard icon
 * button; their button is wider so the word never breaks (owner choice,
 * 2026-11-01, ui g5.workbench.review-prompts).
 */
const WIDE_ACTION_IDS: ReadonlySet<string> = new Set(["rerun-verification"]);

/** A stable empty group list, so a phase without groups keeps its density. */
const NO_GROUPS: WorkbenchActions[WorkspacePhase] = [];

const FOCUS_CLASS =
  "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]";

/**
 * Shape of a ribbon button in each style; `primary` picks the phase tint.
 * Exported so the ribbon's workspace controls (g5.workbench.shell-chrome)
 * take the same shape as the actions in every style.
 */
export function ribbonButtonShape(
  variant: RibbonStyle,
  options: { primary?: boolean; wide?: boolean },
  hue = "var(--g5-info)",
  soft = "transparent",
): RibbonButtonShape {
  const action = { primary: options.primary === true };
  const primaryStyle = action.primary
    ? { backgroundColor: soft, borderColor: hue, color: hue }
    : undefined;
  switch (variant) {
    case "extended-icons":
      return {
        className: classNames(
          "flex flex-col items-center gap-1 rounded-[var(--g5-radius-sm)] px-1 py-1.5 text-center text-[11px] font-medium leading-tight",
          options.wide ? "w-[96px]" : "w-[76px]",
          "text-[var(--g5-text1)] hover:bg-[var(--g5-overlay-hover)] hover:text-[var(--g5-text0)]",
          "disabled:cursor-not-allowed disabled:opacity-60",
          FOCUS_CLASS,
        ),
        style: action.primary ? { color: hue } : undefined,
        iconSize: 22,
        labelHidden: false,
        column: "full",
      };
    case "compact-icons":
      // One step smaller than `extended-icons` (the dynamic style's shrink).
      return {
        className: classNames(
          "flex flex-col items-center gap-0.5 rounded-[var(--g5-radius-sm)] px-0.5 py-1 text-center text-[10px] font-medium leading-tight",
          options.wide ? "w-[80px]" : "w-[60px]",
          "text-[var(--g5-text1)] hover:bg-[var(--g5-overlay-hover)] hover:text-[var(--g5-text0)]",
          "disabled:cursor-not-allowed disabled:opacity-60",
          FOCUS_CLASS,
        ),
        style: action.primary ? { color: hue } : undefined,
        iconSize: 18,
        labelHidden: false,
        column: "compact",
      };
    case "minimal-icons":
      return {
        className: classNames(
          "inline-flex h-8 min-w-8 items-center justify-center gap-0.5 rounded-[var(--g5-radius-sm)] px-1.5",
          "text-[var(--g5-text1)] hover:bg-[var(--g5-overlay-hover)] hover:text-[var(--g5-text0)]",
          "disabled:cursor-not-allowed disabled:opacity-60",
          FOCUS_CLASS,
        ),
        style: action.primary
          ? { backgroundColor: soft, color: hue }
          : undefined,
        iconSize: 18,
        labelHidden: true,
      };
    default:
      return {
        className: classNames(
          "inline-flex h-7 items-center gap-1.5 whitespace-nowrap rounded-[var(--g5-radius-sm)] border px-2.5 text-[12px] font-medium",
          "disabled:cursor-not-allowed disabled:opacity-60",
          FOCUS_CLASS,
          action.primary
            ? ""
            : "border-[var(--g5-stroke)] bg-[var(--g5-bg2)] text-[var(--g5-text1)] hover:bg-[var(--g5-bg0)]",
        ),
        style: primaryStyle,
        iconSize: 13,
        labelHidden: false,
      };
  }
}

/** An item of a ribbon dropdown; the owner menu shares it. */
export const RIBBON_MENU_ITEM_CLASS =
  "flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-[12px] text-[var(--g5-text1)] hover:bg-[var(--g5-bg2)] disabled:cursor-not-allowed disabled:opacity-60";
const MENU_ITEM_CLASS = RIBBON_MENU_ITEM_CLASS;
/**
 * A ribbon dropdown's surface, below its button; placement is the caller's.
 * Under `dynamic` the ribbon swaps `absolute top-full mt-1` for a fixed
 * position, since its scrolling row would clip an in-flow dropdown.
 */
const MENU_SURFACE_CLASS =
  "flex min-w-[14rem] flex-col gap-0.5 rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] p-1 shadow-lg";
export const RIBBON_MENU_CLASS = `absolute top-full z-20 mt-1 ${MENU_SURFACE_CLASS}`;

/**
 * The ribbon's own enablement rules before the page's state applies. The
 * Guided panel (g5.workbench.journey) applies the same rules to its step
 * buttons, so a step never enables what the ribbon would not.
 */
export function ribbonActionState(
  action: WorkbenchAction,
  configStatus: RibbonConfigStatus,
): RibbonActionState {
  if (action.kind === "menu") {
    return action.items?.length
      ? {}
      : { disabled: true, title: RIBBON_UNSUPPORTED_TITLE };
  }
  if (action.kind === "chat") {
    if (!action.prompt) {
      return { disabled: true, title: RIBBON_UNSUPPORTED_TITLE };
    }
    if (configStatus === "fallback" || configStatus === "loading") {
      return { disabled: true, title: RIBBON_FALLBACK_CHAT_TITLE };
    }
    return {};
  }
  if (action.kind === "open") {
    if (!action.target || !KNOWN_TARGETS.has(action.target)) {
      return { disabled: true, title: RIBBON_UNSUPPORTED_TITLE };
    }
    if (action.target === "view" && !action.tab) {
      return { disabled: true, title: RIBBON_UNSUPPORTED_TITLE };
    }
    return {};
  }
  return { disabled: true, title: RIBBON_UNSUPPORTED_TITLE };
}

/**
 * A button in the ribbon's shape: the glyph, the label under or beside it
 * (hidden under `minimal-icons`, where it becomes the accessible name), and
 * an optional menu caret. The ribbon's actions and its workspace controls
 * (g5.workbench.shell-chrome) both render through it.
 */
export const RibbonShapedButton: React.FC<
  {
    shape: RibbonButtonShape;
    icon: ReactNode;
    label: string;
    /** Tooltip; the icon-only style always names the button here. */
    title?: string;
    menuProps?: { open: boolean; id: string };
    caret?: boolean;
    /**
     * Under the icon, render `head` on its own line over a truncating
     * `tail` instead of wrapping `label`, so a long value (an owner name)
     * never adds a third label line; `title` keeps the full text. The
     * text styles ignore it and truncate `label` as usual.
     */
    stacked?: { head: string; tail: string };
    /** Stable anchor for tests and flows. */
    testId: string;
  } & Omit<
    React.ButtonHTMLAttributes<HTMLButtonElement>,
    "title" | "children" | "className" | "style"
  > & { className?: string; style?: CSSProperties }
> = ({
  shape,
  icon,
  label,
  title,
  menuProps,
  caret: showCaret,
  stacked,
  testId,
  className,
  style,
  ...rest
}) => {
  const column = shape.labelHidden ? undefined : shape.column;
  // A compact label clamps to two lines; when it is cut, the full label
  // becomes the tooltip unless the button already carries one.
  const labelRef = useRef<HTMLSpanElement | null>(null);
  const [clamped, setClamped] = useState(false);
  useLayoutEffect(() => {
    const span = labelRef.current;
    setClamped(
      column === "compact" && !!span && span.scrollHeight > span.clientHeight,
    );
  }, [column, label]);
  const caret = showCaret ? (
    <ChevronDown
      size={shape.labelHidden || shape.column === "compact" ? 10 : 12}
      aria-hidden="true"
      className="shrink-0 opacity-80"
    />
  ) : null;
  return (
    <button
      type="button"
      data-testid={testId}
      title={
        shape.labelHidden
          ? title
            ? `${label} — ${title}`
            : label
          : (title ?? (clamped ? label : undefined))
      }
      aria-label={shape.labelHidden ? label : undefined}
      aria-haspopup={menuProps ? "menu" : undefined}
      aria-expanded={menuProps ? menuProps.open : undefined}
      aria-controls={menuProps ? menuProps.id : undefined}
      className={classNames(shape.className, className)}
      style={{ ...shape.style, ...style }}
      {...rest}
    >
      {icon}
      {shape.labelHidden ? null : column && stacked ? (
        <span className="flex min-w-0 max-w-full flex-col items-center">
          <span>{stacked.head} </span>
          <span className="flex min-w-0 max-w-full items-center gap-0.5">
            <span className="truncate">{stacked.tail}</span>
            {caret}
          </span>
        </span>
      ) : (
        <span
          ref={labelRef}
          // Under the icon the label wraps, anywhere if it must (two lines
          // at most in the compact column); beside it the label truncates.
          className={
            column === "compact"
              ? "line-clamp-2 min-w-0 max-w-full [overflow-wrap:anywhere]"
              : column
                ? "min-w-0 max-w-full [overflow-wrap:anywhere]"
                : "truncate"
          }
        >
          {label}
          {column && caret ? (
            <span className="ml-0.5 inline-flex align-[-1px]">{caret}</span>
          ) : null}
        </span>
      )}
      {column ? null : caret}
    </button>
  );
};

/**
 * The ribbon's own rules, then the page's: a disabled ribbon rule wins and
 * the page's state (reason or enablement) applies otherwise. The Guided panel
 * resolves its step buttons through the same function
 * (g5.workbench.journey).
 */
export function resolveRibbonActionState(
  action: WorkbenchAction,
  configStatus: RibbonConfigStatus,
  actionState?: (action: WorkbenchAction) => RibbonActionState | undefined,
): RibbonActionState {
  const own = ribbonActionState(action, configStatus);
  if (own.disabled) return own;
  return actionState?.(action) ?? own;
}

/** One action as a button in a given shape; the ribbon and the Guided panel share it. */
export const RibbonButton: React.FC<{
  action: WorkbenchAction;
  shape: RibbonButtonShape;
  state: RibbonActionState;
  pending: boolean;
  onClick: (event: React.MouseEvent<HTMLButtonElement>) => void;
  menuProps?: { open: boolean; id: string };
  /** Defaults to the ribbon's `ribbon-action-<id>`. */
  testId?: string;
}> = ({ action, shape, state, pending, onClick, menuProps, testId }) => {
  const Icon = workbenchActionIcon(action);
  return (
    <RibbonShapedButton
      shape={shape}
      label={action.label}
      // A disabled action's tooltip is its reason.
      title={state.title}
      disabled={state.disabled || pending}
      testId={testId ?? `ribbon-action-${action.id}`}
      data-kind={action.kind}
      data-primary={action.primary ? "true" : undefined}
      menuProps={menuProps}
      caret={action.kind === "menu"}
      onClick={onClick}
      icon={
        pending ? (
          <Loader2
            size={shape.iconSize}
            className="shrink-0 animate-spin"
            aria-hidden="true"
          />
        ) : (
          <Icon
            size={shape.iconSize}
            className="shrink-0"
            aria-hidden="true"
            {...(action.primary && shape.iconSize > 13
              ? { style: { strokeWidth: 2.25 } }
              : {})}
          />
        )
      }
    />
  );
};

export const ActionRibbon: React.FC<ActionRibbonProps> = ({
  phase,
  actions,
  configStatus,
  variant = "extended-icons",
  configError,
  onRetryConfig,
  actionState,
  onAction,
  pendingActionId,
  renderAction,
  endActions,
  leading,
  trailing,
}) => {
  const info = WORKBENCH_PHASES[phase];
  const hue = `var(${info.token})`;
  const soft = `var(${info.softToken})`;
  const groups = actions[phase] ?? NO_GROUPS;
  const [openMenu, setOpenMenu] = useState<string | null>(null);
  const rootRef = useRef<HTMLDivElement | null>(null);
  const outerRef = useRef<HTMLDivElement | null>(null);
  const groupsRef = useRef<HTMLDivElement | null>(null);

  // A menu closes on an outside click or Escape, like every dropdown.
  const closeMenu = useCallback(() => setOpenMenu(null), []);
  useDismissOnOutside(rootRef, openMenu !== null, closeMenu);

  // Dynamic density: `full` until the groups overflow, then `compact` until
  // the row is back at the width (`need`) where the full layout just fit.
  // The outer row's width does not depend on the density, so the two rules
  // cannot oscillate. A new phase, action set, variant, or change in which
  // final actions/workspace controls are present starts over at `full`.
  // The reset happens during render; the layout effect measures before
  // paint, so nothing flashes. Any overflow (not a
  // 1px tolerance) goes compact, so `need` is exactly the width where full
  // fits and the state at a width never depends on the resize direction.
  const dynamic = variant === "dynamic";
  const hasTrailing = Boolean(trailing);
  const hasEndActions = Boolean(endActions);
  const [densityState, setDensityState] = useState<{
    groups: typeof groups;
    variant: ActionBarStyle;
    hasTrailing: boolean;
    hasEndActions: boolean;
    density: "full" | "compact";
  }>({ groups, variant, hasTrailing, hasEndActions, density: "full" });
  const density =
    densityState.groups === groups &&
    densityState.variant === variant &&
    densityState.hasTrailing === hasTrailing &&
    densityState.hasEndActions === hasEndActions
      ? densityState.density
      : "full";
  const needRef = useRef<number | null>(null);
  const measure = useCallback(() => {
    const outer = outerRef.current;
    const row = groupsRef.current;
    if (!outer || !row) return;
    if (density === "full") {
      const over = row.scrollWidth - row.clientWidth;
      if (over > 0) {
        needRef.current = outer.clientWidth + over;
        setDensityState({
          groups,
          variant,
          hasTrailing,
          hasEndActions,
          density: "compact",
        });
      }
    } else if (
      needRef.current !== null &&
      outer.clientWidth >= needRef.current
    ) {
      setDensityState({
        groups,
        variant,
        hasTrailing,
        hasEndActions,
        density: "full",
      });
    }
  }, [density, groups, hasTrailing, hasEndActions, variant]);
  useLayoutEffect(() => {
    if (dynamic) measure();
  }, [dynamic, measure]);
  useEffect(() => {
    if (!dynamic || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => measure());
    // The outer row for the viewport; the groups row for trailing controls
    // that change width inside it.
    if (outerRef.current) observer.observe(outerRef.current);
    if (groupsRef.current) observer.observe(groupsRef.current);
    return () => observer.disconnect();
  }, [dynamic, measure]);

  const style: RibbonStyle = dynamic
    ? density === "compact"
      ? "compact-icons"
      : "extended-icons"
    : variant;

  // Under `dynamic` an open menu is fixed below its trigger (the scrolling
  // row would clip it) while staying the trigger's DOM descendant, so test
  // ids and the outside-click boundary are unchanged. It closes when the row
  // scrolls or the window resizes, where the taken position goes stale.
  const [menuPos, setMenuPos] = useState<{ left: number; top: number } | null>(
    null,
  );
  const menuRef = useRef<HTMLDivElement | null>(null);
  useLayoutEffect(() => {
    // Keep a fixed menu inside the viewport's right edge.
    const menu = menuRef.current;
    if (!dynamic || !menuPos || !menu) return;
    const overflow =
      menu.getBoundingClientRect().right - (window.innerWidth - 8);
    const left = Math.max(8, menuPos.left - Math.max(0, overflow));
    if (left !== menuPos.left) setMenuPos({ ...menuPos, left });
  }, [dynamic, menuPos]);
  useEffect(() => {
    if (!dynamic || openMenu === null) return;
    const row = groupsRef.current;
    const scrollLeft = row?.scrollLeft ?? 0;
    const onScroll = () => {
      if (row && row.scrollLeft !== scrollLeft) setOpenMenu(null);
    };
    row?.addEventListener("scroll", onScroll);
    window.addEventListener("resize", closeMenu);
    return () => {
      row?.removeEventListener("scroll", onScroll);
      window.removeEventListener("resize", closeMenu);
    };
  }, [closeMenu, dynamic, openMenu]);

  const stateOf = useCallback(
    (action: WorkbenchAction): RibbonActionState =>
      resolveRibbonActionState(action, configStatus, actionState),
    [actionState, configStatus],
  );

  const renderItem = (item: WorkbenchAction) => {
    const state = stateOf(item);
    const pending = pendingActionId === item.id;
    const custom =
      !state.disabled && renderAction
        ? renderAction(item, {
            className: MENU_ITEM_CLASS,
            iconSize: 12,
            labelHidden: false,
          })
        : undefined;
    if (custom !== undefined) {
      return (
        <span
          key={item.id}
          role="menuitem"
          data-testid={`ribbon-action-${item.id}`}
          data-kind={item.kind}
        >
          {custom}
        </span>
      );
    }
    return (
      <button
        key={item.id}
        type="button"
        role="menuitem"
        disabled={state.disabled || pending}
        title={state.title}
        data-testid={`ribbon-action-${item.id}`}
        data-kind={item.kind}
        onClick={() => {
          setOpenMenu(null);
          onAction?.(item);
        }}
        className={MENU_ITEM_CLASS}
      >
        {pending ? (
          <Loader2 size={12} className="animate-spin" aria-hidden="true" />
        ) : null}
        {item.label}
      </button>
    );
  };

  const renderButton = (action: WorkbenchAction) => {
    // A hidden action is started by id elsewhere (the Review meeting dialog
    // runs `extract-decisions`); the ribbon never draws it.
    if (action.hidden) return null;
    const state = stateOf(action);
    const pending = pendingActionId === action.id;
    const shape = ribbonButtonShape(
      style,
      { primary: action.primary, wide: WIDE_ACTION_IDS.has(action.id) },
      hue,
      soft,
    );
    if (action.kind === "menu") {
      const menuId = `ribbon-menu-${action.id}`;
      const open = openMenu === action.id;
      const fixed = dynamic && open && menuPos !== null;
      return (
        <div key={action.id} className="relative">
          <RibbonButton
            action={action}
            shape={shape}
            state={state}
            pending={false}
            menuProps={{ open, id: menuId }}
            onClick={(event) => {
              if (dynamic && !open) {
                const rect = event.currentTarget.getBoundingClientRect();
                setMenuPos({ left: rect.left, top: rect.bottom + 4 });
              }
              setOpenMenu(open ? null : action.id);
            }}
          />
          {open ? (
            <div
              ref={menuRef}
              id={menuId}
              role="menu"
              aria-label={action.label}
              data-testid={menuId}
              className={
                fixed
                  ? classNames("fixed z-50", MENU_SURFACE_CLASS)
                  : classNames(RIBBON_MENU_CLASS, "left-0")
              }
              style={
                fixed ? { left: menuPos.left, top: menuPos.top } : undefined
              }
            >
              {(action.items ?? []).map(renderItem)}
            </div>
          ) : null}
        </div>
      );
    }
    const custom =
      !state.disabled && renderAction ? renderAction(action, shape) : undefined;
    if (custom !== undefined) {
      return (
        <span
          key={action.id}
          data-testid={`ribbon-action-${action.id}`}
          data-kind={action.kind}
          className="inline-flex"
        >
          {custom}
        </span>
      );
    }
    return (
      <RibbonButton
        key={action.id}
        action={action}
        shape={shape}
        state={state}
        pending={pending}
        onClick={() => onAction?.(action)}
      />
    );
  };

  const captioned = style === "extended-text";
  const columns = style === "extended-icons" || style === "compact-icons";
  // Divider-separated groups in the uncaptioned styles; captioned groups
  // keep their own column with the caption above. A group wraps inside
  // itself too, so a large committed group never pushes actions off-screen;
  // under `dynamic` it never wraps (the row scrolls instead).
  const groupClass = captioned
    ? "flex flex-col gap-1"
    : classNames(
        "flex items-start gap-1 border-r border-[var(--g5-stroke)] px-2.5 first:pl-0 last:border-r-0 last:pr-0",
        dynamic ? "shrink-0 flex-nowrap" : "flex-wrap",
        columns ? "gap-0.5" : "items-center",
      );
  // The fixed styles wrap rather than scroll: a scrolling container would
  // clip the absolutely positioned dropdown menus. The groups wrap inside
  // their own container so the trailing controls keep the first row's right
  // end. `dynamic` scrolls instead (its menus are fixed) and the trailing
  // controls stay outside the scroller.
  const rowClass = classNames(
    "flex min-w-0 flex-1",
    dynamic
      ? "flex-nowrap items-stretch overflow-x-auto overflow-y-hidden [scrollbar-width:thin]"
      : "flex-wrap",
    dynamic
      ? ""
      : captioned
        ? "items-end gap-x-4 gap-y-2"
        : columns
          ? "items-stretch gap-y-1.5"
          : "items-center gap-y-1.5",
  );

  return (
    <div
      ref={rootRef}
      role="toolbar"
      aria-label={`${info.name} actions`}
      data-testid="action-ribbon"
      data-phase={phase}
      data-variant={variant}
      data-density={dynamic ? density : undefined}
      data-config-status={configStatus}
      className="flex min-w-0 shrink-0 flex-col border-b border-[var(--g5-stroke)] bg-[var(--g5-bg1)]"
    >
      {configStatus === "invalid" ? (
        <div
          role="alert"
          data-testid="ribbon-config-invalid"
          className="flex items-center gap-2 border-b border-[var(--g5-blocked)]/40 bg-[var(--g5-blocked)]/10 px-3 py-1.5 text-[12px] text-[var(--g5-text0)]"
        >
          <AlertTriangle size={13} aria-hidden="true" className="shrink-0" />
          <span className="min-w-0 truncate" title={configError ?? undefined}>
            Workbench configuration invalid: {configError ?? "unknown error"} —
            showing the built-in actions.
          </span>
        </div>
      ) : null}
      {configStatus === "fallback" ? (
        <div
          role="status"
          data-testid="ribbon-config-fallback"
          className="flex items-center gap-2 border-b border-[var(--g5-stroke)] bg-[var(--g5-bg2)] px-3 py-1.5 text-[12px] text-[var(--g5-text1)]"
        >
          <AlertTriangle size={13} aria-hidden="true" className="shrink-0" />
          <span className="min-w-0 truncate">
            Workbench configuration unavailable — showing the built-in fallback;
            templated chats are disabled.
          </span>
          {onRetryConfig ? (
            <button
              type="button"
              data-testid="ribbon-config-retry"
              onClick={onRetryConfig}
              className="ml-auto underline"
            >
              Retry
            </button>
          ) : null}
        </div>
      ) : null}
      <div
        ref={outerRef}
        className={classNames(
          "flex items-start px-3",
          captioned ? "py-2" : "py-1.5",
        )}
      >
        <div ref={groupsRef} data-testid="ribbon-groups" className={rowClass}>
          {leading ? (
            <div data-testid="ribbon-leading" className={groupClass}>
              {captioned ? (
                <span className="text-[10px] font-semibold uppercase tracking-wide text-[var(--g5-text2)]">
                  Workspace
                </span>
              ) : null}
              {typeof leading === "function" ? leading(style) : leading}
            </div>
          ) : null}
          {groups.map((group) => (
            <div
              key={group.name}
              role="group"
              aria-label={group.name}
              data-testid={`ribbon-group-${group.name.toLowerCase().replace(/\s+/g, "-")}`}
              className={groupClass}
            >
              {captioned ? (
                <span className="text-[10px] font-semibold uppercase tracking-wide text-[var(--g5-text2)]">
                  {group.name}
                </span>
              ) : null}
              {captioned ? (
                <div className="flex flex-wrap items-center gap-1.5">
                  {group.actions.map(renderButton)}
                </div>
              ) : (
                group.actions.map(renderButton)
              )}
            </div>
          ))}
          {endActions ? (
            <div data-testid="ribbon-end-actions" className={groupClass}>
              {typeof endActions === "function"
                ? endActions(style)
                : endActions}
            </div>
          ) : null}
        </div>
        {trailing ? (
          <div
            data-testid="ribbon-trailing"
            className={classNames(
              // Shrinkable: a long owner label wraps inside the controls
              // instead of pushing them past the row.
              "ml-auto flex min-w-0 max-w-full shrink items-start gap-0.5 pl-2.5",
              groups.length > 0 && "border-l border-[var(--g5-stroke)]",
              captioned ? "self-end" : "",
            )}
          >
            {typeof trailing === "function" ? trailing(style) : trailing}
          </div>
        ) : null}
      </div>
    </div>
  );
};
