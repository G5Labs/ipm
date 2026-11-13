// Robust ontology links (g5.documents.ontology.links): line fragments open at
// the line, relative targets fall back to the repository root and `.g5`, and
// a link that finds no file says which paths were tried. Existence is read,
// not listed, so folders hidden from Files resolve; other documents keep
// their links as authored.
import { useRef } from "react";
import { fireEvent, screen, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it, vi } from "vitest";
import { renderApp } from "../../test/render";
import { server } from "../../test/msw/server";
import { CurrentOntologyReference } from "./CurrentOntologyReference";

const path = ".g5/ontology/features/receiving.g5.md";
const files = new Set([
  "docs/receiving.md",
  "docs/knowledge/receiving.md",
  ".g5/TLA_verification/models/numbering/BINDINGS.md",
  ".g5/TLA_verification/models/numbering/manifest.json",
  "src/Create.cs",
]);

/** Serves `files`; returns the paths read. */
function serve() {
  const read: string[] = [];
  server.use(
    http.get("*/api/v1/workspaces/ws/files/*", ({ request }) => {
      const file = decodeURIComponent(
        new URL(request.url).pathname.split("/files/")[1],
      );
      read.push(file);
      return files.has(file)
        ? HttpResponse.json({
            data: {
              path: file,
              content: "x",
              size: 1,
              encoding: "utf-8",
              hash: "h",
              source: "live",
              repoOid: "oid",
              snapshotAt: null,
            },
          })
        : HttpResponse.json(
            { error: { code: "NOT_FOUND", message: "missing" } },
            { status: 404 },
          );
    }),
  );
  return read;
}

function Links({
  href,
  base,
  source = path,
  holder,
  navigate,
  open,
}: {
  href: string;
  base?: string;
  source?: string;
  /** The statement or finding holding the link. */
  holder?: Record<string, string>;
  navigate: ReturnType<typeof vi.fn>;
  open: ReturnType<typeof vi.fn>;
}) {
  const rootRef = useRef<HTMLDivElement>(null);
  return (
    <div ref={rootRef}>
      <div data-link-base={base} {...holder}>
        <a href={href}>Evidence</a>
      </div>
      <CurrentOntologyReference
        workspaceId="ws"
        repoId="repo"
        path={source}
        content=""
        rootRef={rootRef}
        onNavigate={navigate}
        onOpenWorkspaceFile={open}
      />
    </div>
  );
}

function click(href: string, base?: string, source?: string) {
  const navigate = vi.fn();
  const open = vi.fn();
  renderApp(
    <Links
      href={href}
      base={base}
      source={source}
      navigate={navigate}
      open={open}
    />,
  );
  fireEvent.click(screen.getByRole("link", { name: "Evidence" }));
  return { navigate, open };
}

describe("ontology link resolution", () => {
  it("opens a line fragment at its line", async () => {
    serve();
    const { open, navigate } = click(
      "../../../docs/knowledge/receiving.md#L26-L28",
    );
    await waitFor(() =>
      // Outside any statement there is no statement to return to.
      expect(open).toHaveBeenCalledWith(
        "docs/knowledge/receiving.md",
        "repo",
        26,
        undefined,
      ),
    );
    expect(navigate).not.toHaveBeenCalled();
  });

  const TARGET = ".g5/TLA_verification/models/numbering/BINDINGS.md";
  it.each([false, true])(
    "gives the opened file the check's read unless a newer one is there (%s)",
    async (cached) => {
      const read = serve();
      const navigate = vi.fn();
      const { queryClient } = renderApp(
        <Links
          href="TLA_verification/models/numbering/BINDINGS.md"
          navigate={navigate}
          open={vi.fn()}
        />,
      );
      const key = (file: string) => ["workspace-file", "ws", "repo", file];
      const newer = { path: TARGET, content: "newer" };
      if (cached) queryClient.setQueryData(key(TARGET), newer);
      fireEvent.click(screen.getByRole("link", { name: "Evidence" }));
      await waitFor(() =>
        expect(navigate).toHaveBeenCalledWith(
          expect.objectContaining({ id: TARGET }),
        ),
      );
      const opened = queryClient.getQueryData(key(TARGET));
      if (cached) expect(opened).toBe(newer);
      else
        expect(opened).toMatchObject({
          path: TARGET,
          content: "x",
        });
      // A candidate that was not found leaves nothing for another view.
      const missed = read.filter((file) => file !== TARGET);
      expect(missed.length).toBeGreaterThan(0);
      for (const file of missed)
        expect(queryClient.getQueryState(key(file))).toBeUndefined();
    },
  );

  it("falls back to the repository root for a link written from the wrong folder", async () => {
    serve();
    const { navigate } = click("../../../../docs/receiving.md");
    await waitFor(() =>
      expect(navigate).toHaveBeenCalledWith(
        expect.objectContaining({ type: "file", id: "docs/receiving.md" }),
      ),
    );
  });

  it("falls back to the .g5 folder", async () => {
    serve();
    const { navigate } = click("TLA_verification/models/numbering/BINDINGS.md");
    await waitFor(() =>
      expect(navigate).toHaveBeenCalledWith(
        expect.objectContaining({
          id: ".g5/TLA_verification/models/numbering/BINDINGS.md",
        }),
      ),
    );
  });

  it("says which paths it tried when no file exists", async () => {
    serve();
    const { navigate, open } = click("../../missing/file.md");
    expect(
      await screen.findByText(
        "Couldn't find ../../missing/file.md. Tried: .g5/missing/file.md, missing/file.md",
      ),
    ).toBeInTheDocument();
    expect(navigate).not.toHaveBeenCalled();
    expect(open).not.toHaveBeenCalled();
  });

  it("opens the authored path when a read cannot say whether it exists", async () => {
    server.use(
      http.get("*/api/v1/workspaces/ws/files/*", () =>
        HttpResponse.json(
          { error: { code: "INTERNAL", message: "boom" } },
          { status: 500 },
        ),
      ),
    );
    const { navigate } = click("../../../docs/guide.md");
    await waitFor(() =>
      expect(navigate).toHaveBeenCalledWith(
        expect.objectContaining({ type: "file", id: "docs/guide.md" }),
      ),
    );
  });

  it("keeps the explicit refusal of a query string", async () => {
    serve();
    click("../../docs/receiving.md?raw=1");
    expect(
      await screen.findByText(
        /Repository links with query parameters are unsupported/,
      ),
    ).toBeInTheDocument();
  });

  it("leaves other documents' links as authored, without reads", async () => {
    const read = serve();
    const { navigate, open } = click(
      "README.md#L5",
      undefined,
      "docs/ws/plan.md",
    );
    await waitFor(() =>
      expect(navigate).toHaveBeenCalledWith(
        expect.objectContaining({ type: "file", id: "docs/ws/README.md" }),
      ),
    );
    expect(open).not.toHaveBeenCalled();
    expect(read).toEqual([]);
  });

  it.each([
    [{ "data-record-span": "rcv.ceiling" }, "rcv.ceiling"],
    [{ "data-finding-id": "bug-001" }, "bug-001"],
  ])(
    "names the statement or finding holding a line link, for Back",
    async (holder, from) => {
      serve();
      const open = vi.fn();
      renderApp(
        <Links
          href="../../../docs/knowledge/receiving.md#L26"
          holder={holder}
          navigate={vi.fn()}
          open={open}
        />,
      );
      fireEvent.click(screen.getByRole("link", { name: "Evidence" }));
      await waitFor(() =>
        expect(open).toHaveBeenCalledWith(
          "docs/knowledge/receiving.md",
          "repo",
          26,
          from,
        ),
      );
    },
  );

  it("resolves links in another file's content from that file", async () => {
    serve();
    const { open } = click(
      "../../../../src/Create.cs#L10",
      ".g5/TLA_verification/models/numbering/BINDINGS.md",
    );
    await waitFor(() =>
      expect(open).toHaveBeenCalledWith("src/Create.cs", "repo", 10, undefined),
    );
  });
});
