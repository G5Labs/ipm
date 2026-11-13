# E2E Critical-Path Batch — "Wire It End-to-End"

Cherry-picked from B3, B4, B7, B8, B9, B12 (and cross-cutting items from B6/B11).
Completing this batch makes every core product flow work from org creation through
uplift, without touching items that are polish, edge-case hardening, or CLI-only.

> **Scope**: Happy-path blockers & critical correctness issues only.
> Items that are nice-to-have, affect only edge cases, or only affect g5d CLI
> without a browser-facing consequence are deliberately excluded.

---

## Summary

| Phase | Flow | Items | Source Batches |
|-------|------|------:|----------------|
| 0 | Write-Path DB Session Fixes | 7 | B12, B6 |
| 1 | Org Creation & Bootstrap | 11 | B4, B8, B12 |
| 2 | Multi-Org Navigation & Switching | 9 | B4, B12 |
| 3 | Workspace & Project Setup | 10 | B3, B8 |
| 4 | View & Doc Authoring | 13 | B7, B3 |
| 5 | CRDT & Draft Editing | 11 | B9 |
| 6 | Downlift Pipeline | 15 | B7 |
| 7 | Uplift & Ontology | 11 | B7, B8 |
| | **TOTAL** | **87** | |

---

## Phase 0 — Write-Path DB Session Fixes

**Why first**: Multiple route handlers create read-scoped database sessions for
endpoints that perform writes. Every write-mutation in Phases 1-7 is potentially
dead-on-arrival until these are fixed. This is the single highest-leverage phase.

| § | Title | Batch | Status |
|---|-------|-------|--------|
| 38.1138 | Org Admin Mutation Routes Still Mint Read-Scoped DB Sessions | B12 | ✅ |
| 38.1170 | Public Doc Mutation Routes Still Mint Read-Scoped DB Sessions | B12 | ✅ |
| 38.1179 | Draft Mutation Routes Still Mint Read-Scoped DB Sessions | B9 → B12 | ✅ |
| 38.1186 | Workspace Mutation Routes Still Mint Read-Scoped DB Sessions | B12 | ✅ |
| 38.1143 | Public Chat Mutation Routes Still Mint Read-Scoped DB Sessions | B12 | ✅ |
| 38.1163 | Workspace Settings Mutation Route Still Mints A Read-Scoped DB Session | B12 | ✅ |
| 38.1149 | Task Intervention Routes Still Mint Read-Scoped DB Sessions | B6 | ✅ |

---

## Phase 1 — Org Creation & Bootstrap

**Flow**: User clicks "Create Org" → wizard completes all 4 steps → org, docs repo,
base workspace, docs branch all exist and are consistent.

| § | Title | Batch | Status |
|---|-------|-------|--------|
| 38.97 | Org Create/Update Still Accept Arbitrary Caller-Supplied Slugs | B4 | ✅ |
| 38.214 | Create-Org And Create-Workspace APIs Still Accept Whitespace-Only Names | B4 | ✅ |
| 38.452 | Create-Org Clears Tenant Context Before It Updates The Bootstrapped Docs Head | B4 | ✅ |
| 38.120 | Org Creation Still Returns A Docs Repo `gitUrl` That Does Not Match The Bootstrapped Repo Path | B8 | ✅ |
| 38.455 | Normal Workspace Creation Still Wires A Docs Repo Without Creating The Docs Branch Row | B8 | ✅ |
| 38.215 | Org Creation And Workspace Clone Still Perform Local Git Side Effects Before The Database Commit | B8 | |
| 38.493 | `create_workspace()` Still Seeds Docs Metadata Independently Of The Actual Docs Branch Spec | B8 | ✅ |
| 38.95 | SCM Connect From The Create-Org Wizard Cannot Complete In The Browser | B4 | |
| 38.114 | Create-Org SCM Connect Still Leaves Step 2 Stuck On Stale Provider State After Return | B4 | |
| 38.96 | Org Invite Flows Accept And Even Advance On Invalid Email Input | B4 | ✅ |
| 38.1320 | Create-Org Wizard Still Lets Back/Next/Skip Run During Import And Invite Mutations | B4 | ✅ |

---

## Phase 2 — Multi-Org Navigation & Switching

**Flow**: User with multiple orgs can switch between them; JWT, routes, and
React Query caches all reflect the correct org.

| § | Title | Batch | Status |
|---|-------|-------|--------|
| 38.10 | `/projects` Only Loads Workspaces For The First Org | B4 | ✅ |
| 38.30 | Root And Settings Navigation Still Hardcode The First Org | B4 | ✅ |
| 38.167 | The Projects Page Still Creates New Workspaces In The Oldest Org With No Org Picker | B4 | ✅ |
| 38.1094 | Explicit Org Switch Still Leaves The Shared `current-user` Query Stale | B4 | ✅ |
| 38.1106 | Projects Page Still Offers "New Project" Even When There Is No Org To Create Into | B4 | ✅ |
| 38.181 | WorkspacePage Still Falls Back To Route Slugs For Org-Scoped Write Dialogs | B4 | ✅ |
| 38.180 | Cross-Org Client Navigation Still Never Refreshes JWT Org Context | B12 | ✅ |
| 38.1099 | `CurrentUserDTO` Still Omits Active-Org Identity While Ordering Memberships Opposite To Session Auth | B12 | ✅ |
| 38.1100 | Refresh-Token Fallback Still Pins Stale Org Context Instead Of Re-Resolving Current Memberships | B12 | ✅ |

---

## Phase 3 — Workspace & Project Setup

**Flow**: User creates a workspace from the projects page → navigates into it →
workspace has correct docs repo, branch, and settings.

| § | Title | Batch | Status |
|---|-------|-------|--------|
| 38.9 | `/projects` Create-Workspace Navigation Uses the Wrong Identifier | B3 | ✅ |
| 38.6 | Workspace Creation Needs Safer Partial-Failure Recovery | B3 | ✅ |
| 38.100 | Invalid Workspace Slugs Never Surface A Not-Found State | B3 | ✅ |
| 38.196 | Base-Workspace Navigation Still Hardcodes `/base` Instead Of The Real Base Workspace Identity | B3 | ✅ |
| 38.265 | Workspace Mutation Hooks Still Model Create/Update Responses As `WorkspaceSummary` Instead Of The Live Detail DTO | B3 | ✅ |
| 38.150 | `CreateDocDialog` Still Persists A Shared Bare `#!internal-workspace-docs` Path | B3 | ✅ |
| 38.492 | Workspace Branch Attachment Still Defaults To Literal `main` Instead Of The Repo's Real Default Branch | B8 | ✅ |
| 38.488 | Branch Creation Still Accepts Nonexistent Base Branches And Reports Success With A Zero Head | B8 | ✅ |
| 38.1136 | Newly Added Repos Still Start With An All-Zero `head_commit` | B8 | ✅ |
| 38.469 | Workspace Switcher Still Counts Branches As Repositories | B8 | ✅ |

---

## Phase 4 — View & Doc Authoring ✅

**Flow**: User creates a view (scoped, future, or generic) from the sidebar →
view doc written to correct path → linked docs resolved → view list/detail works.

| § | Title | Batch | Status |
|---|-------|-------|--------|
| 38.132 | Create View Still Persists Linked Doc IDs In The `linked_doc_paths` Field | B7 | ✅ |
| 38.199 | View Creation Still Serializes Linked Doc IDs Into `linkedDocPaths` | B7 | ✅ |
| 38.319 | The Generic `New View` Dialog Still Creates A Plain `design` Doc Instead Of A Real View | B7 | ✅ (already fixed) |
| 38.332 | Scoped View Creation Is Still Unreachable From The Live Workspace UI | B7 | ✅ |
| 38.204 | Future-View Creation Still Drops The Persona And Filename Chosen In `CreateProjectDocModal` | B7 | ✅ |
| 38.238 | View Doc-Type Taxonomy Still Diverges Between Frontend Types And Backend Canonical Values | B7 | ✅ (already fixed) |
| 38.227 | View File-Path Naming Still Disagrees Across Browser Preview, Tooling, And Backend Canonical Storage | B7 | ✅ |
| 38.1013 | View Creation Still Accepts Empty Or Punctuation-Only Names And Collapses Them To `views/.md` | B7 | ✅ |
| 38.333 | The Public API Still Has No Dedicated View List/Detail Contract | B7 | ✅ |
| 38.1260 | Doc/View Create And Delete Still Have No Cross-Client Live Refresh Path | B7 | ✅ |
| 38.1066 | View Create/Sync Completions Still Leave Open Doc Tabs Stale | B7 | ✅ |
| 38.156 | Center-Panel Navigation Still Conflates File Paths, Doc IDs, And Ontology Node IDs | B7 | ✅ |
| 38.253 | Views Sidebar Still Treats All Non-Internal Docs As View Docs | B7 | ✅ |

---

## Phase 5 — CRDT & Draft Editing

**Flow**: User opens a doc → CRDT connection established → edits sync to server →
draft state reported correctly → commit draft persists changes.

| § | Title | Batch | Status |
|---|-------|-------|--------|
| 38.277 | File Content Still Hardcodes `draftDirty = false`, So The DTO Lies About Live Draft State | B9 | ✅ |
| 38.356 | CRDT Connection Setup Still Hardcodes Same-Origin Host And Ticket Paths | B9 | ✅ |
| 38.265 | `GET /api/v1/branches/:branchId/drafts` Still Auto-Creates Active Drafts On Read Scope | B9 | ✅ |
| 38.863 | Spec/View Read Tools Still Bypass Draft And CRDT State | B9 | ✅ |
| 38.864 | Spec Write Tools Still Report Success After Local-Only Writes When CRDT Sync Fails | B9 | ✅ |
| 38.865 | View Artifact Emitters Still Report Success After Local-Only Writes When CRDT Sync Fails | B9 | ✅ |
| 38.935 | Cold-Start CRDT Materialization Still Writes Only Dirty Files | B9 | ✅ |
| 38.936 | The CRDT Sync Engine's Periodic Pull Fallback Is Still A Stub | B9 | ✅ |
| 38.933 | Inbound CRDT Materialization Still Ignores Remote File Deletions | B9 | ✅ |
| 38.944 | Inbound CRDT Updates Can Still Be Dropped When The Materializer Queue Is Full | B9 | ✅ |
| 38.945 | Initial And Reconnect CRDT Handshakes Still Succeed After Failed State Application | B9 | ✅ |

---

## Phase 6 — Downlift Pipeline

**Flow**: User clicks "Sync" on a view → downlift runs → Phase B parses view
correctly → specs created/updated → codegen produces source files.

| § | Title | Batch |
|---|-------|-------|
| 38.59 | The Sync Modal's "Sync" Choice Is Still A Downlift Placeholder | B7 |
| 38.186 | View Downlift Still Depends On `linked_nodes` Frontmatter That The View Authoring Flow No Longer Guarantees | B7 |
| 38.187 | Direct View Downlift Still Looks For View Files In The Wrong On-Disk Location | B7 |
| 38.70 | Cascading Downlift Treats Missing Target Specs As A Successful No-Op | B7 |
| 38.78 | "Full" Downlift Only Walks The First Ontology Root | B7 |
| 38.169 | Dedicated View Sync Still Drops The View's Scope Before Bootstrap | B7 |
| 38.155 | View Sync Phase B Still Treats Missing `relevant_specs` As A Successful No-Op | B7 |
| 38.254 | Phase B Still Cannot Find The Default Numbered `## 4. Implementation Mapping` Heading | B7 |
| 38.256 | Existing-Spec View Sync Still Silently Drops Codegen When `source_files` Frontmatter Is Missing Or Malformed | B7 |
| 38.273 | Existing-Feature View Mappings Still Use A Table Shape Phase B Parses Against The Wrong Columns | B7 |
| 38.274 | Future-Feature View Templates Still Omit The File Paths Phase B Needs For New-Spec Codegen | B7 |
| 38.891 | Phase B Still Coerces Malformed `new_specs` / `relevant_specs` Frontmatter Into A Successful No-Op | B7 |
| 38.275 | View-Sync Codegen Still Collides When Two Specs Target The Same Source File | B7 |
| 38.1006 | Prompted "Exact Frontmatter" Still Uses `src_files` Where Phase B Reads `source_files` | B7 |
| 38.303 | Unreadable `semlift.yaml` Still Silently Re-Enables Fan-Out Sync And Codegen | B7 |

---

## Phase 7 — Uplift & Ontology

**Flow**: Agent produces code changes → scoped uplift runs → ontology upsert
correctly creates/updates nodes → tree reflects the new structure → file/spec
reads respect the correct branch.

| § | Title | Batch |
|---|-------|-------|
| 38.69 | Scoped Uplift Silently Degrades When Ontology Upsert Mapping Is Missing | B7 |
| 38.154 | Scoped Uplift Still Falls Back To A Full-Workspace Analysis When The Scope Has No `file_paths` | B7 |
| 38.1268 | Internal Ontology Upsert Still Reconciles New Nodes By `(parent_id, name)` Instead Of `componentId` | B7 |
| 38.1224 | Ontology Upsert Still Silently Flattens Missing Parents Into Roots | B7 |
| 38.1225 | Ontology Upsert Still Turns Cyclic Parent Graphs Into Generic 500s | B7 |
| 38.1228 | Ontology Upsert Still Never Validates Duplicate `componentId` Values | B7 |
| 38.1235 | Ontology Upsert Still Ignores Rename-Only Updates | B7 |
| 38.1239 | Ontology Upsert Still Treats Invalid `nodeType` Values As Internal Errors | B7 |
| 38.1245 | Shared Internal Ontology DTO Still Advertises The Wrong `nodeType` Vocabulary | B7 |
| 38.184 | File-Backed Doc Reads Still Ignore The Workspace's Docs Branch | B8 |
| 38.193 | File Browsing Still Ignores The Requested Branch And Reads Branchless Repo Storage | B8 |

---

## Execution Notes

### Phase dependencies

```
Phase 0 ──► Phase 1 ──► Phase 2
                │
                ▼
             Phase 3 ──► Phase 4 ──► Phase 6
                │            │
                ▼            ▼
             Phase 5      Phase 7
```

- **Phase 0** is prerequisite for everything (write paths broken without it).
- **Phase 1** must precede **Phase 2** (can't switch orgs if you can't create one).
- **Phase 3** must precede **Phase 4** (can't author views without a workspace).
- **Phases 4, 5** are independent of each other.
- **Phases 6, 7** can run in parallel after Phase 4.

### What this batch does NOT cover

These are deliberately excluded — they're important but not on the happy-path
critical path:

| Excluded Area | Reason | Batch |
|---------------|--------|-------|
| Invitation lifecycle (accept, decline, resend, expiry) | Invite works; delivery & history are polish | B4 |
| Invite-only workspace visibility | Public workspaces work fine | B11 |
| API key / worker token edge cases | Key creation works; negative expiry etc. is hardening | B12 |
| Chat session forking, attaching, resuming | Chat send works; advanced session ops are separate | B1 |
| g5d CLI verbs (push, merge, fork, doctor) | Not browser-facing | B8/B15 |
| Escalation lifecycle | Task execution works; escalation is a parallel subsystem | B5 |
| Search, Spotlight, reindex | Content appears via sidebar; full-text search is additive | B10 |
| Artifact rendering (JSON, diff, Monaco) | Content renders as text; rich rendering is polish | B2 |
| Comment threading & mentions | Comments post; threading and @mention are additive | B16 |
| Worker registration, heartbeats, scaling | Worker pool management separate from task execution | B6 |
| Workspace tabs, sidebar state, keyboard nav | UI state management is polish | B3/B16 |
