// =============================================================================
// BuildMockModal — the Build mock dialog (workbench g5.workbench.mock)
// =============================================================================

import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { renderApp } from "../../test/render";
import { createMockDocNode } from "../../test/fixtures";
import { recentMockDocs } from "../../lib/mockDocs";
import { BuildMockModal } from "./BuildMockModal";

const FOLDER = "docs/11111111-1111-4111-8111-111111111111";

const older = createMockDocNode({
  id: "mock-older",
  name: "Checkout flow",
  filePath: `${FOLDER}/mocks/checkout.html`,
  docType: "doc",
  contentUpdatedAt: "2026-10-28T09:00:00Z",
  updatedAt: "2026-10-28T09:00:00Z",
});
const newest = createMockDocNode({
  id: "mock-newest",
  name: "Mock index",
  filePath: `${FOLDER}/mocks/index.html`,
  docType: "doc",
  contentUpdatedAt: "2026-10-30T09:00:00Z",
  updatedAt: "2026-10-30T09:00:00Z",
});
const nested = createMockDocNode({
  id: "mock-nested",
  name: "Settings page",
  filePath: `${FOLDER}/mocks/settings/page.html`,
  docType: "doc",
  contentUpdatedAt: "2026-10-29T09:00:00Z",
  updatedAt: "2026-10-29T09:00:00Z",
});
const requirement = createMockDocNode({
  id: "doc-requirement",
  name: "Requirement",
  filePath: `${FOLDER}/requirement.md`,
  contentUpdatedAt: "2026-10-31T09:00:00Z",
  updatedAt: "2026-10-31T09:00:00Z",
});
const foreign = createMockDocNode({
  id: "mock-foreign",
  name: "Another workspace's mock",
  filePath: "docs/22222222-2222-4222-8222-222222222222/mocks/index.html",
  contentUpdatedAt: "2026-10-31T09:00:00Z",
  updatedAt: "2026-10-31T09:00:00Z",
});

describe("recentMockDocs", () => {
  it("keeps this workspace's mocks/ documents, newest first", () => {
    const docs = recentMockDocs(
      [older, requirement, newest, foreign, nested],
      FOLDER,
    );
    expect(docs.map((doc) => doc.id)).toEqual([
      "mock-newest",
      "mock-nested",
      "mock-older",
    ]);
  });

  it("lists nothing while the workspace folder is unknown", () => {
    expect(recentMockDocs([newest], null)).toEqual([]);
    expect(recentMockDocs([newest], undefined)).toEqual([]);
  });
});

describe("BuildMockModal", () => {
  function renderModal(
    overrides: Partial<React.ComponentProps<typeof BuildMockModal>> = {},
  ) {
    const onOpenMock = vi.fn();
    const onBuild = vi.fn();
    const onClose = vi.fn();
    renderApp(
      <BuildMockModal
        open
        docsFolderPath={FOLDER}
        mocks={recentMockDocs([older, newest, nested], FOLDER)}
        buildDisabledReason={null}
        onOpenMock={onOpenMock}
        onBuild={onBuild}
        onClose={onClose}
        {...overrides}
      />,
      { route: "/org-g5labs/test-workspace" },
    );
    return { onOpenMock, onBuild, onClose };
  }

  it("lists the recent mocks newest first and opens one in its reader tab", async () => {
    const user = userEvent.setup();
    const { onOpenMock, onBuild, onClose } = renderModal();
    const dialog = screen.getByRole("dialog", { name: "Build mock" });
    const rows = within(
      within(dialog).getByTestId("build-mock-list"),
    ).getAllByRole("button");
    expect(rows.map((row) => row.textContent)).toEqual([
      expect.stringContaining("index.html"),
      expect.stringContaining("page.html"),
      expect.stringContaining("checkout.html"),
    ]);
    // A nested mock shows its folder under mocks/.
    expect(rows[1]).toHaveTextContent("settings/");

    await user.click(within(dialog).getByTestId("build-mock-open-mock-older"));
    expect(onOpenMock).toHaveBeenCalledWith(
      expect.objectContaining({ id: "mock-older" }),
    );
    expect(onClose).toHaveBeenCalledTimes(1);
    expect(onBuild).not.toHaveBeenCalled();
  });

  it("seeds the prompt from its primary action and closes", async () => {
    const user = userEvent.setup();
    const { onBuild, onClose, onOpenMock } = renderModal({ mocks: [] });
    expect(screen.getByTestId("build-mock-empty")).toHaveTextContent(
      "No mocks yet under mocks/",
    );
    await user.click(screen.getByTestId("build-mock-submit"));
    expect(onBuild).toHaveBeenCalledTimes(1);
    expect(onClose).toHaveBeenCalledTimes(1);
    expect(onOpenMock).not.toHaveBeenCalled();
  });

  it("greys its primary action with the composer's reason while the mocks stay readable", async () => {
    const user = userEvent.setup();
    const { onBuild, onOpenMock } = renderModal({
      buildDisabledReason: "Start the workspace to chat",
    });
    const submit = screen.getByTestId("build-mock-submit");
    expect(submit).toBeDisabled();
    expect(submit).toHaveAttribute("title", "Start the workspace to chat");
    await user.click(submit);
    expect(onBuild).not.toHaveBeenCalled();
    await user.click(screen.getByTestId("build-mock-open-mock-newest"));
    expect(onOpenMock).toHaveBeenCalledWith(
      expect.objectContaining({ id: "mock-newest" }),
    );
  });

  it("renders nothing while closed", () => {
    renderModal({ open: false });
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});
