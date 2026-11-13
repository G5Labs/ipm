import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { PhaseChevrons } from "./PhaseChevrons";

// Workbench g5.workbench.phases / g5.workbench.phases.state: three chevrons,
// the active one highlighted, earlier ones done; the owner moves the phase by
// clicking, everyone else (and Base) sees the same chevrons read-only.

describe("PhaseChevrons", () => {
  it("renders the three phases with the active one pressed and earlier ones done", () => {
    render(
      <PhaseChevrons phase="se" variant="extended" onSelectPhase={() => {}} />,
    );

    const nav = screen.getByTestId("phase-chevrons");
    expect(nav).toHaveAttribute("data-phase", "se");
    expect(
      screen.getByRole("button", { name: "01 Business Engineering" }),
    ).toHaveAttribute("data-state", "done");
    const active = screen.getByRole("button", {
      name: "02 System Engineering",
    });
    expect(active).toHaveAttribute("aria-pressed", "true");
    expect(active).toHaveAttribute("data-state", "active");
    expect(
      screen.getByRole("button", { name: "03 Integration Engineering" }),
    ).toHaveAttribute("data-state", "upcoming");
    expect(nav).toHaveAttribute("data-variant", "extended");
    expect(screen.getByText("Requirement doc + mock")).toBeInTheDocument();
    expect(screen.getByText("Design doc + prototype")).toBeInTheDocument();
    expect(
      screen.getByText("Code update + operation plan"),
    ).toBeInTheDocument();
  });

  // g5.workbench.header-styles: minimal is the default and drops the
  // deliverable subline; vertical keeps the full names, icons, states and
  // meters in a column; the move and read-only rules hold in every style.
  it("defaults to the minimal style without the deliverable sublines", () => {
    render(<PhaseChevrons phase="se" onSelectPhase={() => {}} />);

    expect(screen.getByTestId("phase-chevrons")).toHaveAttribute(
      "data-variant",
      "minimal",
    );
    expect(screen.getByText("System Engineering")).toBeInTheDocument();
    expect(
      screen.queryByText("Design doc + prototype"),
    ).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "01 Business Engineering" }),
    ).toHaveAttribute("data-state", "done");
  });

  it("renders the vertical column with full names, states, meters and the owner move", async () => {
    const onSelectPhase = vi.fn();
    render(
      <PhaseChevrons
        phase="se"
        variant="vertical"
        onSelectPhase={onSelectPhase}
        approvalMeters={{ ie: <span>0/1</span> }}
        error="refused"
      />,
    );

    const nav = screen.getByTestId("phase-chevrons");
    expect(nav).toHaveAttribute("data-variant", "vertical");
    expect(
      screen.getByRole("button", { name: "01 Business Engineering" }),
    ).toHaveTextContent("Business Engineering");
    expect(
      screen.getByRole("button", { name: "01 Business Engineering" }),
    ).toHaveAttribute("data-state", "done");
    expect(screen.getByTestId("phase-chevron-se")).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(screen.getByTestId("phase-chevron-ie")).toHaveTextContent(
      "Integration Engineering",
    );
    expect(screen.getByTestId("phase-meter-ie")).toHaveTextContent("0/1");
    expect(
      screen.queryByText("Requirement doc + mock"),
    ).not.toBeInTheDocument();
    expect(screen.getByRole("alert")).toHaveTextContent(
      "Could not move the phase: refused",
    );

    await userEvent.click(screen.getByTestId("phase-chevron-ie"));
    expect(onSelectPhase).toHaveBeenCalledWith("ie");
  });

  it("asks the owner's move for another phase and ignores the active one", async () => {
    const onSelectPhase = vi.fn();
    render(<PhaseChevrons phase="be" onSelectPhase={onSelectPhase} />);

    await userEvent.click(screen.getByTestId("phase-chevron-se"));
    expect(onSelectPhase).toHaveBeenCalledWith("se");

    await userEvent.click(screen.getByTestId("phase-chevron-be"));
    expect(onSelectPhase).toHaveBeenCalledTimes(1);
    expect(screen.getByTestId("phase-chevron-ie")).not.toHaveAttribute(
      "aria-disabled",
    );
  });

  it("is read-only for non-owners and Base while still showing the phase", async () => {
    const onSelectPhase = vi.fn();
    render(<PhaseChevrons phase="ie" readOnly onSelectPhase={onSelectPhase} />);

    const target = screen.getByTestId("phase-chevron-be");
    expect(target).toHaveAttribute("aria-disabled", "true");
    await userEvent.click(target);
    expect(onSelectPhase).not.toHaveBeenCalled();
    expect(screen.getByTestId("phase-chevron-ie")).toHaveAttribute(
      "aria-pressed",
      "true",
    );
  });

  it("ignores clicks while a move is pending", async () => {
    const onSelectPhase = vi.fn();
    render(<PhaseChevrons phase="be" pending onSelectPhase={onSelectPhase} />);

    await userEvent.click(screen.getByTestId("phase-chevron-se"));
    expect(onSelectPhase).not.toHaveBeenCalled();
  });

  it("renders a phase meter only when one is supplied (g5.workbench.gates)", () => {
    const { rerender } = render(<PhaseChevrons phase="be" />);
    expect(screen.queryByTestId("phase-meter-be")).not.toBeInTheDocument();

    rerender(
      <PhaseChevrons phase="be" approvalMeters={{ be: <span>1/2</span> }} />,
    );
    expect(screen.getByTestId("phase-meter-be")).toHaveTextContent("1/2");
    expect(screen.queryByTestId("phase-meter-se")).not.toBeInTheDocument();
  });
});
