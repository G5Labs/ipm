/**
 * G5 issue image attachments in the browser (g5.issue.native.attachments).
 *
 * Covered: a G5 issue's text shows its organization's attachments (absolute
 * at the deployment or as a path) through the authenticated API read, never
 * by loading the page address as an image, says "Attachment removed" once
 * one is gone and fetches nothing for another organization's; the comment
 * composer uploads an attached image (fresh key per file, the same key on
 * Retry) and inserts its Markdown on its own line, keeping every image when
 * uploads finish together and refusing other files and oversized images
 * before sending; GitHub composers offer no upload; the
 * creation form keeps images behind placeholders, uploads them once the
 * issue exists and edits the description at the version read, reporting an
 * image that was not added while keeping the issue, then a parent that was
 * not added; an image chosen with Attach image before the person clicked or
 * typed in a description is only attached (edit panel and creation form),
 * one dropped then goes at the end and a placed caret is honoured; the
 * attachments list
 * offers Remove only to the uploader or an administrator and calls DELETE.
 */

import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { HttpResponse, http } from "msw";
import { Route, Routes } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { RequireAuth } from "../../hooks/useAuth";
import { IssuePage } from "../../pages/IssuePage";
import type {
  CreateIssueInput,
  ExternalIssueDetailDTO,
  NativeAttachmentListItemDTO,
} from "../../lib/types";
import {
  NATIVE_ALEX,
  NATIVE_MAYA,
  makeIssueCreation,
  makeIssueDetail,
  makeIssueWriteTarget,
  makeNativeCreateTarget,
  makeNativeIssueDetail,
  serveExternalIssuesFeature,
  serveNativeIssuesFeature,
} from "../../test/externalIssueFixtures";
import { createMockUser, createMockWorkspace } from "../../test/fixtures";
import { server } from "../../test/msw/server";
import { renderApp } from "../../test/render";
import { CreateIssueDialog } from "./CreateIssueDialog";

const ORG_ID = "org-g5labs";
const ISSUE_URL = `/api/v1/orgs/${ORG_ID}/issues/g5/12`;
const ATTACHMENTS_URL = `${ISSUE_URL}/attachments`;
const PAGE = "http://localhost:3000/g5labs/issues/g5/12";

const PNG_BYTES = new Uint8Array([137, 80, 78, 71]);

function pngFile(name = "shot.png") {
  return new File([PNG_BYTES], name, { type: "image/png" });
}

function attachment(
  id: string,
  overrides: Partial<NativeAttachmentListItemDTO> = {},
): NativeAttachmentListItemDTO {
  const url = `${PAGE}/attachments/${id}`;
  const fileName = overrides.fileName ?? `image-${id}.png`;
  return {
    id,
    fileName,
    contentType: "image/png",
    sizeBytes: 2048,
    sha256: "ab".repeat(32),
    uploader: NATIVE_ALEX,
    origin: "person",
    createdAt: "2026-10-30T12:00:00Z",
    url,
    markdown: `![${fileName}](${url})`,
    place: "description",
    commentId: null,
    ...overrides,
  };
}

const MEMBER = createMockUser({
  orgs: [
    {
      orgId: ORG_ID,
      orgName: "G5 Labs",
      orgSlug: "g5labs",
      role: "member",
    },
  ],
});

/** Serve image bytes for `present` ids, 404 for the rest; record headers. */
function serveImages(present: Set<string>) {
  const reads: { id: string; authorization: string | null }[] = [];
  server.use(
    http.get(`${ATTACHMENTS_URL}/:attachmentId`, ({ params, request }) => {
      const id = String(params.attachmentId);
      reads.push({ id, authorization: request.headers.get("Authorization") });
      if (!present.has(id)) {
        return HttpResponse.json(
          { error: { code: "NOT_FOUND", message: "the attachment" } },
          { status: 404 },
        );
      }
      return new HttpResponse(PNG_BYTES, {
        headers: { "Content-Type": "image/png" },
      });
    }),
  );
  return reads;
}

function serveIssue(detail: ExternalIssueDetailDTO) {
  server.use(http.get(ISSUE_URL, () => HttpResponse.json({ data: detail })));
}

function renderNativePage(user = createMockUser()) {
  return renderApp(
    <Routes>
      <Route
        path="/:org/issues/g5/:number"
        element={
          <RequireAuth>
            <IssuePage />
          </RequireAuth>
        }
      />
    </Routes>,
    { route: "/g5labs/issues/g5/12", user },
  );
}

beforeEach(() => {
  window.localStorage.clear();
  server.use(
    http.get("/api/v1/orgs/:orgId/workspaces", () =>
      HttpResponse.json({
        data: [createMockWorkspace({ id: "base", name: "base", isBase: true })],
      }),
    ),
  );
});

describe("G5 issue text with attachments", () => {
  beforeEach(() => serveNativeIssuesFeature());

  it("loads its organization's attachments through the API and marks removed ones", async () => {
    const reads = serveImages(new Set(["5"]));
    const detail = makeNativeIssueDetail({
      body: `Before\n\n![shot.png](${PAGE}/attachments/5)`,
    });
    detail.comments[0] = {
      ...detail.comments[0],
      body: [
        "![gone.png](/g5labs/issues/g5/12/attachments/6)",
        "![other.png](/acme/issues/g5/1/attachments/9)",
      ].join("\n\n"),
    };
    serveIssue(detail);
    renderNativePage();

    const body = await screen.findByTestId("external-issue-body");
    const image = await within(body).findByTestId("native-attachment-image");
    expect(image).toHaveAttribute("alt", "shot.png");
    await waitFor(() =>
      expect(image).toHaveAttribute("src", expect.stringMatching(/^blob:/)),
    );

    const comment = screen.getByTestId("external-issue-comment-41");
    expect(
      await within(comment).findByTestId("native-attachment-removed"),
    ).toHaveTextContent("Attachment removed");
    expect(comment).toHaveTextContent("Image from another organization");

    // Only this organization's attachments were read, with the bearer token.
    expect(reads.map((read) => read.id).sort()).toEqual(["5", "6"]);
    for (const read of reads) {
      expect(read.authorization).toMatch(/^Bearer /);
    }
    // No image element ever points at an attachment's page address.
    for (const element of document.querySelectorAll("img")) {
      expect(element.getAttribute("src") ?? "").not.toContain("/attachments/");
    }
  });
});

describe("Uploading from a G5 issue's comment composer", () => {
  beforeEach(() => {
    serveNativeIssuesFeature();
    serveIssue(makeNativeIssueDetail());
  });

  it("uploads an attached image and inserts its Markdown on its own line", async () => {
    const uploads: Record<string, string>[] = [];
    server.use(
      http.post(ATTACHMENTS_URL, async ({ request }) => {
        uploads.push((await request.json()) as Record<string, string>);
        return HttpResponse.json(
          { data: attachment("7", { fileName: "shot.png" }) },
          { status: 201 },
        );
      }),
    );
    const user = userEvent.setup();
    renderNativePage();

    const input = await screen.findByTestId("activity-comment-input");
    await user.type(input, "Look:");
    await user.upload(
      screen.getByTestId("activity-comment-image-input"),
      pngFile(),
    );

    await waitFor(() =>
      expect(input).toHaveValue(`Look:\n![shot.png](${PAGE}/attachments/7)\n`),
    );
    expect(uploads).toEqual([
      {
        fileName: "shot.png",
        contentBase64: "iVBORw==",
        idempotencyKey: expect.any(String),
      },
    ]);
  });

  it("keeps every image's Markdown when uploads finish together", async () => {
    // Both responses wait for one release, so they settle before a render.
    let release!: () => void;
    const released = new Promise<void>((resolve) => (release = resolve));
    let next = 7;
    server.use(
      http.post(ATTACHMENTS_URL, async ({ request }) => {
        const { fileName } = (await request.json()) as { fileName: string };
        const id = String(next++);
        if (next === 9) release();
        await released;
        return HttpResponse.json(
          {
            data: attachment(id, {
              fileName,
              markdown: `![${fileName}](${PAGE}/attachments/${id})`,
            }),
          },
          { status: 201 },
        );
      }),
    );
    const user = userEvent.setup();
    renderNativePage();

    const input = await screen.findByTestId("activity-comment-input");
    await user.upload(screen.getByTestId("activity-comment-image-input"), [
      pngFile("one.png"),
      pngFile("two.png"),
    ]);

    await waitFor(() => {
      const text = (input as HTMLTextAreaElement).value;
      expect(text).toContain("![one.png](");
      expect(text).toContain("![two.png](");
    });
  });

  it("puts the image where it was added, whatever is typed meanwhile", async () => {
    let release = () => {};
    const released = new Promise<void>((resolve) => (release = resolve));
    server.use(
      http.post(ATTACHMENTS_URL, async () => {
        await released;
        return HttpResponse.json(
          { data: attachment("7", { fileName: "shot.png" }) },
          { status: 201 },
        );
      }),
    );
    const user = userEvent.setup();
    renderNativePage();
    const input = (await screen.findByTestId(
      "activity-comment-input",
    )) as HTMLTextAreaElement;
    await user.type(input, "Before");
    await user.upload(
      screen.getByTestId("activity-comment-image-input"),
      pngFile(),
    );
    await waitFor(() => expect(input.value).toContain("pending-image:"));
    await user.click(input);
    input.setSelectionRange(input.value.length, input.value.length);
    await user.type(input, "after");
    release();

    await waitFor(() =>
      expect(input).toHaveValue(
        `Before\n![shot.png](${PAGE}/attachments/7)\nafter`,
      ),
    );
  });

  it("refuses non-images and oversized images before sending", async () => {
    let posted = 0;
    server.use(
      http.post(ATTACHMENTS_URL, () => {
        posted += 1;
        return HttpResponse.json({ data: attachment("7") }, { status: 201 });
      }),
    );
    renderNativePage();
    const input = await screen.findByTestId("activity-comment-input");

    const text = new File(["notes"], "notes.txt", { type: "text/plain" });
    fireEvent.paste(input, {
      clipboardData: {
        items: [{ kind: "file", getAsFile: () => text }],
        files: [text],
      },
    });
    expect(
      await screen.findByTestId("activity-comment-image-refused"),
    ).toHaveTextContent("notes.txt is not a PNG, JPEG, GIF or WebP image.");

    const big = pngFile("huge.png");
    Object.defineProperty(big, "size", { value: 10 * 1024 * 1024 + 1 });
    fireEvent.drop(input, {
      dataTransfer: {
        items: [{ kind: "file", getAsFile: () => big }],
        files: [big],
        types: ["Files"],
      },
    });
    expect(
      await screen.findByTestId("activity-comment-image-refused"),
    ).toHaveTextContent("huge.png is larger than 10 MiB.");
    expect(posted).toBe(0);
    expect(input).toHaveValue("");
  });

  it("retries a failed upload under the same key", async () => {
    const keys: string[] = [];
    server.use(
      http.post(ATTACHMENTS_URL, async ({ request }) => {
        const body = (await request.json()) as { idempotencyKey: string };
        keys.push(body.idempotencyKey);
        if (keys.length === 1) {
          return HttpResponse.json(
            { error: { code: "INTERNAL", message: "storage unavailable" } },
            { status: 500 },
          );
        }
        return HttpResponse.json(
          { data: attachment("7", { fileName: "shot.png" }) },
          { status: 201 },
        );
      }),
    );
    const user = userEvent.setup();
    renderNativePage();
    const input = await screen.findByTestId("activity-comment-input");
    await user.upload(
      screen.getByTestId("activity-comment-image-input"),
      pngFile(),
    );
    expect(
      await screen.findByTestId("activity-comment-image-failed"),
    ).toHaveTextContent("shot.png was not added");
    expect(input).toHaveValue("");

    await user.click(screen.getByTestId("activity-comment-image-retry"));
    await waitFor(() =>
      expect(input).toHaveValue(`![shot.png](${PAGE}/attachments/7)\n`),
    );
    expect(keys).toHaveLength(2);
    expect(keys[1]).toBe(keys[0]);
    expect(
      screen.queryByTestId("activity-comment-image-failed"),
    ).not.toBeInTheDocument();
  });

  it("offers no upload on a GitHub issue's page", async () => {
    serveExternalIssuesFeature();
    server.use(
      http.get("/api/v1/orgs/org-g5labs/issues/github/11/12", () =>
        HttpResponse.json({ data: makeIssueDetail() }),
      ),
      http.get("/api/v1/orgs/org-g5labs/issues/github/11/12/write-target", () =>
        HttpResponse.json({ data: makeIssueWriteTarget() }),
      ),
    );
    renderApp(
      <Routes>
        <Route
          path="/:org/issues/github/:repositoryId/:number"
          element={
            <RequireAuth>
              <IssuePage />
            </RequireAuth>
          }
        />
      </Routes>,
      { route: "/g5labs/issues/github/11/12", user: createMockUser() },
    );
    expect(
      await screen.findByTestId("activity-comment-input"),
    ).toBeInTheDocument();
    expect(
      screen.queryByTestId("activity-comment-image-button"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("native-issue-attachments"),
    ).not.toBeInTheDocument();
  });
});

describe("Placing images in a G5 issue's description", () => {
  beforeEach(() => {
    serveNativeIssuesFeature();
    serveIssue(makeNativeIssueDetail());
  });

  function serveUploads() {
    const uploads: string[] = [];
    server.use(
      http.post(ATTACHMENTS_URL, async ({ request }) => {
        const { fileName } = (await request.json()) as { fileName: string };
        uploads.push(fileName);
        const url = `${PAGE}/attachments/${uploads.length}`;
        return HttpResponse.json(
          {
            data: attachment(String(uploads.length), {
              fileName,
              url,
              markdown: `![${fileName}](${url})`,
            }),
          },
          { status: 201 },
        );
      }),
    );
    return uploads;
  }

  it("only attaches an image chosen before the person was in the description", async () => {
    const uploads = serveUploads();
    const user = userEvent.setup();
    renderNativePage();

    await user.click(await screen.findByTestId("issue-page-edit-button"));
    const body = screen.getByTestId(
      "issue-page-edit-body",
    ) as HTMLTextAreaElement;
    await user.upload(
      screen.getByTestId("issue-page-edit-image-input"),
      pngFile("logo.png"),
    );

    expect(
      await screen.findByTestId("issue-page-edit-image-attached"),
    ).toHaveTextContent("Attached logo.png to the issue");
    expect(uploads).toEqual(["logo.png"]);
    expect(body).toHaveValue("Native search takes **seconds**.");

    // Once the person has been in the box, its caret says where.
    await user.click(body);
    body.setSelectionRange(0, 0);
    await user.upload(
      screen.getByTestId("issue-page-edit-image-input"),
      pngFile("shot.png"),
    );
    await waitFor(() =>
      expect(body).toHaveValue(
        `![shot.png](${PAGE}/attachments/2)\nNative search takes **seconds**.`,
      ),
    );
  });

  it("puts an image dropped before the person was in the description at its end", async () => {
    serveUploads();
    const user = userEvent.setup();
    renderNativePage();

    await user.click(await screen.findByTestId("issue-page-edit-button"));
    const body = screen.getByTestId("issue-page-edit-body");
    const file = pngFile("drop.png");
    fireEvent.drop(body, {
      dataTransfer: {
        items: [{ kind: "file", getAsFile: () => file }],
        files: [file],
      },
    });

    await waitFor(() =>
      expect(body).toHaveValue(
        `Native search takes **seconds**.\n![drop.png](${PAGE}/attachments/1)\n`,
      ),
    );
  });
});

describe("The attachments list on a G5 issue's page", () => {
  beforeEach(() => {
    serveNativeIssuesFeature();
    serveIssue(
      makeNativeIssueDetail({
        body: `![mine.png](${PAGE}/attachments/5)`,
      }),
    );
  });

  function serveList() {
    const present = new Set(["5", "8"]);
    const deleted: string[] = [];
    serveImages(present);
    server.use(
      http.get(ATTACHMENTS_URL, () =>
        HttpResponse.json({
          data: [
            attachment("5", { fileName: "mine.png" }),
            attachment("8", {
              fileName: "hers.png",
              uploader: NATIVE_MAYA,
              place: "issue",
              sizeBytes: 3 * 1024 * 1024,
            }),
          ].filter((item) => present.has(item.id)),
        }),
      ),
      http.delete(`${ATTACHMENTS_URL}/:attachmentId`, ({ params }) => {
        const id = String(params.attachmentId);
        deleted.push(id);
        present.delete(id);
        return new HttpResponse(null, { status: 204 });
      }),
    );
    return deleted;
  }

  it("lets the uploader remove their own after confirming", async () => {
    const deleted = serveList();
    const user = userEvent.setup();
    renderNativePage(MEMBER);

    const mine = await screen.findByTestId("native-attachment-5");
    expect(mine).toHaveTextContent("mine.png");
    expect(mine).toHaveTextContent("2 KB · Alex Chen · In the description");
    const hers = screen.getByTestId("native-attachment-8");
    expect(hers).toHaveTextContent(
      "3.0 MB · Maya Patel · Not shown in the text",
    );
    expect(
      within(hers).queryByTestId("native-attachment-remove-8"),
    ).not.toBeInTheDocument();

    await user.click(screen.getByTestId("native-attachment-remove-5"));
    expect(deleted).toEqual([]);
    await user.click(
      screen.getByTestId("native-attachment-remove-confirm-button-5"),
    );
    await waitFor(() => expect(deleted).toEqual(["5"]));
    await waitFor(() =>
      expect(
        screen.queryByTestId("native-attachment-5"),
      ).not.toBeInTheDocument(),
    );
    // The description still names it, and now says it was removed.
    const body = screen.getByTestId("external-issue-body");
    expect(
      await within(body).findByTestId("native-attachment-removed"),
    ).toBeInTheDocument();
  });

  it("lets an administrator remove anyone's", async () => {
    serveList();
    renderNativePage(createMockUser());
    const hers = await screen.findByTestId("native-attachment-8");
    expect(
      within(hers).getByTestId("native-attachment-remove-8"),
    ).toBeInTheDocument();
    expect(
      screen.getByTestId("native-attachment-remove-5"),
    ).toBeInTheDocument();
  });

  it("offers viewers no Remove", async () => {
    serveList();
    renderNativePage(
      createMockUser({
        orgs: [
          {
            orgId: ORG_ID,
            orgName: "G5 Labs",
            orgSlug: "g5labs",
            role: "viewer",
          },
        ],
      }),
    );
    await screen.findByTestId("native-attachment-5");
    expect(
      screen.queryByTestId("native-attachment-remove-5"),
    ).not.toBeInTheDocument();
  });
});

describe("Images in the creation form", () => {
  const CREATED_URL = `/api/v1/orgs/${ORG_ID}/issues/g5/13`;

  function renderDialog(createTarget = makeNativeCreateTarget()) {
    server.use(
      http.get(`/api/v1/orgs/${ORG_ID}/issues/create-target`, () =>
        HttpResponse.json({ data: createTarget }),
      ),
    );
    const onCreated = vi.fn();
    renderApp(
      <CreateIssueDialog
        orgId={ORG_ID}
        open
        onClose={() => {}}
        onCreated={onCreated}
      />,
      { route: "/g5labs/home", user: createMockUser() },
    );
    return { onCreated };
  }

  it("uploads pending images to the new issue and reports one that was not added", async () => {
    serveNativeIssuesFeature();
    let sent: CreateIssueInput | null = null;
    const uploads: string[] = [];
    const patches: { body: string; expectedUpdatedAt: string }[] = [];
    server.use(
      http.post(`/api/v1/orgs/${ORG_ID}/issues`, async ({ request }) => {
        sent = (await request.json()) as CreateIssueInput;
        return HttpResponse.json(
          {
            data: makeIssueCreation({
              issue: {
                externalIssueId: "713",
                repositoryId: "g5",
                number: 13,
                displayKey: "ACME-13",
                title: sent.title,
                url: "http://localhost:3000/g5labs/issues/g5/13",
              },
              idempotencyKey: sent.idempotencyKey,
              destination: makeNativeCreateTarget().destination!,
              githubLogin: null,
              payload: sent.body,
            }),
          },
          { status: 201 },
        );
      }),
      http.post(`${CREATED_URL}/attachments`, async ({ request }) => {
        const { fileName } = (await request.json()) as { fileName: string };
        uploads.push(fileName);
        if (fileName === "bad.gif") {
          return HttpResponse.json(
            {
              error: {
                code: "VALIDATION_ERROR",
                message: "invalid",
                details: {
                  field: "content",
                  error: "must be a PNG, JPEG, GIF or WebP image",
                },
              },
            },
            { status: 422 },
          );
        }
        const url = "http://localhost:3000/g5labs/issues/g5/13/attachments/7";
        return HttpResponse.json(
          {
            data: attachment("7", {
              fileName,
              url,
              markdown: `![${fileName}](${url})`,
            }),
          },
          { status: 201 },
        );
      }),
      http.get(CREATED_URL, () =>
        HttpResponse.json({
          data: makeNativeIssueDetail({
            locator: { repositoryId: "g5", number: 13 },
            body: sent?.body ?? "",
            updatedAt: "2026-11-01T09:00:00.000001Z",
          }),
        }),
      ),
      http.patch(CREATED_URL, async ({ request }) => {
        patches.push(
          (await request.json()) as { body: string; expectedUpdatedAt: string },
        );
        return HttpResponse.json({
          data: makeNativeIssueDetail({
            locator: { repositoryId: "g5", number: 13 },
          }),
        });
      }),
    );
    const user = userEvent.setup();
    const { onCreated } = renderDialog();

    await user.type(await screen.findByTestId("create-issue-title"), "Broken");
    const body = screen.getByTestId("create-issue-body");
    await user.type(body, "Steps:");
    await user.upload(
      screen.getByTestId("create-issue-image-input"),
      pngFile("shot.png"),
    );
    const gif = new File([new Uint8Array([1, 2])], "bad.gif", {
      type: "image/gif",
    });
    fireEvent.paste(body, {
      clipboardData: {
        items: [{ kind: "file", getAsFile: () => gif }],
        files: [gif],
      },
    });
    await waitFor(() =>
      expect((body as HTMLTextAreaElement).value).toContain(
        "![Uploading bad.gif…](pending-image:",
      ),
    );
    expect((body as HTMLTextAreaElement).value).toContain(
      "![Uploading shot.png…](pending-image:",
    );

    await user.click(screen.getByTestId("create-issue-submit"));

    const notice = await screen.findByTestId("create-issue-image-notice");
    expect(notice).toHaveTextContent(
      "Created ACME-13, but an image was not added:",
    );
    expect(notice).toHaveTextContent(
      "bad.gif was not added: must be a PNG, JPEG, GIF or WebP image.",
    );
    expect(uploads).toEqual(["shot.png", "bad.gif"]);
    expect(sent!.body).toContain("pending-image:");
    expect(patches).toHaveLength(1);
    expect(patches[0].expectedUpdatedAt).toBe("2026-11-01T09:00:00.000001Z");
    expect(patches[0].body).toContain(
      "![shot.png](http://localhost:3000/g5labs/issues/g5/13/attachments/7)",
    );
    expect(patches[0].body).toContain("_(Image bad.gif was not added.)_");
    expect(patches[0].body).not.toContain("pending-image:");
    expect(patches[0].body.startsWith("Steps:\n")).toBe(true);

    // The issue exists: continuing hands it on.
    expect(onCreated).not.toHaveBeenCalled();
    await user.click(screen.getByTestId("create-issue-image-continue"));
    expect(onCreated.mock.calls[0][0]).toMatchObject({ displayKey: "ACME-13" });
  });

  it("only attaches an image chosen before the person was in the description", async () => {
    serveNativeIssuesFeature();
    let sent: CreateIssueInput | null = null;
    const uploads: string[] = [];
    let patched = 0;
    server.use(
      http.post(`/api/v1/orgs/${ORG_ID}/issues`, async ({ request }) => {
        sent = (await request.json()) as CreateIssueInput;
        return HttpResponse.json(
          {
            data: makeIssueCreation({
              issue: {
                externalIssueId: "713",
                repositoryId: "g5",
                number: 13,
                displayKey: "ACME-13",
                title: sent.title,
                url: "http://localhost:3000/g5labs/issues/g5/13",
              },
              idempotencyKey: sent.idempotencyKey,
              destination: makeNativeCreateTarget().destination!,
              githubLogin: null,
              payload: sent.body,
            }),
          },
          { status: 201 },
        );
      }),
      http.post(`${CREATED_URL}/attachments`, async ({ request }) => {
        const { fileName } = (await request.json()) as { fileName: string };
        uploads.push(fileName);
        return HttpResponse.json(
          { data: attachment("7", { fileName }) },
          { status: 201 },
        );
      }),
      http.patch(CREATED_URL, () => {
        patched += 1;
        return HttpResponse.json({ data: makeNativeIssueDetail() });
      }),
    );
    const user = userEvent.setup();
    const { onCreated } = renderDialog();

    await user.type(await screen.findByTestId("create-issue-title"), "Logo");
    const input = screen.getByTestId("create-issue-image-input");
    await user.upload(input, pngFile("logo.png"));
    await user.upload(input, pngFile("spare.png"));

    expect(screen.getByTestId("create-issue-body")).toHaveValue("");
    const listed = screen.getAllByTestId("create-issue-image-unplaced");
    expect(listed[0]).toHaveTextContent(
      "logo.png is attached to the issue, not shown in the description.",
    );
    await user.click(
      within(listed[1]).getByTestId("create-issue-image-unplaced-remove"),
    );
    expect(screen.getAllByTestId("create-issue-image-unplaced")).toHaveLength(
      1,
    );

    await user.click(screen.getByTestId("create-issue-submit"));

    await waitFor(() => expect(onCreated).toHaveBeenCalled());
    expect(sent!.body).toBe("");
    expect(uploads).toEqual(["logo.png"]);
    expect(patched).toBe(0);
  });

  it("says so when the new description lost an image's place meanwhile", async () => {
    serveNativeIssuesFeature();
    let patched = 0;
    server.use(
      http.post(`/api/v1/orgs/${ORG_ID}/issues`, async ({ request }) => {
        const sent = (await request.json()) as CreateIssueInput;
        return HttpResponse.json(
          {
            data: makeIssueCreation({
              issue: {
                externalIssueId: "713",
                repositoryId: "g5",
                number: 13,
                displayKey: "ACME-13",
                title: sent.title,
                url: "http://localhost:3000/g5labs/issues/g5/13",
              },
              idempotencyKey: sent.idempotencyKey,
              destination: makeNativeCreateTarget().destination!,
              githubLogin: null,
              payload: sent.body,
            }),
          },
          { status: 201 },
        );
      }),
      http.post(`${CREATED_URL}/attachments`, () =>
        HttpResponse.json({ data: attachment("7") }, { status: 201 }),
      ),
      // Someone rewrote the description before the images were shown.
      http.get(CREATED_URL, () =>
        HttpResponse.json({
          data: makeNativeIssueDetail({
            locator: { repositoryId: "g5", number: 13 },
            body: "Rewritten",
          }),
        }),
      ),
      http.patch(CREATED_URL, () => {
        patched += 1;
        return HttpResponse.json({ data: makeNativeIssueDetail() });
      }),
    );
    const user = userEvent.setup();
    renderDialog();
    await user.type(await screen.findByTestId("create-issue-title"), "Broken");
    await user.click(screen.getByTestId("create-issue-body"));
    await user.upload(
      screen.getByTestId("create-issue-image-input"),
      pngFile("shot.png"),
    );
    await user.click(screen.getByTestId("create-issue-submit"));

    const notice = await screen.findByTestId("create-issue-image-notice");
    expect(notice).toHaveTextContent(
      "someone changed it first, removing where an image was to go",
    );
    expect(notice).toHaveTextContent("listed on the issue's page");
    expect(patched).toBe(0);
  });

  it("still reports a parent that was not added after the image notice", async () => {
    serveNativeIssuesFeature();
    server.use(
      http.post(`/api/v1/orgs/${ORG_ID}/issues`, async ({ request }) => {
        const sent = (await request.json()) as CreateIssueInput;
        return HttpResponse.json(
          {
            data: makeIssueCreation({
              issue: {
                externalIssueId: "713",
                repositoryId: "g5",
                number: 13,
                displayKey: "ACME-13",
                title: sent.title,
                url: "http://localhost:3000/g5labs/issues/g5/13",
              },
              idempotencyKey: sent.idempotencyKey,
              destination: makeNativeCreateTarget().destination!,
              githubLogin: null,
              payload: sent.body,
              parent: {
                externalIssueId: "707",
                number: 7,
                displayKey: "ACME-7",
                title: "Epic",
                url: "http://localhost:3000/g5labs/issues/g5/7",
                state: "not_attached",
                message: "the parent is closed",
              },
            }),
          },
          { status: 201 },
        );
      }),
      http.post(`${CREATED_URL}/attachments`, () =>
        HttpResponse.json(
          { error: { code: "INTERNAL_ERROR", message: "boom" } },
          { status: 500 },
        ),
      ),
      http.get(CREATED_URL, () =>
        HttpResponse.json({
          data: makeNativeIssueDetail({
            locator: { repositoryId: "g5", number: 13 },
          }),
        }),
      ),
      http.patch(CREATED_URL, () =>
        HttpResponse.json({
          data: makeNativeIssueDetail({
            locator: { repositoryId: "g5", number: 13 },
          }),
        }),
      ),
    );
    const user = userEvent.setup();
    const { onCreated } = renderDialog();

    await user.type(await screen.findByTestId("create-issue-title"), "Broken");
    await user.upload(
      screen.getByTestId("create-issue-image-input"),
      pngFile("shot.png"),
    );
    await user.click(screen.getByTestId("create-issue-submit"));

    await screen.findByTestId("create-issue-image-notice");
    await user.click(screen.getByTestId("create-issue-image-continue"));

    // The parent outcome is not lost behind the image notice.
    expect(onCreated).not.toHaveBeenCalled();
    expect(
      await screen.findByTestId("create-issue-parent-notice"),
    ).toHaveTextContent("Created ACME-13, but it was not added under ACME-7.");
  });

  it("offers no images for a GitHub destination", async () => {
    serveExternalIssuesFeature();
    renderDialog(
      makeNativeCreateTarget({
        destination: {
          domainId: "domain-1",
          trackerId: "tracker-1",
          repositoryId: "11",
          displayKey: "acme/web",
          contentTrusted: true,
        },
        githubLogin: "ada-gh",
        assignableUsers: [],
      }),
    );
    await screen.findByText("acme/web");
    expect(
      screen.queryByTestId("create-issue-image-button"),
    ).not.toBeInTheDocument();
  });
});
