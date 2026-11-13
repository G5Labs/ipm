# Fable 5.2, release-ordered picker, Opus 5.5 default, single 1M form

Work: [owner request](../../chat_intent/fable-5-2/request.md) (2026-11-09). Authorized
branch: `davide-fable-52`. Baseline `44658cf2240785e94c496fb93045216fd79ce518`.

Governing statement: [g5.chat.native-catalog](../../ontology/features/chat.g5.md#g5.chat.native-catalog),
which owns offered route-specific models, their order and the bundled default, under
[g5.chat.configuration](../../ontology/features/chat.g5.md#g5.chat.configuration),
[g5.chat.inheritance](../../ontology/features/chat.g5.md#g5.chat.inheritance) and
[g5.chat.launch](../../ontology/features/chat.g5.md#g5.chat.launch). The catalog
contract itself is unchanged: `backend/g5-contracts/src/claude_models.rs` stays the
single source, `just gen-model-catalog` projects it into the UI data file and the
native test matrix, and Rust CI rejects stale projections
([provider CLI contract](../../../docs/lean-provider-cli-contract.md)).

## Decisions

- **Fable 5.2** joins as a one-form family (`claude-fable-5-2` / `anthropic/claude-fable-5.2`,
  full effort ladder, default `high`), released 2026-11-09 per Anthropic's
  [model deprecations table](https://platform.claude.com/docs/en/about-claude/model-deprecations)
  and the Claude Code 2.1.321 changelog. OpenRouter lists the slug (listing read 2026-11-09).
- **Release order.** Each family records Anthropic's first-party release day; the catalog
  lists families newest first and a test enforces strict descending order and the date
  shape. The composer options derive from catalog order, so no component sorts. Days:
  Fable 5.2 2026-11-09, Sonnet 5.5 2026-09-28, Opus 5.5 2026-09-22, Fable 5.1 2026-09-01,
  Opus 5 2026-07-24, Sonnet 5 2026-06-30, Fable 5 2026-06-09, Opus 4.8 2026-05-28,
  Opus 4.7 2026-04-16, Sonnet 4.6 2026-02-17, Opus 4.6 2026-02-05, Haiku 4.5 2025-10-15
  (model pages' “Released” rows and the deprecations table's retirement anchors).
- **Default Opus 5.5.** `CLAUDE_DIRECT_CHAT_DEFAULT_MODEL` = `claude-opus-5-5`,
  `CLAUDE_OPENROUTER_CHAT_DEFAULT_MODEL` = `anthropic/claude-opus-5.5`. The composer keeps
  storing the default as the slug and represents the default family once, in its release
  position; `catalogDefaultChoiceFor` resolves the default by identity, not list position.
  "Anyone that didn't change it yet": the composer persists the default concretely, so a
  stored `anthropic/claude-opus-4.8` (or the old direct default `claude-opus-4-8[1m]`) is
  the unchanged default; `useUserSettings` folds those two values into the current default
  the same way it already folded the pre-catalog Sonnet 4 id. A stored plain
  `claude-opus-4-8` is an explicit pick and is kept. Server-side defaults
  (`resolve_claude_chat_model_for_provider` with no model, approval definitions without
  launch fields, formal-review canonical model) read the constants and move with them.
- **Single 1M form.** Claude Code's model configuration states that Fable, Sonnet 5 and
  later, Haiku 5.5 and Opus 4.7 and later run the 1M window without `[1m]`, and 2.1.311
  dropped the suffix from its own picker where it has no effect while still accepting it.
  Every `[1m]` variant becomes `offered: false`; `direct_preferred` is the plain id. The
  variants stay in `direct_aliases`, so saved selections keep resolving, keep their
  effort set and show their catalog label. The OpenRouter route refusal of plain ids
  (#1043) guarded a 200K pin against the 1M variant; both forms now name one model, so
  the refusal is removed and both collapse to the family slug. The composer still moves a
  saved `[1m]` pick to the plain id on an OpenRouter account, with its notice reworded.
- **Not in scope.** Haiku 5.5 (available since 2.1.293) is not added; the Codex lineup is
  unchanged; no database migration (choices live in browser settings and explicit launch
  preferences).

## Claude upgrade evaluation, before changing the pin (2026-11-09)

Baseline **2.1.308** (2026-10-22). Fable 5.2 requires **2.1.321** (2026-11-09), npm's
`latest`/`next`; `stable` was 2.1.311. The owner's "update claude code as needed"
selects 2.1.321 exactly. Sources: [npm metadata](https://www.npmjs.com/package/@anthropic-ai/claude-code),
[changelog](https://github.com/anthropics/claude-code/blob/main/CHANGELOG.md),
[model configuration](https://code.claude.com/docs/en/model-config).

| Area | Evaluated delta 2.1.309 → 2.1.321 | G5 implication and verification |
|---|---|---|
| Model inventory and ids | 2.1.321 adds `claude-fable-5-2` as the default Fable and moves saved Fable 5.1 picks to the `fable` alias in Claude's own config; 2.1.311 drops `[1m]` from names where it has no effect but still accepts the suffix; 2.1.314 fixes dotted gateway ids (`claude-opus-4.8`) being treated as older models; 2.1.316 deprecates Opus 4.8 fast mode. | G5 passes explicit catalog ids and `modelOverrides`, so alias moves do not apply. The flag gate passed on 2.1.321; the native matrix runs every offered id, including Fable 5.2 and the dotted slugs, with efforts. |
| Headless completion and exit | 2.1.314 makes `-p` print the error and exit 1 on startup failure; 2.1.319 exits 1 when auto mode refuses with no safety check and when `--resume` loses a deferred tool call; 2.1.321 exits 1 on lost transcript lines; 2.1.318 makes Stop cancel a message sent right after a model change in headless sessions. | Exit codes stay evidence, never completion. The ten lifecycle schedules were re-recorded on 2.1.321 (question, bypass question, allow, deny, cancel, result-before-EOF, background report/EOF, interrupt idle/active) and all passed. |
| stream-json and controls | 2.1.311 escapes DEL, C1 and Unicode format characters in `json`/`stream-json` output (parsed values unchanged); 2.1.314 fixes acknowledgement of prompts holding `<bash-stdout>`-like tags; 2.1.321 denies a host's `AskUserQuestion` answer carrying `afkTimeoutMs`. | G5 parses JSON and sends no `afkTimeoutMs`; the recorded control envelopes match the admitted protocol (`protocol.rs` current-pin test). |
| Background work | 2.1.318 moves a background subagent's shell command to the background after 3 minutes on all providers; 2.1.321 tells `-p` sessions background commands stop after 10 minutes; 2.1.319 keeps instant background output. | Supervisor custody and bounded settling unchanged; background report/EOF/interrupt recordings re-taken. |
| Routing, auth and gateways | 2.1.310/2.1.314 retry without rejected betas or tool-search blocks behind gateways; 2.1.312 extends refusal fallback model switches to unrecognized names (`CLAUDE_CODE_DISABLE_REFUSAL_FALLBACK=1` opts out); 2.1.318 adds `credential_expires_at` to `-p` JSON results and a stderr warning before a claude.ai sign-in ends, and fixes `setup-token` refusal under `forceLoginOrgUUID`. | Frozen model remains the ordinary launch contract; a fallback switch would surface in the native result's model, which the matrix asserts. Open: G5 does not set `CLAUDE_CODE_DISABLE_REFUSAL_FALLBACK`; pinning the frozen model against native refusal fallback is a follow-up decision, not changed here. |
| Thinking and context | 2.1.318 removes the fix that kept earlier thinking across a resume from pre-2.1.287 sessions; 2.1.311 compacts instead of failing when a standard-context session exceeds 200K. | No G5 contract reads thinking blocks; compaction stays native. |
| Permissions and sandbox | 2.1.321 fixes read-only Bash running outside the sandbox without a prompt and permission checks judging writes from the wrong folder; 2.1.319 fixes a Bash permission bypass through an environment prefix and a worktree sandbox escape. | Tightening only; no permission expansion. Formal review's two-tool surface and `ENABLE_TOOL_SEARCH=false` retained. |

## Verification

- `cargo test -p g5-contracts` (catalog order, offered lineup, preferred forms, route
  resolution, generated projections) and `cargo test -p g5d-agent` (current-pin
  recordings).
- `scripts/check-claude-cli-flags.sh <2.1.321> 2.1.321`: OK.
- `scripts/test-native-claude-lifecycle.py --claude <2.1.321> --out tests/fixtures/agent-turn-native/claude-2.1.321`: 10 schedules PASS.
- `scripts/test-native-provider-catalog.py --claude <2.1.321> --codex <0.164.0>`: PASS, all 352 Claude and 352 Codex model × native mode × effort combinations (2026-11-09, Linux x86_64).
- Frontend: `claudeModels.baseline.test.ts` against the regenerated baseline,
  `harnessPicker.test.ts`, `useUserSettings.test.ts`, `ChatPanel.test.tsx`.
- Not executed here: worker image rebuilds and authenticated upstream acceptance of
  Fable 5.2 on a real account; account-specific availability remains live acceptance.
