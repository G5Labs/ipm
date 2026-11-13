# Approvals follow-up

Human direction: use “Approvals Required to merge into <parent>”. Below the current requirements, keep requested or responded-to approvals no longer required by the parent in a Legacy approvals section that starts collapsed. Keep their per-approval timelines.

The human wants one clear place to enter comments and suggests moving the Activity page’s comment entry to Reviews on Approvals. They ask what an open review item means, whether it can be a doc/file comment, and how a published review can have no open items.

Agent interpretation: Reviews owns creation of general workspace comments; contextual doc/file comments and replies keep their existing content/thread owners. Activity navigates to the canonical entry. The Activity page also has a separate composer that publishes to the linked GitHub issue. The agent asked whether to put it under Issues & tasks or move both composers into Reviews; after allowing time for an answer, proceeded with the stated recommended split: general workspace comments in Reviews, GitHub issue comments in the full Workspace Issues section. This is an agent assumption and does not record a human answer.

Human preview feedback: open the exact retained agent report in a new read-only browser tab with rendered Markdown and raw source modes, reusing G5's Markdown displayer; remove Browse reviewed commits. The request composer should focus on new requests rather than show completed prior batches. Each approval timeline groups by reviewer and runs newest first. Put the definitions of open items, published reviews and workspace comments behind one contextual-help icon rather than showing explanatory paragraphs by default. The human also asks what the Commit action does.

Human follow-up: remove Commit from Approvals; this is not its surface. Audit E2E and value-loop coverage of the expected surfaces and use cases, add missing tests, integrate the latest main, and push/fix CI and confident in-scope CodeRabbit findings until green. Preserve the authorized branch history under the workspace instructions.
