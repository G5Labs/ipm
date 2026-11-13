// =============================================================================
// G5 Client — Default Configuration Constants
// =============================================================================
// Static configuration values that are NOT mock data. These are genuine
// application defaults used in production: theme options, default user
// settings, etc.
// =============================================================================

import type { ThemeName, UserSettings } from "./types";
import { catalogDefaultChoiceFor } from "./harnessPicker";

// ---------------------------------------------------------------------------
// Theme Options
// ---------------------------------------------------------------------------

export interface ThemeOption {
  name: ThemeName;
  label: string;
  accent: string;
}

export const THEME_OPTIONS: ThemeOption[] = [
  { name: "editor", label: "Editor", accent: "#3b82f6" },
  { name: "signal-studio", label: "Signal Studio", accent: "#8b5cf6" },
  { name: "obsidian-gold", label: "Obsidian & Gold", accent: "#d97706" },
  { name: "carbon-clay", label: "Carbon Clay", accent: "#ef4444" },
  { name: "alpine-graphite", label: "Alpine Graphite", accent: "#059669" },
  { name: "darkroom", label: "Darkroom", accent: "#f59e0b" },
  { name: "ink-parchment", label: "Ink & Parchment", accent: "#78716c" },
  { name: "studio-night", label: "Studio Night", accent: "#ec4899" },
  { name: "pm-paper", label: "PM Paper", accent: "#6366f1" },
  { name: "green-room", label: "Green room", accent: "#1ed760" },
  { name: "classic-98", label: "Old school", accent: "#000080" },
  { name: "aero-glass", label: "Aero Glass", accent: "#4da3ff" },
];

// ---------------------------------------------------------------------------
// Default User Settings
// ---------------------------------------------------------------------------

export const DEFAULT_SETTINGS: UserSettings = {
  themeName: "editor",
  themeMode: "dark",
  // Per-harness catalog defaults (#1342 plan Story 3, D-6/D-12).
  defaultAgentProviderChoices: {
    claude_code: catalogDefaultChoiceFor("claude_code"),
    codex: catalogDefaultChoiceFor("codex"),
  },
  // The full sidebar that collapses to the icon rail (ui g5.ui.sidebar-default).
  sidebarDisplayMode: "collapse-to-rail",
  // Composer usage chip defaults to the 7-day window (agent_interaction_ux D8).
  subscriptionUsageWindow: "7d",
  // Workbench header styles (ui g5.workbench.header-styles).
  phaseBarStyle: "minimal",
  actionBarStyle: "dynamic",
  // The center tab strip stays hidden until the person turns it on
  // (workspaces g5.workspace.ui.center-tabs).
  showTabBar: false,
  // The Guided section stays hidden until the person turns it on
  // (ui g5.ui.guided-toggle).
  showGuidedJourney: false,
  // Five rows per Home tile until the person raises it (workbench
  // g5.workbench.home-workspaces).
  homeRowsPerTile: 5,
  // 3D robot heads above the Chat corner button, five before the rest fold
  // into a count (ui g5.workspace.ui.chat-bubble-stack).
  chatBubbleStyle: "3d",
  chatBubbleLimit: 5,
  budgetWarnAt: 80,
  editor: {
    editorFontSize: 14,
    editorTabSize: 4,
    rainbowBrackets: true,
    codeFolding: true,
    showMinimap: false,
    wordWrap: "off",
    indentGuides: true,
    bracketPairGuides: true,
    ligatures: false,
    stickyScroll: true,
    renderWhitespace: "selection",
  },
};
