import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it, vi } from "vitest";
import { server } from "../../test/msw/server";
import {
  WorkspaceLiveGateProvider,
  type WorkspaceLiveGate,
} from "../../lib/workspaceLiveGate";
import {
  cardStatement,
  messageOntologyCard,
  newestOntologyCardMessageId,
  type OntologyCard,
  type OntologyCardChange,
} from "../../lib/ontologyCard";
import type { ChatMessageDTO } from "../../lib/types";
import {
  ComposerStoreContext,
  createComposerStore,
} from "../../hooks/useComposerStore";
import { OntologyChangesCard } from "./OntologyChangesCard";

const REPO = "11111111-1111-1111-1111-111111111111";
const part = (id: string, body: string, kind = "I") => ({
  source: `<a id="${id}"></a>\n[${kind} · ${id}]\n\n${body}\n\n`,
  metadata: "\n\n- agent\n\n",
});
const added = (id: string, body: string): OntologyCardChange => ({
  id,
  change: "added",
  after: part(id, body),
});

function card(overrides: Partial<OntologyCard> = {}): OntologyCard {
  return {
    schema: "g5.ontology-card.v1",
    files: [
      {
        repoId: REPO,
        rootPath: "code",
        path: ".g5/ontology/features/chat.g5.md",
        changes: [
          {
            id: "x.modified",
            change: "modified",
            before: part("x.modified", "The old wording."),
            after: part("x.modified", "The new wording."),
          },
          added("x.added", "A new decision."),
          {
            id: "x.deleted",
            change: "deleted",
            before: part("x.deleted", "A removed rule."),
          },
        ],
      },
    ],
    governing: [],
    edits: [],
    ...overrides,
  };
}

const OWNER_LIVE: WorkspaceLiveGate = {
  kind: "live",
  canWrite: true,
  live: true,
  canStart: false,
  reviewCopy: false,
  reviewIdentityKnown: true,
  owner: true,
  workspaceNoun: "the workspace",
};

function renderCard(
  props: Partial<React.ComponentProps<typeof OntologyChangesCard>> = {},
  gate: WorkspaceLiveGate | null = OWNER_LIVE,
) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const element = (
    <OntologyChangesCard
      card={card()}
      sessionId="session-1"
      messageId="message-1"
      newest
      turnRunning={false}
      {...props}
    />
  );
  return render(
    <QueryClientProvider client={queryClient}>
      {gate ? (
        <WorkspaceLiveGateProvider value={gate}>
          {element}
        </WorkspaceLiveGateProvider>
      ) : (
        element
      )}
    </QueryClientProvider>,
  );
}

describe("ontology card model", () => {
  it("reads the card from a reply's artifacts only", () => {
    const base = { role: "assistant", artifacts: null } as Pick<
      ChatMessageDTO,
      "role" | "artifacts"
    >;
    expect(messageOntologyCard(base)).toBeNull();
    const withCard = {
      ...base,
      artifacts: {
        ontology_card: card(),
      } as unknown as ChatMessageDTO["artifacts"],
    };
    expect(messageOntologyCard(withCard)?.files).toHaveLength(1);
    expect(messageOntologyCard({ ...withCard, role: "user" })).toBeNull();
    expect(
      messageOntologyCard({
        ...base,
        artifacts: {
          platform_notice: {},
        } as unknown as ChatMessageDTO["artifacts"],
      }),
    ).toBeNull();
  });

  it("rebuilds a stored statement with the editor's parser", () => {
    const statement = cardStatement("x.a", part("x.a", "Alpha text.", "D"));
    expect(statement?.record.id).toBe("x.a");
    expect(statement?.body.trim()).toBe("Alpha text.");
    expect(statement?.metadata).toContain("- agent");
    expect(
      cardStatement("x.a", { source: "not a statement", metadata: "" }),
    ).toBeNull();
  });

  it("finds the newest reply with a card", () => {
    const message = (id: string, index: number, withCard: boolean) =>
      ({
        id,
        role: "assistant",
        messageIndex: index,
        artifacts: withCard ? { ontology_card: card() } : null,
      }) as unknown as ChatMessageDTO;
    expect(
      newestOntologyCardMessageId([
        message("a", 1, true),
        message("b", 3, true),
        message("c", 5, false),
      ]),
    ).toBe("b");
    expect(newestOntologyCardMessageId([message("c", 5, false)])).toBeNull();
  });
});

describe("OntologyChangesCard", () => {
  it("shows every changed statement with its Before text, grouped by file, without a total", () => {
    renderCard();
    const cardEl = screen.getByTestId("ontology-changes-card");
    expect(cardEl).toHaveAttribute("open");
    expect(
      screen.getByText("Ontology changes in this reply"),
    ).toBeInTheDocument();
    expect(
      screen.getByLabelText("1 added, 1 modified, 1 deleted"),
    ).toBeInTheDocument();
    expect(within(cardEl).getByText("features/chat")).toBeInTheDocument();
    expect(screen.getByText("The new wording.")).toBeInTheDocument();
    expect(
      within(
        screen.getByRole("region", { name: "Before statement x.modified" }),
      ).getByText("The old wording."),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("region", { name: "Deleted statement x.deleted" }),
    ).toHaveTextContent("A removed rule.");
    expect(cardEl).not.toHaveTextContent(/total/i);
    expect(
      screen.getByText(
        "Ontology changes between the start and end of this turn.",
      ),
    ).toBeInTheDocument();
  });

  it("shows 5 statements, then Show more reveals the rest in place", () => {
    const changes = Array.from({ length: 8 }, (_, i) =>
      added(`x.s${i}`, `Statement ${i}.`),
    );
    renderCard({
      card: card({
        files: [
          {
            repoId: REPO,
            rootPath: "code",
            path: ".g5/ontology/b.g5.md",
            changes: changes.slice(0, 5),
          },
          {
            repoId: REPO,
            rootPath: "code",
            path: ".g5/ontology/a.g5.md",
            changes: changes.slice(5),
          },
        ],
      }),
    });
    expect(screen.getAllByTestId("ontology-card-statement")).toHaveLength(5);
    // Files in file order: a before b.
    const files = screen
      .getAllByTestId("ontology-card-file")
      .map((f) => f.dataset.path);
    expect(files).toEqual([".g5/ontology/a.g5.md", ".g5/ontology/b.g5.md"]);
    expect(screen.getByTestId("ontology-card-show-more")).toHaveTextContent(
      "Show more (3)",
    );
    fireEvent.click(screen.getByTestId("ontology-card-show-more"));
    expect(screen.getAllByTestId("ontology-card-statement")).toHaveLength(8);
    expect(screen.queryByTestId("ontology-card-show-more")).toBeNull();
    // Revealed statements of the newest card are editable as the others.
    expect(screen.getAllByTestId("ontology-card-edit")).toHaveLength(8);
  });

  it("shows a file's refusal and the rest of the card, and a too-large card's notice", () => {
    renderCard({
      card: card({
        files: [
          ...card().files!,
          {
            repoId: REPO,
            rootPath: "code",
            path: ".g5/ontology/broken.g5.md",
            refusal: "Statement comparison unavailable: unclosed code fence.",
          },
        ],
      }),
    });
    expect(screen.getByRole("alert")).toHaveTextContent("unclosed code fence");
    expect(screen.getByText("The new wording.")).toBeInTheDocument();
  });

  it("says when a turn's changes are too large to show", () => {
    const onOpenAllChanges = vi.fn();
    renderCard({
      card: { schema: "g5.ontology-card.v1", tooLarge: true },
      onOpenAllChanges,
    });
    expect(
      screen.getByText("This turn changed too many statements to show here."),
    ).toBeInTheDocument();
    // The link's scope is the workspace's whole change, not this reply's.
    expect(screen.getByTestId("ontology-card-view-all")).toHaveTextContent(
      "View all changes in workspace",
    );
    fireEvent.click(screen.getByTestId("ontology-card-view-all"));
    expect(onOpenAllChanges).toHaveBeenCalled();
  });

  it("leaves out governing names among the card's own changes", () => {
    const named = (id: string, path = ".g5/ontology/features/chat.g5.md") => ({
      id,
      repoId: REPO,
      path,
      reason: `Named ${id}.`,
      statement: null,
    });
    const { unmount } = renderCard({
      card: card({
        governing: [
          named("x.added"),
          named("x.modified"),
          named("x.deleted"),
          // The same statement named under another file, as after a move.
          named("x.modified", ".g5/ontology/features/other.g5.md"),
          named("g5.missing"),
        ],
      }),
    });
    const governing = screen.getByTestId("ontology-card-governing");
    expect(
      within(governing).getByText("Unchanged governing statements"),
    ).toBeInTheDocument();
    expect(
      within(governing).getByTestId("ontology-card-governing-count"),
    ).toHaveTextContent("1");
    expect(
      within(governing)
        .getAllByTestId("ontology-card-governing-statement")
        .map((item) => item.dataset.statementId),
    ).toEqual(["g5.missing"]);
    unmount();
    renderCard({
      card: card({ governing: [named("x.added"), named("x.deleted")] }),
    });
    expect(screen.queryByTestId("ontology-card-governing")).toBeNull();
  });

  it("opens a statement's relative links in its repository's file", () => {
    const onOpenFile = vi.fn();
    renderCard({
      onOpenFile,
      card: card({
        files: [
          {
            repoId: REPO,
            rootPath: "code",
            path: ".g5/ontology/features/chat.g5.md",
            changes: [
              added(
                "x.added",
                "Follows [the rule](documents.g5.md#g5.documents.rule).",
              ),
            ],
          },
        ],
        governing: [
          {
            id: "g5.root",
            repoId: REPO,
            path: ".g5/ontology/root.g5.md",
            reason: "Frames it.",
            statement: part(
              "g5.root",
              "See [the notes](../docs/notes.md#L12).",
            ),
          },
        ],
      }),
    });
    // A statement link names the statement, so the file opens at it.
    fireEvent.click(screen.getByRole("link", { name: "the rule" }));
    expect(onOpenFile).toHaveBeenLastCalledWith(
      ".g5/ontology/features/documents.g5.md",
      REPO,
      undefined,
      "g5.documents.rule",
    );
    fireEvent.click(screen.getByText("Unchanged governing statements"));
    fireEvent.click(screen.getByRole("link", { name: "the notes" }));
    expect(onOpenFile).toHaveBeenLastCalledWith(".g5/docs/notes.md", REPO, 12);
  });

  it("opens a statement's in-file and details-footer links in its file", () => {
    const onOpenFile = vi.fn();
    const withSource = (id: string, body: string) => ({
      source: `<a id="${id}"></a>\n[D · ${id}]\n\n${body}\n\n`,
      metadata:
        "\n\n- agent\n- Source: [the intent](../../chat_intent/x/request.md#why)\n\n",
    });
    renderCard({
      onOpenFile,
      card: card({
        files: [
          {
            repoId: REPO,
            rootPath: "code",
            path: ".g5/ontology/features/chat.g5.md",
            changes: [
              {
                id: "x.added",
                change: "added",
                after: withSource(
                  "x.added",
                  "Runs per [turn](#g5.chat.turns).",
                ),
              },
            ],
          },
        ],
        governing: [
          {
            id: "g5.root",
            repoId: REPO,
            path: ".g5/ontology/root.g5.md",
            reason: "Frames it.",
            statement: withSource("g5.root", "See [the edit rule](#g5.edit)."),
          },
        ],
      }),
    });
    // A link to another statement of the same file opens it there.
    fireEvent.click(screen.getByRole("link", { name: "turn" }));
    expect(onOpenFile).toHaveBeenLastCalledWith(
      ".g5/ontology/features/chat.g5.md",
      REPO,
      undefined,
      "g5.chat.turns",
    );
    // The details footer resolves its links against the statement's file.
    const changed = screen.getByTestId("ontology-card-statement");
    fireEvent.click(
      within(changed).getByTestId("semantic-record-details-toggle"),
    );
    fireEvent.click(within(changed).getByRole("link", { name: "the intent" }));
    expect(onOpenFile).toHaveBeenLastCalledWith(
      ".g5/chat_intent/x/request.md",
      REPO,
      undefined,
    );
    fireEvent.click(screen.getByText("Unchanged governing statements"));
    fireEvent.click(screen.getByRole("link", { name: "the edit rule" }));
    expect(onOpenFile).toHaveBeenLastCalledWith(
      ".g5/ontology/root.g5.md",
      REPO,
      undefined,
      "g5.edit",
    );
    const governing = screen.getByTestId("ontology-card-governing-statement");
    fireEvent.click(
      within(governing).getByTestId("semantic-record-details-toggle"),
    );
    fireEvent.click(
      within(governing).getByRole("link", { name: "the intent" }),
    );
    expect(onOpenFile).toHaveBeenLastCalledWith(
      "chat_intent/x/request.md",
      REPO,
      undefined,
    );
  });

  it("lists the governing statements collapsed, with the agent's reasons and unknown names flagged", () => {
    renderCard({
      card: card({
        governing: [
          {
            id: "g5.chat.replay",
            repoId: REPO,
            path: ".g5/ontology/features/chat.g5.md",
            reason: "Message identities stay stable.",
            statement: part(
              "g5.chat.replay",
              "Live streaming and saved history agree.",
            ),
          },
          {
            id: "g5.root",
            repoId: REPO,
            path: ".g5/ontology/root.g5.md",
            reason: "The root frames the change.",
            statement: part("g5.root", "The root decision."),
          },
          {
            id: "g5.missing",
            repoId: REPO,
            path: ".g5/ontology/features/chat.g5.md",
            reason: "Named by the agent.",
            statement: null,
          },
        ],
      }),
    });
    const governing = screen.getByTestId("ontology-card-governing");
    expect(governing).not.toHaveAttribute("open");
    expect(
      within(governing).getByText("Unchanged governing statements"),
    ).toBeInTheDocument();
    expect(
      within(governing).getByTestId("ontology-card-governing-count"),
    ).toHaveTextContent("3");
    // Grouped by file in the changes' file order, the agent's order within one.
    const groups = within(governing).getAllByTestId(
      "ontology-card-governing-file",
    );
    expect(groups.map((group) => group.dataset.path)).toEqual([
      ".g5/ontology/root.g5.md",
      ".g5/ontology/features/chat.g5.md",
    ]);
    expect(
      within(groups[1])
        .getAllByTestId("ontology-card-governing-statement")
        .map((item) => item.dataset.statementId),
    ).toEqual(["g5.chat.replay", "g5.missing"]);
    // Bulleted rows like the changes, under a muted label; the ID is in a
    // found one's details.
    const [found, missing] = within(groups[1]).getAllByTestId(
      "ontology-card-governing-statement",
    );
    expect(found).toHaveClass("ontology-statement-bullet");
    expect(
      within(found).getByRole("region", {
        name: "Unchanged statement g5.chat.replay",
      }),
    ).toHaveTextContent(/^Unchanged/);
    expect(
      within(missing).getByRole("region", {
        name: "Not found statement g5.missing",
      }),
    ).toHaveTextContent(/^Not found/);
    expect(found.closest(".ontology-statement-list")).not.toBeNull();
    expect(found).toHaveTextContent("Live streaming and saved history agree.");
    expect(found).not.toHaveTextContent(".g5/ontology/features/chat.g5.md");
    expect(within(found).getByTestId("semantic-record-id")).toHaveTextContent(
      "g5.chat.replay",
    );
    expect(found).toHaveTextContent(
      "Why it applies: Message identities stay stable.",
    );
    expect(within(missing).getByRole("alert")).toHaveTextContent(
      "No statement with the ID g5.missing was in the ontology",
    );
    expect(missing).toHaveTextContent("↳Why it applies: Named by the agent.");
    expect(within(missing).getByText("Why it applies:")).toHaveAttribute(
      "title",
      "The agent's reason for naming this statement",
    );
    // Read-only and uncounted.
    expect(within(governing).queryByTestId("ontology-card-edit")).toBeNull();
    expect(
      screen.getByLabelText("1 added, 1 modified, 1 deleted"),
    ).toBeInTheDocument();
  });

  it("collapses an earlier card and offers no Edit on it", () => {
    renderCard({ newest: false });
    expect(screen.getByTestId("ontology-changes-card")).not.toHaveAttribute(
      "open",
    );
    expect(screen.queryByTestId("ontology-card-edit")).toBeNull();
  });

  it("disables Edit for readers, while a turn runs and while the VM is stopped, with the reason", () => {
    const { unmount } = renderCard(
      {},
      {
        ...OWNER_LIVE,
        kind: "reader",
        owner: false,
        live: false,
        canWrite: false,
      },
    );
    const edit = screen.getAllByTestId("ontology-card-edit")[0];
    expect(edit).toBeDisabled();
    expect(edit).toHaveAttribute(
      "title",
      "Only the owner of a running workspace can edit the card.",
    );
    unmount();
    const running = renderCard({ turnRunning: true });
    expect(screen.getAllByTestId("ontology-card-edit")[0]).toHaveAttribute(
      "title",
      "Wait for the reply in progress to finish before editing the card.",
    );
    running.unmount();
    renderCard(
      {},
      { ...OWNER_LIVE, kind: "stopped", live: false, canStart: true },
    );
    expect(screen.getAllByTestId("ontology-card-edit")[0]).toHaveAttribute(
      "title",
      "Start the workspace to edit the card.",
    );
  });

  it("keeps an open edit's draft but refuses Save once a turn starts", () => {
    const queryClient = new QueryClient();
    const tree = (turnRunning: boolean) => (
      <QueryClientProvider client={queryClient}>
        <WorkspaceLiveGateProvider value={OWNER_LIVE}>
          <OntologyChangesCard
            card={card()}
            sessionId="session-1"
            messageId="message-1"
            newest
            turnRunning={turnRunning}
          />
        </WorkspaceLiveGateProvider>
      </QueryClientProvider>
    );
    const { rerender } = render(tree(false));
    fireEvent.click(screen.getAllByTestId("ontology-card-edit")[0]);
    fireEvent.change(screen.getByTestId("ontology-card-edit-text"), {
      target: { value: "Reworded." },
    });
    expect(screen.getByTestId("ontology-card-edit-save")).toBeEnabled();
    rerender(tree(true));
    expect(screen.getByTestId("ontology-card-edit-text")).toHaveValue(
      "Reworded.",
    );
    expect(screen.getByTestId("ontology-card-edit-save")).toBeDisabled();
    expect(screen.getByRole("alert")).toHaveTextContent(
      "Wait for the reply in progress to finish before editing the card.",
    );
  });

  it("shows what it can of a malformed card instead of failing", () => {
    renderCard({
      card: {
        schema: "g5.ontology-card.v1",
        files: [
          { repoId: REPO, rootPath: "code" } as never,
          {
            repoId: REPO,
            rootPath: "code",
            path: ".g5/ontology/features/chat.g5.md",
            changes: [
              { id: "x.empty", change: "modified" },
              added("x.added", "A new decision."),
            ],
          },
          {
            repoId: REPO,
            rootPath: "code",
            path: ".g5/ontology/other.g5.md",
            changes: "not a list" as never,
          },
        ],
        governing: [
          { id: "g.odd", repoId: REPO, path: "p", reason: {} as never },
          { id: 7 } as never,
        ],
        edits: null as never,
      },
    });
    // A change the parser cannot rebuild is still listed and counted.
    const statements = screen.getAllByTestId("ontology-card-statement");
    expect(statements.map((el) => el.dataset.statementId)).toEqual([
      "x.empty",
      "x.added",
    ]);
    expect(
      within(statements[0]).getByRole("region", {
        name: "Modified statement x.empty",
      }),
    ).toHaveTextContent("Modifiedx.empty can't be shown here.");
    expect(
      within(statements[0]).queryByTestId("ontology-card-edit"),
    ).toBeNull();
    expect(
      screen.getByLabelText("1 added, 1 modified, 0 deleted"),
    ).toBeInTheDocument();
    expect(
      screen.getAllByTestId("ontology-card-governing-statement"),
    ).toHaveLength(1);
  });

  it("saves an owner's edit with the card's text and one retry key per text", async () => {
    const requests: Record<string, unknown>[] = [];
    let fail = 2;
    server.use(
      http.post(
        "*/api/v1/sessions/session-1/messages/message-1/ontology-card/edits",
        async ({ request }) => {
          const body = (await request.json()) as Record<string, unknown>;
          requests.push(body);
          if (fail) {
            fail -= 1;
            return HttpResponse.json(
              {
                error: {
                  code: "CONFLICT",
                  message:
                    "the statement changed since the card was made; reload before editing",
                },
              },
              { status: 409 },
            );
          }
          return HttpResponse.json({
            data: { ...body, editedBy: "u", editedAt: "now" },
          });
        },
      ),
    );
    renderCard();
    // Deleted statements have no Edit; added and modified ones do.
    expect(screen.getAllByTestId("ontology-card-edit")).toHaveLength(2);
    fireEvent.click(screen.getAllByTestId("ontology-card-edit")[0]);
    const text = screen.getByTestId("ontology-card-edit-text");
    expect(text).toHaveValue("The new wording.");
    fireEvent.change(text, { target: { value: "The owner's wording." } });
    fireEvent.click(screen.getByTestId("ontology-card-edit-save"));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "the statement changed",
    );
    fireEvent.click(screen.getByTestId("ontology-card-edit-save"));
    await waitFor(() => expect(requests).toHaveLength(2));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "the statement changed",
    );
    // Changed text is a new edit, with a new key.
    fireEvent.change(text, { target: { value: "The owner's second try." } });
    fireEvent.click(screen.getByTestId("ontology-card-edit-save"));
    await waitFor(() =>
      expect(screen.queryByTestId("ontology-card-edit-text")).toBeNull(),
    );
    expect(requests).toHaveLength(3);
    expect(requests[0]).toMatchObject({
      repoId: REPO,
      path: ".g5/ontology/features/chat.g5.md",
      statementId: "x.modified",
      expectedText: "The new wording.",
      text: "The owner's wording.",
    });
    expect(requests[1].idempotencyKey).toBe(requests[0].idempotencyKey);
    expect(requests[2]).toMatchObject({ text: "The owner's second try." });
    expect(requests[2].idempotencyKey).not.toBe(requests[0].idempotencyKey);
  });

  it("fits the edit box to its text up to six lines, then scrolls", () => {
    // jsdom has no layout: a line is 16px (the fallback), each "\n" starts
    // one, and the stub's scrollHeight leaves out padding.
    const scrollHeight = vi
      .spyOn(HTMLTextAreaElement.prototype, "scrollHeight", "get")
      .mockImplementation(function (this: HTMLTextAreaElement) {
        return Math.max(2, this.value.split("\n").length) * 16;
      });
    try {
      renderCard();
      fireEvent.click(screen.getAllByTestId("ontology-card-edit")[0]);
      const text = screen.getByTestId("ontology-card-edit-text");
      const style = getComputedStyle(text);
      const px = (value: string) => parseFloat(value) || 0;
      const border = px(style.borderTopWidth) + px(style.borderBottomWidth);
      const padding = px(style.paddingTop) + px(style.paddingBottom);
      const box = (lines: number) => `${lines * 16 + border}px`;
      expect(text).toHaveStyle({ height: box(2), overflowY: "hidden" });
      fireEvent.change(text, { target: { value: "1\n2\n3\n4" } });
      expect(text).toHaveStyle({ height: box(4), overflowY: "hidden" });
      fireEvent.change(text, { target: { value: "1\n2\n3\n4\n5\n6\n7\n8" } });
      expect(text).toHaveStyle({
        height: `${6 * 16 + padding + border}px`,
        overflowY: "auto",
      });
      // It shrinks back as text is removed.
      fireEvent.change(text, { target: { value: "1\n2\n3" } });
      expect(text).toHaveStyle({ height: box(3), overflowY: "hidden" });
    } finally {
      scrollHeight.mockRestore();
    }
  });

  it("edits in place: the text becomes the editor while the label, bullet and Before stay", async () => {
    const requests: Record<string, unknown>[] = [];
    server.use(
      http.post(
        "*/api/v1/sessions/session-1/messages/message-1/ontology-card/edits",
        async ({ request }) => {
          const body = (await request.json()) as Record<string, unknown>;
          requests.push(body);
          return HttpResponse.json({
            data: { ...body, editedBy: "u", editedAt: "now" },
          });
        },
      ),
    );
    renderCard();
    const statement = screen
      .getAllByTestId("ontology-card-statement")
      .find((el) => el.dataset.statementId === "x.modified")!;
    const region = within(statement).getByRole("region", {
      name: "Modified statement x.modified",
    });
    const label = region.querySelector(".ontology-comparison-label")!;
    const edit = within(statement).getByTestId("ontology-card-edit");
    // Edit is a hover action beside the statement's details toggle.
    expect(edit).toHaveClass("semantic-record-hover-action");
    expect(
      within(region).getByTestId("semantic-record-identity"),
    ).toContainElement(edit);
    expect(edit).toHaveAccessibleName("Edit x.modified");

    fireEvent.click(edit);
    const text = within(region).getByTestId("ontology-card-edit-text");
    expect(text).toHaveFocus();
    expect(label).toHaveTextContent("Modified");
    expect(region.querySelector(".semantic-record-frame")).toBeNull();
    expect(
      within(statement).getByRole("region", {
        name: "Before statement x.modified",
      }),
    ).toHaveTextContent("The old wording.");
    // The editor stays in the pair, the list item that carries the bullet.
    expect(text.closest(".ontology-comparison-pair")).not.toBeNull();

    // Escape cancels without saving and returns focus to Edit.
    fireEvent.keyDown(text, { key: "Escape" });
    expect(
      within(statement).queryByTestId("ontology-card-edit-text"),
    ).toBeNull();
    expect(within(statement).getByTestId("ontology-card-edit")).toHaveFocus();
    expect(region).toHaveTextContent("The new wording.");

    // Ctrl+Enter saves only a changed draft.
    fireEvent.click(within(statement).getByTestId("ontology-card-edit"));
    const again = within(statement).getByTestId("ontology-card-edit-text");
    fireEvent.keyDown(again, { key: "Enter", ctrlKey: true });
    expect(requests).toHaveLength(0);
    fireEvent.change(again, { target: { value: "The owner's wording." } });
    fireEvent.keyDown(again, { key: "Enter", ctrlKey: true });
    await waitFor(() => expect(requests).toHaveLength(1));
  });

  describe("keeps an open edit when the chat is rebuilt", () => {
    const EDITS =
      "*/api/v1/sessions/session-1/messages/message-1/ontology-card/edits";
    // The page's owner outlives the card, as across WorkspacePage's desktop
    // and mobile layouts; `shown` removes and rebuilds the card under it.
    function renderInPage(props: { newest?: boolean } = {}) {
      const store = createComposerStore();
      const queryClient = new QueryClient({
        defaultOptions: { mutations: { retry: false } },
      });
      const page = (shown: boolean, newest = props.newest ?? true) => (
        <QueryClientProvider client={queryClient}>
          <ComposerStoreContext.Provider value={store}>
            <WorkspaceLiveGateProvider value={OWNER_LIVE}>
              {shown && (
                <OntologyChangesCard
                  card={card()}
                  sessionId="session-1"
                  messageId="message-1"
                  newest={newest}
                  turnRunning={false}
                />
              )}
            </WorkspaceLiveGateProvider>
          </ComposerStoreContext.Provider>
        </QueryClientProvider>
      );
      const view = render(page(true));
      return {
        hide: () => view.rerender(page(false)),
        show: (newest?: boolean) => view.rerender(page(true, newest)),
      };
    }
    const open = (text: string) => {
      fireEvent.click(screen.getAllByTestId("ontology-card-edit")[0]);
      fireEvent.change(screen.getByTestId("ontology-card-edit-text"), {
        target: { value: text },
      });
    };

    it("keeps the open editor and its draft", () => {
      const page = renderInPage();
      open("The owner's draft.");
      page.hide();
      expect(screen.queryByTestId("ontology-card-edit-text")).toBeNull();
      page.show();
      const text = screen.getByTestId("ontology-card-edit-text");
      expect(text).toHaveValue("The owner's draft.");
      expect(text).toHaveFocus();
      // Cancel drops it: the next rebuild shows the statement again.
      fireEvent.click(screen.getByTestId("ontology-card-edit-cancel"));
      page.hide();
      page.show();
      expect(screen.queryByTestId("ontology-card-edit-text")).toBeNull();
      fireEvent.click(screen.getAllByTestId("ontology-card-edit")[0]);
      expect(screen.getByTestId("ontology-card-edit-text")).toHaveValue(
        "The new wording.",
      );
    });

    it("keeps a failed Save's reason and retries it with the same key", async () => {
      const keys: unknown[] = [];
      let fail = true;
      server.use(
        http.post(EDITS, async ({ request }) => {
          const body = (await request.json()) as Record<string, unknown>;
          keys.push(body.idempotencyKey);
          if (fail) {
            fail = false;
            return HttpResponse.json(
              { error: { code: "UNAVAILABLE", message: "try again" } },
              { status: 503 },
            );
          }
          return HttpResponse.json({
            data: { ...body, editedBy: "u", editedAt: "now" },
          });
        }),
      );
      const page = renderInPage();
      open("The owner's draft.");
      fireEvent.click(screen.getByTestId("ontology-card-edit-save"));
      expect(await screen.findByRole("alert")).toHaveTextContent("try again");
      page.hide();
      page.show();
      expect(screen.getByRole("alert")).toHaveTextContent("try again");
      fireEvent.click(screen.getByTestId("ontology-card-edit-save"));
      await waitFor(() =>
        expect(screen.queryByTestId("ontology-card-edit-text")).toBeNull(),
      );
      expect(keys).toHaveLength(2);
      expect(keys[1]).toBe(keys[0]);
    });

    it("completes a Save sent before the rebuild", async () => {
      let reply!: () => void;
      const replied = new Promise<void>((resolve) => (reply = resolve));
      server.use(
        http.post(EDITS, async ({ request }) => {
          const body = (await request.json()) as Record<string, unknown>;
          await replied;
          return HttpResponse.json({
            data: { ...body, editedBy: "u", editedAt: "now" },
          });
        }),
      );
      const page = renderInPage();
      open("The owner's draft.");
      fireEvent.click(screen.getByTestId("ontology-card-edit-save"));
      page.hide();
      page.show();
      // Still sending after the rebuild; it closes once the write lands.
      expect(screen.getByTestId("ontology-card-edit-save")).toHaveTextContent(
        "Saving…",
      );
      reply();
      await waitFor(() =>
        expect(screen.queryByTestId("ontology-card-edit-text")).toBeNull(),
      );
    });

    it("hides a kept edit once the card is no longer the newest", () => {
      const page = renderInPage();
      open("The owner's draft.");
      page.hide();
      page.show(false);
      expect(screen.queryByTestId("ontology-card-edit-text")).toBeNull();
    });
  });

  it("marks an edited statement and keeps the agent's text viewable", () => {
    renderCard({
      card: card({
        edits: [
          {
            idempotencyKey: "k",
            repoId: REPO,
            path: ".g5/ontology/features/chat.g5.md",
            statementId: "x.added",
            text: "The owner's decision.",
            editedBy: "owner",
            editedAt: "2026-10-29T00:00:00Z",
          },
        ],
      }),
    });
    const statement = screen
      .getAllByTestId("ontology-card-statement")
      .find((el) => el.dataset.statementId === "x.added")!;
    expect(
      within(statement).getByText("The owner's decision."),
    ).toBeInTheDocument();
    const edited = within(statement).getByTestId("ontology-card-edited");
    expect(edited).toHaveTextContent("(edited)");
    // The mark follows the statement's text, inside its Markdown.
    expect(
      within(statement).getByTestId("ontology-statement-markdown"),
    ).toContainElement(edited);
    expect(edited).toHaveAttribute(
      "title",
      expect.stringMatching(/^You edited this after the agent's turn/),
    );
    expect(
      within(statement).queryByText("A new decision."),
    ).not.toBeInTheDocument();
  });

  it("shows readers who edited", () => {
    renderCard(
      {
        newest: false,
        card: card({
          edits: [
            {
              idempotencyKey: "k",
              repoId: REPO,
              path: ".g5/ontology/features/chat.g5.md",
              statementId: "x.added",
              text: "The owner's decision.",
              editedBy: "owner",
              editedAt: "2026-10-29T00:00:00Z",
            },
          ],
        }),
      },
      { ...OWNER_LIVE, kind: "reader", owner: false },
    );
    expect(screen.getByTestId("ontology-card-edited")).toHaveAttribute(
      "title",
      expect.stringMatching(/^The workspace owner edited this/),
    );
  });
});

describe("ontology card change highlights", () => {
  // g5.documents.ontology.change-highlights, through the shared pair. jsdom
  // has no CSS Custom Highlight API, so a minimal registry stands in.
  it("marks what changed inside a modified statement, after an owner's edit too", () => {
    const registry = new Map<string, Set<Range>>();
    vi.stubGlobal("Highlight", class extends Set<Range> {});
    vi.stubGlobal("CSS", { highlights: registry });
    const marks = (name: string) =>
      [...(registry.get(name) ?? [])].map((r) => r.toString()).sort();
    try {
      renderCard({
        card: card({
          files: [
            {
              repoId: REPO,
              rootPath: "code",
              path: ".g5/ontology/features/chat.g5.md",
              changes: [
                {
                  id: "x.modified",
                  change: "modified",
                  before: part(
                    "x.modified",
                    "Keep this rule here. Retry three times.",
                  ),
                  after: part(
                    "x.modified",
                    "Keep this rule here. Retry five times.",
                  ),
                },
              ],
            },
          ],
          edits: [
            {
              idempotencyKey: "k",
              repoId: REPO,
              path: ".g5/ontology/features/chat.g5.md",
              statementId: "x.modified",
              text: "Keep this rule here. Retry five times now.",
              editedBy: "owner",
              editedAt: "2026-10-29T00:00:00Z",
            },
          ],
        }),
      });
      expect(screen.getByTestId("ontology-card-edited")).toBeInTheDocument();
      // The owner's text is compared; the "(edited)" mark is not.
      expect(marks("ontology-inserted")).toEqual(["five", "now"]);
      expect(marks("ontology-removed")).toEqual(["three"]);
    } finally {
      vi.unstubAllGlobals();
    }
  });
});
