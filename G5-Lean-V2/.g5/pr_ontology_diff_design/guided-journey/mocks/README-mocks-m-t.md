# Mocks M–T — Guided panel improvements (2026-10-20)

Review material for the follow-up to PR #444 (Mock G shipped). Each mock keeps the
`g5.workbench.journey*` contract (fixed catalog over ribbon actions, evidence or owner
marks, rosters between phases, chips) and changes how the panel reads and behaves.
Screenshots: `../screenshots/mock-m-*.png`, `mock-n-*.png`, `mock-o-*.png`. Open the
HTML files directly; each has a mock-only state switch at the top of the sidebar.

| Mock | Shows | Guideline it answers | Contract impact |
|---|---|---|---|
| **M — Focused checklist** | Only the next step is open; done and later steps are one-line rows that expand on click. SE folds 6 steps into 4 outcome-worded steps with a payoff line. A step whose job is running shows **Running · started … · view job**. **Skip** is separate from Done; **Undo mark / Undo skip** replace "✓ Not done". Status is a text label, done steps are muted (no strikethrough), the next step carries `aria-current="step"`. | 3–6 visible steps (Atlassian, Pluma, SaaSUI); steps as outcomes not features (SaaSUI); current/in-progress status (every stepper spec); skip ≠ dismiss, reversible (SaaSUI, Appcues); state not by colour alone (WCAG 1.4.1) | UI within `journey.steps`/`journey.state`; a skipped mark is a new mark kind (`journey.marks`); a running state reads the existing jobs/conversations bound to the action id |
| **N — Role-aware summary** | The summary card is written for the viewer: an **approver** sees "Your turn" with Approve / Request changes / read the explainer first, and the roster row marks "you are next"; an **owner** whose approver requested changes sees that as the next action (the Mock K loop as a state, not an edge); a **reader** sees who owns it and what is next, with no disabled buttons. Steps name who runs them instead of greying out. | Tailor the first experience by role (Appcues); no mandatory flow that blocks experienced users; wizards name the next action specifically (NN/g) | New `journey` summary rule per viewer role; "Request changes" needs the approvals model to carry a request-changes decision |
| **O — Retire and complete** | Three states: a **first workspace** opens Guided expanded (as today); **after the person's first merge** Guided opens collapsed and its header carries the next step; **journey complete** retires the timeline into one card (merged where/when, counts, open the parent / fork again, show the timeline). A per-person setting "Open Guided by default: Automatically / Always / Never". | Retire or collapse once the user is clearly active, don't nag established users, keep it recallable (SaaSUI, NN/g "easy to dismiss and recall"); clear finished state | Amends `journey.default-open` (per-person rule + merge-based automatic mode) and the complete state of `journey` |

Not mocked, recommended alongside: derive more step evidence from existing facts
(preview session, review conversation, design document) so progress does not depend on
the owner remembering to mark; record `source: guided` on action dispatch for completion
and drop-off metrics.

# Mocks P–T — from the UI-Skills study (2026-10-20, same day)

Second round, after reading the curated skills in `~/UI-Skills` (Refactoring UI, Nielsen
heuristics, Norman, Saffer microinteractions, WCAG 2.2, Vercel web guidelines, taste-skill
app-screen rules, impeccable, hallmark slop gates, Emil Kowalski's animation standards,
LottieFiles motion principles, interface-design, baseline-ui). Three parallel reviews of the
shipped panel and of M–O converged on the same findings; P–T answer them. These mocks use
the shipped dark tokens (`--g5-phase-*`, `--g5-ok/info/warn`, bg0–bg2) and two added greys:
muted `rgb(236 242 255 / 0.68)` (8.5:1 on bg1) and meta `rgb(236 242 255 / 0.56)` (6.2:1).

## What the skills flagged (shipped panel, and where M–O still fail)

| Finding | Skill source | Fix carried into P–T |
|---|---|---|
| Upcoming phases dimmed with `opacity-60` → 3.9:1 / 2.6:1 text, 1.22:1 ring | WCAG 1.4.3 / 1.4.11; Refactoring UI; taste colour §3 | Never dim by opacity. Muted token for upcoming text; ring stroke white 35% (3.3:1) |
| Eyebrow label (`JOURNEY SUMMARY`), tracked caps, index numbers in nodes; M's `DONE / NEXT / RUNNING`, N's `YOUR TURN`, O's `OPEN GUIDED BY DEFAULT` | impeccable craft-floor ("a ban, not a default"); taste app-screens §13; hallmark anti-patterns | No eyebrow; the first sentence is the heading. Status words sentence-case 12px. Nodes show check / dot / empty ring |
| Accent in eight places + 4px halo on the active node; N has three accent families | interface-design "one accent"; taste "accent on data or one selected state", "no glows" | One accent element: the next step's button. Summary is a bg2 step with no border |
| Six font sizes between 9 and 13px, five of them within 3px | Refactoring UI typography; taste type §2; web-typography | 14 / 13 / 12 only, line-height 20 / 18 / 16, weights 400 / 500 |
| Step rows 6px apart, phases 12px; two-line rows on a 34px pitch | Refactoring UI spacing; taste app §7 | Rows 32px (40 with a note), phases 24px apart |
| Strikethrough on done labels | taste; Refactoring UI | Muted colour and a check |
| Hit areas: buttons 28px, chips 20px, avatars 24px overlapped with no gap | WCAG 2.5.8; interface-design "40 minimum"; ui-ux-pro-max target-size | Buttons 32px, chips 24px, ring 20px visible / 40px hit |
| No `prefers-reduced-motion`; `animate-spin` loops forever; roster animates `margin` on hover | Emil animate §7; motion-design context-adaptation; fixing-motion-performance | Loops stop, "Running" is a word; transform/opacity only |
| No running state; a dispatched job looks untouched | Nielsen #1; Saffer core states | Row status "Running 2 min", link to the job (P, Q, R, S) |
| "Mark done / Not done" share a glyph; reason for disabled lives in `title` only; failure only as a toast | Saffer trigger/feedback; Norman constraints; WCAG 1.4.13 | Ring is the control (`aria-pressed`), 8s Undo window, inline failure with `role="alert"`, reasons in words (Q) |
| `ie-rebase` and `ie-pull-request` both bind `create-pr`: one control, two steps | Norman mapping; Saffer rules | One step, "Rebase and open the pull request", evidence = PR exists |
| Copy narrates features ("Generate an explainer", "The review skill, as a conversation"); middle-dot meta strings | impeccable onboard/clarify; taste copy §6; frontend-design calibration | Outcome labels, sentences with commas, no middle dots |
| Error as a warn-coloured parenthetical inside the summary sentence | taste app §11 honest states; baseline-ui | One line: what failed, when, last good, Retry |
| Loading is a text string; no skeleton | baseline-ui structural skeletons | Flat bars at real sizes, no shimmer (S) |

## The mocks

| Mock | Shows | Rules | Contract impact |
|---|---|---|---|
| **P — Evidence ledger** (`mock-p-evidence-ledger.html`) | Art direction: "the workspace's evidence becomes the timeline". One row per step: outcome left, proof right (document, PR, "Running 2 min", or an empty dashed slot). The next step is the first empty slot and holds the panel's only accent, its ribbon button. Approvals are rows of the same ledger. No circles, numbers, eyebrow, halo or caps. Density switch moves type, pitch and padding together (40px / 28px). Marks-error state as one sentence with Retry. | taste §4 idea-from-subject, app §9 / §10 / §11; impeccable operate; Refactoring UI hierarchy | Presentation only; keeps the catalog, evidence rules, marks and rosters. Copy deck changes step labels (`journey.steps` labels) |
| **Q — Step row states** (`mock-q-step-row-states.html`) | One row in nine states at 318px: upcoming, next, running, done by evidence, done by mark with undo window, saving, save failed, reader, action removed from ribbon. The ring is the Mark-done control with a 40px hit area and `aria-pressed`. Live row driven through the state machine with a polite live region. Overlays: hit areas, reduced motion. | Saffer trigger · rules · feedback · loop; Norman signifiers; baseline-ui; WCAG 2.5.8 / 4.1.3 | A "skip" mark kind; optimistic mark with rollback; failure state in the marks mutation |
| **R — Motion spec** (`mock-r-motion-spec.html`) | Seven events with tier, choreography, reduced-motion column, playable at 1× and 4× slower: expand/collapse (180/120ms), mark and undo (ring fill, check .9→1, next word +60ms), evidence arrives (chips 4px, 50ms stagger), running (2px ring, optional 3s breathe), phase move (240ms, ease-move collapse, blur crossfade), approval arrives (ring colour + 1.06 pop), journey complete (30ms stagger out, card .96 in). Tokens: `--g5-ease-out`, `--g5-ease-move`, 120/180/240ms. | Emil STANDARDS (durations, easing, interruptibility), animate §7, motion-design timing/choreography | Three CSS tokens in `index.css`; no behaviour change |
| **S — Honest states** (`mock-s-honest-states.html`) | Eight frames: loading skeleton at real sizes; marks could not load (evidence rows stay true, mark-only rows read "Unknown"); pull requests unavailable; reader; every step done but phase not moved; no approvers set; offline; worst case (52-char label, 30-char no-space label, 1 h 12 min running, hostile file name middle-truncated, 8 approvers with "+5", grapheme initials). | taste app §11; break-ui catalog; Nielsen #1 / #9; hallmark eight states | Loading and error states in `journey.state` presentation; "+N" roster disclosure in `journey.gates` |
| **T — Keyboard and screen reader** (`mock-t-keyboard-screenreader.html`) | Real focus order with numbered pills: five Tab stops (summary action, next step's ring, its action, finished roster, next phase heading); roving tabindex inside the list; 2px ring with 2px offset and `scroll-margin-top`; VoiceOver transcript per stop; polite live region on marks. Markup outline and the WCAG criteria it satisfies. | WCAG 1.3.1, 1.4.1, 1.4.11, 2.4.3/7/11, 2.5.8, 4.1.3; A11Y patterns | Markup changes in `GuidedJourneySection.tsx` only |

## Copy deck (used by P–T)

Summary: lead = the next step's outcome ("Explain the design"); sub = "System Engineering,
1 of 4 done. You own this workspace." Steps: Capture what the business wants · Surface the
open questions · Show it as a mock · Plan the implementation · Explain the design · Build the
code · Get the code reviewed · Rebase and open the pull request · Land it. Notes name the
payoff, never the UI ("One page a reviewer reads in a minute"). Buttons keep the ribbon's
names (contract). Marks: "Mark done" / "Undo" / "Skip". Errors: "Couldn't load your marks at
14:06. Last good 14:02. Retry". Approvals: "Jin Lee approved 2 h ago", "1 of 2, Jada next".

## The cut list (Steve Jobs review, no mock needed)

Cut from the shipped panel: the "Journey summary" label; the row button when it equals the
summary button; phase sublines that repeat the chevrons; notes under done steps; "No approvals
required / Request approval" rows; rosters for phases not yet reached; "Not done"; the
`ie-rebase` step (merged into the pull-request step). Result: 3 phases, 9 steps, one visible
button, one roster, and the sentence that names the next thing.

Recommended order: Q's row and the type/spacing tokens first (every other mock sits on them),
then P as the direction decision against G/M, then R, S and T as implementation specs.

# Mock U — the combined panel (2026-10-20)

`mock-u-combined.html` puts Q, R, S and T together in one working 318px panel. It is the one
to review; Q–T remain as the per-concern specs behind it.

- **Q in it:** 32px rows, 20px ring with a 40px hit area as the Mark-done control
  (`aria-pressed`), sentence-case status words, muted done rows (no strikethrough),
  optimistic mark with an 8-second Undo, inline "Couldn't save your mark. Retry" with
  `role="alert"`, "Steps that produce a document complete themselves" hint until the first
  mark, reader rows that name who acts, non-next rows that fold open on click.
- **R in it:** `--g5-ease-out`, `--g5-ease-move`, 120/180/240ms; folds 180/120ms; ring fill and
  check .9→1; the next row's word 60ms later; chips rise 4px with a 50ms stagger; running ring
  breathes over 3s; phase move with a 2px blur crossfade; approval ring colour and 1.06 pop;
  completion fades rows out and scales the card in. "4× slower" and "Reduced motion" toggles.
- **S in it:** scenario switch for loading (flat bars at real sizes), marks error (summary
  line with last-good time and Retry; mark-only rows read "Unknown"), pull requests
  unavailable (in place, on the two rows that depend on them), offline, reader, worst case
  (52-char label, 30-char label without spaces, 1 h 12 min job, hostile file name,
  "+5" roster).
- **T in it:** `nav` + `h3` per phase + `ol`, `aria-current="step"`, hidden Done/Next/To do
  prefixes, six Tab stops (summary action, finished phase heading, its roster, next ring, next
  action, next phase heading) with roving tabindex (↑ ↓) inside the list, 2px focus ring with
  offset, `scroll-margin-top`, polite live region, transcript of what VoiceOver says at each
  stop. "Focus order" and "Hit areas" overlays.
- **Drive:** "Jada approves SE", "Next save fails", and the panel's own buttons walk the whole
  journey: mark, run, evidence arrives, move to Integration Engineering, open the PR, land it,
  completion card. Screenshots: `mock-u-combined.png`, `mock-u-focus-and-hits.png`,
  `mock-u-marked-undo.png`, `mock-u-marks-error.png`, `mock-u-worst-case.png`,
  `mock-u-complete.png`.

Not in U by design: P's ledger layout (a competing direction, decide G/M-style rows vs P's
ledger first), N's role-aware summary and O's default-open rules (contract changes, reviewed
separately).
