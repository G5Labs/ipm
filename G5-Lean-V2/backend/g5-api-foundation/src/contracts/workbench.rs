//! Workbench configuration contracts (workbench `g5.workbench.actions`,
//! `g5.workbench.persona`): the per-phase ribbon action set and the phase
//! personas served by `GET /api/v1/workspaces/:workspaceId/workbench`.
//!
//! The same structs deserialize a committed `.g5/workbench/actions.yaml`
//! (every behaviour field optional, inherited from the built-in action with
//! the same `id`) and serialize the resolved set to the browser (camelCase,
//! `kind` required). Field names are identical in both so a team can copy a
//! served action back into its YAML unchanged.

use serde::{Deserialize, Serialize};

/// Engineering phase (`g5.workbench.phases`), also the persona vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkbenchPhase {
    Be,
    Se,
    Ie,
}

impl WorkbenchPhase {
    pub const ALL: [WorkbenchPhase; 3] =
        [WorkbenchPhase::Be, WorkbenchPhase::Se, WorkbenchPhase::Ie];

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            WorkbenchPhase::Be => "be",
            WorkbenchPhase::Se => "se",
            WorkbenchPhase::Ie => "ie",
        }
    }

    /// Display name used in prompts and the chat header.
    #[must_use]
    pub fn display_name(self) -> &'static str {
        match self {
            WorkbenchPhase::Be => "Business Engineering",
            WorkbenchPhase::Se => "System Engineering",
            WorkbenchPhase::Ie => "Integration Engineering",
        }
    }

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "be" => Some(WorkbenchPhase::Be),
            "se" => Some(WorkbenchPhase::Se),
            "ie" => Some(WorkbenchPhase::Ie),
            _ => None,
        }
    }
}

/// What a ribbon action does: `chat` starts a templated conversation under a
/// persona, `open` opens an existing workspace surface, `menu` groups
/// actions behind one dropdown button (its `items`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkbenchActionKind {
    Chat,
    Open,
    Menu,
}

/// Existing surface an `open` action reaches through the handler that owns it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WorkbenchActionTarget {
    Approvals,
    Ship,
    Merge,
    /// The Rebase Workspace dialog (`g5.workbench.ship-actions`): a
    /// source-only rebase onto a chosen target that never advances it.
    Rebase,
    Preview,
    NewDoc,
    Upload,
    /// Upload a meeting transcript (`g5.workbench.meeting`): the page opens
    /// the document upload in transcript mode and, once the transcript is
    /// stored, starts the phase's `extract-decisions` action over it.
    Meeting,
    /// A center tab named by `tab` (`workspace_activity`, `workspace_details`, …).
    View,
    /// The Create Explainer modal (guided journey `g5.workbench.journey.steps`).
    Explainer,
    /// The Build mock dialog (`g5.workbench.mock`): this workspace's mocks under
    /// Project Docs `mocks/`, newest first, and the seeded `generate-mock` prompt.
    Mock,
    /// Seeds the Review Workspace prompt into the chat composer (guided
    /// journey `g5.workbench.journey.steps`; the prompt stays one definition).
    Review,
    /// Seeds the PR incremental re-verification prompt into the chat composer
    /// under its cost notice (`g5.workbench.review-prompts`).
    Verification,
}

/// Context a `chat` action serializes into its message text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkbenchContextKind {
    CurrentDoc,
    Phase,
    None,
}

/// One ribbon action. Deserialized from YAML with every field but `id`
/// optional; the loader fills omitted behaviour fields from the built-in
/// action with the same `id` and then requires `label` and `kind`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkbenchActionDTO {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<WorkbenchActionKind>,
    /// Tinted with the phase hue; the group's main move.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary: Option<bool>,
    /// Reachable by id (a dialog or journey step starts it) but never drawn
    /// in the ribbon (`g5.workbench.meeting`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hidden: Option<bool>,
    /// Persona a `chat` action starts under; defaults to the ribbon phase.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persona: Option<WorkbenchPhase>,
    /// Template prompt of a `chat` action (a complete request as message text).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<WorkbenchContextKind>,
    /// Surface an `open` action reaches.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<WorkbenchActionTarget>,
    /// Phase an approvals surface opens on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<WorkbenchPhase>,
    /// Center tab a `view` target opens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab: Option<String>,
    /// Dropdown entries of a `menu` action (never nested further).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<WorkbenchActionDTO>>,
}

/// A labelled group of actions in the ribbon.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkbenchActionGroupDTO {
    pub name: String,
    pub actions: Vec<WorkbenchActionDTO>,
}

/// Groups per phase. In the served form every phase is present; a committed
/// file names only the phases it overrides.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", deny_unknown_fields)]
pub struct WorkbenchPhaseActionsDTO {
    #[serde(default)]
    pub be: Vec<WorkbenchActionGroupDTO>,
    #[serde(default)]
    pub se: Vec<WorkbenchActionGroupDTO>,
    #[serde(default)]
    pub ie: Vec<WorkbenchActionGroupDTO>,
}

impl WorkbenchPhaseActionsDTO {
    #[must_use]
    pub fn phase(&self, phase: WorkbenchPhase) -> &[WorkbenchActionGroupDTO] {
        match phase {
            WorkbenchPhase::Be => &self.be,
            WorkbenchPhase::Se => &self.se,
            WorkbenchPhase::Ie => &self.ie,
        }
    }

    pub fn phase_mut(&mut self, phase: WorkbenchPhase) -> &mut Vec<WorkbenchActionGroupDTO> {
        match phase {
            WorkbenchPhase::Be => &mut self.be,
            WorkbenchPhase::Se => &mut self.se,
            WorkbenchPhase::Ie => &mut self.ie,
        }
    }

    /// Every action of `phase`, menu items included, in ribbon order.
    pub fn actions_of(&self, phase: WorkbenchPhase) -> impl Iterator<Item = &WorkbenchActionDTO> {
        self.phase(phase).iter().flat_map(|group| {
            group
                .actions
                .iter()
                .flat_map(|action| std::iter::once(action).chain(action.items.iter().flatten()))
        })
    }

    /// Find an action by id in exactly `phase` (menu items included).
    #[must_use]
    pub fn find_action_in(
        &self,
        phase: WorkbenchPhase,
        action_id: &str,
    ) -> Option<&WorkbenchActionDTO> {
        self.actions_of(phase).find(|action| action.id == action_id)
    }

    /// Find an action by id, searching `preferred` first, then every phase.
    #[must_use]
    pub fn find_action(
        &self,
        preferred: WorkbenchPhase,
        action_id: &str,
    ) -> Option<(WorkbenchPhase, &WorkbenchActionDTO)> {
        std::iter::once(preferred)
            .chain(
                WorkbenchPhase::ALL
                    .into_iter()
                    .filter(|phase| *phase != preferred),
            )
            .find_map(|phase| {
                self.actions_of(phase)
                    .find(|action| action.id == action_id)
                    .map(|action| (phase, action))
            })
    }
}

/// Where a served value came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkbenchSource {
    Builtin,
    Committed,
}

/// A phase persona's resolved instructions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkbenchPersonaDTO {
    pub source: WorkbenchSource,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct WorkbenchPersonasDTO {
    pub be: WorkbenchPersonaDTO,
    pub se: WorkbenchPersonaDTO,
    pub ie: WorkbenchPersonaDTO,
}

impl WorkbenchPersonasDTO {
    #[must_use]
    pub fn phase(&self, phase: WorkbenchPhase) -> &WorkbenchPersonaDTO {
        match phase {
            WorkbenchPhase::Be => &self.be,
            WorkbenchPhase::Se => &self.se,
            WorkbenchPhase::Ie => &self.ie,
        }
    }

    pub fn phase_mut(&mut self, phase: WorkbenchPhase) -> &mut WorkbenchPersonaDTO {
        match phase {
            WorkbenchPhase::Be => &mut self.be,
            WorkbenchPhase::Se => &mut self.se,
            WorkbenchPhase::Ie => &mut self.ie,
        }
    }
}

/// How the committed configuration resolved. `invalid` carries the precise
/// error and the response then holds the built-ins, never a partial merge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkbenchConfigStatus {
    Builtin,
    Committed,
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkbenchConfigStateDTO {
    pub status: WorkbenchConfigStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// `GET /api/v1/workspaces/:workspaceId/workbench` response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkbenchConfigDTO {
    /// Identity of the snapshot this set was resolved from: the committed
    /// head of the docs repository plus a hash of the shipped built-ins, so a
    /// committed change or a code upgrade both change it.
    pub revision: String,
    pub actions: WorkbenchPhaseActionsDTO,
    pub personas: WorkbenchPersonasDTO,
    pub config: WorkbenchConfigStateDTO,
}

/// Context a templated chat start serializes into its first message
/// (`StartSessionRequestDTO.context` companion for workbench starts).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkbenchStartContextDTO {
    pub kind: WorkbenchContextKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc_path: Option<String>,
}

// ---------------------------------------------------------------------------
// Guided journey marks (workbench g5.workbench.journey.marks)
// ---------------------------------------------------------------------------

/// The latest mark of one guided-journey step: the owner's durable
/// "Mark done" / "Not done" (`GET /api/v1/workspaces/:workspaceId/journey`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JourneyMarkDTO {
    pub step_id: String,
    pub done: bool,
    /// The owner skipped the step: not done, left out of the phase count
    /// (ui g5.ui.guided-panel). Never true together with `done`.
    #[serde(default)]
    pub skipped: bool,
    pub marked_at: chrono::DateTime<chrono::Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor_user_id: Option<uuid::Uuid>,
}

/// `GET /api/v1/workspaces/:workspaceId/journey` response: one entry per
/// step that has ever been marked (its latest event).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceJourneyDTO {
    pub marks: Vec<JourneyMarkDTO>,
}

/// `PUT /api/v1/workspaces/:workspaceId/journey/steps/:stepId` body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetJourneyMarkRequestDTO {
    pub done: bool,
    /// Skip the step instead of marking it done; absent means not skipped.
    /// `done` and `skipped` cannot both be true.
    #[serde(default)]
    pub skipped: bool,
}
