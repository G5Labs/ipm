// =============================================================================
// OrgApiKeyManager.test.tsx — API Key Management Tests
// =============================================================================
// Tests for the OrgApiKeyManager component (the API-keys half split out of the
// former OrgCredentialManager) covering:
// - Empty-org guard (renders nothing when orgId is blank)
// - Read-only mode when canManage=false (access notice, no create form)
// - API key listing, creation, reveal card, and revocation
// =============================================================================

import { describe, it, expect, vi, beforeEach } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { HttpResponse, http } from "msw";

import { renderApp } from "../../test/render";
import { server } from "../../test/msw/server";
import type { ApiKey } from "../../lib/types";
import { OrgApiKeyManager } from "./OrgApiKeyManager";

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const ORG_ID = "org-test-123";
const ORG_NAME = "Test Org";

// ---------------------------------------------------------------------------
// Mock Data Factory
// ---------------------------------------------------------------------------

function mockApiKey(overrides: Partial<ApiKey> = {}): ApiKey {
  return {
    id: "ak-1",
    orgId: ORG_ID,
    userId: "user-1",
    name: "ci-deploy",
    keyPrefix: "g5k_ci",
    scopes: ["org:read", "tasks:execute"],
    createdAt: "2024-12-01T00:00:00Z",
    expiresAt: "2025-03-01T00:00:00Z",
    lastUsedAt: "2024-12-15T10:00:00Z",
    revokedAt: null,
    ...overrides,
  };
}

// ---------------------------------------------------------------------------
// Default Props Factory
// ---------------------------------------------------------------------------

function defaultProps(
  overrides: Partial<React.ComponentProps<typeof OrgApiKeyManager>> = {},
) {
  return {
    orgId: ORG_ID,
    orgName: ORG_NAME,
    canManage: true,
    ...overrides,
  };
}

// ---------------------------------------------------------------------------
// MSW Handler Helpers
// ---------------------------------------------------------------------------

function useApiKeyHandlers(keys: ApiKey[] = []) {
  server.use(
    http.get(`/api/v1/orgs/${ORG_ID}/api-keys`, () => {
      return HttpResponse.json({ data: keys });
    }),
  );
}

// =============================================================================
// Test Suite
// =============================================================================

describe("OrgApiKeyManager", () => {
  beforeEach(() => {
    // Stub clipboard for the copy/reveal tests — navigator.clipboard is a
    // read-only property in jsdom so we need defineProperty to override it.
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText: vi.fn().mockResolvedValue(undefined) },
      writable: true,
      configurable: true,
    });
  });

  // -------------------------------------------------------------------------
  // Guard: empty orgId
  // -------------------------------------------------------------------------

  describe("empty orgId guard", () => {
    it("renders nothing when orgId is empty", () => {
      const { container } = renderApp(
        <OrgApiKeyManager {...defaultProps({ orgId: "" })} />,
      );
      expect(container.firstChild).toBeNull();
    });
  });

  // -------------------------------------------------------------------------
  // Read-Only Mode
  // -------------------------------------------------------------------------

  describe("read-only mode", () => {
    it("shows access-required message when canManage is false", async () => {
      useApiKeyHandlers();
      renderApp(<OrgApiKeyManager {...defaultProps({ canManage: false })} />);

      await waitFor(() => {
        expect(
          screen.getByText(/Org admin access is required/),
        ).toBeInTheDocument();
      });
    });

    it("does not show the creation form when canManage is false", async () => {
      useApiKeyHandlers();
      renderApp(<OrgApiKeyManager {...defaultProps({ canManage: false })} />);

      await waitFor(() => {
        expect(
          screen.getByText(/Org admin access is required/),
        ).toBeInTheDocument();
      });
      expect(screen.queryByText("Create API key")).not.toBeInTheDocument();
      expect(
        screen.queryByPlaceholderText("ci-deploy"),
      ).not.toBeInTheDocument();
    });
  });

  // -------------------------------------------------------------------------
  // Create form + scopes
  // -------------------------------------------------------------------------

  describe("create form", () => {
    it("renders the create form with the scope checkboxes when canManage", async () => {
      useApiKeyHandlers([]);
      renderApp(<OrgApiKeyManager {...defaultProps()} />);

      await waitFor(() => {
        expect(screen.getByPlaceholderText("ci-deploy")).toBeInTheDocument();
      });

      // Section header + a representative set of scope checkboxes.
      expect(
        screen.getByRole("button", { name: /Create API key/ }),
      ).toBeInTheDocument();
      expect(screen.getByText("Org read")).toBeInTheDocument();
      expect(screen.getByText("Tasks execute")).toBeInTheDocument();
      expect(screen.getByText("Workers admin")).toBeInTheDocument();
      // 15 scope options rendered as checkboxes.
      expect(screen.getAllByRole("checkbox")).toHaveLength(15);
    });

    it("disables create when the name is empty", async () => {
      useApiKeyHandlers([]);
      renderApp(<OrgApiKeyManager {...defaultProps()} />);

      await waitFor(() => {
        expect(screen.getByPlaceholderText("ci-deploy")).toBeInTheDocument();
      });

      expect(
        screen.getByRole("button", { name: /Create API key/ }),
      ).toBeDisabled();
    });
  });

  // -------------------------------------------------------------------------
  // Listing
  // -------------------------------------------------------------------------

  describe("listing", () => {
    it('shows "No API keys issued yet" when list is empty', async () => {
      useApiKeyHandlers([]);
      renderApp(<OrgApiKeyManager {...defaultProps()} />);

      await waitFor(() => {
        expect(screen.getByText("No API keys issued yet.")).toBeInTheDocument();
      });
    });

    it("renders existing API keys with name, prefix, and scopes", async () => {
      useApiKeyHandlers([mockApiKey()]);
      renderApp(<OrgApiKeyManager {...defaultProps()} />);

      await waitFor(() => {
        expect(screen.getByText("ci-deploy")).toBeInTheDocument();
      });
      expect(screen.getByText("g5k_ci")).toBeInTheDocument();
      expect(screen.getByText("org:read")).toBeInTheDocument();
      expect(screen.getByText("tasks:execute")).toBeInTheDocument();
    });
  });

  // -------------------------------------------------------------------------
  // Create → reveal card
  // -------------------------------------------------------------------------

  it("creates an API key and shows the reveal card", async () => {
    const user = userEvent.setup();
    useApiKeyHandlers([]);

    const createdKey = mockApiKey({ id: "ak-new", name: "my-key" });
    server.use(
      http.post(`/api/v1/orgs/${ORG_ID}/api-keys`, () => {
        return HttpResponse.json({
          data: { apiKey: createdKey, key: "g5k_plaintext_secret_123" },
        });
      }),
    );

    renderApp(<OrgApiKeyManager {...defaultProps()} />);

    await waitFor(() => {
      expect(screen.getByPlaceholderText("ci-deploy")).toBeInTheDocument();
    });

    await user.type(screen.getByPlaceholderText("ci-deploy"), "my-key");
    await user.click(screen.getByRole("button", { name: /Create API key/ }));

    await waitFor(() => {
      expect(screen.getByText("New API key")).toBeInTheDocument();
    });
    expect(screen.getByText("g5k_plaintext_secret_123")).toBeInTheDocument();
    expect(
      screen.getByText(/This value is only shown once/),
    ).toBeInTheDocument();
  });

  // -------------------------------------------------------------------------
  // Revoke
  // -------------------------------------------------------------------------

  it("revokes an API key after confirmation", async () => {
    const user = userEvent.setup();
    useApiKeyHandlers([mockApiKey()]);

    const deleteCalled = vi.fn();
    server.use(
      http.delete(`/api/v1/orgs/${ORG_ID}/api-keys/ak-1`, () => {
        deleteCalled();
        return new HttpResponse(null, { status: 204 });
      }),
    );
    vi.spyOn(window, "confirm").mockReturnValue(true);

    renderApp(<OrgApiKeyManager {...defaultProps()} />);

    await waitFor(() => {
      expect(screen.getByText("ci-deploy")).toBeInTheDocument();
    });

    await user.click(screen.getByRole("button", { name: /Revoke/ }));

    await waitFor(() => {
      expect(deleteCalled).toHaveBeenCalled();
    });
  });
});
