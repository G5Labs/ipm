import { describe, expect, it } from "vitest";

import { invitationEmailLabel } from "./invitationEmail";
import type { InvitationEmailDeliveryDTO } from "./types";

const delivery = (
  status: InvitationEmailDeliveryDTO["status"],
  outcomeUncertain = false,
): InvitationEmailDeliveryDTO => ({
  status,
  outcomeUncertain,
  updatedAt: "2026-10-17T00:00:00Z",
});

describe("invitationEmailLabel", () => {
  it("shows nothing when no email was requested", () => {
    expect(invitationEmailLabel(undefined)).toBeNull();
  });

  it("names each delivery state", () => {
    expect(invitationEmailLabel(delivery("queued"))?.text).toBe("Email queued");
    expect(invitationEmailLabel(delivery("sending"))?.text).toBe(
      "Email queued",
    );
    expect(invitationEmailLabel(delivery("sent"))).toEqual({
      text: "Email sent",
      tone: "ok",
    });
    expect(invitationEmailLabel(delivery("cancelled"))?.text).toBe(
      "Email not sent",
    );
  });

  it("separates a definite failure from a possibly sent message", () => {
    expect(invitationEmailLabel(delivery("failed"))).toEqual({
      text: "Email failed",
      tone: "warn",
    });
    expect(invitationEmailLabel(delivery("failed", true))).toEqual({
      text: "Email may not have been delivered",
      tone: "warn",
    });
  });
});
