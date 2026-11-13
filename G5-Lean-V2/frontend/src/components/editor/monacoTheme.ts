// =============================================================================
// G5 Editor — monacoTheme
// =============================================================================
// The light/dark mode the page shows and the g5 Monaco themes built from the
// current CSS tokens, shared by every Monaco surface (the code editor and the
// Compare dialog).
// =============================================================================

import { useEffect, useState } from "react";

// ---------------------------------------------------------------------------
// useThemeMode — observes <html data-mode> for light/dark switching
// ---------------------------------------------------------------------------

export function useThemeMode(): "light" | "dark" {
  const [mode, setMode] = useState<"light" | "dark">(() => {
    return (
      (document.documentElement.dataset.mode as "light" | "dark") || "dark"
    );
  });

  useEffect(() => {
    const observer = new MutationObserver(() => {
      const next = document.documentElement.dataset.mode as "light" | "dark";
      setMode(next || "dark");
    });
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["data-mode"],
    });
    return () => observer.disconnect();
  }, []);

  return mode;
}

/**
 * Read a CSS custom property from the document root.
 */
function cssVar(name: string): string {
  return getComputedStyle(document.documentElement)
    .getPropertyValue(name)
    .trim();
}

/** Shared token rules for both g5 Monaco themes — reads current CSS vars. */
function g5TokenRules() {
  return [
    { token: "keyword", foreground: cssVar("--g5-syn-keyword") },
    { token: "string", foreground: cssVar("--g5-syn-string") },
    { token: "number", foreground: cssVar("--g5-syn-number") },
    {
      token: "comment",
      foreground: cssVar("--g5-syn-comment"),
      fontStyle: "italic",
    },
    { token: "type", foreground: cssVar("--g5-syn-type") },
    { token: "identifier", foreground: cssVar("--g5-syn-variable") },
    { token: "delimiter", foreground: cssVar("--g5-syn-punctuation") },
    { token: "operator", foreground: cssVar("--g5-syn-operator") },
    { token: "tag", foreground: cssVar("--g5-syn-tag") },
    { token: "attribute.name", foreground: cssVar("--g5-syn-attribute") },
  ].map((r) => ({ ...r, foreground: rgbToHex(r.foreground) }));
}

/** Shared editor colours for both g5 Monaco themes — reads current CSS vars. */
function g5EditorColors() {
  return {
    "editor.background": rgbToHexHash(
      cssVar("--g5-editor") || cssVar("--g5-bg1"),
    ),
    "editor.foreground": rgbToHexHash(cssVar("--g5-text0")),
    "editor.lineHighlightBackground": rgbToHexHash(
      cssVar("--g5-overlay-hover") || cssVar("--g5-bg2"),
    ),
    "editorLineNumber.foreground": rgbToHexHash(
      cssVar("--g5-linenum") || cssVar("--g5-text2"),
    ),
    "editorGutter.background": rgbToHexHash(
      cssVar("--g5-gutter") || cssVar("--g5-bg1"),
    ),
  };
}

/**
 * Register g5-light and g5-dark Monaco themes using current CSS custom
 * properties.  Called each time the document theme/mode changes so
 * colours stay in sync.
 */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export function defineG5Themes(monaco: any): void {
  monaco.editor.defineTheme("g5-light", {
    base: "vs",
    inherit: true,
    rules: g5TokenRules(),
    colors: g5EditorColors(),
  });
  monaco.editor.defineTheme("g5-dark", {
    base: "vs-dark",
    inherit: true,
    rules: g5TokenRules(),
    colors: g5EditorColors(),
  });
}

/** Convert an `rgb(r g b)` or `rgb(r, g, b)` string to a bare hex string (no #). */
function rgbToHex(rgb: string): string {
  return rgbToHexHash(rgb).replace("#", "");
}

/**
 * Convert a colour read from a `--g5-*` token to the `#rrggbb` (or
 * `#rrggbbaa`) form Monaco's token colour map accepts.
 *
 * Tokens are authored as rgb/rgba (alpha is preserved), but the production
 * build minifies CSS and rewrites some values as hex, including the 3-digit
 * short form: Aero Glass light's `--g5-editor: rgb(255 255 255)` reaches the
 * page as `#fff`, which Monaco rejects ("Illegal value for token color") and
 * takes the editor down with it. Short hex is expanded; 6- and 8-digit hex
 * passes through; anything else falls back to black.
 */
export function rgbToHexHash(rgb: string): string {
  if (!rgb) return "#000000";
  if (rgb.startsWith("#")) {
    const hex = rgb.slice(1);
    if (/^[0-9a-fA-F]{3,4}$/.test(hex)) {
      return "#" + [...hex].map((c) => c + c).join("");
    }
    return /^[0-9a-fA-F]{6}([0-9a-fA-F]{2})?$/.test(hex) ? rgb : "#000000";
  }
  const match = rgb.match(/^rgba?\((.*)\)$/i);
  if (!match) return "#000000";
  const channels = match[1].trim().split(/[\s,/]+/);
  if (channels.length < 3 || channels.length > 4) return "#000000";
  const bytes = channels.map((channel, index) => {
    const value = parseFloat(channel);
    const scale = channel.endsWith("%") ? 255 / 100 : index === 3 ? 255 : 1;
    return Math.max(0, Math.min(255, Math.round(value * scale)));
  });
  if (bytes.some((value) => !Number.isFinite(value))) return "#000000";
  return (
    "#" + bytes.map((value) => value.toString(16).padStart(2, "0")).join("")
  );
}
