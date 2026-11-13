import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it } from "vitest";

import { SearchField } from "./SearchField";

const Probe = () => {
  const [value, setValue] = useState("");
  return (
    <SearchField
      testId="probe-search"
      label="Search things"
      value={value}
      onChange={setValue}
    />
  );
};

describe("SearchField", () => {
  it("names itself, reports typing and clears with its control", async () => {
    render(<Probe />);
    const input = screen.getByRole("searchbox", { name: "Search things" });
    expect(input).toHaveAttribute("placeholder", "Search things");
    expect(screen.queryByTestId("probe-search-clear")).toBeNull();
    await userEvent.type(input, "abc");
    expect(input).toHaveValue("abc");
    await userEvent.click(screen.getByTestId("probe-search-clear"));
    expect(input).toHaveValue("");
    expect(screen.queryByTestId("probe-search-clear")).toBeNull();
  });
});
