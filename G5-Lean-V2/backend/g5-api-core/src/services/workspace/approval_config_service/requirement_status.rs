//! The one requirement evaluator (g5.governance.approval.status): overall
//! status, aggregate commit currentness, the path to approval and the
//! recipients a request should address, all from the same counted evidence.
//! Merge admission and the approvals projection both call [`evaluate`].

use g5_contracts::types::{MergeSourceHead, SourceCommitVectorPolicy};
use uuid::Uuid;

use super::{normalized_head_vector, ApprovalDecisionLike};

/// The overall status vocabulary; request lifecycle states are not statuses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RequirementStatus {
    Approved,
    Rejected,
    Pending,
}

impl RequirementStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Pending => "pending",
        }
    }
}

/// One configured approver, identified the way requests and decisions are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewerRef {
    /// `agent` or `human`.
    pub actor_type: &'static str,
    pub user_id: Option<Uuid>,
    pub display_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurrentnessState {
    Matching,
    Changed,
    Unknown,
    /// No counted approved or rejected evidence.
    None,
}

impl CurrentnessState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Matching => "matching",
            Self::Changed => "changed",
            Self::Unknown => "unknown",
            Self::None => "none",
        }
    }
}

/// Aggregate currentness of counted evidence; informational only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Currentness {
    pub state: CurrentnessState,
    pub not_current: Vec<ReviewerRef>,
}

/// The remaining path for a Rejected requirement, or a Pending one after a
/// response; absent for untouched or approved requirements.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalPath {
    HumansRejected {
        humans: Vec<ReviewerRef>,
        /// All-of with other configured humans who must also approve.
        others_required: bool,
    },
    AgentRejected {
        agent_name: String,
        /// A configured human approval may override the agent rejection.
        human_override: bool,
    },
    Needed {
        approvers: Vec<ReviewerRef>,
        require_all: bool,
        all_humans_required: bool,
    },
}

/// Recipients one request should address; active requests are excluded.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecipientNeeds {
    pub agent: bool,
    pub humans: Vec<Uuid>,
    /// Choose exactly one of these at random before display.
    pub random_human_from: Vec<Uuid>,
}

impl RecipientNeeds {
    fn is_empty(&self) -> bool {
        !self.agent && self.humans.is_empty() && self.random_human_from.is_empty()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RequestNeeds {
    /// What Request all and Request outstanding approvals send now.
    pub outstanding: RecipientNeeds,
    /// What Request all sends when a satisfied requirement is included.
    pub repeat: RecipientNeeds,
    pub agent_requestable: bool,
    pub agent_approved: bool,
}

/// How an overridable agent rejection reads (g5.governance.approval.status):
/// a display of counted evidence, never a status or stored decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentEscalationState {
    /// The counted agent rejection is the only rejection and an eligible
    /// human who may be requested could still override it.
    NeedsHumanEscalation,
    /// A counted eligible human approval overrides the agent rejection.
    OverriddenByHuman,
}

impl AgentEscalationState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NeedsHumanEscalation => "needsHumanEscalation",
            Self::OverriddenByHuman => "overriddenByHuman",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentEscalation {
    pub state: AgentEscalationState,
    /// The counted agent decision the display describes.
    pub agent_review_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequirementEvaluation {
    pub(crate) status: RequirementStatus,
    pub currentness: Currentness,
    pub path: Option<ApprovalPath>,
    pub needs: RequestNeeds,
    pub agent_escalation: Option<AgentEscalation>,
}

/// One configured human of the latest definition version.
#[derive(Debug, Clone)]
pub(crate) struct HumanCandidate {
    pub user_id: Uuid,
    pub display_name: String,
    /// Counts toward quorum (false only for a disallowed self approver).
    pub eligible: bool,
    /// May receive a new request: eligible and an active member.
    pub requestable: bool,
}

/// One request of the requirement, any version or state.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RequestFact {
    pub actor_type_agent: bool,
    pub actor_user_id: Option<Uuid>,
    /// Pending and active under the request PR rule.
    pub active: bool,
}

pub(crate) struct RequirementInput<'a, T: ApprovalDecisionLike> {
    pub require_all: bool,
    pub human_override: bool,
    pub agent_configured: bool,
    pub agent_name: String,
    pub humans: Vec<HumanCandidate>,
    /// Decisions in counted scope (human PR scope already applied), newest first.
    pub statuses: Vec<&'a T>,
    pub requests: Vec<RequestFact>,
    pub boundary: i32,
    /// `None` when the current source vector cannot be read.
    pub current_heads: Option<&'a [MergeSourceHead]>,
    pub policy: SourceCommitVectorPolicy,
}

/// The decision that counts for one actor slot: the latest at an eligible
/// definition version and, under MAKE_PENDING, on the current source vector.
/// A latest `deferred` (withdrawn) decision is returned and reads as no verdict.
pub(super) fn counted_decision<'a, T: ApprovalDecisionLike + 'a>(
    statuses: impl IntoIterator<Item = &'a T>,
    actor_type: &str,
    actor_user_id: Option<Uuid>,
    boundary: i32,
    current_heads: Option<&[MergeSourceHead]>,
    policy: SourceCommitVectorPolicy,
) -> Option<&'a T> {
    statuses.into_iter().find(|status| {
        status.actor_type() == actor_type
            && status.actor_user_id() == actor_user_id
            && status.definition_version() >= boundary
            && match policy {
                SourceCommitVectorPolicy::Ignore => true,
                SourceCommitVectorPolicy::MakePending => current_heads.is_some_and(|current| {
                    normalized_head_vector(status.reviewed_git_heads())
                        .is_ok_and(|heads| heads == current)
                }),
            }
    })
}

/// A counted approved/rejected verdict; withdrawal and absence carry none.
fn verdict<T: ApprovalDecisionLike>(decision: Option<&T>) -> Option<&'static str> {
    match decision.map(ApprovalDecisionLike::outcome) {
        Some("approved") => Some("approved"),
        Some("rejected") => Some("rejected"),
        _ => None,
    }
}

pub(crate) fn evaluate<T: ApprovalDecisionLike>(
    input: &RequirementInput<'_, T>,
) -> RequirementEvaluation {
    let counted = |actor_type: &str, user_id: Option<Uuid>| {
        counted_decision(
            input.statuses.iter().copied(),
            actor_type,
            user_id,
            input.boundary,
            input.current_heads,
            input.policy,
        )
    };
    let eligible: Vec<&HumanCandidate> = input.humans.iter().filter(|h| h.eligible).collect();
    let human_decisions: Vec<(&HumanCandidate, Option<&T>)> = eligible
        .iter()
        .map(|human| (*human, counted("human", Some(human.user_id))))
        .collect();
    let human_verdict = |human: &HumanCandidate| {
        human_decisions
            .iter()
            .find(|(candidate, _)| candidate.user_id == human.user_id)
            .and_then(|(_, decision)| verdict(*decision))
    };
    let agent_decision = input
        .agent_configured
        .then(|| counted("agent", None))
        .flatten();
    let agent_verdict = verdict(agent_decision);

    // Quorum (g5.governance.approval.quorum): a human no always vetoes; a
    // human yes overrides an agent no only in Any-of when allowed.
    let rejecting_humans: Vec<&HumanCandidate> = eligible
        .iter()
        .copied()
        .filter(|human| human_verdict(human) == Some("rejected"))
        .collect();
    let human_yes = eligible
        .iter()
        .any(|human| human_verdict(human) == Some("approved"));
    let all_humans_yes = input.humans.is_empty()
        || (!eligible.is_empty()
            && eligible
                .iter()
                .all(|human| human_verdict(human) == Some("approved")));
    let agent_rejected = agent_verdict == Some("rejected");
    let agent_yes = agent_verdict == Some("approved");
    let override_allowed = !input.require_all && input.human_override;
    let override_agent = override_allowed && human_yes;
    let has_yes = human_yes || agent_yes;
    let approved = if input.require_all {
        all_humans_yes && (!input.agent_configured || agent_yes) && has_yes
    } else {
        has_yes
    };
    let status = if !rejecting_humans.is_empty() || (agent_rejected && !override_agent) {
        RequirementStatus::Rejected
    } else if approved {
        RequirementStatus::Approved
    } else {
        RequirementStatus::Pending
    };

    let agent_ref = || ReviewerRef {
        actor_type: "agent",
        user_id: None,
        display_name: input.agent_name.clone(),
    };
    let human_ref = |human: &HumanCandidate| ReviewerRef {
        actor_type: "human",
        user_id: Some(human.user_id),
        display_name: human.display_name.clone(),
    };

    // Currentness: counted verdicts compared with the current source vector.
    let mut evidence: Vec<(ReviewerRef, &T)> = Vec::new();
    if let Some(decision) = agent_decision.filter(|_| agent_verdict.is_some()) {
        evidence.push((agent_ref(), decision));
    }
    for (human, decision) in &human_decisions {
        if let Some(decision) = decision.filter(|_| human_verdict(human).is_some()) {
            evidence.push((human_ref(human), decision));
        }
    }
    let mut not_current = Vec::new();
    let mut unknown = false;
    for (reviewer, decision) in &evidence {
        match (
            input.current_heads,
            normalized_head_vector(decision.reviewed_git_heads()),
        ) {
            (Some(current), Ok(heads)) if heads == current => {}
            (Some(_), Ok(_)) => not_current.push(reviewer.clone()),
            _ => unknown = true,
        }
    }
    let currentness = Currentness {
        state: if evidence.is_empty() {
            CurrentnessState::None
        } else if !not_current.is_empty() {
            CurrentnessState::Changed
        } else if unknown {
            CurrentnessState::Unknown
        } else {
            CurrentnessState::Matching
        },
        not_current,
    };

    // Path to approval.
    let responded = input.statuses.iter().any(|status| {
        status.definition_version() >= input.boundary
            && ((status.actor_type() == "agent" && input.agent_configured)
                || eligible
                    .iter()
                    .any(|human| status.actor_user_id() == Some(human.user_id)))
    });
    let unapproved_humans: Vec<&HumanCandidate> = eligible
        .iter()
        .copied()
        .filter(|human| human_verdict(human) != Some("approved"))
        .collect();
    let all_of_needed = || {
        let mut approvers = Vec::new();
        if input.agent_configured && !agent_yes {
            approvers.push(agent_ref());
        }
        approvers.extend(unapproved_humans.iter().map(|human| human_ref(human)));
        ApprovalPath::Needed {
            approvers,
            require_all: true,
            all_humans_required: !input.humans.is_empty(),
        }
    };
    let path = match status {
        RequirementStatus::Approved => None,
        RequirementStatus::Rejected if !rejecting_humans.is_empty() => {
            Some(ApprovalPath::HumansRejected {
                humans: rejecting_humans.iter().map(|h| human_ref(h)).collect(),
                others_required: input.require_all && eligible.len() > rejecting_humans.len(),
            })
        }
        RequirementStatus::Rejected if input.require_all && !unapproved_humans.is_empty() => {
            Some(all_of_needed())
        }
        RequirementStatus::Rejected => Some(ApprovalPath::AgentRejected {
            agent_name: input.agent_name.clone(),
            human_override: override_allowed && !eligible.is_empty(),
        }),
        RequirementStatus::Pending if !responded => None,
        RequirementStatus::Pending if input.require_all => Some(all_of_needed()),
        RequirementStatus::Pending => {
            let mut approvers = Vec::new();
            if input.agent_configured {
                approvers.push(agent_ref());
            }
            approvers.extend(eligible.iter().map(|human| human_ref(human)));
            Some(ApprovalPath::Needed {
                approvers,
                require_all: false,
                all_humans_required: false,
            })
        }
    };

    // Request needs (g5.governance.approval.selection).
    let active = |agent: bool, user_id: Option<Uuid>| {
        input.requests.iter().any(|request| {
            request.active
                && request.actor_type_agent == agent
                && (agent || request.actor_user_id == user_id)
        })
    };
    let agent_active = input.agent_configured && active(true, None);
    let agent_requestable = input.agent_configured && !agent_active;
    let human_ever_requested = input
        .requests
        .iter()
        .any(|request| !request.actor_type_agent);
    let agent_ever_requested = input
        .requests
        .iter()
        .any(|request| request.actor_type_agent);
    let requestable: Vec<&HumanCandidate> = input
        .humans
        .iter()
        .filter(|human| human.eligible && human.requestable)
        .collect();
    // An actual human route: override applies and an eligible active member
    // may be (or already is) asked; a configuration flag alone is not one.
    let can_override = override_allowed && !requestable.is_empty();
    let human_active = |human: &HumanCandidate| active(false, Some(human.user_id));
    let any_human_active = requestable.iter().any(|human| human_active(human));
    let free_humans = || {
        requestable
            .iter()
            .filter(|human| !human_active(human))
            .map(|human| human.user_id)
            .collect::<Vec<_>>()
    };
    let any_active = agent_active || any_human_active;

    let all_of = |include_approved: bool| RecipientNeeds {
        agent: agent_requestable && (include_approved || !agent_yes),
        humans: requestable
            .iter()
            .filter(|human| {
                !human_active(human)
                    && (include_approved || human_verdict(human) != Some("approved"))
            })
            .map(|human| human.user_id)
            .collect(),
        random_human_from: Vec::new(),
    };
    let one_new = |prefer_agent: bool| {
        if any_active {
            RecipientNeeds::default()
        } else if input.agent_configured && prefer_agent {
            RecipientNeeds {
                agent: true,
                ..RecipientNeeds::default()
            }
        } else if !free_humans().is_empty() {
            RecipientNeeds {
                random_human_from: free_humans(),
                ..RecipientNeeds::default()
            }
        } else {
            RecipientNeeds {
                agent: agent_requestable,
                ..RecipientNeeds::default()
            }
        }
    };
    let outstanding = if status == RequirementStatus::Approved {
        RecipientNeeds::default()
    } else if input.require_all {
        all_of(false)
    } else {
        let mut needs = RecipientNeeds {
            humans: rejecting_humans
                .iter()
                .filter(|human| human.requestable && !human_active(human))
                .map(|human| human.user_id)
                .collect(),
            ..RecipientNeeds::default()
        };
        // A waiting human who may override already covers the rejection.
        if agent_rejected && !(can_override && any_human_active) {
            if can_override && !human_ever_requested {
                needs.random_human_from = free_humans();
            } else {
                needs.agent = agent_requestable;
            }
        } else if rejecting_humans.is_empty() {
            needs = one_new(!agent_ever_requested || requestable.is_empty());
        }
        needs
    };
    let repeat = if input.require_all {
        all_of(true)
    } else {
        one_new(true)
    };
    debug_assert!(status != RequirementStatus::Approved || outstanding.is_empty());

    let escalation = if !agent_rejected || !rejecting_humans.is_empty() {
        None
    } else if override_agent {
        Some(AgentEscalationState::OverriddenByHuman)
    } else if status == RequirementStatus::Rejected && can_override {
        Some(AgentEscalationState::NeedsHumanEscalation)
    } else {
        None
    };
    let agent_escalation = escalation.map(|state| AgentEscalation {
        state,
        agent_review_id: agent_decision.and_then(ApprovalDecisionLike::agent_review_id),
    });

    RequirementEvaluation {
        status,
        currentness,
        path,
        needs: RequestNeeds {
            outstanding,
            repeat,
            agent_requestable,
            agent_approved: agent_yes,
        },
        agent_escalation,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Decision {
        actor_type: &'static str,
        user: Option<Uuid>,
        outcome: &'static str,
        version: i32,
        heads: serde_json::Value,
        review: Option<Uuid>,
    }

    impl ApprovalDecisionLike for Decision {
        fn definition_version(&self) -> i32 {
            self.version
        }
        fn outcome(&self) -> &str {
            self.outcome
        }
        fn actor_type(&self) -> &str {
            self.actor_type
        }
        fn actor_user_id(&self) -> Option<Uuid> {
            self.user
        }
        fn reviewed_git_heads(&self) -> &serde_json::Value {
            &self.heads
        }
        fn created_at(&self) -> chrono::DateTime<chrono::Utc> {
            chrono::DateTime::UNIX_EPOCH
        }
        fn agent_review_id(&self) -> Option<Uuid> {
            self.review
        }
    }

    const REPO_A: &str = "00000000-0000-0000-0000-00000000000a";
    const REPO_B: &str = "00000000-0000-0000-0000-00000000000b";
    const WS: &str = "00000000-0000-0000-0000-0000000000ff";

    fn heads(a: char, b: char) -> serde_json::Value {
        serde_json::json!([
            {"repoId": REPO_A, "branchName": "main", "workspaceId": WS, "headCommit": a.to_string().repeat(40)},
            {"repoId": REPO_B, "branchName": "main", "workspaceId": WS, "headCommit": b.to_string().repeat(40)},
        ])
    }

    fn current(a: char, b: char) -> Vec<MergeSourceHead> {
        normalized_head_vector(&heads(a, b)).unwrap()
    }

    fn agent(outcome: &'static str, head: (char, char)) -> Decision {
        Decision {
            actor_type: "agent",
            user: None,
            outcome,
            version: 1,
            heads: heads(head.0, head.1),
            // A distinct review per outcome, so tests can tell which decision counted.
            review: Some(Uuid::from_u128(match outcome {
                "approved" => 1,
                "rejected" => 2,
                _ => 3,
            })),
        }
    }

    fn human(user: Uuid, outcome: &'static str, head: (char, char)) -> Decision {
        Decision {
            actor_type: "human",
            user: Some(user),
            outcome,
            version: 1,
            heads: heads(head.0, head.1),
            review: None,
        }
    }

    fn candidate(user: Uuid, name: &str) -> HumanCandidate {
        HumanCandidate {
            user_id: user,
            display_name: name.into(),
            eligible: true,
            requestable: true,
        }
    }

    fn request(user: Option<Uuid>, active: bool) -> RequestFact {
        RequestFact {
            actor_type_agent: user.is_none(),
            actor_user_id: user,
            active,
        }
    }

    struct Case {
        require_all: bool,
        human_override: bool,
        agent: bool,
        humans: Vec<HumanCandidate>,
        decisions: Vec<Decision>,
        requests: Vec<RequestFact>,
        boundary: i32,
        heads: Vec<MergeSourceHead>,
        policy: SourceCommitVectorPolicy,
    }

    impl Case {
        fn any(agent: bool, humans: Vec<HumanCandidate>) -> Self {
            Self {
                require_all: false,
                human_override: true,
                agent,
                humans,
                decisions: Vec::new(),
                requests: Vec::new(),
                boundary: 1,
                heads: current('1', '2'),
                policy: SourceCommitVectorPolicy::Ignore,
            }
        }

        fn all(agent: bool, humans: Vec<HumanCandidate>) -> Self {
            Self {
                require_all: true,
                ..Self::any(agent, humans)
            }
        }

        fn eval(&self) -> RequirementEvaluation {
            evaluate(&RequirementInput {
                require_all: self.require_all,
                human_override: self.human_override,
                agent_configured: self.agent,
                agent_name: "UI agent".into(),
                humans: self.humans.clone(),
                statuses: self.decisions.iter().collect(),
                requests: self.requests.clone(),
                boundary: self.boundary,
                current_heads: Some(&self.heads),
                policy: self.policy,
            })
        }
    }

    #[test]
    fn human_approval_supersedes_agent_rejection_in_any_of() {
        let kai = Uuid::new_v4();
        let mut case = Case::any(true, vec![candidate(kai, "Kai")]);
        case.decisions = vec![
            human(kai, "approved", ('1', '2')),
            agent("rejected", ('1', '2')),
        ];
        let eval = case.eval();
        assert_eq!(eval.status, RequirementStatus::Approved);
        assert_eq!(eval.path, None);
        assert!(eval.needs.outstanding.is_empty());

        case.human_override = false;
        let eval = case.eval();
        assert_eq!(eval.status, RequirementStatus::Rejected);
        assert_eq!(
            eval.path,
            Some(ApprovalPath::AgentRejected {
                agent_name: "UI agent".into(),
                human_override: false
            })
        );
        assert!(eval.needs.outstanding.agent);
    }

    #[test]
    fn agent_rejection_with_override_requests_a_random_human_once() {
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        let mut case = Case::any(true, vec![candidate(a, "A"), candidate(b, "B")]);
        case.decisions = vec![agent("rejected", ('1', '2'))];
        case.requests = vec![request(None, false)];
        let eval = case.eval();
        assert_eq!(eval.status, RequirementStatus::Rejected);
        assert_eq!(
            eval.path,
            Some(ApprovalPath::AgentRejected {
                agent_name: "UI agent".into(),
                human_override: true
            })
        );
        assert_eq!(eval.needs.outstanding.random_human_from, vec![a, b]);
        assert!(!eval.needs.outstanding.agent);

        // Once a human was requested (and later cancelled), ask the agent again.
        case.requests.push(request(Some(a), false));
        let eval = case.eval();
        assert!(eval.needs.outstanding.agent);
        assert!(eval.needs.outstanding.random_human_from.is_empty());

        // A waiting human request covers it.
        case.requests.push(request(Some(b), true));
        case.requests.retain(|r| r.actor_user_id != Some(a));
        let eval = case.eval();
        assert!(eval.needs.outstanding.is_empty());
    }

    #[test]
    fn human_veto_names_rejecters_and_rerequests_them() {
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        let mut case = Case::any(true, vec![candidate(a, "A"), candidate(b, "B")]);
        case.decisions = vec![
            human(a, "rejected", ('1', '2')),
            agent("approved", ('1', '2')),
        ];
        let eval = case.eval();
        assert_eq!(eval.status, RequirementStatus::Rejected);
        assert!(matches!(
            &eval.path,
            Some(ApprovalPath::HumansRejected { humans, others_required: false })
                if humans.len() == 1 && humans[0].user_id == Some(a)
        ));
        assert_eq!(eval.needs.outstanding.humans, vec![a]);
        // Do not duplicate a waiting request to the rejecting human.
        case.requests = vec![request(Some(a), true)];
        assert!(case.eval().needs.outstanding.is_empty());

        let mut all = Case::all(false, vec![candidate(a, "A"), candidate(b, "B")]);
        all.decisions = vec![human(a, "rejected", ('1', '2'))];
        assert!(matches!(
            all.eval().path,
            Some(ApprovalPath::HumansRejected {
                others_required: true,
                ..
            })
        ));
    }

    #[test]
    fn any_of_requests_the_agent_first_then_a_random_human() {
        let a = Uuid::new_v4();
        let case = Case::any(true, vec![candidate(a, "A")]);
        let eval = case.eval();
        assert_eq!(eval.status, RequirementStatus::Pending);
        assert_eq!(eval.path, None, "untouched requirement has no path");
        assert!(eval.needs.outstanding.agent);

        let humans_only = Case::any(false, vec![candidate(a, "A")]);
        assert_eq!(
            humans_only.eval().needs.outstanding.random_human_from,
            vec![a]
        );

        let mut waiting = Case::any(true, vec![candidate(a, "A")]);
        waiting.requests = vec![request(None, true)];
        assert!(waiting.eval().needs.outstanding.is_empty());
    }

    #[test]
    fn all_of_partial_lists_every_needed_approver() {
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        let mut case = Case::all(true, vec![candidate(a, "A"), candidate(b, "B")]);
        case.decisions = vec![human(a, "approved", ('1', '2'))];
        case.requests = vec![request(Some(b), true)];
        let eval = case.eval();
        assert_eq!(eval.status, RequirementStatus::Pending);
        match eval.path {
            Some(ApprovalPath::Needed {
                approvers,
                require_all: true,
                all_humans_required: true,
            }) => {
                let names: Vec<_> = approvers.iter().map(|r| r.display_name.as_str()).collect();
                assert_eq!(names, ["UI agent", "B"]);
            }
            other => panic!("unexpected path {other:?}"),
        }
        assert!(eval.needs.outstanding.agent);
        assert!(eval.needs.outstanding.humans.is_empty(), "B is waiting");
        assert_eq!(eval.needs.repeat.humans, vec![a]);

        case.decisions.push(human(b, "approved", ('1', '2')));
        case.decisions.push(agent("approved", ('1', '2')));
        assert_eq!(case.eval().status, RequirementStatus::Approved);
    }

    #[test]
    fn withdrawal_and_invalidation_return_to_pending() {
        let a = Uuid::new_v4();
        let mut case = Case::any(false, vec![candidate(a, "A")]);
        case.decisions = vec![
            human(a, "deferred", ('1', '2')),
            human(a, "approved", ('1', '2')),
        ];
        let eval = case.eval();
        assert_eq!(eval.status, RequirementStatus::Pending);
        assert!(eval.path.is_some(), "a withdrawn response is a response");

        let mut old = Case::any(false, vec![candidate(a, "A")]);
        old.decisions = vec![human(a, "approved", ('1', '2'))];
        old.boundary = 2;
        assert_eq!(old.eval().status, RequirementStatus::Pending);
    }

    #[test]
    fn source_policy_decides_whether_moved_evidence_counts() {
        let a = Uuid::new_v4();
        let mut case = Case::any(true, vec![candidate(a, "A")]);
        // Only repository B moved since both reviews.
        case.decisions = vec![
            human(a, "approved", ('1', '1')),
            agent("approved", ('1', '2')),
        ];
        case.heads = current('1', '3');
        let eval = case.eval();
        assert_eq!(eval.status, RequirementStatus::Approved);
        assert_eq!(eval.currentness.state, CurrentnessState::Changed);
        assert_eq!(eval.currentness.not_current.len(), 2);

        case.policy = SourceCommitVectorPolicy::MakePending;
        assert_eq!(case.eval().status, RequirementStatus::Pending);

        case.heads = current('1', '2');
        let eval = case.eval();
        assert_eq!(eval.status, RequirementStatus::Approved, "agent matches");
        assert_eq!(eval.currentness.state, CurrentnessState::Matching);
    }

    #[test]
    fn unreadable_evidence_is_unknown_and_never_changes_status() {
        let a = Uuid::new_v4();
        let mut case = Case::any(false, vec![candidate(a, "A")]);
        let mut decision = human(a, "approved", ('1', '2'));
        decision.heads = serde_json::json!([]);
        case.decisions = vec![decision];
        let eval = case.eval();
        assert_eq!(eval.status, RequirementStatus::Approved);
        assert_eq!(eval.currentness.state, CurrentnessState::Unknown);
        assert_eq!(
            Case::any(false, vec![]).eval().currentness.state,
            CurrentnessState::None
        );
    }

    #[test]
    fn disallowed_owner_and_inactive_members_are_never_chosen() {
        let (owner, gone, ok) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        let mut humans = vec![
            candidate(owner, "Owner"),
            candidate(gone, "Gone"),
            candidate(ok, "Ok"),
        ];
        humans[0].eligible = false;
        humans[0].requestable = false;
        humans[1].requestable = false;
        let case = Case::any(false, humans);
        assert_eq!(case.eval().needs.outstanding.random_human_from, vec![ok]);
    }

    /// Issue #1095's escalation table: the display follows counted evidence
    /// and an actual human route, never the override flag alone.
    #[test]
    fn agent_escalation_follows_counted_evidence_and_an_actual_human_route() {
        let (maya, sam) = (Uuid::new_v4(), Uuid::new_v4());
        let escalation = |case: &Case| case.eval().agent_escalation.map(|e| e.state);
        let rejected_agent = Some(Uuid::from_u128(2));

        // An effective agent rejection with a requestable human: escalate,
        // naming the counted agent decision, not the older approval it
        // superseded; the status stays Rejected.
        let mut case = Case::any(true, vec![candidate(maya, "Maya")]);
        case.decisions = vec![agent("rejected", ('1', '2')), agent("approved", ('1', '2'))];
        let eval = case.eval();
        assert_eq!(eval.status, RequirementStatus::Rejected);
        assert_eq!(
            eval.agent_escalation,
            Some(AgentEscalation {
                state: AgentEscalationState::NeedsHumanEscalation,
                agent_review_id: rejected_agent,
            })
        );

        // A human request already Waiting does not settle it.
        case.requests = vec![request(Some(maya), true)];
        assert_eq!(
            escalation(&case),
            Some(AgentEscalationState::NeedsHumanEscalation)
        );

        // A withdrawn human response carries no verdict.
        case.decisions = vec![
            human(maya, "deferred", ('1', '2')),
            agent("rejected", ('1', '2')),
        ];
        assert_eq!(
            escalation(&case),
            Some(AgentEscalationState::NeedsHumanEscalation)
        );

        // An eligible human approval overrides: Approved, agent overridden.
        case.decisions = vec![
            human(maya, "approved", ('1', '2')),
            agent("rejected", ('1', '2')),
        ];
        assert_eq!(case.eval().status, RequirementStatus::Approved);
        assert_eq!(
            escalation(&case),
            Some(AgentEscalationState::OverriddenByHuman)
        );

        // A counted human rejection keeps both Rejected, even beside an approval.
        let mut mixed = Case::any(true, vec![candidate(maya, "Maya"), candidate(sam, "Sam")]);
        mixed.decisions = vec![
            human(sam, "rejected", ('1', '2')),
            human(maya, "approved", ('1', '2')),
            agent("rejected", ('1', '2')),
        ];
        assert_eq!(mixed.eval().status, RequirementStatus::Rejected);
        assert_eq!(escalation(&mixed), None);

        // A later effective agent approval resolves it without escalation.
        case.decisions = vec![agent("approved", ('1', '2')), agent("rejected", ('1', '2'))];
        assert_eq!(case.eval().status, RequirementStatus::Approved);
        assert_eq!(escalation(&case), None);

        // No requestable eligible human: the flag alone promises nothing.
        let mut departed = candidate(maya, "Maya");
        departed.requestable = false;
        let mut nohuman = Case::any(true, vec![departed]);
        nohuman.decisions = vec![agent("rejected", ('1', '2'))];
        assert_eq!(nohuman.eval().status, RequirementStatus::Rejected);
        assert_eq!(escalation(&nohuman), None);
        let mut owner = candidate(maya, "Maya");
        owner.eligible = false;
        nohuman.humans = vec![owner];
        assert_eq!(escalation(&nohuman), None);
        nohuman.humans = Vec::new();
        assert_eq!(escalation(&nohuman), None);

        // Override disabled, or All-of: plain Rejected.
        let mut off = Case::any(true, vec![candidate(maya, "Maya")]);
        off.human_override = false;
        off.decisions = vec![
            human(maya, "approved", ('1', '2')),
            agent("rejected", ('1', '2')),
        ];
        assert_eq!(off.eval().status, RequirementStatus::Rejected);
        assert_eq!(escalation(&off), None);
        let mut all = Case::all(true, vec![candidate(maya, "Maya")]);
        all.decisions = off.decisions.drain(..).collect();
        assert_eq!(all.eval().status, RequirementStatus::Rejected);
        assert_eq!(escalation(&all), None);

        // No counted agent verdict (failed or cancelled attempts record none).
        let mut untouched = Case::any(true, vec![candidate(maya, "Maya")]);
        untouched.requests = vec![request(None, true)];
        assert_eq!(escalation(&untouched), None);

        // An invalidated agent rejection does not count.
        let mut stale = Case::any(true, vec![candidate(maya, "Maya")]);
        stale.decisions = vec![agent("rejected", ('1', '2'))];
        stale.boundary = 2;
        assert_eq!(escalation(&stale), None);
    }
}
