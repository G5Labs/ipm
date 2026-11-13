# Finding the useful balance: ontology and native agents

Study completed 2026-09-28. This is a guide to what we actually asked agents to do,
how we checked their work, and what the results support.

This PR retains the study setup and conclusions. Per-run prompts, worktrees, patches,
logs, and machine-readable evidence are omitted. Paths in code font below identify
those historical records, rather than files available in this checkout.
The candidate changes below were applied after the initial study. The first follow-up
is complete: [nine independent review-and-fix runs](review-fix/README.md), with
summarized findings, repairs, independent checks, and effort.

**The study in brief:** agents designed two features—inviting someone to take over a
workspace, and retrying only the failed part of a workflow. We then asked two agents
to implement the retry feature from the same plan. Separately, agents reviewed that
new code and the earlier Experiment 7 code. Finally, we tried giving reviewers specific
questions, giving a coder confirmed bugs to fix, and having an agent choose review
questions without being told the bugs. Each step is explained below.

## What we wanted to learn

1. **Design:** Does access to ontology help an agent propose a better design?
2. **Implementation:** Can an agent carry the important decisions in a design into code?
3. **Review:** Can an agent find violations of those decisions before a human has to point them out?

Here, **ontology** means the repository's saved system rules, design decisions and known
implementation gaps. **Harness** means instructions about how the agent should work,
such as updating ontology during planning and reviewing the proposed design. Giving an
agent ontology to read does not necessarily mean requiring it to follow that harness.

The initial study ran **19 fresh GPT-6 Sol sessions**, all with `xhigh` reasoning, plus **three follow-up
turns in existing sessions**. GPT-6 Astra prepared the comparisons, checked the results
and supplied feedback in the supervised follow-ups. These were several different
experiments, not 19 repetitions of the same comparison. Later experiments were chosen
in response to earlier findings. The first follow-up adds **nine fresh Sol review-and-fix sessions**, three per condition; it is reported separately below.

**The main unanswered question:** we did not take the independently produced plain and
ontology-based designs and compare the code generated from each. For the coding
experiment, Astra combined and corrected the designs into **one shared plan**. That
lets us examine whether agents follow a plan, but leaves the full benefit of
ontology-based design through to implementation unmeasured.

## 1. Could agents design two new features? — six sessions

### The two feature requests

**Feature A: invite someone to take over a workspace.** Alice owns a workspace and
offers it to Bob. Bob can accept or decline; Alice can withdraw; an unanswered offer
expires after 24 hours. Sending the offer must not interrupt Alice's work or change
ownership. Acceptance must safely stop the old owner's compute and remove their
credentials before changing ownership. Bob must explicitly start compute afterward.
The existing administrator action for immediately transferring ownership must remain.

The new invitation feature applies to ordinary working workspaces. G5 also has a
special **base workspace** that reads protected/default repository branches without
its own working branch or virtual machine. Existing rules allow an administrator to
change its owner. Thus “do not offer base workspaces through the new invitation flow”
does **not** mean “remove the administrator's existing transfer ability.” This was the
distinction previously compressed into “administrator base transfer and non-base offers.”

**Feature B: retry only the failed part of a workflow.** Imagine task A fails, so its
dependent tasks B and C cannot run. Other independent tasks have already succeeded.
The owner should be able to edit A's instruction and create a new linked run containing
A, B and C. Keep the original run and its history intact; do not repeat successful
work. Show the new run in the existing workflow/chat interface. If the browser loses
the submission response and retries, return the same new run rather than creating another.

A **virtual machine (VM)** is the machine where the workspace's code runs. Whether it
is running, stopping or stopped affects which new work the server may accept.

### What each planner received

Each feature was designed once under each of these three conditions:

| Condition | Information available | Required way of working |
|---|---|---|
| Plain planning | Application source code and ordinary documentation | Use the agent's normal planning approach. No ontology or repository skills. |
| Ontology with normal planning | The same source/docs, plus ontology and its metadata | Consult the ontology; no mandatory ontology edits or separate design review. |
| Current planning harness | Ontology and the maintained planning/authoring skills; **no application source or ordinary docs** | Follow the current planning workflow, including proposed ontology changes and self-review. |

The last two conditions differ in both required procedure and source access. Their
comparison cannot tell us whether a difference came specifically from self-review,
ontology editing, or planning without source code.

### How we judged designs

Before the agents ran, we wrote **eight requirements for each feature**. We checked
each plan against them: did it state the requirement clearly, cover it partly, omit
it, contradict it, or correctly leave an implementation question for source inspection?
We also checked for invented restrictions. There was no numerical quality score.

For invitations, the requirements covered ownership changes, permissions, safe transfer,
existing exceptions, repeated/concurrent requests, recovery after interruption, the
user interface and a usable handoff to a coder. For workflow retry, they covered the
selected tasks/history, fresh task identities, module responsibilities, duplicate
requests, execution permissions, cancellation/progress, the interface and unresolved
design choices. [Full criteria and requests](protocol.md#frozen-design-obligations).

### What happened

All six plans covered the main requested feature. There were useful differences:

- **Ontology helped recover a module responsibility.** The workflow module should
  coordinate the retry, while `TaskService` creates and validates tasks and the first
  input queued to start each one, and `ChatService` creates chat sessions. The ontology-based plans made these
  responsibilities clearer than the plain plan; the full harness explicitly assigned
  the first input to `TaskService`.
- **Ontology helped distinguish new work from retrieving an existing result.** Creating
  a new retry run requires a healthy running VM. But suppose the server already created
  the run, the browser lost the response, and the VM then stopped: resending that same
  request should still retrieve the existing run. It creates no new work. The earlier
  README called these “running submission” and “historical replay.” Both ontology-based
  plans preserved the distinction. The plain plan proposed accepting new work while
  stopped, which would require an explicit change to existing policy.
- **Source inspection also helped.** Plain and ontology-with-normal-planning agents
  noticed that the current transfer API and notification access rules needed changes
  so the recipient could accept an invitation and see it in their inbox.
- **All three invitation designs failed to clearly preserve the administrator's base
  workspace exception.** The two source-reading plans explicitly prohibited it; the
  full harness narrowed the transfer rule in its proposed ontology changes.

The initial ontology had a general transfer story but **no explicit base-workspace
transfer exception**. That exception existed in ordinary docs, which the source-reading
planners did not recover. The source-free planner could not read those docs. Section 2
added `g5.lifecycle.transfer.base`; this was primarily a missing-knowledge correction,
not evidence that all three agents ignored an already explicit ontology block.

This establishes useful examples of ontology knowledge reaching a design. It does not
establish a reliable advantage for the full planning harness. Its proposed ontology
changes might help human design review, but we did not measure that benefit.
[Detailed design comparison](design-adjudication.md).

## 2. Would correcting missing facts fix the design mistake? — three sessions

We clarified three existing rules:

- An administrator may transfer a base workspace, even though the new invitation
  feature excludes it.
- Transfer must refuse older execution paths that the common stop mechanism cannot
  safely handle. That restriction should not accidentally prohibit ordinary current
  tasks that the normal stop mechanism can safely pause.
- Account/workspace management actions have their own permission checks. They do not
  all require a running VM merely because actions that execute code do.

We reran the invitation design with three fresh agents: one using corrected ontology
and normal planning, one using corrected ontology and the full planning harness, and
one using source/docs plus those facts as a short ordinary prose note, without ontology.

**Evaluation:** did the plan preserve administrator transfer of a base workspace while
keeping the new invitation feature limited to ordinary workspaces? Did it respect the
other two distinctions? **All three preserved the administrator exception**, with
different levels of detail on the other rules and remaining implementation questions.

“No additional procedure” means we added **no new mandatory workflow step**. The full
harness still required its existing ontology edits and self-review. The other agents
still planned normally.

This supports correcting missing or poorly scoped knowledge. It does not prove that
ontology's format caused the improvement: the short prose note also worked. We retested
the same problem that motivated the correction, so this is not evidence of success on
an unseen feature. [Follow-up setup and results](design-followup-results.md).

## 3. Could agents implement the same design? — two sessions

We chose **Feature B, retrying a failed workflow branch**, for implementation. Astra
combined and corrected the three designs into one [shared implementation plan](implementation-plan.md).
Both fresh coders received that exact plan and the same source/docs. One additionally
received ontology. Both used their normal coding and testing approach; neither was
required to run the full planning/review harness.

**This was not a fully plain implementation versus a full-harness implementation.**
Both received a plan informed by ontology. The difference was additional ontology
access during coding.

The extra instruction was to consult governing ontology when a consequential question
arose. However, the initial coder's public action history (local run record `../runs/2026-09-28-balance-study/i-o/initial/public-history.jsonl`)
shows no explicit ontology retrieval. This comparison tests availability with a light
instruction; it provides little evidence about active ontology use during implementation.

### What worked, and how we checked it

Both produced substantial implementations: **2,386 added application/test lines without
ontology access during coding; 1,880 with it**. Both included backend APIs, saved records
for duplicate-request recovery and run history, UI integration and tests.

Astra checked them independently using real API/database/task-scheduling code with
private PostgreSQL and Redis services. The checks exercised:

- Creating only the three intended new tasks, preserving original history and editing
  only the selected task's instruction.
- Returning the same run for repeated or simultaneous identical submissions, and
  refusing a changed request that reused the same identifier.
- Refusing invalid task selections, missing inputs, invalid permissions and workspaces
  that cannot accept new work; still returning an already accepted result after VM stop.
- Leaving no partial run after a forced database-write failure, and cancelling the new
  tasks without changing the original run.
- Preserving the model selected when the owner approved the retry.

Both passed **nine of the first ten test functions**. Both failed the model-selection
check described below. Two additional tests selected after review also failed in both.
The final expanded set therefore gave **nine passing and three failing functions for
each initial implementation**. These counts are test results, not `/10` quality scores;
one function may exercise several requirements, and untested behavior remains.

### The four shared failures, explained

| Expected behavior | What both implementations did | Where the obligation was lost |
|---|---|---|
| If the owner approves model A, queued work must still use A after the organization's default changes to B. Current account/credential permissions must still be checked. | Saved the approved choice for display, but selected the model again when starting work, allowing B to replace A. | The shared plan explicitly required preserving the approved choice. This was a failure to implement the plan. |
| A running VM remains open to new work during advance warning of a future shutdown, until the warning deadline. | Treated a scheduled shutdown record as if shutdown had already begun, rejecting a valid retry request. | The older ordinary docs contained this exception; the compact ontology and plans did not carry it forward explicitly. We corrected the ontology afterward. |
| If a workflow permits only one task at a time, two newly ready dependent tasks must not start together. | Copied the limit into the new run, but the existing scheduler did not enforce it. | Ontology and one ontology-based design named the rule. Astra's shared plan weakened it to generic “capacity” wording. This was a handoff weakness as well as an implementation gap. |
| `TaskService` must validate/create each task and its first input; `ChatService` creates the chat session. | Put first-input creation in `ChatService`, with the workflow module coordinating the pieces. | Ontology and the shared plan explicitly assigned this responsibility. This was another failure to implement the plan. |

The shutdown warning is called a **“nudge”** in the code. The one-task-at-a-time setting
is a **“job concurrency cap.”** The earlier phrase “nudge and job-cap probes” meant
automated tests of the second and third rows. For the latter test, the retried first
task was marked complete and two dependent tasks became ready; the scheduler allowed
both to start even though the limit was one.

The first three failures were reproduced by automated tests. The module-responsibility
failure was established by reading the code, separately from those tests. The database
rollback test passed: assigning work to the wrong module did not mean its transaction
was broken.

**When did these failures remain?** All four were present after the authors' normal
self-checks. The later reviews in section 5 were inspection-only and did **not** use our
review skill: both found the model-choice failure; the ontology reviewer also found the
concurrency and module-responsibility failures; neither found the shutdown-warning case.
Repairs were prohibited in that comparison. Section 7 then repaired all four selected
issues. The later [nine-run comparison](review-fix/README.md) tests full review and repair
on this same untouched retry implementation, without supplied findings.

### How much confidence these checks provide

The eight broad outcome areas were written before coding; the detailed tests evolved
as we inspected code and review findings. We corrected mistakes in evaluator setup,
including configuration and VM/shutdown fixtures, retaining earlier versions. Both
initial implementations and the later repair received the **same final twelve test
functions**. The shutdown-warning test used the real lifecycle code; the completed task
in the concurrency test was prepared test data.

We did **not** execute a real agent/provider turn inside a VM or run a browser journey.
Equal test counts do not mean the two implementations had equal overall quality.
[Full test results and limitations](implementation-results.md),
test construction record (`implementation-protocol.md`).

## 4. Could reviewers find known bugs in earlier Experiment 7 code? — four sessions

This used a different feature: **Fork & Start**, which combines creating a workspace
from an existing one, starting its VM and initiating a chat. We reviewed the preserved
ontology implementation from **Experiment 7's second run**, commit `6032975`. We did
not regenerate that feature in this study.

Four fresh agents reviewed the same code, diff, request and clarified intended behavior:

1. Normal review with source/docs.
2. Normal review with source/docs and ontology.
3. Review with the current review skill and ontology.
4. The same as 3, with only the review skill replaced by its earlier version from `ff7b5ce`.

All were asked to inspect code only: no execution, repairs or metadata edits. Thus the
last pair compares old/current **review instructions under these restrictions**, not
the entire old/current harness in normal operation.

### What we looked for

We had two confirmed bugs that we did not reveal to the reviewers:

- **Cancel could interrupt unrelated work.** Cancelling the combined Fork & Start
  operation could call the workspace-wide Stop operation, affecting another ordinary
  chat already running in that workspace. Earlier API/database checks had reproduced
  this effect.
- **A background worker discarded its queue-position updates.** It checks waiting
  operations in batches and updates their timestamps to let later operations get a
  turn. It then rolls back the database transaction, discarding those updates. With
  more than one batch, it can repeatedly revisit the same early operations. The lost
  update was confirmed in source; we did not run a sustained-load test proving later
  operations wait forever. “Runtime starvation” referred to that possible consequence.

We also checked whether reviewers would incorrectly prohibit allowed behavior:

- **Starting a chat later can be valid.** Once the system has recorded that the VM
  became ready within the Start deadline, creating the chat later on that same
  still-current VM can be allowed. The original deadline does not automatically
  become a separate deadline for chat creation. “Incarnation” meant one particular VM
  instance: authorization for it does not automatically carry over to a replacement VM.
- **Existing UI can show later progress.** A feature banner can say “chat created” and
  link to the existing chat/VM panels for execution progress. It need not duplicate all
  their status displays. We checked the source integration, not the actual browser flow.
- **Request identifiers need a declared policy.** Identifiers let the server recognize
  repeated requests. If “Fork only” and “Fork & Start” both use `request-123`, should they
  share an underlying fork or represent independent actions? Existing requirements did
  not mandate one answer. “Namespace assumption” meant a reviewer assuming these actions
  must have separate identifier scopes, then calling shared behavior a bug without an
  established requirement. An unspecified policy should be reported as a design question.

**Evaluation:** did the reviewer find each known bug, avoid falsely blocking allowed
behavior, and distinguish an undecided policy from a confirmed defect? We also checked
new findings against the actual code; findings outside our original list could be valid.

### What happened

**All four missed both known bugs.** None falsely prohibited the later chat creation
or the use of existing progress UI. The plain reviewer and the prior-skill reviewer
did overstate the requirement for separate request-identifier scopes.

All four also found other supported bugs. Examples included starting compute before
validating the user's confirmed selection, starting after a cancellation/Stop race,
and changing the accepted model before execution. Their findings differed; missing
the two target bugs does not mean their reviews had no value.

This single matched old/current-skill pair did not demonstrate either a review benefit
or a regression from the revised instructions. It also did not establish equivalence.
The earlier `/10` scores are historical records; this study used the specific checks
above. Review setup (`review-protocol.md`), [checked findings](review-adjudication.md).

### Were the two missed bugs already covered by ontology?

**Cancellation: yes.** [The operation-authority block](../../../.g5/ontology/concepts/data_model.g5.md#keep-operation-authority-distinct-from-resource-state)
explicitly requires authority over any other work affected. The original Experiment 7
designer retrieved it, but its saved design omitted an explicit case preserving unrelated
work during cancellation. None of the three ontology-enabled review traces in this study
shows retrieval of that block's text. All four reviewers did receive the broader
cancellation-scope rule in the shared review contract. This combines incomplete retrieval
with failure to turn a governing rule into a concrete check.

**Queue fairness: only general recovery rules were present.** We found no explicit
ontology commitment that a waiting batch must not indefinitely exclude other eligible
operations. More revealingly, the original Experiment 7 self-review (local run record `../runs/2026-09-28-experiment7-second-run/ontology/final/worker-artifacts/ontology-review.md`)
*did* identify unfair batching. Its repair added the timestamp update but rolled it back,
then claimed success without checking the persisted ordering. This is also a repair
verification failure. The subsequent candidate revision adds a recovery-progress
commitment; it does not prescribe a timestamp algorithm or promise strict FIFO fairness.

## 5. Could ontology help review the newly generated retry feature? — two sessions

Two fresh agents reviewed the **same ontology-access coder's initial implementation**
from section 3, commit `844536f`. Both received source/docs and the shared plan; one
also received ontology. Both reviewed normally, without running code or making repairs.
We checked each reported issue against the code and intended behavior.

- **Both** found the changing-model bug and a UI recovery bug: after the server accepted
  a retry but the response was lost, the UI could prevent retrieval of that result if
  the VM stopped or a new preview failed, even though the backend supported recovery.
- **The ontology reviewer also found** the ignored per-workflow concurrency limit and
  the first-input responsibility violation described in section 3. The responsibility
  rule was also present in the shared plan, so it was not exclusive to ontology here.
- **The reviewer without ontology also found** missing context needed to display progress
  correctly for retries of synchronization workflows, such as the target document.
- **Some claims needed correction.** For example, the ontology reviewer reported missing
  negative test cases that were actually present inside a larger test. Inspecting test
  source is different from running it, but neither justifies claiming the cases are absent.

These were complementary reviews. The other implementation did not receive equivalent
reviews, so these results cannot compare total defect counts between the two coders.
[Review comparison and evidence](review-adjudication.md#independent-review-of-i-o-retry-implementation).

The ontology reviewer had the same code and used native review, not the review skill.
Public traces show that the plain reviewer followed `job_progress_service.rs`, while
the ontology reviewer did not visibly inspect that consumer. This supports a difference
in investigation coverage. It does not establish that ontology instructions suppressed
native review, that the rule was impossible to infer, or that model capacity was exhausted.

## 6. Would specific questions help reviewers find the missed bugs? — two follow-ups

Astra returned to the original plain and current-skill Experiment 7 reviewers and asked
them to investigate two questions:

1. Could cancelling this operation affect an unrelated chat in the same workspace?
2. Do the background worker's queue-position updates survive the database transaction?

We did not supply faulty lines or patches. **Both reviewers then found both bugs.**
The evaluation was simply whether their source tracing correctly established each issue.

This shows they could reason through the bugs once attention was directed. However,
Astra already knew the answers when choosing the questions, and the reviewers reused
their existing context. It does not show that the harness can independently select the
right questions on a new feature. This is what we meant by **focused supervision**:
concrete investigative guidance from a supervisor, here Astra acting in that role.
Exact follow-up instructions (`focused-review-protocol.md`).

## 7. Could the author fix concrete review findings? — one follow-up

Astra gave the original ontology-access coder the four confirmed failures from section 3
and the expected behavior, including the corrected shutdown-warning rule. The coder
received no evaluator test code, implementation patch, new skill or prescribed sequence.

**Evaluation:** rerun the same final twelve tests and independently read the code to
check module responsibilities. **All twelve passed after repair**, and `TaskService`
now performed the required task/first-input validation and creation. The original
implementation was preserved separately from the repair.

This demonstrates successful repair from specific, correct feedback. It took another
**9m56s** in the author's existing session, excluding Astra's work finding and validating
the issues. It does not show that ontology alone caused the repair or that an automated
review would reliably have found all four problems.

The comparison still missing at that point was review against the agreed plan and governing
blocks **without being given the defects**, followed by repair and independent recheck.
It has now been run in the [first follow-up](review-fix/README.md). Section 7 supplied
the answers about what was wrong; it tested the author's ability to act on that feedback.

The UI recovery and synchronization-progress problems were outside this four-issue
repair and remained open. Browser behavior, real provider execution and broader
concurrency stress were still unverified. Passing these tests did not make the whole
PR ready. [Repair results](repair-results.md).

## 8. Could an agent choose useful review questions itself? — two sessions

To test whether we could reduce the supervision in section 6, a fresh agent received
the same Experiment 7 source, ontology and intended behavior as the earlier normal
ontology reviewer. It could not see prior findings or the known-bug list. It selected
up to two risk questions. We passed them **unchanged** to another fresh reviewer.

It chose questions about cancelling a chat after its first input was created, and
about request identifiers shared between actions. We judged the resulting review by
the same known-bug and unsupported-claim checks as section 4.

**The reviewer still missed both target bugs.** It found one other supported recovery
bug: the combined operation could join a fork already in progress without saving a
reliable link to it. After a lost response and restart, it could fail to rediscover the
completed fork and instead fail because the workspace name was already taken.

It also made two overstated claims: that the queued chat input could not be cancelled
(ordinary chat/task controls provided a path), and that action types necessarily needed
separate request-identifier scopes.

This attempt produced a useful finding but did not establish a reliable replacement
for informed supervision. The questions were explicitly described as leads, not new
requirements; that warning did not prevent unsupported claims. We did not add a
mandatory question-selection phase. [Setup and results](selector-results.md).

## Observed effort

These are observed worker times and output-token counts, including reported reasoning
output. They exclude Astra's preparation, checking and supervision. They are not dollar
costs or reliable averages.

| Work | Elapsed time | Output tokens |
|---|---:|---:|
| Invitation design — plain | 101 seconds | 6,787 |
| Invitation design — ontology, normal planning | 129 seconds | 6,014 |
| Invitation design — current planning harness | 225 seconds | 16,345 |
| Retry design — plain | 143 seconds | 9,848 |
| Retry design — ontology, normal planning | 128 seconds | 8,274 |
| Retry design — current planning harness | 269 seconds | 18,490 |
| Coding shared plan — without / with ontology access | 34m04s / 26m54s | 79,868 / 67,623 |
| Experiment 7 review — plain | 4m13s | 13,030 |
| Experiment 7 review — ontology, normal review | 4m45s | 14,453 |
| Experiment 7 review — current skill | 4m42s | 15,859 |
| Experiment 7 review — prior skill | 4m32s | 14,291 |
| Two-question follow-up — plain / current-skill reviewer | 48 / 76 seconds extra | 2,866 / 4,411 extra |
| Four-issue repair | 9m56s extra | 28,041 extra |
| Automatic question selection + review / original normal ontology review | 4m06s / 4m45s | 14,439 / 14,453 |

“Roughly twice” referred to **current-harness planning versus normal planning with
ontology** in the first six rows. It did not mean twice the total development time.
The harness also required extra outputs and withheld source access. The coder with
ontology was faster in this one pair. None of these individual comparisons establishes
a general cost advantage or disadvantage.

Planning overhead is not the current optimization priority. The next experiments should
prioritize correctly implemented commitments, reliable review and total repair effort.

All trial time/token records, including corrected-fact designs and the other reviews,
are in initial telemetry (`telemetry.json`) and follow-up telemetry (`followup-telemetry.json`).

## What changed, and what decisions remain open?

Maintained changes clarified existing rules: what a timely Start permits, when a repeated
request only reads an existing result, the administrator's base-workspace exception,
the limited refusal for older execution paths, which actions require a live VM, and
accepting work during a future shutdown warning. **No new mandatory harness procedure
was added during the completed 19-session study.**

Checkpoint `1738018` was pushed before exploration; `a839c11` saved the design results
and rule corrections; `ca6a582` saved the remaining study and shutdown-warning correction.
Experimental application changes remain separate as saved patches; they were not merged
into the maintained application. Private experiment services are stopped and records retained.

The evidence supports three bounded observations: ontology supplied useful design
knowledge; explicit design instructions still failed to reach code; and agents could
repair selected defects after accurate feedback. Reliable independent discovery remains
the harder, unresolved part. A mandatory supervisor session is not established as the
best solution; the supervision used here has a real, separately incurred cost.

Retaining design → normal coding → review is provisional and largely matches the
existing harness. It is **not a newly demonstrated solution** to its reliability problems.
The workflow failed to prevent important defects; these experiments do not establish
that it caused them. Neither a wholesale revert nor another mandatory phase is justified
by this study alone.

### Candidate improvements applied after the study

Following the discussion, the user approved four narrow changes: a recovery-progress
ontology commitment; governing links from compound/lifecycle operations to cancellation
authority and recovery; a shared set of specific commitments in the saved design; and
review coverage plus repair closure based on evidence of the actual promised behavior.
See the handoff (`SESSION-HANDOFF.md#applied-improvements`) for scope and provenance.

The pre-revision harness is preserved at `ca6a582`. The revised planning/review skills
refine existing phases; implementation remains native. No mandatory second review,
grilling phase or coding checkpoints were added. The first review comparison found uneven
repair coverage and remaining defects; it did not establish an overall winner. The revised planning handoff remains untested.

### First follow-up completed: review and fix without supplied bugs

Nine fresh Sol workers reviewed and repaired the same initial retry code. All received
the original plan, corrected ontology and equivalent private runtimes. Three used normal
review, three the old review skill, and three the revised skill. **There was no code-only
group.** Findings were saved before repairs; Astra checked final code independently.

**Did the skill help?** Old-skill runs all repaired the tested concurrency limit; revised
runs all preserved the accepted model. Normal review with ontology did those in zero and
one attempts respectively. The groups repaired different defects, even when test totals
matched. Both skill groups used more time and output tokens. Specific repairs were verified;
a general skill advantage or better results per unit of effort was not established.

**Could review reliably find and fix everything?** None of the nine did. The records now
separate the failure stages: no worker visibly retrieved the shutdown exception; all
retrieved concurrency and ownership rules but some missed their violations; one found
model drift and applied the wrong repair; another found missing progress context but
copied too much of it. Three repaired a UI component while missing the parent that hid it.
One old and one revised run also introduced a restriction on permitted successful-command
reuse. Useful additional discoveries and unsupported claims are recorded separately.

Workers used 423,913 output tokens. Astra preparation and evaluation are accounted for
separately. [Readable results, costs and limits](review-fix/README.md),
[which worker failed at which stage](review-fix/issue-trace.md).

### Selected next experiments — not yet run

The goal is implementation consistency with the original **ontology diff and approved design,
including the relevant governing concepts, contracts and system invariants**. The next
concrete plan (`review-fix/NEXT-EXPERIMENT.md`) splits discovery from repair:

| Order | Comparison | What it decides |
|---|---|---|
| **1. Discover issues** | Three fresh independent general reviewers versus three fresh reviewers focused on intent/invariants, behavior/compatibility, and architecture/code quality. Same full inputs and allowance; identical consolidation; three paired repetitions. No repairs. | Whether different primary responsibilities improve collective coverage, without losing minority findings, missing interactions or adding false claims. |
| **2. Repair identical findings** | One common packet of newly discovered issues given to three fresh fixers. Preserve the first fix; then allow at most two additional repair rounds, each informed by a fresh review. | How consistently agents repair the same issues, what additional rounds actually fix, what they break, and whether a clean review can be false. |
| **3. Validate benefit and effort** | If loops help, compare with a longer single repair at the same total allowance. Validate a promising complete workflow on two other features with actual approved ontology diffs. | Whether added agents/rounds earn their cost and the result transfers beyond known retry defects. |

All reviewers share a short [qualitative rubric](review-fix/REVIEW-RUBRIC.md), using Supported,
Partial, Violated and Unverified with evidence. The seven known outcomes remain concrete controls;
review also covers all material intent, governing rules and affected behavior/design/code quality.
Do not optimize for issue count or a single /10 average. End the report with actual implementation
alignment after the first fix and after the bounded loop, not just a review finding count.

Prior findings and evaluator answers remain hidden from discovery workers. Consolidation retains
every claim and does not use majority voting. The repair experiment shares only newly generated
findings and holds that packet identical across fixers. Sol performs the workflow; Astra evaluates
independently without supplying repair hints. Costs and false-clean stops are explicit.

The retry coder's approved feature contract was the shared plan, not the broader D2 H ontology
proposal. Label its feature-intent alignment accordingly; do not silently turn that draft into
a requirement. Independent design-to-code comparisons remain a separate question. No split review,
new rubric or loop has been adopted as a mandatory maintained-harness change.

**About “grill-me”:** assuming Matt Pocock's skill, its current upstream
[grilling instructions](https://github.com/mattpocock/skills/blob/main/skills/productivity/grilling/SKILL.md)
organize a user interview around unresolved decisions. That can help clarify a design,
including for a capable model. It is not evidence of reliable code inspection. A pass
that challenges how code could violate each agreed commitment is a useful experimental
variant; it still needs source tracing and checks, and its value remains untested here.

## Evidence and limits

The section links above lead to protocols and results. Additional records:

- [Resolution of earlier evaluator ambiguities](../ontology_harness_tuning/ontology-evaluator-adjudication.md).
- Original plans and proposed ontology edits (`designs/README.md`),
  saved implementation patches (`implementations/`), review reports (`reviews/README.md`).
- [Final source checks of the repair and automatic review](repair-and-selector-adjudication.md).
- [Earlier evidence reassessment](../ontology_harness_tuning/ontology-harness-evidence-reassessment.md), including
  [Experiment 5](../ontology_harness_tuning/ontology-experiment-5-history-audit.md) and
  [Experiment 7](../ontology_harness_tuning/ontology-experiment-7-history-audit.md).
- The older Lane 1 experiment archive in the separate G5 repository
  supplied background examples. Those earlier reports are not additional repetitions
  in this study; some old raw histories were no longer available.

Astra knew which condition produced each result; evaluation was **not blind**. In the initial study, each
configuration ran once per assigned task; the first review-and-fix follow-up used three
runs per condition and separate source adjudicators with the mapping withheld where feasible. We adapted later experiments to earlier
findings, used shared host resources, and sometimes compared different information
packages. We therefore report observed cases, not general success rates or causal
proof of harness benefit. Supported source findings, executed tests and unverified
browser/provider behavior are kept separate.

Preparation scripts, prompts, plans, patches, reports and concise execution logs are
tracked here. Full public action histories and intermediate records remain local under
`../runs/2026-09-28-balance-study/`; private reasoning is excluded. Agents were instructed
to stay within assigned inputs, rather than isolated by filesystem permissions; global
tool/skill descriptions and the parent working directory remained available. Reproducing
the runs elsewhere requires adjusting local paths/ports and providing equivalent services.
