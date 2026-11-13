# Issue 229 implementation request

The user asked to read issue 229 and latest origin/main, implement in a separate
branch/worktree, validate the changed production boundary and affected tests before
workspace checks and Claude CLI review, limit review to six rounds, and open a PR
with successful required Cloud CI. The issue requires whole agent-tree cancellation,
bounded harvest, exit proof before capacity release and sibling isolation.

Agent interpretation: a private Linux subreaper can provide lifecycle ownership
without requiring guest cgroup delegation. This does not provide hostile-code isolation.
