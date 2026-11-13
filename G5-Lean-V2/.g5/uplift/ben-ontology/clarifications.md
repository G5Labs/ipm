# Open clarification index

These agent questions do not amend Ben's statements. Each names a provisional option
so analysis can proceed without silently substituting code behavior for human intent.
No application implementation depends on answering them during this uplift.

| Question | Provisional choice | Consequence / evidence |
| --- | --- | --- |
| [Physical VM uniqueness](../../ontology/software_architecture/workspace_vm.g5.md#g5.vm.physical-uniqueness-question) | Preserve physical singleton; require independent termination before replacement. | Logical custody model is insufficient physical evidence; RUNTIME-Q-PHYSICAL. |
| [Dispatcher request isolation](../../ontology/software_architecture/workspace_vm.g5.md#g5.vm.request-isolation-question) | Preserve stated per-request process separation. | In-process filesystem dispatch remains RUNTIME-003. |
| [Existing-user invitation](../../ontology/features/organizations.g5.md#g5.organization.existing-user-invitation-question) | Grant at verified login. | Immediate add is CI-Q1 until the exception is explicitly selected. |
| [Local process mode](../../ontology/software_architecture/hosts.g5.md#g5.runtime.local-mode-scope) | Keep production microVM-only; process mode supplies development assurance only. | RUNTIME-Q-LOCAL; explicit acknowledgment is implemented, network-local enforcement is not established. |
| [Base branch creation](../../ontology/software_architecture/git.g5.md#g5.git.base-create-exception) | Preserve supplied no-push rule. | GIT-Q-BASE; earlier merged human exception exists, later source is absolute. |
| [Viewer collaboration](../../ontology/features/organizations.g5.md#g5.organization.viewer-collaboration-question) | Preserve viewer role ceiling; collaboration requires role eligibility. | Current reply/vote/reader-suggestion routes use read authority; clarify whether those actions are intended exceptions. |
| [Spend-scope policy](../../ontology/features/chat.g5.md#g5.chat.spend-scope-question) | Retain period accounting; threshold enforcement provisionally deferred. | Exposed settings and historical post-MVP scope need reconciliation; this is not proof of a working spend cap. |
| [Granular grant lifecycle](../../ontology/features/organizations.g5.md#g5.organization.grant-lifecycle-question) | Preserve supported grants within current role ceilings while reconciling retirement. | Earlier retirement design and active consumers conflict in scope; unrestricted union is not adopted. CI-G3 remains separate. |
| [Admission continuity](../../ontology/features/users.g5.md#g5.user.admission-continuity-question) | Provisionally preserve verified invitation/current-member exceptions to first-admission allowlisting. | The original policy authority was not recovered. Fresh AdmissionContinuity checks this conditional option; it does not settle invitation revocation or prove current code enforces the entire flow. |
| [Review verdict and formal approval](../../ontology/features/workspaces.g5.md#g5.workspace.approval-review-verdict-question) | Require eligible addressed-request or existing owner authority; published review alone grants no additional authority. | ReviewVerdictSync compares current requestless sync with this provisional rule. A distinct review-verdict grant and withdrawal policy requires a human decision. |

Read together, these choices preserve legitimate invitation/transfer/operation flows;
they do not introduce a blanket refusal policy. Code gaps with clear supplied intent
(cascade, review merge, ignored files, scoped settings) need implementation or an
explicit later amendment rather than an unnecessary clarification question.
