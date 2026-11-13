/** The assistant-reply wait's failure bound (ontology
 * g5.quality.delivery.ci.provider-failure-bound), checked against real DOM
 * with injected transcript and task-status readers; no backend is involved.
 */
import { expect, test, type Page } from "@playwright/test";

import {
  APP_BASE_URL,
  captureChatTurn,
  expectAssistantReply,
} from "../helpers";

const TURN = { taskId: "task", sessionId: "session", inputId: "current-input" };
const readMessages = async () => [
  { id: "current-input", role: "user", messageIndex: 4 },
  { id: "current", role: "assistant", taskId: "task", messageIndex: 5 },
];

const ERROR_BLOCK =
  '<div data-testid="assistant-status-block" data-status-kind="error" role="alert">Claude Code exited before completing its response.</div>';

async function render(page: Page, inner: string) {
  await page.setContent(
    `<div data-testid="assistant-message">earlier reply</div><div data-testid="assistant-message" data-message-id="current" id="latest">${inner}</div>`,
  );
}

async function appendLater(page: Page, html: string, delayMs: number) {
  await page.evaluate(
    ({ html: content, delayMs: delay }) => {
      setTimeout(() => {
        document
          .getElementById("latest")!
          .insertAdjacentHTML("beforeend", content);
      }, delay);
    },
    { html, delayMs },
  );
}

async function transcriptReadFailures(
  page: Page,
  statuses: number[],
  body = "done",
  onReadFailure?: () => Promise<void>,
) {
  let reads = 0;
  await page.route(`${APP_BASE_URL}/projects`, (route) =>
    route.fulfill({
      contentType: "text/html",
      body: `<div data-testid="assistant-message" data-message-id="current" id="latest">${body}</div>`,
    }),
  );
  await page.route(
    `${APP_BASE_URL}/api/v1/sessions/session/messages?*`,
    async (route) => {
      const status = statuses[Math.min(reads++, statuses.length - 1)];
      if (status !== 200) await onReadFailure?.();
      return route.fulfill({
        status,
        json:
          status === 200
            ? {
                data: {
                  messages: await readMessages(),
                  cursor: null,
                  hasMore: false,
                },
              }
            : { error: "transcript unavailable" },
      });
    },
  );
  await page.goto(`${APP_BASE_URL}/projects`);
  return () => reads;
}

test.describe("assistant reply wait", () => {
  test("recovers from one transient transcript HTTP failure", async ({
    page,
  }) => {
    const reads = await transcriptReadFailures(page, [503, 200]);
    await expectAssistantReply(page, TURN, "done", {
      timeout: 10_000,
      readTaskStatus: async () => "running",
    });
    expect(reads()).toBe(2);
  });

  test("reports persistent transcript HTTP errors after three failed reads", async ({
    page,
  }) => {
    const reads = await transcriptReadFailures(page, [503]);
    const error = await expectAssistantReply(page, TURN, "done", {
      timeout: 20_000,
      readTaskStatus: async () => "running",
    }).then(
      () => undefined,
      (failure: Error) => failure,
    );
    expect(error?.message).toContain(
      "Chat transcript read failed with HTTP 503",
    );
    expect(reads()).toBe(3);
  });

  test("does not retry a definitive transcript authorization failure", async ({
    page,
  }) => {
    const reads = await transcriptReadFailures(page, [403]);
    const error = await expectAssistantReply(page, TURN, "done", {
      timeout: 20_000,
      readTaskStatus: async () => "running",
    }).then(
      () => undefined,
      (failure: Error) => failure,
    );
    expect(error?.message).toContain("chat transcript must be readable");
    expect(reads()).toBe(1);
  });

  test("a transient read does not delay an already identified provider failure", async ({
    page,
  }) => {
    const reads = await transcriptReadFailures(
      page,
      [200, 503],
      "working",
      async () => {
        await page.locator("#latest").evaluate((element, html) => {
          element.insertAdjacentHTML("beforeend", html);
        }, ERROR_BLOCK);
      },
    );
    const error = await expectAssistantReply(page, TURN, "done", {
      timeout: 20_000,
      readTaskStatus: async () => "failed",
    }).then(
      () => undefined,
      (failure: Error) => failure,
    );
    expect(error?.message).toContain("The provider turn failed");
    expect(error?.message).toContain(
      "Claude Code exited before completing its response.",
    );
    expect(reads()).toBe(2);
  });

  for (const send of ["start", "follow-up"]) {
    test(`resolves ${send} input to its distinct native reply across transcript pages`, async ({
      page,
    }) => {
      const start =
        send === "start"
          ? `${APP_BASE_URL}/api/v1/workspaces/workspace/sessions/start`
          : `${APP_BASE_URL}/api/v1/sessions/session/messages`;
      await page.route(`${APP_BASE_URL}/api/v1/sessions/session`, (route) =>
        route.fulfill({
          json: { data: { workspaceId: "workspace" } },
        }),
      );
      await page.route(
        `${APP_BASE_URL}/api/v1/sessions/session/task`,
        (route) =>
          route.fulfill({
            status: 500,
            json: {
              error: "must retain the task identity returned by this send",
            },
          }),
      );
      const cursors: Array<string | null> = [];
      await page.route(`${APP_BASE_URL}/projects`, (route) =>
        route.fulfill({
          contentType: "text/html",
          body:
            '<div data-testid="assistant-message" data-message-id="previous">accepted reply</div>' +
            '<div data-testid="assistant-message" data-message-id="current" id="latest">working</div>' +
            '<div data-testid="assistant-message" data-message-id="later">accepted reply</div>',
        }),
      );
      await page.route(`${APP_BASE_URL}/api/v1/tasks/task`, (route) =>
        route.fulfill({
          json: { data: { status: "running" } },
        }),
      );
      await page.route(start, (route) =>
        route.fulfill({
          status: 201,
          json: {
            data:
              send === "start"
                ? {
                    session: { id: "session", workspaceId: "workspace" },
                    message: { inputId: "current-input", taskId: "task" },
                    assistantMessageId: "current-input",
                    taskId: "task",
                  }
                : {
                    inputId: "current-input",
                    messageId: "current-input",
                    taskId: "task",
                  },
          },
        }),
      );
      await page.route(
        `${APP_BASE_URL}/api/v1/sessions/session/messages?*`,
        (route) => {
          const cursor = new URL(route.request().url()).searchParams.get(
            "cursor",
          );
          cursors.push(cursor);
          if (cursors.length === 1)
            return route.fulfill({
              json: { data: { messages: [], cursor: null, hasMore: false } },
            });
          return route.fulfill({
            json: {
              data:
                cursor === null
                  ? {
                      messages: [
                        {
                          id: "current-input",
                          role: "user",
                          messageIndex: 499,
                        },
                      ],
                      cursor: 499,
                      hasMore: true,
                    }
                  : {
                      messages: [
                        {
                          id: "current",
                          role: "assistant",
                          taskId: "task",
                          messageIndex: 500,
                        },
                      ],
                      cursor: null,
                      hasMore: false,
                    },
            },
          });
        },
      );
      await page.goto(`${APP_BASE_URL}/projects`);
      const sent = page.waitForResponse((response) => response.url() === start);
      await page.evaluate((url) => fetch(url, { method: "POST" }), start);
      const turn = await captureChatTurn(page, await sent);
      expect(turn).toEqual({ ...TURN, workspaceId: "workspace" });
      await appendLater(page, "accepted reply", 1_500);
      await expectAssistantReply(page, turn, "accepted reply", {
        timeout: 10_000,
      });
      expect(cursors[0]).toBeNull();
      expect(cursors).toContain("499");
      expect(await page.locator("#latest").textContent()).toContain(
        "accepted reply",
      );
    });
  }

  test("resolves a steered input to its own paired reply in the same task", async ({
    page,
  }) => {
    await page.setContent(
      '<div data-testid="assistant-message" data-message-id="canonical-attempt">done</div>' +
        '<div data-testid="assistant-message" data-message-id="steered-reservation-reply">steering</div>' +
        '<div data-testid="assistant-message" data-message-id="unrelated-reply">done</div>',
    );
    await page.evaluate(() =>
      setTimeout(() => {
        document.querySelector(
          '[data-message-id="steered-reservation-reply"]',
        )!.textContent = "done";
      }, 1_500),
    );
    await expectAssistantReply(
      page,
      { ...TURN, inputId: "steered-input" },
      "done",
      {
        timeout: 10_000,
        readMessages: async () => [
          { id: "initial-input", role: "user", messageIndex: 0 },
          {
            id: "canonical-attempt",
            role: "assistant",
            taskId: "task",
            messageIndex: 1,
          },
          { id: "steered-input", role: "user", messageIndex: 2 },
          {
            id: "steered-reservation-reply",
            role: "assistant",
            taskId: "task",
            messageIndex: 3,
          },
          { id: "unrelated-input", role: "user", messageIndex: 4 },
          {
            id: "unrelated-reply",
            role: "assistant",
            taskId: "task",
            messageIndex: 5,
          },
        ],
        readTaskStatus: async () => "running",
      },
    );
    expect(
      await page
        .locator('[data-message-id="steered-reservation-reply"]')
        .textContent(),
    ).toBe("done");
  });

  test("fails promptly when the task fails before input acceptance", async ({
    page,
  }) => {
    await page.setContent(
      '<div data-testid="assistant-message">earlier reply</div>',
    );
    const error = await expectAssistantReply(page, TURN, "reply", {
      timeout: 3_000,
      readMessages: async () => [],
      readTaskStatus: async () => "failed",
    }).then(
      () => undefined,
      (failure: Error) => failure,
    );
    expect(error?.message).toContain("The provider turn failed");
    expect(error?.message).toContain("input was not accepted");
  });

  test("follows a native question's task-bound continuation without accepting a different task's reply", async ({
    page,
  }) => {
    const reply = "Native question answered Blue";
    await page.route(`${APP_BASE_URL}/projects`, (route) =>
      route.fulfill({
        contentType: "text/html",
        body:
          '<div data-testid="assistant-message" data-message-id="current">Question answered</div>' +
          '<div data-testid="assistant-message" data-message-id="continuation" id="latest">resuming</div>' +
          `<div data-testid="assistant-message" data-message-id="unrelated">${reply}</div>`,
      }),
    );
    const cursors: Array<string | null> = [];
    await page.route(
      `${APP_BASE_URL}/api/v1/sessions/session/messages?*`,
      async (route) => {
        const cursor = new URL(route.request().url()).searchParams.get(
          "cursor",
        );
        cursors.push(cursor);
        return route.fulfill({
          json: {
            data:
              cursors.length === 1
                ? {
                    messages: await readMessages(),
                    cursor: null,
                    hasMore: false,
                  }
                : cursor === null
                  ? {
                      messages: [
                        ...(await readMessages()),
                        {
                          id: "continuation-input",
                          role: "system",
                          messageIndex: 6,
                        },
                      ],
                      cursor: 6,
                      hasMore: true,
                    }
                  : {
                      messages: [
                        {
                          id: "continuation",
                          role: "assistant",
                          taskId: "task",
                          messageIndex: 7,
                        },
                        {
                          id: "other-task-input",
                          role: "system",
                          messageIndex: 8,
                        },
                        {
                          id: "unrelated",
                          role: "assistant",
                          taskId: "other-task",
                          messageIndex: 9,
                        },
                      ],
                      cursor: null,
                      hasMore: false,
                    },
          },
        });
      },
    );
    await page.goto(`${APP_BASE_URL}/projects`);
    await appendLater(page, reply, 1_500);
    await expectAssistantReply(page, TURN, reply, {
      timeout: 10_000,
      readTaskStatus: async () => "running",
    });
    expect(cursors[0]).toBeNull();
    expect(cursors).toContain("6");
    expect(await page.locator("#latest").textContent()).toContain(reply);
  });

  test("does not consume a later human input's continuation in the same task", async ({
    page,
  }) => {
    await page.setContent(
      '<div data-testid="assistant-message" data-message-id="current">working</div>' +
        '<div data-testid="assistant-message" data-message-id="later">done</div>',
    );
    const error = await expectAssistantReply(page, TURN, "done", {
      timeout: 2_000,
      readTaskStatus: async () => "running",
      readMessages: async () => [
        ...(await readMessages()),
        { id: "next-human", role: "user", messageIndex: 6 },
        {
          id: "next-reply",
          role: "assistant",
          taskId: "task",
          messageIndex: 7,
        },
        { id: "continuation-input", role: "system", messageIndex: 8 },
        { id: "later", role: "assistant", taskId: "task", messageIndex: 9 },
      ],
    }).then(
      () => undefined,
      (failure: Error) => failure,
    );
    expect(error?.message).toContain("toContainText");
  });

  test("returns once the reply renders in the captured message", async ({
    page,
  }) => {
    await render(page, "working");
    await appendLater(page, "<p>Fake workspace write completed</p>", 1_500);
    await expectAssistantReply(page, TURN, "Fake workspace write completed", {
      timeout: 20_000,
      readMessages,
      readTaskStatus: async () => "running",
    });
  });

  for (const pending of ["not rendered", "working"]) {
    test(`ignores another turn's identical reply while the captured turn is ${pending}`, async ({
      page,
    }) => {
      const reply = "Fake workspace Save completed: generation 2";
      await page.setContent(
        `<div data-testid="assistant-message" data-message-id="previous">${reply}</div>` +
          (pending === "working"
            ? '<div data-testid="assistant-message" data-message-id="current" id="latest">working</div>'
            : ""),
      );
      await page.evaluate(
        ({ pending, reply }) => {
          setTimeout(() => {
            if (pending === "not rendered") {
              document.body.insertAdjacentHTML(
                "beforeend",
                '<div data-testid="assistant-message" data-message-id="current" id="latest">working</div>',
              );
            }
            document.body.insertAdjacentHTML(
              "beforeend",
              `<div data-testid="assistant-message" data-message-id="another">${reply}</div>`,
            );
          }, 250);
          setTimeout(() => {
            document.getElementById("latest")!.textContent = reply;
          }, 1_500);
        },
        { pending, reply },
      );
      await expectAssistantReply(page, TURN, reply, {
        timeout: 10_000,
        readMessages,
        readTaskStatus: async () => "running",
      });
      expect(await page.locator("#latest").textContent()).toBe(reply);
    });
  }

  for (const status of ["failed", "cancelled"]) {
    test(`fails at once when the turn is ${status}`, async ({ page }) => {
      await render(page, "working");
      await appendLater(page, ERROR_BLOCK, 500);
      const started = Date.now();
      const error = await expectAssistantReply(
        page,
        TURN,
        "Fake workspace write completed",
        { timeout: 60_000, readMessages, readTaskStatus: async () => status },
      ).then(
        () => undefined,
        (failure: Error) => failure,
      );
      expect(error?.message).toContain(`The provider turn ${status}`);
      expect(error?.message).toContain("Claude Code exited before completing");
      expect(Date.now() - started).toBeLessThan(10_000);
    });
  }

  test("keeps waiting on an error status while the task is active", async ({
    page,
  }) => {
    await render(page, `working${ERROR_BLOCK}`);
    await appendLater(page, "<p>Fake workspace write completed</p>", 2_500);
    let reads = 0;
    await expectAssistantReply(page, TURN, "Fake workspace write completed", {
      timeout: 20_000,
      readMessages,
      readTaskStatus: async () => {
        reads += 1;
        return "running";
      },
    });
    expect(reads).toBeGreaterThan(0);
  });

  test("an error in an earlier message does not end the wait", async ({
    page,
  }) => {
    await page.setContent(
      `<div data-testid="assistant-message">${ERROR_BLOCK}</div><div data-testid="assistant-message" data-message-id="current" id="latest">working</div>`,
    );
    await appendLater(page, "<p>Fake workspace write completed</p>", 1_500);
    await expectAssistantReply(page, TURN, "Fake workspace write completed", {
      timeout: 20_000,
      readMessages,
      readTaskStatus: async () => "failed",
    });
  });

  test("without a reply or a failure it ends with the ordinary assertion", async ({
    page,
  }) => {
    await render(page, "working");
    const error = await expectAssistantReply(
      page,
      TURN,
      "Fake workspace write completed",
      { timeout: 2_000, readMessages, readTaskStatus: async () => undefined },
    ).then(
      () => undefined,
      (failure: Error) => failure,
    );
    expect(error?.message).toContain("toContainText");
  });
});
