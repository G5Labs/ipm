import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { describe, expect, it, vi } from "vitest";
import { renderApp } from "../../test/render";
import { server } from "../../test/msw/server";
import { GithubConnectionCard } from "./GithubConnectionCard";

describe("shared GitHub connection", () => {
  it("connects once through account settings and preserves the intended destination", async () => {
    let body: unknown;
    server.use(
      http.get("/api/v1/github-app/user-connection", () =>
        HttpResponse.json({ data: { connected: false, githubLogin: null } }),
      ),
      http.post("/api/v1/github-app/user-connection", async ({ request }) => {
        body = await request.json();
        return HttpResponse.json({
          data: {
            redirectUrl: "https://github.com/login/oauth/authorize?state=test",
          },
        });
      }),
    );
    const redirect = vi.fn();
    renderApp(
      <GithubConnectionCard
        afterLogin
        returnTo="/acme/work?view=activity"
        redirect={redirect}
      />,
    );
    expect(
      screen.getByRole("link", { name: "Continue without connecting" }),
    ).toHaveAttribute("href", "/acme/work?view=activity");
    await userEvent.click(
      await screen.findByRole("button", { name: "Connect GitHub" }),
    );
    await waitFor(() =>
      expect(body).toEqual({
        returnPath:
          "/account/github?returnTo=%2Facme%2Fwork%3Fview%3Dactivity&afterLogin=1",
      }),
    );
    expect(redirect).toHaveBeenCalledWith(
      "https://github.com/login/oauth/authorize?state=test",
    );
  });

  it("shows the account and disconnects the shared grant", async () => {
    let disconnected = false;
    server.use(
      http.get("/api/v1/github-app/user-connection", () =>
        HttpResponse.json({
          data: {
            connected: !disconnected,
            githubLogin: disconnected ? null : "octocat",
          },
        }),
      ),
      http.post("/api/v1/github-app/user-connection/disconnect", () => {
        disconnected = true;
        return HttpResponse.json({
          data: { connected: false, githubLogin: null },
        });
      }),
    );
    renderApp(<GithubConnectionCard returnTo="/acme/work" />);
    expect(await screen.findByText("Connected as octocat")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Reconnect GitHub" }),
    ).toBeInTheDocument();
    await userEvent.click(
      screen.getByRole("button", { name: "Disconnect GitHub" }),
    );
    expect(
      await screen.findByRole("button", { name: "Connect GitHub" }),
    ).toBeInTheDocument();
    expect(disconnected).toBe(true);
  });

  it("keeps sign-in usable when the connection request fails", async () => {
    server.use(
      http.get("/api/v1/github-app/user-connection", () =>
        HttpResponse.json({ data: { connected: false, githubLogin: null } }),
      ),
      http.post("/api/v1/github-app/user-connection", () =>
        HttpResponse.json(
          {
            error: {
              code: "SERVICE_UNAVAILABLE",
              message: "GitHub is temporarily unavailable",
            },
          },
          { status: 503 },
        ),
      ),
    );
    const redirect = vi.fn();
    renderApp(
      <GithubConnectionCard
        afterLogin
        returnTo="/acme/work"
        redirect={redirect}
      />,
    );
    await userEvent.click(
      await screen.findByRole("button", { name: "Connect GitHub" }),
    );
    expect(await screen.findByRole("alert")).toBeInTheDocument();
    expect(
      screen.getByRole("link", { name: "Continue without connecting" }),
    ).toHaveAttribute("href", "/acme/work");
    expect(redirect).not.toHaveBeenCalled();
  });

  it("displays callback failure even when an older grant is stored", async () => {
    server.use(
      http.get("/api/v1/github-app/user-connection", () =>
        HttpResponse.json({
          data: { connected: true, githubLogin: "old-account" },
        }),
      ),
    );
    renderApp(
      <GithubConnectionCard
        afterLogin
        connectionFailed
        returnTo="/acme/work"
      />,
    );
    expect(
      await screen.findByRole("button", { name: "Reconnect GitHub" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("alert")).toHaveTextContent(
      "GitHub could not be connected",
    );
  });
});
