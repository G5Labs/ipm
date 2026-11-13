//! A preview run as Activity and the preview dialog describe it (ui
//! `g5.ui.activity.preview-row`): the task's name at creation, and a pure
//! projection of its status, spec, outcome and two lifecycle facts into a
//! live stage or a structured ending. Owned by the Workspace context so the
//! activity projection and the Agents' task lifecycle share one derivation.

use serde::Serialize;
use serde_json::Value;

/// The name a preview task carries from creation (ui
/// `g5.ui.activity.preview-row`): the run's profile, so every task list tells
/// runs apart; the repository is deliberately not part of it.
pub fn preview_task_name(profile: Option<&str>) -> String {
    format!("Preview · {}", profile.unwrap_or("default profile"))
}

/// A closing preview (`g5.vm.preview-stop`): Close was requested on the task
/// and its app is being stopped and cleaned up while the task still runs.
pub fn preview_closing(status: &str, spec: &Value) -> bool {
    status == "running"
        && spec
            .pointer("/lifecycle/cancel_requested")
            .and_then(Value::as_bool)
            .unwrap_or(false)
}

/// How a cancelled preview closed, from its outcome (`clean`, `failed`,
/// `killed` from the guest; `lost` when the VM went away during the close);
/// absent for any other ending.
pub fn preview_close(status: &str, outcome: &Value) -> Option<String> {
    (status == "cancelled")
        .then(|| outcome["detail"]["close"].as_str().map(str::to_string))
        .flatten()
}

/// Stage reasons and endings of a preview run as Activity shows them (ui
/// `g5.ui.activity.preview-row`), derived from the task's status, spec,
/// outcome and two facts from its lifecycle events: whether the module
/// started and whether it reported ready. The same facts the preview dialog
/// reads; nothing here is persisted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewRunProjection {
    pub profile: Option<String>,
    pub port: Option<u16>,
    /// `queued`, `starting`, `building`, `serving` or `closing` while the task
    /// is live; absent once ended.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ending: Option<PreviewEnding>,
}

/// Why a preview run ended, structured so the client words it once for
/// Activity and the run log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewEnding {
    /// `ended` (the app exited on its own), `closed` (the owner's Close),
    /// `stopped` (a platform stop), `failed` or `did_not_start`.
    pub kind: &'static str,
    /// For `failed`: `app_exited`, `module`, `timed_out`, `limit_exceeded`,
    /// `interrupted` or `vm_lost`. For `stopped` without a retained reason:
    /// `unstarted` when the module never started.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cause: Option<&'static str>,
    /// The retained stop reason (`g5.preview.service.stop-reason`), when one
    /// was recorded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// How a close ended: `clean`, `failed` or `killed`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub close: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signal: Option<i64>,
    /// The module's or dispatcher's own words, when it gave any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Facts a preview run's lifecycle events hold: whether its module started
/// (`invocation.started` or any `module.*` event) and reported ready
/// (`module.preview.ready`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PreviewRunEvents {
    pub started: bool,
    pub ready: bool,
}

pub fn project_preview_run(
    status: &str,
    spec: &Value,
    outcome: Option<&Value>,
    events: PreviewRunEvents,
) -> PreviewRunProjection {
    let profile = spec["preview"]["profile"].as_str().map(str::to_string);
    let port = preview_spec_port(spec);
    let stage = match status {
        "queued" => Some("queued"),
        "running" | "waiting" => Some(if preview_closing(status, spec) {
            "closing"
        } else if events.ready {
            "serving"
        } else if events.started {
            "building"
        } else {
            "starting"
        }),
        _ => None,
    };
    let ending = match status {
        "succeeded" => Some(PreviewEnding {
            kind: "ended",
            cause: None,
            reason: None,
            close: None,
            exit_code: None,
            signal: None,
            message: None,
        }),
        "cancelled" | "failed" => {
            let outcome = outcome.cloned().unwrap_or(Value::Null);
            let reason = outcome["reason"].as_str().map(normalize_stop_reason);
            let close = preview_close(status, &outcome);
            // A queued or waiting preview cancelled by a VM or host loss
            // (`cancel_queued_workspace_previews`) is a loss, not a stop.
            let lost_before_start =
                status == "cancelled" && reason.as_deref() == Some("vm_lost") && close.is_none();
            Some(
                if status == "cancelled"
                    && matches!(reason.as_deref(), Some("vm_start_failed" | "start_ended"))
                {
                    // A queued preview whose VM never came up
                    // (`workspace_vm_release`), or one recorded with a Start
                    // that ended before the VM ran (`g5.preview.start-and-preview`,
                    // settled by the consent trigger of migration 152).
                    let message = if reason.as_deref() == Some("start_ended") {
                        "the workspace Start ended before the preview ran"
                    } else {
                        "the workspace VM did not start"
                    };
                    PreviewEnding {
                        kind: "did_not_start",
                        cause: None,
                        reason,
                        close: None,
                        exit_code: None,
                        signal: None,
                        message: Some(message.to_string()),
                    }
                } else if status == "cancelled" && !lost_before_start {
                    PreviewEnding {
                        kind: if reason.as_deref() == Some("owner_cancel") {
                            "closed"
                        } else {
                            "stopped"
                        },
                        cause: (reason.is_none() && !events.started).then_some("unstarted"),
                        reason,
                        close,
                        exit_code: None,
                        signal: None,
                        message: None,
                    }
                } else {
                    preview_failure(&outcome, reason)
                },
            )
        }
        _ => None,
    };
    PreviewRunProjection {
        profile,
        port,
        stage,
        ending,
    }
}

/// The owner-facing stop vocabulary (`preview_stop_reason`) for reasons other
/// writers record on a queued or waiting preview
/// (`cancel_queued_workspace_previews`, `settle_agents_after_release`).
fn normalize_stop_reason(reason: &str) -> String {
    match reason {
        "host_inventory_missing" | "host_force_terminated" => "vm_lost",
        "ownership_transferred" => "transfer",
        other => other,
    }
    .to_string()
}

/// A failed preview's ending. A VM loss is settled with a `reason` in the
/// stop vocabulary (`apply_vm_loss`); an admission failure with
/// `dispatch_admission_failed` and a detail text; the module's own outcome
/// carries its outer status and, for an app exit, the wrapper's result.
fn preview_failure(outcome: &Value, reason: Option<String>) -> PreviewEnding {
    let result = &outcome["result"];
    let detail = &outcome["detail"];
    let text = |value: &Value| value.as_str().filter(|s| !s.is_empty()).map(str::to_string);
    let mut ending = PreviewEnding {
        kind: "failed",
        cause: None,
        reason: reason.clone(),
        close: None,
        exit_code: None,
        signal: None,
        message: None,
    };
    match (reason.as_deref(), outcome["status"].as_str()) {
        (Some("dispatch_admission_failed"), _) => {
            ending.kind = "did_not_start";
            ending.message = text(detail)
                .or_else(|| text(&detail["reason"]))
                .map(|message| {
                    message
                        .strip_prefix("Conflict: ")
                        .map_or(message.clone(), str::to_string)
                });
        }
        // Only `apply_vm_loss` writes these on a failed preview: the loss
        // itself, or the stop it interrupted (`preview_stop_reason`).
        (
            Some(
                "vm_lost" | "idle_stop" | "owner_stop" | "workspace_stop" | "transfer"
                | "start_required" | "host_drain",
            ),
            _,
        ) => ending.cause = Some("vm_lost"),
        (None, Some("timed_out")) => ending.cause = Some("timed_out"),
        (None, Some("limit_exceeded")) => ending.cause = Some("limit_exceeded"),
        (None, Some("interrupted")) => ending.cause = Some("interrupted"),
        _ if result["status"] == "app_exited" => {
            ending.cause = Some("app_exited");
            ending.exit_code = result["app_exit_code"].as_i64();
            ending.signal = result["app_signal"].as_i64();
        }
        _ => {
            ending.cause = Some("module");
            ending.message = text(&result["message"])
                .or_else(|| text(&detail["reason"]))
                .or_else(|| text(detail))
                .or_else(|| text(&outcome["error"]))
                .or_else(|| reason.map(|value| value.replace('_', " ")));
        }
    }
    ending
}

/// The port a preview serves on: the one the guest reported ready
/// (`preview.ready_port`, `g5.preview.start-and-preview`), else the
/// configured one it was recorded with. Every surface reads it here so the
/// relay, the dialog and the Activity row agree.
pub(crate) fn preview_spec_port(spec: &Value) -> Option<u16> {
    [&spec["preview"]["ready_port"], &spec["preview"]["port"]]
        .into_iter()
        .find_map(|value| {
            value
                .as_u64()
                .and_then(|value| u16::try_from(value).ok())
                .filter(|value| *value > 0)
        })
}
