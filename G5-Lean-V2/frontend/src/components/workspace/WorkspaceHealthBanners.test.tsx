import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { AsyncState, ScopedConfig } from "../../lib/types";
import {
  hasLlmCredential,
  LLM_CREDENTIAL_SECRET_KEYS,
  WorkspaceHealthBanners,
} from "./WorkspaceHealthBanners";

function configFixture(
  effectiveSecretKeys: string[],
  effectiveSettings: Record<string, unknown> = {
    codex_subscription_enabled: false,
  },
): AsyncState<ScopedConfig> {
  return {
    status: "ok",
    data: {
      scope: "workspace",
      settings: {},
      secretKeys: [],
      effectiveSettings,
      effectiveSecretKeys,
      updatedBy: null,
      updatedAt: null,
    },
  };
}

describe("WorkspaceHealthBanners", () => {
  it("renders nothing when an LLM credential is configured", () => {
    render(
      <WorkspaceHealthBanners
        workspaceConfig={configFixture(["ANTHROPIC_AUTH_TOKEN"])}
        onOpenSettings={vi.fn()}
      />,
    );
    expect(
      screen.queryByTestId("workspace-health-banners"),
    ).not.toBeInTheDocument();
  });

  it("links the missing-credentials warning to workspace settings", async () => {
    const onOpenSettings = vi.fn();
    render(
      <WorkspaceHealthBanners
        workspaceConfig={configFixture([])}
        onOpenSettings={onOpenSettings}
      />,
    );

    const link = screen.getByRole("link", { name: "Configure them here" });
    expect(link).toHaveAttribute("href", "?view=settings");
    await userEvent.click(link);
    expect(onOpenSettings).toHaveBeenCalledTimes(1);
  });

  it("links to the Settings URL the page gives it, for copy and new-tab opens", () => {
    render(
      <WorkspaceHealthBanners
        workspaceConfig={configFixture([])}
        settingsHref="/acme/demo?view=settings"
        onOpenSettings={vi.fn()}
      />,
    );
    expect(
      screen.getByRole("link", { name: "Configure them here" }),
    ).toHaveAttribute("href", "/acme/demo?view=settings");
  });

  it("stays silent while config is loading or malformed", () => {
    const { rerender } = render(
      <WorkspaceHealthBanners
        workspaceConfig={{ status: "loading" }}
        onOpenSettings={vi.fn()}
      />,
    );
    expect(
      screen.queryByTestId("workspace-health-banners"),
    ).not.toBeInTheDocument();

    rerender(
      <WorkspaceHealthBanners
        workspaceConfig={
          { status: "ok", data: [] } as unknown as AsyncState<ScopedConfig>
        }
        onOpenSettings={vi.fn()}
      />,
    );
    expect(
      screen.queryByTestId("workspace-health-banners"),
    ).not.toBeInTheDocument();
  });

  it("recognizes provider credentials and the direct Codex subscription route", () => {
    for (const key of LLM_CREDENTIAL_SECRET_KEYS) {
      expect(
        hasLlmCredential([key], { codex_subscription_enabled: false }),
      ).toBe(true);
    }
    expect(hasLlmCredential([], {})).toBe(true);
    expect(
      hasLlmCredential([], {
        OPENAI_BASE_URL: "https://openrouter.ai/api/v1",
        codex_subscription_enabled: true,
      }),
    ).toBe(false);
    expect(hasLlmCredential([], { codex_subscription_enabled: "true" })).toBe(
      false,
    );
  });
});
