import { useRef } from "react";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it, vi } from "vitest";
import { renderApp } from "../../test/render";
import { server } from "../../test/msw/server";
import { CurrentOntologyReference } from "./CurrentOntologyReference";

const path = ".g5/ontology/root.g5.md";
const content =
  '<a id="keep"></a>\n[I · keep]\n\nUnsaved current body.\n\n## Metadata\n\n### keep\n\n- agent\n';
function Reference({
  repoRef,
  navigate,
}: {
  repoRef: string;
  navigate: ReturnType<typeof vi.fn>;
}) {
  const rootRef = useRef<HTMLDivElement>(null);
  return (
    <div ref={rootRef}>
      <a href={`g5://file/${path}?repo=${repoRef}#keep`}>Reference</a>
      <CurrentOntologyReference
        workspaceId="ws"
        repoId="repo-current"
        path={path}
        content={content}
        rootRef={rootRef}
        onNavigate={navigate}
      />
    </div>
  );
}

describe("qualified ontology reference ownership", () => {
  it("uses the current repository and unsaved document without requesting branch membership", async () => {
    const branches = vi.fn(() =>
      HttpResponse.json(
        { error: { code: "UNAVAILABLE", message: "Unavailable" } },
        { status: 503 },
      ),
    );
    server.use(http.get("/api/v1/workspaces/ws/branches", branches));
    const navigate = vi.fn();
    renderApp(<Reference repoRef="repo-current" navigate={navigate} />);
    fireEvent.click(screen.getByRole("link", { name: "Reference" }));
    const preview = await screen.findByTestId("ontology-reference");
    expect(
      within(preview).getByTestId("ontology-reference-content"),
    ).toHaveTextContent("Unsaved current body.");
    fireEvent.click(
      within(preview).getByRole("button", { name: "Open document" }),
    );
    expect(navigate).toHaveBeenCalledWith(
      expect.objectContaining({
        repoId: "repo-current",
        id: path,
        intentRecordNumber: "#keep",
      }),
    );
    expect(branches).not.toHaveBeenCalled();
  });

  it.each([
    { members: [] },
    {
      members: [
        { repoId: "other-1", repoName: "other" },
        { repoId: "other-2", repoName: "other" },
      ],
    },
  ])("refuses missing or ambiguous other repositories", async ({ members }) => {
    const branches = vi.fn(() => HttpResponse.json({ data: members }));
    server.use(http.get("/api/v1/workspaces/ws/branches", branches));
    const navigate = vi.fn();
    renderApp(<Reference repoRef="other" navigate={navigate} />);
    fireEvent.click(screen.getByRole("link", { name: "Reference" }));
    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent(
        "missing or ambiguous",
      ),
    );
    expect(
      screen.getByRole("button", { name: "Open document" }),
    ).toBeDisabled();
    expect(branches).toHaveBeenCalledOnce();
    expect(navigate).not.toHaveBeenCalled();
  });

  it("styles the portalled popover's body and details links as document links", async () => {
    const linked =
      '<a id="keep"></a>\n[I · keep]\n\nSee [the guide](https://example.com/guide).\n\n## Metadata\n\n### keep\n\n- [Evidence](https://example.com/evidence)\n';
    function LinkedReference() {
      const rootRef = useRef<HTMLDivElement>(null);
      return (
        <div ref={rootRef}>
          <a href={`g5://file/${path}#keep`}>Reference</a>
          <CurrentOntologyReference
            workspaceId="ws"
            repoId="repo-current"
            path={path}
            content={linked}
            rootRef={rootRef}
            onNavigate={vi.fn()}
          />
        </div>
      );
    }
    renderApp(<LinkedReference />);
    fireEvent.click(screen.getByRole("link", { name: "Reference" }));
    const preview = await screen.findByTestId("ontology-reference");
    fireEvent.click(
      within(preview).getByRole("button", { name: "Reference details" }),
    );
    for (const name of ["the guide", "Evidence"]) {
      const link = within(preview).getByRole("link", { name });
      // The popover is portalled outside the document's link-styled root.
      expect(link.closest(".g5-document-links")).not.toBeNull();
      expect(
        link
          .closest(".g5-document-links")
          ?.contains(screen.getByRole("link", { name: "Reference" })),
      ).toBe(false);
    }
  });
});
