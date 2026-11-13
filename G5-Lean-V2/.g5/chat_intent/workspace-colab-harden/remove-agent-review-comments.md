# Remove agent review comments — 2026-10-24

The owner instructed:

> please remove the mechanism for agent-originated review comments completely !!! you MUST remove everything including doc, code and also the database tables. you must NOT touch `g5d issue comment and g5d pr comment`

The agent interprets this as removing the internal review handoff and review-round
tools, their launch configuration, the UI action that requests them, their dedicated
data and current documentation. Ordinary file/workspace comments, human review
suggestions and formal approval reviews are separate features.

The SQL review tables also hold human review suggestions. The agent asked whether
the owner wants those shared tables and human reviews removed too, or only agent
records and support. No broader removal has been authorized. The agent therefore
keeps the shared human-review tables and removes agent records, agent-only columns
and agent authorship support, within the explicit agent-comment scope.
