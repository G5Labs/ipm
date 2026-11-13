import { afterEach, describe, expect, it, vi } from "vitest";

import {
  CHAT_PROMPT_HISTORY_PREFIX,
  MAX_CHAT_PROMPT_HISTORY,
  MAX_CHAT_PROMPT_HISTORY_CHARS,
  MAX_CHAT_PROMPT_HISTORY_CHATS,
  MAX_CHAT_PROMPT_HISTORY_TOTAL_CHARS,
  chatPromptHistoryKey,
  clearChatPromptHistory,
  readChatPromptHistory,
  recordChatPromptHistory,
  resetChatPromptHistoryForTests,
  stepPromptHistory,
  withRecordedPrompt,
} from "./chatPromptHistory";

afterEach(() => {
  vi.restoreAllMocks();
  localStorage.clear();
  resetChatPromptHistoryForTests();
});

describe("chat prompt history storage (g5.chat.ui.prompt-history.store)", () => {
  it("records newest first per person, workspace and chat", () => {
    const mine = chatPromptHistoryKey("user-1", "ws-1", "chat-a");
    recordChatPromptHistory(mine, "first");
    recordChatPromptHistory(mine, "second");
    recordChatPromptHistory(
      chatPromptHistoryKey("user-1", "ws-1", "chat-b"),
      "b1",
    );
    recordChatPromptHistory(
      chatPromptHistoryKey("user-2", "ws-1", "chat-a"),
      "theirs",
    );
    recordChatPromptHistory(
      chatPromptHistoryKey("user-1", "ws-2", "chat-a"),
      "other",
    );
    expect(readChatPromptHistory(mine)).toEqual(["second", "first"]);
    expect(
      readChatPromptHistory(chatPromptHistoryKey("user-1", "ws-1", "chat-b")),
    ).toEqual(["b1"]);
    expect(
      readChatPromptHistory(chatPromptHistoryKey("user-1", "ws-1", "chat-new")),
    ).toEqual([]);
  });

  it("moves a repeated prompt to the front instead of repeating it", () => {
    expect(withRecordedPrompt(["b", "a"], "a")).toEqual(["a", "b"]);
    expect(withRecordedPrompt(["a"], "a")).toEqual(["a"]);
  });

  it("caps entries and skips empty or oversized prompts", () => {
    let entries: string[] = [];
    for (let index = 0; index < MAX_CHAT_PROMPT_HISTORY + 5; index += 1) {
      entries = withRecordedPrompt(entries, `prompt ${index}`);
    }
    expect(entries).toHaveLength(MAX_CHAT_PROMPT_HISTORY);
    expect(entries[0]).toBe(`prompt ${MAX_CHAT_PROMPT_HISTORY + 4}`);
    expect(withRecordedPrompt(["a"], "")).toEqual(["a"]);
    expect(
      withRecordedPrompt(["a"], "x".repeat(MAX_CHAT_PROMPT_HISTORY_CHARS + 1)),
    ).toEqual(["a"]);
  });

  it("reads malformed storage as empty and keeps only strings", () => {
    const key = chatPromptHistoryKey("user-1", "ws-1", "chat-a");
    localStorage.setItem(key, "{not json");
    expect(readChatPromptHistory(key)).toEqual([]);
    localStorage.setItem(key, JSON.stringify(["a bare list"]));
    expect(readChatPromptHistory(key)).toEqual([]);
    localStorage.setItem(
      key,
      JSON.stringify({ usedAt: 1, entries: ["ok", 3, null] }),
    );
    expect(readChatPromptHistory(key)).toEqual(["ok"]);
  });

  it("discards earlier workspace-wide buffers without seeding any chat", () => {
    const legacy = `${CHAT_PROMPT_HISTORY_PREFIX}user-1:ws-1`;
    localStorage.setItem(legacy, JSON.stringify(["mixed A", "mixed B"]));
    const chat = chatPromptHistoryKey("user-1", "ws-1", "chat-a");
    expect(readChatPromptHistory(chat)).toEqual([]);
    expect(localStorage.getItem(legacy)).toBeNull();
    recordChatPromptHistory(chat, "a1");
    expect(readChatPromptHistory(chat)).toEqual(["a1"]);
  });

  it("evicts the least recently recorded chats beyond the chat budget", () => {
    const now = vi.spyOn(Date, "now");
    for (let index = 0; index <= MAX_CHAT_PROMPT_HISTORY_CHATS; index += 1) {
      now.mockReturnValue(1_000 + index);
      recordChatPromptHistory(
        chatPromptHistoryKey("user-1", "ws-1", `chat-${index}`),
        `prompt ${index}`,
      );
    }
    expect(
      readChatPromptHistory(chatPromptHistoryKey("user-1", "ws-1", "chat-0")),
    ).toEqual([]);
    expect(
      readChatPromptHistory(chatPromptHistoryKey("user-1", "ws-1", "chat-1")),
    ).toEqual(["prompt 1"]);
    expect(
      readChatPromptHistory(
        chatPromptHistoryKey(
          "user-1",
          "ws-1",
          `chat-${MAX_CHAT_PROMPT_HISTORY_CHATS}`,
        ),
      ),
    ).toEqual([`prompt ${MAX_CHAT_PROMPT_HISTORY_CHATS}`]);
  });

  it("keeps the browser's history within the character budget", () => {
    const now = vi.spyOn(Date, "now");
    const prompt = (tag: string) =>
      `${tag}:${"x".repeat(MAX_CHAT_PROMPT_HISTORY_CHARS - 10)}`;
    const per = Math.floor(
      MAX_CHAT_PROMPT_HISTORY_TOTAL_CHARS / MAX_CHAT_PROMPT_HISTORY_CHARS,
    );
    for (let index = 0; index < per + 10; index += 1) {
      now.mockReturnValue(1_000 + index);
      recordChatPromptHistory(
        chatPromptHistoryKey("user-1", "ws-1", `chat-${index % 3}`),
        prompt(String(index)),
      );
    }
    let total = 0;
    for (let index = 0; index < localStorage.length; index += 1) {
      const key = localStorage.key(index)!;
      if (key.startsWith(CHAT_PROMPT_HISTORY_PREFIX)) {
        total += localStorage.getItem(key)!.length;
      }
    }
    expect(total).toBeLessThanOrEqual(MAX_CHAT_PROMPT_HISTORY_TOTAL_CHARS);
    const newest = readChatPromptHistory(
      chatPromptHistoryKey("user-1", "ws-1", `chat-${(per + 9) % 3}`),
    );
    expect(newest[0]).toBe(prompt(String(per + 9)));
  });

  it("keeps history for the page when browser storage refuses writes", () => {
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new DOMException("full", "QuotaExceededError");
    });
    const key = chatPromptHistoryKey("user-1", "ws-1", "chat-a");
    recordChatPromptHistory(key, "kept for the page");
    expect(readChatPromptHistory(key)).toEqual(["kept for the page"]);
    clearChatPromptHistory();
    expect(readChatPromptHistory(key)).toEqual([]);
  });

  it("recalls a prompt kept for the page over older stored history", () => {
    const key = chatPromptHistoryKey("user-1", "ws-1", "chat-a");
    recordChatPromptHistory(key, "stored");
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new DOMException("full", "QuotaExceededError");
    });
    recordChatPromptHistory(key, "accepted after the store filled");
    expect(readChatPromptHistory(key)).toEqual([
      "accepted after the store filled",
      "stored",
    ]);
  });

  it("gives an oversized prompt no chat slot", () => {
    const key = chatPromptHistoryKey("user-1", "ws-1", "chat-long");
    recordChatPromptHistory(key, "x".repeat(MAX_CHAT_PROMPT_HISTORY_CHARS + 1));
    expect(localStorage.getItem(key)).toBeNull();
    expect(readChatPromptHistory(key)).toEqual([]);
  });

  it("clears every person's history on sign-out and nothing else", () => {
    recordChatPromptHistory(chatPromptHistoryKey("user-1", "ws-1", "a"), "a");
    recordChatPromptHistory(chatPromptHistoryKey("user-2", "ws-2", "b"), "b");
    localStorage.setItem("g5-user-settings", "{}");
    clearChatPromptHistory();
    expect(localStorage.length).toBe(1);
    expect(localStorage.getItem("g5-user-settings")).toBe("{}");
  });
});

describe("stepPromptHistory", () => {
  const entries = ["newest", "middle", "oldest"];

  it("walks back to the oldest entry, then stops", () => {
    let step = stepPromptHistory(entries, null, "draft", "older");
    expect(step?.value).toBe("newest");
    step = stepPromptHistory(entries, step!.cursor, step!.value, "older");
    expect(step?.value).toBe("middle");
    step = stepPromptHistory(entries, step!.cursor, step!.value, "older");
    expect(step?.value).toBe("oldest");
    expect(
      stepPromptHistory(entries, step!.cursor, step!.value, "older"),
    ).toBeNull();
  });

  it("walks forward and restores the draft after the newest entry", () => {
    const first = stepPromptHistory(entries, null, "draft", "older")!;
    const second = stepPromptHistory(
      entries,
      first.cursor,
      first.value,
      "older",
    )!;
    const back = stepPromptHistory(
      entries,
      second.cursor,
      second.value,
      "newer",
    )!;
    expect(back.value).toBe("newest");
    const restored = stepPromptHistory(
      entries,
      back.cursor,
      back.value,
      "newer",
    )!;
    expect(restored).toEqual({ cursor: null, value: "draft" });
    expect(stepPromptHistory(entries, null, "draft", "newer")).toBeNull();
  });

  it("treats an edited recall as the new draft", () => {
    const recalled = stepPromptHistory(entries, null, "draft", "older")!;
    expect(
      stepPromptHistory(entries, recalled.cursor, "newest edited", "newer"),
    ).toBeNull();
    const again = stepPromptHistory(
      entries,
      recalled.cursor,
      "newest edited",
      "older",
    )!;
    expect(again.value).toBe("newest");
    expect(again.cursor?.draft).toBe("newest edited");
  });

  it("browses the snapshot taken when browsing began", () => {
    const first = stepPromptHistory(entries, null, "", "older")!;
    const next = stepPromptHistory(
      ["from another tab", ...entries],
      first.cursor,
      first.value,
      "older",
    );
    expect(next?.value).toBe("middle");
  });

  it("has nothing to recall from an empty history", () => {
    expect(stepPromptHistory([], null, "", "older")).toBeNull();
  });
});
