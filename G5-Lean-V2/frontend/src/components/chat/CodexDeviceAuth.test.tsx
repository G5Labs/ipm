import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../../lib/api";
import { CodexDeviceAuth, type CodexAuthView } from "./CodexDeviceAuth";

const challenge: CodexAuthView = {
  taskId: "task",
  attempt: 1,
  phase: "waiting",
  deadlineUnixMs: Date.now() + 300_000,
  verificationUrl: "https://auth.openai.com/codex/device",
  userCode: "TEST-12345",
  message: null,
};

function setup(value: CodexAuthView | null, sessionId = "session") {
  const get = vi.spyOn(api, "get").mockResolvedValue(value);
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  const panel = () => (
    <QueryClientProvider client={client}>
      <CodexDeviceAuth sessionId={sessionId} />
    </QueryClientProvider>
  );
  const view = render(panel());
  return { client, get, panel, ...view };
}

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
  sessionStorage.clear();
  Reflect.deleteProperty(navigator, "clipboard");
});

describe("Codex device authorization", () => {
  async function advanceTime(milliseconds: number) {
    await act(async () => {
      await vi.advanceTimersByTimeAsync(milliseconds);
    });
  }

  it.each([null, "ready", "ended", "superseded"] as const)(
    "polls idle state %s every three seconds without stopping discovery",
    async (phase) => {
      vi.useFakeTimers();
      const { get } = setup(phase ? { ...challenge, phase } : null);
      await advanceTime(0);
      expect(get).toHaveBeenCalledTimes(1);
      await advanceTime(2_999);
      expect(get).toHaveBeenCalledTimes(1);
      await advanceTime(1);
      expect(get).toHaveBeenCalledTimes(2);
      await advanceTime(3_000);
      expect(get).toHaveBeenCalledTimes(3);
    },
  );

  it.each(["checking", "waiting"] as const)(
    "polls active state %s every second",
    async (phase) => {
      vi.useFakeTimers();
      const { get } = setup({ ...challenge, phase });
      await advanceTime(0);
      expect(get).toHaveBeenCalledTimes(1);
      await advanceTime(999);
      expect(get).toHaveBeenCalledTimes(1);
      await advanceTime(1);
      expect(get).toHaveBeenCalledTimes(2);
      await advanceTime(1_000);
      expect(get).toHaveBeenCalledTimes(3);
    },
  );

  it("discovers a new challenge from idle, clears success promptly, and discovers a retry", async () => {
    vi.useFakeTimers();
    const { get } = setup(null);
    await advanceTime(0);
    get.mockResolvedValue(challenge);

    // The owner panel is already mounted before the first message. An idle
    // response must not disable polling and lose that message's challenge.
    await advanceTime(3_001);
    expect(get).toHaveBeenCalledTimes(2);
    expect(screen.getByText("TEST-12345")).toBeVisible();

    get.mockResolvedValue({ ...challenge, phase: "ready" });
    await advanceTime(1_000);
    expect(get).toHaveBeenCalledTimes(3);
    expect(screen.queryByText("TEST-12345")).not.toBeInTheDocument();

    const retry = { ...challenge, taskId: "retry", userCode: "NEXT-67890" };
    get.mockResolvedValue(retry);
    await advanceTime(2_998);
    expect(get).toHaveBeenCalledTimes(3);
    expect(screen.queryByText("TEST-12345")).not.toBeInTheDocument();
    await advanceTime(2);
    expect(get).toHaveBeenCalledTimes(4);
    expect(screen.getByText("NEXT-67890")).toBeVisible();
    expect(screen.queryByText("TEST-12345")).not.toBeInTheDocument();
  });

  it("shows the verification link and code, then removes them on success", async () => {
    const { client } = setup(challenge);
    expect(await screen.findByText("TEST-12345")).toBeVisible();
    expect(screen.getByRole("link")).toHaveAttribute(
      "href",
      challenge.verificationUrl,
    );
    // A follow-up queues behind the prompt (g5.chat.login-before-prompt); the
    // card must not promise that it cancels the sign-in.
    expect(
      screen.getByText(/Messages you send meanwhile stay queued/),
    ).toBeVisible();
    expect(screen.queryByText(/cancels this attempt/)).not.toBeInTheDocument();
    act(() => client.setQueryData(["codex-device-auth", "session"], null));
    await waitFor(() =>
      expect(screen.queryByText("TEST-12345")).not.toBeInTheDocument(),
    );
  });

  it("hides expired codes even when the server has not delivered a terminal event", async () => {
    setup({ ...challenge, deadlineUnixMs: Date.now() - 1 });
    expect(await screen.findByText("Finishing Codex sign-in…")).toBeVisible();
    expect(screen.queryByText("TEST-12345")).not.toBeInTheDocument();
    expect(screen.queryByRole("link")).not.toBeInTheDocument();
  });

  it.each(["ended", "superseded"] as const)(
    "leaves the %s explanation in the transcript without a terminal auth card",
    async (phase) => {
      const { get } = setup({
        ...challenge,
        phase,
        userCode: null,
        verificationUrl: null,
        message:
          "Device-code authorization was interrupted. Send any message to retry.",
      });
      await waitFor(() => expect(get).toHaveBeenCalled());
      expect(screen.queryByTestId("codex-device-auth")).not.toBeInTheDocument();
      expect(screen.queryByRole("link")).not.toBeInTheDocument();
    },
  );

  it("settles a stale waiting card by attempt and continues discovering retries", async () => {
    vi.useFakeTimers();
    const { get, rerender, client } = setup(challenge);
    await advanceTime(0);
    expect(screen.getByText("TEST-12345")).toBeVisible();
    rerender(
      <QueryClientProvider client={client}>
        <CodexDeviceAuth
          sessionId="session"
          terminalFailures={[{ taskId: challenge.taskId, attempt: 1 }]}
        />
      </QueryClientProvider>,
    );
    expect(screen.queryByTestId("codex-device-auth")).not.toBeInTheDocument();
    get.mockResolvedValue({ ...challenge, attempt: 2, userCode: "NEXT-67890" });
    await advanceTime(1_000);
    expect(screen.getByText("NEXT-67890")).toBeVisible();
  });

  it("never links to an unexpected verification host", async () => {
    setup({ ...challenge, verificationUrl: "https://evil.example" });
    await screen.findByTestId("codex-device-auth");
    expect(screen.queryByRole("link")).not.toBeInTheDocument();
    expect(screen.queryByText("TEST-12345")).not.toBeInTheDocument();
  });

  it("hides a cached code when authorization or connectivity is lost", async () => {
    const { client } = setup(challenge);
    await screen.findByText("TEST-12345");
    vi.mocked(api.get).mockRejectedValue(new Error("Forbidden"));
    await act(async () => {
      await client.invalidateQueries({
        queryKey: ["codex-device-auth", "session"],
      });
    });
    expect(await screen.findByText(/Reconnecting/)).toBeVisible();
    expect(screen.queryByText("TEST-12345")).not.toBeInTheDocument();
    expect(screen.queryByRole("link")).not.toBeInTheDocument();
  });
});

describe("Codex device code copy (g5.chat.ui.device-code-copy)", () => {
  // Each case uses its own session: a challenge is auto-copied at most once
  // per tab, so sharing one would make cases depend on their order.
  let sessionSerial = 0;
  // jsdom reports an unfocused document; a person watching the chat has it
  // focused.
  beforeEach(() => {
    vi.spyOn(document, "hasFocus").mockReturnValue(true);
  });
  function nextSession() {
    sessionSerial += 1;
    return `copy-session-${sessionSerial}`;
  }

  function mockClipboard(writeText: (text: string) => Promise<void>) {
    const spy = vi.fn(writeText);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText: spy },
    });
    return spy;
  }

  it("copies exactly the newly shown code and briefly confirms it", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const writeText = mockClipboard(async () => {});
    setup(challenge, nextSession());
    expect(
      await screen.findByText(/Code copied to your clipboard/),
    ).toBeVisible();
    expect(writeText).toHaveBeenCalledTimes(1);
    expect(writeText).toHaveBeenCalledWith("TEST-12345");
    expect(screen.getByTestId("codex-device-auth-code")).toHaveTextContent(
      /^TEST-12345$/,
    );
    expect(screen.getByRole("link")).toHaveAttribute(
      "href",
      "https://auth.openai.com/codex/device",
    );
    expect(screen.getByTestId("codex-device-auth-copy")).toHaveTextContent(
      "Copied",
    );
    await act(async () => {
      await vi.advanceTimersByTimeAsync(3_900);
    });
    expect(screen.getByTestId("codex-device-auth-copy-status")).toBeVisible();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(100);
    });
    expect(
      screen.queryByTestId("codex-device-auth-copy-status"),
    ).not.toBeInTheDocument();
    expect(screen.getByTestId("codex-device-auth-copy")).toHaveTextContent(
      "Copy code",
    );
  });

  it("does not copy the same challenge again on refetch, remount or reload", async () => {
    const writeText = mockClipboard(async () => {});
    const sessionId = nextSession();
    const { client, panel, unmount } = setup(challenge, sessionId);
    await screen.findByText(/Code copied to your clipboard/);
    await act(async () => {
      await client.invalidateQueries({
        queryKey: ["codex-device-auth", sessionId],
      });
    });
    unmount();
    render(panel());
    expect(await screen.findByText("TEST-12345")).toBeVisible();
    await act(async () => {});
    expect(writeText).toHaveBeenCalledTimes(1);
    expect(
      screen.queryByTestId("codex-device-auth-copy-status"),
    ).not.toBeInTheDocument();
    // The tab-scoped record that survives a reload never holds the code.
    const stored = Object.keys(sessionStorage).map((key) => [
      key,
      sessionStorage.getItem(key),
    ]);
    expect(stored).toEqual([
      [`g5:codex-auth-auto-copied:${sessionId}:task:1`, "1"],
    ]);
  });

  it("reports a write still in flight to a remounted panel without copying twice", async () => {
    let finishAuto!: () => void;
    const writeText = mockClipboard(
      () =>
        new Promise<void>((resolve) => {
          finishAuto = resolve;
        }),
    );
    const { panel, unmount } = setup(challenge, nextSession());
    await screen.findByText("TEST-12345");
    await waitFor(() => expect(writeText).toHaveBeenCalledTimes(1));
    // Dock or Pop out remounts the panel before the clipboard answers.
    unmount();
    render(panel());
    await screen.findByText("TEST-12345");
    await act(async () => {
      finishAuto();
    });
    expect(
      await screen.findByText(/Code copied to your clipboard/),
    ).toBeVisible();
    expect(writeText).toHaveBeenCalledTimes(1);
  });

  it("keeps a manual copy's outcome when the automatic write settles later", async () => {
    let finishAuto!: () => void;
    const writeText = mockClipboard(
      () =>
        new Promise<void>((_resolve, reject) => {
          finishAuto = () => reject(new Error("late refusal"));
        }),
    );
    setup(challenge, nextSession());
    await screen.findByText("TEST-12345");
    await waitFor(() => expect(writeText).toHaveBeenCalledTimes(1));
    writeText.mockImplementation(async () => {});
    fireEvent.click(screen.getByTestId("codex-device-auth-copy"));
    expect(
      await screen.findByText("Code copied to your clipboard."),
    ).toBeVisible();
    await act(async () => {
      finishAuto();
    });
    expect(
      screen.getByTestId("codex-device-auth-copy-status"),
    ).toHaveTextContent("Code copied to your clipboard.");
  });

  it("copies a new attempt's code", async () => {
    const writeText = mockClipboard(async () => {});
    const { client } = setup(challenge, nextSession());
    await screen.findByText(/Code copied to your clipboard/);
    const retry = { ...challenge, attempt: 2, userCode: "NEXT-67890" };
    vi.mocked(api.get).mockResolvedValue(retry);
    await act(async () => {
      await client.invalidateQueries();
    });
    await waitFor(() => expect(writeText).toHaveBeenCalledTimes(2));
    expect(writeText).toHaveBeenLastCalledWith("NEXT-67890");
  });

  it("offers Copy code and keeps the code visible when copying fails", async () => {
    const writeText = mockClipboard(async () => {
      throw new DOMException("Denied", "NotAllowedError");
    });
    setup(challenge, nextSession());
    expect(
      await screen.findByText(/Couldn't copy the code automatically/),
    ).toBeVisible();
    expect(screen.getByText("TEST-12345")).toBeVisible();
    expect(screen.getByRole("link")).toBeVisible();

    writeText.mockImplementation(async () => {});
    fireEvent.click(screen.getByTestId("codex-device-auth-copy"));
    expect(
      await screen.findByText("Code copied to your clipboard."),
    ).toBeVisible();
    expect(writeText).toHaveBeenLastCalledWith("TEST-12345");
  });

  it("explains a failed manual copy", async () => {
    mockClipboard(async () => {
      throw new Error("blocked");
    });
    setup(challenge, nextSession());
    await screen.findByText(/Couldn't copy the code automatically/);
    fireEvent.click(screen.getByTestId("codex-device-auth-copy"));
    expect(
      await screen.findByText(/Select it and copy it manually/),
    ).toBeVisible();
  });

  it("keeps the code for manual copy without a Clipboard API", async () => {
    setup(challenge, nextSession());
    expect(
      await screen.findByText(/Couldn't copy the code automatically/),
    ).toBeVisible();
    expect(screen.getByText("TEST-12345")).toBeVisible();
    expect(screen.getByTestId("codex-device-auth-copy")).toBeVisible();
  });

  it("waits for an unfocused page to regain focus before copying", async () => {
    const writeText = mockClipboard(async () => {});
    const hasFocus = vi.mocked(document.hasFocus).mockReturnValue(false);
    setup(challenge, nextSession());
    await screen.findByText("TEST-12345");
    await act(async () => {});
    expect(writeText).not.toHaveBeenCalled();

    hasFocus.mockReturnValue(true);
    await act(async () => {
      window.dispatchEvent(new Event("focus"));
    });
    expect(
      await screen.findByText(/Code copied to your clipboard/),
    ).toBeVisible();
    await act(async () => {
      window.dispatchEvent(new Event("focus"));
    });
    expect(writeText).toHaveBeenCalledTimes(1);
  });

  it("never copies a code it does not show", async () => {
    const writeText = mockClipboard(async () => {});
    setup(
      { ...challenge, verificationUrl: "https://evil.example" },
      nextSession(),
    );
    await screen.findByTestId("codex-device-auth");
    setup({ ...challenge, deadlineUnixMs: Date.now() - 1 }, nextSession());
    await screen.findByText("Finishing Codex sign-in…");
    await act(async () => {});
    expect(writeText).not.toHaveBeenCalled();
  });
});
