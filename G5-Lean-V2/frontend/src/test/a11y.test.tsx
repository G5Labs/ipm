// =============================================================================
// Shared-Primitive Accessibility Guard (design_system D4/D5)
// =============================================================================
// Every shared primitive in components/ui must pass axe-core with zero
// violations: new UI composed from primitives inherits the accessibility
// floor (design_system I3), so a defect here multiplies across every surface.
// This suite is also the usage pattern for component-level axe assertions —
// render, then `expect(await axeViolations(container)).toEqual([])`.
// =============================================================================

import React from "react";
import { describe, expect, it } from "vitest";
import { render } from "@testing-library/react";
import { axeViolations } from "./axe";
import {
  Button,
  Chip,
  IconButton,
  SectionTitle,
  ThemedSelect,
} from "../components/ui/UI";
import { Modal } from "../components/ui/Modal";

describe("shared primitives pass axe-core", () => {
  it("Button in all three kinds, enabled and disabled", async () => {
    const { container } = render(
      <main>
        <Button kind="primary">Save</Button>
        <Button kind="secondary">Cancel</Button>
        <Button kind="ghost">Dismiss</Button>
        <Button kind="primary" disabled>
          Saving…
        </Button>
      </main>,
    );
    expect(await axeViolations(container)).toEqual([]);
  });

  it("IconButton exposes its title as the accessible name", async () => {
    const { container, getByRole } = render(
      <main>
        <IconButton title="Close panel">×</IconButton>
      </main>,
    );
    expect(getByRole("button", { name: "Close panel" })).toBeInTheDocument();
    expect(await axeViolations(container)).toEqual([]);
  });

  it("Chip status vocabulary renders accessibly in every state", async () => {
    const { container } = render(
      <main>
        <Chip state="ok" />
        <Chip state="info" />
        <Chip state="warn" />
        <Chip state="blocked" />
        <Chip state="active" />
        <Chip state="muted" text="Archived" />
      </main>,
    );
    expect(await axeViolations(container)).toEqual([]);
  });

  it("SectionTitle and ThemedSelect", async () => {
    const options = [
      { value: "a", label: "Alpha" },
      { value: "b", label: "Beta" },
    ];
    const { container } = render(
      <main>
        <SectionTitle>Workspaces</SectionTitle>
        <label htmlFor="pick">Pick one</label>
        <ThemedSelect
          id="pick"
          value="a"
          options={options}
          onChange={() => {}}
        />
      </main>,
    );
    expect(await axeViolations(container)).toEqual([]);
  });

  it("Modal shell with a visible title", async () => {
    render(
      <Modal labelledBy="dlg-title" onClose={() => {}}>
        <h2 id="dlg-title">Confirm removal</h2>
        <p>This action cannot be undone.</p>
        <Button kind="secondary">Cancel</Button>
        <Button kind="primary">Remove</Button>
      </Modal>,
    );
    // The Modal portals into document.body; scan the whole document so the
    // dialog wiring (role, aria-modal, accessible name) is in scope.
    expect(await axeViolations(document.body)).toEqual([]);
  });
});
