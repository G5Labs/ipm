# Activity order, current work and the Activity rail panel — request and interpretation

## User instructions (2026-10-20)

- Observed that Activity is sorted ascending (oldest first) on QA and asked for web
  research on "UX guidelines on this whether it should be asc or desc". The research
  (design systems, GitHub's 2025 feed reversal, pattern libraries) favoured newest first
  for activity logs, oldest first only for threaded discussion read as a conversation, and
  sorting conversation lists by last activity rather than creation time.
- "change the activity view to sort descending by last activity"
- "add an 'activity' for ongoing chats/tasks that are ongoing, so mark them as 'current'
  and keep them always at the top"
- "make the activity button on the side an actual panel like the others, showing only the
  most important events (comments, approvals, pull requests and guided steps), with a
  button/link on top to the full activity page"

## Agent interpretation

- "Last activity" is the record's latest durable time: a conversation's last message
  (`chat_sessions.updated_at`, bumped on every turn), a task's end else its start, and
  the time each other kind already carries. The whole record, issue discussion included,
  reads newest first; the comment composer moves above the record so a posted comment
  lands beside it.
- "Ongoing" is a conversation with a queued, running or waiting agent task, or a preview
  or command task in one of those states — the same predicate the Guided panel's
  Running state and the active-only task list use. Current is a live group served by its
  own read and refreshed while shown; it is not a feed kind, so the filter row does not
  hide it, and an entry shows once: in Current while ongoing, in the record afterwards.
- "A panel like the others" is a collapsible sidebar section that starts collapsed like
  every section except Guided, opens in the rail overlay like the others, and keeps the
  rail's collapse control on its header. Its body is "Open full activity" on top, the
  Current group, then the latest comments, approvals, pull requests and guided steps
  from the record's first loaded page, capped at eight rows. Showing Current in the rail
  is an interpretation of "always at the top"; the user listed only the four event kinds.
- The timeline remains the landing page and the complete record; nothing is persisted
  for Current.
