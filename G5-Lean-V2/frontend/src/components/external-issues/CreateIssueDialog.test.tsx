/**
 * CreateIssueDialog — a GitHub issue in the organization's creation target.
 *
 * Covered: the destination and author line; a trusted domain creating from
 * the form (Create and Create & Fork) with the exact request; an untrusted
 * domain's review step showing the payload with G5's hidden reference line;
 * the assignee picker with "Assign to me"; every blocked target state with
 * submit disabled and Connect GitHub; a detected secret explained without
 * echoing it and the draft kept; a changed destination asking for a new
 * review; an unknown outcome resolved by Check again (look only, same key)
 * and Create anyway (same key), and a new key for an edited draft. A parent
 * issue: sent as entered, previewed with a bare `#N` looked up in the
 * destination, refused before sending when outside the destination or not
 * found, shown in the review step, and a 422 refusal shown at the field; an
 * issue left outside its parent explained with Retry adding parent (same
 * key) and Continue without parent, only Continue for another parent, the
 * same notice after Check again, and a new key after editing the parent.
 * A native G5 destination: no GitHub account or review step even when not
 * trusted, members assignable by name ("Assign to me" sent as `@me`), and a
 * bare `#N` parent looked up as `ACME-N`. Labels: picked from the
 * destination domain's catalog and sent by name (a changed label set is a
 * new submission), shown in the review step, dropped when the destination
 * changes, and GitHub's dropped labels listed before continuing.
 */

import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { HttpResponse, http } from "msw";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type {
  CreateIssueInput,
  CreationParentDTO,
  ExternalIssueSummaryDTO,
  IssueCreateTargetDTO,
  IssueCreationDTO,
} from "../../lib/types";
import {
  makeIssueCreateTarget,
  makeIssueCreation,
  issueErrorResponse,
  makeIssueSummary,
  makeNativeCreateTarget,
  makeNativeIssueSummary,
  makeLabelPage,
  serveExternalIssuesFeature,
  serveLabelCatalog,
  serveNativeIssuesFeature,
} from "../../test/externalIssueFixtures";
import { createMockUser } from "../../test/fixtures";
import { server } from "../../test/msw/server";
import { renderApp } from "../../test/render";
import { CreateIssueDialog } from "./CreateIssueDialog";

const ORG_ID = "org-g5labs";
const TARGET_URL = `/api/v1/orgs/${ORG_ID}/issues/create-target`;
const CREATE_URL = `/api/v1/orgs/${ORG_ID}/issues`;
const RESOLVE_URL = `/api/v1/orgs/${ORG_ID}/issues/resolve`;

const EPIC = makeIssueSummary({
  displayKey: "acme/web#7",
  title: "Login epic",
  locator: { repositoryId: "11", number: 7 },
});

function target(
  overrides: Partial<IssueCreateTargetDTO> = {},
  trusted = true,
): IssueCreateTargetDTO {
  return makeIssueCreateTarget({
    destination: {
      domainId: "domain-1",
      trackerId: "tracker-1",
      repositoryId: "11",
      displayKey: "acme/web",
      contentTrusted: trusted,
    },
    githubLogin: "ada-gh",
    assignableUsers: [
      { login: "ada-gh", avatarUrl: null },
      { login: "hubot", avatarUrl: null },
      { login: "octocat", avatarUrl: null },
    ],
    ...overrides,
  });
}

function creation(
  input: CreateIssueInput,
  overrides: Partial<IssueCreationDTO> = {},
): IssueCreationDTO {
  return makeIssueCreation({
    issue: {
      externalIssueId: "900012",
      repositoryId: "11",
      number: 12,
      displayKey: "acme/web#12",
      title: input.title,
      url: "https://github.com/acme/web/issues/12",
    },
    idempotencyKey: input.idempotencyKey,
    destination: target().destination!,
    githubLogin: "ada-gh",
    title: input.title,
    payload: `${input.body}\n\n<!-- g5-issue 0123 -->`,
    ...overrides,
  });
}

function parentResult(
  state: CreationParentDTO["state"],
  message: string | null = null,
): CreationParentDTO {
  return {
    externalIssueId: EPIC.externalIssueId,
    number: 7,
    displayKey: "acme/web#7",
    title: "Login epic",
    url: "https://github.com/acme/web/issues/7",
    state,
    message,
  };
}

/** Resolve the listed references; anything else is not found. */
function serveResolve(issues: Record<string, ExternalIssueSummaryDTO>) {
  const requested: string[] = [];
  server.use(
    http.get(RESOLVE_URL, ({ request }) => {
      const ref = new URL(request.url).searchParams.get("ref") ?? "";
      requested.push(ref);
      const issue = issues[ref];
      return issue
        ? HttpResponse.json({ data: issue })
        : issueErrorResponse(404, "not_found");
    }),
  );
  return requested;
}

function serveTarget(value: IssueCreateTargetDTO) {
  const reads = vi.fn();
  server.use(
    http.get(TARGET_URL, () => {
      reads();
      return HttpResponse.json({ data: value });
    }),
  );
  return reads;
}

/** Answer each POST with the next responder; record the requests. */
function serveCreate(
  ...responders: Array<(input: CreateIssueInput) => Response>
) {
  const requests: CreateIssueInput[] = [];
  server.use(
    http.post(CREATE_URL, async ({ request }) => {
      const input = (await request.json()) as CreateIssueInput;
      requests.push(input);
      const responder = responders.shift();
      if (!responder) throw new Error("unexpected create request");
      return responder(input);
    }),
  );
  return requests;
}

const created =
  (overrides: Partial<IssueCreationDTO> = {}) =>
  (input: CreateIssueInput) =>
    HttpResponse.json(
      { data: creation(input, overrides) },
      { status: overrides.sent === false ? 200 : 201 },
    );

function renderDialog(
  props: Partial<React.ComponentProps<typeof CreateIssueDialog>> = {},
) {
  const onCreated = vi.fn();
  const onClose = vi.fn();
  const view = renderApp(
    <CreateIssueDialog
      orgId={ORG_ID}
      open
      onClose={onClose}
      onCreated={onCreated}
      {...props}
    />,
    { route: "/g5labs/home", user: createMockUser() },
  );
  return { ...view, onCreated, onClose };
}

beforeEach(() => serveExternalIssuesFeature());

describe("CreateIssueDialog", () => {
  it("creates straight from the form for a trusted destination", async () => {
    serveTarget(target());
    const requests = serveCreate(created(), created());
    const user = userEvent.setup();
    const { onCreated } = renderDialog({ allowFork: true });

    expect(
      await screen.findByText(/as @ada-gh/, {
        selector: '[data-testid="create-issue-destination"]',
      }),
    ).toHaveTextContent("Creating in acme/web as @ada-gh");
    expect(screen.getByTestId("create-issue-title")).toHaveValue("");
    expect(screen.getByTestId("create-issue-body")).toHaveValue("");
    expect(
      screen.queryByTestId("create-issue-review-button"),
    ).not.toBeInTheDocument();
    // A title is required.
    expect(screen.getByTestId("create-issue-submit")).toBeDisabled();

    await user.type(screen.getByTestId("create-issue-title"), "Fix login");
    await user.type(screen.getByTestId("create-issue-body"), "Steps");
    // The author is "Assign to me", not a second row.
    expect(
      screen.queryByTestId("create-issue-assignee-ada-gh"),
    ).not.toBeInTheDocument();
    await user.click(screen.getByTestId("create-issue-assign-me"));
    await user.click(screen.getByTestId("create-issue-assignee-hubot"));
    await user.click(screen.getByTestId("create-issue-submit"));

    await waitFor(() => expect(onCreated).toHaveBeenCalledTimes(1));
    expect(onCreated).toHaveBeenCalledWith(
      expect.objectContaining({ displayKey: "acme/web#12", number: 12 }),
      { fork: false },
    );
    expect(requests[0]).toEqual({
      title: "Fix login",
      body: "Steps",
      assignees: ["@me", "hubot"],
      idempotencyKey: expect.any(String),
      domainId: "domain-1",
      checkOnly: false,
    });

    await user.click(screen.getByTestId("create-issue-submit-fork"));
    await waitFor(() => expect(onCreated).toHaveBeenCalledTimes(2));
    expect(onCreated).toHaveBeenLastCalledWith(
      expect.objectContaining({ displayKey: "acme/web#12" }),
      { fork: true },
    );
    // The same submission keeps its key, so a retry finds the first issue.
    expect(requests[1].idempotencyKey).toBe(requests[0].idempotencyKey);
  });

  it("offers Create & Fork only where allowed", async () => {
    serveTarget(target());
    renderDialog();
    await screen.findByText("Creating in", { exact: false });
    expect(screen.getByTestId("create-issue-submit")).toBeInTheDocument();
    expect(
      screen.queryByTestId("create-issue-submit-fork"),
    ).not.toBeInTheDocument();
  });

  it("reviews the exact issue first for an untrusted destination", async () => {
    serveTarget(target({}, false));
    const requests = serveCreate(created());
    const user = userEvent.setup();
    const { onCreated } = renderDialog({
      allowFork: true,
      initialTitle: "Decide the rollout",
      initialBody: "From the meeting",
    });

    const review = await screen.findByTestId("create-issue-review-button");
    expect(screen.queryByTestId("create-issue-submit")).not.toBeInTheDocument();
    await user.click(screen.getByTestId("create-issue-assignee-octocat"));
    await user.click(review);

    const panel = screen.getByTestId("create-issue-review");
    expect(panel).toHaveTextContent("created in acme/web as @ada-gh");
    expect(panel).toHaveTextContent("hidden G5 reference line");
    expect(panel).toHaveTextContent("Decide the rollout");
    expect(panel).toHaveTextContent(
      "From the meeting <!-- g5-issue (reference id) -->",
    );
    expect(panel).toHaveTextContent("@octocat");
    expect(screen.queryByTestId("create-issue-title")).not.toBeInTheDocument();

    // Edit returns to the form with the draft intact.
    await user.click(screen.getByTestId("create-issue-edit"));
    expect(screen.getByTestId("create-issue-title")).toHaveValue(
      "Decide the rollout",
    );
    await user.click(screen.getByTestId("create-issue-review-button"));
    await user.click(screen.getByTestId("create-issue-submit-fork"));

    await waitFor(() =>
      expect(onCreated).toHaveBeenCalledWith(
        expect.objectContaining({ displayKey: "acme/web#12" }),
        { fork: true },
      ),
    );
    expect(requests).toHaveLength(1);
    expect(requests[0]).toMatchObject({
      title: "Decide the rollout",
      body: "From the meeting",
      assignees: ["octocat"],
    });
  });

  it.each([
    [
      "no_target",
      { destination: null },
      "No repository is set up for new issues",
    ],
    [
      "comments_disabled",
      {},
      "An organization admin has not enabled G5 comments and issue creation for acme/web",
    ],
    [
      "public_destination",
      {},
      "acme/web is public, and G5 does not create issues in public repositories",
    ],
    [
      "visibility_unknown",
      {},
      "GitHub did not report whether acme/web is private",
    ],
    [
      "unavailable",
      { error: { issueError: "unavailable", message: "GitHub timed out" } },
      "GitHub could not be reached to check acme/web: GitHub timed out",
    ],
  ] as const)(
    "explains a %s target and disables creating",
    async (state, overrides, reason) => {
      serveTarget(target({ state, ...overrides }));
      const requests = serveCreate();
      const user = userEvent.setup();
      renderDialog({ allowFork: true });

      expect(
        await screen.findByTestId("create-issue-blocked"),
      ).toHaveTextContent(reason);
      // The draft can still be written.
      await user.type(screen.getByTestId("create-issue-title"), "Keep me");
      expect(screen.getByTestId("create-issue-submit")).toBeDisabled();
      expect(screen.getByTestId("create-issue-submit-fork")).toBeDisabled();
      expect(screen.queryByTestId("create-issue-connect")).toBeNull();
      expect(requests).toHaveLength(0);
    },
  );

  it("asks to connect GitHub when the person has no connected account", async () => {
    serveTarget(target({ state: "connection_required", githubLogin: null }));
    renderDialog();
    expect(await screen.findByTestId("create-issue-blocked")).toHaveTextContent(
      "Connect GitHub to create issues",
    );
    expect(screen.getByTestId("create-issue-connect")).toHaveAttribute(
      "href",
      `/account/github?returnTo=${encodeURIComponent("/g5labs/home")}`,
    );
    expect(screen.getByTestId("create-issue-submit")).toBeDisabled();
    expect(screen.getByTestId("create-issue-destination")).toHaveTextContent(
      "Creating in acme/web",
    );
  });

  it("explains a detected secret without echoing it and keeps the draft", async () => {
    const secret = `ghp_${"a".repeat(36)}`;
    serveTarget(target());
    serveCreate(() =>
      HttpResponse.json(
        {
          error: {
            code: "VALIDATION_ERROR",
            message: "Validation failed",
            details: {
              field: "body",
              code: "secret_detected",
              error:
                "the body appears to contain a secret; remove it before creating the issue",
              kinds: ["github_token"],
            },
          },
        },
        { status: 422 },
      ),
    );
    const user = userEvent.setup();
    const { onCreated } = renderDialog();
    await user.type(
      await screen.findByTestId("create-issue-title"),
      "Rotate keys",
    );
    await user.type(screen.getByTestId("create-issue-body"), `token ${secret}`);
    await user.click(screen.getByTestId("create-issue-submit"));

    const error = await screen.findByTestId("create-issue-error");
    expect(error).toHaveTextContent(
      "The description appears to contain a secret (github_token). Remove it before creating the issue.",
    );
    expect(error.textContent).not.toContain(secret);
    expect(screen.getByTestId("create-issue-title")).toHaveValue("Rotate keys");
    expect(screen.getByTestId("create-issue-body")).toHaveValue(
      `token ${secret}`,
    );
    expect(onCreated).not.toHaveBeenCalled();
  });

  it("asks for a new review when the destination changed", async () => {
    const reads = serveTarget(target({}, false));
    serveCreate(() =>
      HttpResponse.json(
        {
          error: {
            code: "CONFLICT",
            message: "the repository for new issues changed",
            details: { code: "destination_changed" },
          },
        },
        { status: 409 },
      ),
    );
    const user = userEvent.setup();
    renderDialog();
    await user.type(await screen.findByTestId("create-issue-title"), "Fix it");
    await user.click(screen.getByTestId("create-issue-review-button"));
    await user.click(screen.getByTestId("create-issue-submit"));

    expect(await screen.findByTestId("create-issue-error")).toHaveTextContent(
      "The organization's repository for new issues changed. Review the new destination and create again.",
    );
    const readsBefore = reads.mock.calls.length;
    await user.click(screen.getByTestId("create-issue-review-again"));
    await waitFor(() =>
      expect(reads.mock.calls.length).toBeGreaterThan(readsBefore),
    );
    expect(screen.queryByTestId("create-issue-error")).toBeNull();
    // Back on the form, the review step is required again.
    expect(screen.getByTestId("create-issue-title")).toHaveValue("Fix it");
    expect(
      screen.getByTestId("create-issue-review-button"),
    ).toBeInTheDocument();
  });

  it("resolves an unknown outcome by checking with the same key, without sending", async () => {
    serveTarget(target());
    const requests = serveCreate(
      created({
        state: "outcome_unknown",
        issue: null,
        message: "GitHub timed out",
      }),
      created({ state: "created", sent: false }),
    );
    const user = userEvent.setup();
    const { onCreated } = renderDialog({ allowFork: true });
    await user.type(await screen.findByTestId("create-issue-title"), "Fix it");
    await user.click(screen.getByTestId("create-issue-submit-fork"));

    const unknown = await screen.findByTestId("create-issue-unknown");
    expect(unknown).toHaveTextContent(
      "the issue may or may not exist in acme/web. G5 will not create it again on its own.",
    );
    expect(unknown).toHaveTextContent("GitHub timed out");
    // The form stays, and only Check again / Create anyway are offered.
    expect(screen.getByTestId("create-issue-title")).toHaveValue("Fix it");
    expect(screen.queryByTestId("create-issue-submit")).toBeNull();
    expect(onCreated).not.toHaveBeenCalled();

    await user.click(within(unknown).getByTestId("create-issue-check-again"));
    await waitFor(() =>
      expect(onCreated).toHaveBeenCalledWith(
        expect.objectContaining({ displayKey: "acme/web#12" }),
        { fork: true },
      ),
    );
    expect(requests).toHaveLength(2);
    expect(requests[0].checkOnly).toBe(false);
    expect(requests[1].checkOnly).toBe(true);
    expect(requests[1].idempotencyKey).toBe(requests[0].idempotencyKey);
  });

  it("creates anyway with the same key after a check finds nothing", async () => {
    serveTarget(target());
    const requests = serveCreate(
      created({ state: "outcome_unknown", issue: null }),
      created({ state: "not_found", sent: false, issue: null }),
      created(),
    );
    const user = userEvent.setup();
    const { onCreated } = renderDialog();
    await user.type(await screen.findByTestId("create-issue-title"), "Fix it");
    await user.click(screen.getByTestId("create-issue-submit"));
    await user.click(await screen.findByTestId("create-issue-check-again"));
    await waitFor(() =>
      expect(screen.getByTestId("create-issue-unknown")).toHaveTextContent(
        "GitHub has no issue from this submission in acme/web.",
      ),
    );
    expect(onCreated).not.toHaveBeenCalled();

    await user.click(screen.getByTestId("create-issue-create-anyway"));
    await waitFor(() =>
      expect(onCreated).toHaveBeenCalledWith(
        expect.objectContaining({ number: 12 }),
        { fork: false },
      ),
    );
    expect(requests.map((request) => request.checkOnly)).toEqual([
      false,
      true,
      false,
    ]);
    expect(
      new Set(requests.map((request) => request.idempotencyKey)).size,
    ).toBe(1);
  });

  it("keeps a caller's fixed key for every submission", async () => {
    serveTarget(target());
    const requests = serveCreate(
      created({ state: "outcome_unknown", issue: null }),
      created(),
    );
    const user = userEvent.setup();
    renderDialog({
      initialTitle: "[High] Fix it",
      idempotencyKey: "finding:repo:bugs_issues.md#a",
    });
    await screen.findByTestId("create-issue-title");
    await user.click(screen.getByTestId("create-issue-submit"));
    await screen.findByTestId("create-issue-unknown");
    await user.type(screen.getByTestId("create-issue-title"), " now");
    await user.click(screen.getByTestId("create-issue-submit"));
    await waitFor(() => expect(requests).toHaveLength(2));
    expect(requests.map((r) => r.idempotencyKey)).toEqual([
      "finding:repo:bugs_issues.md#a",
      "finding:repo:bugs_issues.md#a",
    ]);
  });

  it("treats an edited draft as a new submission with a new key", async () => {
    serveTarget(target());
    const requests = serveCreate(
      created({ state: "outcome_unknown", issue: null }),
      created(),
    );
    const user = userEvent.setup();
    renderDialog();
    await user.type(await screen.findByTestId("create-issue-title"), "Fix it");
    await user.click(screen.getByTestId("create-issue-submit"));
    await screen.findByTestId("create-issue-unknown");

    await user.type(screen.getByTestId("create-issue-title"), " now");
    expect(screen.queryByTestId("create-issue-unknown")).toBeNull();
    await user.click(screen.getByTestId("create-issue-submit"));
    await waitFor(() => expect(requests).toHaveLength(2));
    expect(requests[1].title).toBe("Fix it now");
    expect(requests[1].idempotencyKey).not.toBe(requests[0].idempotencyKey);
  });

  describe("parent issue", () => {
    it("looks a bare number up in the destination and sends the parent as entered", async () => {
      serveTarget(target());
      const requested = serveResolve({ "acme/web#7": EPIC });
      const requests = serveCreate(
        created({ parent: parentResult("attached") }),
      );
      const user = userEvent.setup();
      const { onCreated } = renderDialog();
      await user.type(
        await screen.findByTestId("create-issue-title"),
        "Fix it",
      );
      expect(screen.getByTestId("create-issue-parent")).toHaveValue("");
      await user.type(screen.getByTestId("create-issue-parent"), "#7");
      // Create waits for the lookup.
      expect(screen.getByTestId("create-issue-submit")).toBeDisabled();

      await waitFor(() =>
        expect(
          screen.getByTestId("create-issue-parent-lookup"),
        ).toHaveTextContent("acme/web#7 · Login epic"),
      );
      expect(requested).toContain("acme/web#7");
      expect(screen.getByTestId("create-issue-submit")).toBeEnabled();
      await user.click(screen.getByTestId("create-issue-submit"));

      // Added under the parent: the normal success path.
      await waitFor(() =>
        expect(onCreated).toHaveBeenCalledWith(
          expect.objectContaining({ displayKey: "acme/web#12" }),
          { fork: false },
        ),
      );
      expect(requests[0]).toMatchObject({ title: "Fix it", parent: "#7" });
      expect(screen.queryByTestId("create-issue-parent-notice")).toBeNull();
    });

    it("refuses a parent outside the destination or not found before sending", async () => {
      serveTarget(target());
      serveResolve({
        "other/repo#3": makeIssueSummary({
          domainId: "domain-2",
          displayKey: "other/repo#3",
          title: "Elsewhere",
          locator: { repositoryId: "22", number: 3 },
        }),
      });
      const requests = serveCreate();
      const user = userEvent.setup();
      renderDialog({ allowFork: true });
      await user.type(
        await screen.findByTestId("create-issue-title"),
        "Fix it",
      );
      await user.type(
        screen.getByTestId("create-issue-parent"),
        "other/repo#3",
      );

      expect(
        await screen.findByTestId("create-issue-parent-error"),
      ).toHaveTextContent(
        "The parent must be an issue in acme/web, where the new issue is created.",
      );
      expect(screen.getByTestId("create-issue-submit")).toBeDisabled();
      expect(screen.getByTestId("create-issue-submit-fork")).toBeDisabled();

      await user.clear(screen.getByTestId("create-issue-parent"));
      await user.type(screen.getByTestId("create-issue-parent"), "99");
      await waitFor(() =>
        expect(
          screen.getByTestId("create-issue-parent-error"),
        ).toHaveTextContent("The issue or repository could not be found."),
      );
      expect(screen.getByTestId("create-issue-submit")).toBeDisabled();

      // Without a parent the issue can be created again.
      await user.clear(screen.getByTestId("create-issue-parent"));
      expect(screen.queryByTestId("create-issue-parent-error")).toBeNull();
      expect(screen.getByTestId("create-issue-submit")).toBeEnabled();
      expect(requests).toHaveLength(0);
    });

    it("shows the parent in the review step", async () => {
      serveTarget(target({}, false));
      serveResolve({ "acme/web#7": EPIC });
      const requests = serveCreate(
        created({ parent: parentResult("attached") }),
      );
      const user = userEvent.setup();
      const { onCreated } = renderDialog({ initialTitle: "Fix it" });
      await user.type(
        await screen.findByTestId("create-issue-parent"),
        "acme/web#7",
      );
      await waitFor(() =>
        expect(screen.getByTestId("create-issue-review-button")).toBeEnabled(),
      );
      await user.click(screen.getByTestId("create-issue-review-button"));

      expect(
        screen.getByTestId("create-issue-review-parent"),
      ).toHaveTextContent("acme/web#7 · Login epic");
      await user.click(screen.getByTestId("create-issue-submit"));
      await waitFor(() => expect(onCreated).toHaveBeenCalledTimes(1));
      expect(requests[0].parent).toBe("acme/web#7");
    });

    it("retries adding the parent with the same key, then continues", async () => {
      serveTarget(target());
      serveResolve({ "acme/web#7": EPIC });
      const requests = serveCreate(
        created({
          parent: parentResult("not_attached", "GitHub refused the sub-issue"),
        }),
        created({ sent: false, parent: parentResult("attached") }),
      );
      const user = userEvent.setup();
      const { onCreated } = renderDialog({ allowFork: true });
      await user.type(
        await screen.findByTestId("create-issue-title"),
        "Fix it",
      );
      await user.type(screen.getByTestId("create-issue-parent"), "7");
      await waitFor(() =>
        expect(screen.getByTestId("create-issue-submit-fork")).toBeEnabled(),
      );
      await user.click(screen.getByTestId("create-issue-submit-fork"));

      const notice = await screen.findByTestId("create-issue-parent-notice");
      expect(notice).toHaveTextContent(
        "Created acme/web#12, but it was not added under acme/web#7.",
      );
      expect(notice).toHaveTextContent("GitHub refused the sub-issue");
      // The issue exists: no form and no Create.
      expect(onCreated).not.toHaveBeenCalled();
      expect(screen.queryByTestId("create-issue-title")).toBeNull();
      expect(screen.queryByTestId("create-issue-submit")).toBeNull();

      await user.click(within(notice).getByTestId("create-issue-retry-parent"));
      await waitFor(() =>
        expect(onCreated).toHaveBeenCalledWith(
          expect.objectContaining({ displayKey: "acme/web#12" }),
          { fork: true },
        ),
      );
      expect(requests).toHaveLength(2);
      expect(requests[1]).toEqual(requests[0]);
      expect(requests[1].checkOnly).toBe(false);
    });

    it("continues without the parent on request", async () => {
      serveTarget(target());
      serveResolve({ "acme/web#7": EPIC });
      const requests = serveCreate(
        created({
          parent: parentResult("outcome_unknown", "GitHub timed out"),
        }),
      );
      const user = userEvent.setup();
      const { onCreated } = renderDialog({ allowFork: true });
      await user.type(
        await screen.findByTestId("create-issue-title"),
        "Fix it",
      );
      await user.type(screen.getByTestId("create-issue-parent"), "#7");
      await waitFor(() =>
        expect(screen.getByTestId("create-issue-submit")).toBeEnabled(),
      );
      await user.click(screen.getByTestId("create-issue-submit"));

      const notice = await screen.findByTestId("create-issue-parent-notice");
      expect(
        within(notice).getByTestId("create-issue-retry-parent"),
      ).toBeInTheDocument();
      await user.click(
        within(notice).getByTestId("create-issue-continue-without-parent"),
      );
      expect(onCreated).toHaveBeenCalledWith(
        expect.objectContaining({ displayKey: "acme/web#12" }),
        { fork: false },
      );
      expect(requests).toHaveLength(1);
    });

    it("offers only Continue when the issue is under another parent", async () => {
      serveTarget(target());
      serveResolve({ "acme/web#7": EPIC });
      serveCreate(
        created({
          sent: false,
          parent: parentResult(
            "other_parent",
            "acme/web#12 is already a sub-issue of acme/web#3",
          ),
        }),
      );
      const user = userEvent.setup();
      const { onCreated } = renderDialog();
      await user.type(
        await screen.findByTestId("create-issue-title"),
        "Fix it",
      );
      await user.type(screen.getByTestId("create-issue-parent"), "#7");
      await waitFor(() =>
        expect(screen.getByTestId("create-issue-submit")).toBeEnabled(),
      );
      await user.click(screen.getByTestId("create-issue-submit"));

      const notice = await screen.findByTestId("create-issue-parent-notice");
      expect(notice).toHaveTextContent("already a sub-issue of acme/web#3");
      expect(
        within(notice).queryByTestId("create-issue-retry-parent"),
      ).toBeNull();
      await user.click(
        within(notice).getByTestId("create-issue-continue-without-parent"),
      );
      expect(onCreated).toHaveBeenCalledTimes(1);
    });

    it("shows the parent notice when Check again finds the issue outside its parent", async () => {
      serveTarget(target());
      serveResolve({ "acme/web#7": EPIC });
      const requests = serveCreate(
        created({
          state: "outcome_unknown",
          issue: null,
          parent: parentResult("pending"),
        }),
        created({ sent: false, parent: parentResult("not_attached") }),
      );
      const user = userEvent.setup();
      const { onCreated } = renderDialog();
      await user.type(
        await screen.findByTestId("create-issue-title"),
        "Fix it",
      );
      await user.type(screen.getByTestId("create-issue-parent"), "#7");
      await waitFor(() =>
        expect(screen.getByTestId("create-issue-submit")).toBeEnabled(),
      );
      await user.click(screen.getByTestId("create-issue-submit"));
      await user.click(await screen.findByTestId("create-issue-check-again"));

      const notice = await screen.findByTestId("create-issue-parent-notice");
      expect(notice).toHaveTextContent(
        "Created acme/web#12, but it was not added under acme/web#7.",
      );
      expect(screen.queryByTestId("create-issue-unknown")).toBeNull();
      expect(
        within(notice).getByTestId("create-issue-retry-parent"),
      ).toBeInTheDocument();
      expect(onCreated).not.toHaveBeenCalled();
      expect(requests[1]).toMatchObject({ checkOnly: true, parent: "#7" });
      expect(requests[1].idempotencyKey).toBe(requests[0].idempotencyKey);
    });

    it("shows a refused parent at its field and keeps the draft", async () => {
      serveTarget(target());
      serveResolve({ "acme/web#7": EPIC });
      serveCreate(() =>
        HttpResponse.json(
          {
            error: {
              code: "VALIDATION_ERROR",
              message: "Validation failed",
              details: {
                field: "parent",
                code: "parent_not_an_issue",
                error: "the parent is a pull request",
              },
            },
          },
          { status: 422 },
        ),
      );
      const user = userEvent.setup();
      const { onCreated } = renderDialog();
      await user.type(
        await screen.findByTestId("create-issue-title"),
        "Fix it",
      );
      await user.type(screen.getByTestId("create-issue-parent"), "#7");
      await waitFor(() =>
        expect(screen.getByTestId("create-issue-submit")).toBeEnabled(),
      );
      await user.click(screen.getByTestId("create-issue-submit"));

      expect(
        await screen.findByTestId("create-issue-parent-error"),
      ).toHaveTextContent("The parent is a pull request, not an issue.");
      expect(screen.queryByTestId("create-issue-error")).toBeNull();
      expect(screen.getByTestId("create-issue-title")).toHaveValue("Fix it");
      expect(screen.getByTestId("create-issue-parent")).toHaveValue("#7");
      expect(onCreated).not.toHaveBeenCalled();

      // Editing the parent clears the refusal.
      await user.clear(screen.getByTestId("create-issue-parent"));
      expect(screen.queryByTestId("create-issue-parent-error")).toBeNull();
    });

    it("treats an edited parent as a new submission with a new key", async () => {
      serveTarget(target());
      serveResolve({
        "acme/web#7": EPIC,
        "acme/web#8": makeIssueSummary({
          displayKey: "acme/web#8",
          title: "Other epic",
          locator: { repositoryId: "11", number: 8 },
        }),
      });
      const requests = serveCreate(
        created({ state: "outcome_unknown", issue: null }),
        created(),
      );
      const user = userEvent.setup();
      renderDialog();
      await user.type(
        await screen.findByTestId("create-issue-title"),
        "Fix it",
      );
      await user.type(screen.getByTestId("create-issue-parent"), "#7");
      await waitFor(() =>
        expect(screen.getByTestId("create-issue-submit")).toBeEnabled(),
      );
      await user.click(screen.getByTestId("create-issue-submit"));
      await screen.findByTestId("create-issue-unknown");

      await user.clear(screen.getByTestId("create-issue-parent"));
      await user.type(screen.getByTestId("create-issue-parent"), "#8");
      expect(screen.queryByTestId("create-issue-unknown")).toBeNull();
      await waitFor(() =>
        expect(screen.getByTestId("create-issue-submit")).toBeEnabled(),
      );
      await user.click(screen.getByTestId("create-issue-submit"));
      await waitFor(() => expect(requests).toHaveLength(2));
      expect(requests[1].parent).toBe("#8");
      expect(requests[1].idempotencyKey).not.toBe(requests[0].idempotencyKey);
    });
  });
});

describe("CreateIssueDialog — native G5 destination", () => {
  it("creates in G5 without a GitHub account or review, with members and a native parent", async () => {
    serveNativeIssuesFeature();
    // Not trusted: a GitHub destination would review the payload first.
    serveTarget(
      makeNativeCreateTarget({
        destination: {
          ...makeNativeCreateTarget().destination!,
          contentTrusted: false,
        },
      }),
    );
    const requested = serveResolve({
      "ACME-3": makeNativeIssueSummary({
        locator: { repositoryId: "g5", number: 3 },
        title: "Search epic",
      }),
    });
    const requests = serveCreate((input) =>
      HttpResponse.json(
        {
          data: makeIssueCreation({
            issue: {
              externalIssueId: "713",
              repositoryId: "g5",
              number: 13,
              displayKey: "ACME-13",
              title: input.title,
              url: "http://localhost:3000/g5labs/issues/g5/13",
            },
            idempotencyKey: input.idempotencyKey,
            destination: makeNativeCreateTarget().destination!,
            githubLogin: null,
            payload: input.body,
            parent: {
              externalIssueId: "703",
              number: 3,
              displayKey: "ACME-3",
              title: "Search epic",
              url: "http://localhost:3000/g5labs/issues/g5/3",
              state: "attached",
              message: null,
            },
          }),
        },
        { status: 201 },
      ),
    );
    const user = userEvent.setup();
    const { onCreated } = renderDialog();

    const destination = await screen.findByTestId("create-issue-destination");
    await waitFor(() =>
      expect(destination).toHaveTextContent("Creating in ACME"),
    );
    expect(destination).not.toHaveTextContent("@");
    const dialog = screen.getByTestId("create-issue-dialog");
    expect(dialog).not.toHaveTextContent("GitHub");
    expect(dialog).not.toHaveTextContent("hidden");

    await user.type(screen.getByTestId("create-issue-title"), "Index rebuild");
    // The caller is "Assign to me"; other members by name and email.
    expect(
      screen.getByTestId("create-issue-assign-me").parentElement,
    ).toHaveTextContent("Assign to me");
    expect(
      screen.getByTestId("create-issue-assignee-maya@g5labs.dev").parentElement,
    ).toHaveTextContent("Maya Patel (maya@g5labs.dev)");
    expect(
      screen.queryByTestId("create-issue-assignee-alex@g5labs.dev"),
    ).not.toBeInTheDocument();
    await user.click(screen.getByTestId("create-issue-assign-me"));
    await user.click(
      screen.getByTestId("create-issue-assignee-maya@g5labs.dev"),
    );
    await user.type(screen.getByTestId("create-issue-parent"), "#3");
    expect(
      await screen.findByText("Search epic", { exact: false }),
    ).toBeInTheDocument();
    expect(requested).toContain("ACME-3");

    expect(
      screen.queryByTestId("create-issue-review-button"),
    ).not.toBeInTheDocument();
    await user.click(screen.getByTestId("create-issue-submit"));
    await waitFor(() => expect(onCreated).toHaveBeenCalled());
    expect(requests[0]).toMatchObject({
      title: "Index rebuild",
      assignees: ["@me", "maya@g5labs.dev"],
      domainId: "domain-g5",
      parent: "#3",
    });
    expect(onCreated.mock.calls[0][0]).toMatchObject({
      repositoryId: "g5",
      displayKey: "ACME-13",
    });
  });
});

describe("CreateIssueDialog — labels", () => {
  it("sends the labels picked from the destination's catalog, reviewed first when untrusted", async () => {
    serveTarget(target({}, false));
    serveLabelCatalog("domain-1", makeLabelPage({ displayKey: "acme/web" }));
    const requests = serveCreate(created({ labels: ["bug", "wip"] }));
    const user = userEvent.setup();
    const { onCreated } = renderDialog();

    await user.type(await screen.findByTestId("create-issue-title"), "Crash");
    await user.click(await screen.findByTestId("label-picker-option-bug"));
    await user.click(screen.getByTestId("label-picker-option-wip"));
    // A typed name the catalog lacks is never added.
    await user.type(
      screen.getByTestId("label-picker-search"),
      "release{Enter}",
    );
    expect(screen.getByTestId("label-picker-no-match")).toBeVisible();
    await user.click(screen.getByTestId("create-issue-review-button"));
    expect(screen.getByTestId("create-issue-review-labels")).toHaveTextContent(
      "bug, wip",
    );
    await user.click(screen.getByTestId("create-issue-submit"));

    await waitFor(() => expect(onCreated).toHaveBeenCalled());
    expect(requests[0]).toMatchObject({
      title: "Crash",
      labels: ["bug", "wip"],
      domainId: "domain-1",
    });
  });

  it("treats a changed label set as a new submission", async () => {
    serveTarget(target());
    serveLabelCatalog();
    const requests = serveCreate(
      created({ state: "outcome_unknown", issue: null }),
      created(),
    );
    const user = userEvent.setup();
    renderDialog();

    await user.type(await screen.findByTestId("create-issue-title"), "Crash");
    await user.click(await screen.findByTestId("label-picker-option-bug"));
    await user.click(screen.getByTestId("create-issue-submit"));
    await screen.findByTestId("create-issue-unknown");
    await user.click(screen.getByTestId("label-picker-option-wip"));
    await user.click(screen.getByTestId("create-issue-submit"));

    await waitFor(() => expect(requests).toHaveLength(2));
    expect(requests[1].labels).toEqual(["bug", "wip"]);
    expect(requests[1].idempotencyKey).not.toBe(requests[0].idempotencyKey);
  });

  it("lists labels GitHub left out before continuing", async () => {
    serveTarget(target());
    serveLabelCatalog();
    serveCreate(created({ labels: ["bug", "wip"], labelsNotApplied: ["wip"] }));
    const user = userEvent.setup();
    const { onCreated } = renderDialog({ allowFork: true });

    await user.type(await screen.findByTestId("create-issue-title"), "Crash");
    await user.click(await screen.findByTestId("label-picker-option-bug"));
    await user.click(screen.getByTestId("label-picker-option-wip"));
    await user.click(screen.getByTestId("create-issue-submit-fork"));

    expect(
      await screen.findByTestId("create-issue-labels-not-applied"),
    ).toHaveTextContent(
      "Created acme/web#12, but GitHub did not apply the label “wip”: it leaves out labels the author may not set. You can add them from the issue's page.",
    );
    expect(onCreated).not.toHaveBeenCalled();
    expect(screen.queryByTestId("create-issue-submit")).not.toBeInTheDocument();
    await user.click(screen.getByTestId("create-issue-continue"));
    expect(onCreated).toHaveBeenCalledWith(
      expect.objectContaining({ displayKey: "acme/web#12" }),
      { fork: true },
    );
  });

  it("drops the labels when the destination changes", async () => {
    serveTarget(target());
    serveLabelCatalog();
    serveLabelCatalog(
      "domain-2",
      makeLabelPage({
        domainId: "domain-2",
        displayKey: "acme/other",
        labels: [{ id: "201", name: "bug", color: null }],
      }),
    );
    const requests = serveCreate(
      () =>
        HttpResponse.json(
          {
            error: {
              code: "CONFLICT",
              message: "the repository for new issues changed",
              details: { code: "destination_changed" },
            },
          },
          { status: 409 },
        ),
      created(),
    );
    const user = userEvent.setup();
    renderDialog();

    await user.type(await screen.findByTestId("create-issue-title"), "Crash");
    await user.click(await screen.findByTestId("label-picker-option-bug"));
    await user.click(screen.getByTestId("create-issue-submit"));
    await screen.findByTestId("create-issue-review-again");

    serveTarget(
      target({
        destination: {
          domainId: "domain-2",
          trackerId: "tracker-1",
          repositoryId: "12",
          displayKey: "acme/other",
          contentTrusted: true,
        },
      }),
    );
    await user.click(screen.getByTestId("create-issue-review-again"));
    await waitFor(() =>
      expect(screen.getByTestId("create-issue-destination")).toHaveTextContent(
        "acme/other",
      ),
    );
    // The other domain's `bug` is not the selection made for acme/web.
    expect(
      await screen.findByTestId("label-picker-option-bug"),
    ).not.toBeChecked();
    expect(screen.getByText("No labels selected.")).toBeVisible();
    await user.click(screen.getByTestId("create-issue-submit"));
    await waitFor(() => expect(requests).toHaveLength(2));
    expect(requests[0]).toMatchObject({
      domainId: "domain-1",
      labels: ["bug"],
    });
    expect(requests[1].domainId).toBe("domain-2");
    expect(requests[1].labels).toBeUndefined();
  });
});
