# TECH_DEBT §38 — Batch Execution Plan

This plan organizes all **1,236 audit follow-up items** from `TECH_DEBT.md` §38
into **16 independent execution batches**, grouped by subsystem and ordered by priority.

> **Status note (2026-04-19)**: This remains the live batch tracker. Canonical
> item status lives in `TECH_DEBT.md`; checkmarks here are synced to match the
> fixes landed through this branch.

> **Coverage verification**: 1236/1236 items assigned (100%)

## Summary

| Batch | Name | Items | Priority |
|-------|------|------:|----------|
| B6 | Worker & Task Lifecycle | 179 | 80 |
| B9 | Draft, CRDT & Commit | 43 | 4 |
| B8 | Git & Branch Management | 120 | 13 |
| B7 | View Sync & Downlift | 107 | 5 |
| B11 | Invite-Only Visibility & Cross-Org | 19 | 2 |
| B12 | Auth, Tokens, Permissions & Tenant Isolation | 83 | 10 |
| B1 | Chat & Session UX | 140 | 22 |
| B3 | Workspace & Projects Page | 99 | 10 |
| B4 | Org Onboarding & Invitations | 43 | 7 |
| B5 | Escalation & HIL | 58 | 14 |
| B10 | Search, Reindex & Spotlight | 23 | 4 |
| B2 | Artifact Rendering | 19 | 6 |
| B14 | Agent Tools & Skills | 81 | 6 |
| B15 | g5d CLI | 31 | 1 |
| B16 | UI Polish, Comments, Notifications & Misc | 191 | 31 |
| | **TOTAL** | **1236** | |

## Priority Ordering

Execute in this order. Batches within the same priority tier are independent
and can run in parallel.

### P1 — Functional Correctness

- **B6** — Worker & Task Lifecycle (179 items)
- **B9** — Draft, CRDT & Commit (43 items)
- **B8** — Git & Branch Management (120 items)
- **B7** — View Sync & Downlift (107 items)

### P2 — Security

- **B11** — Invite-Only Visibility & Cross-Org (19 items)
- **B12** — Auth, Tokens, Permissions & Tenant Isolation (83 items)

### P3 — Product Workflows

- **B1** — Chat & Session UX (140 items)
- **B3** — Workspace & Projects Page (99 items)
- **B4** — Org Onboarding & Invitations (43 items)
- **B5** — Escalation & HIL (58 items)
- **B10** — Search, Reindex & Spotlight (23 items)

### P4 — Polish

- **B2** — Artifact Rendering (19 items)
- **B14** — Agent Tools & Skills (81 items)
- **B15** — g5d CLI (31 items)
- **B16** — UI Polish, Comments, Notifications & Misc (191 items)

## Cross-Batch Dependencies

Most batches are fully independent. Known light dependencies:

| Dependency | Reason |
|------------|--------|
| B3 ↔ B8 | Workspace page shows branch status from git |
| B5 → B6 | Escalation triggers depend on task lifecycle events |
| B7 → B9 | View sync operates on committed drafts |
| B12 → B11 | Auth tokens carry org/tenant scoping |

These are sequencing preferences, not hard blockers. Each batch can be
developed independently; integration testing catches cross-boundary issues.

---

## Batch Details

### B6 — Worker & Task Lifecycle (179 items)

| § | Title |
|---|-------|--------|
| 38.7 | Supervisor Low-Confidence Outcomes Never Reach Human Review Context |  |
| 38.17 | Chat Send Can Succeed Even When No Worker Will Process It | ✅ |
| 38.20 | Job Task Listing Fails Open On Metadata Enrichment Errors | ✅ |
| 38.30 | Task APIs Still Bypass Invite-Only Workspace Visibility |  |
| 38.31 | Job Rollback Can Report Success Without Reverting Workspace State | ✅ |
| 38.32 | Task Diff Endpoint Is Still A False-Success Stub | ✅ |
| 38.39 | Worker APIs Still Bypass Invite-Only Workspace Visibility |  |
| 38.47 | Worker Selection UI Is Clickable But Has No Effect |  |
| 38.52 | Internal Workspace Write Endpoints Are Not Bound To The Assigned Task Or Workspace | ✅ |
| 38.55 | Job And Task Panels Silently Truncate After The First Page |  |
| 38.57 | Public Job Creation Cannot Express `dependsOnJob: null` | ✅ |
| 38.58 | Job Hover Actions Do Not Match The Actual Lifecycle Semantics | ✅ |
| 38.60 | Execution Drawer Diff "Retry" Is A No-Op | ✅ |
| 38.68 | Job Task Lists Promise Dependency Order But Render Creation Order |  |
| 38.71 | Worker Inbox Connection Failure Downgrades Interactive Tasks To A No-Op Inbox |  |
| 38.77 | Job Merge And Rollback Still Advance Only One Repo In Multi-Repo Workspaces |  |
| 38.85 | Task Intervention Routes Mutate Status But Do Not Send Live Control Signals | ✅ |
| 38.86 | Task Panels Treat `blocked` And `failed` As Resumable Even Though The API Only Resumes `paused` | ✅ |
| 38.87 | `stop_generation` Marks Messages Stopped But Never Stops The Running Worker | ✅ |
| 38.91 | Worker Detail Route Can Still Read Foreign-Org Workers By Raw ID | ✅ |
| 38.92 | Workspace Worker Listing Still Ignores Invite-Only Membership | ✅ |
| 38.101 | Worker Port-Forward Hooks Target Routes That Do Not Exist | ✅ |
| 38.102 | Malformed Pending Tasks Can Poison Worker Polling Indefinitely | ✅ |
| 38.105 | WorkerHub Reverse Mapping Breaks Concurrent Chat Abort Delivery | ✅ |
| 38.127 | Failed Task-Branch Mirroring Can Still Mark The Job Completed At An Unreachable Commit | ✅ |
| 38.128 | Failed Result Reporting Lets Completed Work Be Requeued Or Failed By The Lease Reaper |  |
| 38.129 | Job Merge-Back Updates Workspace Branch State Even When The Git Merge Failed | ✅ |
| 38.146 | Worker Registration Token Metadata Is Still Readable To Ordinary Members And Viewers | ✅ |
| 38.168 | Job Pause Still Leaves Running Tasks Live Despite The Public Contract |  |
| 38.170 | Cancelling An Escalation Still Leaves A Blocked Task In A Non-Actionable Dead End |  |
| 38.171 | View Tasks Can Still Report `completed` After Swallowed Post-Task Hook Failures |  |
| 38.172 | Internal Worker Listing Still Enumerates Cross-Org Workers | ✅ |
| 38.170 | View Task Context Still Ignores Current `doc_node_links` And Keeps A Stale Single Scope Node |  |
| 38.193 | Intervene-Panel Message Injection Still Posts To A Nonexistent Task Endpoint | ✅ |
| 38.198 | Job Creation Still Inserts The Wrong DTO Into The Jobs Cache | ✅ |
| 38.202 | Task Pause UI Still Models `paused` Tasks As `blocked` | ✅ |
| 38.226 | Sync-Prep Guidance Still Tells Agents To Complete Missing-File Tasks Even Though Post-Task Hooks Treat Missing Files As Errors |  |
| 38.272 | Worker Task Assignment Still Depends On `repos`, `users`, And `user_scm_connections` From Scoped Worker Sessions |  |
| 38.280 | Draft Worker Badge Still Treats The First Human Editor As The Active Worker | ✅ |
| 38.285 | Workspace View Query Params Still Only Hydrate Inside Job Routes | ✅ |
| 38.286 | Bottom Drawer Task State Still Persists After Route Exit Or Workspace Switch | ✅ |
| 38.290 | `useCreateJob()` Still Treats The Create Response As A Full `JobSummary` And Poisons The Job List Cache | ✅ |
| 38.291 | Job And Task Action Hooks Still Assume `204`/`void` Even Though The Live API Returns Action Envelopes | ✅ |
| 38.304 | Worker Session-Message Persistence Failures Are Still Silently Dropped | ✅ |
| 38.307 | Job-Scope Escalations Still Open Into A Drawer That Cannot Show Them | ✅ |
| 38.308 | Failed Takeover Attachment Can Still Leave A Task Paused Without Entering Takeover Mode | ✅ |
| 38.310 | Changes Tab Still Keeps A Stale Selected File Across Task Switches | ✅ |
| 38.311 | Tab URL Sync Still Blows Away Task Routes And Query-State Hints On Ordinary Tab Clicks |  |
| 38.313 | Timeline Footer Still Displays The Worker ID As Though It Were The Session ID | ✅ |
| 38.314 | Timeline Duration Still Measures Drawer Open Time Instead Of Task Runtime | ✅ |
| 38.321 | Chat Response Timeouts Still Live Only In An In-Memory Detached Task | ✅ |
| 38.326 | Task Timeline Pagination Still Opens On The Oldest Entries And Labels Newer Pages As “Older” | ✅ |
| 38.330 | Worker Status Surfaces Still Fabricate CPU/Memory Utilization From Static Capacity Fields | ✅ |
| 38.331 | Bottom Drawer Still Shows A Workspace-Level “Connected Worker” Instead Of The Selected Task’s Worker | ✅ |
| 38.336 | Worker Badges Still Count Idle Workers As “Active” | ✅ |
| 38.337 | The Top-Bar “Connected Worker” Pill Still Picks An Arbitrary Idle Pool Worker | ✅ |
| 38.348 | Forked Task Sessions Still Pretend To Be Attached To The Original Task | ✅ |
| 38.350 | Forking An In-Flight Session Still Creates A Fake Streaming State With No Worker Behind It | ✅ |
| 38.352 | Public Task-Scoped Session Creation Still Violates Task-Session Invariants | ✅ |
| 38.357 | Selected Task Views Still Never Subscribe To `task:<id>` Channels | ✅ |
| 38.358 | Worker Status Surfaces Still Miss Live Cache Refresh And Reconnect Recovery | ✅ |
| 38.360 | Task Checkpoint Lists Still Never Refresh After New Checkpoints Are Created | ✅ |
| 38.361 | Ordinary Tasks Still Run With Ephemeral Session IDs Instead Of Persisted Task Sessions |  |
| 38.362 | Attached Task-Session Sends Still Spin Up Chat Jobs Instead Of Reaching The Original Task Agent |  |
| 38.366 | Public Task DTOs Still Hardcode `sessionId = null` | ✅ |
| 38.368 | Session Timeline Persistence Still Ignores Task-Scoped Sessions | ✅ |
| 38.369 | Timeline Fan-Out Still Falls Back To An Arbitrary Task Session | ✅ |
| 38.370 | Task And Session Timeline Pagination Still Uses Timestamp-Only Cursors | ✅ |
| 38.375 | User-Driven Task Completion Still Ignores `commitDraft` And `commitMessage` | ✅ |
| 38.376 | Manual Task Completion Still Leaves Job Counters And Terminality Stale | ✅ |
| 38.377 | Task Rollback Still Never Resets Branch State To The Chosen Checkpoint | ✅ |
| 38.378 | Primary Failed-Task Recovery Still Has No Live Client Entry Point | ✅ |
| 38.379 | Task Rollback Still Has No Live Client Entry Point Or Checkpoint Surface | ✅ |
| 38.380 | Right-Panel Chat Still Hardcodes Workspace Scope For Valid Task And Node Sessions | ✅ |
| 38.381 | Task-Scoped Session Creation Still Accepts Foreign Task IDs From Other Workspaces | ✅ |
| 38.382 | Attach-Session CRDT Metadata Still Uses Task IDs Even Though Drafts Are Branch-Scoped | ✅ |
| 38.393 | Client Execution Surfaces Still Lose `waiting` / `cancelled` / `rolled_back` Task States | ✅ |
| 38.394 | Side-Door Task State Mutations Still Skip `task.status` WebSocket Broadcasts | ✅ |
| 38.395 | Dependent And Checkpoint-Gated Task Status Changes Still Mutate Siblings Silently | ✅ |
| 38.396 | Task Rollback Still Has No Eligibility Or Lease Guard | ✅ |
| 38.389 | Worker-Token Creation Still Accepts Negative Expiry Durations | ✅ |
| 38.427 | Workspace `workerIdleTimeoutMin` Is Still A Dead UI Setting |  |
| 38.428 | Workspace `workerScalingIntervalSec` Is Persisted But Ignored By The Scaler |  |
| 38.433 | Task Drawer Still Ships A Removed `Open In IDE` Control | ✅ |
| 38.536 | Non-Worker Smart-HTTP Push Still Allows Non-Branch Refs The Product Never Models | ✅ |
| 38.559 | Worker Smart-HTTP Repo Auth Still Grants Sibling-Repo Access From Any Running Workspace Task |  |
| 38.563 | `g5d worker` Still Ignores Workspace Isolation Settings And Falls Back To Default Process/VM Parameters |  |
| 38.564 | `g5d worker` Still Drops Cache Configuration On The Floor | ✅ |
| 38.565 | `g5d worker` Still Does Not Auto-Narrow Registration To The Current Workspace |  |
| 38.568 | `g5d worker` Still Ignores The Shared Env-Only Task/Workspace Startup Contract |  |
| 38.571 | `g5d worker --workspace <UUID>` Still Runs Against Whatever Checkout `cwd` Resolves To | ✅ |
| 38.572 | The Per-Worker Credential-Encryption Handshake Is Still A Dead Stub |  |
| 38.573 | Worker Heartbeats Still Refresh And Even Revive Terminated Worker Rows | ✅ |
| 38.574 | Worker Registration Still Accepts `max_concurrent_tasks` Values That Overflow The Stored DB Type | ✅ |
| 38.575 | Workers Still Ignore The Server-Advertised Heartbeat Interval | ✅ |
| 38.578 | Worker Heartbeats Still Mark Multi-Task Workers `idle` After Cancelling Just One Task | ✅ |
| 38.579 | Worker Release-On-Completion Still Assumes One Current `workers.task_id` |  |
| 38.580 | Workspace Worker Listing Still Misattributes Multi-Task Workers Through A Singular `task_id` |  |
| 38.581 | Worker Shutdown Reporting Still Collapses Every Clean Exit Into `task_complete` | ✅ |
| 38.583 | `g5d worker` Still Ignores The Remaining Shared Env-Backed Runtime Knobs |  |
| 38.584 | `g5d worker --idle-timeout` Still Uses A Different Unit And Default Than The Shared Worker Contract | ✅ |
| 38.585 | g5d Worker Health Uptime Still Stays At Zero Because Startup Time Is Never Initialized | ✅ |
| 38.612 | Worker Pull Still Allows The Checkout To Change Under A Running Task | ✅ |
| 38.613 | Worker Health Still Collapses Multi-Task Execution To One Arbitrary `task_id` |  |
| 38.630 | `g5d` Manifest Fetch Still Falls Back To The Internal Worker Route Only On `401` | ✅ |
| 38.706 | Mounted Worker Status And File Reads Still Ignore Per-Task Workspaces | ✅ |
| 38.707 | Internal Worker File Reads Still Accept `?ref=` But Ignore It | ✅ |
| 38.708 | Worker Data-Channel Stream Routes Are Still Never Mounted |  |
| 38.720 | Mounted Worker Status Still Downgrades Git Probe Failures Into A False-Clean Response | ✅ |
| 38.721 | Mounted Worker Pull Still Treats Dirty-Check Failures As A Clean Workspace | ✅ |
| 38.722 | Mounted Worker Health And Status Still Disagree About Idle No-Runtime Workers | ✅ |
| 38.723 | Mounted Worker File Reads Still Reject Legitimate Filenames Containing `..` | ✅ |
| 38.724 | The Live `g5d worker` Path Still Never Starts The Mounted Worker API Server |  |
| 38.725 | Mounted Worker Routes Still Validate Against The Bootstrap `instance_id` Instead Of The Live Worker ID |  |
| 38.726 | Worker Registration Still Succeeds Even When The Server Cannot Mint Worker Session JWTs | ✅ |
| 38.728 | Connection Brokering Still Hardcodes Localhost And Cannot Represent Cloud Workers |  |
| 38.732 | Legacy Browser Worker Detail Routes Still Fabricate Runtime State On Every Request |  |
| 38.737 | Legacy Browser Worker-Management Mutation Routes Are Still Hardcoded Failure Stubs |  |
| 38.752 | Worker Stream Connections Still Never Deliver Browser Input To Stream Sources |  |
| 38.753 | Worker Stream Types Still Have No Live Source Registration Path |  |
| 38.759 | `/api/v1/auth/ws-ticket` Still Mints The Wrong JWT Shape For Worker Stream Auth |  |
| 38.801 | `supervisor_assist.search_codebase` Still Ignores Its `file_pattern` Contract | ✅ |
| 38.812 | `supervisor_assist.query_experiences` Still Never Enforces Its Category Enum | ✅ |
| 38.813 | `supervisor_assist.read_spec` Fallback Still Cannot Find Nested Specs By Leaf Name | ✅ |
| 38.815 | `supervisor_assist.query_experiences` Still Can Panic On Non-ASCII Preview Truncation | ✅ |
| 38.816 | `supervisor_assist.read_spec` Still Trusts Raw `spec_id` As A Filesystem Path | ✅ |
| 38.817 | `supervisor_assist.query_experiences` Still Lets Raw Category Paths Escape `.g5/experience` | ✅ |
| 38.818 | `supervisor_assist.search_codebase` Still Follows Symlinked Paths Outside The Workspace | ✅ |
| 38.823 | `supervisor_assist.query_experiences` Still Returns `_index.md` As A First-Class Match | ✅ |
| 38.824 | `supervisor_assist.search_codebase` Still Silently Truncates At 20 Results | ✅ |
| 38.825 | `supervisor_assist` Still Uses Shape-Shifting Success Payloads | ✅ |
| 38.838 | Public Task-Session Injection Still Ignores Its Declared `priority` Field |  |
| 38.840 | Public Chat `model` / `maxTokens` Overrides Still Never Reach Worker Execution | ✅ |
| 38.842 | Task Transition Requests Still Drop Their Explanation Metadata On The Floor | ✅ |
| 38.845 | Workspace File-Mutation Tools Still Bypass Their Declared Task Scope |  |
| 38.849 | Terminal Stream Still Exposes Every Active Session On The Worker |  |
| 38.896 | `get_spec_git_diff` Still Uses The Current Task's Sync Anchor For Arbitrary Specs | ✅ |
| 38.915 | `semantic_search` Still Uses A Search Index Workers Never Build Or Refresh |  |
| 38.942 | `retry_all_failed()` Still Cannot Clear Materialization Failure State |  |
| 38.973 | `CrdtPeer::disconnect()` Still Returns Before The Background Sync Task Has Actually Stopped | ✅ |
| 38.1008 | `supervisor_assist.read_spec` Still Ignores Legacy `.g5/specs` Fallbacks | ✅ |
| 38.1011 | `supervisor_assist.query_experiences` Still Treats Empty `uncertainty_pattern` As “Return Everything” | ✅ |
| 38.1012 | `supervisor_assist.search_codebase` Still Treats Empty `query` As “Return The First 20 Files” | ✅ |
| 38.1017 | Task Completion Still Persists The Sentinel `"no-changes"` As A Real Commit SHA | ✅ |
| 38.1023 | Supervisor Research Still Swallows Filesystem Failures As Clean “No Results” | ✅ |
| 38.1029 | Internal “Return To Human Review” Still Accepts `routing_decision = "to_supervisor"` | ✅ |
| 38.1031 | Task Escalation Creation Still Bypasses The Supervisor Router |  |
| 38.1056 | Escalation Creation Still Succeeds After Task/Job Blocking Becomes Impossible |  |
| 38.1063 | Job-Scoped Escalations Are Still Deduplicated Per Task Instead Of Per Job |  |
| 38.1067 | Supervisor-Routed Escalations Still Have No Live Dispatch Path |  |
| 38.1068 | Escalation Creation Still Emits Only Generic UI Events For Supervisor Work |  |
| 38.1071 | Resolved Escalation Context Still Has No Live Path Back Into Task Execution |  |
| 38.1075 | Job-Scoped Escalations Still Hardcode `routing_decision = "to_human"` |  |
| 38.1085 | Internal Escalation Endpoints Still Trust Same-Org Workers Instead Of Task Ownership | ✅ |
| 38.1090 | Workspace Drawer Still Inherits An Unrelated Escalation When No Task Is Selected | ✅ |
| 38.1092 | Return-To-Human-Review Still Duplicates Supervisor Evidence On Retries | ✅ |
| 38.1107 | Org Members Error “Retry” Still Hard-Reloads The Entire App | ✅ |
| 38.1135 | Add-Repo Still Returns Success Even When The Initial Uplift Job Was Never Created | ✅ |
| 38.1147 | Task-Scoped Session Creation Still Allows Missing `scopeRef` And Persists Taskless “Task” Sessions | ✅ |
| 38.1149 | Task Intervention Routes Still Mint Read-Scoped DB Sessions | ✅ |
| 38.1154 | Job Lifecycle Routes Still Mint Read-Scoped DB Sessions | ✅ |
| 38.1155 | Shared Internal API Client Still Omits The Worker-Safe Manifest Endpoint | ✅ |
| 38.1160 | Worker Token Creation Still Accepts Blank Or Whitespace-Only Labels | ✅ |
| 38.1161 | Worker Token Scope Metadata Still Allows Non-Workspace Tokens To Claim A Workspace They Do Not Enforce | ✅ |
| 38.1162 | Worker Registration Tokens Still Have No Usage Or Last-Used Telemetry |  |
| 38.1201 | User-Scoped Worker Registration Still Fails Open Against Current Access Rights | ✅ |
| 38.1202 | Worker Capability Parsing Still Broadens Malformed Input To Full Scheduler Access | ✅ |
| 38.1203 | Internal Worker Listing Still Drops Non-UUID Worker IDs | ✅ |
| 38.1204 | Worker Registration Still Lets `maxConcurrentTasks = 0` Fall Through To A Generic 500 | ✅ |
| 38.1209 | Internal Worker Lifecycle Routes Still Assume UUID Worker IDs | ✅ |
| 38.1210 | Internal Task Transition Still Lets Same-Org Workers Mutate Unowned Pending/Waiting Tasks | ✅ |
| 38.1211 | Internal Search Reindex Still Acts As An Org-Wide Worker Admin Verb | ✅ |
| 38.1213 | Org Worker Inventory Still Has No First-Party Client Surface |  |
| 38.1219 | Worker Registration Still Discards The Caller's Host Identity |  |
| 38.1258 | Create-View Still Can Hide Its Own Background Job |  |
| 38.1276 | View-Row Sync Buttons Still Allow Duplicate Sync Job Submission |  |
| 38.1281 | Jobs Sidebar Still Never Auto-Expands The Selected Task's Parent Job |  |
| 38.1285 | Query-Param Updates Still Flatten Task Deep Links When The Task Map Is Incomplete |  |
| 38.1289 | HIL Banner Task Review Still Switches Drawer Context Without Updating The Route | ✅ |

### B9 — Draft, CRDT & Commit (43 items)

| § | Title |
|---|-------|
| 38.195 | Draft Mutation Hooks Still Discard Structured Success Payloads While Tests Mock 204s |
| 38.244 | Draft Commit Still Emits `draft.commit` Before The Transaction Commits |
| 38.245 | Draft Discard/Behind Events Still Publish Before The Underlying Branch Transaction Commits |
| 38.249 | Draft Commit Still Reports Success Even Though Remote Push Is Explicitly Disabled |
| 38.257 | Draft Participants And SCM Token Lookup Still Depend On System Tables The Tenant Role Cannot Read |
| 38.258 | Commit-On-Flush Still Conflicts With The Explicit Draft Boundary |
| 38.265 | `GET /api/v1/branches/:branchId/drafts` Still Auto-Creates Active Drafts On Read Scope ✅ |
| 38.266 | `commit_all` Still Uses One Shared Transaction And Can Strand Failed Drafts In `committing` ✅ |
| 38.267 | Draft Changed-Files UI Still Cannot Represent Deletions Correctly |
| 38.271 | Draft List And Creation Flows Still Depend On `repos` From Tenant-Scoped Sessions |
| 38.277 | File Content Still Hardcodes `draftDirty = false`, So The DTO Lies About Live Draft State ✅ |
| 38.292 | Draft Mutation Hooks Still Discard The Live Commit/Discard/Commit-All Results And Their Tests Still Pin `204` ✅ |
| 38.356 | CRDT Connection Setup Still Hardcodes Same-Origin Host And Ticket Paths |
| 38.397 | Runtime-Config Editors Still Wipe Unsaved JSON / Secret Drafts On Routine State Refresh |
| 38.419 | Draft Status Bar Still Fails Open During Draft Load/Error States |
| 38.496 | Draft Status Bar Still Reports `syncing` As Green `Synced` |
| 38.517 | `g5d merge` Still Exits Successfully When Repos Hit Merge Conflicts |
| 38.537 | `g5d revert` Still Leaves CRDT Draft State Stale |
| 38.542 | CRDT Materialization Still Trusts Raw File Paths Outside The Workspace Root |
| 38.863 | Spec/View Read Tools Still Bypass Draft And CRDT State |
| 38.864 | Spec Write Tools Still Report Success After Local-Only Writes When CRDT Sync Fails |
| 38.865 | View Artifact Emitters Still Report Success After Local-Only Writes When CRDT Sync Fails |
| 38.913 | `git_pull` Still Rewrites The Checkout Without Any Draft Or Dirty-Tree Guard |
| 38.922 | Out-Of-Band CRDT Change Detection Still Follows Symlinked Paths Outside The Workspace |
| 38.923 | Out-Of-Band CRDT Sync Still Corrupts Non-UTF-8 File Content |
| 38.933 | Inbound CRDT Materialization Still Ignores Remote File Deletions |
| 38.934 | Inbound CRDT Materialization Still Bypasses Exclusion And Size-Tier Guards |
| 38.935 | Cold-Start CRDT Materialization Still Writes Only Dirty Files |
| 38.936 | The CRDT Sync Engine's Periodic Pull Fallback Is Still A Stub |
| 38.944 | Inbound CRDT Updates Can Still Be Dropped When The Materializer Queue Is Full |
| 38.945 | Initial And Reconnect CRDT Handshakes Still Succeed After Failed State Application |
| 38.950 | WebSocket Flush Failures Still Lose The Rest Of The Outbound CRDT Batch |
| 38.955 | Late `SYNC_STEP2` Updates Still Mutate The CRDT Document Without Materializing To Disk |
| 38.957 | CRDT Delete Materialization Failures Still Disappear As “Skipped” |
| 38.961 | `CrdtPeer::flush()` Still Returns Success Before Any Outbound Delta Is Actually Sent |
| 38.962 | `CrdtPeer::disconnect()` Still Races Its Own Flush And Can Drop Queued Deltas |
| 38.988 | CRDT Safe-Point Gating Still Does Nothing In The Live Inbound Path |
| 38.989 | Inbound CRDT Materialization Still Holds The Document Mutex Across Disk I/O |
| 38.991 | `CrdtPeer` Reconnect Still Consumes Its Inbound Materialization Channel On First Connect |
| 38.1000 | Tool/CRDT Path Identity Still Splits Equivalent File Spellings |
| 38.1179 | Draft Mutation Routes Still Mint Read-Scoped DB Sessions |
| 38.1257 | Chat Send Failures Still Drop The User’s Drafted Message |
| 38.1288 | Draft Branch Toasts Still Filter By Bare Branch Name Instead Of Branch Identity |

### B8 — Git & Branch Management (120 items)

| § | Title |
|---|-------|
| 38.3 | Branch-Head Advances Do Not Reliably Trigger Search Reindex | ✅ |
| 38.5 | Org Onboarding Still Contains Stubbed Repo Import + Fake Availability UX |
| 38.11 | Branch File APIs Bypass Invite-Only Workspace Visibility |
| 38.13 | Non-Cloned Workspace Merge Can Leave In-Workspace Base Branch Heads Stale |
| 38.16 | SCM Repo Import Leaks Cross-Org Existence And Reuses Foreign Repo Rows | ✅ |
| 38.28 | Repo-Backed Doc Reads Hide Storage Failures As Missing Content |
| 38.31 | Comments Are Not Repo-Scoped In Multi-Repo Workspaces |
| 38.40 | Add-Repo Still Bypasses Invite-Only Workspace Membership |
| 38.41 | Add-Repo Reports Success Even When Bootstrap Work Is Never Scheduled |
| 38.61 | Workspace Repo Chips Are Still Clickable No-Ops ✅ |
| 38.75 | Workspace Diff Base-Branch State Is Dead And Node Diffs Stay Pinned To `main` ✅ |
| 38.76 | Multi-Repo Regular Jobs Still Collapse Onto A Single Repo Branch |
| 38.89 | Blob Capability Minting Still Trusts Caller-Supplied Repo IDs |
| 38.104 | Shared-Checkout Fallback Can Leak Dirty State Into Later Tasks |
| 38.106 | One Broken SCM Provider Blocks Repo Import From Healthy Providers |
| 38.107 | Multi-Provider Repo Import Can Leave The Wizard In A Hidden Partial-Success State |
| 38.111 | SCM Repo Import Still Bypasses Write-Scope Enforcement |
| 38.120 | Org Creation Still Returns A Docs Repo `gitUrl` That Does Not Match The Bootstrapped Repo Path |
| 38.126 | Multi-Repo Bootstrap Silently Proceeds With Partial Or Empty Checkouts |
| 38.175 | Branch Git-Status Hook Still Targets A Route The API Does Not Mount |
| 38.179 | Workspace Search Still Collapses Same-Path Hits Across Branches And Repos, Then Opens Them Without Branch Context |
| 38.184 | File-Backed Doc Reads Still Ignore The Workspace's Docs Branch |
| 38.193 | File Browsing Still Ignores The Requested Branch And Reads Branchless Repo Storage ✅ |
| 38.194 | Search Reindex Still Ignores The Requested `commit_sha` And Indexes Branchless Content ✅ |
| 38.195 | Clone Workspace Still Reports Success Even When Search Bootstrap Is Never Scheduled |
| 38.209 | File-Content Fetches Still Splice Raw Repo Paths Into The URL |
| 38.215 | Org Creation And Workspace Clone Still Perform Local Git Side Effects Before The Database Commit |
| 38.217 | Clone-Workspace Still Accepts Blank-Looking Names At The API Boundary ✅ |
| 38.218 | Clone Dialog Can Be Dismissed Mid-Clone Even Though The Clone Keeps Running |
| 38.221 | Add-Repo Dialog Can Also Be Dismissed While Repo Attachment Continues |
| 38.229 | Clone Branch Overrides Still Cannot Represent Multiple Source Branches From The Same Repo |
| 38.230 | Clone Still Accepts Invalid Override Branch Names And Returns Success After Local Ref Materialization Fails |
| 38.232 | `CommitModal` Can Be Dismissed Mid-Commit While The Branch Still Advances |
| 38.250 | Public Comment Routes Still Strip Diff-Range Metadata |
| 38.262 | Repo Storage Still Cannot Express Branch- Or Commit-Scoped Reads Even Though Multiple Services Promise Them |
| 38.278 | Workspace Lifecycle Mutations Still Reach Back Into The Org Repo Catalog From Scoped Sessions |
| 38.279 | Add-Repo Still Tries To Write The Org Repo Catalog Through A Scoped Tenant Session | ✅ |
| 38.287 | Org Detail And Repo Listing Still Depend On `orgs` And `repos` From Tenant-Scoped Sessions |
| 38.293 | Removing A Workspace Branch Still Blindly Deletes Branch State And Can Strand Live Work |
| 38.294 | Workspace Merge Still Counts Base Branch Rows As Successful Merges |
| 38.346 | Workspace Branch Divergence Metrics Are Still Hardcoded To Zero Across Public APIs |
| 38.400 | Diff Line-Number DTOs Still Do Not Match The Live Client Field Names ✅ |
| 38.402 | Diff Comment Anchors Still Persist Flattened Row Indices Instead Of Real File Lines |
| 38.404 | Public Comment-Create Contract Still Cannot Express `originBranch` |
| 38.416 | Diff Inline Threads Still Collapse Separate Roots And Drop Real Reply History |
| 38.421 | Create-Org Coverage Still Does Not Assert The Persisted Docs Branch Head |
| 38.454 | `issue_test_token()` Stores The Wrong Docs Branch On Seeded Demo Workspaces |
| 38.455 | Normal Workspace Creation Still Wires A Docs Repo Without Creating The Docs Branch Row |
| 38.456 | Workspace Clones Keep The Source Docs Branch Name Even Though Branches Are Renamed |
| 38.466 | Workspace Branch Creation Still Accepts Foreign `repo_id` Values |
| 38.467 | Branch Removal Is Only Org-Scoped, Not Workspace-Scoped |
| 38.465 | Diff Artifacts Still Promise A Monaco Diff Viewer But Render As Plain Unified Text |
| 38.469 | Workspace Switcher Still Counts Branches As Repositories ✅ |
| 38.489 | Workspace Manifests Still Publish The Docs Repo Twice |
| 38.490 | Workspace Repo Materialization Still Collides On Duplicate Repo Names |
| 38.485 | Workspace Branch Names Are Still Globally Unique Per Repo Across All Workspaces |
| 38.486 | Optional Branch Names Still Collapse Multi-Branch Repo Flows Onto One Fixed Default |
| 38.487 | Invalid Branch `trackingMode` Values Still Bubble Out As 500s ✅ |
| 38.488 | Branch Creation Still Accepts Nonexistent Base Branches And Reports Success With A Zero Head |
| 38.492 | Workspace Branch Attachment Still Defaults To Literal `main` Instead Of The Repo’s Real Default Branch |
| 38.497 | `g5d fork` Still Copies Workspace Docs Metadata Without The Docs Checkout |
| 38.494 | Raw File Tabs Still Follow The Current Branch Instead Of The Branch They Were Opened From |
| 38.503 | `g5d commit` Still Returns Success After Repo Or Docs Commit Failures |
| 38.514 | Failed `g5d clone` Still Leaves A Poisoned Target Workspace Behind |
| 38.512 | `g5d refresh` Still Reports Success After Partial Repo Or Docs Refresh Failure |
| 38.513 | Local-Only `g5d refresh` Still Ignores The Docs Checkout |
| 38.515 | `g5d clone` Still Leaves A Poisoned Workspace Directory After Partial Materialization Failure |
| 38.516 | Local-Only `g5d refresh` Still Returns Success After Repo Fetch Failures |
| 38.519 | Smart-HTTP Git Push Still Treats Invite-Only `viewer` Membership As Write Access ✅ |
| 38.520 | Smart-HTTP Repo Access Still Collapses Workspace-Scoped Permissions Into Whole-Repo Access |
| 38.523 | Smart-HTTP Ref Deletions Still Leave Stale Workspace Branch State Behind |
| 38.527 | `g5d merge` Still Falls Back To Display-Name Repo Matching |
| 38.528 | `g5d add` Still Assumes Public-Repo Access |
| 38.529 | `g5d fork` Still Returns Success After Partial Repo Or Metadata Failures |
| 38.532 | `g5d merge` Still Ignores The Workspace Docs Checkout Entirely |
| 38.534 | `git-receive-pack` Still Does Not Implement The Advertised Ancestry Check |
| 38.540 | `g5d doctor` Still Ignores Docs Checkout Health |
| 38.543 | Manifest-Driven Checkout Paths Still Escape The Workspace Root |
| 38.546 | Docs Materialization Still Fabricates The Requested Branch From Whatever `HEAD` Points To |
| 38.547 | Explicit-Target `g5d push` Still Overwrites Local Branch Refs To Match The Destination |
| 38.555 | Full-URL `g5d clone` Still Resolves On One Host And Fetches From Another |
| 38.558 | `g5d doctor` Still Treats Any Existing Repo Path As Healthy |
| 38.561 | `g5d merge` Still Collapses Multiple Branches Of The Same Repo Onto One Merge Target |
| 38.562 | Remote-Source `g5d merge` Still Leaves Persistent Temp Remotes And Branches Behind |
| 38.566 | `g5d push` Still Falls Back To The Local Repo Remote When The Target Workspace Has No Matching Repo |
| 38.576 | `g5d add` Still Accepts Repo Paths That Escape The Workspace Root ✅ |
| 38.582 | `g5d add` Still Leaves Orphaned Checkouts Behind On Post-Clone Failure |
| 38.599 | `g5d refresh` Still Rewrites `workspace.yaml` After Partial Repo Or Docs Materialization Failures |
| 38.600 | Local-Only `g5d refresh` Still Ignores The Docs Checkout Entirely |
| 38.601 | `g5d refresh` Still Loses Local State When A Repo Moves Paths In The Manifest |
| 38.602 | `g5d refresh` Still Leaves Removed Docs Checkouts Behind As Unmanaged State |
| 38.608 | `g5d merge --dry-run` Still Creates Rebased Commit Objects While “Planning” |
| 38.609 | `g5d merge` Still Applies Successful Repo Results Non-Atomically |
| 38.700 | `g5d push` Still Silently Skips Missing Repo Checkouts |
| 38.702 | `g5d push` Still Falls Back To Repo-Name Matching When Repo IDs Are Unparseable |
| 38.710 | Workspace Materialization Still Deletes Any Preexisting Non-Repo Directory At A Checkout Path |
| 38.711 | API-Backed Docs Refresh Still Assumes An Existing `.g5/docs` Checkout Already Has `origin` |
| 38.729 | `g5d` Still Lets Manifest Checkout Paths Clobber The `.g5` Control Tree |
| 38.733 | `pin_ref()` Still Echoes The Requested Ref As The Active Branch Even For Detached Commit Pins |
| 38.739 | Legacy `get_ref()` Still Collapses Branch-Read Failures Into A Fake `"HEAD"` Response ✅ |
| 38.740 | `g5d fork` Still Copies Pinned Repo SHAs Without Re-Pinning The Forked Checkout |
| 38.741 | Legacy Git Ref Responses Still Flatten Upstream Probe Failures To Synthetic `0/0` State ✅ |
| 38.755 | Browser Tree Requests Can Still Bypass The “Always Exclude .git” Rule By Targeting `.git` As The Root |
| 38.835 | `git_push` Still Defaults To A Literal `HEAD` Branch Instead Of The Current Branch |
| 38.836 | Git Tools Still Pick The First SCM Credential Instead Of Matching The Remote URL |
| 38.854 | Workspace Chat Still Cannot Create Or Reopen Branch-Pinned Sessions Correctly |
| 38.855 | Chat Session Branch Pins Still Accept Unknown Values And Silently Fall Back To Another Branch |
| 38.1037 | Comment DTO Still Drops `branch_deleted`, So Preserved Deleted-Branch Threads Look Normal |
| 38.1049 | Cross-Branch Comment Discovery Still Papers Over Missing Anchors With Fake Defaults |
| 38.1053 | Cross-Branch Comment Discovery Still Has No Live Client Surface |
| 38.1120 | Add-Repo Still Trusts Arbitrary `gitUrl` Input And Can Persist Clone-Impossible Repo Rows |
| 38.1127 | Deep Workspace Clone Still Drops `workspace_dependencies` And Returns A False Empty Graph |
| 38.1128 | Add-Repo Still Trusts Caller-Supplied Repo Names Even Though Manifests Reuse Them As Checkout Paths |
| 38.1131 | Add-Repo Still Cannot Reuse An Existing Org Repo For A New Workspace |
| 38.1133 | Invite-Only Workspace Clone Still Drops Collaborator Access While Preserving `visibility = "invite_only"` |
| 38.1136 | Newly Added Repos Still Start With An All-Zero `head_commit` |
| 38.1144 | Add-Repo Still Accepts Blank `defaultBranch` Values And Persists Broken Repo Metadata ✅ |
| 38.1157 | Public Chat Session Create Still Trusts Raw `branch` Values And Silently Falls Back At Execution Time |
| 38.1249 | Workspace Branch Management Still Has No First-Party Client Surface |
| 38.1287 | WorkspacePage Still Collapses Multi-Repo Branch Context Onto One Global `activeBranchId` |

### B7 — View Sync & Downlift (107 items)

| § | Title |
|---|-------|
| 38.2 | Clone Sync/Merge Can Persist Placeholder Zero SHAs |
| 38.42 | File API Ontology Enrichment Is Still A Stub |
| 38.44 | Document Sync UI Only Uses The First Linked Node |
| 38.59 | The Sync Modal's "Sync" Choice Is Still A Downlift Placeholder |
| 38.63 | Permission Hooks Are Out Of Sync With The Permissions API |
| 38.69 | Scoped Uplift Silently Degrades When Ontology Upsert Mapping Is Missing |
| 38.70 | Cascading Downlift Treats Missing Target Specs As A Successful No-Op |
| 38.78 | "Full" Downlift Only Walks The First Ontology Root |
| 38.132 | Create View Still Persists Linked Doc IDs In The `linked_doc_paths` Field |
| 38.149 | File-Type Search Results Still Open As Fake Ontology Nodes |
| 38.151 | Future-View Handoff Still Drops The Chosen Persona And Filename |
| 38.154 | Scoped Uplift Still Falls Back To A Full-Workspace Analysis When The Scope Has No `file_paths` |
| 38.155 | View Sync Phase B Still Treats Missing `relevant_specs` As A Successful No-Op |
| 38.156 | Center-Panel Navigation Still Conflates File Paths, Doc IDs, And Ontology Node IDs |
| 38.176 | View-List Filters Still Do Nothing Because The Docs API Ignores Them |
| 38.169 | Dedicated View Sync Still Drops The View's Scope Before Bootstrap |
| 38.171 | Deleting A View Still Strips Live Jobs Of Their View Context |
| 38.1841 | Repo-Backed View Docs Still Have Split-Brain Content Sources |
| 38.185 | File-Backed Doc Relinks Still Get Reverted By Linked-Doc Reindexing |
| 38.186 | View Downlift Still Depends On `linked_nodes` Frontmatter That The View Authoring Flow No Longer Guarantees |
| 38.187 | Direct View Downlift Still Looks For View Files In The Wrong On-Disk Location |
| 38.188 | View Workflow Guidance Still Advertises `scope_mode` Values The Backend Rejects |
| 38.189 | Create-View Scope Still Behaves Like Decorative Metadata |
| 38.190 | Test-Persona View Creation Still Can Complete Without Writing The View File |
| 38.191 | Deleting A View Still Leaves Its Repo File Behind To Resurface Later |
| 38.192 | The View-Management Skill Still Teaches A Mapping-Based Workflow After `mapping.json` Was Removed |
| 38.199 | View Creation Still Serializes Linked Doc IDs Into `linkedDocPaths` |
| 38.203 | Internal View-Mapping Endpoints Still Ship As Mounted Dead Code After `mapping.json` Removal | ✅ |
| 38.204 | Future-View Creation Still Drops The Persona And Filename Chosen In `CreateProjectDocModal` |
| 38.207 | Queue-Driven Reindex Still Cannot Detect Deleted Files, Leaving Stale Search/Ontology State Behind |
| 38.208 | Search Reindex Queue Still Reports Success After Ontology Or Linked-Doc Sidecars Fail |
| 38.210 | View Sync Still Starts Even When The Backing View File Does Not Exist |
| 38.212 | Future Views Can Still Hit The Regular Sync Endpoint |
| 38.213 | View Review-Iteration Defaults Are Still Split Between 1-Pass UI And 2-Pass Backend Execution |
| 38.223 | Create-View Modal Can Be Closed Mid-Submit While The View Creation Still Lands |
| 38.224 | Live Sidebar Still Loses Future-View State Because `WorkspacePage` Uses Generic `/docs` Summaries |
| 38.225 | Shared Doc/View Detail Hooks Still Model `GET /docs/:docId` As A Summary DTO |
| 38.227 | View File-Path Naming Still Disagrees Across Browser Preview, Tooling, And Backend Canonical Storage |
| 38.238 | View Doc-Type Taxonomy Still Diverges Between Frontend Types And Backend Canonical Values |
| 38.240 | Sidebar Sync Status Dots Still Ignore Real Doc Staleness Metadata ✅ |
| 38.253 | Views Sidebar Still Treats All Non-Internal Docs As View Docs ✅ |
| 38.254 | Phase B Still Cannot Find The Default Numbered `## 4. Implementation Mapping` Heading |
| 38.255 | Duplicate View Frontmatter Or Mapping Entries Still Abort The Entire Phase B Submission |
| 38.256 | Existing-Spec View Sync Still Silently Drops Codegen When `source_files` Frontmatter Is Missing Or Malformed |
| 38.259 | Ontology Isolation Still Points At A Mutable Base-Table Flag Model That Conflicts With The Snapshot/Overlay Plan |
| 38.260 | Spec Storage Remediation Still Treats The Docs Repo As A Valid Fix Path Despite The Repo-Local Spec Design |
| 38.270 | Ontology Tree And Node Reads Still Depend On `repos` From Tenant-Scoped Sessions |
| 38.273 | Existing-Feature View Mappings Still Use A Table Shape Phase B Parses Against The Wrong Columns |
| 38.274 | Future-Feature View Templates Still Omit The File Paths Phase B Needs For New-Spec Codegen |
| 38.275 | View-Sync Codegen Still Collides When Two Specs Target The Same Source File |
| 38.281 | Phase B Still Mistakes Real Component IDs Containing `spec` Or `component` For Table Headers |
| 38.282 | Phase B Still Collapses Multi-File Mapping Cells Into One Bogus Source Path |
| 38.296 | File Tabs Still Use A Different Identity Than Ontology Code Tabs For The Same `/file/...` URL |
| 38.303 | Unreadable `semlift.yaml` Still Silently Re-Enables Fan-Out Sync And Codegen |
| 38.319 | The Generic `New View` Dialog Still Creates A Plain `design` Doc Instead Of A Real View |
| 38.332 | Scoped View Creation Is Still Unreachable From The Live Workspace UI |
| 38.333 | The Public API Still Has No Dedicated View List/Detail Contract |
| 38.354 | Sync Modal Still Has No In-Flight Guard, So Rapid Double-Clicks Can Queue Duplicate Sync Jobs |
| 38.386 | Client View Doc-Type Vocabulary Still Uses Stale `perf_view` / `ux_view` / `custom_view` Tokens ✅ |
| 38.401 | Ontology Diff Comments Still Route Through Node Comments Instead Of Line Comments |
| 38.409 | Unknown `/spec/*` Links Still Fail Open Instead Of Redirecting Or 404ing |
| 38.418 | Editor Diff Toggles Still Only Flip Button State And Never Open A Diff View |
| 38.422 | Spec Comments Still Anchor To Mutable Block Indices |
| 38.447 | Ontology Sidebar Search Still Drops Every Non-Root Node |
| 38.449 | Left Sidebar Ontology Search UI Is Fully Scaffolded But Unreachable |
| 38.462 | JSON Artifacts Still Promise Foldable `react-json-view` But Render As Plain Highlighted Text |
| 38.476 | Clone Sync/Merge Still Pair Branches Only By `repo_id` |
| 38.477 | Branch Management Still Does Not Keep Docs Branch Metadata In Sync |
| 38.493 | `create_workspace()` Still Seeds Docs Metadata Independently Of The Actual Docs Branch Spec |
| 38.509 | `g5d add` Still Cannot Sync Repo Attachments Back To The API |
| 38.521 | `git-receive-pack` Still Returns Success After Post-Push Workspace State Sync Fails |
| 38.522 | Post-Push Head Sync Still Advances Pinned Branches As Though They Were Tracking |
| 38.775 | `trigger_workflow` Still Lets Custom-Persona View Creation Miss The Required Label |
| 38.856 | Workspace Chat Session Switching Still Does Not Sync The URL |
| 38.885 | `apply_sync_to_spec` Still Only Returns Instructions Instead Of Applying Sync |
| 38.891 | Phase B Still Coerces Malformed `new_specs` / `relevant_specs` Frontmatter Into A Successful No-Op |
| 38.895 | `EmitSyncPlanTool` Still Ships As A Dead Phase-B Surface |
| 38.899 | `list_specs` Still Drops Legacy Specs Whenever `.semlift/specs` Exists |
| 38.927 | Terminal Out-Of-Band Filesystem Sync Is Still Completely Unwired |
| 38.956 | Inbound Sync Still Overwrites Files When Conflict Preservation Errors |
| 38.980 | The Sync Reconnect Loop Still Drops `FlushNow` / `PullNow` Controls Instead Of Replaying Them |
| 38.994 | Spec Frontmatter Parsing Still Treats The First Raw `---` As The Closing Delimiter |
| 38.996 | `list_specs` Still Treats Any Markdown File Under The Specs Tree As A Real Spec |
| 38.1005 | Missing-Target File/Spec Mutations Still Escalate As Hard Tool Failures |
| 38.1006 | Prompted “Exact Frontmatter” Still Uses `src_files` Where Phase B Reads `source_files` |
| 38.1013 | View Creation Still Accepts Empty Or Punctuation-Only Names And Collapses Them To `views/.md` |
| 38.1014 | Custom View Creation Still Accepts Blank `custom_persona_label` |
| 38.1022 | Generic Doc Create/Update Still Let Callers Mint Malformed “View” Rows Outside The Real View Workflow |
| 38.1034 | Ontology Code Tabs Still Persist Line-Looking Comments As Node Comments |
| 38.1066 | View Create/Sync Completions Still Leave Open Doc Tabs Stale |
| 38.1224 | Ontology Upsert Still Silently Flattens Missing Parents Into Roots |
| 38.1225 | Ontology Upsert Still Turns Cyclic Parent Graphs Into Generic 500s |
| 38.1226 | Ontology Upsert Still Accepts Payload Fields It Never Persists |
| 38.1228 | Ontology Upsert Still Never Validates Duplicate `componentId` Values |
| 38.1231 | Workspace-Wide Sync Still Has No First-Party Client Surface |
| 38.1243 | Internal View-Persistence Routes Still Accept Non-View Doc IDs |
| 38.1235 | Ontology Upsert Still Ignores Rename-Only Updates |
| 38.1239 | Ontology Upsert Still Treats Invalid `nodeType` Values As Internal Errors |
| 38.1245 | Shared Internal Ontology DTO Still Advertises The Wrong `nodeType` Vocabulary ✅ |
| 38.1259 | View Deletion Still Has No First-Party Browser Surface |
| 38.1260 | Doc/View Create And Delete Still Have No Cross-Client Live Refresh Path |
| 38.1264 | View-Row Keyboard Activation Still Hijacks Nested Sync And Implement Buttons |
| 38.1265 | View-Row Action Buttons Still Stay Invisible To Keyboard Users |
| 38.1267 | Create-View Input Still Silently Drops All But The First Scope Node |
| 38.1268 | Internal Ontology Upsert Still Reconciles New Nodes By `(parent_id, name)` Instead Of `componentId` |
| 38.1275 | View Sync Still Throws Away The Returned `jobId` In The First-Party Browser Flow |
| 38.1328 | Spec Tooling Still Lacks A First-Class `delete_spec` Primitive |

### B11 — Invite-Only Visibility & Cross-Org (19 items)

| § | Title |
|---|-------|
| 38.12 | Invite-Only Workspace Reads Still Need Consistent Visibility Filtering |
| 38.21 | Comment APIs Still Bypass Invite-Only Workspace Visibility |
| 38.22 | Chat Session APIs Still Bypass Invite-Only Workspace Visibility |
| 38.23 | Docs APIs Still Bypass Invite-Only Workspace Visibility |
| 38.24 | Workspace Search Still Ignores Invite-Only Membership |
| 38.25 | Workspace Settings And Spend Endpoints Still Bypass Invite-Only Visibility |
| 38.29 | Jobs API Still Bypasses Invite-Only Workspace Visibility |
| 38.37 | Human Escalation APIs Still Bypass Invite-Only Workspace Visibility |
| 38.38 | Notification APIs Still Bypass Invite-Only Workspace Visibility |
| 38.90 | Human Escalation Routes Still Bypass Invite-Only Workspace Membership |
| 38.159 | Invite-Only Workspace Membership Management Still Ignores Workspace Roles |
| 38.269 | Invite-Only Workspace Member Listing Still Joins `users` From A Tenant-Scoped Session |
| 38.325 | `useUpdateWorkspaceVisibility()` Still Throws Away The Workspace Detail Payload ✅ |
| 38.501 | `PATCH /api/v1/workspaces/:id` Still Turns Invalid `status` And `visibility` Values Into 500s ✅ |
| 38.506 | Workspace Member APIs Still Allow Latent ACL Rows Outside Invite-Only Membership |
| 38.1206 | Invite-Only Workspace Membership Is Still A Backend-Only Feature In The Client |
| 38.1217 | Invite-Only Workspace Visibility Still Has No Runtime Toggle In The Client |
| 38.1236 | Cross-Org `/projects` Rows Still Hide Org Identity |
| 38.1237 | Workspace List Surfaces Still Cannot Show Invite-Only Access Mode |

### B12 — Auth, Tokens, Permissions & Tenant Isolation (83 items)

| § | Title |
|---|-------|
| 38.1 | Invitation Decline Still Needs Recipient Authorization |
| 38.26 | Workspace Config Scope Endpoints Still Expose Private Workspace Config |
| 38.64 | Workspace Scope Checks Can Mix Foreign-Org Overrides Into The Current Org |
| 38.65 | Permission Read Routes Are Over-Restricted Relative To Their Own Contract |
| 38.66 | Production Auth Still Persists Bearer JWTs In `localStorage` ✅ |
| 38.72 | Permission-Management Tables Still Allow Cross-Org Reads And Writes By Path ID ✅ |
| 38.94 | Resolver Redirect Remains An Authenticated Open Redirect |
| 38.98 | Token-Only Sessions Cannot Really Switch Orgs, And The Create-Org Wizard Treats That As Success |
| 38.110 | Refresh Still Exposes The Long-Lived Refresh Token To JavaScript And Does Not Rotate It |
| 38.112 | Permissions Hooks Still Encode The Wrong Route Shapes And Response Types |
| 38.113 | Periodic Auth Refresh Still Leaves `CurrentUserDTO` Stale For The Life Of The Tab |
| 38.133 | Org Runtime Config Still Uses Workspace Scopes Instead Of Settings/Admin Authorization |
| 38.134 | Non-Admin Settings Sessions Still Fire Admin-Only Credential Queries |
| 38.136 | Org Members Permissions Still Mix Route Membership With JWT Scopes From Another Org |
| 38.137 | Reused Authz Session Rows Still Let Concurrent Requests Activate The Wrong Org/Scope Context |
| 38.138 | Permissions APIs Still Ignore Role-Granted Default Scopes |
| 38.143 | `scoped_db_for_*` Still Stamps Requested Scopes Without Verifying Them |
| 38.158 | API Key Creation Still Lets `org:admin` Callers Mint Broader Scopes Than They Hold |
| 38.161 | Member-Role Mutation Endpoints Still Turn Bad Input Into 500s ✅ |
| 38.180 | Cross-Org Client Navigation Still Never Refreshes JWT Org Context |
| 38.237 | Public User Lookup Still Exposes Cross-Org Profiles To Any Authenticated Token |
| 38.248 | Notification And `@mention` Flows Still Depend On Ungranted System Tables From Tenant-Scoped Sessions |
| 38.263 | Workspace Visibility Enforcement Still Lacks A Canonical Entity-Scoped Authorization Primitive |
| 38.297 | Workspace-Member Permission Hooks Still Point At The Wrong Contracts |
| 38.355 | Browser Auth Bootstrap And Event-WebSocket Ticketing Still Ignore `VITE_API_URL` |
| 38.371 | Event WebSocket Ticket Failures Still Degrade Into Empty-Token Reconnect Loops |
| 38.388 | API Key Creation Still Accepts Negative Expiry Durations |
| 38.399 | Org Credential Revoke Actions Still Fail Silently In Settings |
| 38.410 | OAuth Login Entry Points Still Ignore `VITE_API_URL` |
| 38.411 | Auth Gating Still Sends Logged-Out Users To The Marketing Landing Page Instead Of The Live Login UI |
| 38.415 | Non-Author Comment Threads Still Show Resolve Actions The API Will Reject |
| 38.429 | Workspace Settings Still Accept Negative Spend/Token Limits And Can Instantly Self-Lock A Workspace ✅ |
| 38.430 | Logout And Session Expiry Still Leave User-Scoped React Query Data Warm Across Re-Login |
| 38.446 | Spotlight Tabs Still Ignore Their Own Scope When Showing Recent Items |
| 38.457 | OAuth Login Still Has No End-To-End Return-Target Plumbing |
| 38.533 | Remote-Capable CLI Verbs Still Inconsistently Support Auth Tokens |
| 38.560 | Smart-HTTP Push Auth Still Ignores The Actual Workspace Write Model For Org-Visible Workspaces |
| 38.570 | `g5d` Still Collapses `NotFound` Errors Into The Network/Auth Exit Code | ✅ |
| 38.727 | Data-Channel Token Minting Still Targets A Nonexistent g5-api Route |
| 38.750 | Data-Channel Token Permissions Still Do Not Constrain Stream Access |
| 38.751 | Data-Channel Tokens Still Lack Issuer/Audience Binding At Validation Time |
| 38.901 | `emit_view_artifact` Still Trusts Raw `view_path` / `mapping_path` When No Authoritative Path Is Present |
| 38.1003 | `delete_file` Still Bypasses Read-First And Human-Authority Guards |
| 38.1065 | User-Triggered Background Jobs Still Drop `created_by` And Lose User-Scoped Runtime Secrets |
| 38.1081 | User Preferences GET Still Performs A Durable Write Under A Read Scope |
| 38.1098 | Personal Access Token Scopes Still Do Not Bound Authenticated-Only Routes |
| 38.1099 | `CurrentUserDTO` Still Omits Active-Org Identity While Ordering Memberships Opposite To Session Auth |
| 38.1100 | Refresh-Token Fallback Still Pins Stale Org Context Instead Of Re-Resolving Current Memberships |
| 38.1101 | PAT Authorization Still Does Not Track Current Org Role Or Live Scope Grants |
| 38.1103 | OAuth Account Linking Still Trusts Non-GitHub Provider Emails Without Any Verification Check |
| 38.1108 | Self-Scope Introspection Still Fails For Authenticated Users With No Live Org Context |
| 38.1109 | The Two Public Scope-Check APIs Still Disagree On The Permission Model |
| 38.1110 | First OAuth Login With Multiple Pending Invitations Still Picks An Arbitrary Active Org |
| 38.1112 | Initial Auth Bootstrap Still Accepts A JWT-Only Session Even When Profile Load Fails |
| 38.1113 | Permission Replacement APIs Still Turn Duplicate Scope Lists Into Generic 500s |
| 38.1121 | Permission Grant APIs Still Accept Arbitrary Existing Users And Pre-Seed Dormant Scope Rows |
| 38.1122 | Permission Replacement APIs Still Skip Target Validation And Toggle Between False-Success And FK 500s |
| 38.1123 | `set_user_scopes` Still Accepts Past-Dated `expiresAt` Values And Reports Success For Already-Dead Grants |
| 38.1125 | OAuth Auto-Accept Still Consumes Pending Invitations Even When The Invited Role Was Not Applied |
| 38.1129 | Permissions Read APIs Still Collapse Missing Targets Into Empty Success Responses |
| 38.1130 | `DELETE /api/v1/workspaces/:workspaceId/permissions/:userId` Still Always Reports `removed: true` |
| 38.1134 | App-Wide Event WebSocket Ticketing Still Cannot Recover From A Stale JWT |
| 38.1138 | Org Admin Mutation Routes Still Mint Read-Scoped DB Sessions ✅ |
| 38.1142 | Re-Inviting With A Different Role Still Silently Keeps The Old Role |
| 38.1143 | Public Chat Mutation Routes Still Mint Read-Scoped DB Sessions |
| 38.1159 | API Key Creation Still Accepts Blank Or Whitespace-Only Names ✅ |
| 38.1163 | Workspace Settings Mutation Route Still Mints A Read-Scoped DB Session |
| 38.1164 | API Key Creation Still Accepts The Reserved `internal:*` Scope Even Though PAT Auth Cannot Use It |
| 38.1165 | API Key Creation Still Accepts Duplicate Scope Entries ✅ |
| 38.1166 | API Key Validation Still Silently Drops Unknown Stored Scopes Instead Of Failing Closed |
| 38.1170 | Public Doc Mutation Routes Still Mint Read-Scoped DB Sessions |
| 38.1178 | Notification Read-State Mutation Routes Still Mint Read-Scoped DB Sessions |
| 38.1182 | Workspace Spend And Token Limits Still Treat `0` As A Valid Saved Value Even Though The Limiter Reads It As Already Exhausted |
| 38.1186 | Workspace Mutation Routes Still Mint Read-Scoped DB Sessions |
| 38.1189 | Runtime-Config Secret Updates Still Accept Blank Secret Key Names |
| 38.1194 | OAuth Login Still Overwrites User-Customized Profile Fields |
| 38.1218 | Permission Administration Still Has No First-Party Client Surface |
| 38.1223 | Internal Chat Completion Still Accepts Negative Token Totals ✅ |
| 38.1302 | “Shown Once” Credential Reveals Still Persist Across Drawer Close/Reopen And Org Changes |
| 38.1303 | Credential Reveal Copy Action Still Assumes The Clipboard API Works |
| 38.1308 | Closed Settings Drawer Still Eagerly Fires Settings, Spend, Config, And Credential Queries ✅ |
| 38.1309 | Credential Manager Still Lets A Second Create Silently Overwrite The First One-Time Secret |
| 38.1310 | Switching Away From `Organizations` Still Destroys The One-Time Credential Reveal |

### B1 — Chat & Session UX (140 items)

| § | Title |
|---|-------|
| 38.15 | Live Comment Replies Still Miss The Reply-Thread Cache ✅ |
| 38.33 | Chat "Resume" Is Wired To Forks Or No-Ops Instead Of Session Resume ✅ |
| 38.43 | Chat Session Summaries Still Do Not Use The Advertised LLM Flow |
| 38.46 | Chat Fork Mutation Uses The Wrong Response Contract ✅ |
| 38.48 | Attach-Session Hook And Tests Still Model A Fake Response Shape |
| 38.50 | Chat Citation Pills Are Rendered But Never Navigate ✅ |
| 38.51 | Chat Artifact Download Buttons Are Still No-Ops ✅ |
| 38.53 | Internal Chat Completion Endpoints Only Verify Org Membership |
| 38.80 | Chat Session Lists Only Load The First Page |
| 38.82 | Chat Message History Still Truncates Long Sessions At 50 Messages |
| 38.88 | Waiting Chat Reactivation Can Report Success Without Reactivating Anything |
| 38.109 | Logout Still Clears Client State Without Successfully Revoking The Server Session |
| 38.123 | Normal Chat Sessions Never Leave `active`, So Completed/Resume Semantics Are Mostly Dead |
| 38.124 | Waiting Chat Reactivation Reboots The Agent Without Prior Conversation Context |
| 38.131 | Chat Fork Requests Still Send `atIndex` Instead Of `fromMessageIndex` ✅ |
| 38.140 | Shared Chat-Model Fixtures Still Use The Pre-DTO String Array Shape ✅ |
| 38.145 | The Frontend Chat-Model Schema Still Omits Backend Cost Fields ✅ |
| 38.165 | Viewer Sessions Still See Editable Workspace/User Runtime-Config Controls They Cannot Save |
| 38.178 | Workspace Search Still Advertises Comment/Chat Results That The Backend Never Produces |
| 38.196 | Chat Send-Message Hooks And Tests Still Treat The Pending Assistant Handle As A Full Message DTO |
| 38.200 | Session Forking Still Treats The Fork Response As A Full Session DTO ✅ |
| 38.201 | Attach-Session Still Uses An Obsolete `{ attached, sessionId }` Client Contract |
| 38.239 | Chat Session Cost Totals Still Use A Placeholder Flat-Rate Formula |
| 38.243 | `stop_generation` Still Emits `chat.done` Before The Stopped State Commits |
| 38.247 | Comment APIs Still Join `users` From Tenant-Scoped Sessions Even Though `g5_tenant` Cannot Read That Table |
| 38.268 | Workspace Overview And Manifest Routes Still Depend On `repos` From Tenant-Scoped Sessions |
| 38.284 | Active Chat Session State Still Survives Workspace Switches And Can Target The Previous Workspace's Session |
| 38.288 | Org Member And Invitation Admin Routes Still Depend On `org_memberships`, `org_invitations`, And `users` From Tenant-Scoped Sessions |
| 38.289 | `useForkSession()` Still Sends The Wrong Request Field And Expects The Wrong Response DTO |
| 38.298 | Chat Control Hooks Still Model Obsolete `stop` And `attach` Response Payloads ✅ |
| 38.312 | Live Chat Timeline Entries Still Bleed Across Session Switches |
| 38.317 | Chat Session Detail/Create Hooks Still Downgrade `SessionDetailDTO` To `ChatSessionDTO` |
| 38.320 | Chat Still Advertises Incremental Streaming Even Though The Runtime Only Delivers Final `complete`/`error` Events |
| 38.322 | Chat Session URL State Still Drifts Away From The Session The User Is Actually Viewing |
| 38.323 | Chat Auto-Scroll Still Ignores Streamed Trajectory Entries |
| 38.327 | Session Timeline History Still Caps Out At Page 1 With No Way To Reach Older Or Newer Trace Pages |
| 38.328 | Chat Trajectory Still Ingests Live Trace Entries From Unrelated Workspace Tasks |
| 38.329 | Stale Chat Session IDs Still Fail Open And Can Send Messages Into A Hidden Session |
| 38.334 | Long Chat Sessions Still Lose Live Assistant Streaming After A Reload |
| 38.335 | Chat Session Creation Still Has No In-Flight Guard, So Double Actions Can Spawn Duplicate Blank Sessions |
| 38.338 | First-Turn Chat Send Failures Still Do Not Roll Back Cleanly From An Empty Cache ✅ |
| 38.339 | Chat Session Lists Still Go Stale After Ordinary Message Activity |
| 38.340 | WebSocket Reconnect Still Does Not Recover Missed Session-Timeline History |
| 38.341 | Concurrent Chat Sends Can Still Allocate Duplicate `message_index` Slots |
| 38.342 | Bare Chat `end_turn` Still Leaves The Public Assistant Message Pending |
| 38.343 | Waiting Chat Turns Still Discard Uncommitted Workspace Changes Before The Next Follow-Up |
| 38.344 | Chat Message Pagination Still Returns `hasMore = true` For Exact-Limit Pages ✅ |
| 38.349 | Forked Sessions Still Reset Copied Conversation Cost To `$0` |
| 38.351 | Chat Model Metadata Still Advertises Streaming Support Even Though The Runtime Does Not Stream |
| 38.353 | Public `inject_message` Still Never Delivers The Injection To The Agent Inbox |
| 38.367 | Session Timeline Still Returns A False Empty Success For Unknown Session IDs |
| 38.390 | Chat Message History Still Forwards Raw `limit` Values Straight Into SQL ✅ |
| 38.414 | Shared Comment Thread UI Still Advertises Unresolve Without Any Wiring |
| 38.417 | Comment Column "Add Thread" Hover Affordance Is Still A Dead Stub |
| 38.472 | Artifact `apply` / `run` Actions Are Still Modeled But Inert In Workspace Chat |
| 38.474 | Artifact `open` Actions Still Render But Cannot Navigate In Workspace Chat |
| 38.483 | Chat Message Artifact Dedupe Still Collapses Distinct Artifacts That Share The Same Body |
| 38.491 | User And System Chat Messages Still Bypass The Shared Inline Markdown Renderer |
| 38.495 | Assistant Message Citation References Still Bypass The Citation-Aware Renderer |
| 38.548 | `g5d push --all` Still Only Changes The Final Status Message |
| 38.731 | Legacy Browser Chat Endpoints Still Advertise A Real Session Flow But Only Log And Ping |
| 38.735 | Legacy `search_live` Still Buffers The Entire Search Before Sending The First “Streaming” Result |
| 38.736 | Legacy Chat Session Creation Still Returns WebSocket URLs Built From The Server Bind Interface |
| 38.768 | Prompt-Extension Skills Still Do Not Actually Inject Their Protocol Into The Main Loop |
| 38.803 | Prompt-Extension Skills Still Bypass The Self-Invocation Guard |
| 38.814 | Prompt-Extension `invoke_skill` Still Drops Structured Skill Arguments |
| 38.819 | `send_message` Still Tries To Inject System Metadata Through The Ordinary User-Message Route |
| 38.821 | Prompt-Extension Skills Still Ignore Their Declared `max_turns` |
| 38.822 | `debug_tests.analyze_only` Still Asks For Write-Side Effects In Its Own Prompt |
| 38.829 | Prompt-Extension Skills Still Ignore Their Declared `tool_allowlist` |
| 38.830 | Prompt-Extension Skills Still Cannot Satisfy Their Declared `output_schema` |
| 38.831 | `send_message` Still Drives The Ordinary Chat-Send Workflow Instead Of A One-Way Injection |
| 38.832 | `send_message` Still Accepts Any Active Session Instead Of Only Agent Sessions |
| 38.833 | `send_message` Still Throws Away The API’s Structured Response Envelope |
| 38.839 | Public Chat Send Still Drops Its Declared `context` Attachments |
| 38.843 | `respond_to_user` Still Lies About Session Flow |
| 38.848 | Background Terminal Sessions Still Report `Idle` Instead Of Running |
| 38.850 | Terminal Streams Still Ignore Sessions Created After The Browser Connects |
| 38.851 | Terminal `SessionState` Messages Still Exist Only As Catch-Up Snapshots |
| 38.852 | Terminal Session `is_background` Metadata Still Drifts After Session Reuse |
| 38.857 | Terminal Session Limits Still Count Dead Sessions Forever ✅ |
| 38.859 | Timed-Out Terminal Commands Still Fail Open Into An `Idle` Reusable Session |
| 38.861 | Terminal Catch-Up Still Replays Completed Sessions' Historical Output |
| 38.867 | VM-Isolated `run_in_terminal` Still Ignores Session And Background Semantics |
| 38.868 | Terminal Manager Still Holds Session-Map Locks Across Awaited Session Work |
| 38.869 | Terminal Catch-Up Still Misreports Running Sessions As `Idle` On Contention |
| 38.871 | Background Terminal Runs Still Hijack Default Session Reuse |
| 38.872 | `global_escalation` Still Drops The Originating Session ID ✅ |
| 38.884 | Terminal Session Shutdown Still Fakes The Advertised Signal Cascade |
| 38.886 | `extract_for_view` Still Returns Prompt Context Instead Of Extracted Entries |
| 38.897 | `search_streaming()` Still Holds The Index Read Lock Across Awaited Sends |
| 38.1026 | `respond_to_user` Still Succeeds As A Silent No-Op On Non-Chat Tasks |
| 38.1032 | Chat Terminal Completion Still Accepts Blank Or Whitespace-Only Content |
| 38.1050 | Comment Lists Still Ignore Thread Activity Ordering |
| 38.1062 | Internal Global Escalation Requests Still Pretend `sessionId` Is Optional |
| 38.1064 | Internal Escalation Creation Still Trusts Caller-Supplied `session_id` Provenance |
| 38.1148 | Chat Session Creation Still Treats Invalid `sessionType` / `scopeType` As Internal Errors |
| 38.1156 | Public Chat Session Create Still Never Validates Node-Scoped `scopeRef` |
| 38.1158 | Public Chat Send Still Accepts Whitespace-Only User Messages ✅ |
| 38.1167 | `stop_generation` Still Ignores The Zero-Row Race And Can Report A False Stop |
| 38.1168 | Public Chat Session Create Still Accepts Blank Titles, And The UI Treats Empty String As A Real Title ✅ |
| 38.1172 | Workspace-Scoped Chat Routes Still Never Validate The Target Workspace Exists |
| 38.1173 | `list_sessions` Still Accepts Invalid Filter Enums And Treats Them As Empty Results |
| 38.1180 | Fork Session Still Accepts Blank Or Whitespace-Only Explicit Titles ✅ |
| 38.1184 | Chat Model Catalog Still Returns A Hardcoded Model List Instead Of Reflecting Instance Configuration |
| 38.1185 | Session Search Still Skips The Query Validation Floor Used By Workspace Search |
| 38.1187 | Session Summarization Still Ignores Topic Shifts In Longer Conversations |
| 38.1193 | Session Summarization Still Panics On Multi-Byte UTF-8 Titles And Topic Hints ✅ |
| 38.1196 | Chat Session Search Pagination Is Still Unstable Across Relevance Ties |
| 38.1208 | Chat Session List Pagination Is Still Unstable Across Ordering Ties |
| 38.1215 | Session Summarization Still Rewrites Chat Recency Ordering |
| 38.1216 | Chat Session `linkedCommits` / `linkedNodes` Still Have No Live Population Path |
| 38.1222 | Chat Session `tags` Still Have No Live Population Path |
| 38.1227 | Chat Session Summaries Still Have No First-Party Trigger In The Client |
| 38.1235 | Forked Chat Sessions Still Drop Existing Session Summaries |
| 38.1240 | Session Picker Still Renders Valid `null` Titles As Blank Rows ✅ |
| 38.1241 | `ChatService::update_message_status` Still Isn’t Actually Atomic |
| 38.1242 | Internal Chat Completion/Error Routes Still Report Success On No-Op Transitions |
| 38.1246 | Chat Panel History Dropdown Still Mishandles Valid `null` Titles ✅ |
| 38.1247 | Session Picker Trigger Still Renders Valid `null` Titles As Blank |
| 38.1252 | Session Picker Search Still Ignores Visible Summary Text |
| 38.1254 | Session Picker Keyboard Selection Still Operates On Stale Search Results |
| 38.1255 | Session Picker Reopen Still Shows A Stale Search Filter For 300ms |
| 38.1256 | Session Picker Never Clamps Keyboard Focus After Search Results Change |
| 38.1261 | Session Picker Closed State Still Forgets Active Sessions Missing From The Loaded List |
| 38.1262 | Browser-Created Chat Sessions Still Persist The Literal Title "New Session" |
| 38.1263 | Browser Chat History Still Cannot Identify Forked Sessions |
| 38.1266 | Workspace Chat Creation Still Pollutes Filtered Session Caches |
| 38.1269 | In-Panel Chat History Search Still Ignores Visible Summary Text |
| 38.1270 | In-Panel Chat History Dropdown Still Reopens With A Stale Hidden Filter |
| 38.1271 | Session Picker Rows Still Reserve Blank Summary Lines For Unsummarized Chats |
| 38.1274 | In-Panel Chat History Dropdown Still Hides Non-Active Session Status |
| 38.1278 | In-Panel Chat History Dropdown Still Has No Keyboard Interaction Model |
| 38.1280 | In-Panel Chat History Dropdown Still Drops Message Count And Cost Context |
| 38.1283 | Inline Chat Citation References Still Aren't Keyboard-Activatable |
| 38.1284 | Inline Chat Citation Parsing Still Restyles Plain `[number]` Text As A Citation |
| 38.1298 | Mobile Workspace Route Still Drops The Browser Into A Chat-Only Shell |
| 38.1304 | Terminal Artifacts Still Reinterpret A Real First Prompt-Like Line As Command Metadata |
| 38.1324 | SessionPicker Clear Search Still Keeps A Stale Focused Session Alive ✅ |
| 38.1325 | SessionPicker Container Enter Handling Still Hijacks Nested Button Activation |

### B3 — Workspace & Projects Page (99 items)

| § | Title |
|---|-------|
| 38.4 | Workspace Search Contract Overpromises Docs/Comments Coverage |
| 38.6 | Workspace Creation Needs Safer Partial-Failure Recovery |
| 38.9 | `/projects` Create-Workspace Navigation Uses the Wrong Identifier |
| 38.35 | `/projects` Workspace Actions Expose Pause/Abort/Delete Controls That Do Nothing |
| 38.36 | Projects Table Always Mislabels Workspace Mode As `pinned` ✅ |
| 38.49 | Instance Search Status Returns Arbitrary Workspace Search Settings |
| 38.79 | Workspace Lists Still Drop Everything After Page 1 |
| 38.83 | Recipient-Scoped Notifications Are Still Broadcast To Entire Workspaces |
| 38.84 | Workspace UI Never Subscribes To The Intended Private Notification Channel |
| 38.100 | Invalid Workspace Slugs Never Surface A Not-Found State |
| 38.116 | Notification Read Actions Are Still Over-Gated Behind `workspaces:write` |
| 38.150 | `CreateDocDialog` Still Persists A Shared Bare `#!internal-workspace-docs` Path |
| 38.162 | Workspace Resolution Still Breaks On Names That Differ Only By Case |
| 38.173 | The Document Sidebar Still Hides Legitimate Duplicate Docs |
| 38.194 | Workspace Dependencies Hook Still Expects Graph Edges From An Id-List Endpoint ✅ |
| 38.196 | Base-Workspace Navigation Still Hardcodes `/base` Instead Of The Real Base Workspace Identity |
| 38.206 | Projects-Page Workspace Pause/Abort Actions Are Still Wired To No-Ops |
| 38.235 | Reply Creation Still Drops Live Updates And Mentions When Parent Workspace Lookup Fails |
| 38.241 | Notification Hook Still Carries Live Notifications Across Workspace Changes |
| 38.251 | Workspace Settings Still Make Embedding Config Write-Only Through The Public API |
| 38.252 | Workspace Settings UI Still Hides Live Autoscaling Knobs |
| 38.265 | Workspace Mutation Hooks Still Model Create/Update Responses As `WorkspaceSummary` Instead Of The Live Detail DTO ✅ |
| 38.261 | Workspace Materialization Still Exists In Parallel Legacy Forms Instead Of One Canonical Contract |
| 38.283 | Workspace Tab Persistence Still Bleeds Across Workspace Switches And Can Overwrite The New Workspace's Saved Tabs |
| 38.365 | Saving Workspace Settings Still Leaves The Spend Breakdown Cache Stale |
| 38.373 | Search-Indexer Workspace Events Still Die In The Browser Contract |
| 38.374 | Workspace-Channel Reconnect Recovery Still Never Refetches Comment Caches |
| 38.387 | Workspace Runtime-Config Upserts Still Allow `workspace_id` / `org_id` Drift |
| 38.391 | Workspace Search Still Lacks Lower-Bound Validation For `limit` |
| 38.398 | Workspace Settings Save Failures Without Field-Level `details` Still Disappear Silently |
| 38.408 | Workspace Deep-Link Resolution Still Overmatches Via Content Substring Search |
| 38.412 | Workspace Settings Form Still Drops Unsaved Edits On Query Refresh |
| 38.426 | `workspace.settings_updated` Still Exists Only In Enums And Tests |
| 38.432 | Workspace Tab Persistence Still Is Not Namespaced Per User |
| 38.439 | Left Sidebar Section Order Still Bleeds Across Workspace Switches |
| 38.440 | Sidebar Section Order Persistence Still Is Not Namespaced Per User |
| 38.443 | Workspace Page Still Hijacks `Cmd/Ctrl+K` Inside Editing Contexts |
| 38.444 | Workspace Search `totalCount` Still Reports Only The Truncated Page Size ✅ |
| 38.448 | Workspace Sidebar Still Hides File-Tree Fetch Failures Behind A Fake Empty State |
| 38.451 | Left Sidebar Collapse And Expansion State Still Bleed Across Workspace Switches |
| 38.459 | Landing-Page Waitlist Submission Still Ignores `VITE_API_URL` |
| 38.468 | Files Sidebar Still Cannot Highlight The Active File Tab |
| 38.498 | `g5d revert` Still Ignores Workspace Docs Changes |
| 38.500 | `PATCH /api/v1/workspaces/:id` Still Accepts Whitespace-Only Names ✅ |
| 38.502 | `DELETE /api/v1/workspaces/:id` Still Hard-Deletes Active Workspace State Via FK Cascade |
| 38.508 | `DELETE /api/v1/workspaces/:id/members/:userId` Still Returns Success For No-Op Deletes |
| 38.504 | `g5d push` Still Rewrites `workspace.lock` On Partial Push Failure |
| 38.510 | `g5d init` Still Invents A Published Workspace Identity |
| 38.514 | Workspace Write Routes Still Label Mutations As `workspaces:read` Internally |
| 38.530 | `sync_workspace_meta()` Still Hides Workspace-Meta Commit Failures |
| 38.539 | `g5d init` Still Reports Success After Workspace-Meta Initialization Failures |
| 38.544 | Remote Workspace-Name Merge Still Searches Only The First Page Of Workspaces |
| 38.551 | `g5d init --force` Still Reuses Stale Workspace State Instead Of Reinitializing Cleanly |
| 38.567 | `g5d push` Still Pushes Local Docs To The Current Workspace When The Target Workspace Has No Docs Config |
| 38.591 | `g5d doctor` Still Detects Linux CoW Support On `/` Instead Of The Workspace Filesystem |
| 38.592 | `g5d doctor` Still Treats Any Existing `workspace-meta` Directory As Healthy |
| 38.593 | `g5d doctor` Still Does Not Validate Remote-Enabled Workspace Identity |
| 38.603 | `g5d merge` Still Treats Missing Local Source Paths As Remote Workspace Identifiers |
| 38.604 | `g5d add` Still Reports Success After Workspace-Meta Durability Failures |
| 38.730 | Browser Tree Listing Still Follows Symlinked Directories Outside The Workspace |
| 38.742 | Legacy Browser Pull Still Treats Dirty-Check Failures As A Clean Workspace |
| 38.758 | Sub-Agent Workspace Tree Injection Still Follows Symlinked Directories |
| 38.898 | `semantic_search` Still Accepts Empty Queries And Turns Them Into A Workspace-Wide Line Scan ✅ |
| 38.912 | Shared `resolve_path()` Still Allows Workspace Escapes Via Lexical Absolute Paths And Symlinks |
| 38.917 | `trigger_workflow` Still Isn't Bound To The Current Workspace |
| 38.919 | Prepopulation Workspace Listings Still Splice Raw Paths Into Shell Commands |
| 38.925 | Reasoning-Model Fallback Prepopulation Still Never Includes Workspace Structure |
| 38.926 | Prepopulation Workspace Listings Still Use Host Paths That Break Under VM Isolation |
| 38.1018 | `run_in_terminal` Still Lies About Defaulting `cwd` To The Workspace Root | ✅ |
| 38.1051 | Comment Mutation Routes Still Label Writes As `workspaces:read` Internally ✅ |
| 38.1057 | `CommentsSidebar` Still Exists Only As A Dead Comment Surface |
| 38.1114 | Workspace Names Still Accept Raw Route Delimiters Even Though The Product Treats Them As URL Segments |
| 38.1115 | The 255-Character Workspace-Name Contract Still Only Exists On The Create Path |
| 38.1116 | `PATCH /api/v1/workspaces/:id` Still Treats Empty Bodies As Successful Updates |
| 38.1118 | Workspace Dependency Graph Still Allows Cycles Despite The Published DAG Contract |
| 38.1119 | Workspace Dependency Create/Replace Still Turn Duplicate IDs Into Generic 500s |
| 38.1175 | Workspace Settings Update Still Relies On Database CHECK Constraints For `spendPeriod` And `embeddingProvider` |
| 38.1176 | Workspace Settings Update Still Treats An Empty Body As A Real Mutation |
| 38.1177 | Workspace Settings Still Accept Non-Positive Scaler Knobs And The Scheduler Quietly Reinterprets Them |
| 38.1181 | Workspace Settings Update Still Silently Coerces Unrepresentable `maxSpendUsd` Values To `0` |
| 38.1207 | Workspace Stats Are Still A Dead Client Surface |
| 38.1214 | Workspace Dependency Inventory Still Has No Runtime UI |
| 38.1229 | `usePatchWorkspace()` Still Models The Wrong Request Shape ✅ |
| 38.1230 | Workspace Metadata Editing Still Has No First-Party Client Surface |
| 38.1238 | Workspace List Pagination Is Still Unstable Across `updated_at` Ties |
| 38.1250 | Workspace Manifest Route Still Has No First-Party Browser Surface |
| 38.1251 | Workspace List Surfaces Still Cannot Identify The Base Workspace |
| 38.1253 | Internal Staleness Updates Still Ignore The Workspace Path And Mutate By Raw Node ID |
| 38.1272 | Sidebar Header `+` Actions Still Toggle Section Collapse For Keyboard Users |
| 38.1273 | Jobs Sidebar Hover Actions Are Still Mouse-Only |
| 38.1279 | Jobs Sidebar Hover State Still Blanks Metadata For Non-Actionable Jobs |
| 38.1282 | Jobs Sidebar Rows Still Cannot Be Activated From The Keyboard |
| 38.1307 | Spotlight `Workspace` Tab Still Excludes Docs And Leaves No Scoped Home For Workspace Docs |
| 38.1312 | Spotlight `External` Tab Still Just Duplicates Workspace File Search |
| 38.1313 | Workspace Switcher Clear Search Still Keeps A Stale Keyboard Selection Alive ✅ |
| 38.1318 | Workspace Switcher Still Collapses Loading And Error States Into “No Workspaces Found” |
| 38.1319 | Workspace Switcher Container Key Handling Still Hijacks Nested Button Activation |
| 38.1321 | Workspace Table Rows Are Still Mouse-Only Despite Acting As Primary Navigation |
| 38.1322 | Workspace Modals Still Claim `aria-modal` Without Trapping Focus Or Isolating The Background |

### B4 — Org Onboarding & Invitations (43 items)

| § | Title |
|---|-------|
| 38.10 | `/projects` Only Loads Workspaces For The First Org ✅ |
| 38.29 | Settings Panel Falls Back To The Wrong Org On Unknown Slugs |
| 38.30 | Root And Settings Navigation Still Hardcode The First Org |
| 38.95 | SCM Connect From The Create-Org Wizard Cannot Complete In The Browser |
| 38.96 | Org Invite Flows Accept And Even Advance On Invalid Email Input |
| 38.97 | Org Create/Update Still Accept Arbitrary Caller-Supplied Slugs | ✅ |
| 38.108 | `PATCH /api/v1/orgs/:orgId` Bypasses Create-Time Validation And Can Strand Org Routing |
| 38.114 | Create-Org SCM Connect Still Leaves Step 2 Stuck On Stale Provider State After Return |
| 38.119 | Org Invitation Resend/Revoke Still Operate On Raw Invitation UUIDs Outside The Path Org |
| 38.122 | Explicit Invitation Accept Still Is Not Idempotent Under Duplicate Or Racing Acceptance ✅ |
| 38.135 | Org Members Navigation Still Never Resolves Invalid Or Stale Org Slugs |
| 38.142 | Org-Scoped DB Capabilities Still Accumulate Old Org IDs And Never Shrink |
| 38.147 | Org Members Admin Actions Still Fail Silently And Dismiss UI State Before Success |
| 38.148 | Org Invitation DTOs Still Ship Blank Inviter Metadata And A Split Contract | ✅ |
| 38.160 | Mention Resolution Still Notifies Same-Org Non-Members About Private Comments | ✅ |
| 38.167 | The Projects Page Still Creates New Workspaces In The Oldest Org With No Org Picker |
| 38.181 | WorkspacePage Still Falls Back To Route Slugs For Org-Scoped Write Dialogs |
| 38.197 | The Create-Org Wizard Still Rejects Names Whose Generated Slug Is Empty Even Though The Backend Supports A Fallback |
| 38.214 | Create-Org And Create-Workspace APIs Still Accept Whitespace-Only Names ✅ |
| 38.216 | Invite UIs Still Treat Idempotent No-Op Responses As Fresh Invitations |
| 38.220 | Create-Org Step 2 And Step 3 Work Can Also Continue After The Wizard Is Closed |
| 38.246 | Actor-Unscoped User/Org Routes Still Depend On The Restricted Tenant Pool Reaching System Tables |
| 38.301 | Org-Admin Mutation Hooks Still Throw Away The Updated Member And Invitation DTOs |
| 38.345 | Workspace Dependency Writes Still Do Not Enforce The Same-Org Validation Contract |
| 38.452 | Create-Org Clears Tenant Context Before It Updates The Bootstrapped Docs Head |
| 38.453 | Org Invite Idempotency Still Fails Under Concurrent Requests |
| 38.507 | Workspace Member Roster Still Cannot Represent Or Revoke Creator Access |
| 38.773 | Skill Router Still Accepts Unregistered Skill Names From The Model |
| 38.1094 | Explicit Org Switch Still Leaves The Shared `current-user` Query Stale ✅ |
| 38.1106 | Projects Page Still Offers “New Project” Even When There Is No Org To Create Into ✅ |
| 38.1137 | Org Members Page Still Silently Drops Invitation Fetch Failures |
| 38.1140 | Org Members Page Still Hides Accepted, Declined, And Expired Invitation History |
| 38.1141 | `GET /api/v1/orgs/:orgId/invitations` Still Returns All Historical Rows Despite Advertising “Recent” Invitations |
| 38.1145 | Invitation History DTOs Still Drop Acceptance Actor And Status-Change Time |
| 38.1150 | Org Invitations Still Have No Actual Delivery Path |
| 38.1151 | “Resend Invitation” Still Only Extends Expiry Without Re-Delivering Anything |
| 38.1152 | Public Invitation Acceptance Still Has No First-Party Client Flow |
| 38.1153 | Invitation Status Still Lags Real Expiry Until Hourly GC, So Dead Invites Can Show Up As Pending |
| 38.1292 | Standalone Invite Dialog Still Accepts Malformed Emails And Waits For A Validation Error The Backend Never Emits |
| 38.1294 | Settings Drawer Escape Handler Still Fires Behind The Nested Invite Dialog |
| 38.1295 | Nested Invite Dialog Still Outlives The Settings Drawer That Spawned It |
| 38.1311 | Invalid Org-Members Routes Still Leave A Live Invite Flow Pointing At `/api/v1/orgs//members` |
| 38.1320 | Create-Org Wizard Still Lets Back/Next/Skip Run During Import And Invite Mutations |

### B5 — Escalation & HIL (58 items)

| § | Title |
|---|-------|
| 38.81 | Escalation Surfaces Still Truncate At The First Page |
| 38.118 | Viewer Users Still Cannot Update Their Own HIL Routing Preferences |
| 38.144 | The Frontend Health Contract Still Uses `up/down` While The Backend Returns `healthy/unhealthy` ✅ |
| 38.169 | Human Escalation Resolution Still Claims Success Before Any Unblock Persists, And Never Resumes Paused Jobs | ✅ | 38.168 | Public Mark-In-Review Still Fails To Broadcast `escalation.updated` |
| 38.177 | Cancelled Escalations Can Still Be Resolved Through The Public Respond Route |
| 38.183 | File-Backed Doc Updates Still Save Successfully While Reads Keep Showing The Old File |
| 38.219 | Create-Workspace Dialog Can Also Be Dismissed While Creation Continues |
| 38.231 | `CreateProjectDocModal` Can Be Dismissed Mid-Create While The Doc Still Appears Later |
| 38.233 | Mention Delivery Still Fails Softly While Comment/Reply Creation Reports Success |
| 38.242 | `UserSettingsPanel` Can Be Closed Mid-Save While Settings Mutations Continue |
| 38.266 | Landing Page Still Exposes A Dev-Only Login CTA While Instance Info Is Loading ✅ |
| 38.300 | Escalation Action Hooks And Tests Still Assume `204` Instead Of `EscalationActionDTO` ✅ |
| 38.306 | Bottom Drawer Can Still Get Stuck On A Missing Escalation Tab |
| 38.545 | Deferred-Fork `g5d push` Still Creates Duplicate Remote Children After Partial Failure |
| 38.771 | `spawn_subagent` Still Advertises A 15-Turn Default While Runtime Uses 25 ✅ |
| 38.781 | Escalation Tools Still Report Success With `"unknown"` IDs On Malformed 2xx Responses |
| 38.782 | `semantic_search` Still Advertises Regex Search While Forcing Text Mode |
| 38.800 | Escalation Auto-Resolve Still Bypasses `recommended_option` Validation |
| 38.1010 | Escalation Tools Still Defer `recommended_option` Validation To The API |
| 38.1019 | Escalation Tools Still Silently Drop Non-String `supporting_evidence` |
| 38.1024 | Escalation Tools Still Accept Blank `context` And `uncertainty` Strings ✅ |
| 38.1025 | Escalation Tools Still Accept Blank Option Labels And Descriptions ✅ |
| 38.1028 | Escalation Tools Still Accept Blank `supporting_evidence` Entries |
| 38.1039 | Escalation Options Still Accept Duplicate Labels |
| 38.1040 | Internal Escalation Resolve Still Accepts Arbitrary `decision_source` Strings |
| 38.1041 | Internal Escalation Resolve Still Accepts Blank `response_text` |
| 38.1042 | Public Escalation Detail Still Drops `resolution_rationale` ✅ |
| 38.1043 | Internal Escalation Resolve Still Accepts Out-Of-Range Confidence Scores ✅ |
| 38.1044 | Human Escalation Resolution Still Never Persists `resolved_by_user_id` |
| 38.1045 | Cancelled Escalations Still Never Record A Cancellation Timestamp |
| 38.1046 | Escalation Provenance Fields Still Die At The Client Boundary |
| 38.1054 | Escalation List Filters Still Accept Arbitrary Strings And Fail As False Empty `200`s |
| 38.1055 | Global Escalation Creation Still Never Verifies `task_id` Belongs To `job_id` |
| 38.1058 | `in_review` Escalations Still Disappear From The Active Review UI ✅ |
| 38.1060 | Replacing An Active Escalation Still Leaves Old Global-Pause Side Effects Behind |
| 38.1061 | Superseded Escalations Still Cancel Silently Without `escalation.cancelled` |
| 38.1069 | Internal Escalation Creation Still Accepts Empty `options_to_unblock` | ✅ |
| 38.1070 | Internal Global Escalation Creation Still Accepts Blank `impact_rationale` | ✅ |
| 38.1072 | Escalation Routing Still Dies At The Public Client Boundary |
| 38.1074 | `escalation_notification_level` Still Exists Only As Dead Stored Data |
| 38.1076 | Global Escalations Still Let Running Sibling Tasks Continue |
| 38.1077 | Concurrent Escalation Creation Still Falls Through To A Raw Unique-Constraint Failure |
| 38.1079 | Escalation Resolve Still Re-Runs Post-Resolution Orchestration On Idempotent Retries |
| 38.1086 | The Live Settings Drawer Still Has No UI For Real HIL User Preferences |
| 38.1087 | Escalation Evidence Rows Still Render As Faux Links With No Open Action |
| 38.1088 | Escalation Panel Still Hides Cancel And Mark-In-Review Failures |
| 38.1093 | Escalation Resolve Still Treats Conflicting Second Decisions As Idempotent Success |
| 38.1095 | Escalation Option Buttons Still Persist Boilerplate `Choose option:` Text As The Final Decision ✅ |
| 38.1096 | WebSocket Reconnect Still Leaves Escalation Queries Stale After Missed Events |
| 38.1097 | Return-To-Human-Review Still Accepts Severity Downgrades |
| 38.1105 | Escalation List Pagination Still Uses An Unstable `created_at` Sort ✅ |
| 38.1111 | Escalation-Context Route Still Omits `originalContext` From The Shared Resume DTO |
| 38.1117 | Escalation `decision_source` Still Has A Three-Value Schema But A Two-Value API Contract |
| 38.1124 | Escalation-Context Route Still Returns Nullable Fields For A Non-Nullable Resume DTO |
| 38.1126 | Shared Internal API Client Still Omits The Dedicated Escalation-Context Endpoint |
| 38.1139 | Shared Internal API Client Still Omits The Escalation-Create Endpoints |
| 38.1146 | Shared Internal API Client Still Omits The Internal Escalation-Cancel Endpoint |

### B10 — Search, Reindex & Spotlight (23 items)

| § | Title |
|---|-------|
| 38.18 | Semantic Search Failures Are Reported As Successful Semantic Results |
| 38.435 | Spotlight Recent History Still Leaks Prior Users' Search Context |
| 38.445 | Spotlight Prefix Filters Still Stick After The Prefix Is Removed |
| 38.460 | Top Bar Inline Quick Search Is Still A Dead Product Path |
| 38.734 | Legacy Browser Search Endpoints Still Lie About `total_matches` And `truncated` At The Result Cap |
| 38.744 | Legacy Browser Search Still Cannot Honor Explicit Includes For Hidden Or Build Trees |
| 38.746 | Legacy Search Still Reports Invalid Regexes As Internal Failures ✅ |
| 38.747 | Legacy Search Still Silently Ignores Malformed Include/Exclude Globs |
| 38.748 | Legacy Search Still Treats Empty Queries As Match-Everything Scans |
| 38.757 | Legacy Search Still Accepts `max_results = 0` And Then Claims The Empty Result Was Truncated |
| 38.902 | `semantic_search` Still Treats Non-String `file_pattern` Input As “Search Everything” |
| 38.906 | Search Index Updates Still Delete Files On Transient Reindex Errors |
| 38.907 | Search Still Indexes Hidden Dotfiles By Default |
| 38.909 | `semantic_search` Still Isn't Semantic Search |
| 38.916 | `SearchIndex::update_files()` Still Ignores `.gitignore` During Incremental Reindex |
| 38.929 | Case-Insensitive Search Still Reports Unicode Match Columns From A Lowercased Surrogate String ✅ |
| 38.940 | Experience List/Search Results Still Come Back In Raw Filesystem Order |
| 38.948 | Text Search Still Panics On Valid Multibyte Queries ✅ |
| 38.1036 | Comment List Still Cannot Filter By `resolved` Even Though The Service Supports It |
| 38.1199 | `/api/v1/instance/search/status` Still Has No First-Party Client Surface |
| 38.1306 | Mobile Spotlight Result Selection Still Navigates Into Hidden Desktop Tabs |
| 38.1317 | Spotlight Still Falls Into A Blank Dead State For One-Character Queries ✅ |
| 38.1327 | Spotlight Still Collapses Real Search Failures Into “No Results Found” |

### B2 — Artifact Rendering (19 items)

| § | Title |
|---|-------|
| 38.34 | Notification Action Buttons Are Rendered But Never Execute |
| 38.461 | Chart Artifacts Are Still Rendered As A Placeholder Card Instead Of A Chart |
| 38.463 | DataFrame Artifact Sorting Still Breaks On Numeric Columns |
| 38.464 | Code Artifacts Still Promise A Monaco Viewer But Render As Static HTML Blocks |
| 38.470 | Math Artifacts Still Depend On A Missing `window.katex` Global |
| 38.471 | DataFrame Artifacts Still Drop Columns That Appear Only In Later Rows ✅ |
| 38.475 | Base64 Image Artifacts Still Assume Everything Is PNG |
| 38.480 | DataFrame Artifacts Still Promise Filtering / AG Grid-Like Inspection But Ship Only A Sortable Table |
| 38.481 | Table Artifacts Still Drop Empty Markdown Cells And Shift Column Alignment ✅ |
| 38.484 | JSON-Array Table Artifacts Still Drop Columns That Appear Only In Later Rows ✅ |
| 38.1286 | Artifact Copy Action Still Crashes When The Clipboard API Is Missing ✅ |
| 38.1293 | ArtifactRenderer Still Ignores `artifact.actions` From The Shared Contract ✅ |
| 38.1296 | Artifact Action Icons Still Ignore The Shared `icon` Field |
| 38.1297 | Terminal Artifacts Still Reinterpret A Real Final `exit N` Output Line As Metadata |
| 38.1300 | Image Artifact Lightbox Still Closes When The User Clicks The Enlarged Preview |
| 38.1305 | Artifact Action State Still Keys Off Non-Unique Labels |
| 38.1315 | Dev-Only `My Usage` Tab Still Renders Mock Totals Instead Of Live Usage Data |
| 38.1316 | Artifact Copy Success State Still Clears Early After Rapid Re-Copy ✅ |
| 38.1323 | Image Artifact Preview Still Opens The Lightbox Only For Pointer Users |

### B14 — Agent Tools & Skills (81 items)

| § | Title |
|---|-------|
| 38.305 | Tool Results And Live Task Events Still Have No Reliable Failure Path |
| 38.478 | Assistant Tool Calls Still Drop Arguments, Results, And Timing In Live Chat |
| 38.479 | Assistant Tool Calls Still Render As Green Successes Even When They Are Running Or Failed |
| 38.482 | Assistant Tool-Call Rows Still Depend On A Nonexistent `toolCalls` Field |
| 38.778 | Parallel `spawn_subagent` Budget-Exhaustion Errors Still Never Broadcast Live Tool Results |
| 38.783 | `track_pass_rate` Still Zero-Fills Required Counts And Can Record A Fake 100% `all_pass` ✅ |
| 38.796 | `track_pass_rate` Still Trusts Raw `scope_ref` And Can Escape The Workspace Root |
| 38.797 | `track_pass_rate` Still Interpolates Raw `test_command` Into YAML Frontmatter |
| 38.798 | `track_pass_rate` Still Bypasses Draft/CRDT Write Paths |
| 38.799 | `track_pass_rate` Still Accepts Arbitrary Target Files Instead Of Only Spec/Test-View Docs |
| 38.807 | `track_pass_rate` Still Accepts Impossible Test Count Combinations |
| 38.808 | `track_pass_rate` Still Advertises `failure_summary` But Drops It Entirely ✅ |
| 38.810 | `save_experience` Still Leaves `_index.md` Metadata Stale On Overwrite |
| 38.826 | `save_experience` Still Lets `_index` Clobber The Category Index Itself ✅ |
| 38.827 | `save_experience` Still Injects Raw Metadata Into `_index.md` |
| 38.846 | `list_directory` Still Leaks Hidden Trees When `include_hidden` Is False |
| 38.862 | `read_file` Still Bypasses The CRDT Document And Can Return Stale Base Files |
| 38.873 | `extract_interface` Still Advertises Tree-Sitter Accuracy It Does Not Provide |
| 38.874 | `extract_interface` Still Bypasses Draft And CRDT State |
| 38.877 | `read_file` Still Treats Invalid `offset` / `limit` As A Wider Read |
| 38.878 | `list_directory` Still Treats Invalid `max_depth` As Unbounded Traversal |
| 38.879 | `extract_interface` Cache Still Ignores The Requested `language` |
| 38.883 | `extract_interface` Still Returns Private Symbols Under A Public-Only Contract |
| 38.889 | `validate_spec` Still Reports Top-Level `valid: true` When Format Validation Failed |
| 38.890 | `extract_interface` Still Auto-Detects Go Files Without Any Go Extractor ✅ |
| 38.892 | `validate_spec` Still Treats Broken Dependency References As Valid |
| 38.894 | `extract_interface` Still Accepts Unsupported `language` Overrides As Success |
| 38.900 | `extract_interface` Still Leaves Every Class `methods` / `fields` List Empty |
| 38.908 | `extract_interface` Still Leaves Every Function `parameters` List Empty |
| 38.911 | `extract_interface` Still Flattens Class Methods Into Top-Level `functions` |
| 38.918 | `extract_interface` Still Ignores HTML `class="..."` Selectors It Claims To Surface |
| 38.924 | `extract_interface` Still Drops C `typedef struct` API Types Entirely |
| 38.930 | `extract_interface` Still Omits Rust `pub trait` / `pub type` Items Despite Claiming Type Definitions |
| 38.931 | `extract_interface` Still Never Surfaces Rust `pub use` Re-Exports |
| 38.932 | `extract_interface` Still Drops Exported JS/TS `interface` / `type` / `enum` Definitions |
| 38.937 | Prepopulation Still JSON-Encodes Plain-Text Tool Results Instead Of Injecting The Real Payload |
| 38.938 | `extract_interface` Still Mishandles Anonymous JS/TS Default Exports |
| 38.939 | `extract_interface` Still Treats Vue/Svelte Files As Bare HTML And Ignores Script APIs |
| 38.941 | `extract_interface` Still Misses Implicitly Public Java Interface Methods |
| 38.951 | `extract_interface` Still Auto-Detects `.h` Headers As C Instead Of C++ |
| 38.952 | `extract_interface` Still Drops Public Java Constructors |
| 38.953 | `extract_interface` Still Misses Implicitly Public Java Interface Constants |
| 38.958 | `extract_interface` Still Misses Or Misclassifies Common Java Public Type Forms |
| 38.959 | `extract_interface` Still Treats Exported JS/TS Arrow Functions As Constants |
| 38.960 | `extract_interface` Still Omits C/C++ Enum Type Definitions |
| 38.963 | `extract_interface` Still Drops Namespace-Scoped And Indented `extern "C"` Free Functions |
| 38.964 | `extract_interface` Still Misparses C/C++ Constants With Multi-Token Types |
| 38.966 | `edit_file_replace` And `edit_spec_replace` Still Accept Empty `old_string` |
| 38.969 | `extract_interface` Still Misclassifies Sass `@forward` As An Import |
| 38.970 | `extract_interface` Still Misses Public Rust Functions With Common Qualifiers |
| 38.971 | `extract_interface` Still Misses JS/TS Symbols Exported Via Later Export Lists |
| 38.972 | `extract_interface` Still Drops C/C++ Public Type Aliases Outside `typedef struct` |
| 38.974 | `extract_interface` Still Misparses Java `public static final` Constants With Rigid Spacing Assumptions |
| 38.975 | `extract_interface` Still Hardcodes LESS Mixin Detection To `.mixin...` |
| 38.976 | `extract_interface` Still Misparses C/C++ Reference-Return Function Names |
| 38.977 | `extract_interface` Still Omits Rust `pub union` Type Definitions |
| 38.978 | `extract_interface` Still Omits Rust `pub mod` Module Exports |
| 38.979 | `extract_interface` Still Misses Named JS/TS Default-Export References |
| 38.981 | `list_directory` Still Returns Raw WalkDir Order With No Deterministic Sorting ✅ |
| 38.982 | `extract_interface` Still Ignores Exported JS/TS `let` / `var` Bindings |
| 38.983 | `extract_interface` Still Misses Single-Line CSS Selector Rules |
| 38.984 | `extract_interface` Still Misses Annotated Python Module Constants |
| 38.985 | `extract_interface` Still Misparses Rust `pub static mut` Names |
| 38.986 | `extract_interface` Still Omits C/C++ `union` Type Definitions |
| 38.987 | `extract_interface` Still Drops JS/TS Default Exports Of Expression Forms |
| 38.990 | `list_directory` Still Swallows Traversal Errors And Returns Partial Success |
| 38.992 | `extract_interface` Still Ignores Valid Unquoted HTML Attribute Values |
| 38.993 | `create_spec` And `edit_spec_replace` Still Accept Ontology-Invalid Spec Content |
| 38.997 | `extract_interface` Still Drops Python Class Methods Entirely |
| 38.998 | `extract_interface` Still Drops Ruby Class/Module Constants |
| 38.999 | `read_file` Still Treats A Slice Read As Reading The Whole File |
| 38.1001 | `extract_interface` Still Ignores CommonJS Export Forms |
| 38.1002 | `extract_interface` Still Ignores Kebab-Case HTML Component Tags |
| 38.1004 | `create_spec` Still Overwrites Existing Specs Under A “Create” Contract |
| 38.1007 | `merge_spec_context_into_content()` Still Writes Internal Frontmatter Keys To Disk |
| 38.1009 | `validate_spec` Still Misclassifies Markdown Thematic Breaks As Setext Headings |
| 38.1015 | `save_experience` Still Never Enforces Its Snake-Case Filename Contract |
| 38.1016 | `track_pass_rate` Still Treats Unterminated Frontmatter As A Fresh File | ✅ |
| 38.1027 | Chat Terminal Tools Still Depend On A Hidden `assistant_message_id` Argument |
| 38.1169 | Chat Terminal Tools Still Never Use The Session-Level `complete-pending` Recovery Route |
| 38.1171 | Chat Terminal Tools Still Drop Token Usage When Completing Replies |

### B15 — g5d CLI (31 items)

| § | Title |
|---|-------|
| 38.8 | Configured Isolation Mode Can Silently Degrade To Unconfined Process Execution |
| 38.499 | `g5d push` Still Swallows Docs Push Failures And Records The Docs SHA Anyway |
| 38.511 | `g5d init` Still Creates `.g5/docs` Without Recording Docs Config |
| 38.518 | `g5d merge` Still Reports “Added” Repos Without Actually Adding Them |
| 38.524 | `g5d merge` Still Silently Drops Missing Or Unreadable Repos From The Merge Set |
| 38.525 | Remote-Source `g5d merge` Still Drops Source-Only Repos Before Planning |
| 38.526 | `g5d merge --dry-run` Still Mutates Local Repos |
| 38.531 | `g5d fork` Still Writes Into Existing Target Directories Without Guarding |
| 38.538 | `g5d init` Still Reports Success After Docs Bootstrap Failures |
| 38.541 | `g5d doctor` Still Assumes Copy-On-Write Support On macOS |
| 38.552 | `g5d revert` Still Reports Success After Silently Skipping Broken Repos |
| 38.557 | `g5d conflicts --accept-local` Still Resolves Only The Filesystem Copy |
| 38.569 | `g5d` Still Advertises Isolation Exit Code `5` But Never Emits It | ✅ |
| 38.577 | `g5d commit` Still Reports Docs-Only Commits As “No Changes” |
| 38.586 | `g5d commit --all` Still Does Not Change Commit Selection Behavior |
| 38.587 | `g5d refresh --pull-images` Still Exists As A Dead Flag |
| 38.588 | `g5d merge` Still Prints JSON In Human Mode And Ignores `--quiet` |
| 38.589 | `g5d doctor` Still Exits Successfully Even When Checks Fail |
| 38.590 | `g5d doctor` Still Checks The Wrong Cache Directory |
| 38.594 | `g5d doctor` Still Flags Missing MicroVM Support Even When `process` Mode Is Explicitly Configured |
| 38.595 | `g5d doctor` Still Claims It Is “Using Process Isolation” Even Though MicroVM Mode Now Fails Closed |
| 38.596 | `g5d doctor` Still Treats Every Supported Language Runtime As Mandatory |
| 38.605 | `g5d add` Still Treats `--name` As An Unvalidated Filesystem Path Segment |
| 38.606 | `g5d conflicts --accept-remote` Still Deletes The Sidecar Without Verifying The Remote Version Is On Disk |
| 38.607 | `g5d conflicts` Still Accepts Contradictory Action Flags And Silently Picks One |
| 38.610 | `g5d refresh` Still Replays Local Commits Onto Whatever Target The New Manifest Names |
| 38.611 | g5d Still Advertises Browser-Facing `/api/v1/instances/*` Routes That The Live Router Never Mounts |
| 38.701 | `g5d push` Still Treats Missing Or Corrupt `.g5/docs` As A No-Op Success |
| 38.703 | `g5d conflicts --accept-local` Still Trusts Raw Paths And Can Escape The Conflict Tree |
| 38.704 | `g5d conflicts --clear` Still Trusts Sidecar `.meta.path` When Deleting Files |
| 38.705 | Explicit-Target `g5d push` Still Leaves Local `origin` Remotes Pointing At The Destination |

### B16 — UI Polish, Comments, Notifications & Misc (191 items)

| § | Title |
|---|-------|
| 38.14 | File Tree Route Previously Hid Backend Failures As Empty Directories |
| 38.19 | File Content Hook Drops Binary Encoding Metadata |
| 38.27 | Comment List Pagination Parameters Are Ignored |
| 38.45 | Notification Read State Is Derived From The Wrong Field ✅ |
| 38.54 | Doc Line Comments Are Effectively Pinned To `main` ✅ |
| 38.56 | `useCreateJob()` Assumes The Wrong Response Contract ✅ |
| 38.62 | Mention Notifications Can Never Reach The Dedicated "Mentions" Group ✅ |
| 38.67 | The Members Settings Route Is Not Actually Admin-Gated |
| 38.73 | Spend History Still Skips Its Intended `billing:admin` Gate ✅ |
| 38.74 | Negative `periods` Values Can Blow Up Spend-History Requests ✅ |
| 38.93 | Slug Resolver Still Enumerates Foreign Orgs And Repos |
| 38.99 | Error-Boundary Recovery Buttons Are Still Inert |
| 38.103 | Blocked And Failed Tasks Claim To Preserve Partial Results But Drop Them On Cleanup |
| 38.115 | Notification Read State Still Uses Local Heuristics Instead Of The Server Field |
| 38.117 | The Settings Panel Still Never Syncs User Preferences To The Real Backend API |
| 38.121 | `PATCH /api/v1/orgs/:orgId/members/:userId` Still Returns A 500 For Invalid Non-Owner Roles |
| 38.125 | Completed Tasks Can Still Report Success After Silent Collect Failures |
| 38.130 | `transition_task_state(\"reviewing\")` Is A False-Success Stub | ✅ | 38.139 | The Instance Health Hook Still Assumes An Envelope The Backend Does Not Return |
| 38.141 | Health-Check Uptime Still Starts At First Probe Instead Of Process Start ✅ |
| 38.152 | Blocked Tasks Can Strand Jobs In `running` With No Runnable Work |
| 38.153 | `cancel_job()` Still Leaves `waiting` And `paused` Tasks Alive Under Cancelled Jobs | ✅ | 38.157 | Newly Opened Doc Tabs Still Leak Opaque UUIDs As Titles |
| 38.166 | Runtime-Config Save Failures Still Vanish Without Inline Feedback |
| 38.174 | File-Watch Hook Still Cannot Ever Report Changed Files |
| 38.182 | Doc Link Mutations Still Succeed After Silently Dropping Bad Targets |
| 38.205 | The Global Error-Boundary Recovery Actions Are Still Non-Functional Placeholders |
| 38.211 | Future Views Are Still Advertised As Implementable Without Any End-To-End Implementation Path |
| 38.222 | Create-Doc Dialog Can Be Dismissed Mid-Create And Still Open A New Tab Later |
| 38.228 | Canonical `views/<slug>.md` Paths Still Collapse Same-Name Views Across Personas And Future State |
| 38.234 | Comment Creation Still Publishes `comment.new` Before The Transaction Commits |
| 38.236 | Notification Delivery Still Publishes Websocket Events Before The Transaction Commits |
| 38.267 | The Standalone `/settings` Route Is Still A Mostly Static Shell Instead Of The Actual Settings Surface |
| 38.264 | The Inventory Still Reuses Finding IDs, Making Closure And Count Tracking Ambiguous |
| 38.276 | Raw File Tabs Still Present An Editable Editor With No Persistence Path |
| 38.295 | Bottom-Drawer `initialTab` Deep Links Still Do Not Apply After Mount |
| 38.299 | Comment Reply/Resolve Hooks Still Throw Away The Server-Returned Reply And Comment DTOs |
| 38.302 | Notification Read Hooks Still Throw Away The Server Acknowledgement Payloads |
| 38.309 | Return Control Still Exits Takeover Mode Even When Resume Fails | ✅ | 38.315 | Terminal Panel Still Shows A Live Cursor For Finished Tasks |
| 38.316 | Terminal Output Still Forces Auto-Scroll Even When The User Scrolls Up |
| 38.318 | Create-Doc Still Downgrades The API's `DocDetailDTO` Response To `DocNode` |
| 38.324 | `useUser()` Still Promises `UserRef` Fields The Public User API Never Returns ✅ |
| 38.347 | `useNodeContent()` Still Downgrades `NodeContentResponse` To A Bare String ✅ |
| 38.348 | `useNodeDeps()` Still Expects `deps` Even Though The API Sends `dependencies` | ✅ |
| 38.359 | Spend Breakdown Still Has No Real-Time Cost Refresh Path |
| 38.363 | `useSpecFormatValidation()` Still Expects The Inner Result Instead Of The Real Response DTO ✅ |
| 38.364 | `useUncertaintySummary()` Still Reads The Wrong Level Of The Response Contract |
| 38.372 | Concurrent Channel Subscriptions Can Still Race `WebSocketManager.connect()` Into Overlapping Sockets |
| 38.383 | Central Route Registry Still Omits Several Live Frontend Route Families |
| 38.384 | Route-Contract Test Still Gives False Confidence About Hook Coverage |
| 38.385 | Runtime-Config Hooks Still Have No Dedicated Contract Or Shared-MSW Coverage |
| 38.392 | Notification Listing Still Lacks Lower-Bound Validation For `limit` ✅ |
| 38.403 | Raw File Tabs Still Never Load Their Existing Line Comments |
| 38.405 | Public Line Comments Still Are Not Actually Commit-Anchored |
| 38.406 | DiffView Add-Comment Buttons Still Submit Blank Comment Bodies |
| 38.407 | Line-Comment Optimistic State Still Only Works For Node Comments |
| 38.413 | Comment Resolve/Delete Still Have No Live Event Fanout |
| 38.420 | Bootstrapped Docs Repos Are Marked As Tracking But Have No Fetchable `origin` |
| 38.423 | SpecEditor Outline Toggle Still Only Changes Button Color |
| 38.424 | SpecEditor Block-Insertion Controls Still Do Not Insert Blocks |
| 38.425 | SpecEditor Undo/Redo Buttons Are Still Clickable No-Ops |
| 38.431 | Local `g5-user-settings` Still Bleed Across Accounts On Shared Browsers |
| 38.434 | `/login` Dev Login Still Ignores The API Base URL And Hard-Resets To `/` |
| 38.436 | `SpecEditor` Global Shortcut Handling Still Rewrites Unrelated Inputs |
| 38.437 | `SpecEditor` Still Advertises List Formatting But Never Implements It |
| 38.438 | `SpecEditor` Right-Click "Create" Actions Still Close As No-Ops |
| 38.441 | Notification Banner Surfaces Are Defined But Never Mounted |
| 38.442 | `/ide` Dev Route Still Ships An Interactive Mockup After IDE Handoff Was Meant To Be Removed |
| 38.450 | `SpecEditor` Text-Selection Toolbar Is Dead Scaffolding |
| 38.458 | `/login` Still Falls Through To A Blank Sign-In Screen When Instance Info Fails |
| 38.473 | Login Return State Still Drops Query And Hash Information |
| 38.535 | `parse_receive_pack_ref_updates()` Still Silently Drops Malformed Update Lines |
| 38.556 | Remote Merge-By-Name Still Uses Exact Case-Sensitive Matching |
| 38.709 | Data-Channel Connect Helpers Still Return The Wrong WebSocket URL Shape |
| 38.738 | Internal File MIME Detection Still Misses Extensionless Text Files Like `Dockerfile` And `Makefile` ✅ |
| 38.743 | Browser Tree Route Still Documents Unlimited Depth But Hard-Defaults To 20 ✅ |
| 38.745 | Connection Brokering Still Drops Caller Identity Before Ticket Minting |
| 38.749 | Sub-Agent Runs Still Discard Their Detailed Tool/Dialog Audit Trail |
| 38.754 | `invoke_skill` Still Ignores Declared Structured Output Schemas |
| 38.756 | Browser Tree Still Turns “Path Points To A File” Into A 500 |
| 38.760 | WebSocket Slow-Client Protection Still Stops Working After The First Window |
| 38.761 | Redis-Backed WebSocket Fanout Still Double-Delivers Origin Events On The Publishing Instance |
| 38.762 | The Redis WebSocket Listener Still Dies Permanently On The First Pub/Sub Drop |
| 38.763 | Browser Cache Recovery Still Depends On WebSocket Events The Backend Never Emits |
| 38.764 | `spawn_subagent` Still Ignores Its Advertised `context` Parameter |
| 38.765 | Browser Tree Targeted Subtree Requests Still Bypass Hidden/Build-Directory Suppression |
| 38.766 | `invoke_skill` Still Never Validates Arguments Against The Declared `input_schema` |
| 38.767 | Invoke-Mode Skills Still Do Not Receive Their Allowlisted Tool Definitions |
| 38.769 | Skill Router Still Has No Production Wiring |
| 38.770 | Parallel `spawn_subagent` Still Exposes `trigger_workflow` Unlike The Single-Call Path |
| 38.772 | Skill Router Still Never Enforces Its Own Confidence Threshold |
| 38.774 | Delegation Tools Still Ignore Their Own Top-Level Required Parameters |
| 38.776 | `SubAgentRequest` Still Advertises Control Fields The Runtime Never Consumes |
| 38.777 | Invoke-Mode Skill Runs Still Have No Live Or Durable Transcript Surface |
| 38.779 | Empty Skill `tool_allowlist` Still Means “No Tools” Instead Of “All Tools” |
| 38.780 | `trigger_workflow` Still Reports Success With `"unknown"` IDs On Malformed 2xx Responses |
| 38.790 | `view_management` Still Cannot Call The `trigger_workflow` Tool It Is Defined Around |
| 38.791 | Sub-Agent `structured_output` Still Carries Telemetry And Gets Dropped Before Parent Use |
| 38.792 | `experience_capture` Still Cannot Perform Its Prompted Duplicate-Update Path |
| 38.793 | Sub-Agent Prompting Still Never Asks For The Structured Result Block The Runtime Expects |
| 38.794 | `transition_task_state` Still Fail-Opens Missing Explanation Fields With Placeholder Text |
| 38.795 | Skill Manifests Still Collapse `when_to_use` Into The Generic Description |
| 38.802 | `experience_capture` Still Exposes A `technical` Category Its Tool Schemas Hide |
| 38.804 | `list_experience` Still Never Returns The Titles And Keywords It Advertises |
| 38.805 | `search_experience` Still Double-Counts Entries By Returning `_index.md` As A Real Match ✅ |
| 38.806 | `debug_tests.analyze_only` Still Has No Runtime Read-Only Enforcement |
| 38.809 | Experience Tools Still Promise A Root `_index.md` That No Code Maintains |
| 38.811 | Experience Tool Responses Still Leak The On-Disk `anti_patterns` Name ✅ |
| 38.820 | `trigger_workflow` Still Accepts Zero Or Negative `review_iterations` |
| 38.828 | `search_experience` Still Treats Malformed Category Input As `"all"` |
| 38.834 | `git_push` Still Reports Success When The Remote Rejects The Push |
| 38.837 | `git_diff` Still Ignores Its Declared `staged` Parameter ✅ |
| 38.841 | `transition_task_state("blocked" / "fail")` Still Reports Success On HTTP Error Responses |
| 38.844 | `delete_file` Still Replicates Deletions As Empty Files |
| 38.847 | `run_in_terminal` Background Mode Still Has No Usable Output Or Completion Retrieval Path |
| 38.853 | `view_management` Still Tells The User-Detached Skill Loop To Ask For Clarification |
| 38.858 | Background Terminal Output Still Stalls Once The PTY Reader Queue Fills |
| 38.860 | Terminal EOF Handling Still Mislabels Non-Zero Shell Exit As `Completed` ✅ |
| 38.866 | `run_in_terminal` Output Truncation Still Panics On Multi-Byte UTF-8 ✅ |
| 38.870 | Terminal Catch-Up Still Silently Drops Scrollback On Contention |
| 38.875 | VM-Isolated `run_in_terminal` Still Misinterprets `cwd` As The Guest Mount Point |
| 38.876 | Tool Registry Still Never Enforces Tool Parameter Schemas |
| 38.880 | `spec_path()` / `view_path()` Still Trust Raw `component_id` Values As Filesystem Paths |
| 38.881 | `list_specs` Still Widens Invalid `pattern` Input To “All Specs” ✅ |
| 38.882 | `build_tasks_from_view_frontmatter()` Still Trusts Raw `view_file_path` |
| 38.887 | `evaluate_relevance` Still Returns Instructions Instead Of A Relevance Decision |
| 38.888 | `classify_future_action` Still Returns Comparison Context Instead Of A Classification |
| 38.893 | `git_status` / `git_diff` Still Flatten Inspection Failures Into A Clean Working Tree |
| 38.903 | `semantic_search` Still Accepts `max_results = 0` And Returns Empty “Truncated” Output ✅ |
| 38.904 | `git_commit` Still Creates Empty Checkpoint Commits On A Clean Tree |
| 38.905 | `git_fetch` Still Ignores `FetchOutcome` And Always Claims A Fresh Fetch |
| 38.910 | `list_specs` Still Leaks Absolute On-Disk Paths In Its Result Payload ✅ |
| 38.914 | `semantic_search` Still Reports Malformed `file_pattern` Globs As Internal Errors |
| 38.920 | `list_specs` Still Doesn't Provide The Names / Types Prepopulation Says It Does |
| 38.921 | `detect_fs_changes()` Still Never Propagates File Deletions |
| 38.928 | `search_experience` Still Accepts Empty Queries And Matches Every Markdown File ✅ |
| 38.943 | Inbound Materialization Failures Still Bypass `failed_writes` And `is_clean()` |
| 38.946 | `search_experience` Still Returns Category `_index.md` Sidecars As User-Facing Matches |
| 38.947 | `search_experience` Still Computes Snippet Windows From A Lowercased Surrogate String |
| 38.949 | `SyncEngine::flush_outbound()` And `stop()` Still Drop Queued Deltas Instead Of Sending Them |
| 38.954 | Conflict Detection Still Fail-Opens When Known State Is Missing Or The File Was Deleted Locally |
| 38.965 | `write_file` Still Leaves Freshly Written Files “Unread” For `edit_file_replace` |
| 38.967 | File Mutation Tools Still Return Error After The Local Mutation Already Happened |
| 38.968 | `edit_file_replace` Still Lossily Rewrites Non-UTF-8, NUL-Free Files |
| 38.995 | `list_specs` Still Returns Raw WalkDir Order With No Deterministic Sorting |
| 38.1020 | Generic Doc Create/Update Still Accept Blank Or Whitespace-Only Names ✅ |
| 38.1021 | Generic Doc Create/Update Still Surface Invalid `doc_type` As A DB Error Instead Of Validation ✅ |
| 38.1030 | `PUT /docs/:id` Still Treats An Empty Body As A False-Success No-Op |
| 38.1033 | `POST /docs` Still Creates “File-Backed” Docs Without Writing The File |
| 38.1035 | Top-Level Comment Create/Update Still Accept Blank Or Whitespace-Only Content |
| 38.1038 | `PATCH /comments/:id` Still Treats An Empty Body As A False-Success Mutation |
| 38.1047 | Comment Creation Still Accepts Hybrid Node/Line Anchors |
| 38.1048 | Line-Scoped Comment Creation Still Accepts Blank Or Impossible Anchors |
| 38.1052 | `DELETE /api/v1/comments/:id` Still Has No Live Client Entry Point |
| 38.1059 | Collapsed Comment Cards Still Drop Known Reply Counts |
| 38.1073 | User Preference PATCH Still Treats Empty Bodies As Successful Updates | ✅ |
| 38.1078 | Playwright Flow Specs Still Fail-Open Behind `softVisible` Early Returns |
| 38.1080 | Dev-Mode Notification And Budget Preferences Still Persist As Inert Local Settings |
| 38.1082 | Unanchored Node Comments Still Get Fake Line/Block Anchors In Editors |
| 38.1083 | Main Preferences Controls Beyond Theme/Mode Still Do Not Affect Runtime Behavior |
| 38.1084 | First-Login Users Still Cannot Reach Their Own Preferences API |
| 38.1089 | Mark-In-Review Still Records No Reviewer Identity Or Claim Timestamp |
| 38.1091 | The Dedicated `/settings` Route Still Ships Placeholder Panels Instead Of Live Settings UI |
| 38.1102 | User Account Linking Still Treats Email Identity As Raw Case-Sensitive Text |
| 38.1104 | `switchOrg()` Still Reports Success After The Profile Reload Fails |
| 38.1132 | Shared Internal Resolve Client Still Discards The Endpoint's Detail Payload |
| 38.1174 | Spend History Still Accepts Arbitrary `granularity` Strings And Labels The Fallback As If It Were Real |
| 38.1183 | Spend History Still Has No Upper Bound On Positive `periods`, So One Request Can Drive Huge Preallocation And N+1 Aggregate Queries |
| 38.1188 | Runtime-Config Update Routes Still Accept Empty Bodies As False-Success Mutations |
| 38.1190 | `/api/v1/instance/info` Still Hardcodes `maxUploadSizeMb` And `motd` |
| 38.1191 | `InstanceInfoDTO.features` And `motd` Still Have No Runtime Consumer In The Client |
| 38.1192 | `PATCH /api/v1/users/me` Still Accepts Blank `avatarUrl` Strings ✅ |
| 38.1195 | `InstanceInfoDTO.features` Still Hardcodes Core Capabilities Instead Of Reporting Real Runtime Feature Flags |
| 38.1198 | `useInstanceHealth()` Still Has No Runtime Consumer Outside Tests |
| 38.1200 | `/api/v1/instance/health` Still Cannot Signal Hard Failure To Probes |
| 38.1205 | The Promised Spend History Chart Still Has No Runtime Surface |
| 38.1212 | `TaskTransitionResponse.hasMoreWork` Still Reflects Global Pending Tasks |
| 38.1220 | Internal `task_status` Alias Still Accepts Full Transition Payloads |
| 38.1221 | Internal Usage Recording Still Accepts Impossible Negative Cache And Duration Values |
| 38.1232 | Internal `upload_file` Still Pretends To Upload File Bytes It Never Stores |
| 38.1233 | Internal `upload_file` Still Collapses File Identity To The Basename |
| 38.1234 | Internal `update_staleness` Still Reports Missing Node Updates As Success |
| 38.1248 | Internal Staleness Updates Still Treat Invalid `syncStatus` Values As Internal Errors |
| 38.1277 | Notification `mark_read` Still Treats Already-Read Rows As `NotFound` |
| 38.1290 | Closed Settings Drawer Still Leaves A Hidden Interactive Surface In The Tab Order |
| 38.1291 | Settings Drawer Tabs Still Advertise ARIA Tab Semantics Without A Real Tab Pattern |
| 38.1299 | Mobile Notifications Bell Still Opens No Notification Panel |
| 38.1301 | Settings Drawer Still Lacks Real Modal Focus Isolation When Open |
| 38.1314 | Production Still Has No Real First-Party `My Usage` Surface |
| 38.1326 | NotificationPanel Still Defines A `Comments` Group The Shared Notification Contract Can Never Populate ✅ |
