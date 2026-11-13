//! Branch integration state machine — the pure function at the heart
//! of the remote-repo-tracking integration pipeline.
//!
//! Given three git SHAs (workspace tip, mirror tip, baseline), the
//! current `dirty_files` count, and the cycle's trigger family, this
//! module decides what action to take and what audit-row payload to
//! write. It performs NO I/O — no DB, no git, no network. The caller
//! (the `branch_integration_worker`) is responsible for the I/O and for
//! holding the per-`(repo_id, remote_branch)` advisory lock around the
//! call site.
//!
//! See ontology: `g5/g5-api/services/branch_integration_service`.
//! See arch view: [`remote_repo_tracking.arch_view.md`](../../../../views/arch_views/remote_repo_tracking.arch_view.md)
//! D10–D11, D49, D54–D56.

use serde::{Deserialize, Serialize};

use crate::models::branch_sync_run::{status as run_status, trigger_kind as run_trigger};

// ---------------------------------------------------------------------------
// Inputs
// ---------------------------------------------------------------------------

/// Inputs captured for a provider integration cycle.
///
/// Built by the worker after acquiring the per-branch advisory lock and
/// reading fresh state from the DB + the local git mirror. The struct
/// is opaque to the worker (it just fills the fields); the state
/// machine reads it.
#[derive(Debug, Clone)]
pub struct ClassifyInput {
    /// Local working ref captured from Git under the integration fence.
    pub workspace_tip: String,
    /// Remote tip — the SHA on `refs/remotes/origin/<remote_branch>` after
    /// a successful fetch. `None` if the fetch step failed.
    pub mirror_tip: Option<String>,
    /// Git synchronization watermark. The all-zero sentinel means no accepted
    /// common history and always requires explicit reconciliation.
    pub baseline: String,
    /// Observed dirty-file count for diagnostics; never gates ref delivery.
    pub dirty_files: i32,
    /// What triggered this cycle; together with policy, controls publication.
    pub trigger_kind: TriggerKind,
    /// Per-workspace push mode (`workspaces.push_mode`); with `trigger_kind`
    /// it determines whether a "workspace ahead" classification pushes
    /// (`should_push`).
    pub push_mode: PushMode,
    /// Automatic cycles may observe incoming commits without accepting them.
    pub auto_pull_enabled: bool,
    /// False for Base, which never publishes to its provider on any trigger
    /// (`g5.workspace.remote.base-parity`). Local history Base holds beyond the
    /// provider pauses for the owner to accept the provider history instead.
    pub publication_allowed: bool,
    /// Result of the ancestry checks the worker computes via git. The
    /// state machine consumes the typed enum so the matrix logic stays
    /// in this pure module.
    pub tip_relation: TipRelation,
}

/// Trigger family for the current cycle. Mirrors the
/// `branch_integration_queue.trigger_kind` CHECK enum (per
/// `g5/g5-api/models/branch_integration_queue` D3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerKind {
    /// Background scheduler tick. Publication follows the workspace policy.
    BackgroundInbound,
    /// Workspace-side commit (worker commit, draft commit, merge completion).
    Commit,
    /// Operator-invoked `POST /workspaces/{id}/sync`.
    Manual,
    /// Workspace-open hook one-shot sync.
    WorkspaceOpen,
    /// Explicit PushToRemote action — always publishes (arch D49/D67).
    Push,
    /// Approved reset-below-baseline publication handled by the worker before
    /// this pure classifier. It is parsed here so audit strings stay typed.
    ApprovedRebasePublish,
}

impl TriggerKind {
    /// Convert to the string constant used by `branch_sync_runs.trigger_kind`.
    #[must_use]
    pub fn as_audit_str(self) -> &'static str {
        match self {
            TriggerKind::BackgroundInbound => run_trigger::BACKGROUND_INBOUND,
            TriggerKind::Commit => run_trigger::COMMIT,
            TriggerKind::Manual => run_trigger::MANUAL,
            TriggerKind::WorkspaceOpen => run_trigger::WORKSPACE_OPEN,
            TriggerKind::Push => run_trigger::PUSH,
            TriggerKind::ApprovedRebasePublish => run_trigger::APPROVED_REBASE_PUBLISH,
        }
    }
}

/// Per-workspace outbound-publication mode (`workspaces.push_mode`, arch D66).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PushMode {
    /// Opt-in manual policy: only the explicit `'push'`
    /// trigger (PushToRemote) publishes a workspace-ahead branch.
    Manual,
    /// Commit-coupled advances AND the background loop publish automatically.
    Auto,
}

impl PushMode {
    /// Parse the `workspaces.push_mode` column; unknown/NULL → `Manual`.
    #[must_use]
    pub fn from_db(s: &str) -> Self {
        match s {
            "auto" => PushMode::Auto,
            _ => PushMode::Manual,
        }
    }

    #[must_use]
    pub fn as_db_str(self) -> &'static str {
        match self {
            PushMode::Manual => "manual",
            PushMode::Auto => "auto",
        }
    }
}

/// Whether an outbound push is permitted for a "workspace ahead" classification
/// (arch D49 / design §6.2): the explicit `'push'` trigger always publishes;
/// every other trigger publishes only in `auto` mode.
#[must_use]
pub fn should_push(trigger_kind: TriggerKind, push_mode: PushMode) -> bool {
    !matches!(trigger_kind, TriggerKind::ApprovedRebasePublish)
        && (matches!(trigger_kind, TriggerKind::Push) || matches!(push_mode, PushMode::Auto))
}

/// Ancestry relation between `workspace_tip` and `mirror_tip` (per
/// arch D11). The worker computes this via `git2::Repository::graph_descendant_of`
/// against the local mirror and passes it in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipRelation {
    /// Tips are byte-equal.
    Equal,
    /// `workspace_tip` is a strict descendant of `mirror_tip`. Workspace
    /// is ahead; outbound push is the natural action.
    WorkspaceAhead,
    /// `mirror_tip` is a strict descendant of `workspace_tip`. Remote is
    /// ahead; the G5 branch can fast-forward without changing the VM checkout.
    MirrorAhead,
    /// Neither tip is a descendant of the other; share a common ancestor.
    Diverged,
    /// The G5 branch no longer contains `baseline` while the provider still
    /// equals it: an accepted workspace rewrite, published by push policy
    /// with the provider tip as its lease (`g5.governance.remote.workspace-rewrite`).
    WorkspaceRewritten,
    /// The provider tip is not a descendant of `baseline` — a provider
    /// history rewrite, which requires explicit approval.
    WatermarkViolation,
}

// ---------------------------------------------------------------------------
// Outputs
// ---------------------------------------------------------------------------

/// The action the worker MUST execute after the state machine returns.
///
/// Provider refresh never edits the workspace; publication follows policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// No git mutation. Worker MAY advance `baseline` if it lags.
    Noop,
    /// Push `workspace_tip` to the customer remote via refspec
    /// `<local>:<remote>`, leased on the fetched provider tip. Only emitted
    /// when the trigger family allows push under the workspace policy. For
    /// `WorkspaceRewritten` the leased push replaces the unchanged provider tip.
    Pushed,
    /// Refresh provider refs only. Reconciliation belongs to the VM.
    Fetched,
    /// Fast-forward the G5 branch and synchronization watermark atomically.
    /// The VM checkout and saved snapshots remain unchanged.
    Imported,
    /// Open a `workspace_merge_runs` row with `source_kind = 'remote_tracking'`
    /// and set `sync_paused_reason`. The `phase` field discriminates
    /// regular divergence vs. watermark violation.
    Diverged { phase: DivergedPhase },
    /// Pause and audit an automatic auto-push divergence without opening a
    /// reconciliation workflow. Explicit push/manual intent may later resume
    /// this pause-only state through the normal diverged path.
    DivergedPauseOnly,
}

/// `phase` discriminator on the merge run for diverged outcomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DivergedPhase {
    /// Tips share a common ancestor; standard rebase-based merge run.
    WorkspaceRebasePreflight,
    /// The provider tip is not a descendant of `baseline` — a provider
    /// history rewrite awaiting explicit approval.
    ResetBelowBaseline,
}

/// The decision payload returned by `classify_and_act`. The worker uses
/// this to (a) execute the action via git/DB and (b) write a
/// `branch_sync_runs` audit row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    /// What to do.
    pub action: Action,
    /// `branch_sync_runs.status` string (one of `models::branch_sync_run::status::*`).
    pub audit_status: &'static str,
    /// `branch_sync_runs.error_detail` string when relevant.
    pub audit_error_detail: Option<&'static str>,
    /// Whether the provider observation should refresh UI status.
    pub refresh_advisory: bool,
}

// ---------------------------------------------------------------------------
// The state machine
// ---------------------------------------------------------------------------

/// Pure decision function for provider observations and outbound publication.
/// Fetching never changes workspace files, regardless of dirty state. Outbound
/// publication follows push policy; divergence requires explicit reconciliation.
#[must_use]
pub fn classify_and_act(input: &ClassifyInput) -> Decision {
    use TipRelation::*;
    use TriggerKind::*;

    // A provider rewrite maps to Diverged regardless of trigger family.
    // Arch D11 watermark row.
    if matches!(input.tip_relation, WatermarkViolation) {
        return Decision {
            action: Action::Diverged {
                phase: DivergedPhase::ResetBelowBaseline,
            },
            audit_status: run_status::DIVERGED,
            audit_error_detail: None,
            refresh_advisory: false,
        };
    }

    match input.tip_relation {
        Equal => {
            // Tips equal: advance baseline if it lags. No git mutation.
            // Action::Noop covers both trigger families and both clean/dirty.
            //
            // C31: request an advisory refresh. The tips are equal, so the
            // observed mirror tip == head; refreshing subsumes the advisory
            // under head on clean branches (design §6.4 / arch D3 case (c)) and
            // reconciles any residual stale advisory (e.g. one frozen at a
            // pre-import value) so the client `isBehind` (advisory != head)
            // cannot stay falsely lit after convergence. The worker's
            // `refresh_advisory` UPDATE is `IS DISTINCT FROM`-guarded, so an
            // already-converged branch is not rewritten every sweep.
            Decision {
                action: Action::Noop,
                audit_status: run_status::NOOP,
                audit_error_detail: None,
                refresh_advisory: true,
            }
        }
        // Base never publishes: history it holds beyond the provider pauses
        // without a resolution run, for the owner to accept provider history.
        WorkspaceAhead | WorkspaceRewritten | Diverged if !input.publication_allowed => Decision {
            action: Action::DivergedPauseOnly,
            audit_status: run_status::DIVERGED,
            audit_error_detail: None,
            refresh_advisory: false,
        },
        // An accepted workspace rewrite against the unchanged provider follows
        // the same policy as a fast-forward; manual mode leaves it pending
        // without moving the watermark or importing the provider back over it.
        WorkspaceAhead | WorkspaceRewritten => {
            if should_push(input.trigger_kind, input.push_mode) {
                // Push: explicit 'push' trigger (any mode), or auto mode.
                // Per arch D11: dirty workspace does NOT block push
                // because the push mutates only the remote ref.
                Decision {
                    action: Action::Pushed,
                    audit_status: run_status::PUSHED,
                    audit_error_detail: None,
                    refresh_advisory: false,
                }
            } else {
                // Manual policy waits for an explicit outbound push.
                Decision {
                    action: Action::Noop,
                    audit_status: run_status::NOOP,
                    audit_error_detail: None,
                    refresh_advisory: false,
                }
            }
        }
        MirrorAhead if input.auto_pull_enabled || matches!(input.trigger_kind, Manual | Push) => {
            Decision {
                action: Action::Imported,
                audit_status: run_status::IMPORTED,
                audit_error_detail: None,
                refresh_advisory: true,
            }
        }
        MirrorAhead => Decision {
            action: Action::Fetched,
            audit_status: run_status::FETCHED,
            audit_error_detail: None,
            refresh_advisory: true,
        },
        Diverged => {
            let automatic_auto_push = input.push_mode == PushMode::Auto
                && matches!(
                    input.trigger_kind,
                    BackgroundInbound | Commit | WorkspaceOpen
                );
            if automatic_auto_push {
                return Decision {
                    action: Action::DivergedPauseOnly,
                    audit_status: run_status::DIVERGED,
                    audit_error_detail: None,
                    refresh_advisory: false,
                };
            }
            // Explicit reconciliation opens an operation; it never changes files.
            Decision {
                action: Action::Diverged {
                    phase: DivergedPhase::WorkspaceRebasePreflight,
                },
                audit_status: run_status::DIVERGED,
                audit_error_detail: None,
                refresh_advisory: false,
            }
        }
        WatermarkViolation => unreachable!("handled above"),
    }
}

// ---------------------------------------------------------------------------
// Unit tests — every cell of the arch D11 matrices
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn input_mode(
        trigger: TriggerKind,
        push_mode: PushMode,
        dirty: i32,
        relation: TipRelation,
    ) -> ClassifyInput {
        ClassifyInput {
            workspace_tip: "aa".repeat(20),
            mirror_tip: Some("bb".repeat(20)),
            baseline: "cc".repeat(20),
            dirty_files: dirty,
            trigger_kind: trigger,
            push_mode,
            auto_pull_enabled: true,
            publication_allowed: true,
            tip_relation: relation,
        }
    }

    // Existing relation tests default to auto mode, so a commit/manual/
    // workspace_open trigger pushes when workspace-ahead (as before push_mode
    // existed). Manual-mode and 'push'-trigger behavior is covered explicitly.
    fn input(trigger: TriggerKind, dirty: i32, relation: TipRelation) -> ClassifyInput {
        input_mode(trigger, PushMode::Auto, dirty, relation)
    }

    // -- Clean workspace, commit-coupled (arch D11 clean, commit column) --

    #[test]
    fn clean_commit_equal_is_noop() {
        let d = classify_and_act(&input(TriggerKind::Commit, 0, TipRelation::Equal));
        assert_eq!(d.action, Action::Noop);
        assert_eq!(d.audit_status, "noop");
        // C31: a clean Equal cycle refreshes the advisory (tips equal ⇒
        // advisory := head, subsuming the advisory under head on clean
        // branches per design §6.4 / arch D3 case (c)). The worker's
        // `refresh_advisory` is `IS DISTINCT FROM`-guarded so an
        // already-converged branch is not rewritten every sweep.
        assert!(d.refresh_advisory);
    }

    #[test]
    fn clean_commit_workspace_ahead_is_pushed() {
        let d = classify_and_act(&input(TriggerKind::Commit, 0, TipRelation::WorkspaceAhead));
        assert_eq!(d.action, Action::Pushed);
        assert_eq!(d.audit_status, "pushed");
    }

    #[test]
    fn clean_commit_mirror_ahead_fast_forwards_g5() {
        let d = classify_and_act(&input(TriggerKind::Commit, 0, TipRelation::MirrorAhead));
        assert_eq!(d.action, Action::Imported);
        assert_eq!(d.audit_status, "imported");
    }

    #[test]
    fn clean_commit_diverged_auto_requires_pause_only_red() {
        let d = classify_and_act(&input(TriggerKind::Commit, 0, TipRelation::Diverged));
        assert_eq!(d.action, Action::DivergedPauseOnly);
        assert_eq!(d.audit_status, "diverged");
        assert!(!d.refresh_advisory);
    }

    #[test]
    fn clean_commit_watermark_violation_opens_reset_phase() {
        let d = classify_and_act(&input(
            TriggerKind::Commit,
            0,
            TipRelation::WatermarkViolation,
        ));
        assert_eq!(
            d.action,
            Action::Diverged {
                phase: DivergedPhase::ResetBelowBaseline,
            }
        );
    }

    // -- Clean workspace, background_inbound (D11 clean, background column) --

    #[test]
    fn manual_mode_background_workspace_ahead_is_noop() {
        // push_mode=manual: the background loop never publishes (should_push=false).
        let d = classify_and_act(&input_mode(
            TriggerKind::BackgroundInbound,
            PushMode::Manual,
            0,
            TipRelation::WorkspaceAhead,
        ));
        assert_eq!(
            d.action,
            Action::Noop,
            "manual mode: background MUST NOT push (arch D49)"
        );
        assert_eq!(d.audit_status, "noop");
    }

    #[test]
    fn auto_mode_background_workspace_ahead_is_pushed() {
        // push_mode=auto: the background loop publishes workspace-ahead (test_view D84).
        let d = classify_and_act(&input_mode(
            TriggerKind::BackgroundInbound,
            PushMode::Auto,
            0,
            TipRelation::WorkspaceAhead,
        ));
        assert_eq!(
            d.action,
            Action::Pushed,
            "auto mode: background publishes workspace-ahead"
        );
        assert_eq!(d.audit_status, "pushed");
    }

    #[test]
    fn clean_background_mirror_ahead_fast_forwards_g5() {
        let d = classify_and_act(&input(
            TriggerKind::BackgroundInbound,
            0,
            TipRelation::MirrorAhead,
        ));
        assert_eq!(d.action, Action::Imported);
    }

    #[test]
    fn clean_background_diverged_auto_requires_pause_only_red() {
        let d = classify_and_act(&input(
            TriggerKind::BackgroundInbound,
            0,
            TipRelation::Diverged,
        ));
        assert_eq!(d.action, Action::DivergedPauseOnly);
        assert_eq!(d.audit_status, "diverged");
    }

    #[test]
    fn clean_workspace_open_diverged_auto_requires_pause_only_red() {
        let d = classify_and_act(&input(TriggerKind::WorkspaceOpen, 0, TipRelation::Diverged));
        assert_eq!(d.action, Action::DivergedPauseOnly);
        assert_eq!(d.audit_status, "diverged");
    }

    // -- Dirty workspace, commit-coupled (D11 dirty, commit column) --

    #[test]
    fn dirty_commit_equal_is_noop() {
        let d = classify_and_act(&input(TriggerKind::Commit, 3, TipRelation::Equal));
        assert_eq!(d.action, Action::Noop);
    }

    #[test]
    fn dirty_commit_workspace_ahead_pushes_despite_dirty() {
        // The commit-on-dirty race resolution per C6 / arch D11 dirty
        // sub-table: push proceeds because it mutates only the remote
        // ref; the in-flight draft on top of the just-committed tip
        // is irrelevant to what is pushed.
        let d = classify_and_act(&input(TriggerKind::Commit, 5, TipRelation::WorkspaceAhead));
        assert_eq!(d.action, Action::Pushed, "commit-on-dirty MUST push");
    }

    #[test]
    fn dirty_commit_mirror_ahead_refreshes_without_touching_local_work() {
        let d = classify_and_act(&input(TriggerKind::Commit, 5, TipRelation::MirrorAhead));
        assert_eq!(d.action, Action::Imported);
        assert_eq!(d.audit_status, "imported");
        assert!(d.refresh_advisory);
    }

    #[test]
    fn dirty_commit_diverged_auto_requires_pause_only_red() {
        let d = classify_and_act(&input(TriggerKind::Commit, 5, TipRelation::Diverged));
        assert_eq!(d.action, Action::DivergedPauseOnly);
        assert_eq!(d.audit_status, "diverged");
    }

    #[test]
    fn dirty_commit_watermark_violation_opens_reset_phase() {
        let d = classify_and_act(&input(
            TriggerKind::Commit,
            5,
            TipRelation::WatermarkViolation,
        ));
        assert_eq!(
            d.action,
            Action::Diverged {
                phase: DivergedPhase::ResetBelowBaseline,
            }
        );
    }

    // -- Dirty workspace, background_inbound (D11 dirty, background column) --

    #[test]
    fn background_classification_is_independent_of_vm_dirty_files() {
        for relation in [
            TipRelation::Equal,
            TipRelation::WorkspaceAhead,
            TipRelation::MirrorAhead,
            TipRelation::Diverged,
            TipRelation::WorkspaceRewritten,
            TipRelation::WatermarkViolation,
        ] {
            let clean = classify_and_act(&input(TriggerKind::BackgroundInbound, 0, relation));
            let dirty = classify_and_act(&input(TriggerKind::BackgroundInbound, 5, relation));
            assert_eq!(
                clean, dirty,
                "provider refs never replace dirty workspace files"
            );
        }
    }

    // -- Trigger family invariants --

    #[test]
    fn manual_trigger_pushes_when_workspace_ahead() {
        let d = classify_and_act(&input(TriggerKind::Manual, 0, TipRelation::WorkspaceAhead));
        assert_eq!(d.action, Action::Pushed);
    }

    #[test]
    fn workspace_open_trigger_pushes_when_workspace_ahead() {
        let d = classify_and_act(&input(
            TriggerKind::WorkspaceOpen,
            0,
            TipRelation::WorkspaceAhead,
        ));
        assert_eq!(d.action, Action::Pushed);
    }

    #[test]
    fn base_never_publishes_for_any_trigger_or_mode() {
        for trigger in [
            TriggerKind::BackgroundInbound,
            TriggerKind::Commit,
            TriggerKind::Manual,
            TriggerKind::WorkspaceOpen,
            TriggerKind::Push,
        ] {
            for mode in [PushMode::Auto, PushMode::Manual] {
                for relation in [
                    TipRelation::WorkspaceAhead,
                    TipRelation::WorkspaceRewritten,
                    TipRelation::Diverged,
                ] {
                    let mut base = input_mode(trigger, mode, 0, relation);
                    base.publication_allowed = false;
                    let decision = classify_and_act(&base);
                    assert_eq!(
                        decision.action,
                        Action::DivergedPauseOnly,
                        "{trigger:?} {mode:?} {relation:?}"
                    );
                }
            }
        }
        // Base still imports provider fast-forwards like any workspace.
        let mut base = input_mode(
            TriggerKind::BackgroundInbound,
            PushMode::Manual,
            0,
            TipRelation::MirrorAhead,
        );
        base.publication_allowed = false;
        assert_eq!(classify_and_act(&base).action, Action::Imported);
    }

    #[test]
    fn should_push_matrix() {
        // Explicit 'push' trigger always publishes, regardless of mode.
        assert!(should_push(TriggerKind::Push, PushMode::Manual));
        assert!(should_push(TriggerKind::Push, PushMode::Auto));
        // auto mode: every trigger publishes.
        assert!(should_push(TriggerKind::Commit, PushMode::Auto));
        assert!(should_push(TriggerKind::BackgroundInbound, PushMode::Auto));
        assert!(!should_push(
            TriggerKind::ApprovedRebasePublish,
            PushMode::Auto
        ));
        // manual mode: only 'push' publishes.
        assert!(!should_push(TriggerKind::Commit, PushMode::Manual));
        assert!(!should_push(TriggerKind::Manual, PushMode::Manual));
        assert!(!should_push(TriggerKind::WorkspaceOpen, PushMode::Manual));
        assert!(!should_push(
            TriggerKind::BackgroundInbound,
            PushMode::Manual
        ));
        assert!(!should_push(
            TriggerKind::ApprovedRebasePublish,
            PushMode::Manual
        ));
    }

    #[test]
    fn manual_mode_commit_workspace_ahead_is_noop() {
        // Default mode: a commit does NOT auto-publish; PushToRemote required (D86).
        let d = classify_and_act(&input_mode(
            TriggerKind::Commit,
            PushMode::Manual,
            0,
            TipRelation::WorkspaceAhead,
        ));
        assert_eq!(d.action, Action::Noop, "manual mode: commit MUST NOT push");
        assert_eq!(d.audit_status, "noop");
    }

    #[test]
    fn push_trigger_publishes_in_manual_mode() {
        // PushToRemote ('push') always publishes a workspace-ahead branch (D85).
        let d = classify_and_act(&input_mode(
            TriggerKind::Push,
            PushMode::Manual,
            0,
            TipRelation::WorkspaceAhead,
        ));
        assert_eq!(
            d.action,
            Action::Pushed,
            "'push' trigger MUST publish even in manual mode"
        );
        assert_eq!(d.audit_status, "pushed");
    }

    #[test]
    fn push_trigger_diverged_opens_merge_run() {
        // A 'push' on a diverged branch opens a merge run, not a force-push (D85).
        let d = classify_and_act(&input_mode(
            TriggerKind::Push,
            PushMode::Manual,
            0,
            TipRelation::Diverged,
        ));
        assert!(matches!(d.action, Action::Diverged { .. }));
    }

    #[test]
    fn manual_mode_automatic_triggers_diverged_still_open_merge_run() {
        for trigger in [
            TriggerKind::BackgroundInbound,
            TriggerKind::Commit,
            TriggerKind::WorkspaceOpen,
        ] {
            let d = classify_and_act(&input_mode(
                trigger,
                PushMode::Manual,
                0,
                TipRelation::Diverged,
            ));
            assert!(
                matches!(d.action, Action::Diverged { .. }),
                "manual mode retains operator reconciliation for {trigger:?}"
            );
        }
    }

    #[test]
    fn explicit_triggers_diverged_still_open_merge_run_in_auto_mode() {
        for trigger in [TriggerKind::Push, TriggerKind::Manual] {
            let d = classify_and_act(&input_mode(
                trigger,
                PushMode::Auto,
                0,
                TipRelation::Diverged,
            ));
            assert!(
                matches!(d.action, Action::Diverged { .. }),
                "explicit {trigger:?} intent retains operator reconciliation in auto mode"
            );
        }
    }

    #[test]
    fn auto_background_watermark_violation_still_opens_reset_lane() {
        let d = classify_and_act(&input_mode(
            TriggerKind::BackgroundInbound,
            PushMode::Auto,
            0,
            TipRelation::WatermarkViolation,
        ));
        assert_eq!(
            d.action,
            Action::Diverged {
                phase: DivergedPhase::ResetBelowBaseline,
            }
        );
    }

    #[test]
    fn workspace_rewrite_follows_push_policy_without_approval() {
        for trigger in [
            TriggerKind::BackgroundInbound,
            TriggerKind::Commit,
            TriggerKind::WorkspaceOpen,
            TriggerKind::Manual,
            TriggerKind::Push,
        ] {
            let auto = classify_and_act(&input_mode(
                trigger,
                PushMode::Auto,
                0,
                TipRelation::WorkspaceRewritten,
            ));
            assert_eq!(auto.action, Action::Pushed, "auto {trigger:?}");
            let manual = classify_and_act(&input_mode(
                trigger,
                PushMode::Manual,
                0,
                TipRelation::WorkspaceRewritten,
            ));
            // Manual mode keeps the rewrite pending; only an explicit push
            // publishes it. Nothing is imported back over it.
            let expected = if trigger == TriggerKind::Push {
                Action::Pushed
            } else {
                Action::Noop
            };
            assert_eq!(manual.action, expected, "manual {trigger:?}");
        }
        let approved = classify_and_act(&input_mode(
            TriggerKind::ApprovedRebasePublish,
            PushMode::Auto,
            0,
            TipRelation::WorkspaceRewritten,
        ));
        assert_eq!(approved.action, Action::Noop);
    }

    #[test]
    fn approved_rebase_publish_trigger_does_not_classifier_push() {
        let d = classify_and_act(&input_mode(
            TriggerKind::ApprovedRebasePublish,
            PushMode::Auto,
            0,
            TipRelation::WorkspaceAhead,
        ));
        assert_eq!(d.action, Action::Noop);
        assert_eq!(d.audit_status, "noop");
    }

    #[test]
    fn trigger_kind_as_audit_str_matches_constants() {
        assert_eq!(
            TriggerKind::BackgroundInbound.as_audit_str(),
            "background_inbound"
        );
        assert_eq!(TriggerKind::Commit.as_audit_str(), "commit");
        assert_eq!(TriggerKind::Manual.as_audit_str(), "manual");
        assert_eq!(TriggerKind::WorkspaceOpen.as_audit_str(), "workspace_open");
        assert_eq!(TriggerKind::Push.as_audit_str(), "push");
        assert_eq!(
            TriggerKind::ApprovedRebasePublish.as_audit_str(),
            "approved_rebase_publish"
        );
    }

    // -- Boundary conditions --

    #[test]
    fn incoming_fast_forward_obeys_auto_pull_but_explicit_intent_can_accept_it() {
        for trigger in [
            TriggerKind::Manual,
            TriggerKind::Push,
            TriggerKind::Commit,
            TriggerKind::WorkspaceOpen,
            TriggerKind::BackgroundInbound,
        ] {
            for enabled in [false, true] {
                let mut incoming = input(trigger, 5, TipRelation::MirrorAhead);
                incoming.auto_pull_enabled = enabled;
                let decision = classify_and_act(&incoming);
                let imports = enabled || matches!(trigger, TriggerKind::Manual | TriggerKind::Push);
                assert_eq!(
                    decision.action,
                    if imports {
                        Action::Imported
                    } else {
                        Action::Fetched
                    }
                );
            }
        }
    }

    #[test]
    fn dirty_files_zero_is_treated_as_clean() {
        let d = classify_and_act(&input(TriggerKind::Commit, 0, TipRelation::MirrorAhead));
        assert_eq!(d.action, Action::Imported, "dirty_files=0 must be clean");
    }

    #[test]
    fn dirty_files_one_still_allows_g5_fast_forward() {
        let d = classify_and_act(&input(TriggerKind::Commit, 1, TipRelation::MirrorAhead));
        assert_eq!(d.action, Action::Imported);
    }
}
