import type { InvitationEmailDeliveryDTO } from "./types";

export type InvitationEmailTone = "muted" | "ok" | "warn";

/**
 * The administrators' label for an invitation's latest email
 * (`g5.organization.invitation-email-status`), or `null` when no email was
 * requested (email disabled, or an invitation created before email existed).
 */
export function invitationEmailLabel(
  delivery: InvitationEmailDeliveryDTO | undefined,
): { text: string; tone: InvitationEmailTone } | null {
  if (!delivery) return null;
  switch (delivery.status) {
    case "queued":
    case "sending":
      return { text: "Email queued", tone: "muted" };
    case "sent":
      return { text: "Email sent", tone: "ok" };
    case "cancelled":
      return { text: "Email not sent", tone: "muted" };
    case "failed":
      return delivery.outcomeUncertain
        ? { text: "Email may not have been delivered", tone: "warn" }
        : { text: "Email failed", tone: "warn" };
  }
}
