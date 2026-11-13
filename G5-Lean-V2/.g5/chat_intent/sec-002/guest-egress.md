# SEC-002 guest egress

Human instruction, 2026-10-09: fix SEC-002, SEC-003 and SEC-004 from the external G5-Lean review using build-to-ship. Use `/Users/zliu/Workspace/G5-Lean` as the starting checkout. Ship the three findings one at a time in separate PRs.

Agent interpretation for SEC-002: replace the accepted metadata/host-loopback exposure for Linux microVM networking with enforcement before guest execution, preserving required DNS, cache/API access and preview connections. Keep process mode and macOS's single-tenant development trust boundary explicit. This does not promise general private-network isolation or authorize changes to the other accepted MVP exposures.
