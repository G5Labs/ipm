import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { GithubPatConnect } from "./GithubPatConnect";
import { ApiError } from "../../lib/api";

// Mock only api.post; keep ApiError real so error-mapping is exercised.
const postMock = vi.fn();
vi.mock("../../lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../lib/api")>();
  return {
    ...actual,
    api: {
      ...actual.api,
      post: (...args: unknown[]) => postMock(...args),
    },
  };
});

describe("GithubPatConnect", () => {
  beforeEach(() => {
    postMock.mockReset();
  });

  it("shows generation guidance with the fine-grained token link and required permissions", () => {
    render(<GithubPatConnect onConnected={vi.fn()} />);

    const link = screen.getByRole("link", {
      name: /generate token on github/i,
    });
    expect(link).toHaveAttribute(
      "href",
      "https://github.com/settings/personal-access-tokens/new",
    );
    expect(link).toHaveAttribute("target", "_blank");
    expect(screen.getByText(/Contents: Read and write/i)).toBeInTheDocument();
    expect(screen.getByText(/Metadata: Read/i)).toBeInTheDocument();
  });

  it("posts the trimmed token and calls onConnected on success", async () => {
    const user = userEvent.setup();
    const onConnected = vi.fn();
    postMock.mockResolvedValue({
      provider: "github",
      providerUsername: "octo-dev",
      scopes: [],
      connectedAt: "2026-06-07T00:00:00Z",
      returnTo: "",
    });

    render(<GithubPatConnect onConnected={onConnected} />);
    await user.type(
      screen.getByLabelText(/personal access token/i),
      "  github_pat_test  ",
    );
    await user.click(screen.getByRole("button", { name: /connect github/i }));

    await waitFor(() => expect(postMock).toHaveBeenCalledTimes(1));
    expect(postMock).toHaveBeenCalledWith("/api/v1/scm/github/connect", {
      token: "github_pat_test",
    });
    expect(onConnected).toHaveBeenCalledTimes(1);
  });

  it("does not report a failed connect when only the onConnected callback rejects", async () => {
    const user = userEvent.setup();
    const onConnected = vi.fn().mockRejectedValue(new Error("refresh failed"));
    postMock.mockResolvedValue({
      provider: "github",
      providerUsername: "octo-dev",
      scopes: [],
      connectedAt: "2026-06-07T00:00:00Z",
      returnTo: "",
    });

    render(<GithubPatConnect onConnected={onConnected} />);
    await user.type(
      screen.getByLabelText(/personal access token/i),
      "github_pat_test",
    );
    await user.click(screen.getByRole("button", { name: /connect github/i }));

    // The connect itself succeeded: the token field is cleared and no error
    // banner appears even though the parent's refresh callback failed — but
    // the failure is surfaced as a distinct non-error notice. Assert against
    // the committed DOM state inside waitFor rather than relying on timing.
    await waitFor(() => {
      expect(onConnected).toHaveBeenCalledTimes(1);
      expect(screen.getByLabelText(/personal access token/i)).toHaveValue("");
      expect(screen.queryByText(/failed to connect/i)).not.toBeInTheDocument();
    });
    expect(
      await screen.findByText(/connected, but refreshing/i),
    ).toBeInTheDocument();
  });

  it("surfaces the backend detail message when the token is rejected", async () => {
    const user = userEvent.setup();
    postMock.mockRejectedValue(
      new ApiError({
        code: "VALIDATION_ERROR",
        message: "Validation failed",
        status: 400,
        details: {
          code: "invalid_token",
          message: "GitHub rejected the token.",
        },
      }),
    );

    render(<GithubPatConnect onConnected={vi.fn()} />);
    await user.type(screen.getByLabelText(/personal access token/i), "bad");
    await user.click(screen.getByRole("button", { name: /connect github/i }));

    expect(
      await screen.findByText(/GitHub rejected the token/i),
    ).toBeInTheDocument();
  });
});
