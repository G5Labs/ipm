// =============================================================================
// IdentityAvatar — resolution order and deterministic fallback
// =============================================================================
// Governing intent: @ref software_architecture/client/shared-components#D39.
// The contract worth pinning is the ORDER (explicit url > member lookup >
// deterministic initial) and the determinism of the fallback: an actor that
// changes colour between renders or surfaces stops reading as an identity.
// =============================================================================

import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";

import { IdentityAvatar } from "./IdentityAvatar";
import type { OrgMemberDTO } from "../../lib/types";

const MEMBER: OrgMemberDTO = {
  userId: "019fd9ab-3f99-79f2-948d-7f3ed4fe3f6e",
  email: "kaihuiz@g5labs.ai",
  displayName: "Kaihui Z",
  avatarUrl: "https://avatars.example/u/298389204",
  role: "admin",
  deactivatedAt: null,
  activityRevision: 0,
  shutdownPending: false,
  joinedAt: "2026-01-01T00:00:00Z",
};

describe("IdentityAvatar", () => {
  it("prefers an explicit avatarUrl over the member roster", () => {
    render(
      <IdentityAvatar
        identity={MEMBER.email}
        avatarUrl="https://avatars.example/explicit.png"
        members={[MEMBER]}
      />,
    );
    expect(screen.getByTestId("identity-avatar")).toHaveAttribute(
      "src",
      "https://avatars.example/explicit.png",
    );
  });

  it("resolves an email to the member avatar, case-insensitively", () => {
    render(<IdentityAvatar identity="KaiHuiZ@G5Labs.ai" members={[MEMBER]} />);
    expect(screen.getByTestId("identity-avatar")).toHaveAttribute(
      "src",
      MEMBER.avatarUrl,
    );
  });

  it("falls back to an initial when the identity resolves to nothing", () => {
    // A raw user id — the case the workspace Owner row previously rendered as
    // a bare string with no avatar at all.
    render(<IdentityAvatar identity="019fd9ab-3f99-79f2" members={[MEMBER]} />);
    const avatar = screen.getByTestId("identity-avatar");
    expect(avatar).toHaveTextContent("0");
    expect(avatar).toHaveAttribute("title", "019fd9ab-3f99-79f2");
  });

  it("skips leading punctuation when picking the initial, and never renders empty", () => {
    render(<IdentityAvatar identity="@@@" />);
    expect(screen.getByTestId("identity-avatar")).toHaveTextContent("?");
  });

  it("gives the same identity the same tint, and different identities different tints", () => {
    const { unmount } = render(<IdentityAvatar identity="agent@g5.dev" />);
    const first = screen.getByTestId("identity-avatar").style.color;
    unmount();

    render(<IdentityAvatar identity="agent@g5.dev" />);
    expect(screen.getByTestId("identity-avatar").style.color).toBe(first);

    // Determinism is only useful if it also DISCRIMINATES: a hash that mapped
    // every actor to one tint would satisfy the stability check above.
    const tints = new Set<string>();
    for (const who of ["a@x", "b@x", "c@x", "d@x", "e@x", "f@x", "g@x"]) {
      const view = render(<IdentityAvatar identity={who} />);
      tints.add(screen.getAllByTestId("identity-avatar").at(-1)!.style.color);
      view.unmount();
    }
    expect(tints.size).toBeGreaterThan(1);
  });

  it("is decorative: the adjacent text already names the actor", () => {
    render(<IdentityAvatar identity={MEMBER.email} members={[MEMBER]} />);
    expect(screen.getByTestId("identity-avatar")).toHaveAttribute(
      "aria-hidden",
      "true",
    );
  });
});
