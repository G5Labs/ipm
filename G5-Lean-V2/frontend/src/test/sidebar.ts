// =============================================================================
// Sidebar test helpers
// =============================================================================
// The LeftSidebar renders stacked, independently collapsible sections. Tests
// that assert on a given section's content should expand that section first.
// =============================================================================

import { fireEvent, screen, within } from "@testing-library/react";

export type SidebarSectionTestId =
  | "sidebar-docs-tab"
  | "sidebar-all-docs-tab"
  | "sidebar-source-control-tab"
  | "sidebar-ontology-tab"
  | "sidebar-tasks-tab"
  | "sidebar-files-tab";

/**
 * Expand a sidebar section if needed. Safe to call repeatedly: it targets the
 * section header button and only clicks while `aria-expanded="false"`.
 */
export function activateSidebarSection(testId: SidebarSectionTestId): void {
  const section = screen.getByTestId(testId);
  const sectionButton = within(section).getAllByRole("button")[0];
  if (sectionButton.getAttribute("aria-expanded") !== "true") {
    fireEvent.click(sectionButton);
  }
}

/** Promise-based variant that first waits for the section wrapper to appear. */
export async function activateSidebarSectionAsync(
  testId: SidebarSectionTestId,
): Promise<void> {
  const section = await screen.findByTestId(testId);
  const sectionButton = within(section).getAllByRole("button")[0];
  if (sectionButton.getAttribute("aria-expanded") !== "true") {
    fireEvent.click(sectionButton);
  }
}
