import React from "react";
import { describe, it, expect } from "vitest";
import { screen, waitFor } from "@testing-library/react";

import { useAuth } from "../hooks/useAuth";
import { createMockUser } from "./fixtures";
import { renderApp } from "./render";

function AuthProbe() {
  const { user, scopes, isLoading } = useAuth();

  if (isLoading) {
    return <div>loading</div>;
  }

  return (
    <div>
      <span data-testid="auth-name">{user?.displayName ?? "none"}</span>
      <span data-testid="auth-scopes">{scopes.join(",")}</span>
    </div>
  );
}

describe("renderApp auth overrides", () => {
  it("applies the provided mock user to the auth context", async () => {
    renderApp(<AuthProbe />, {
      user: createMockUser({
        displayName: "Taylor Test",
        scopes: ["sessions:read"],
      }),
    });

    await waitFor(() => {
      expect(screen.getByTestId("auth-name")).toHaveTextContent("Taylor Test");
    });
    expect(screen.getByTestId("auth-scopes")).toHaveTextContent(
      "sessions:read",
    );
  });

  it("can render unauthenticated when user is null", async () => {
    renderApp(<AuthProbe />, { user: null });

    await waitFor(() => {
      expect(screen.getByTestId("auth-name")).toHaveTextContent("none");
    });
    expect(screen.getByTestId("auth-scopes")).toHaveTextContent("");
  });
});
