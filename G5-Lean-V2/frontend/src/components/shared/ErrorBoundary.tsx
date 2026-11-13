// =============================================================================
// G5 Shared Components — ErrorBoundary
// =============================================================================
// A React error boundary component that wraps entire pages. Catches React
// render errors and displays a styled error panel.
//
// This is the ONLY class component in the G5 client — required because
// React error boundaries must use componentDidCatch lifecycle method.
// =============================================================================

import {
  AlertTriangle,
  Bug,
  ChevronDown,
  ChevronRight,
  RotateCcw,
} from "lucide-react";
import React from "react";
import { Button, classNames } from "../ui/UI";

// ---------------------------------------------------------------------------
// Props & State
// ---------------------------------------------------------------------------

export interface ErrorBoundaryProps {
  children: React.ReactNode;
  fallback?: React.ReactNode;
}

export interface ErrorBoundaryState {
  hasError: boolean;
  error: Error | null;
  errorInfo: React.ErrorInfo | null;
}

// ---------------------------------------------------------------------------
// ErrorBoundary Component (Class Component)
// ---------------------------------------------------------------------------

/**
 * React error boundary that catches render errors and displays a styled panel.
 *
 * Behavior:
 * - Catches errors thrown during rendering, in lifecycle methods, and in
 *   constructors of child components.
 * - Displays a styled panel with:
 *   - Error icon and "Something went wrong" heading
 *   - Error message text
 *   - Stack trace in a collapsible monospace block
 *   - "Reload" button to refresh the page
 *   - "Report" button (placeholder for error reporting)
 * - Uses g5 design tokens for styling.
 */
export class ErrorBoundary extends React.Component<
  ErrorBoundaryProps,
  ErrorBoundaryState
> {
  constructor(props: ErrorBoundaryProps) {
    super(props);
    this.state = {
      hasError: false,
      error: null,
      errorInfo: null,
    };
  }

  static getDerivedStateFromError(error: Error): Partial<ErrorBoundaryState> {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, errorInfo: React.ErrorInfo): void {
    this.setState({ error, errorInfo });
    // Future: send error to reporting service
    console.error("[ErrorBoundary] Caught error:", error, errorInfo);
  }

  handleReload = (): void => {
    window.location.reload();
  };

  handleReport = (): void => {
    // Placeholder for error reporting integration
    const errorData = {
      message: this.state.error?.message,
      stack: this.state.error?.stack,
      componentStack: this.state.errorInfo?.componentStack,
    };
    console.info("[ErrorBoundary] Error report:", errorData);
    // Future: POST to error reporting endpoint
  };

  render(): React.ReactNode {
    if (this.state.hasError) {
      // Use custom fallback if provided
      if (this.props.fallback) {
        return this.props.fallback;
      }

      return (
        <ErrorPanel
          error={this.state.error}
          errorInfo={this.state.errorInfo}
          onReload={this.handleReload}
          onReport={this.handleReport}
        />
      );
    }

    return this.props.children;
  }
}

// ---------------------------------------------------------------------------
// ErrorPanel — Styled error display (functional sub-component)
// ---------------------------------------------------------------------------

interface ErrorPanelProps {
  error: Error | null;
  errorInfo: React.ErrorInfo | null;
  onReload: () => void;
  onReport: () => void;
}

const ErrorPanel: React.FC<ErrorPanelProps> = ({
  error,
  errorInfo,
  onReload,
  onReport,
}) => {
  const [showStack, setShowStack] = React.useState(false);

  const stackTrace = [
    error?.stack,
    errorInfo?.componentStack
      ? `\nComponent Stack:${errorInfo.componentStack}`
      : "",
  ]
    .filter(Boolean)
    .join("\n");

  return (
    <div
      className={classNames(
        "flex items-center justify-center",
        "min-h-[300px] w-full p-8",
        "bg-[var(--g5-bg0)]",
      )}
    >
      <div
        className={classNames(
          "max-w-lg w-full",
          "rounded-[var(--g5-radius-lg)]",
          "bg-[var(--g5-bg1)]",
          "border border-[var(--g5-stroke)]",
          "overflow-hidden",
        )}
        style={{
          borderWidth: "var(--g5-border-w)",
          boxShadow: "var(--shadow-pane)",
        }}
      >
        {/* Header */}
        <div
          className={classNames(
            "flex items-center gap-3 px-5 py-4",
            "border-b border-[var(--g5-stroke)]",
          )}
          style={{ borderBottomWidth: "var(--g5-border-w)" }}
        >
          <div
            className="w-10 h-10 rounded-full flex items-center justify-center shrink-0"
            style={{
              backgroundColor:
                "color-mix(in srgb, var(--g5-blocked) 15%, transparent)",
            }}
          >
            <AlertTriangle
              className="w-5 h-5"
              style={{ color: "var(--g5-blocked)" }}
            />
          </div>
          <div>
            <h2 className="text-base font-semibold text-[var(--g5-text0)]">
              Something went wrong
            </h2>
            <p className="text-sm text-[var(--g5-text2)] mt-0.5">
              An unexpected error occurred while rendering this page.
            </p>
          </div>
        </div>

        {/* Error message */}
        <div className="px-5 py-4">
          {error?.message && (
            <div
              className={classNames(
                "px-3 py-2 mb-4",
                "rounded-[var(--g5-radius-sm)]",
                "text-sm font-mono",
                "text-[var(--g5-blocked)]",
              )}
              style={{
                backgroundColor:
                  "color-mix(in srgb, var(--g5-blocked) 8%, transparent)",
              }}
            >
              {error.message}
            </div>
          )}

          {/* Collapsible stack trace */}
          {stackTrace && (
            <div className="mb-4">
              <button
                type="button"
                data-testid="error-stack-toggle"
                className={classNames(
                  "flex items-center gap-1.5 text-xs text-[var(--g5-text2)]",
                  "hover:text-[var(--g5-text1)]",
                  "cursor-pointer",
                  "transition-colors duration-150",
                )}
                onClick={() => setShowStack(!showStack)}
              >
                {showStack ? (
                  <ChevronDown className="w-3 h-3" />
                ) : (
                  <ChevronRight className="w-3 h-3" />
                )}
                Stack trace
              </button>

              {showStack && (
                <pre
                  className={classNames(
                    "mt-2 p-3",
                    "rounded-[var(--g5-radius-sm)]",
                    "bg-[var(--g5-bg0)]",
                    "border border-[var(--g5-stroke)]",
                    "text-xs leading-relaxed",
                    "text-[var(--g5-text2)]",
                    "overflow-x-auto max-h-60 overflow-y-auto",
                  )}
                  style={{
                    fontFamily: "var(--g5-font-mono)",
                    borderWidth: "var(--g5-border-w)",
                  }}
                >
                  {stackTrace}
                </pre>
              )}
            </div>
          )}

          {/* Action buttons */}
          <div className="flex items-center gap-3">
            <Button
              kind="primary"
              data-testid="error-reload-button"
              onClick={onReload}
            >
              <RotateCcw className="w-3.5 h-3.5" />
              Reload
            </Button>
            <Button
              kind="secondary"
              data-testid="error-report-button"
              onClick={onReport}
            >
              <Bug className="w-3.5 h-3.5" />
              Report
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
};
