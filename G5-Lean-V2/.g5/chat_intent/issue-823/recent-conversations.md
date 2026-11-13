# Recent conversations strip

Human instruction (2026-10-26, workspace chat): "yes, use option 2. rename by itself
should not move a chat to the top." Option 2 was: the center no-chat view's "Recent
conversations" strip takes the first three chats in the server's activity order and
shows the same activity time as the chat picker, instead of re-sorting by `updatedAt`
and showing `updatedAt` as a time. The same reply approved implementing the #823 design.

Interpretation: the issue's out-of-scope "landing-page changes" refers to the
organization landing page; this in-chat strip keeps its layout and only follows the
[activity order](../../ontology/features/chat.g5.md#g5.chat.activity.order).
