# Agents cite workspace files as clickable links (#875)

Sources: issue [G5Labs/G5-Lean#875](https://github.com/G5Labs/G5-Lean/issues/875) and the
owner's answers of 2026-10-27 to the Project Docs question set
`docs/dc2d7087-7895-46c6-a5ca-f20e910fba19/questions/agent-file-links.g5q.json` (docs
repository; `open-questions.md` beside it explains each question). All 14 answers are
owner decisions. Question ids are quoted below as `Q<n> id`.

In G5 chat, agents cite files as `path:line` or backticked paths, which chat cannot open.
Chat already opens `g5://file/<path>` links in a workspace tab, but no agent is told about
them, and Claude Code's own prompt tells it to write `path:line`. This change tells agents
(Claude and Codex) to write `g5://file` links in chat replies, and adds a line form to
those links so a cited line opens at that line.

## Decisions from the owner's answers

| Question | Answer | Effect on the design |
|---|---|---|
| Q1 `verified-click-behaviour` | Project Docs and single-code-repository links checked | Both unqualified cases are confirmed; `?repo=` is covered by tests and the live check below. |
| Q2 `rule-surfaces` | Chat replies only | Documents keep relative links; GitHub text and email keep plain paths. |
| Q3 `terminal-sessions` | The rule names G5 chat | No chat-only delivery mechanism; the wording carries the condition. |
| Q4 `link-text` | Agent chooses | File name, or a path when the name is ambiguous. |
| Q5 `line-references` | Links must open at the line | Adds a line form to `g5://file` (frontend change, below). |
| Q6 `skills-and-prompts` | `.agents/AGENTS.md` only | No persona, workbench-action or skill edits. |
| Q7 `record-in-ontology` | Record as intent | New blocks below. |
| Q8 `repo-qualifier-identifier` | "Could you check what works in code?" | Checked: repository id ([finding](#q8-what-works-in-code)). |
| Q9 `repo-qualifier-when` | Only with several code repositories | Project Docs links never carry `?repo=`. |
| Q10 `non-openable-files` | Not covered | The rule says nothing about personal, reference or out-of-repository files. |
| Q11 `personal-workspace-chat` | Exclude personal chats | The personal guidance says to keep plain paths. |
| Q12 `renderer-gaps-scope` | Leave as is | Relative links and backticked paths in chat stay unchanged; no follow-up issue. |
| Q13 `acceptance-providers` | Claude and Codex | The live check runs for both. |
| Q14 `acceptance-evidence` | One recorded live check | Plus the dispatch test from the issue. |

### Q8: what works in code

- The chat link resolver, `resolveDeepLinkRepoId` (`frontend/src/lib/workspaceBranches.ts`),
  accepts an exact repository id first, then a G5 repository name that exactly one
  repository has. Anything else fails closed with "Cannot open file link".
- In the VM the agent can read each checkout's repository id: it is the last segment of
  `git remote get-url origin`, and `/work/.g5/layout.json` lists `repo_id`, `role`
  (`code`/`docs`) and `root_path` for each repository. Neither shows the G5 repository name.
- Checkout folder names are the workspace's root paths (`code`, `docs` here). The resolver
  matches them only when a folder happens to equal the G5 name.
- So the repository id is the only identifier that both works and is always visible. A
  readable name would need G5 to expose names to the VM (a new mechanism); the link text
  already hides the id. Recorded as an agent decision,
  [`g5.chat.file-citations.repo-id`](../../ontology/features/chat.g5.md#g5.chat.file-citations.repo-id).
  The owner can still overrule it.

## Ontology changes

| Block | Change | Why |
|---|---|---|
| [`g5.documents.file-links`](../../ontology/features/documents.g5.md#g5.documents.file-links) (new D, documents) | Records the existing `g5://file` grammar and repository ownership: `?repo=` by id or unique name, else the Project Docs folder, else the only code repository; unknown or ambiguous owners fail closed. | The chat rule depends on this rule, and it was only in the earlier ontology (center-panel D73). Existing behaviour, not a change. |
| [`g5.documents.file-links.line`](../../ontology/features/documents.g5.md#g5.documents.file-links.line) (new D, documents) | `#L<n>` or `#L<n>-L<m>` after any `?repo=` opens at line `n`; other fragments open the file without a line; the ontology editor keeps heading fragments. | Q5. |
| [`g5.chat.file-citations`](../../ontology/features/chat.g5.md#g5.chat.file-citations) (new I, chat) | In chat replies, agents cite files as file links, with lines in the fragment, overriding `path:line`; chat replies only. | Q2, Q4, Q5, Q7, Q10, Q13. |
| [`g5.chat.file-citations.instructions`](../../ontology/features/chat.g5.md#g5.chat.file-citations.instructions) (new D, chat) | Stated once in the packaged instructions, whose wording names G5 chat; personas, workbench prompts and skills do not repeat it. | Q3, Q6. |
| [`g5.chat.file-citations.repo-qualifier`](../../ontology/features/chat.g5.md#g5.chat.file-citations.repo-qualifier) (new I, chat) | `?repo=` only with several code repositories; Project Docs links never carry it. | Q9. |
| [`g5.chat.file-citations.repo-id`](../../ontology/features/chat.g5.md#g5.chat.file-citations.repo-id) (new D, chat) | `?repo=` takes the repository id. | Q8 finding. |
| [`g5.personal.file-citations`](../../ontology/features/personal_workspace.g5.md#g5.personal.file-citations) (new D, personal workspace) | The citation rule does not apply in personal workspaces; the personal guidance says to keep plain paths. | Q11: Home's side panel cannot open file links. |

## Governing ontology

- [g5.documents.source-editing](../../ontology/features/documents.g5.md#g5.documents.source-editing):
  line citations open Source at the line. The line form reuses that path; it adds no
  second way to open a file.
- [g5.documents.ontology.links](../../ontology/features/documents.g5.md#g5.documents.ontology.links)
  and [g5.diff-graph.relationships](../../ontology/features/diff_graph.g5.md#g5.diff-graph.relationships):
  ontology and Project Docs keep relative links (Q2). The Diff Graph already ignores
  `g5://file` fragments when classifying links, so it needs no change.
- [g5.chat.presentation](../../ontology/features/chat.g5.md#g5.chat.presentation): Claude and
  Codex replies share one rendering path. The rule and the renderer apply equally to both.
- [g5.personal.home](../../ontology/features/personal_workspace.g5.md#g5.personal.home) and
  [g5.personal.project-docs](../../ontology/features/personal_workspace.g5.md#g5.personal.project-docs):
  personal chats show on Home without the workspace page's link delegate, and the harness
  already adds personal guidance there.
- [g5.ontology.material-change](../../ontology/features/ontology_and_harness.g5.md#g5.ontology.material-change):
  the owner chose to record this as intent (Q7).

## Design

Three changes, each with one owner, and no new mechanism.

### 1. The agent rule (`.agents/AGENTS.md`)

`.agents/AGENTS.md` is packaged into the runtime bundle (`infra/docker/Dockerfile.g5d`) and
installed byte-for-byte as Claude Code's `~/.claude/CLAUDE.md` and Codex's
`~/.codex/AGENTS.md` (`backend/g5d-agent/src/harness.rs`). One new bullet in its
"G5 workspace" section reaches both providers. Draft wording, to be finalized during
implementation:

> - In G5 chat replies, cite a workspace file as a Markdown link
>   `[text](g5://file/<repository-relative path>)`, not as `path:line` or a backticked path:
>   chat cannot open those, so this overrides any default to write `file_path:line_number`.
>   Use the file name as the text, or the path when the name is ambiguous. Add `#L<n>` to
>   open at a line. When the workspace has more than one code repository, put
>   `?repo=<repository id>` before any `#L` in links to code files; the id ends the
>   checkout's `origin` URL and is listed in `/work/.g5/layout.json`. Project Docs paths
>   (`docs/<workspace-id>/…`) need no `?repo=`. For example,
>   `[open-questions.md](g5://file/docs/<workspace-id>/open-questions.md)` or
>   `[harness.rs](g5://file/backend/g5d-agent/src/harness.rs#L44)`. Outside G5 chat, and in
>   files you write, GitHub text and email, keep the usual forms.

Rejected: delivering the rule only to chat launches (Q3 chose wording), and repeating it in
personas, workbench actions or skills (Q6).

### 2. The personal exception (`PERSONAL_GUIDANCE`)

`PERSONAL_GUIDANCE` in `backend/g5d-agent/src/harness.rs` is appended after the packaged
instructions for personal launches only, so a bullet there overrides rule 1 in personal
chats and nowhere else: write file paths as backticked plain text, not `g5://file` links,
because Home's side panel cannot open them.

### 3. The line form (frontend)

Grammar: `g5://file/<path>[?repo=<id or name>][#L<n>[-L<m>]]`. This is the fragment form
relative links already use (`resolveRepoLink`, `frontend/src/components/files/repoLinks.ts`)
and the one GitHub uses. The existing `MarkdownPreview.links.test.tsx` fixture already uses
this form.

- **One line grammar.** Export the `L<n>(-L<m>)?` parsing from `repoLinks.ts` and use it in
  both relative and `g5://file` links.
- **`parseG5FileHref`** (`sandboxedHtml.ts`) splits off the fragment before the path and
  query checks and returns `{ filePath, repoRef?, line? }`. A line fragment sets `line`;
  any other fragment is ignored. Path and query validation are unchanged, so a malformed
  query still drops the whole link. A `?` inside the fragment is malformed too
  (amendment from implementation review, 2026-10-27): otherwise `…#L12?repo=api`, with the
  qualifier misplaced after the line, would open as an unqualified link, breaking the rule
  that a link naming a repository is never read as ownerless.
- **Workspace page delegate** (`WorkspacePage.tsx`) passes `parsed.line` to
  `handleOpenDeepLinkFile`, which already accepts a line and hands `[line, line]` to the
  citation pathway (`handleOpenCitationFile` → `handleSelectFile` → tab `lineRange`).
  Ownership resolution is unchanged.
- **Sandboxed HTML documents** keep the same grammar (one format for every opener). The
  in-frame copy `SANDBOXED_FILE_NAV_JS` strips the fragment the same way and posts `line`.
  `SandboxedHtmlFrame` re-validates it: if present, it must be a positive safe integer, or
  the whole proposal is dropped. It then calls `onOpenWorkspaceFile(filePath, repoRef, line)`.
  The existing source, visibility and rate-limit guards are unchanged.
- **Unchanged:** `MarkdownPreview` and `renderInlineMarkdown` already keep the full href
  and mark it `data-g5-file-link`. `CurrentOntologyReference` already splits fragments
  before parsing and keeps resolving headings. The Diff Graph's `link_destination` already
  separates fragments.

Rejected alternatives:
- A `:12` suffix: it collides with path text, and no other G5 link uses it.
- A `?line=` parameter: the query is deliberately exactly one `repo=` pair, failing closed.
  A second parameter weakens that rule for no gain.
- A line form for chat only: sandboxed explainers would then treat the same link as
  malformed.

## Phases

1. **Line form.** Parser, delegate, sandbox bridge and tests (frontend only). This can
   ship without the rule.
2. **Agent rule and personal exception.** `.agents/AGENTS.md` bullet, `PERSONAL_GUIDANCE`
   bullet, harness and dispatch tests. It goes in the same PR as phase 1, because the
   rule tells agents to write `#L`.
3. **Evidence.** Automated suites, then the owner's live check for each provider, recorded
   in the PR.

## Acceptance outcomes

- A fresh Claude agent and a fresh Codex agent in G5 chat, asked to point at a Project Doc
  and at a code line, each reply with `g5://file` links. Clicking them opens the file in a
  workspace tab, and the code link opens at the cited line. One recorded live check per
  provider (Q13, Q14).
- In a workspace with two or more code repositories, a code-file link with
  `?repo=<repository id>` opens the file in that repository. The same link without the
  qualifier shows "Cannot open file link" and lists the candidate repositories. Automated
  test in a two-code-repository fixture.
- `#L<n>`, `?repo=<id>#L<n>` and `#L<n>-L<m>` open at line `n`. A heading fragment opens the
  file without a line. Malformed paths and queries still open nothing. Sandboxed documents
  behave the same, and a forged non-integer `line` is dropped.
- Dispatch tests show that the installed Claude and Codex instructions contain the rule.
  The harness unit test shows that personal launches add the plain-path guidance and
  ordinary launches do not.

## Tests

- Unit (`sandboxedFileNav.test.ts`): `parseG5FileHref` and the shipped in-frame script
  bytes, covering line, range, `?repo=` with a line, a heading fragment, `#L0`, and the
  existing path and query refusals with a fragment present. `SandboxedHtmlFrame`
  re-validation of `line`.
- Component (new `frontend/src/pages/WorkspacePageFileLinks.test.tsx`, using the existing
  MSW `renderApp` harness): click a rendered chat or Markdown `g5://file` link. Covers a
  Project Docs path, a one-code-repository path with `#L12` (tab opens with the line), a
  two-code-repository fixture with `?repo=<id>` (right repository), and the unqualified
  case (error toast).
- `MarkdownPreview.links.test.tsx` and `workspaceBranches.test.ts` as regression checks.
- Rust: in `backend/g5d-agent/src/harness.rs`, extend the personal-guidance test. In
  `backend/g5d/tests/dispatch_modules.rs`, the Claude and Codex harness tests (which
  already compare installed instructions byte-for-byte with `.agents/AGENTS.md`) also
  assert the rule is present.
- Normal frontend lint, typecheck and vitest, plus `cargo test` for `g5d-agent` and the
  affected `g5d` dispatch tests.
- Before merge, as a proxy for the live check: run `claude -p` and `codex exec` in the VM
  with the new instructions, and check that the reply's link form opens in the in-VM
  preview. The recorded live check needs the change deployed (see assumptions).

## Risks

- **Agents may not follow the rule.** Model replies vary, and Claude Code's own prompt
  pushes `path:line`. One live check per provider can pass by chance (accepted in Q14). If
  a provider ignores the rule, the fix is stronger wording, not new mechanics.
- **Terminal sessions** read the same instructions. The rule depends on the agent
  recognising "G5 chat" (Q3). A person running a CLI in the workspace terminal might
  still get `g5://` links.
- **Deployment skew.** The rule reaches VMs with the runtime bundle; the line form reaches
  browsers with the frontend. If the bundle lands first, `#L` links do nothing until the
  frontend follows. They ship in one PR; deploy order is Integration Engineering's call.
- **Links in chat history break when a code repository is added** (Q9 accepted this).
  Unqualified code links written in a one-code-repository workspace fail closed afterwards.
- **Line past the end of a file:** resolved. The editor's reveal clamps to the last line
  (`CodeEditor.tsx`, `Math.min(scrollToLine, contentLines.length)`).

## Open assumptions

- The `?repo=` case has not been clicked live (Q1). Automated tests plus the live check
  cover it only if a two-code-repository workspace is available. Otherwise the live
  multi-repository click stays an evidence gap.
- The recorded live check (Q14) assumes a person can run it after the change reaches a G5
  deployment, or in a full-stack in-VM preview with real provider credentials. Where it
  runs is left to the owner and Integration Engineering.
- The repository-id choice (Q8) is an agent decision on the owner's delegation. Using the
  G5 name would need a way for the VM to learn repository names.

## Implementation and verification — 2026-10-27

Implemented as designed, plus one amendment found in review: a `?` after the `#` makes a
link malformed (see the parser bullet above and
[`g5.documents.file-links.line`](../../ontology/features/documents.g5.md#g5.documents.file-links.line)).
The rule's final wording is the bullet in `.agents/AGENTS.md`, close to the draft above.

Executed on branch `issue-875-agents-cite-workspace-files` in this VM:

| Check | Result |
|---|---|
| Frontend typecheck (`tsc -p tsconfig.json`), ESLint on changed files, Prettier | Clean. The ESLint warnings shown are pre-existing (`SandboxedHtmlFrame.tsx` effect, untouched lines). |
| Targeted Vitest: `sandboxedFileNav`, `SandboxedHtmlFrame`, `MarkdownPreview.links`, `workspaceBranches`, `CurrentOntologyReference`, new `WorkspacePageFileLinks` | 375 passed |
| Full Vitest suite | 345 files, 5236 tests passed |
| Mutation: the delegate drops the line | `WorkspacePageFileLinks` "opens a cited line … in Source" fails |
| `cargo test -p g5d-agent --lib harness::` | 10 passed |
| `cargo test -p g5d --test dispatch_modules` (Claude and Codex harness tests) | 2 passed; both fail when the rule text is removed |
| `cargo fmt --check` (g5d, g5d-agent) | Clean |
| Playwright, process-isolation dev stack with fake Claude/Codex (`just dev-up`, Chromium): `agent-file-links.spec.ts` | 3 passed: a Claude reply and a Codex reply each open a Project Doc (docs repository, Preview) and a code file at line 150 (Source; Monaco shows line 150 and not the top); an explainer's `?repo=<code repository id>#L150` link opens at the line and an unknown repository shows "Cannot open file link" naming it. The served bundle was checked to contain the final parser. |
| Playwright regression: `current-ontology.spec.ts`, `chat.spec.ts`, `question-store.spec.ts` (sandboxed explainer bridge) | 7 passed. `chat.spec` first failed with `missing_llm_credential` until the org test key was seeded as CI does; this is a setup step, not a product failure. |

Live check (Q14), 2026-10-27, in the owner's full-stack preview of this branch:
- Setup: the preview's process-mode runtime installs this branch's `.agents/AGENTS.md`
  (byte-identical). The org has a Claude subscription account and no Codex credential.
  Workspace `source-control-with-pr`: one code repository and a docs repository.
- **Claude: passed.** A new chat (session `b19428af-bb95-41f7-8de3-e0fa08c03505`) was
  asked "In this workspace's checkout (your working directory), where is the checkout
  plan, and where is the discount defined in the code?" It replied with
  `[preview-checkout.md](g5://file/docs/e1416431-e276-4a03-bcf0-167a542262a1/preview-checkout.md)`
  and `[preview-cart.ts](g5://file/src/preview-cart.ts#L2)`, with no `path:line`. Clicked
  in Chromium: the Project Doc opened in Preview and `src/preview-cart.ts` opened in a tab,
  with no error notice. The file has two lines, so the jump to line 2 is not visible
  there; the dev-stack E2E checks it with a 200-line file.
- An earlier attempt (session `df063b16-…`) is void. The preview's agents run as
  unisolated host processes, so a question without "your working directory" sent the
  agent through the real `/work` checkout. It found the throwaway driver script there,
  which named the check, and answered about that. The driver was then renamed and the
  prompt passed through the environment. This is a property of the process-isolation
  dev host, not of workspace VMs, where `/work` is the workspace.
- **Codex: passed, checked by the owner.** After adding a Codex credential, the owner
  sent the same prompt to Codex (session `25a8b298-f2a4-4e43-890e-f745a473ce76`) and
  confirmed that both links open. The reply cited
  `[preview-checkout.md](g5://file/docs/e1416431-e276-4a03-bcf0-167a542262a1/preview-checkout.md)`
  and `[src/preview-cart.ts](g5://file/src/preview-cart.ts#L2)`.
- The owner also asked a Claude chat for `preview-follow-up.md` (session
  `7f41bed1-…`). It answered with
  `[preview-follow-up.md](g5://file/docs/e1416431-e276-4a03-bcf0-167a542262a1/preview-follow-up.md)`.

Not executed, and remaining gaps:
- A live click in a workspace with two or more code repositories. The jsdom page test
  covers that resolution; the browser flows ran in a one-code-repository workspace.
- Terminal sessions and deployment order, as listed under Risks.

Review of the change against the governing blocks and affected consumers:
- Ownership resolution, the Diff Graph and the ontology editor's heading fragments are
  unchanged. `CurrentOntologyReference` still splits the fragment before parsing.
- Relative links now share `lineFromFragment`. The only behaviour change: `#L0` and
  out-of-range numbers open without a line, where a line 0 was passed before, which the
  opener already ignored.
- The frame bridge still re-validates every field, keeps the rate limit, and drops forged
  lines.
- No Rust production logic changed (an instruction string and tests), so no Verus proof
  applies. The TypeScript has no proof tooling; the mutation controls above supplement
  the tests.

## Out of scope

Q12 keeps these as they are: relative Markdown links in chat open a browser tab, and
backticked paths stay plain code. Also out of scope: making Home's side panel open file
links (Q11), citations of files no link can open (Q10), and any Diff Graph change.
