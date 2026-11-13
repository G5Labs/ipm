// #1370 dropdown readability (design-system D6, chat intent 2bca6571):
// truncated labels always have a hover recourse (native title tooltips on the
// trigger and every option), and the open listbox grows to its content
// instead of clamping to the trigger width, so options sharing long prefixes
// stay distinguishable in narrow columns.
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { ThemedSelect } from "./UI";

const OPTIONS = [
  { value: "gpt-5.6-sol", label: "GPT 5.6 Sol" },
  { value: "gpt-5.6-terra", label: "GPT 5.6 Terra" },
];

describe("ThemedSelect readability", () => {
  it("exposes the full selected label as a hover tooltip on the trigger", () => {
    render(
      <ThemedSelect
        value="gpt-5.6-sol"
        options={OPTIONS}
        onChange={vi.fn()}
        testId="probe-select"
      />,
    );
    const trigger = screen.getByTestId("probe-select");
    expect(trigger.querySelector("[title='GPT 5.6 Sol']")).not.toBeNull();
  });

  it("exposes each option's full label as a hover tooltip", () => {
    render(
      <ThemedSelect
        value="gpt-5.6-sol"
        options={OPTIONS}
        onChange={vi.fn()}
        testId="probe-select"
      />,
    );
    fireEvent.click(screen.getByTestId("probe-select"));
    const nonSelectedOption = screen.getByTestId(
      "probe-select-option-gpt-5.6-terra",
    );
    expect(nonSelectedOption).toHaveAttribute("title", "GPT 5.6 Terra");
    expect(nonSelectedOption.querySelector(".min-w-0")).toHaveAttribute(
      "title",
      "GPT 5.6 Terra",
    );
  });

  it("lets the open listbox grow beyond the trigger width", () => {
    render(
      <ThemedSelect
        value="gpt-5.6-sol"
        options={OPTIONS}
        onChange={vi.fn()}
        testId="probe-select"
      />,
    );
    fireEvent.click(screen.getByTestId("probe-select"));
    const listbox = screen.getByRole("listbox");
    // Content-width menu: min-w-full keeps it at least trigger-wide, w-max
    // lets it grow, and it is no longer clamped with right-0.
    expect(listbox.className).toContain("min-w-full");
    expect(listbox.className).toContain("w-max");
    expect(listbox.className).not.toContain("right-0");
  });

  it("opens above when an overflow-clipping ancestor leaves too little room below", async () => {
    render(
      <div data-testid="select-clip" style={{ overflowY: "auto" }}>
        <ThemedSelect
          value="gpt-5.6-sol"
          options={OPTIONS}
          onChange={vi.fn()}
          testId="probe-select"
        />
      </div>,
    );
    const trigger = screen.getByTestId("probe-select");
    const root = trigger.parentElement!;
    const clippingAncestor = screen.getByTestId("select-clip");
    vi.spyOn(root, "getBoundingClientRect").mockReturnValue({
      top: 450,
      bottom: 480,
      left: 20,
      right: 220,
      width: 200,
      height: 30,
      x: 20,
      y: 450,
      toJSON: () => ({}),
    });
    vi.spyOn(clippingAncestor, "getBoundingClientRect").mockReturnValue({
      top: 100,
      bottom: 520,
      left: 0,
      right: 300,
      width: 300,
      height: 420,
      x: 0,
      y: 100,
      toJSON: () => ({}),
    });

    fireEvent.click(trigger);

    const listbox = screen.getByRole("listbox");
    await waitFor(() =>
      expect(listbox).toHaveAttribute("data-placement", "above"),
    );
    expect(listbox.className).toContain("bottom-full");
    expect(listbox.className).not.toContain("top-full");
  });

  it("recomputes placement when the viewport layout changes", async () => {
    render(
      <div data-testid="select-clip" style={{ overflowY: "auto" }}>
        <ThemedSelect
          value="gpt-5.6-sol"
          options={OPTIONS}
          onChange={vi.fn()}
          testId="probe-select"
        />
      </div>,
    );
    const trigger = screen.getByTestId("probe-select");
    const root = trigger.parentElement!;
    const clippingAncestor = screen.getByTestId("select-clip");
    let triggerTop = 450;
    vi.spyOn(root, "getBoundingClientRect").mockImplementation(() => ({
      top: triggerTop,
      bottom: triggerTop + 30,
      left: 20,
      right: 220,
      width: 200,
      height: 30,
      x: 20,
      y: triggerTop,
      toJSON: () => ({}),
    }));
    vi.spyOn(clippingAncestor, "getBoundingClientRect").mockReturnValue({
      top: 100,
      bottom: 520,
      left: 0,
      right: 300,
      width: 300,
      height: 420,
      x: 0,
      y: 100,
      toJSON: () => ({}),
    });

    fireEvent.click(trigger);
    const listbox = screen.getByRole("listbox");
    await waitFor(() =>
      expect(listbox).toHaveAttribute("data-placement", "above"),
    );

    triggerTop = 140;
    fireEvent(window, new Event("resize"));

    await waitFor(() =>
      expect(listbox).toHaveAttribute("data-placement", "below"),
    );
    expect(listbox.className).toContain("top-full");
  });

  it("keeps disabled styling authoritative when an option is also muted", () => {
    render(
      <ThemedSelect
        value="gpt-5.6-sol"
        options={[OPTIONS[0], { ...OPTIONS[1], muted: true, disabled: true }]}
        onChange={vi.fn()}
        testId="probe-select"
      />,
    );

    fireEvent.click(screen.getByTestId("probe-select"));
    const disabledOption = screen.getByTestId(
      "probe-select-option-gpt-5.6-terra",
    );
    expect(disabledOption.className).toContain("opacity-50");
    expect(disabledOption.className).not.toContain("opacity-60");
  });

  it("keeps assistive option descriptions outside the listbox", () => {
    render(
      <ThemedSelect
        value="gpt-5.6-sol"
        options={[
          OPTIONS[0],
          {
            ...OPTIONS[1],
            assistiveDescription: "No compatible worker connected",
          },
        ]}
        onChange={vi.fn()}
        testId="probe-select"
      />,
    );

    fireEvent.click(screen.getByTestId("probe-select"));
    const listbox = screen.getByRole("listbox");
    expect(
      Array.from(listbox.children).every(
        (child) => child.getAttribute("role") === "option",
      ),
    ).toBe(true);
    const option = screen.getByTestId("probe-select-option-gpt-5.6-terra");
    const descriptionId = option.getAttribute("aria-describedby");
    expect(descriptionId).toBeTruthy();
    const description = document.getElementById(descriptionId!);
    expect(description).toHaveTextContent("No compatible worker connected");
    expect(listbox).not.toContainElement(description);
  });

  it("can show a plain caret and take its name from labelling elements", () => {
    render(
      <>
        <span id="probe-label">Probe</span>
        <ThemedSelect
          id="probe-trigger"
          value="gpt-5.6-sol"
          options={OPTIONS}
          onChange={vi.fn()}
          testId="probe-select"
          indicator="caret"
          labelledBy="probe-label probe-trigger"
        />
      </>,
    );
    const trigger = screen.getByRole("button", { name: "Probe GPT 5.6 Sol" });
    expect(trigger).toHaveTextContent(/^GPT 5\.6 Sol▾$/);
    expect(trigger.querySelector("svg")).toBeNull();
  });

  it("supports native-select keyboard use and names its list", async () => {
    const onChange = vi.fn();
    render(
      <>
        <span id="kb-label">Model</span>
        <ThemedSelect
          id="kb-trigger"
          value="b"
          options={[
            { value: "a", label: "Alpha" },
            { value: "b", label: "Beta" },
            { value: "c", label: "Gamma", disabled: true },
            { value: "d", label: "Delta" },
          ]}
          onChange={onChange}
          testId="kb-select"
          labelledBy="kb-label"
        />
      </>,
    );
    const trigger = screen.getByTestId("kb-select");
    trigger.focus();
    fireEvent.keyDown(trigger, { key: "ArrowDown" });
    const list = await screen.findByRole("listbox", { name: "Model" });
    const option = (name: string) => screen.getByRole("option", { name });
    await waitFor(() => expect(option("Beta")).toHaveFocus());
    fireEvent.keyDown(list, { key: "ArrowDown" });
    expect(option("Delta")).toHaveFocus(); // skips the disabled option
    fireEvent.keyDown(list, { key: "ArrowDown" });
    expect(option("Delta")).toHaveFocus(); // stays at the end
    fireEvent.keyDown(list, { key: "Home" });
    expect(option("Alpha")).toHaveFocus();
    fireEvent.keyDown(list, { key: "End" });
    expect(option("Delta")).toHaveFocus();
    fireEvent.keyDown(list, { key: "ArrowUp" });
    expect(option("Beta")).toHaveFocus();
    fireEvent.click(option("Beta"));
    expect(onChange).toHaveBeenCalledWith("b");
    expect(screen.queryByRole("listbox")).toBeNull();
    expect(trigger).toHaveFocus();
  });
});
