//! Workbench configuration loader (workbench `g5.workbench.actions`,
//! `g5.workbench.persona`).
//!
//! The ribbon's action templates and the phase personas are data: built-ins
//! ship in `workbench_builtins/` (the single source of truth, mirrored to the
//! browser fallback `frontend/src/lib/workbenchBuiltins.json`), and a
//! workspace commits `.g5/workbench/actions.yaml` and
//! `.g5/workbench/personas/<phase>.md` on its docs repository to change them.
//! Both are read from ONE committed snapshot of that branch
//! (`WorkspaceService::resolve_branch_committed_view`, as the explainer-template
//! loader does), never from a live checkout, so a fork sees its source's files
//! and no VM is needed.
//!
//! Override semantics differ from the append-only explainer catalog: a
//! committed `actions.yaml` replaces the built-in set of every phase it names
//! (actions matched by `id` inherit the behaviour fields they omit), unnamed
//! phases keep the built-ins, and a committed persona replaces that phase's
//! text. Any validation failure makes the whole configuration `invalid`: the
//! route returns the precise error with the built-ins, never a partial merge,
//! so a team never silently gets back actions it removed.

use std::collections::HashSet;
use std::sync::OnceLock;

use g5_api_foundation::contracts::workbench::{
    WorkbenchActionDTO, WorkbenchActionGroupDTO, WorkbenchActionKind, WorkbenchActionTarget,
    WorkbenchConfigDTO, WorkbenchConfigStateDTO, WorkbenchConfigStatus, WorkbenchContextKind,
    WorkbenchPersonaDTO, WorkbenchPersonasDTO, WorkbenchPhase, WorkbenchPhaseActionsDTO,
    WorkbenchSource, WorkbenchStartContextDTO,
};
use g5_authz::ScopedDb;
use g5_contracts::error::G5Error;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::services::workspace::workspace_service::WorkspaceService;

/// Committed configuration paths, relative to the docs repository root.
pub const ACTIONS_PATH: &str = ".g5/workbench/actions.yaml";

#[must_use]
pub fn persona_path(phase: WorkbenchPhase) -> String {
    format!(".g5/workbench/personas/{}.md", phase.as_str())
}

const BUILTIN_ACTIONS_YAML: &str = include_str!("workbench_builtins/actions.yaml");
const BUILTIN_PERSONA_BE: &str = include_str!("workbench_builtins/personas/be.md");
const BUILTIN_PERSONA_SE: &str = include_str!("workbench_builtins/personas/se.md");
const BUILTIN_PERSONA_IE: &str = include_str!("workbench_builtins/personas/ie.md");

/// The shipped defaults, parsed once.
pub struct WorkbenchBuiltins {
    pub actions: WorkbenchPhaseActionsDTO,
    pub personas: WorkbenchPersonasDTO,
    /// Short hash of every built-in byte: part of the served `revision`, so a
    /// code upgrade changes the revision a browser pinned.
    pub hash: String,
}

#[must_use]
pub fn builtins() -> &'static WorkbenchBuiltins {
    static BUILTINS: OnceLock<WorkbenchBuiltins> = OnceLock::new();
    BUILTINS.get_or_init(|| {
        let raw = parse_actions_yaml(BUILTIN_ACTIONS_YAML)
            .expect("built-in workbench actions.yaml parses");
        let actions = resolve_actions(raw, None, WorkbenchPhase::ALL.iter().copied())
            .expect("built-in workbench actions validate");
        let persona = |text: &str| WorkbenchPersonaDTO {
            source: WorkbenchSource::Builtin,
            text: text.trim().to_string(),
        };
        let mut hasher = Sha256::new();
        for bytes in [
            BUILTIN_ACTIONS_YAML,
            BUILTIN_PERSONA_BE,
            BUILTIN_PERSONA_SE,
            BUILTIN_PERSONA_IE,
        ] {
            hasher.update(bytes.as_bytes());
            hasher.update([0]);
        }
        WorkbenchBuiltins {
            actions,
            personas: WorkbenchPersonasDTO {
                be: persona(BUILTIN_PERSONA_BE),
                se: persona(BUILTIN_PERSONA_SE),
                ie: persona(BUILTIN_PERSONA_IE),
            },
            hash: hex::encode(hasher.finalize())[..12].to_string(),
        }
    })
}

/// The committed files of one snapshot, `None` where absent.
#[derive(Debug, Default)]
pub struct CommittedWorkbenchFiles {
    pub actions_yaml: Option<Vec<u8>>,
    pub personas: [Option<Vec<u8>>; 3],
}

/// Resolve the served configuration from one snapshot. `head_commit` is the
/// docs repository head the files were read at (`None` when the workspace
/// has no committed docs branch yet).
#[must_use]
pub fn resolve_workbench_config(
    head_commit: Option<&str>,
    files: &CommittedWorkbenchFiles,
) -> WorkbenchConfigDTO {
    let builtins = builtins();
    let revision = format!("{}.{}", head_commit.unwrap_or("builtin"), builtins.hash);
    let committed_anything =
        files.actions_yaml.is_some() || files.personas.iter().any(Option::is_some);
    let builtin_response =
        |status: WorkbenchConfigStatus, error: Option<String>| WorkbenchConfigDTO {
            revision: revision.clone(),
            actions: builtins.actions.clone(),
            personas: builtins.personas.clone(),
            config: WorkbenchConfigStateDTO { status, error },
        };
    if !committed_anything {
        return builtin_response(WorkbenchConfigStatus::Builtin, None);
    }
    match merge_committed(builtins, files) {
        Ok((actions, personas)) => WorkbenchConfigDTO {
            revision,
            actions,
            personas,
            config: WorkbenchConfigStateDTO {
                status: WorkbenchConfigStatus::Committed,
                error: None,
            },
        },
        Err(error) => builtin_response(WorkbenchConfigStatus::Invalid, Some(error)),
    }
}

fn merge_committed(
    builtins: &WorkbenchBuiltins,
    files: &CommittedWorkbenchFiles,
) -> Result<(WorkbenchPhaseActionsDTO, WorkbenchPersonasDTO), String> {
    let mut actions = builtins.actions.clone();
    if let Some(yaml) = files.actions_yaml.as_deref() {
        let text =
            std::str::from_utf8(yaml).map_err(|_| format!("{ACTIONS_PATH}: file is not UTF-8"))?;
        let (raw, named) = parse_committed_actions_yaml(text)?;
        let resolved = resolve_actions(raw, Some(&builtins.actions), named.iter().copied())?;
        for phase in named {
            *actions.phase_mut(phase) = resolved.phase(phase).to_vec();
        }
    }
    let mut personas = builtins.personas.clone();
    for (index, phase) in WorkbenchPhase::ALL.into_iter().enumerate() {
        if let Some(bytes) = files.personas[index].as_deref() {
            let path = persona_path(phase);
            let text =
                std::str::from_utf8(bytes).map_err(|_| format!("{path}: file is not UTF-8"))?;
            let text = text.trim();
            if text.is_empty() {
                return Err(format!("{path}: persona file is empty"));
            }
            *personas.phase_mut(phase) = WorkbenchPersonaDTO {
                source: WorkbenchSource::Committed,
                text: text.to_string(),
            };
        }
    }
    Ok((actions, personas))
}

/// Parse an `actions.yaml` whose every phase may be absent.
fn parse_actions_yaml(text: &str) -> Result<WorkbenchPhaseActionsDTO, String> {
    serde_yaml::from_str::<WorkbenchPhaseActionsDTO>(text)
        .map_err(|error| format!("{ACTIONS_PATH}: {error}"))
}

/// Parse a committed file and report which phases it names (a named phase
/// replaces the built-in set even when it is empty).
fn parse_committed_actions_yaml(
    text: &str,
) -> Result<(WorkbenchPhaseActionsDTO, Vec<WorkbenchPhase>), String> {
    let value: serde_yaml::Value =
        serde_yaml::from_str(text).map_err(|error| format!("{ACTIONS_PATH}: {error}"))?;
    let mapping = match &value {
        serde_yaml::Value::Mapping(mapping) => mapping,
        serde_yaml::Value::Null => {
            return Err(format!("{ACTIONS_PATH}: file is empty"));
        }
        _ => {
            return Err(format!(
                "{ACTIONS_PATH}: expected a mapping of phases (be, se, ie) to groups"
            ));
        }
    };
    let mut named = Vec::new();
    for key in mapping.keys() {
        let Some(key) = key.as_str() else {
            return Err(format!("{ACTIONS_PATH}: phase keys must be strings"));
        };
        let Some(phase) = WorkbenchPhase::parse(key) else {
            return Err(format!(
                "{ACTIONS_PATH}: unknown phase `{key}` (expected one of be, se, ie)"
            ));
        };
        named.push(phase);
    }
    let raw = parse_actions_yaml(text)?;
    Ok((raw, named))
}

/// Validate `raw` for `phases`, filling behaviour fields omitted by an action
/// whose `id` matches an `inherit` action of the same phase.
fn resolve_actions(
    raw: WorkbenchPhaseActionsDTO,
    inherit: Option<&WorkbenchPhaseActionsDTO>,
    phases: impl IntoIterator<Item = WorkbenchPhase>,
) -> Result<WorkbenchPhaseActionsDTO, String> {
    let mut resolved = WorkbenchPhaseActionsDTO::default();
    for phase in phases {
        let mut seen = HashSet::new();
        let mut groups = Vec::new();
        for (group_index, group) in raw.phase(phase).iter().enumerate() {
            let at = format!(
                "{ACTIONS_PATH}: phase `{}` group {}",
                phase.as_str(),
                group_index + 1
            );
            if group.name.trim().is_empty() {
                return Err(format!("{at}: missing required field `name`"));
            }
            let at = format!("{at} (`{}`)", group.name);
            let mut actions = Vec::with_capacity(group.actions.len());
            // Phase-wide built-in lookup: an action keeps inheriting by id when
            // a team moves it into (or out of) a menu.
            let lookup = |id: &str| -> Option<&WorkbenchActionDTO> {
                inherit.and_then(|set| set.find_action_in(phase, id))
            };
            for action in &group.actions {
                let action = resolve_action(action, &lookup, &at, false)?;
                for id in std::iter::once(&action.id)
                    .chain(action.items.iter().flatten().map(|item| &item.id))
                {
                    if !seen.insert(id.clone()) {
                        return Err(format!(
                            "{at}: duplicate action id `{id}` in phase `{}`",
                            phase.as_str()
                        ));
                    }
                }
                actions.push(action);
            }
            groups.push(WorkbenchActionGroupDTO {
                name: group.name.clone(),
                actions,
            });
        }
        *resolved.phase_mut(phase) = groups;
    }
    Ok(resolved)
}

fn resolve_action<'a>(
    action: &WorkbenchActionDTO,
    lookup: &dyn Fn(&str) -> Option<&'a WorkbenchActionDTO>,
    at: &str,
    nested: bool,
) -> Result<WorkbenchActionDTO, String> {
    let id = action.id.trim();
    if id.is_empty() {
        return Err(format!("{at}: an action is missing its `id`"));
    }
    let at = format!("{at} action `{id}`");
    let inherited = lookup(id);
    let pick = |own: &Option<String>, parent: fn(&WorkbenchActionDTO) -> &Option<String>| {
        own.clone()
            .or_else(|| inherited.and_then(|i| parent(i).clone()))
    };
    let label = pick(&action.label, |a| &a.label)
        .filter(|label| !label.trim().is_empty())
        .ok_or_else(|| format!("{at}: missing required field `label`"))?;
    let kind = action
        .kind
        .or_else(|| inherited.and_then(|i| i.kind))
        .ok_or_else(|| format!("{at}: missing required field `kind` (chat, open or menu)"))?;
    let prompt = pick(&action.prompt, |a| &a.prompt);
    let target = action.target.or_else(|| inherited.and_then(|i| i.target));
    let persona = action.persona.or_else(|| inherited.and_then(|i| i.persona));
    let context = action.context.or_else(|| inherited.and_then(|i| i.context));
    let tab = pick(&action.tab, |a| &a.tab);
    let phase = action.phase.or_else(|| inherited.and_then(|i| i.phase));
    let mut items = None;
    match kind {
        WorkbenchActionKind::Chat => {
            if prompt.as_deref().is_none_or(|p| p.trim().is_empty()) {
                return Err(format!("{at}: `kind: chat` requires a `prompt`"));
            }
        }
        WorkbenchActionKind::Open => {
            let Some(target) = target else {
                return Err(format!("{at}: `kind: open` requires a `target`"));
            };
            if target == WorkbenchActionTarget::View && tab.as_deref().is_none_or(str::is_empty) {
                return Err(format!("{at}: `target: view` requires a `tab`"));
            }
        }
        WorkbenchActionKind::Menu => {
            if nested {
                return Err(format!("{at}: a menu cannot contain another menu"));
            }
            let own_items = action
                .items
                .as_ref()
                .or_else(|| inherited.and_then(|i| i.items.as_ref()))
                .filter(|items| !items.is_empty())
                .ok_or_else(|| format!("{at}: `kind: menu` requires non-empty `items`"))?;
            let mut resolved_items = Vec::with_capacity(own_items.len());
            for item in own_items {
                resolved_items.push(resolve_action(item, lookup, &at, true)?);
            }
            items = Some(resolved_items);
        }
    }
    if kind != WorkbenchActionKind::Menu && action.items.is_some() {
        return Err(format!("{at}: only `kind: menu` actions carry `items`"));
    }
    Ok(WorkbenchActionDTO {
        id: id.to_string(),
        label: Some(label),
        kind: Some(kind),
        primary: action.primary.or_else(|| inherited.and_then(|i| i.primary)),
        hidden: action.hidden.or_else(|| inherited.and_then(|i| i.hidden)),
        persona,
        prompt,
        context,
        target,
        phase,
        tab,
        items,
    })
}

/// The message text of a templated chat action without the persona prefix:
/// the prompt, then the serialized context, then the user's typed text
/// (the `generateChatPrompt.ts` rule — complete requests travel as message
/// text). The browser builds the same text for an append to an open persona
/// session (`frontend/src/lib/workbenchMessage.ts`).
#[must_use]
pub fn workbench_action_message(
    prompt: &str,
    phase: WorkbenchPhase,
    context: Option<&WorkbenchStartContextDTO>,
    user_text: &str,
) -> String {
    let mut sections = vec![prompt.trim().to_string()];
    match context {
        Some(WorkbenchStartContextDTO {
            kind: WorkbenchContextKind::CurrentDoc,
            doc_path: Some(path),
        }) if !path.trim().is_empty() => {
            sections.push(format!(
                "Context: the current document is `{}`.",
                path.trim()
            ));
        }
        Some(WorkbenchStartContextDTO {
            kind: WorkbenchContextKind::CurrentDoc,
            ..
        }) => {
            sections.push("Context: no document is open; ask which one to use.".to_string());
        }
        Some(WorkbenchStartContextDTO {
            kind: WorkbenchContextKind::Phase,
            ..
        }) => {
            sections.push(format!(
                "Context: the workspace is in the {} phase.",
                phase.display_name()
            ));
        }
        Some(WorkbenchStartContextDTO {
            kind: WorkbenchContextKind::None,
            ..
        })
        | None => {}
    }
    let user_text = user_text.trim();
    if !user_text.is_empty() {
        sections.push(user_text.to_string());
    }
    sections.join("\n\n")
}

/// A persona session's first message: the persona instructions, then the
/// request (`g5.workbench.persona`: prepended before the bounded prompt is
/// frozen).
#[must_use]
pub fn persona_first_message(instructions: &str, body: &str) -> String {
    format!("{}\n\n{}", instructions.trim(), body.trim())
}

/// The request a "follow view" fork queues under its new persona
/// (`g5.workbench.persona`): the copied transcript is historical context.
#[must_use]
pub fn follow_view_message(phase: WorkbenchPhase, copied_messages: i32) -> String {
    if copied_messages > 0 {
        format!(
            "You are taking over this conversation as the {} persona. The transcript above is historical context from the previous persona: review it, state briefly how you will continue in this role, and ask what to do next.",
            phase.display_name()
        )
    } else {
        format!(
            "Start this conversation as the {} persona: introduce your role in two sentences and ask what to work on.",
            phase.display_name()
        )
    }
}

impl WorkspaceService {
    /// The repository the workbench configuration is read from: the docs
    /// repository, else the workspace's first repository.
    pub(crate) async fn workbench_repository(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
    ) -> Result<Option<crate::models::workspace::WorkspaceRepositoryState>, G5Error> {
        let docs_repo_id: Option<Uuid> = sqlx::query_scalar(
            "SELECT docs_repo_id FROM workspaces WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(workspace_id)
        .fetch_optional(db.conn())
        .await?
        .ok_or_else(|| G5Error::NotFound {
            resource: format!("workspace {workspace_id}"),
        })?;
        let members = self
            .list_workspace_repository_states(db, workspace_id)
            .await?;
        Ok(members
            .iter()
            .find(|member| Some(member.repo_id) == docs_repo_id)
            .or_else(|| members.first())
            .cloned())
    }

    /// Load the workbench configuration from one committed snapshot of the
    /// workspace's docs branch. A workspace without a committed docs branch
    /// gets the built-ins under a `builtin.<hash>` revision.
    pub async fn load_workbench_config(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
    ) -> Result<WorkbenchConfigDTO, G5Error> {
        let Some(branch) = self.workbench_repository(db, workspace_id).await? else {
            return Ok(resolve_workbench_config(
                None,
                &CommittedWorkbenchFiles::default(),
            ));
        };
        if branch.materialization_status != "materialized" || branch.head_commit.is_empty() {
            return Ok(resolve_workbench_config(
                None,
                &CommittedWorkbenchFiles::default(),
            ));
        }
        let (_, view) = self.resolve_branch_committed_view(db, &branch).await?;
        let head_commit = view.head_commit().to_string();
        let mut paths = vec![ACTIONS_PATH.to_string()];
        paths.extend(WorkbenchPhase::ALL.into_iter().map(persona_path));
        let mut files = view.read_files(self.repo_storage(), &paths).await?;
        let committed = CommittedWorkbenchFiles {
            actions_yaml: files.remove(ACTIONS_PATH).flatten(),
            personas: WorkbenchPhase::ALL.map(|phase| files.remove(&persona_path(phase)).flatten()),
        };
        Ok(resolve_workbench_config(Some(&head_commit), &committed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(actions: Option<&str>, be_persona: Option<&str>) -> CommittedWorkbenchFiles {
        CommittedWorkbenchFiles {
            actions_yaml: actions.map(|text| text.as_bytes().to_vec()),
            personas: [be_persona.map(|text| text.as_bytes().to_vec()), None, None],
        }
    }

    fn ids(groups: &[WorkbenchActionGroupDTO]) -> Vec<String> {
        groups
            .iter()
            .flat_map(|group| group.actions.iter().map(|action| action.id.clone()))
            .collect()
    }

    #[test]
    fn builtins_validate_and_cover_the_v1_button_set() {
        let builtins = builtins();
        assert_eq!(
            builtins
                .actions
                .be
                .iter()
                .map(|group| group.name.as_str())
                .collect::<Vec<_>>(),
            ["Author", "Analysis", "Mock", "Alignment"]
        );
        assert_eq!(
            builtins
                .actions
                .se
                .iter()
                .map(|group| group.name.as_str())
                .collect::<Vec<_>>(),
            ["Author", "Analysis", "Design", "Build", "Land"]
        );
        assert_eq!(
            ids(&builtins.actions.ie),
            [
                "new-doc",
                "import",
                "review-meeting",
                "extract-decisions",
                "approval",
                "rerun-verification",
                "create-pr",
                "get-approvals",
                "merge"
            ]
        );
        assert_eq!(
            serde_json::to_value(&builtins.actions.ie[..2]).unwrap(),
            serde_json::to_value(&builtins.actions.se[..2]).unwrap()
        );
        // Review meeting (g5.workbench.meeting) is one dialog: the ribbon entry
        // opens the transcript flow, and the extraction template is a hidden
        // action of the Analysis group that the dialog starts by id under
        // the BE persona over the chosen transcript.
        for phase in WorkbenchPhase::ALL {
            let meeting = builtins
                .actions
                .find_action_in(phase, "review-meeting")
                .expect("review-meeting exists");
            assert_eq!(meeting.kind, Some(WorkbenchActionKind::Open));
            assert_eq!(meeting.target, Some(WorkbenchActionTarget::Meeting));
            assert!(meeting.items.is_none());
            let extract = builtins
                .actions
                .find_action_in(phase, "extract-decisions")
                .expect("extract-decisions exists");
            assert_eq!(extract.hidden, Some(true));
            assert_eq!(extract.kind, Some(WorkbenchActionKind::Chat));
            assert_eq!(extract.persona, Some(WorkbenchPhase::Be));
            assert_eq!(extract.context, Some(WorkbenchContextKind::CurrentDoc));
            let prompt = extract.prompt.as_deref().unwrap();
            assert!(prompt.starts_with("Extract the decisions from the meeting transcript."));
            assert!(prompt.contains("g5d question create --set meeting-<slug>"));
            assert!(prompt.contains("\"id\": \"attach\""));
            assert!(builtins
                .actions
                .find_action_in(phase, "apply-decisions")
                .is_none());
            // No Analyze menu in any phase (owner instruction, 2026-10-29);
            // the Import menu's items are all drawn.
            assert!(builtins.actions.find_action_in(phase, "analyze").is_none());
            let import = builtins
                .actions
                .find_action_in(phase, "import")
                .expect("import exists");
            let items = import.items.as_deref().unwrap_or_default();
            assert!(items.iter().all(|item| item.hidden.is_none()));
        }
        // Build mock (g5.workbench.mock) is one dialog like Review meeting:
        // the ribbon entry opens the mocks dialog, and the prompt is a hidden
        // action of the Mock group the dialog seeds by id under the BE
        // persona. Mocks are Project Docs; no preview profile is involved.
        let (phase, build_mock) = builtins
            .actions
            .find_action(WorkbenchPhase::Be, "build-mock")
            .expect("build-mock exists");
        assert_eq!(phase, WorkbenchPhase::Be);
        assert_eq!(build_mock.kind, Some(WorkbenchActionKind::Open));
        assert_eq!(build_mock.target, Some(WorkbenchActionTarget::Mock));
        assert_eq!(build_mock.primary, Some(true));
        let (_, generate_mock) = builtins
            .actions
            .find_action(WorkbenchPhase::Be, "generate-mock")
            .expect("generate-mock exists");
        assert_eq!(generate_mock.hidden, Some(true));
        assert_eq!(generate_mock.kind, Some(WorkbenchActionKind::Chat));
        assert_eq!(generate_mock.persona, Some(WorkbenchPhase::Be));
        assert_eq!(generate_mock.context, Some(WorkbenchContextKind::Phase));
        let prompt = generate_mock.prompt.as_deref().unwrap();
        assert!(prompt.contains("`mocks/<surface>.html`"));
        assert!(!prompt.contains("preview_profiles"));
        assert!(builtins
            .actions
            .find_action(WorkbenchPhase::Be, "preview-mock")
            .is_none());
        // Get Approvals (g5.workbench.get-approvals) seeds an IE chat after
        // Create PR; its prompt keeps the order and limits the design relies
        // on: evidence pushed before any review, every agent review before
        // waiting, people last from the guidance files, requests only.
        let (_, get_approvals) = builtins
            .actions
            .find_action(WorkbenchPhase::Ie, "get-approvals")
            .expect("get-approvals exists");
        assert_eq!(get_approvals.label.as_deref(), Some("Get Approvals"));
        assert_eq!(get_approvals.kind, Some(WorkbenchActionKind::Chat));
        assert_eq!(get_approvals.persona, Some(WorkbenchPhase::Ie));
        assert_eq!(get_approvals.context, Some(WorkbenchContextKind::Phase));
        assert!(get_approvals.hidden.is_none());
        let prompt = get_approvals.prompt.as_deref().unwrap();
        for needle in [
            "never decide, waive or override",
            "g5d workspace approvals --json",
            "Commit and push every repository before requesting any review",
            "`requestNeeds.outstanding.agent` and `requestNeeds.agentRequestable` are both true",
            "Start all of them before waiting on any.",
            "--retry <reviewId>",
            "never change the work to get past it",
            "at most two fix rounds per requirement",
            "Be very conservative about what you fix.",
            "A defect you find while capturing follows the rule in step 4",
            "When unsure, treat it as significant.",
            "Raise it with the owner",
            "other than one waiting on the owner's decision",
            "Request people last",
            "/work/personal/approval-guidance.md",
            "the personal file wins",
            "--reviewer <userId>",
        ] {
            assert!(
                prompt.contains(needle),
                "get-approvals prompt lacks {needle:?}"
            );
        }
        for phase in [WorkbenchPhase::Be, WorkbenchPhase::Se] {
            assert!(builtins
                .actions
                .find_action_in(phase, "get-approvals")
                .is_none());
        }
        for phase in WorkbenchPhase::ALL {
            assert!(!builtins.personas.phase(phase).text.is_empty());
            assert!(builtins.personas.phase(phase).text.contains("AGENTS.md"));
        }
        assert_eq!(builtins.hash.len(), 12);
    }

    #[test]
    fn absent_files_are_builtin_and_the_revision_names_the_head() {
        let config = resolve_workbench_config(Some("abc123"), &files(None, None));
        assert_eq!(config.config.status, WorkbenchConfigStatus::Builtin);
        assert_eq!(config.revision, format!("abc123.{}", builtins().hash));
        assert_eq!(config.actions, builtins().actions);
        let no_head = resolve_workbench_config(None, &files(None, None));
        assert!(no_head.revision.starts_with("builtin."));
    }

    #[test]
    fn a_committed_phase_replaces_the_builtin_set_and_inherits_by_id() {
        let yaml = "ie:\n  - name: Release\n    actions:\n      - id: merge\n        label: Ship it\n      - id: approval\n";
        let config = resolve_workbench_config(Some("h"), &files(Some(yaml), None));
        assert_eq!(
            config.config.status,
            WorkbenchConfigStatus::Committed,
            "{:?}",
            config.config
        );
        assert_eq!(ids(&config.actions.ie), ["merge", "approval"]);
        let merge = &config.actions.ie[0].actions[0];
        assert_eq!(merge.label.as_deref(), Some("Ship it"));
        assert_eq!(merge.kind, Some(WorkbenchActionKind::Open));
        assert_eq!(merge.target, Some(WorkbenchActionTarget::Merge));
        assert_eq!(merge.primary, Some(true));
        // Unnamed phases keep the built-ins untouched.
        assert_eq!(config.actions.be, builtins().actions.be);
        assert_eq!(config.actions.se, builtins().actions.se);
        // Menu items inherit too.
        let yaml = "be:\n  - name: Author\n    actions:\n      - id: import\n        items:\n          - id: import-user-stories\n            label: Import backlog\n";
        let config = resolve_workbench_config(Some("h"), &files(Some(yaml), None));
        assert_eq!(
            config.config.status,
            WorkbenchConfigStatus::Committed,
            "{:?}",
            config.config
        );
        let item = &config.actions.be[0].actions[0].items.as_ref().unwrap()[0];
        assert_eq!(item.label.as_deref(), Some("Import backlog"));
        assert_eq!(item.kind, Some(WorkbenchActionKind::Chat));
        assert!(item.prompt.is_some());
    }

    #[test]
    fn an_action_moved_into_a_menu_still_inherits_by_id() {
        let yaml = "be:\n  - name: Mock\n    actions:\n      - id: tools\n        label: Tools\n        kind: menu\n        items:\n          - id: generate-mock\n            label: Mock it\n          - id: new-doc\n";
        let config = resolve_workbench_config(Some("h"), &files(Some(yaml), None));
        assert_eq!(
            config.config.status,
            WorkbenchConfigStatus::Committed,
            "{:?}",
            config.config
        );
        let items = config.actions.be[0].actions[0].items.as_ref().unwrap();
        assert_eq!(items[0].label.as_deref(), Some("Mock it"));
        assert_eq!(items[0].kind, Some(WorkbenchActionKind::Chat));
        assert!(items[0]
            .prompt
            .as_deref()
            .unwrap()
            .contains("clickable mock"));
        assert_eq!(items[1].kind, Some(WorkbenchActionKind::Open));
        assert_eq!(items[1].target, Some(WorkbenchActionTarget::NewDoc));
        // And out of one: a menu item promoted to the top level.
        let yaml = "be:\n  - name: Author\n    actions:\n      - id: import-user-stories\n        label: Import stories\n";
        let config = resolve_workbench_config(Some("h"), &files(Some(yaml), None));
        assert_eq!(
            config.config.status,
            WorkbenchConfigStatus::Committed,
            "{:?}",
            config.config
        );
        assert!(config.actions.be[0].actions[0].prompt.is_some());
    }

    #[test]
    fn a_committed_persona_replaces_that_phase_only() {
        let config =
            resolve_workbench_config(Some("h"), &files(None, Some("  # Custom BE\nBe brief.\n")));
        assert_eq!(config.config.status, WorkbenchConfigStatus::Committed);
        assert_eq!(config.personas.be.source, WorkbenchSource::Committed);
        assert_eq!(config.personas.be.text, "# Custom BE\nBe brief.");
        assert_eq!(config.personas.se, builtins().personas.se);
    }

    #[test]
    fn invalid_configuration_returns_builtins_with_a_precise_error() {
        let cases: Vec<(&str, &str)> = vec![
            ("be: [", "actions.yaml"),
            ("qa:\n  - name: X\n    actions: []\n", "unknown phase `qa`"),
            ("be:\n  - name: A\n    actions:\n      - id: x\n        label: X\n        kind: chat\n", "requires a `prompt`"),
            ("be:\n  - name: A\n    actions:\n      - id: x\n        label: X\n        kind: open\n", "requires a `target`"),
            ("be:\n  - name: A\n    actions:\n      - id: x\n        label: X\n        kind: open\n        target: nowhere\n", "unknown variant `nowhere`"),
            ("be:\n  - name: A\n    actions:\n      - id: x\n        label: X\n        kind: fly\n        prompt: p\n", "unknown variant `fly`"),
            ("be:\n  - name: A\n    actions:\n      - id: x\n        label: X\n        kind: chat\n        prompt: p\n        persona: qa\n", "unknown variant `qa`"),
            ("be:\n  - name: A\n    actions:\n      - id: x\n        kind: chat\n        prompt: p\n", "missing required field `label`"),
            ("be:\n  - name: A\n    actions:\n      - id: brand-new\n        label: X\n        prompt: p\n", "missing required field `kind`"),
            ("be:\n  - name: A\n    actions:\n      - id: x\n        label: X\n        kind: chat\n        prompt: p\n      - id: x\n        label: Y\n        kind: chat\n        prompt: q\n", "duplicate action id `x`"),
            ("be:\n  - name: A\n    actions:\n      - id: m\n        label: M\n        kind: menu\n        items: []\n", "requires non-empty `items`"),
            ("be:\n  - name: A\n    actions:\n      - id: v\n        label: V\n        kind: open\n        target: view\n", "requires a `tab`"),
            ("be:\n  - actions: []\n", "missing field `name`"),
        ];
        for (yaml, expected) in cases {
            let config = resolve_workbench_config(Some("h"), &files(Some(yaml), None));
            assert_eq!(
                config.config.status,
                WorkbenchConfigStatus::Invalid,
                "{yaml}"
            );
            let error = config.config.error.clone().unwrap_or_default();
            assert!(error.contains(expected), "{yaml}: {error}");
            // Never a partial merge: the built-ins come back whole.
            assert_eq!(config.actions, builtins().actions, "{yaml}");
            assert_eq!(config.personas, builtins().personas, "{yaml}");
        }
        let blank_persona = resolve_workbench_config(Some("h"), &files(None, Some("  \n")));
        assert_eq!(blank_persona.config.status, WorkbenchConfigStatus::Invalid);
        assert!(blank_persona
            .config
            .error
            .as_deref()
            .unwrap()
            .contains("personas/be.md: persona file is empty"));
    }

    #[test]
    fn action_message_serializes_prompt_context_and_user_text() {
        let text = workbench_action_message(
            "Do the thing.",
            WorkbenchPhase::Se,
            Some(&WorkbenchStartContextDTO {
                kind: WorkbenchContextKind::Phase,
                doc_path: None,
            }),
            "  and hurry  ",
        );
        assert_eq!(
            text,
            "Do the thing.\n\nContext: the workspace is in the System Engineering phase.\n\nand hurry"
        );
        let text = workbench_action_message(
            "Import.",
            WorkbenchPhase::Be,
            Some(&WorkbenchStartContextDTO {
                kind: WorkbenchContextKind::CurrentDoc,
                doc_path: Some("docs/ws/backlog.csv".into()),
            }),
            "",
        );
        assert_eq!(
            text,
            "Import.\n\nContext: the current document is `docs/ws/backlog.csv`."
        );
        assert_eq!(
            workbench_action_message("P", WorkbenchPhase::Be, None, ""),
            "P"
        );
        assert_eq!(persona_first_message(" I ", " B "), "I\n\nB");
    }

    /// The browser's fallback for an unavailable route
    /// (`frontend/src/lib/workbenchBuiltins.generated.ts`) must equal the
    /// shipped built-in actions. Regenerate with
    /// `UPDATE_WORKBENCH_BUILTINS=1 cargo test -p g5-api-core frontend_fallback`.
    #[test]
    fn frontend_fallback_matches_the_builtin_actions() {
        const HEADER: &str =
            "// GENERATED from backend/g5-api-core/src/services/agents/workbench_builtins/actions.yaml\n\
// by `UPDATE_WORKBENCH_BUILTINS=1 cargo test -p g5-api-core frontend_fallback`.\n\
// Do not edit: the client fallback for an unavailable workbench route\n\
// (workbench g5.workbench.actions).\n\
import type { WorkbenchActions } from \"./types\";\n\n\
export const WORKBENCH_BUILTIN_ACTIONS: WorkbenchActions = ";
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../frontend/src/lib/workbenchBuiltins.generated.ts");
        let json = serde_json::to_string_pretty(&builtins().actions).unwrap();
        let expected = format!("{HEADER}{json};\n");
        if std::env::var_os("UPDATE_WORKBENCH_BUILTINS").is_some() {
            std::fs::write(&path, &expected).expect("write frontend fallback");
        }
        let actual = std::fs::read_to_string(&path)
            .expect("frontend/src/lib/workbenchBuiltins.generated.ts exists");
        let actual_json = actual
            .strip_prefix(HEADER)
            .and_then(|rest| rest.strip_suffix(";\n"))
            .unwrap_or_else(|| panic!("{} does not have the generated shape", path.display()));
        let actual: serde_json::Value = serde_json::from_str(actual_json).unwrap();
        let expected: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(
            actual, expected,
            "frontend/src/lib/workbenchBuiltins.generated.ts drifted from the backend built-ins; \
             run `UPDATE_WORKBENCH_BUILTINS=1 cargo test -p g5-api-core frontend_fallback`"
        );
    }
}
