import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useRef, useState } from "react";
import { describe, expect, it } from "vitest";

import { useDismissOnOutside } from "./useDismissOnOutside";

const Probe = () => {
  const ref = useRef<HTMLDivElement | null>(null);
  const [open, setOpen] = useState(true);
  useDismissOnOutside(ref, open, () => setOpen(false));
  return (
    <div>
      <div ref={ref} data-testid="inside">
        <button type="button" data-testid="inside-button">
          inside
        </button>
        {open ? <span data-testid="menu">menu</span> : null}
      </div>
      <button type="button" data-testid="outside">
        outside
      </button>
    </div>
  );
};

describe("useDismissOnOutside", () => {
  it("keeps the menu on an inside click, closes it on an outside click", async () => {
    render(<Probe />);
    await userEvent.click(screen.getByTestId("inside-button"));
    expect(screen.getByTestId("menu")).toBeInTheDocument();
    await userEvent.click(screen.getByTestId("outside"));
    expect(screen.queryByTestId("menu")).toBeNull();
  });

  it("closes on Escape", async () => {
    render(<Probe />);
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByTestId("menu")).toBeNull();
  });
});
