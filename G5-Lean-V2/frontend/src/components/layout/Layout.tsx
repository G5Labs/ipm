// =============================================================================
// G5 Design System — Layout Components
// =============================================================================
// AppShell and TopBar.
//
// These components provide the structural scaffolding for the G5 workspace UI.
// All use CSS custom properties for colors/spacing to support instant theme switching.
// =============================================================================

import React from "react";
import { useWorkspaceTabs } from "../../hooks/useWorkspaceTabs";
import { classNames } from "../ui/UI";
import { WorkspaceTabStrip } from "./WorkspaceTabStrip";

// ---------------------------------------------------------------------------
// AppShell — Full-screen flex column container
// ---------------------------------------------------------------------------

export interface AppShellProps extends React.HTMLAttributes<HTMLDivElement> {
  children: React.ReactNode;
  /**
   * The page's global controls for the right end of the workspace tab strip
   * (workbench g5.workbench.shell-chrome); rendered only while the strip is.
   */
  shellControls?: React.ReactNode;
}

const SelfHostedPreProductionBannerContext = React.createContext(false);

export interface SelfHostedPreProductionBannerProviderProps {
  enabled: boolean;
  children: React.ReactNode;
}

export const SelfHostedPreProductionBannerProvider: React.FC<
  SelfHostedPreProductionBannerProviderProps
> = ({ enabled, children }) => (
  <SelfHostedPreProductionBannerContext.Provider value={enabled}>
    {children}
  </SelfHostedPreProductionBannerContext.Provider>
);

/**
 * Full-screen flex column container that wraps the entire application.
 * Applies h-screen, flex flex-col, and --g5-bg0 background.
 * Also applies the optional --g5-texture background image for themes
 * that use textures (Obsidian & Gold, Darkroom, Ink & Parchment).
 */
export const AppShell: React.FC<AppShellProps> = ({
  children,
  shellControls,
  className: extraClassName,
  style: extraStyle,
  ...rest
}) => {
  const showPreProductionBanner = React.useContext(
    SelfHostedPreProductionBannerContext,
  );
  // The workspace tab strip (workbench g5.workbench.tabs) sits above every
  // shell page; the provider decides whether it shows at all.
  const workspaceTabs = useWorkspaceTabs();

  return (
    <div
      className={classNames(
        "h-screen flex flex-col",
        "bg-[var(--g5-bg0)]",
        "text-[var(--g5-text0)]",
        "font-sans",
        "overflow-hidden",
        extraClassName,
      )}
      style={{ backgroundImage: "var(--g5-texture)", ...extraStyle }}
      data-surface="shell"
      {...rest}
    >
      {showPreProductionBanner && (
        <div
          role="status"
          aria-label="Pre Production deployment"
          className="shrink-0 bg-red-700 px-4 py-2 text-center text-base font-bold text-white md:text-lg"
        >
          Pre Production deployment
        </div>
      )}
      {workspaceTabs.enabled && <WorkspaceTabStrip trailing={shellControls} />}
      {children}
    </div>
  );
};

// ---------------------------------------------------------------------------
// TopBar — Fixed top bar (44px height)
// ---------------------------------------------------------------------------

export interface TopBarProps {
  children: React.ReactNode;
}

/**
 * Fixed-height top bar (44px / h-11) for breadcrumbs, action buttons,
 * and connection indicators.
 * Background: --g5-bg2, border-bottom: --g5-stroke.
 */
export const TopBar: React.FC<TopBarProps> = ({ children }) => {
  return (
    <header
      data-surface="titlebar"
      className={classNames(
        "flex items-center shrink-0",
        "h-11",
        "px-3",
        "bg-[var(--g5-bg2)]",
        "border-b border-[var(--g5-stroke)]",
      )}
      style={{ borderBottomWidth: "var(--g5-border-w)" }}
    >
      {children}
    </header>
  );
};
