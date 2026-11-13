// =============================================================================
// ProviderAccountsSection tests — test_contract/provider_accounts I6 (Vitest
// half); settings-page D16. HTTP is mocked at the MSW layer like the sibling
// settings tests; the component and hooks under test are real.
// =============================================================================

import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { server } from "../../test/msw/server";
import { renderApp } from "../../test/render";
import type { ProviderAccountDTO, ProviderAccountUsage } from "../../lib/types";
import { ProviderAccountsSection } from "./ProviderAccountsSection";

// Provider-accounts kill-switch (contract D14): the section resolves it via
// isProviderAccountsEnabled over the instance-info query. Default ON here;
// the gate-off test flips the holder before rendering.
const providerAccountsGate = vi.hoisted(() => ({ enabled: true }));
vi.mock("../../lib/providerFlags", async (importOriginal) => {
  const actual =
    await importOriginal<typeof import("../../lib/providerFlags")>();
  return {
    ...actual,
    isProviderAccountsEnabled: () => providerAccountsGate.enabled,
  };
});

const ORG_ID = "org-g5labs";
const WORKSPACE_ID = "workspace-test";
const LIST_URL = `/api/v1/orgs/${ORG_ID}/provider-accounts`;
const ITEM_URL = `${LIST_URL}/:accountId`;

function account(overrides: Partial<ProviderAccountDTO>): ProviderAccountDTO {
  return {
    id: "acc-1",
    orgId: ORG_ID,
    name: "Personal Max",
    kind: "claude_subscription",
    family: "claude",
    scope: "user",
    workspaceId: null,
    userId: "user-alex",
    baseUrl: null,
    hasSecret: true,
    secretHint: "sk-ant-oat01",
    createdBy: "user-alex",
    createdAt: "2026-07-02T10:00:00Z",
    updatedAt: "2026-07-02T10:00:00Z",
    usage: null,
    ...overrides,
  };
}

const SUBSCRIPTION = account({});
const ORG_API_KEY = account({
  id: "acc-2",
  name: "Claude API keys",
  kind: "anthropic_api_key",
  scope: "org",
  userId: null,
  baseUrl: "https://api.anthropic.com",
  secretHint: "sk-ant-api03",
});
const CHATGPT = account({
  id: "acc-3",
  name: "ChatGPT Pro",
  kind: "chatgpt_subscription",
  family: "codex",
  hasSecret: false,
  secretHint: null,
});
const COMPATIBLE = account({
  id: "acc-4",
  name: "GLM Coding Plan",
  kind: "anthropic_compatible_bearer",
  scope: "workspace",
  workspaceId: WORKSPACE_ID,
  userId: null,
  baseUrl: "https://api.z.ai/api/anthropic",
  secretHint: "zai-7c3e0123",
});

interface Recorded {
  listCalls: Array<{ workspaceId: string | null }>;
  patches: Array<{ id: string; body: Record<string, unknown> }>;
  posts: Array<Record<string, unknown>>;
  deletes: string[];
}

interface StoreOptions {
  accounts?: ProviderAccountDTO[];
  /** Typed token (or "forbidden") the PATCH should fail with. */
  patchErrorCode?: string;
  /** Typed token the DELETE should fail with. */
  deleteErrorCode?: string;
  /** Typed token the POST should fail with. */
  postErrorCode?: string;
}

const VALIDATION_TOKENS = new Set([
  "account_kind_mismatch",
  "account_base_url_invalid",
  "account_secret_invalid",
  "account_scope_forbidden",
  "account_name_invalid",
]);

/**
 * Exact error carriers the provider-accounts routes emit: validation → 422
 * VALIDATION_ERROR with the token in `details.code`; conflicts → 409 CONFLICT
 * with `message` = "<token>: text"; forbidden → plain 403 envelope. The
 * `error.code` field is never the typed token.
 */
function wireError(token: string) {
  if (token === "forbidden") {
    return HttpResponse.json(
      { error: { code: "FORBIDDEN", message: "forbidden" } },
      { status: 403 },
    );
  }
  if (VALIDATION_TOKENS.has(token)) {
    return HttpResponse.json(
      {
        error: {
          code: "VALIDATION_ERROR",
          message: "validation failed",
          details: { code: token, message: `server text for ${token}` },
        },
      },
      { status: 422 },
    );
  }
  return HttpResponse.json(
    {
      error: {
        code: "CONFLICT",
        message: `Conflict: ${token}: server conflict text`,
      },
    },
    { status: 409 },
  );
}

function installStore(options: StoreOptions = {}): Recorded {
  let rows = [...(options.accounts ?? [SUBSCRIPTION, ORG_API_KEY, CHATGPT])];
  const recorded: Recorded = {
    listCalls: [],
    patches: [],
    posts: [],
    deletes: [],
  };
  let seq = 100;
  server.use(
    http.get(LIST_URL, ({ request }) => {
      const url = new URL(request.url);
      recorded.listCalls.push({
        workspaceId: url.searchParams.get("workspaceId"),
      });
      return HttpResponse.json(
        { data: rows },
        { headers: { "Cache-Control": "no-store" } },
      );
    }),
    http.post(LIST_URL, async ({ request }) => {
      const body = (await request.json()) as Record<string, unknown>;
      recorded.posts.push(body);
      if (options.postErrorCode) return wireError(options.postErrorCode);
      const created = account({
        id: `acc-${seq++}`,
        name: String(body.name),
        kind: body.kind as ProviderAccountDTO["kind"],
        scope: body.scope as ProviderAccountDTO["scope"],
        workspaceId: (body.workspaceId as string | undefined) ?? null,
        baseUrl: (body.baseUrl as string | undefined) ?? null,
        hasSecret: typeof body.secret === "string" && body.secret.length > 0,
        // The server derives the bounded hint; the client never does.
        secretHint:
          typeof body.secret === "string" && body.secret.length >= 24
            ? body.secret.slice(0, 12)
            : null,
      });
      rows = [...rows, created];
      return HttpResponse.json(
        { data: created },
        { status: 201, headers: { "Cache-Control": "no-store" } },
      );
    }),
    http.patch(ITEM_URL, async ({ params, request }) => {
      const id = String(params.accountId);
      const body = (await request.json()) as Record<string, unknown>;
      recorded.patches.push({ id, body });
      if (options.patchErrorCode) return wireError(options.patchErrorCode);
      rows = rows.map((row) =>
        row.id === id
          ? {
              ...row,
              name: typeof body.name === "string" ? body.name : row.name,
              baseUrl:
                typeof body.baseUrl === "string" ? body.baseUrl : row.baseUrl,
              secretHint:
                typeof body.secret === "string"
                  ? body.secret.slice(0, 12)
                  : row.secretHint,
              hasSecret: typeof body.secret === "string" ? true : row.hasSecret,
            }
          : row,
      );
      const updated = rows.find((row) => row.id === id);
      return HttpResponse.json(
        { data: updated },
        { headers: { "Cache-Control": "no-store" } },
      );
    }),
    http.delete(ITEM_URL, ({ params }) => {
      const id = String(params.accountId);
      recorded.deletes.push(id);
      if (options.deleteErrorCode) return wireError(options.deleteErrorCode);
      rows = rows.filter((row) => row.id !== id);
      return new HttpResponse(null, { status: 204 });
    }),
  );
  return recorded;
}

interface RenderOpts {
  context?: "workspace" | "organization";
  workspaceId?: string | null;
  canEditOrg?: boolean;
  canEditWorkspace?: boolean;
  assignedAccountIds?: string[];
}

function renderSection(opts: RenderOpts = {}) {
  return renderApp(
    <ProviderAccountsSection
      context={opts.context}
      orgId={ORG_ID}
      workspaceId={
        opts.workspaceId === undefined ? WORKSPACE_ID : opts.workspaceId
      }
      canEditOrg={opts.canEditOrg ?? true}
      canEditWorkspace={opts.canEditWorkspace ?? true}
      assignedAccountIds={opts.assignedAccountIds}
    />,
  );
}

async function expandRow(id: string) {
  await userEvent.click(
    await screen.findByTestId(`provider-account-toggle-${id}`),
  );
  return screen.findByTestId(`provider-account-drawer-${id}`);
}

async function pickOption(selectTestId: string, value: string) {
  await userEvent.click(screen.getByTestId(selectTestId));
  await userEvent.click(
    await screen.findByTestId(`${selectTestId}-option-${value}`),
  );
}

beforeEach(() => {
  providerAccountsGate.enabled = true;
});

describe("ProviderAccountsSection — rows", () => {
  it("manages only org accounts in Admin and creates new accounts at org scope", async () => {
    const recorded = installStore();
    renderSection({ context: "organization", workspaceId: null });
    expect(
      await screen.findByTestId("provider-account-row-acc-2"),
    ).toBeInTheDocument();
    expect(screen.queryByTestId("provider-account-row-acc-1")).toBeNull();
    expect(screen.queryByTestId("provider-account-row-acc-4")).toBeNull();
    await userEvent.click(screen.getByTestId("provider-account-add"));
    await userEvent.click(screen.getByTestId("provider-account-scope-select"));
    expect(
      await screen.findByTestId("provider-account-scope-select-option-org"),
    ).toBeEnabled();
    expect(
      screen.queryByTestId("provider-account-scope-select-option-user"),
    ).toBeNull();
    expect(
      screen.queryByTestId("provider-account-scope-select-option-workspace"),
    ).toBeNull();
    await userEvent.keyboard("{Escape}");
    await userEvent.type(
      screen.getByTestId("provider-account-name-input-new"),
      "Shared subscription",
    );
    await userEvent.type(
      screen.getByTestId("provider-account-secret-input-new"),
      "sk-ant-oat01-org-subscription-token",
    );
    await userEvent.click(screen.getByTestId("provider-account-save-new"));
    await waitFor(() => expect(recorded.posts).toHaveLength(1));
    expect(recorded.posts[0]).toEqual({
      name: "Shared subscription",
      kind: "claude_subscription",
      scope: "org",
      secret: "sk-ant-oat01-org-subscription-token",
    });
  });

  it("renders each account with name, type pill, scope pill, and the server hint", async () => {
    const recorded = installStore();
    renderSection();

    const row1 = await screen.findByTestId("provider-account-row-acc-1");
    expect(
      within(row1).getByTestId("provider-account-name-acc-1"),
    ).toHaveTextContent("Personal Max");
    expect(
      within(row1).getByTestId("provider-account-type-pill-acc-1"),
    ).toHaveTextContent("Subscription");
    expect(
      within(row1).getByTestId("provider-account-scope-pill-acc-1"),
    ).toHaveTextContent("You");
    expect(
      within(row1).getByTestId("provider-account-hint-acc-1"),
    ).toHaveTextContent("sk-ant-oat01…");

    expect(screen.queryByTestId("provider-account-row-acc-2")).toBeNull();

    const row3 = screen.getByTestId("provider-account-row-acc-3");
    expect(
      within(row3).getByTestId("provider-account-hint-acc-3"),
    ).toHaveTextContent("nothing stored");

    // The list is fetched for the workspace in context.
    expect(recorded.listCalls[0]).toEqual({ workspaceId: WORKSPACE_ID });
    expect(
      screen.getByText(/Your first account becomes the default for its agent/),
    ).toBeInTheDocument();
  });

  it("renders the list only: no Active configuration group and no resolver call (settings-page D16/D19)", async () => {
    const resolveCalls: string[] = [];
    server.use(
      http.post(
        `/api/v1/workspaces/${WORKSPACE_ID}/config/agent-launch/resolve`,
        () => {
          resolveCalls.push("resolve");
          return HttpResponse.json(
            { error: { code: "VALIDATION_ERROR", message: "unexpected" } },
            { status: 422 },
          );
        },
      ),
    );
    installStore();
    renderSection();

    await screen.findByTestId("provider-account-row-acc-1");
    expect(screen.queryByText("Active configuration")).toBeNull();
    expect(
      screen.queryByTestId("provider-account-assignment-claude_code"),
    ).toBeNull();
    expect(
      screen.queryByTestId("provider-account-assignment-select-claude_code"),
    ).toBeNull();
    expect(
      screen.queryByTestId("provider-account-resolution-claude_code"),
    ).toBeNull();
    expect(resolveCalls).toEqual([]);
  });

  it("shows the empty state with the Add button when there are no accounts", async () => {
    installStore({ accounts: [] });
    renderSection();

    const empty = await screen.findByTestId("provider-accounts-empty");
    expect(
      within(empty).getByTestId("provider-account-add"),
    ).toBeInTheDocument();
  });

  it("expands and collapses a row on click", async () => {
    installStore();
    renderSection();

    const drawer = await expandRow("acc-1");
    expect(screen.getByTestId("provider-account-toggle-acc-1")).toHaveAttribute(
      "aria-expanded",
      "true",
    );
    // Type and Scope are read-only after create: text, not selects.
    expect(within(drawer).getByText("Claude subscription")).toBeInTheDocument();
    expect(
      within(drawer).queryByTestId("provider-account-kind-select"),
    ).toBeNull();
    expect(
      within(drawer).queryByTestId("provider-account-scope-select"),
    ).toBeNull();
    expect(within(drawer).getByLabelText("OAuth token")).toHaveAttribute(
      "type",
      "password",
    );
    expect(within(drawer).getByLabelText("OAuth token")).toHaveAttribute(
      "placeholder",
      expect.stringContaining("sk-ant-oat01"),
    );
    expect(within(drawer).getByText("2026-07-02")).toBeInTheDocument();

    await userEvent.click(screen.getByTestId("provider-account-toggle-acc-1"));
    expect(screen.queryByTestId("provider-account-drawer-acc-1")).toBeNull();
  });

  it("shows the no-credential note for ChatGPT subscriptions and no Base URL for fixed kinds", async () => {
    installStore();
    renderSection();

    const drawer = await expandRow("acc-3");
    expect(
      within(drawer).getByText(
        "Resolved for each execution from the current workspace owner; nothing stored in session history.",
      ),
    ).toBeInTheDocument();
    expect(
      within(drawer).queryByTestId("provider-account-secret-input-acc-3"),
    ).toBeNull();
    expect(
      within(drawer).queryByTestId("provider-account-base-url-acc-3"),
    ).toBeNull();
  });
});

describe("ProviderAccountsSection — setup guide", () => {
  it("shows the steps and the official documentation link for the selected kind in the new row", async () => {
    installStore();
    renderSection();

    await screen.findByTestId("provider-account-row-acc-1");
    await userEvent.click(screen.getByTestId("provider-account-add"));
    const drawer = await screen.findByTestId("provider-account-drawer-new");

    // Defaults to the Claude subscription guide (the default kind).
    const guide = within(drawer).getByTestId("provider-account-guide-new");
    expect(guide).toHaveAttribute("data-kind", "claude_subscription");
    expect(within(guide).getAllByRole("listitem")).toHaveLength(3);
    expect(guide).toHaveTextContent("claude setup-token");
    const link = within(guide).getByTestId("provider-account-guide-link-new");
    expect(link).toHaveAttribute(
      "href",
      "https://code.claude.com/docs/en/authentication#generate-a-long-lived-token",
    );
    expect(link).toHaveAttribute("target", "_blank");
    expect(link).toHaveAttribute("rel", "noopener noreferrer");

    // Switching the kind swaps the guide with it.
    await pickOption("provider-account-kind-select", "openrouter_api_key");
    const swapped = within(drawer).getByTestId("provider-account-guide-new");
    expect(swapped).toHaveAttribute("data-kind", "openrouter_api_key");
    expect(
      within(swapped).getByTestId("provider-account-guide-link-new"),
    ).toHaveAttribute(
      "href",
      "https://openrouter.ai/docs/api-reference/authentication",
    );

    // The compatible-endpoint kind links both GLM providers.
    await pickOption(
      "provider-account-kind-select",
      "anthropic_compatible_bearer",
    );
    expect(
      within(drawer).getAllByTestId("provider-account-guide-link-new"),
    ).toHaveLength(2);
  });

  it("explains the in-VM login for ChatGPT subscriptions instead of a paste step", async () => {
    installStore();
    renderSection();

    const drawer = await expandRow("acc-3");
    const guide = within(drawer).getByTestId("provider-account-guide-acc-3");
    expect(guide).toHaveTextContent("Nothing to paste here");
    expect(guide).toHaveTextContent("device code login");
    expect(
      within(guide).getByTestId("provider-account-guide-link-acc-3"),
    ).toHaveAttribute("href", "https://learn.chatgpt.com/docs/auth");
  });

  it("omits the guide from rows the caller cannot edit", async () => {
    installStore({ accounts: [SUBSCRIPTION, COMPATIBLE] });
    renderSection({ canEditWorkspace: false });

    const drawer = await expandRow("acc-4");
    expect(
      within(drawer).getByTestId("provider-account-readonly-acc-4"),
    ).toBeInTheDocument();
    expect(
      within(drawer).queryByTestId("provider-account-guide-acc-4"),
    ).toBeNull();
  });
});

describe("ProviderAccountsSection — editing", () => {
  it("renames live in the header and Save sends only the changed field", async () => {
    const recorded = installStore();
    renderSection();

    await expandRow("acc-1");
    const save = screen.getByTestId("provider-account-save-acc-1");
    expect(save).toBeDisabled();

    const nameInput = screen.getByTestId("provider-account-name-input-acc-1");
    await userEvent.clear(nameInput);
    await userEvent.type(nameInput, "Work Max");
    expect(screen.getByTestId("provider-account-name-acc-1")).toHaveTextContent(
      "Work Max",
    );
    expect(
      screen.getByTestId("provider-account-dirty-acc-1"),
    ).toHaveTextContent("Unsaved changes");
    expect(save).toBeEnabled();

    await userEvent.click(save);

    await waitFor(() => expect(recorded.patches).toHaveLength(1));
    expect(recorded.patches[0]).toEqual({
      id: "acc-1",
      body: { name: "Work Max" },
    });
    // Closed after save; the refetched row carries the new name.
    await waitFor(() =>
      expect(screen.queryByTestId("provider-account-drawer-acc-1")).toBeNull(),
    );
    await waitFor(() =>
      expect(
        screen.getByTestId("provider-account-name-acc-1"),
      ).toHaveTextContent("Work Max"),
    );
    expect(recorded.listCalls.length).toBeGreaterThanOrEqual(2);
  });

  it("omits `secret` when the credential is left blank and sends it when typed", async () => {
    const recorded = installStore();
    renderSection({ context: "organization", workspaceId: null });

    // On the provider's own host a wrong-kind credential is refused inline
    // before any request (QA 2026-10-27).
    await expandRow("acc-2");
    fireEvent.change(
      screen.getByTestId("provider-account-secret-input-acc-2"),
      {
        target: { value: "sk-ant-oat01-a-subscription-token" },
      },
    );
    await userEvent.click(screen.getByTestId("provider-account-save-acc-2"));
    expect(
      await screen.findByTestId("provider-account-error-acc-2"),
    ).toHaveTextContent("sk-ant-api03-");
    expect(recorded.patches).toHaveLength(0);
    fireEvent.change(
      screen.getByTestId("provider-account-secret-input-acc-2"),
      {
        target: { value: "" },
      },
    );

    const baseUrl = screen.getByTestId("provider-account-base-url-acc-2");
    fireEvent.change(baseUrl, {
      target: { value: "https://gateway.example.com" },
    });
    await userEvent.click(screen.getByTestId("provider-account-save-acc-2"));
    await waitFor(() => expect(recorded.patches).toHaveLength(1));
    expect(recorded.patches[0].body).toEqual({
      baseUrl: "https://gateway.example.com",
    });
    expect(recorded.patches[0].body).not.toHaveProperty("secret");
    expect(recorded.patches[0].body).not.toHaveProperty("kind");
    expect(recorded.patches[0].body).not.toHaveProperty("scope");

    await expandRow("acc-2");
    const secret = screen.getByTestId("provider-account-secret-input-acc-2");
    expect(secret).toHaveAttribute("autocomplete", "off");
    await userEvent.type(secret, "sk-ant-api03-rotated-credential-0001");
    await userEvent.click(screen.getByTestId("provider-account-save-acc-2"));
    await waitFor(() => expect(recorded.patches).toHaveLength(2));
    expect(recorded.patches[1].body).toEqual({
      secret: "sk-ant-api03-rotated-credential-0001",
    });

    // A whitespace-only credential is blank: nothing is sent.
    await expandRow("acc-2");
    fireEvent.change(
      screen.getByTestId("provider-account-secret-input-acc-2"),
      {
        target: { value: "   " },
      },
    );
    await userEvent.click(screen.getByTestId("provider-account-save-acc-2"));
    expect(recorded.patches).toHaveLength(2);
    // Behind the custom gateway set above, the key's shape is the gateway's
    // own, so a value of any shape is sent.
    fireEvent.change(
      screen.getByTestId("provider-account-secret-input-acc-2"),
      {
        target: { value: "gateway-key-0001" },
      },
    );
    await userEvent.click(screen.getByTestId("provider-account-save-acc-2"));
    await waitFor(() => expect(recorded.patches).toHaveLength(3));
    expect(recorded.patches[2].body).toEqual({ secret: "gateway-key-0001" });
  });

  it("Cancel discards the draft and restores the header name", async () => {
    const recorded = installStore();
    renderSection();

    await expandRow("acc-1");
    const nameInput = screen.getByTestId("provider-account-name-input-acc-1");
    await userEvent.clear(nameInput);
    await userEvent.type(nameInput, "Scratch");
    await userEvent.click(screen.getByTestId("provider-account-cancel-acc-1"));

    expect(screen.queryByTestId("provider-account-drawer-acc-1")).toBeNull();
    expect(screen.getByTestId("provider-account-name-acc-1")).toHaveTextContent(
      "Personal Max",
    );
    expect(recorded.patches).toHaveLength(0);
  });

  it("renders the typed error codes inline under the row", async () => {
    installStore({ patchErrorCode: "account_name_conflict" });
    renderSection();

    await expandRow("acc-1");
    const nameInput = screen.getByTestId("provider-account-name-input-acc-1");
    await userEvent.clear(nameInput);
    await userEvent.type(nameInput, "Claude API keys");
    await userEvent.click(screen.getByTestId("provider-account-save-acc-1"));

    const error = await screen.findByTestId("provider-account-error-acc-1");
    expect(error).toHaveTextContent(
      "An account with this name already exists at this scope.",
    );
    // The drawer stays open so the user can fix the draft.
    expect(
      screen.getByTestId("provider-account-drawer-acc-1"),
    ).toBeInTheDocument();
  });

  it("parses a 422 validation carrier token from error.details.code", async () => {
    installStore({ patchErrorCode: "account_base_url_invalid" });
    renderSection({ context: "organization", workspaceId: null });

    await expandRow("acc-2");
    fireEvent.change(screen.getByTestId("provider-account-base-url-acc-2"), {
      target: { value: "http://plain.example.com" },
    });
    await userEvent.click(screen.getByTestId("provider-account-save-acc-2"));

    expect(
      await screen.findByTestId("provider-account-error-acc-2"),
    ).toHaveTextContent("Base URL must be an https:// URL.");
  });

  it("renders a 403 as the scope-permission message", async () => {
    installStore({ patchErrorCode: "forbidden" });
    renderSection();

    await expandRow("acc-1");
    const nameInput = screen.getByTestId("provider-account-name-input-acc-1");
    await userEvent.type(nameInput, "!");
    await userEvent.click(screen.getByTestId("provider-account-save-acc-1"));

    expect(
      await screen.findByTestId("provider-account-error-acc-1"),
    ).toHaveTextContent("You cannot edit accounts at this scope");
  });

  it("offers the compatible-endpoint presets and reveals an input for Custom…", async () => {
    const recorded = installStore({ accounts: [COMPATIBLE] });
    renderSection();

    await expandRow("acc-4");
    const preset = screen.getByTestId("provider-account-base-url-preset-acc-4");
    expect(preset).toHaveTextContent(
      "api.z.ai — Anthropic-compatible (global)",
    );
    expect(screen.queryByTestId("provider-account-base-url-acc-4")).toBeNull();

    await pickOption("provider-account-base-url-preset-acc-4", "custom");
    const custom = await screen.findByTestId("provider-account-base-url-acc-4");
    // Custom shows the current URL so the user can edit it.
    expect(custom).toHaveValue("https://api.z.ai/api/anthropic");
    fireEvent.change(custom, {
      target: { value: "https://glm.example.com/api/anthropic" },
    });
    await userEvent.click(screen.getByTestId("provider-account-save-acc-4"));
    await waitFor(() => expect(recorded.patches).toHaveLength(1));
    expect(recorded.patches[0].body).toEqual({
      baseUrl: "https://glm.example.com/api/anthropic",
    });
  });
});

describe("ProviderAccountsSection — remove", () => {
  it("disables Remove with the reason when the account is assigned", async () => {
    const recorded = installStore();
    renderSection({ assignedAccountIds: ["acc-1"] });

    await expandRow("acc-1");
    const remove = screen.getByTestId("provider-account-remove-acc-1");
    expect(remove).toBeDisabled();
    expect(remove).toHaveAttribute(
      "title",
      expect.stringContaining("assigned"),
    );
    expect(recorded.deletes).toHaveLength(0);
  });

  it("removes an unassigned account and drops the row", async () => {
    const recorded = installStore();
    renderSection();

    await expandRow("acc-1");
    const remove = screen.getByTestId("provider-account-remove-acc-1");
    expect(remove).toBeEnabled();
    await userEvent.click(remove);

    await waitFor(() => expect(recorded.deletes).toEqual(["acc-1"]));
    await waitFor(() =>
      expect(screen.queryByTestId("provider-account-row-acc-1")).toBeNull(),
    );
  });

  it("keeps Remove disabled with the reason after the server answers account_assigned", async () => {
    installStore({ deleteErrorCode: "account_assigned" });
    renderSection();

    await expandRow("acc-1");
    await userEvent.click(screen.getByTestId("provider-account-remove-acc-1"));

    expect(
      await screen.findByTestId("provider-account-error-acc-1"),
    ).toHaveTextContent("assigned to an agent");
    const remove = screen.getByTestId("provider-account-remove-acc-1");
    expect(remove).toBeDisabled();
    expect(remove).toHaveAttribute(
      "title",
      expect.stringContaining("assigned"),
    );
    expect(
      screen.getByTestId("provider-account-row-acc-1"),
    ).toBeInTheDocument();
  });
});

describe("ProviderAccountsSection — add", () => {
  it("opens an expanded new row with Name focused and Type/Scope editable, then fixes them after save", async () => {
    const recorded = installStore();
    renderSection();

    await screen.findByTestId("provider-account-row-acc-1");
    await userEvent.click(screen.getByTestId("provider-account-add"));

    const drawer = await screen.findByTestId("provider-account-drawer-new");
    const nameInput = within(drawer).getByTestId(
      "provider-account-name-input-new",
    );
    expect(nameInput).toHaveFocus();
    expect(
      within(drawer).getByTestId("provider-account-kind-select"),
    ).toBeInTheDocument();
    expect(
      within(drawer).getByTestId("provider-account-scope-select"),
    ).toBeInTheDocument();
    expect(
      within(drawer).getByTestId("provider-account-save-new"),
    ).toBeDisabled();

    await userEvent.type(nameInput, "Team API key");
    await pickOption("provider-account-kind-select", "anthropic_api_key");
    expect(within(drawer).getByLabelText("API key")).toBeInTheDocument();
    await userEvent.type(
      within(drawer).getByTestId("provider-account-secret-input-new"),
      "sk-ant-api03-e2e-test-key-0123456789",
    );
    fireEvent.change(
      within(drawer).getByTestId("provider-account-base-url-new"),
      {
        target: { value: "https://api.anthropic.com" },
      },
    );
    // Scope defaults to You; it is selectable before the first save.
    expect(
      screen.getByTestId("provider-account-scope-select"),
    ).toHaveTextContent("You");

    await userEvent.click(
      within(drawer).getByTestId("provider-account-save-new"),
    );

    await waitFor(() => expect(recorded.posts).toHaveLength(1));
    expect(recorded.posts[0]).toEqual({
      name: "Team API key",
      kind: "anthropic_api_key",
      scope: "user",
      baseUrl: "https://api.anthropic.com",
      secret: "sk-ant-api03-e2e-test-key-0123456789",
    });

    // The new row collapses into the list with the server-derived hint...
    await waitFor(() =>
      expect(screen.queryByTestId("provider-account-drawer-new")).toBeNull(),
    );
    const created = await screen.findByTestId("provider-account-row-acc-100");
    expect(
      within(created).getByTestId("provider-account-hint-acc-100"),
    ).toHaveTextContent("sk-ant-api03…");
    expect(
      within(created).getByTestId("provider-account-type-pill-acc-100"),
    ).toHaveTextContent("API");
    // ...and the raw key never reaches the DOM.
    expect(document.body.textContent).not.toContain("e2e-test-key-0123456789");

    // ...where Type and Scope are now read-only.
    await expandRow("acc-100");
    expect(screen.queryByTestId("provider-account-kind-select")).toBeNull();
    expect(screen.queryByTestId("provider-account-scope-select")).toBeNull();
    expect(screen.getByText("Anthropic API key")).toBeInTheDocument();
  });

  it("limits scope options to what the caller can edit and sends workspaceId for workspace scope", async () => {
    const recorded = installStore();
    renderSection({ canEditOrg: false, canEditWorkspace: true });

    await screen.findByTestId("provider-account-row-acc-1");
    await userEvent.click(screen.getByTestId("provider-account-add"));
    await userEvent.type(
      await screen.findByTestId("provider-account-name-input-new"),
      "Team subscription",
    );

    await userEvent.click(screen.getByTestId("provider-account-scope-select"));
    expect(
      screen.queryByTestId("provider-account-scope-select-option-org"),
    ).toBeNull();
    await userEvent.click(
      screen.getByTestId("provider-account-scope-select-option-workspace"),
    );
    await userEvent.type(
      screen.getByTestId("provider-account-secret-input-new"),
      "sk-ant-oat01-team-subscription-token-01",
    );
    await userEvent.click(screen.getByTestId("provider-account-save-new"));

    await waitFor(() => expect(recorded.posts).toHaveLength(1));
    expect(recorded.posts[0]).toEqual({
      name: "Team subscription",
      kind: "claude_subscription",
      scope: "workspace",
      workspaceId: WORKSPACE_ID,
      secret: "sk-ant-oat01-team-subscription-token-01",
    });
  });

  it("refuses an API key pasted into a Claude subscription account before any request", async () => {
    // QA 2026-10-27: the server accepted it and every turn failed with
    // Claude's "Not logged in · Please run /login".
    const recorded = installStore();
    renderSection();

    await screen.findByTestId("provider-account-row-acc-1");
    await userEvent.click(screen.getByTestId("provider-account-add"));
    await userEvent.type(
      await screen.findByTestId("provider-account-name-input-new"),
      "My plan",
    );
    await userEvent.type(
      screen.getByTestId("provider-account-secret-input-new"),
      "sk-ant-api03-pasted-into-the-wrong-kind",
    );
    await userEvent.click(screen.getByTestId("provider-account-save-new"));

    expect(
      await screen.findByTestId("provider-account-error-new"),
    ).toHaveTextContent("claude setup-token");
    expect(recorded.posts).toHaveLength(0);
    expect(
      screen.getByTestId("provider-account-drawer-new"),
    ).toBeInTheDocument();

    // A credential with whitespace inside is refused the same way. (A line
    // break cannot reach an <input>: browsers strip it from the value.)
    const secret = screen.getByTestId("provider-account-secret-input-new");
    fireEvent.change(secret, {
      target: { value: "sk-ant-oat01-first-half second-half" },
    });
    await userEvent.click(screen.getByTestId("provider-account-save-new"));
    expect(screen.getByTestId("provider-account-error-new")).toHaveTextContent(
      "space or line break",
    );
    expect(recorded.posts).toHaveLength(0);
  });

  it("trims a pasted credential before sending it", async () => {
    const recorded = installStore();
    renderSection();

    await screen.findByTestId("provider-account-row-acc-1");
    await userEvent.click(screen.getByTestId("provider-account-add"));
    await userEvent.type(
      await screen.findByTestId("provider-account-name-input-new"),
      "My plan",
    );
    fireEvent.change(screen.getByTestId("provider-account-secret-input-new"), {
      target: { value: "  sk-ant-oat01-pasted-with-a-newline\n" },
    });
    await userEvent.click(screen.getByTestId("provider-account-save-new"));

    await waitFor(() => expect(recorded.posts).toHaveLength(1));
    expect(recorded.posts[0].secret).toBe("sk-ant-oat01-pasted-with-a-newline");
  });

  it("shows a create error inline and keeps the new row open", async () => {
    installStore({ postErrorCode: "account_secret_invalid" });
    renderSection();

    await screen.findByTestId("provider-account-row-acc-1");
    await userEvent.click(screen.getByTestId("provider-account-add"));
    await userEvent.type(
      await screen.findByTestId("provider-account-name-input-new"),
      "Broken",
    );
    await userEvent.click(screen.getByTestId("provider-account-save-new"));

    expect(
      await screen.findByTestId("provider-account-error-new"),
    ).toHaveTextContent("credential is missing or not valid");
    expect(
      screen.getByTestId("provider-account-drawer-new"),
    ).toBeInTheDocument();
  });
});

describe("ProviderAccountsSection — permissions and gate", () => {
  it("renders rows the caller cannot edit as read-only with a hint", async () => {
    installStore({ accounts: [SUBSCRIPTION, COMPATIBLE, ORG_API_KEY] });
    renderSection({ canEditWorkspace: false });

    const drawer = await expandRow("acc-4");
    expect(
      within(drawer).getByTestId("provider-account-readonly-acc-4"),
    ).toHaveTextContent("cannot edit accounts at this scope");
    expect(
      within(drawer).getByTestId("provider-account-name-input-acc-4"),
    ).toBeDisabled();
    expect(
      within(drawer).getByTestId("provider-account-secret-input-acc-4"),
    ).toBeDisabled();
    expect(
      within(drawer).queryByTestId("provider-account-save-acc-4"),
    ).toBeNull();
    expect(
      within(drawer).queryByTestId("provider-account-remove-acc-4"),
    ).toBeNull();

    // The caller's own user account stays editable.
    await expandRow("acc-1");
    expect(
      screen.getByTestId("provider-account-save-acc-1"),
    ).toBeInTheDocument();
  });

  it("renders nothing while the provider_accounts feature is off", async () => {
    providerAccountsGate.enabled = false;
    const recorded = installStore();
    renderSection();

    await waitFor(() =>
      expect(screen.queryByTestId("provider-accounts-section")).toBeNull(),
    );
    expect(recorded.listCalls).toHaveLength(0);
  });
});

// ---------------------------------------------------------------------------
// I11 steps 3 and 5 (settings-page D18): usage rings on rows whose list DTO
// carries an account-keyed snapshot; nothing on rows without one.
// ---------------------------------------------------------------------------

describe("ProviderAccountsSection — usage rings (I11)", () => {
  const HOUR = 60 * 60 * 1000;
  const DAY = 24 * HOUR;
  // Timestamps relative to the real clock the section ticks against; the
  // margins (2h / 3d / 20m) keep the rendered text stable for the test's life.
  const iso = (deltaMs: number) => new Date(Date.now() + deltaMs).toISOString();
  const USAGE: ProviderAccountUsage = {
    fiveHourUtilization: 0.62,
    fiveHourResetAt: iso(2 * HOUR + 30_000),
    sevenDayUtilization: 0.85,
    sevenDayResetAt: iso(3 * DAY + 30_000),
    capturedAt: iso(-20 * 60 * 1000),
  };

  it("renders 5h and 7d cells with the 50/80 thresholds on a subscription row and no rings on rows without usage", async () => {
    installStore({
      accounts: [
        account({ usage: USAGE }),
        { ...ORG_API_KEY, scope: "user", userId: "user-1" },
      ],
    });
    renderSection();

    const row1 = await screen.findByTestId("provider-account-row-acc-1");
    const cells = within(row1).getByTestId("provider-account-usage-acc-1");
    const fiveHour = within(cells).getByTestId(
      "provider-account-usage-5h-acc-1",
    );
    const sevenDay = within(cells).getByTestId(
      "provider-account-usage-7d-acc-1",
    );
    expect(fiveHour).toHaveTextContent("5h62%");
    expect(sevenDay).toHaveTextContent("7d85%");
    const ring5h = within(fiveHour).getByTestId("usage-ring-5h");
    const ring7d = within(sevenDay).getByTestId("usage-ring-7d");
    expect(ring5h).toHaveAttribute("data-level", "warn");
    expect(ring5h).toHaveAttribute("aria-label", "5h 62% used");
    expect(ring7d).toHaveAttribute("data-level", "danger");
    expect(ring7d).toHaveAttribute("aria-label", "7d 85% used");
    // Hover title carries reset and capture age.
    expect(ring5h.querySelector("title")).toHaveTextContent(
      "5-hour: 62% used · resets in 2h · as of 20m ago",
    );
    expect(ring7d.querySelector("title")).toHaveTextContent(
      "7-day: 85% used · resets in 3d · as of 20m ago",
    );

    // The API-key row (usage: null) renders no cells and no ring at all.
    const row2 = screen.getByTestId("provider-account-row-acc-2");
    expect(
      within(row2).queryByTestId("provider-account-usage-acc-2"),
    ).toBeNull();
    expect(within(row2).queryAllByRole("img")).toHaveLength(0);
    expect(
      within(row2).getByTestId("provider-account-hint-acc-2"),
    ).toHaveTextContent("sk-ant-api03…");
  });

  it("lists each window in the expanded row and shows 'No usage data' for a row without a snapshot", async () => {
    installStore({
      accounts: [
        account({ usage: USAGE }),
        { ...ORG_API_KEY, scope: "user", userId: "user-1" },
      ],
    });
    renderSection();

    const drawer = await expandRow("acc-1");
    const line5h = within(drawer).getByTestId(
      "provider-account-usage-line-5h-acc-1",
    );
    const line7d = within(drawer).getByTestId(
      "provider-account-usage-line-7d-acc-1",
    );
    expect(line5h).toHaveTextContent("5h");
    expect(line5h).toHaveTextContent("62% used");
    expect(line5h).toHaveTextContent("resets in 2h · as of 20m ago");
    expect(within(line5h).getByTestId("usage-ring-line-5h")).toHaveAttribute(
      "data-level",
      "warn",
    );
    expect(line7d).toHaveTextContent("85% used");
    expect(line7d).toHaveTextContent("resets in 3d · as of 20m ago");
    expect(within(line7d).getByTestId("usage-ring-line-7d")).toHaveAttribute(
      "data-level",
      "danger",
    );
    expect(within(drawer).getByText("Usage")).toBeInTheDocument();

    const drawer2 = await expandRow("acc-2");
    expect(
      within(drawer2).getByTestId("provider-account-usage-none-acc-2"),
    ).toHaveTextContent("No usage data");
    expect(
      within(drawer2).queryByTestId("provider-account-usage-line-5h-acc-2"),
    ).toBeNull();
  });

  it("keeps rendering a stale snapshot with its age, and a window without data as the empty ring", async () => {
    installStore({
      accounts: [
        account({
          usage: {
            fiveHourUtilization: null,
            fiveHourResetAt: null,
            sevenDayUtilization: 0.1,
            sevenDayResetAt: null,
            capturedAt: iso(-26 * HOUR),
          },
        }),
      ],
    });
    renderSection();

    const row = await screen.findByTestId("provider-account-row-acc-1");
    const fiveHour = within(row).getByTestId("provider-account-usage-5h-acc-1");
    expect(fiveHour).toHaveTextContent("5h—");
    const ring5h = within(fiveHour).getByTestId("usage-ring-5h");
    expect(ring5h).toHaveAttribute("data-level", "none");
    expect(ring5h).toHaveAttribute("aria-label", "5h no data");
    expect(ring5h.querySelector("title")).toHaveTextContent(
      "5-hour: no data · as of 1d ago",
    );
    const ring7d = within(row).getByTestId("usage-ring-7d");
    expect(ring7d).toHaveAttribute("data-level", "ok");
    // No reset known: only the capture age, still rendered rather than hidden.
    expect(ring7d.querySelector("title")).toHaveTextContent(
      "7-day: 10% used · as of 1d ago",
    );

    const drawer = await expandRow("acc-1");
    expect(
      within(drawer).getByTestId("provider-account-usage-line-5h-acc-1"),
    ).toHaveTextContent("no data");
    expect(
      within(drawer).getByTestId("provider-account-usage-line-7d-acc-1"),
    ).toHaveTextContent("10% usedas of 1d ago");
  });
});
