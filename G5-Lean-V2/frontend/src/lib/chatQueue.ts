import type { ChatMessageDTO, SessionInputDTO } from "./types";

/** Local presentation fields never confer server mutation authority. */
export type ChatQueueInput = SessionInputDTO & {
  localSubmission?: "submitting" | "accepted";
  awaitingHistory?: boolean;
};

export function visibleChatQueue(
  inputs: ChatQueueInput[],
  messages: ChatMessageDTO[],
): ChatQueueInput[] {
  const messageIds = new Set(messages.map((message) => message.id));
  return inputs.filter(
    (input) => input.delivery !== "withdrawn" && !messageIds.has(input.id),
  );
}

/** Absence from /inputs is not delivery evidence: that read omits history. */
export function reconcileChatQueue(
  current: ChatQueueInput[],
  fetched: SessionInputDTO[],
): ChatQueueInput[] {
  const previous = new Map(current.map((input) => [input.id, input]));
  const fetchedIds = new Set(fetched.map((input) => input.id));
  const next: ChatQueueInput[] = fetched.map((input) => {
    const old = previous.get(input.id);
    if (
      old &&
      !old.localSubmission &&
      (old.deliveryGeneration > input.deliveryGeneration ||
        old.revision > input.revision ||
        old.delivery === "withdrawn")
    ) {
      return old;
    }
    return input;
  });
  for (const input of current) {
    if (!fetchedIds.has(input.id)) {
      next.push(
        input.localSubmission === "submitting" || input.delivery === "withdrawn"
          ? input
          : {
              ...input,
              awaitingHistory: true,
              editable: false,
              withdrawable: false,
            },
      );
    }
  }
  return next;
}
