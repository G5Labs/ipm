# Approval guidance

Generated 2026-11-04 from `G5Labs/G5-Lean` main @ `ca5ee0fde`, covering all history since the repository's first commit (2026-09-12) and weighting commits since 2026-10-14 double. Regenerate with [approval-guidance-generation.md](.agents/guidance/approval-guidance-generation.md). Edit only **Manual overrides** by hand; regeneration keeps that section and replaces the rest.

Get Approvals reads this file when it chooses people (step 5). A personal `/work/personal/approval-guidance.md` wins any conflict.

## How to use

1. **Find the areas.** Match each changed file against **Code routing** and **Ontology routing**; the first matching row wins. Weigh areas by changed lines. Files under `.g5/pr_ontology_diff_design/`, `.g5/chat_intent/`, `.g5/ontology-plan-*` and `.g5/ontology-review-*` go with the ontology area they plan. Backend tests (`backend/g5-api/tests/**`, `tests/**`) go with the code they test, and fall back to **be-tests** only on their own.
2. **Pick within the definition.** Choose only from the definition's `humans` that have `stillMember: true`. Never pick the change's author or anyone already pending. Both definitions have `allowSelfApproval: false`. Take the dominant area's **Primary**. If they aren't eligible, use **Backup**, then **Alternates**, then **Approval definitions** below.
3. **One person by default.** Add a second person, the next eligible from the same row (or the primary of the second-largest area), only when:
   - the row is marked **Risky**,
   - the change is over about 800 changed lines, or
   - the change spans three or more areas with different primaries.
4. **Ontology changes.** Prefer someone who knows both the touched statements and their intent. **Intent stakeholders** counts statements whose Metadata `Source` names the person, or cites an issue that person filed. Ben is the recorded source for most `human-provided` statements, so when Ben owns the workspace, use the row's Backup.
5. **Spread the load.** Zhengchun Liu is primary or backup for most areas. When two candidates are close, prefer the other one, and say why in the request note.
6. **Map to `userId`.** Match people by email to the definition's `humans`. The userIds in **People** are there to check that mapping.

## Manual overrides

Hand-maintained; regeneration keeps this section. Add lines such as "`<area>`: prefer X over Y because …" or "X is away until <date>". These override everything below.

_None yet._

## Approval definitions (as of 2026-11-04)

| Definition | Purpose | Eligible people | Who fits where |
|---|---|---|---|
| Ontology Alignment (`e54d41e8-44d4-47f4-8eda-e824b5d3f638`, v10) | Agreement on the goals of the work and the implementation approach | Ben, Hariharan Rahul, Zhengchun Liu, Davide Pagano, James Moore, Kaihui Zheng, Ziniu Wu, Saehan Jo, Tim Kraska, David Chang | Every active person with evidence here is eligible, so use the routing rows directly: the dominant area's Primary, then Backup, then Alternates. For ontology changes, prefer the row's intent stakeholders. Tim Kraska and David Chang have almost no commits here, so pick them only through a Manual override. James Moore has no commits but is the recorded source for the Microsoft Entra requirement in `features/users.g5.md`; consider him for enterprise sign-in and identity intent. |
| UI changes (`cbb33808-841c-4945-b9ce-c5f84ab8ff2e`, v7) | A person approves UI changes that the agent review rejects as significant | Davide Pagano, David Chang, Ben, Tim Kraska | Davide Pagano first: primary for **fe-shell** and **fe-core** and backup for **ont-ui**, and has approved two UI changes. Next comes David Chang (approved a UI change on 2026-11-04), then Ben (strongest on **fe-org**, **fe-workspace** and **ont-ui**) when not the author. Tim Kraska has no recent UI evidence. |

## Code routing

| Area | Matches | Primary | Backup | Alternates | Risky | Evidence (commits since 2026-09-12) |
|---|---|---|---|---|---|---|
| **migrations**: Database migrations | `backend/g5-api/src/db/migrations/**`, `.sqlx/**` | Zhengchun Liu | Ben | Hariharan Rahul, Kaihui Zheng | yes | Zhengchun 107 (63 recent, last 11-03); Ben 43 (23 recent, last 11-02) |
| **data**: Data layer and stores | `backend/g5-api/src/db/**`, `backend/g5-api-data/**`, `backend/g5-api-core/src/db/**`, `backend/g5-storage/**` | Zhengchun Liu | Ben | Kaihui Zheng, Hariharan Rahul |  | Zhengchun 44 (12 recent, last 10-27); Ben 20 (10 recent, last 10-26) |
| **git**: Git, SCM and PR adapters | `backend/g5-scm/**`, `backend/g5-api-core/src/services/git/**` | Ben | Hariharan Rahul | Zhengchun Liu, Kaihui Zheng | yes | Ben 86 (62 recent, last 11-02); Hariharan 34 (34 recent, last 10-30) |
| **workspace-svc**: Workspace services | `backend/g5-api-core/src/services/workspace/**` | Zhengchun Liu | Ben | Kaihui Zheng, Hariharan Rahul |  | Zhengchun 70 (56 recent, last 11-02); Ben 34 (33 recent, last 11-02) |
| **agents-svc**: Agents, chat and workbench backend | `backend/g5-api-core/src/services/agents/**`, `backend/g5d-agent/**` | Zhengchun Liu | Hariharan Rahul | Kaihui Zheng, Saehan Jo |  | Zhengchun 93 (69 recent, last 11-03); Hariharan 39 (39 recent, last 10-30) |
| **identity**: Identity, auth, authz and middleware | `backend/g5-api-core/src/services/identity/**`, `backend/g5-auth/**`, `backend/g5-authz/**`, `backend/g5-api-core/src/auth/**`, `backend/g5-api-core/src/middleware/**` | Ben | Zhengchun Liu | Kaihui Zheng, Davide Pagano | yes | Ben 55 (43 recent, last 11-02); Zhengchun 75 (31 recent, last 11-03) |
| **hosts-svc**: Hosts and VM orchestration (API side) | `backend/g5-api-core/src/services/hosts/**`, `backend/g5-api-core/src/cloud/**` | Zhengchun Liu | Kaihui Zheng | Hariharan Rahul, Davide Pagano | yes | Zhengchun 59 (44 recent, last 11-04); Kaihui 28 (16 recent, last 10-28) |
| **vm-runtime**: VM host runtime, isolation and dispatch | `backend/g5d-host/**`, `backend/g5d-isolation/**`, `backend/g5d-foundation/**`, `backend/g5d-dispatch/**` | Kaihui Zheng | Zhengchun Liu | Hariharan Rahul, Ben | yes | Kaihui 157 (45 recent, last 10-31); Zhengchun 95 (39 recent, last 11-04) |
| **email**: Email | `backend/g5-api-core/src/services/email/**` | Ben | — (gap) | — |  | Ben 16 (16 recent, last 11-02) |
| **api**: API routes, contracts and internal API | `backend/g5-api-routes/**`, `backend/g5-contracts/**`, `backend/g5-internal-api/**`, `backend/g5-api-foundation/**`, `backend/g5-api-core/src/*.rs`, `backend/g5-api-core/src/ws/**`, `backend/g5-api-core/src/errors/**` | Zhengchun Liu | Kaihui Zheng | Ben, Hariharan Rahul |  | Zhengchun 328 (100 recent, last 11-04); Kaihui 248 (28 recent, last 11-03) |
| **g5d**: g5d CLI | `backend/g5d/**` | Ben | Zhengchun Liu | Kaihui Zheng, Hariharan Rahul |  | Ben 67 (54 recent, last 11-02); Zhengchun 125 (38 recent, last 11-04) |
| **semantic**: Ontology parsing (g5-semantic) | `backend/g5-semantic/**` | Kaihui Zheng | Zhengchun Liu | Saehan Jo, Ziniu Wu |  | Kaihui 4 (1 recent, last 10-28); Zhengchun 6 (1 recent, last 10-27) |
| **be-tests**: Backend integration tests (route by the code under test first) | `backend/g5-api/tests/**` | Zhengchun Liu | Ben | Kaihui Zheng, Hariharan Rahul |  | Zhengchun 416 (178 recent, last 11-04); Ben 203 (120 recent, last 11-02) |
| **fe-workspace**: Workspace, landing, review and Source Control UI | `frontend/src/components/workspace/**`, `frontend/src/components/workspace-landing/**`, `frontend/src/components/workspace-review/**`, `frontend/src/components/source-control/**` | Hariharan Rahul | Kaihui Zheng | Zhengchun Liu, Ben |  | Hariharan 74 (74 recent, last 10-31); Kaihui 125 (20 recent, last 11-03) |
| **fe-docs**: Editor, Files, comments and explainer UI | `frontend/src/components/editor/**`, `frontend/src/components/files/**`, `frontend/src/components/comments/**`, `frontend/src/components/explainer/**` | Zhengchun Liu | Saehan Jo | Ziniu Wu, Kaihui Zheng |  | Zhengchun 140 (87 recent, last 10-31); Saehan 34 (34 recent, last 11-04) |
| **fe-chat**: Chat UI | `frontend/src/components/chat/**`, `frontend/src/components/agent/**` | Zhengchun Liu | Hariharan Rahul | Saehan Jo, Kaihui Zheng |  | Zhengchun 69 (34 recent, last 11-03); Hariharan 28 (26 recent, last 10-27) |
| **fe-shell**: Workbench, layout, home, onboarding, settings and UI kit | `frontend/src/components/workbench/**`, `frontend/src/components/layout/**`, `frontend/src/components/home/**`, `frontend/src/components/onboarding/**`, `frontend/src/components/settings/**`, `frontend/src/components/ui/**`, `frontend/src/components/shared/**`, `frontend/src/components/views/**`, `frontend/src/components/tree/**` | Davide Pagano | Zhengchun Liu | Hariharan Rahul, Ben |  | Davide 92 (55 recent, last 11-02); Zhengchun 41 (22 recent, last 11-02) |
| **fe-org**: Organization, issues and personal UI | `frontend/src/components/org/**`, `frontend/src/components/external-issues/**`, `frontend/src/components/personal/**` | Ben | Hariharan Rahul | Zhengchun Liu, Davide Pagano |  | Ben 60 (27 recent, last 10-30); Hariharan 41 (41 recent, last 10-31) |
| **fe-preview**: Preview and execution UI | `frontend/src/components/preview/**`, `frontend/src/components/execution/**` | Kaihui Zheng | Zhengchun Liu | Hariharan Rahul, Davide Pagano |  | Kaihui 53 (37 recent, last 11-03); Zhengchun 21 (4 recent, last 10-23) |
| **fe-core**: Frontend pages, hooks, lib and config | `frontend/src/pages/**`, `frontend/src/hooks/**`, `frontend/src/lib/**`, `frontend/src/data/**`, `frontend/src/test/**`, `frontend/src/*`, `frontend/*` | Davide Pagano | Zhengchun Liu | Kaihui Zheng, Hariharan Rahul |  | Davide 152 (100 recent, last 11-03); Zhengchun 229 (103 recent, last 11-03) |
| **e2e**: End-to-end, browser and mock-preview tests | `frontend/e2e/**`, `tests/e2e/**` | Hariharan Rahul | Zhengchun Liu | Davide Pagano, Kaihui Zheng |  | Hariharan 176 (118 recent, last 11-01); Zhengchun 178 (87 recent, last 11-04) |
| **ci**: CI workflows and CI scripts | `.github/**`, `infra/ci/**`, `scripts/ci/**`, `.config/**` | Kaihui Zheng | Zhengchun Liu | Hariharan Rahul, Davide Pagano | yes | Kaihui 72 (31 recent, last 11-03); Zhengchun 77 (17 recent, last 11-03) |
| **infra**: Deployment infrastructure | `infra/terraform/**`, `infra/helm/**`, `infra/qa/**`, `infra/prod/**`, `infra/staging/**`, `infra/packer/**`, `infra/docker/**`, `infra/self-hosted/**`, `infra/admin/**`, `infra/guest/**`, `scripts/self-hosted-azure/**` | Kaihui Zheng | Zhengchun Liu | Ben, Davide Pagano | yes | Kaihui 126 (63 recent, last 10-28); Zhengchun 98 (14 recent, last 11-04) |
| **scripts**: Scripts and tools | `scripts/**`, `tools/**`, `Justfile` | Kaihui Zheng | Zhengchun Liu | Hariharan Rahul, Davide Pagano |  | Kaihui 201 (86 recent, last 10-28); Zhengchun 195 (49 recent, last 11-04) |
| **harness**: Agent harness and preview profiles | `.agents/**`, `AGENTS.md`, `CLAUDE.md`, `G5_SUBAGENT_BOOTSTRAP.md`, `.g5/preview_profiles/**` | Ziniu Wu | Ben | Zhengchun Liu, Kaihui Zheng |  | Ziniu 53 (21 recent, last 11-03); Ben 44 (39 recent, last 11-02) |
| **docs**: Docs and user guide | `docs/**`, `frontend/public/user-guide/**`, `README.md` | Ziniu Wu | Zhengchun Liu | Kaihui Zheng, Ben |  | Ziniu 151 (25 recent, last 10-31); Zhengchun 244 (45 recent, last 10-30) |

## Ontology routing

Files are under `.g5/ontology/`.

| Area | Files | Primary | Backup | Alternates | Risky | Intent stakeholders (Metadata) | Editing evidence |
|---|---|---|---|---|---|---|---|
| **ont-foundations**: purpose and concepts (security, invariants, principles, data model) | `root.g5.md`, `concepts/**` | Zhengchun Liu | Ben | Kaihui Zheng, Ziniu Wu | yes | Ben 31, Zhengchun Liu 4 | Zhengchun 84 (25 recent, last 11-02); Ben 31 (25 recent, last 11-02) |
| **ont-git**: Git, reviews and agent approvals | `software_architecture/git.g5.md`, `features/reviews.g5.md`, `features/agent_approvals.g5.md` | Hariharan Rahul | Ben | Ziniu Wu, Kaihui Zheng | yes | Ben 83, Hariharan Rahul 48, Zhengchun Liu 8 | Hariharan 40 (40 recent, last 10-31); Ben 60 (59 recent, last 11-02) |
| **ont-workspaces**: workspaces, personal workspaces and repositories | `features/workspaces.g5.md`, `features/personal_workspace.g5.md`, `features/repositories.g5.md` | Ben | Zhengchun Liu | Saehan Jo, Kaihui Zheng |  | Ben 73, Zhengchun Liu 4, Hariharan Rahul 3 | Ben 68 (47 recent, last 10-30); Zhengchun 34 (17 recent, last 10-29) |
| **ont-runtime**: hosts, control plane, workspace VM and terminal | `software_architecture/hosts.g5.md`, `software_architecture/host_control_plane.g5.md`, `software_architecture/workspace_vm.g5.md`, `features/terminal.g5.md` | Kaihui Zheng | Zhengchun Liu | Ben, Ziniu Wu | yes | Ben 95, Zhengchun Liu 21, Kaihui Zheng 5, Saehan Jo 4 | Kaihui 52 (52 recent, last 10-31); Zhengchun 42 (42 recent, last 11-04) |
| **ont-chat**: chat | `features/chat.g5.md` | Saehan Jo | Zhengchun Liu | Hariharan Rahul, Kaihui Zheng |  | Ben 53, Hariharan Rahul 25, Zhengchun Liu 25, Kaihui Zheng 4 | Saehan 38 (38 recent, last 11-02); Zhengchun 43 (43 recent, last 11-03) |
| **ont-docs**: documents, Diff Graph, ontology and harness | `features/documents.g5.md`, `features/diff_graph.g5.md`, `features/ontology_and_harness.g5.md` | Zhengchun Liu | Ziniu Wu | Saehan Jo, Kaihui Zheng |  | Saehan Jo 25, Ben 10, Zhengchun Liu 7 | Zhengchun 90 (40 recent, last 10-31); Ziniu 48 (21 recent, last 11-03) |
| **ont-ui**: UI and workbench | `features/ui.g5.md` | Hariharan Rahul | Davide Pagano | Kaihui Zheng, Ben |  | Hariharan Rahul 38, Ben 13, Zhengchun Liu 3 | Hariharan 41 (41 recent, last 10-30); Davide 38 (38 recent, last 11-03) |
| **ont-org**: organizations, users, issues, operations and email | `features/organizations.g5.md`, `features/users.g5.md`, `features/issues.g5.md`, `features/operations.g5.md`, `software_architecture/email.g5.md` | Ben | Zhengchun Liu | Kaihui Zheng, Davide Pagano |  | Ben 97, Hariharan Rahul 9, Zhengchun Liu 8, Davide Pagano 2 | Ben 63 (63 recent, last 11-02); Zhengchun 26 (26 recent, last 11-03) |

## People

Active members with evidence. `G5 Worker Service` (a service account) and `ZCL Phantom` (no commits) are excluded. "G5 approvals decided" only counts decisions on Ben's workspaces, which are the only ones this generator could read.

| Name | Email | userId | GitHub | Primary for | Backup for | Last commit | PRs merged 10-27..11-04 | G5 approvals decided (seen) |
|---|---|---|---|---|---|---|---|---|
| Ben | ben.vandiver@g5labs.ai | `b0e23af7-a29e-4328-8ca4-6ef0ae18a57b` | bvandiver-g5 | git, identity, email, g5d, fe-org, ont-workspaces, ont-org | migrations, data, workspace-svc, be-tests, harness, ont-foundations, ont-git | 2026-11-02 | 11 | 0 |
| Zhengchun Liu | zhengchun.liu@g5labs.ai | `63f3d155-1416-4131-b288-850ccd085a51` | zcl-g5 | migrations, data, workspace-svc, agents-svc, hosts-svc, api, be-tests, fe-docs, fe-chat, ont-foundations, ont-docs | identity, vm-runtime, g5d, semantic, fe-shell, fe-preview, fe-core, e2e, ci, infra, scripts, docs, ont-workspaces, ont-runtime, ont-chat, ont-org | 2026-11-04 | 28 | 2 |
| Kaihui Zheng | kaihui.zheng@g5labs.ai | `78960217-292b-42bc-b434-d27f879204e0` | kaihuiz-g5 | vm-runtime, semantic, fe-preview, ci, infra, scripts, ont-runtime | hosts-svc, api, fe-workspace | 2026-11-03 | 22 | 0 |
| Hariharan Rahul | hariharan.rahul@g5labs.ai | `ed93c3b6-8e85-4899-ba7b-032275144e8b` | rahul-g5 | fe-workspace, e2e, ont-git, ont-ui | git, agents-svc, fe-chat, fe-org | 2026-11-01 | 7 | 2 |
| Davide Pagano | davide.pagano@g5labs.ai | `ebbf61cb-3ea2-43a9-8eca-811741c2abc4` | dvd-g5 | fe-shell, fe-core | ont-ui | 2026-11-03 | 23 | 2 |
| Saehan Jo | saehan.jo@g5labs.ai | `24feda19-5bdf-41ba-bda8-266cc2ac1295` | saehanjo-g5 | ont-chat | fe-docs | 2026-11-04 | 5 | 0 |
| Ziniu Wu | ziniu.wu@g5labs.ai | `ef838271-db7e-4034-a837-b25eb0cbea2d` | ziniuwu-g5 | harness, docs | ont-docs | 2026-11-03 | 4 | 1 |
| Tim Kraska | tim.kraska@g5labs.ai | `155986dc-9116-4182-b585-d5329bdb6ef5` | kraskat | — | — | 2026-10-23 | 0 | 0 |
| David Chang | david.chang@g5labs.ai | `2118f643-ebcb-4f61-a1df-7352d78b03cf` | dfuchangG5 | — | — | 2026-10-22 | 0 | 1 |
| James Moore | jcmmars@gmail.com | `7c8ddf44-3d64-4258-b839-201c048a5963` | lumost | — | — | none | 0 | 0 |

## Gaps

- **email** has a single expert, Ben. When Ben is the author, the nearest fallback is Zhengchun Liu, as a general backend reviewer.
- **semantic** has thin evidence (at most 6 commits per person). Treat its ranking as weak.
- **Eligibility.** Everyone ranked in Routing is eligible for Ontology Alignment. Only Davide Pagano, David Chang, Ben and Tim Kraska are eligible for UI changes, so for UI areas whose Primary is someone else (for example **fe-workspace**, **fe-docs**, **fe-chat**, **fe-preview**, **ont-ui**), use the UI changes row.
- **No GitHub review signal.** The 100 most recent merged PRs (#852–#1082) have no human GitHub reviews of other people's PRs; nearly all are CodeRabbit or the author's own replies. Human review evidence comes only from 8 G5 approval decisions on Ben's workspaces. Rankings therefore rest mostly on authorship.
- **Ontology history is short.** The ontology was restructured around 2026-10-15, so older statement history sits under file names that no longer exist and isn't counted for the current files.
- **Unmatched identity:** `Alex <demo@g5labs.ai>`, one commit on 2026-10-24, committed by Ben. Ignored.
- **Aliases** were confirmed through the head commits of PRs authored by each login: `kaihuizheng@Kaihuis-MacBook-Pro.local` and `kaihuiz1114@gmail.com` (kaihuiz-g5), `rahul@Hariharans-MacBook-Pro.local` and the `rahul-g5` noreply address (rahul-g5), `dvd@g5labs.ai` (dvd-g5), `bvandiver@g5labs.ai` (bvandiver-g5).
- **Not routed:** generated files, `package-lock.json`, `Cargo.lock`, `.sqlx/` (it goes with **migrations** for routing but isn't scored), `.g5/memory/`, `.g5/uplift/`, `.g5/reviews/` and `tests/fixtures/`.
