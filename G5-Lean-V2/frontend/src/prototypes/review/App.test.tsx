import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import ReviewMock from "./App";

const nav = (name: string) =>
  fireEvent.click(
    within(
      name === "Approvals"
        ? screen.getByRole("toolbar")
        : screen.getByRole("navigation", { name: "Workspace navigation" }),
    ).getByRole("button", { name }),
  );
const role = (value: string) =>
  fireEvent.change(screen.getByLabelText("View as"), { target: { value } });

describe("native review mock flows", () => {
  it("consolidates landing and keeps approvals as the last ribbon action", () => {
    render(<ReviewMock />);
    expect(screen.getByRole("heading", { name: "Timeline" })).toBeVisible();
    expect(
      screen.queryByRole("tab", { name: "Timeline" }),
    ).not.toBeInTheDocument();
    const toolbar = screen.getByRole("toolbar");
    const buttons = within(toolbar).getAllByRole("button");
    expect(buttons.at(-1)).toHaveAccessibleName("Approvals");
    expect(
      within(
        screen.getByRole("navigation", { name: "Workspace navigation" }),
      ).queryByRole("button", { name: "Approvals" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Submit review" }),
    ).not.toBeInTheDocument();
    nav("Approvals");
    expect(
      screen.getByRole("button", { name: "Review requests (2)" }),
    ).toBeEnabled();
    role("viewer");
    expect(
      screen.queryByRole("button", { name: "Submit review" }),
    ).not.toBeInTheDocument();
  });
  it("walks a stopped owner from no PR through independent configured requests and a reviewer decision", () => {
    render(<ReviewMock />);
    fireEvent.change(screen.getByLabelText("Workspace example"), {
      target: { value: "rahul-test" },
    });
    nav("Source Control");
    expect(
      screen.queryByRole("option", { name: "Since my last review" }),
    ).not.toBeInTheDocument();
    nav("Approvals");
    expect(
      screen.getByRole("button", { name: "Request review" }),
    ).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "Submit PR" }));
    fireEvent.click(
      within(screen.getByRole("dialog")).getByRole("button", {
        name: "Submit PR",
      }),
    );
    fireEvent.click(screen.getByRole("button", { name: "Request review" }));
    const dialog = within(screen.getByRole("dialog"));
    fireEvent.change(dialog.getByLabelText("Approval type"), {
      target: { value: "release" },
    });
    fireEvent.click(dialog.getByRole("checkbox", { name: "James Moore" }));
    fireEvent.click(dialog.getByRole("checkbox", { name: "Ziniu Wu" }));
    fireEvent.click(
      dialog.getByRole("button", { name: "Send review request" }),
    );
    role("reviewer");
    fireEvent.click(screen.getByRole("button", { name: /Review requests \(/ }));
    const review = within(screen.getByRole("dialog"));
    fireEvent.click(review.getByRole("radio", { name: /Request changes/ }));
    expect(
      review.getByRole("button", { name: "Submit review" }),
    ).toBeDisabled();
    fireEvent.change(review.getByLabelText("Review comment"), {
      target: { value: "Include a rollback owner." },
    });
    fireEvent.click(review.getByRole("button", { name: "Submit review" }));
    expect(screen.getByText("Include a rollback owner.")).toBeVisible();
    expect(screen.getByText("Awaiting review")).toBeVisible();
    expect(
      screen.queryByRole("button", { name: /Review requests \(/ }),
    ).not.toBeInTheDocument();
  });
  it("shares file discussion and owner replies with the timeline", () => {
    render(<ReviewMock />);
    nav("Core Ontology");
    fireEvent.change(screen.getByLabelText("Write a comment"), {
      target: { value: "Should retry keep the same profile?" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Comment" }));
    nav("Workspace");
    expect(
      screen.getByText("Should retry keep the same profile?"),
    ).toBeVisible();
    role("owner");
    const thread = screen
      .getByText("Should retry keep the same profile?")
      .closest("article")!;
    fireEvent.click(within(thread).getByRole("button", { name: "Reply" }));
    fireEvent.change(screen.getByLabelText("Write a reply"), {
      target: { value: "Yes, the session retains its profile." },
    });
    fireEvent.click(screen.getByRole("button", { name: "Post reply" }));
    nav("Core Ontology");
    expect(
      screen.getByText("Yes, the session retains its profile."),
    ).toBeVisible();
  });
  it("offers current content only to a running owner and keeps it outside pushed content", () => {
    render(<ReviewMock />);
    nav("Source Control");
    expect(
      screen.queryByRole("option", {
        name: /Uncommitted changes|All unpushed changes/,
      }),
    ).not.toBeInTheDocument();
    role("owner");
    fireEvent.click(screen.getByRole("button", { name: "Start" }));
    fireEvent.change(screen.getByLabelText("File view"), {
      target: { value: "unpushed" },
    });
    fireEvent.change(screen.getByLabelText("Current file contents"), {
      target: { value: "Owner-only dirty change" },
    });
    role("reviewer");
    expect(
      screen.queryByText("Owner-only dirty change"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("option", {
        name: /Uncommitted changes|All unpushed changes/,
      }),
    ).not.toBeInTheDocument();
  });
  it("shows docs-only changes since the prior scope-specific review with first-review fallback", () => {
    render(<ReviewMock />);
    nav("Source Control");
    fireEvent.change(screen.getByLabelText("File view"), {
      target: { value: "last" },
    });
    expect(
      screen.queryByRole("button", { name: /Preview session cleanup/ }),
    ).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /Preview lifecycle requirements/ }),
    ).toBeVisible();
    nav("Project Docs");
    expect(
      screen.getByText("Preview lifecycle requirements", {
        selector: ".rm-file-list span",
      }),
    ).toBeVisible();
    fireEvent.change(screen.getByLabelText("Comparison review type"), {
      target: { value: "business" },
    });
    expect(
      screen.getByText(/You have not submitted this review type yet/),
    ).toBeVisible();
  });
  it("discloses a concurrent push, preserves draft text, and records the captured vector", () => {
    render(<ReviewMock />);
    nav("Approvals");
    fireEvent.click(screen.getByRole("button", { name: /Review requests \(/ }));
    const dialog = within(screen.getByRole("dialog"));
    fireEvent.change(dialog.getByLabelText("Review comment"), {
      target: { value: "Reviewed captured docs." },
    });
    fireEvent.click(dialog.getByRole("button", { name: "Simulate docs push" }));
    expect(dialog.getByRole("status")).toHaveTextContent(
      "Your draft still reviews the captured revision",
    );
    fireEvent.click(dialog.getByRole("button", { name: "Submit review" }));
    expect(screen.getByText("Reviewed captured docs.")).toBeVisible();
    expect(screen.getAllByText("docs changed since this review")).toHaveLength(
      1,
    );
  });
  it("preserves a changes-request draft while inspecting a new push", () => {
    render(<ReviewMock />);
    nav("Approvals");
    fireEvent.click(screen.getByRole("button", { name: /Review requests \(/ }));
    let dialog = within(screen.getByRole("dialog"));
    fireEvent.click(dialog.getByRole("radio", { name: /Request changes/ }));
    fireEvent.change(dialog.getByLabelText("Review comment"), {
      target: { value: "Keep this feedback while I inspect." },
    });
    fireEvent.click(dialog.getByRole("button", { name: "Simulate docs push" }));
    fireEvent.click(
      dialog.getByRole("button", {
        name: "Keep draft text and inspect latest changes",
      }),
    );
    nav("Approvals");
    fireEvent.click(screen.getByRole("button", { name: /Review requests \(/ }));
    dialog = within(screen.getByRole("dialog"));
    expect(dialog.getByLabelText("Review comment")).toHaveValue(
      "Keep this feedback while I inspect.",
    );
    expect(
      dialog.getByRole("radio", { name: /Request changes/ }),
    ).toBeChecked();
  });
  it("keeps Approvals in the same rail position through every engineering phase", () => {
    render(<ReviewMock />);
    role("owner");
    nav("Approvals");
    for (const phase of [
      "01 Business Engineering",
      "02 System Engineering",
      "03 Integration Engineering",
    ]) {
      fireEvent.click(screen.getByRole("button", { name: phase }));
      expect(
        screen.getByRole("heading", { name: "Approvals", level: 1 }),
      ).toBeVisible();
      expect(
        within(screen.getByRole("toolbar")).queryByText("Approval"),
      ).not.toBeInTheDocument();
      expect(
        screen.getByRole("button", { name: "Request review" }),
      ).toBeEnabled();
    }
  });
  it("offers separate requested types and keeps their drafts and decisions independent", () => {
    render(<ReviewMock />);
    nav("Approvals");
    fireEvent.click(
      screen.getByRole("button", { name: "Review requests (2)" }),
    );
    let dialog = within(screen.getByRole("dialog"));
    expect(dialog.getAllByRole("option")).toHaveLength(2);
    fireEvent.change(dialog.getByLabelText("Review comment"), {
      target: { value: "Security-specific draft" },
    });
    fireEvent.click(dialog.getByRole("radio", { name: /Request changes/ }));
    fireEvent.change(dialog.getByLabelText("Approval type"), {
      target: { value: "business" },
    });
    expect(dialog.getByLabelText("Review comment")).toHaveValue("");
    expect(dialog.getByRole("radio", { name: /Approve/ })).toBeChecked();
    fireEvent.change(dialog.getByLabelText("Review comment"), {
      target: { value: "Business expectations confirmed" },
    });
    fireEvent.click(dialog.getByRole("button", { name: "Submit review" }));
    expect(
      screen.getByRole("button", { name: "Review requests (1)" }),
    ).toBeEnabled();
    fireEvent.click(
      screen.getByRole("button", { name: "Review requests (1)" }),
    );
    dialog = within(screen.getByRole("dialog"));
    expect(dialog.getByLabelText("Review comment")).toHaveValue(
      "Security-specific draft",
    );
    expect(
      dialog.getByRole("radio", { name: /Request changes/ }),
    ).toBeChecked();
  });
  it("lets unrequested viewers leave feedback without formal decisions and shares owner replies", () => {
    render(<ReviewMock />);
    role("viewer");
    nav("Approvals");
    expect(
      screen.queryByRole("button", { name: /Review requests \(/ }),
    ).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Leave feedback" }));
    const dialog = within(screen.getByRole("dialog"));
    expect(dialog.queryByRole("radio")).not.toBeInTheDocument();
    expect(dialog.queryByLabelText("Approval type")).not.toBeInTheDocument();
    fireEvent.change(dialog.getByLabelText("Feedback"), {
      target: { value: "Please add a recovery example." },
    });
    fireEvent.click(dialog.getByRole("button", { name: "Post feedback" }));
    expect(screen.queryByText("Alex Chen · Approved")).not.toBeInTheDocument();
    role("owner");
    const thread = screen
      .getByText("Please add a recovery example.")
      .closest("article")!;
    fireEvent.click(within(thread).getByRole("button", { name: "Reply" }));
    fireEvent.change(screen.getByLabelText("Write a reply"), {
      target: { value: "Added a recovery example." },
    });
    fireEvent.click(screen.getByRole("button", { name: "Post reply" }));
    nav("Workspace");
    expect(screen.getByText("Added a recovery example.")).toBeVisible();
    nav("Source Control");
    expect(
      screen.queryByText("Added a recovery example."),
    ).not.toBeInTheDocument();
    role("viewer");
    fireEvent.change(screen.getByLabelText("File view"), {
      target: { value: "last" },
    });
    expect(
      screen.getByText(/You have not submitted this review type yet/),
    ).toBeVisible();
  });
  it("allows general feedback before a PR while keeping formal requests unavailable", () => {
    render(<ReviewMock />);
    fireEvent.change(screen.getByLabelText("Workspace example"), {
      target: { value: "rahul-test" },
    });
    role("viewer");
    nav("Approvals");
    fireEvent.click(screen.getByRole("button", { name: "Leave feedback" }));
    const dialog = within(screen.getByRole("dialog"));
    fireEvent.change(dialog.getByLabelText("Feedback"), {
      target: { value: "Early task feedback" },
    });
    fireEvent.click(dialog.getByRole("button", { name: "Post feedback" }));
    expect(screen.getByText("No submitted reviews.")).toBeVisible();
    nav("Workspace");
    expect(screen.getByText("Early task feedback")).toBeVisible();
    expect(screen.queryByText(/Approved ·/)).not.toBeInTheDocument();
  });
  it("opens full descriptions and keeps landing changes against saved base", () => {
    render(<ReviewMock />);
    fireEvent.click(screen.getByRole("button", { name: /Issue #343/ }));
    expect(
      within(screen.getByRole("dialog")).getByText(/Acceptance criteria/),
    ).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "Close description" }));
    fireEvent.click(
      screen.getByRole("button", {
        name: "Read full implementation description",
      }),
    );
    expect(
      within(screen.getByRole("dialog")).getByText(/Review focus:/),
    ).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "Close description" }));
    expect(screen.getByText(/Parent has advanced in/)).toBeVisible();
    fireEvent.click(
      screen.getByRole("button", { name: /Project Docs 2 document changes/ }),
    );
    expect(screen.getByLabelText("File view")).toHaveValue("base");
    fireEvent.click(
      screen.getByRole("button", { name: /Rollout and rollback/ }),
    );
    expect(screen.queryByText(/Parent update:/)).not.toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("File view"), {
      target: { value: "parent" },
    });
    expect(screen.getByText(/Parent update:/)).toBeVisible();
  });
  it("uses the same file choices across docs, source and files and falls back on stop", () => {
    render(<ReviewMock />);
    role("owner");
    fireEvent.click(screen.getByRole("button", { name: "Start" }));
    for (const name of ["Project Docs", "Source Control", "Files"]) {
      nav(name);
      expect(
        within(screen.getByLabelText("File view"))
          .getAllByRole("option")
          .map((o) => o.textContent),
      ).toEqual([
        "All files",
        "Uncommitted changes",
        "All unpushed changes",
        "Pushed vs workspace base",
        "Pushed vs parent",
        "Since my last review",
      ]);
      expect(
        screen.getByRole("button", { name: /Workspace guide/ }),
      ).toBeVisible();
    }
    fireEvent.change(screen.getByLabelText("File view"), {
      target: { value: "uncommitted" },
    });
    expect(
      screen.queryByRole("button", { name: /Rollout and rollback/ }),
    ).not.toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("File view"), {
      target: { value: "unpushed" },
    });
    expect(
      screen.getByRole("button", { name: /Rollout and rollback/ }),
    ).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "Stop" }));
    expect(screen.getByLabelText("File view")).toHaveValue("base");
    expect(screen.getByText(/Live work is unavailable/)).toBeVisible();
    expect(
      screen.queryByLabelText("Current file contents"),
    ).not.toBeInTheDocument();
  });
  it("shows complete workspace vectors and inspects changes between consecutive revisions", () => {
    render(<ReviewMock />);
    for (const name of [
      "Project Docs",
      "Core Ontology",
      "Files",
      "Source Control",
    ]) {
      nav(name);
      expect(
        screen.queryByRole("button", { name: "Return to workspace" }),
      ).not.toBeInTheDocument();
    }
    fireEvent.click(screen.getByRole("button", { name: "Commit History" }));
    const dialog = within(
      screen.getByRole("dialog", { name: "Commit History" }),
    );
    expect(
      dialog.queryByLabelText("History repository"),
    ).not.toBeInTheDocument();
    const table = within(
      dialog.getByRole("table", { name: "Workspace commit vectors" }),
    );
    expect(
      table.getAllByRole("columnheader").map((el) => el.textContent),
    ).toEqual(["Workspace revision", "G5-Lean", "docs"]);
    for (const row of table.getAllByRole("row").slice(1))
      expect(within(row).getAllByRole("cell")).toHaveLength(2);
    const latest = within(table.getAllByRole("row")[1]);
    expect(latest.getByText("8c23f41")).toBeVisible();
    expect(latest.getByText("5ac97e2")).toBeVisible();
    expect(
      within(latest.getAllByRole("cell")[0]).getByText("Unchanged"),
    ).toBeVisible();
    expect(
      within(latest.getAllByRole("cell")[1]).getByText("Updated"),
    ).toBeVisible();
    expect(
      dialog.queryByRole("button", {
        name: "Initialize preview support and documentation",
      }),
    ).not.toBeInTheDocument();
    fireEvent.click(dialog.getByRole("button", { name: "Load more" }));
    expect(dialog.getByText("Workspace base")).toBeVisible();
    expect(
      dialog.getByRole("button", {
        name: "Initialize preview support and documentation",
      }),
    ).toBeVisible();
    fireEvent.click(
      dialog.getByRole("button", {
        name: "Establish the preview lifecycle baseline",
      }),
    );
    // One vector transition can contain changed files from both repositories.
    const options = within(
      dialog.getByLabelText("Revision changed file"),
    ).getAllByRole("option");
    expect(options.some((el) => el.textContent?.startsWith("G5-Lean ·"))).toBe(
      true,
    );
    expect(options.some((el) => el.textContent?.startsWith("docs ·"))).toBe(
      true,
    );
    fireEvent.click(
      dialog.getByRole("button", { name: "All workspace revisions" }),
    );
    fireEvent.click(
      dialog.getByRole("button", {
        name: "Make cleanup failure and retry behavior explicit",
      }),
    );
    expect(
      dialog.getByText(
        /Changes since previous workspace revision: Document the preview rollout/,
      ),
    ).toBeVisible();
    fireEvent.change(dialog.getByLabelText("Revision changed file"), {
      target: { value: "ontology" },
    });
    expect(dialog.getByText(/A failed cleanup is visible/)).toBeVisible();
    expect(
      dialog.queryByLabelText("Current file contents"),
    ).not.toBeInTheDocument();
    fireEvent.click(
      dialog.getByRole("button", { name: "Close commit history" }),
    );
    expect(screen.getByLabelText("File view")).toHaveValue("all");
  });
  it("uses the same complete vector and push evidence for PR state and history after a docs-only push", () => {
    render(<ReviewMock />);
    fireEvent.click(screen.getByRole("button", { name: "PR submitted" }));
    let dialog = within(
      screen.getByRole("dialog", { name: "Pushed revision" }),
    );
    const appCell = dialog.getByText("8c23f41").closest("td")!;
    const original = appCell.textContent;
    expect(appCell.querySelector("time")).toHaveAttribute(
      "datetime",
      "2026-10-10T15:45:00Z",
    );
    expect(
      dialog.getByText("5ac97e2").closest("td")!.querySelector("time"),
    ).toHaveAttribute("datetime", "2026-10-11T13:15:00Z");
    fireEvent.click(dialog.getByRole("button", { name: "Close revision" }));
    fireEvent.click(screen.getByRole("button", { name: "Simulate docs push" }));
    fireEvent.click(screen.getByRole("button", { name: "PR submitted" }));
    dialog = within(screen.getByRole("dialog", { name: "Pushed revision" }));
    expect(dialog.getByText("8c23f41").closest("td")!.textContent).toBe(
      original,
    );
    const currentTable = dialog.getByRole("table", {
      name: "Workspace commit vectors",
    });
    const currentRow = within(currentTable).getAllByRole("row")[1];
    const expected = currentRow.textContent;
    expect(
      within(currentRow).getAllByRole("cell")[1].querySelector("time"),
    ).not.toHaveAttribute("datetime", "2026-10-11T13:15:00Z");
    fireEvent.click(dialog.getByRole("button", { name: "Close revision" }));
    nav("Source Control");
    fireEvent.click(screen.getByRole("button", { name: "Commit History" }));
    dialog = within(screen.getByRole("dialog", { name: "Commit History" }));
    const table = dialog.getByRole("table", {
      name: "Workspace commit vectors",
    });
    expect(within(table).getAllByRole("row")[1].textContent).toBe(expected);
  });
  it("keeps history pushed-only before PR submission, including for a running owner", () => {
    render(<ReviewMock />);
    fireEvent.change(screen.getByLabelText("Workspace example"), {
      target: { value: "rahul-test" },
    });
    role("owner");
    fireEvent.click(screen.getByRole("button", { name: "Start" }));
    nav("Source Control");
    fireEvent.click(screen.getByRole("button", { name: "Commit History" }));
    const dialog = within(
      screen.getByRole("dialog", { name: "Commit History" }),
    );
    expect(
      dialog.getByRole("table", { name: "Workspace commit vectors" }),
    ).toBeVisible();
    expect(dialog.queryByText("12ad076")).not.toBeInTheDocument();
    expect(dialog.queryByText("77c618d")).not.toBeInTheDocument();
  });
});
