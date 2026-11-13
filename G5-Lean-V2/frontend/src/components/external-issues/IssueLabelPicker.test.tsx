/**
 * IssueLabelPicker — select labels from one domain's catalog
 * (g5.issue.labels.surfaces).
 *
 * Covered: the catalog as a checkable list with the selection as chips;
 * filtering by name or description; names compared without regard to case;
 * a name the catalog lacks can never be selected (typed and Enter, or no
 * match); a selected name the catalog lacks stays removable; the limit;
 * the empty catalog pointing to Manage labels; Manage labels opening the
 * catalog editor for the same domain; the disabled state with its reason.
 */

import { useState } from "react";
import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

import {
  makeLabelPage,
  serveLabelCatalog,
} from "../../test/externalIssueFixtures";
import { createMockUser } from "../../test/fixtures";
import { renderApp } from "../../test/render";
import type { IssueLabelPickerProps } from "./IssueLabelPicker";
import { IssueLabelPicker } from "./IssueLabelPicker";

const DOMAIN = {
  domainId: "domain-1",
  displayKey: "acme/api",
  provider: "github" as const,
  contentTrusted: true,
};

function Harness({
  initial = [],
  onSelected,
  ...props
}: Partial<IssueLabelPickerProps> & {
  initial?: string[];
  onSelected?: (names: string[]) => void;
}) {
  const [selected, setSelected] = useState<string[]>(initial);
  return (
    <IssueLabelPicker
      orgId="org-g5labs"
      domain={DOMAIN}
      canManage
      {...props}
      selected={selected}
      onChange={(names) => {
        setSelected(names);
        onSelected?.(names);
      }}
    />
  );
}

function renderPicker(props: Parameters<typeof Harness>[0] = {}) {
  return renderApp(<Harness {...props} />, { user: createMockUser() });
}

function selectedChips() {
  const chips = screen.queryByTestId("label-picker-selected");
  return chips
    ? within(chips)
        .getAllByRole("listitem")
        .map((item) => item.textContent)
    : [];
}

describe("IssueLabelPicker", () => {
  it("selects and deselects catalog labels, shown as chips", async () => {
    const user = userEvent.setup();
    serveLabelCatalog();
    const seen: string[][] = [];
    renderPicker({ onSelected: (names) => seen.push(names) });

    await user.click(await screen.findByTestId("label-picker-option-bug"));
    await user.click(screen.getByTestId("label-picker-option-needs triage"));
    expect(selectedChips()).toEqual(["bug", "needs triage"]);
    expect(screen.getByTestId("label-picker-option-bug")).toBeChecked();

    await user.click(screen.getByTestId("label-picker-option-bug"));
    expect(selectedChips()).toEqual(["needs triage"]);
    // A chip's button removes it too.
    await user.click(
      screen.getByTestId("label-picker-selected-remove-needs triage"),
    );
    expect(selectedChips()).toEqual([]);
    expect(screen.getByText("No labels selected.")).toBeVisible();
    expect(seen.at(-1)).toEqual([]);
  });

  it("filters by name or description and matches names without regard to case", async () => {
    const user = userEvent.setup();
    serveLabelCatalog();
    renderPicker({ initial: ["BUG"] });

    // The issue's `BUG` is the catalog's `bug`.
    expect(await screen.findByTestId("label-picker-option-bug")).toBeChecked();
    await user.type(screen.getByTestId("label-picker-search"), "triage");
    const options = screen.getByTestId("label-picker-options");
    expect(within(options).getAllByRole("checkbox")).toHaveLength(1);
    expect(
      screen.getByTestId("label-picker-option-needs triage"),
    ).toBeVisible();
    await user.clear(screen.getByTestId("label-picker-search"));
    await user.type(screen.getByTestId("label-picker-search"), "broken");
    expect(screen.getByTestId("label-picker-option-bug")).toBeVisible();
    expect(
      screen.queryByTestId("label-picker-option-wip"),
    ).not.toBeInTheDocument();
  });

  it("never selects a name the catalog lacks", async () => {
    const user = userEvent.setup();
    serveLabelCatalog();
    const seen: string[][] = [];
    renderPicker({ onSelected: (names) => seen.push(names) });

    await screen.findByTestId("label-picker-option-bug");
    await user.type(
      screen.getByTestId("label-picker-search"),
      "brand-new{Enter}",
    );
    expect(screen.getByTestId("label-picker-no-match")).toHaveTextContent(
      "No label in acme/api matches “brand-new”. Only existing labels can be selected; create new ones in Manage labels.",
    );
    expect(seen).toEqual([]);
    expect(selectedChips()).toEqual([]);

    // Enter on an exact (case-insensitive) name selects the catalog's label.
    await user.clear(screen.getByTestId("label-picker-search"));
    await user.type(screen.getByTestId("label-picker-search"), "WIP{Enter}");
    expect(seen).toEqual([["wip"]]);
  });

  it("keeps a selected name the catalog lacks removable, never re-addable", async () => {
    const user = userEvent.setup();
    serveLabelCatalog();
    renderPicker({
      initial: ["gone"],
      known: [{ name: "gone", color: "000000" }],
    });

    await screen.findByTestId("label-picker-option-bug");
    expect(selectedChips()).toEqual(["gone"]);
    expect(
      screen.queryByTestId("label-picker-option-gone"),
    ).not.toBeInTheDocument();
    await user.click(screen.getByTestId("label-picker-selected-remove-gone"));
    expect(selectedChips()).toEqual([]);
    await user.type(screen.getByTestId("label-picker-search"), "gone{Enter}");
    expect(selectedChips()).toEqual([]);
  });

  it("stops at the limit", async () => {
    const user = userEvent.setup();
    serveLabelCatalog();
    renderPicker({ limit: 2 });

    await user.click(await screen.findByTestId("label-picker-option-bug"));
    await user.click(screen.getByTestId("label-picker-option-wip"));
    expect(
      screen.getByTestId("label-picker-option-needs triage"),
    ).toBeDisabled();
    expect(screen.getByTestId("label-picker-full")).toHaveTextContent(
      "At most 2 labels can be selected.",
    );
  });

  it("points an empty catalog to Manage labels, which opens the domain's catalog editor", async () => {
    const user = userEvent.setup();
    serveLabelCatalog("domain-1", makeLabelPage({ labels: [] }));
    renderPicker();

    expect(await screen.findByTestId("label-picker-empty")).toHaveTextContent(
      "acme/api has no labels yet. Create them in Manage labels.",
    );
    await user.click(screen.getByTestId("label-picker-manage"));
    const dialog = await screen.findByTestId("label-catalog-dialog");
    expect(within(dialog).getByText("Labels · acme/api")).toBeVisible();
    expect(within(dialog).getByTestId("label-catalog-new")).toBeVisible();
    await user.click(within(dialog).getByTestId("label-catalog-dialog-close"));
    expect(
      screen.queryByTestId("label-catalog-dialog"),
    ).not.toBeInTheDocument();
  });

  it("shows why the labels cannot be changed while disabled", async () => {
    serveLabelCatalog();
    renderPicker({
      disabled: true,
      disabledReason: "Connect your GitHub account to change labels.",
      initial: ["bug"],
    });

    expect(await screen.findByTestId("label-picker-option-bug")).toBeDisabled();
    expect(screen.getByTestId("label-picker-disabled")).toHaveTextContent(
      "Connect your GitHub account to change labels.",
    );
    expect(
      screen.getByTestId("label-picker-selected-remove-bug"),
    ).toBeDisabled();
  });
});
