import type { WorkbenchActions, WorkspacePhase } from "../../lib/types";
import { WORKBENCH_ACTIONS } from "../../lib/workbenchDefaults";

export type View =
  | "activity"
  | "approvals"
  | "docs"
  | "ontology"
  | "source"
  | "files";
export type Mode =
  | "all"
  | "uncommitted"
  | "unpushed"
  | "base"
  | "parent"
  | "last";
export type Role = "owner" | "reviewer" | "viewer";
export type Repo = "G5-Lean" | "docs";
export type Revision = {
  heads: Record<Repo, string>;
  files: Record<string, string>;
};
export type CommitInfo = {
  summary: string;
  author: string;
  committedAt: string;
  pushedAt?: string;
};
export type WorkspaceCommit = {
  id: string;
  summary: string;
  author: string;
  recordedAt: string;
  revision: Revision;
  commits: Record<Repo, CommitInfo>;
};
export function formatCommitTime(value?: string) {
  if (!value) return "Unknown";
  return new Intl.DateTimeFormat("en-US", {
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
    timeZone: "America/New_York",
    timeZoneName: "short",
  }).format(new Date(value));
}
export type Decision = {
  person: string;
  scope: string;
  result: "approved" | "changes";
  body: string;
  revision: Revision;
  time: string;
};
export type Event = {
  id: number;
  kind: "work" | "pr" | "request" | "decision" | "comment";
  who: string;
  body: string;
  phase?: WorkspacePhase;
  file?: string;
  replyTo?: number;
  feedback?: boolean;
  time: string;
};
export const files = [
  {
    id: "requirements",
    repo: "docs" as Repo,
    path: "requirements/preview-lifecycle.md",
    title: "Preview lifecycle requirements",
    kind: "docs",
    phase: "be" as WorkspacePhase,
  },
  {
    id: "ontology",
    repo: "docs" as Repo,
    path: "ontology/PreviewSession.g5.md",
    title: "PreviewSession",
    kind: "ontology",
    phase: "se" as WorkspacePhase,
  },
  {
    id: "code",
    repo: "G5-Lean" as Repo,
    path: "frontend/src/preview/session.ts",
    title: "Preview session cleanup",
    kind: "source",
    phase: "se" as WorkspacePhase,
  },
  {
    id: "runbook",
    repo: "docs" as Repo,
    path: "operations/preview-rollout.md",
    title: "Rollout and rollback",
    kind: "docs",
    phase: "ie" as WorkspacePhase,
  },
];
export const workspaceBase: Revision = {
  heads: { "G5-Lean": "712de43", docs: "103aca2" },
  files: {
    requirements: "A workspace can run multiple preview profiles.",
    ontology: "The workspace owns preview processes.",
    code: "export async function stopPreview() {\n  await stopProcess();\n}",
    runbook: "Deploy the preview service.\nCheck that the preview loads.",
  },
};
export const parent: Revision = {
  heads: { ...workspaceBase.heads, docs: "207bad1" },
  files: {
    ...workspaceBase.files,
    runbook:
      workspaceBase.files.runbook +
      "\nParent update: notify the release channel before deployment.",
  },
};
export const prior: Revision = {
  heads: { "G5-Lean": "8c23f41", docs: "29be605" },
  files: {
    requirements:
      "A workspace runs one preview profile at a time.\nOnly the owner starts or stops the preview.",
    ontology:
      "[D · preview.session]\nThe preview session belongs to its profile.\nClosing the preview runs the profile cleanup command.",
    code: "export async function stopPreview(profile: Profile) {\n  await runCleanup(profile.cleanup);\n  await stopProcess();\n}",
    runbook:
      "Enable the new preview lifecycle for the team.\nVerify the profile cleanup command.\nRollback: disable the lifecycle feature flag.",
  },
};
export const initialPushed: Revision = {
  heads: { "G5-Lean": "8c23f41", docs: "5ac97e2" },
  files: {
    ...prior.files,
    requirements:
      prior.files.requirements +
      "\nIf cleanup fails, retain the failure and offer an owner retry.",
    ontology:
      prior.files.ontology +
      "\nA failed cleanup is visible; it is never reported as stopped successfully.",
  },
};
files.push({
  id: "guide",
  repo: "docs",
  path: "README.md",
  title: "Workspace guide",
  kind: "docs",
  phase: "be",
});
for (const revision of [workspaceBase, parent, prior, initialPushed])
  revision.files.guide =
    "Project documentation and engineering notes.\nUse the workspace timeline for shared discussion.";

function initialHistory(workspace: string): WorkspaceCommit[] {
  const owner = identity(workspace, "owner");
  const info = (
    summary: string,
    committedAt: string,
    pushedAt: string,
    author = owner,
  ): CommitInfo => ({ summary, author, committedAt, pushedAt });
  // Explicit fixture workspace cuts, oldest first. These are not reconstructed
  // by sorting per-repository commit timestamps. Each cut carries every member.
  const first: WorkspaceCommit = {
    id: "revision-1",
    summary: "Initialize preview support and documentation",
    author: "G5 team",
    recordedAt: "2026-10-05T13:15:00Z",
    revision: {
      heads: { "G5-Lean": "07cd891", docs: "03ae827" },
      files: {
        code: "// Preview lifecycle entry point.",
        guide: workspaceBase.files.guide,
      },
    },
    commits: {
      "G5-Lean": info(
        "Initialize preview support",
        "2026-10-05T13:00:00Z",
        "2026-10-05T13:15:00Z",
        "G5 team",
      ),
      docs: info(
        "Initialize project documentation",
        "2026-10-05T13:00:00Z",
        "2026-10-05T13:15:00Z",
        "G5 team",
      ),
    },
  };
  const history = [first];
  const record = (
    summary: string,
    recordedAt: string,
    heads: Partial<Revision["heads"]>,
    snapshot: Revision["files"],
    commits: Partial<WorkspaceCommit["commits"]>,
    author = owner,
  ) => {
    const previous = history[history.length - 1];
    history.push({
      id: `revision-${history.length + 1}`,
      summary,
      author,
      recordedAt,
      revision: {
        heads: { ...previous.revision.heads, ...heads },
        files: { ...previous.revision.files, ...snapshot },
      },
      commits: { ...previous.commits, ...commits },
    });
  };
  record(
    "Add the preview lifecycle entry point",
    "2026-10-06T13:15:00Z",
    { "G5-Lean": "c190fe2" },
    {
      code: "export function stopPreview() {\n  // Stop the preview process.\n}",
    },
    {
      "G5-Lean": info(
        "Add the preview lifecycle entry point",
        "2026-10-06T13:00:00Z",
        "2026-10-06T13:15:00Z",
        "G5 team",
      ),
    },
    "G5 team",
  );
  record(
    "Stop the active preview process",
    "2026-10-07T16:15:00Z",
    { "G5-Lean": "43af918" },
    { code: "export function stopPreview() {\n  stopProcess();\n}" },
    {
      "G5-Lean": info(
        "Stop the active preview process",
        "2026-10-07T16:00:00Z",
        "2026-10-07T16:15:00Z",
        "G5 team",
      ),
    },
    "G5 team",
  );
  record(
    "Establish the preview lifecycle baseline",
    "2026-10-08T13:30:00Z",
    workspaceBase.heads,
    workspaceBase.files,
    {
      "G5-Lean": info(
        "Wait for the preview process to stop",
        "2026-10-08T13:00:00Z",
        "2026-10-08T13:30:00Z",
        "G5 team",
      ),
      docs: info(
        "Describe the initial preview lifecycle",
        "2026-10-08T13:00:00Z",
        "2026-10-08T13:30:00Z",
        "G5 team",
      ),
    },
    "G5 team",
  );
  record(
    "Specify one active preview and owner controls",
    "2026-10-09T13:20:00Z",
    { docs: "38d402b" },
    { requirements: prior.files.requirements },
    {
      docs: info(
        "Specify one active preview and owner controls",
        "2026-10-09T13:00:00Z",
        "2026-10-09T13:20:00Z",
      ),
    },
  );
  record(
    "Bind the preview session to its selected profile",
    "2026-10-09T14:20:00Z",
    { "G5-Lean": "9b1fa22" },
    {
      code: "export async function stopPreview(profile: Profile) {\n  await stopProcess();\n}",
    },
    {
      "G5-Lean": info(
        "Bind the preview session to its selected profile",
        "2026-10-09T14:00:00Z",
        "2026-10-09T14:20:00Z",
      ),
    },
  );
  record(
    "Define profile ownership and cleanup ordering",
    "2026-10-09T16:10:00Z",
    { docs: "77d418a" },
    { ontology: prior.files.ontology },
    {
      docs: info(
        "Define profile ownership and cleanup ordering",
        "2026-10-09T16:00:00Z",
        "2026-10-09T16:10:00Z",
      ),
    },
  );
  record(
    "Run profile cleanup before stopping a preview",
    "2026-10-10T15:45:00Z",
    { "G5-Lean": "8c23f41" },
    { code: prior.files.code },
    {
      "G5-Lean": info(
        "Run profile cleanup before stopping a preview",
        "2026-10-10T15:30:00Z",
        "2026-10-10T15:45:00Z",
      ),
    },
  );
  record(
    "Document the preview rollout and rollback path",
    "2026-10-10T15:50:00Z",
    prior.heads,
    prior.files,
    {
      docs: info(
        "Document the preview rollout and rollback path",
        "2026-10-10T15:35:00Z",
        "2026-10-10T15:50:00Z",
      ),
    },
  );
  record(
    "Make cleanup failure and retry behavior explicit",
    "2026-10-11T13:15:00Z",
    initialPushed.heads,
    initialPushed.files,
    {
      docs: info(
        "Make cleanup failure and retry behavior explicit",
        "2026-10-11T13:00:00Z",
        "2026-10-11T13:15:00Z",
      ),
    },
  );
  return history.reverse();
}
export function sameVector(a: Revision, b: Revision) {
  return (
    Object.keys(a.heads).length === Object.keys(b.heads).length &&
    (Object.keys(a.heads) as Repo[]).every(
      (repo) => a.heads[repo] === b.heads[repo],
    )
  );
}
export function pushedRecord(state: State) {
  return state.history.find((entry) =>
    sameVector(entry.revision, state.pushed),
  );
}
export function pushDocs(
  state: State,
  workspace: string,
  when = new Date().toISOString(),
): State {
  const sha = `6db9${state.events.length.toString(16).padStart(3, "0")}`;
  const revision: Revision = {
    heads: { ...state.pushed.heads, docs: sha },
    files: {
      ...state.pushed.files,
      requirements:
        state.pushed.files.requirements +
        "\nRetry guidance clarified in this push.",
    },
  };
  const previous = pushedRecord(state);
  if (!previous) return state;
  const author = identity(workspace, "owner");
  const summary = "Clarify the cleanup retry guidance";
  const record: WorkspaceCommit = {
    id: `revision-${state.history.length + 1}`,
    summary,
    author,
    recordedAt: when,
    revision,
    commits: {
      ...previous.commits,
      docs: { summary, author, committedAt: when, pushedAt: when },
    },
  };
  return appendEvent(
    { ...state, pushed: revision, history: [record, ...state.history] },
    {
      kind: "work",
      who: author,
      body: "Pushed updated retry guidance in docs. G5-Lean is unchanged.",
    },
  );
}

export function changedFiles(before: Revision, after: Revision) {
  return files.filter((f) => before.files[f.id] !== after.files[f.id]);
}
export function changedRepos(before: Revision, after: Revision) {
  return (Object.keys(after.heads) as Repo[]).filter(
    (repo) => before.heads[repo] !== after.heads[repo],
  );
}
export function committedRevision(pushed: Revision): Revision {
  return {
    heads: { ...pushed.heads, "G5-Lean": "12ad076", docs: "77c618d" },
    files: {
      ...pushed.files,
      runbook:
        pushed.files.runbook +
        "\nLocal commit: name an on-call rollback owner.",
      code: pushed.files.code.replace(
        "await runCleanup(profile.cleanup);",
        "await runCleanup(profile.cleanup, { retry: true });",
      ),
    },
  };
}
export function currentRevision(
  pushed: Revision,
  edits: Record<string, string>,
): Revision {
  const committed = committedRevision(pushed);
  return {
    heads: committed.heads,
    files: {
      ...committed.files,
      code:
        committed.files.code +
        "\n// Uncommitted: report cleanup errors in the status panel.",
      requirements:
        committed.files.requirements +
        "\nUncommitted note: retain the original error on retry.",
      ...edits,
    },
  };
}

export const modeLabels: Record<Mode, string> = {
  all: "All files",
  uncommitted: "Uncommitted changes",
  unpushed: "All unpushed changes",
  base: "Pushed vs workspace base",
  parent: "Pushed vs parent",
  last: "Since my last review",
};
export function contentModes(
  role: Role,
  running: boolean,
  pr: boolean,
): Mode[] {
  return [
    "all",
    ...(role === "owner" && running
      ? (["uncommitted", "unpushed"] as Mode[])
      : []),
    "base",
    "parent",
    ...(pr ? (["last"] as Mode[]) : []),
  ];
}
export function comparison(
  state: State,
  role: Role,
  mode: Mode,
  previous?: Decision,
) {
  const live = role === "owner" && state.running;
  const effectiveMode =
    (mode === "uncommitted" || mode === "unpushed") && !live ? "base" : mode;
  const after =
    live && ["all", "uncommitted", "unpushed"].includes(effectiveMode)
      ? currentRevision(state.pushed, state.edits)
      : state.pushed;
  const before =
    effectiveMode === "uncommitted"
      ? committedRevision(state.pushed)
      : effectiveMode === "unpushed"
        ? state.pushed
        : effectiveMode === "parent"
          ? state.parent
          : effectiveMode === "last" && previous
            ? previous.revision
            : state.base;
  const beforeLabel =
    effectiveMode === "uncommitted"
      ? "Local HEAD"
      : effectiveMode === "unpushed"
        ? "Pushed workspace"
        : effectiveMode === "parent"
          ? "Parent’s latest pushed state"
          : effectiveMode === "last" && previous
            ? "Your last reviewed version"
            : "Workspace base";
  const afterLabel =
    live && ["all", "uncommitted", "unpushed"].includes(effectiveMode)
      ? "Current workspace · includes uncommitted work"
      : "Pushed workspace";
  return {
    effectiveMode,
    before,
    after,
    beforeLabel,
    afterLabel,
    files: effectiveMode === "all" ? files : changedFiles(before, after),
  };
}

export const scopes = [
  {
    id: "business",
    title: "Business approval",
    phase: "be" as WorkspacePhase,
    other: "David Chang",
  },
  {
    id: "security",
    title: "Security review",
    phase: "se" as WorkspacePhase,
    other: "Ben Vandiver",
  },
  {
    id: "release",
    title: "Ready-to-deploy review",
    phase: "ie" as WorkspacePhase,
    other: "Ziniu Wu",
  },
];
export function identity(workspace: string, role: Role) {
  return role === "owner"
    ? workspace === "rahul-test"
      ? "Rahul"
      : "Kaihui Zheng"
    : role === "reviewer"
      ? workspace === "rahul-test"
        ? "James Moore"
        : "Rahul"
      : "Alex Chen";
}
export function configuredReviewers(workspace: string, scope: string) {
  return [
    identity(workspace, "reviewer"),
    scopes.find((s) => s.id === scope)!.other,
  ];
}
export type State = {
  pr: boolean;
  running: boolean;
  pushed: Revision;
  history: WorkspaceCommit[];
  base: Revision;
  parent: Revision;
  decisions: Decision[];
  requests: Record<string, string[]>;
  events: Event[];
  edits: Record<string, string>;
  phase: WorkspacePhase;
};
export function initialState(workspace: string): State {
  const submitted = workspace === "preview-test";
  const events: Event[] = [
    {
      id: 1,
      kind: "work",
      who: identity(workspace, "owner"),
      body: "Created the workspace and linked the preview lifecycle task.",
      time: "Oct 8 · 09:35",
    },
    {
      id: 2,
      kind: "work",
      who: "G5",
      body: submitted
        ? "Prepared the requirements, profile cleanup implementation and rollback notes."
        : "Outlined the preview lifecycle requirements and expected outcomes.",
      phase: submitted ? "se" : "be",
      time: "Oct 10 · 11:20",
    },
  ];
  if (submitted)
    events.push(
      {
        id: 3,
        kind: "pr",
        who: "Kaihui Zheng",
        body: "Submitted the PR for preview lifecycle cleanup. Pushed revision includes G5-Lean and docs.",
        time: "Oct 10 · 12:00",
      },
      {
        id: 4,
        kind: "request",
        who: "Kaihui Zheng",
        phase: "se",
        body: "Requested Security review from Rahul and Ben Vandiver.",
        time: "Oct 10 · 12:05",
      },
      {
        id: 5,
        kind: "decision",
        who: "Rahul",
        phase: "se",
        body: "Requested changes: make failure and retry behavior explicit in the requirements and ontology.",
        time: "Oct 10 · 13:00",
      },
      {
        id: 6,
        kind: "work",
        who: "Kaihui Zheng",
        body: "Updated the lifecycle decision: cleanup failures remain visible and support an owner retry. Only docs advanced; G5-Lean is unchanged since the previous review.",
        file: "ontology",
        phase: "se",
        time: "Oct 11 · 09:15",
      },
      {
        id: 7,
        kind: "request",
        who: "Kaihui Zheng",
        phase: "se",
        body: "Requested another Security review from Rahul after updating the documentation.",
        time: "Oct 11 · 09:20",
      },
    );
  if (submitted)
    events.push({
      id: 8,
      kind: "request",
      who: "Kaihui Zheng",
      phase: "be",
      body: "Requested Business approval from Rahul and David Chang.",
      time: "Oct 11 · 09:25",
    });
  return {
    base: structuredClone(workspaceBase),
    parent: structuredClone(parent),
    pr: submitted,
    running: false,
    pushed: structuredClone(initialPushed),
    history: initialHistory(workspace),
    decisions: submitted
      ? [
          {
            person: "Rahul",
            scope: "security",
            result: "changes",
            body: "Make cleanup failure and retry behavior explicit.",
            revision: structuredClone(prior),
            time: "Oct 10 · 13:00",
          },
        ]
      : [],
    requests: submitted
      ? {
          security: ["Rahul", "Ben Vandiver"],
          business: ["Rahul", "David Chang"],
        }
      : {},
    events,
    edits: {},
    phase: submitted ? "se" : "be",
  };
}
export function latestDecision(state: State, person: string, scope: string) {
  return state.decisions.findLast(
    (d) => d.person === person && d.scope === scope,
  );
}
export function hasRequest(
  state: State,
  workspace: string,
  role: Role,
  scope: string,
) {
  return (
    state.pr &&
    (state.requests[scope] || []).includes(identity(workspace, role))
  );
}
export function mayDecide(
  state: State,
  workspace: string,
  role: Role,
  scope: string,
) {
  return (
    role !== "owner" &&
    hasRequest(state, workspace, role, scope) &&
    configuredReviewers(workspace, scope).includes(identity(workspace, role))
  );
}

export function appendEvent(
  state: State,
  event: Omit<Event, "id" | "time">,
): State {
  return {
    ...state,
    events: [
      ...state.events,
      {
        ...event,
        id: Math.max(...state.events.map((e) => e.id), 0) + 1,
        time: "Just now",
      },
    ],
  };
}
export function requestReview(
  state: State,
  workspace: string,
  role: Role,
  scope: string,
  people: string[],
): State {
  const eligible = configuredReviewers(workspace, scope),
    recipients = people.filter((p) => eligible.includes(p));
  if (role !== "owner" || !state.pr || !recipients.length) return state;
  return appendEvent(
    {
      ...state,
      requests: {
        ...state.requests,
        [scope]: [
          ...new Set([...(state.requests[scope] || []), ...recipients]),
        ],
      },
    },
    {
      kind: "request",
      who: identity(workspace, role),
      phase: scopes.find((s) => s.id === scope)!.phase,
      body: `Requested ${scopes.find((s) => s.id === scope)!.title} from ${recipients.join(", ")}.`,
    },
  );
}
export function submitDecision(
  state: State,
  workspace: string,
  role: Role,
  decision: Omit<Decision, "person" | "time">,
): State {
  if (
    !mayDecide(state, workspace, role, decision.scope) ||
    (decision.result === "changes" && !decision.body.trim())
  )
    return state;
  const person = identity(workspace, role);
  return appendEvent(
    {
      ...state,
      decisions: [
        ...state.decisions,
        {
          ...decision,
          person,
          time: "Just now",
          revision: structuredClone(decision.revision),
        },
      ],
      requests: {
        ...state.requests,
        [decision.scope]: (state.requests[decision.scope] || []).filter(
          (p) => p !== person,
        ),
      },
    },
    {
      kind: "decision",
      who: person,
      phase: scopes.find((s) => s.id === decision.scope)!.phase,
      body: `${decision.result === "approved" ? "Approved" : "Requested changes"} · ${scopes.find((s) => s.id === decision.scope)!.title}. ${decision.body}`,
    },
  );
}
// Preview-local projection: do not change server-served workbench configuration.
export const ribbonActions = Object.fromEntries(
  Object.entries(WORKBENCH_ACTIONS).map(([phase, groups]) => [
    phase,
    groups
      .map((group) => ({
        ...group,
        actions: group.actions.filter(
          (action) => action.target !== "approvals",
        ),
      }))
      .filter((group) => group.actions.length),
  ]),
) as WorkbenchActions;
