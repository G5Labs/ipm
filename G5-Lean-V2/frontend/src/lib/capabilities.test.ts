import { describe, expect, it } from "vitest";

import {
  applyDeploymentWorkspaceUiCapabilities,
  resolveWorkspaceUiCapabilities,
} from "./capabilities";

describe("workspace UI capabilities", () => {
  it("defaults cloud workspace workflows off outside CI and test", () => {
    expect(resolveWorkspaceUiCapabilities({ MODE: "production" })).toEqual({
      cloudWorkspaceWorkflows: false,
      intentLogViewer: false,
    });
  });

  it("enables cloud workspace workflows in test mode", () => {
    expect(resolveWorkspaceUiCapabilities({ MODE: "test" })).toEqual({
      cloudWorkspaceWorkflows: true,
      intentLogViewer: false,
    });
  });

  it("enables cloud workspace workflows for explicit CI workflow env", () => {
    expect(
      resolveWorkspaceUiCapabilities({
        MODE: "production",
        VITE_G5_CI_WORKFLOW: "true",
      }).cloudWorkspaceWorkflows,
    ).toBe(true);
  });

  it("enables cloud workspace workflows for an explicit feature env", () => {
    expect(
      resolveWorkspaceUiCapabilities({
        MODE: "production",
        VITE_G5_ENABLE_CLOUD_WORKSPACE_WORKFLOWS: "1",
      }).cloudWorkspaceWorkflows,
    ).toBe(true);
  });

  it("keeps the intent log viewer off unless explicitly enabled", () => {
    expect(
      resolveWorkspaceUiCapabilities({
        MODE: "test",
        VITE_G5_ENABLE_CLOUD_WORKSPACE_WORKFLOWS: "1",
      }).intentLogViewer,
    ).toBe(false);
  });

  it("enables the intent log viewer for an explicit feature env", () => {
    expect(
      resolveWorkspaceUiCapabilities({
        MODE: "production",
        VITE_G5_ENABLE_INTENT_LOG_VIEWER: "yes",
      }),
    ).toEqual({
      cloudWorkspaceWorkflows: false,
      intentLogViewer: true,
    });
  });

  it("disables cloud workflows for self-hosted or unresolved deployments", () => {
    const capabilities = {
      cloudWorkspaceWorkflows: true,
      intentLogViewer: true,
    };

    expect(
      applyDeploymentWorkspaceUiCapabilities(capabilities, {
        selfHosted: false,
      }),
    ).toEqual(capabilities);

    expect(
      applyDeploymentWorkspaceUiCapabilities(capabilities, {
        selfHosted: true,
      }),
    ).toEqual({
      cloudWorkspaceWorkflows: false,
      intentLogViewer: true,
    });

    expect(applyDeploymentWorkspaceUiCapabilities(capabilities, null)).toEqual({
      cloudWorkspaceWorkflows: false,
      intentLogViewer: true,
    });
  });
});
