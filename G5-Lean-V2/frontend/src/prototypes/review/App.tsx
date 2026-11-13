import { useState } from "react";
import {
  Home,
  Layers,
  LayoutDashboard,
  ShieldCheck,
  FileText,
  Network,
  BookOpen,
  Database,
  FolderTree,
  AlertCircle,
  GitCompare,
  ListTodo,
  Lightbulb,
  GitBranch,
  GitPullRequest,
  MessageSquare,
  ArrowUpRight,
  Check,
  Circle,
  X,
  SlidersHorizontal,
  ChevronRight,
  ChevronDown,
  History,
  Play,
  Square,
  Code2,
} from "lucide-react";
import { PhaseChevrons } from "../../components/workbench/PhaseChevrons";
import {
  ActionRibbon,
  RibbonShapedButton,
  ribbonButtonShape,
} from "../../components/workbench/ActionRibbon";
import { Button, Modal } from "../../components/ui/UI";
import { WORKBENCH_PHASES } from "../../lib/workbenchDefaults";
import {
  appendEvent,
  changedFiles,
  changedRepos,
  configuredReviewers,
  comparison,
  pushedRecord,
  pushDocs,
  hasRequest,
  files,
  identity,
  initialState,
  latestDecision,
  mayDecide,
  requestReview,
  ribbonActions,
  scopes,
  submitDecision,
  type Decision,
  type Event,
  type Mode,
  type Revision,
  type Role,
  type State,
  type View,
} from "./model";
import "./review.css";
import { ContentBrowser } from "./ContentBrowser";
import { CommitHistory } from "./CommitHistory";
import { CommitVectorTable } from "./CommitVectorTable";

const viewNames = {
  activity: "Workspace",
  approvals: "Approvals",
  docs: "Project Docs",
  ontology: "Core Ontology",
  files: "Files",
  source: "Source Control",
};
// Match the production rail, with Approvals in the top ribbon.
const nav = [
  { view: "activity", label: "Workspace", icon: LayoutDashboard },
  { view: "docs", label: "Project Docs", icon: BookOpen },
  { view: "ontology", label: "Core Ontology", icon: Database },
  { view: "files", label: "Files", icon: FolderTree },
  { label: "Issues & tasks", icon: AlertCircle },
  { label: "Jobs", icon: Layers },
  { label: "Explainers", icon: Lightbulb },
  { label: "Archive", icon: FolderTree },
  { view: "source", label: "Source Control", icon: GitCompare },
];

const eventNames = {
  work: "Workspace activity",
  pr: "Pull requests",
  request: "Review requests",
  decision: "Review decisions",
  comment: "Discussion",
};
const taskDescription =
  "Give each preview profile a clear lifecycle. Closing a preview must clean up its resources, surface failures, and leave a reliable path to retry.\n\nExpected outcome: one active profile, visible cleanup status, and an explicit recovery action for the workspace owner.\n\nAcceptance criteria\n• Keep the same profile attached throughout cleanup and retry.\n• Retain the failure message if cleanup fails.\n• Never report a preview as successfully stopped when cleanup failed.\n• Document the rollout owner and rollback procedure.\n\nReview requirements and implementation details separately so reviewers can understand both the task and the chosen design.";
const prDescription =
  "Make preview sessions belong to a single profile. Run the profile cleanup command before stopping; retain failures so the owner can retry. Document the lifecycle contract and the rollout/rollback path together.\n\nDecisions: bind cleanup to the selected profile and preserve the original error when cleanup fails. The owner retries the same session, keeping failure handling predictable.\n\nRollout: enable the lifecycle change for the team, verify cleanup commands and record the on-call rollback owner. The failure message remains visible until the owner resolves it.\n\nReview focus: confirm the business expectations, security boundary around owner actions and the deployment procedure. These decisions cover code and project documentation together.";

function Vector({
  revision,
  before,
  label = "Pushed revision",
}: {
  revision: Revision;
  before?: Revision;
  label?: string;
}) {
  return (
    <details className="rm-vector">
      <summary>
        <GitBranch size={13} />
        {label} · 2 repositories
      </summary>
      <div>
        {Object.entries(revision.heads).map(([repo, sha]) => (
          <div className="rm-vector-row" key={repo}>
            <span>{repo}</span>
            <code>{sha}</code>
            {before && (
              <small>
                {before.heads[repo as keyof typeof before.heads] === sha
                  ? "Unchanged"
                  : `Changed from ${before.heads[repo as keyof typeof before.heads]}`}
              </small>
            )}
          </div>
        ))}
      </div>
    </details>
  );
}
function Avatar({ name }: { name: string }) {
  return (
    <span className="rm-avatar" aria-hidden="true">
      {name
        .split(" ")
        .map((s) => s[0])
        .slice(0, 2)
        .join("")}
    </span>
  );
}

export default function ReviewMock() {
  const [workspace, setWorkspace] = useState("preview-test");
  const [role, setRole] = useState<Role>("reviewer");
  const [states, setStates] = useState<Record<string, State>>(() => ({
    "rahul-test": initialState("rahul-test"),
    "preview-test": initialState("preview-test"),
  }));
  const state = states[workspace];
  const currentRecord = pushedRecord(state);
  const update = (fn: (s: State) => State) =>
    setStates((all) => ({ ...all, [workspace]: fn(all[workspace]) }));
  const [view, setView] = useState<View>("activity");
  const [mode, setMode] = useState<Mode>("all");
  const [scope, setScope] = useState("security");
  const [selectedFile, setSelectedFile] = useState("code");
  const [modal, setModal] = useState<
    "request" | "decision" | "feedback" | "pr" | null
  >(null);
  const [people, setPeople] = useState<string[]>([]);
  const [drafts, setDrafts] = useState<
    Record<string, { body: string; result: Decision["result"] }>
  >({});
  const draftKey = `${workspace}:${role}:${scope}`;
  const body = drafts[draftKey]?.body || "";
  const result = drafts[draftKey]?.result || "approved";
  const setBody = (body: string) =>
    setDrafts((d) => ({
      ...d,
      [draftKey]: { result: d[draftKey]?.result ?? "approved", body },
    }));
  const setResult = (result: Decision["result"]) =>
    setDrafts((d) => ({
      ...d,
      [draftKey]: { body: d[draftKey]?.body ?? "", result },
    }));
  const [reading, setReading] = useState<
    "task" | "implementation" | "revision" | null
  >(null);
  const [feedbackBody, setFeedbackBody] = useState("");
  const [historyOpen, setHistoryOpen] = useState(false);
  const [target, setTarget] = useState<Revision>(state.pushed);
  const [prBody, setPrBody] = useState(prDescription);
  const [descriptions, setDescriptions] = useState<Record<string, string>>({
    "preview-test": prDescription,
  });
  const [comment, setComment] = useState("");
  const [replyTo, setReplyTo] = useState<Event | null>(null);
  const [hiddenKinds, setHiddenKinds] = useState<string[]>([]);
  const [phaseFilter, setPhaseFilter] = useState("all");
  const [notice, setNotice] = useState("");
  const person = identity(workspace, role),
    owner = identity(workspace, "owner");
  const pending = scopes.filter((s) =>
    hasRequest(state, workspace, role, s.id),
  );
  const previous = latestDecision(state, person, scope);
  const selected = files.find((f) => f.id === selectedFile)!;
  const content = comparison(state, role, mode, previous);
  const advanced = changedRepos(state.base, state.parent);
  const workspaceChanges = changedFiles(state.base, state.pushed);
  const openView = (next: View, file?: string) => {
    setView(next);
    setNotice("");
    setReplyTo(null);
    setComment("");
    if (file) setSelectedFile(file);
    else if (["docs", "source", "ontology", "files"].includes(next))
      setSelectedFile(
        next === "source" || next === "files"
          ? "code"
          : files.find((f) => f.kind === next)!.id,
      );
  };
  const showChanges = (next: View, file?: string) => {
    setMode("base");
    openView(next, file);
  };
  const switchRole = (next: Role) => {
    setRole(next);
    setNotice("");
    setMode("all");
    setModal(null);
    setReplyTo(null);
    setComment("");
    setDrafts({});
    setFeedbackBody("");
    setReading(null);
    setHistoryOpen(false);
  };
  const switchWorkspace = (next: string) => {
    setWorkspace(next);
    switchRole(next === "rahul-test" ? "owner" : "reviewer");
    setView("activity");
    setNotice("");
  };
  const openRequest = () => {
    setScope("security");
    setPeople([]);
    setModal("request");
  };
  const openDecision = (type?: string) => {
    const next =
      type || (pending.some((s) => s.id === scope) ? scope : pending[0]?.id);
    if (!state.pr || !next) return;
    setScope(next);
    setTarget(structuredClone(state.pushed));
    setModal("decision");
  };
  const pushSample = () => update((s) => pushDocs(s, workspace));
  const postComment = () => {
    if (!comment.trim()) return;
    update((s) =>
      appendEvent(s, {
        kind: "comment",
        who: person,
        body: comment.trim(),
        file: replyTo
          ? replyTo.file
          : ["docs", "source", "files", "ontology"].includes(view)
            ? selected.id
            : undefined,
        replyTo: replyTo?.id,
        phase: replyTo
          ? replyTo.phase
          : ["docs", "source", "files", "ontology"].includes(view)
            ? selected.phase
            : undefined,
      }),
    );
    setComment("");
    setReplyTo(null);
  };
  const eventCard = (event: Event) => {
    const Icon =
      event.kind === "decision"
        ? ShieldCheck
        : event.kind === "pr"
          ? GitPullRequest
          : event.kind === "comment"
            ? MessageSquare
            : event.kind === "request"
              ? ShieldCheck
              : GitBranch;
    const ref = files.find((f) => f.id === event.file);
    const replies = state.events.filter((e) => e.replyTo === event.id);
    return (
      <article
        className={`rm-event ${event.kind === "comment" ? "rm-discussion" : ""}`}
        key={event.id}
        id={`event-${event.id}`}
      >
        <span className="rm-event-icon">
          <Icon size={15} />
        </span>
        <div className="rm-event-content">
          <div className="rm-event-meta">
            <strong>{event.who}</strong>
            <span>{event.time}</span>
            {event.feedback && (
              <span className="rm-badge">General feedback</span>
            )}
            {event.phase && (
              <span className="rm-phase-tag">
                {WORKBENCH_PHASES[event.phase].name}
              </span>
            )}
          </div>
          <p>{event.body}</p>
          {ref && (
            <button
              data-testid="review-mock-discussion-file"
              className="rm-link"
              onClick={() => showChanges(ref.kind as View, ref.id)}
            >
              {ref.repo} / {ref.path} <ArrowUpRight size={12} />
            </button>
          )}
          {replies.map((reply) => (
            <div className="rm-reply" key={reply.id}>
              <strong>{reply.who}</strong>
              <span>{reply.time}</span>
              <p>{reply.body}</p>
            </div>
          ))}
          {(event.kind === "comment" || event.kind === "decision") && (
            <button
              data-testid="review-mock-discussion-reply"
              className="rm-link rm-small"
              onClick={() => {
                setReplyTo(event);
                setComment("");
              }}
            >
              Reply
            </button>
          )}
        </div>
      </article>
    );
  };
  const composer = (
    <div className="rm-composer">
      <Avatar name={person} />
      <div>
        {replyTo && (
          <div className="rm-row">
            Replying to {replyTo.who}
            <button
              data-testid="review-mock-cancel-reply"
              aria-label="Cancel reply"
              onClick={() => setReplyTo(null)}
            >
              <X size={13} />
            </button>
          </div>
        )}
        <textarea
          aria-label={replyTo ? "Write a reply" : "Write a comment"}
          value={comment}
          onChange={(e) => setComment(e.target.value)}
          placeholder={
            replyTo
              ? "Write a reply…"
              : view === "activity"
                ? "Add to the workspace discussion…"
                : "Discuss this change…"
          }
          rows={2}
        />
        <div className="rm-composer-footer">
          <small>As {person} · shared workspace discussion</small>
          <Button
            kind="secondary"
            disabled={!comment.trim()}
            onClick={postComment}
          >
            {replyTo ? "Post reply" : "Comment"}
          </Button>
        </div>
      </div>
    </div>
  );
  const action =
    role === "owner" ? (
      <Button kind="primary" disabled={!state.pr} onClick={openRequest}>
        Request review
      </Button>
    ) : (
      <div className="rm-review-actions">
        {pending.length > 0 && (
          <Button kind="primary" onClick={() => openDecision()}>
            Review requests ({pending.length})
          </Button>
        )}
        <Button
          kind={pending.length ? "secondary" : "primary"}
          onClick={() => setModal("feedback")}
        >
          Leave feedback
        </Button>
      </div>
    );

  return (
    <div className="rm-app">
      <div className="rm-prototype">
        <span>
          <span className="rm-dot" /> Interactive prototype · sample data · no
          QA writes
        </span>
        <details>
          <summary>
            Demo scenarios <SlidersHorizontal size={12} />
          </summary>
          <div className="rm-scenario">
            <label>
              Workspace example
              <select
                aria-label="Workspace example"
                value={workspace}
                onChange={(e) => switchWorkspace(e.target.value)}
              >
                <option value="preview-test">
                  preview-test ·{" "}
                  {states["preview-test"].pr ? "PR submitted" : "No PR yet"}
                </option>
                <option value="rahul-test">
                  rahul-test ·{" "}
                  {states["rahul-test"].pr ? "PR submitted" : "No PR yet"}
                </option>
              </select>
            </label>
            <label>
              View as
              <select
                aria-label="View as"
                value={role}
                onChange={(e) => switchRole(e.target.value as Role)}
              >
                <option value="owner">Owner · {owner}</option>
                <option value="reviewer">
                  Reviewer · {identity(workspace, "reviewer")}
                </option>
                <option value="viewer">Unrequested reviewer · Alex Chen</option>
              </select>
            </label>
            <Button kind="secondary" onClick={pushSample}>
              Simulate docs push
            </Button>
            <Button
              kind="ghost"
              onClick={() => {
                setStates({
                  "rahul-test": initialState("rahul-test"),
                  "preview-test": initialState("preview-test"),
                });
                setDescriptions({ "preview-test": prDescription });
                setNotice("Demo reset.");
                setMode("all");
              }}
            >
              Reset demo
            </Button>
            <small>
              All actions are local to this tab. Reload resets the examples.
            </small>
          </div>
        </details>
      </div>
      <div className="rm-workspace-tabs" aria-label="Open workspaces">
        <button
          data-testid="review-mock-global-home"
          title="Global Home · outside this prototype"
          aria-label="Global Home"
          disabled
        >
          <Home size={15} />
        </button>
        {["preview-test", "rahul-test"].map((name) => (
          <button
            data-testid="review-mock-workspace-tab"
            key={name}
            aria-current={workspace === name ? "page" : undefined}
            onClick={() => switchWorkspace(name)}
          >
            <Layers size={13} />
            {name}
          </button>
        ))}
      </div>
      <header className="rm-top">
        <strong className="rm-brand">G5</strong>
        <span className="rm-muted">g5</span>
        <ChevronRight size={12} />
        <span>{workspace}</span>
        <span className="rm-top-spacer" />
        <span className="rm-muted">Owner: {owner}</span>
        <span className={`rm-status ${state.running ? "good" : ""}`}>
          <span className="rm-dot" />
          {state.running ? "Running" : "Stopped"}
        </span>
        {role === "owner" && (
          <Button
            kind="secondary"
            onClick={() => update((s) => ({ ...s, running: !s.running }))}
          >
            {state.running ? <Square size={12} /> : <Play size={12} />}{" "}
            {state.running ? "Stop" : "Start"}
          </Button>
        )}
        <Avatar name={person} />
      </header>
      <div className="rm-workbench">
        <PhaseChevrons
          phase={state.phase}
          variant="minimal"
          readOnly={role !== "owner"}
          onSelectPhase={(phase) =>
            update((s) =>
              appendEvent(
                { ...s, phase },
                {
                  kind: "work",
                  who: person,
                  phase,
                  body: `Moved the workspace to ${WORKBENCH_PHASES[phase].name}.`,
                },
              ),
            )
          }
        />
        <ActionRibbon
          phase={state.phase}
          variant="extended-icons"
          configStatus="builtin"
          endActions={
            <RibbonShapedButton
              shape={ribbonButtonShape("extended-icons", {})}
              icon={<ShieldCheck size={22} aria-hidden="true" />}
              label="Approvals"
              title={
                pending.length > 0
                  ? `${pending.length} pending requests`
                  : undefined
              }
              testId="review-mock-ribbon-approvals"
              aria-current={view === "approvals" ? "page" : undefined}
              onClick={() => openView("approvals")}
            />
          }
          actions={ribbonActions}
          actionState={() => ({
            disabled: true,
            title:
              "Agent and deployment actions are outside this review prototype",
          })}
        />
      </div>
      <div className="rm-body">
        <nav className="rm-rail" aria-label="Workspace navigation">
          {nav.map((item) => (
            <button
              data-testid="review-mock-rail-destination"
              key={item.label}
              title={
                item.view
                  ? item.label
                  : `${item.label} · existing destination, outside this prototype`
              }
              aria-label={item.label}
              aria-current={item.view === view ? "page" : undefined}
              disabled={!item.view}
              onClick={() => item.view && openView(item.view as View)}
            >
              <item.icon size={14} />
            </button>
          ))}
        </nav>
        <main className="rm-main" key={`${workspace}-${view}`}>
          <div className="rm-access">
            {role === "owner"
              ? state.running
                ? "Your workspace is running. Current work is available in the content views."
                : "Workspace is stopped. Pushed content and discussion remain available. Start to edit current work."
              : `Viewing ${owner}’s workspace. Pushed content and discussion are available${state.running ? "; current work is private to the owner" : ""}.`}
          </div>
          {notice && (
            <div className="rm-notice" role="status">
              {notice}
              <button
                data-testid="review-mock-dismiss-notice"
                onClick={() => setNotice("")}
                aria-label="Dismiss notice"
              >
                <X size={14} />
              </button>
            </div>
          )}
          <div className="rm-page-head">
            <div>
              <div className="rm-eyebrow">
                {workspace} <span>/</span> {viewNames[view]}
              </div>
              <h1>
                {view === "activity"
                  ? "Preview lifecycle cleanup"
                  : viewNames[view]}
              </h1>
              <p>
                {view === "activity"
                  ? "Task, implementation and the shared history of this workspace."
                  : view === "approvals"
                    ? "Human reviews across the workspace’s engineering phases."
                    : view === "ontology"
                      ? "Review the intent, decisions and relationships behind the change."
                      : "Inspect changes and discuss them in context."}
              </p>
            </div>
            {view === "approvals" && (
              <div className="rm-primary-action">{action}</div>
            )}
          </div>

          {view === "activity" && (
            <>
              <section className="rm-summary-grid">
                <div className="rm-card">
                  <div className="rm-section-label">
                    <ListTodo size={14} /> WORKSPACE TASK{" "}
                    <button
                      data-testid="review-mock-task-issue"
                      className="rm-badge rm-link"
                      onClick={() => setReading("task")}
                    >
                      Issue #343 <ArrowUpRight size={11} />
                    </button>
                  </div>
                  <h2>Make stopping a preview predictable</h2>
                  <p className="rm-description-preview">{taskDescription}</p>
                  <button
                    data-testid="review-mock-read-task"
                    className="rm-link"
                    onClick={() => setReading("task")}
                  >
                    Read full task description
                  </button>
                </div>
                <div className="rm-card">
                  <div className="rm-section-label">
                    <GitPullRequest size={14} /> IMPLEMENTATION & DECISIONS{" "}
                    {state.pr ? (
                      <button
                        data-testid="review-mock-read-revision"
                        className="rm-badge good rm-link"
                        aria-haspopup="dialog"
                        title="View pushed revision"
                        onClick={() => setReading("revision")}
                      >
                        PR submitted <ChevronDown size={11} />
                      </button>
                    ) : (
                      <span className="rm-badge">No PR yet</span>
                    )}
                  </div>
                  {state.pr ? (
                    <>
                      <h2>Profile-scoped cleanup and recovery</h2>
                      <p className="rm-description-preview">
                        {descriptions[workspace]}
                      </p>
                      <button
                        data-testid="review-mock-read-implementation"
                        className="rm-link"
                        onClick={() => setReading("implementation")}
                      >
                        Read full implementation description
                      </button>
                    </>
                  ) : (
                    <>
                      <h2>Work is still taking shape</h2>
                      <p>
                        The timeline already records work and discussion. Submit
                        a PR with the implementation and decisions when the
                        pushed work is ready for review.
                      </p>
                      {role === "owner" ? (
                        <Button
                          kind="secondary"
                          onClick={() => {
                            setPrBody(prDescription);
                            setModal("pr");
                          }}
                        >
                          Submit PR
                        </Button>
                      ) : (
                        <div className="rm-footnote">
                          The workspace owner submits the PR.
                        </div>
                      )}
                    </>
                  )}
                </div>
              </section>
              <section
                className="rm-summary-strip"
                aria-label="Workspace change and approval summary"
              >
                {[
                  {
                    kind: "source",
                    icon: Code2,
                    title: "Source Control",
                    text: `${workspaceChanges.filter((f) => f.kind === "source").length} code changes`,
                  },
                  {
                    kind: "docs",
                    icon: FileText,
                    title: "Project Docs",
                    text: `${workspaceChanges.filter((f) => f.kind === "docs").length} document changes`,
                  },
                  {
                    kind: "ontology",
                    icon: Network,
                    title: "Ontology",
                    text: `${workspaceChanges.filter((f) => f.kind === "ontology").length} decision updated`,
                  },
                ].map((item) => (
                  <button
                    data-testid="review-mock-change-summary"
                    key={item.kind}
                    onClick={() => showChanges(item.kind as View)}
                  >
                    <item.icon size={18} />
                    <span>
                      <strong>{item.title}</strong>
                      <small>{item.text}</small>
                    </span>
                    <ArrowUpRight size={13} />
                  </button>
                ))}
                <button
                  data-testid="review-mock-approvals-summary"
                  onClick={() => openView("approvals")}
                >
                  <ShieldCheck size={18} />
                  <span>
                    <strong>Approvals</strong>
                    <small>
                      {!state.pr
                        ? "Available after PR submission"
                        : `${Object.values(state.requests).flat().length} reviewer requests pending`}
                    </small>
                  </span>
                  <ArrowUpRight size={13} />
                </button>
              </section>
              <p className="rm-comparison-note">
                Changes compared with workspace base.{" "}
                {advanced.length > 0
                  ? `Parent has advanced in ${advanced.join(", ")} since this workspace’s base.`
                  : "Parent has not advanced since workspace base."}
              </p>
              <section className="rm-timeline">
                <div className="rm-section-head">
                  <div>
                    <h2>Timeline</h2>
                    <p>
                      One conversation across Business, System and Integration
                      Engineering.
                    </p>
                  </div>
                  <details className="rm-filters">
                    <summary>
                      <SlidersHorizontal size={14} /> Filter activity
                      {hiddenKinds.length || phaseFilter !== "all" ? " •" : ""}
                    </summary>
                    <div>
                      {Object.entries(eventNames).map(([kind, label]) => (
                        <label key={kind}>
                          <input
                            type="checkbox"
                            checked={!hiddenKinds.includes(kind)}
                            onChange={() =>
                              setHiddenKinds((all) =>
                                all.includes(kind)
                                  ? all.filter((k) => k !== kind)
                                  : [...all, kind],
                              )
                            }
                          />
                          {label}
                        </label>
                      ))}
                      <label>
                        Phase
                        <select
                          aria-label="Timeline phase filter"
                          value={phaseFilter}
                          onChange={(e) => setPhaseFilter(e.target.value)}
                        >
                          <option value="all">All phases</option>
                          {Object.entries(WORKBENCH_PHASES).map(([id, p]) => (
                            <option value={id} key={id}>
                              {p.name}
                            </option>
                          ))}
                        </select>
                      </label>
                      <small>Workspace-wide events remain visible.</small>
                    </div>
                  </details>
                </div>
                <div className="rm-feed">
                  {state.events
                    .filter(
                      (e) =>
                        !e.replyTo &&
                        !hiddenKinds.includes(e.kind) &&
                        (phaseFilter === "all" ||
                          !e.phase ||
                          e.phase === phaseFilter),
                    )
                    .map(eventCard)}
                </div>
                {composer}
              </section>
            </>
          )}

          {view === "approvals" && (
            <>
              <div className="rm-approval-intro">
                <ShieldCheck size={21} />
                <div>
                  <strong>
                    {state.pr
                      ? "Reviews apply to pushed work"
                      : "Submit a PR before requesting reviews"}
                  </strong>
                  <p>
                    {state.pr
                      ? "Every decision records the reviewed revision of all repositories. Unpushed commits and dirty files are excluded."
                      : "Review types and eligible reviewers are configured by your organization. No review has been requested or recorded yet."}
                  </p>
                  {!state.pr && role === "owner" && (
                    <button
                      data-testid="review-mock-submit-pr-link"
                      className="rm-link"
                      onClick={() => {
                        setPrBody(prDescription);
                        setModal("pr");
                      }}
                    >
                      Submit PR <ArrowUpRight size={13} />
                    </button>
                  )}
                </div>
                <Vector revision={state.pushed} />
              </div>
              {role !== "owner" && state.pr && (
                <section className="rm-my-requests">
                  <h2>
                    Your review requests{" "}
                    {pending.length > 0 && `(${pending.length})`}
                  </h2>
                  {pending.length ? (
                    pending.map((t) => (
                      <div className="rm-row" key={t.id}>
                        <div>
                          <strong>{t.title}</strong>
                          <small>
                            {WORKBENCH_PHASES[t.phase].name} · requested by{" "}
                            {owner}
                          </small>
                        </div>
                        <Button
                          kind="secondary"
                          onClick={() => openDecision(t.id)}
                        >
                          Review {t.title}
                        </Button>
                      </div>
                    ))
                  ) : (
                    <p>
                      No reviews are currently requested from you. You can leave
                      general feedback for the workspace owner.
                    </p>
                  )}
                  {pending.length > 0 && (
                    <p className="rm-footnote">
                      Each type is a separate review. Submitting one leaves your
                      other requests pending.
                    </p>
                  )}
                </section>
              )}
              <div className="rm-approval-list">
                {scopes.map((s) => {
                  const names = configuredReviewers(workspace, s.id);
                  return (
                    <section className="rm-card" key={s.id}>
                      <div className="rm-section-head">
                        <div>
                          <div className="rm-eyebrow">
                            {WORKBENCH_PHASES[s.phase].name}
                          </div>
                          <h2>{s.title}</h2>
                        </div>
                        <span className="rm-badge">
                          {s.id === "release"
                            ? "Before deployment"
                            : "Phase sign-off"}
                        </span>
                      </div>
                      <div className="rm-reviewers">
                        {names.map((name) => {
                          const decision = latestDecision(state, name, s.id),
                            requested = (state.requests[s.id] || []).includes(
                              name,
                            ),
                            moved = decision
                              ? changedRepos(decision.revision, state.pushed)
                              : [];
                          return (
                            <div className="rm-reviewer" key={name}>
                              <Avatar name={name} />
                              <div>
                                <strong>
                                  {name}
                                  {name === person ? " (you)" : ""}
                                </strong>
                                <small>
                                  {decision
                                    ? `${decision.result === "approved" ? "Approved" : "Changes requested"} · ${decision.time}`
                                    : requested
                                      ? "Review requested"
                                      : "Not requested"}
                                </small>
                                {decision && moved.length > 0 && (
                                  <span className="rm-changed">
                                    {moved.join(", ")} changed since this review
                                  </span>
                                )}
                              </div>
                              {requested ? (
                                <span className="rm-badge">
                                  {decision
                                    ? "Review requested again"
                                    : "Awaiting review"}
                                </span>
                              ) : decision ? (
                                <span
                                  className={`rm-badge ${decision.result === "approved" ? "good" : "warn"}`}
                                >
                                  {decision.result === "approved" ? (
                                    <Check size={12} />
                                  ) : (
                                    <Circle size={12} />
                                  )}{" "}
                                  {decision.result === "approved"
                                    ? "Approved"
                                    : "Changes requested"}
                                </span>
                              ) : (
                                <span className="rm-muted">—</span>
                              )}
                            </div>
                          );
                        })}
                      </div>
                    </section>
                  );
                })}
              </div>
              <section className="rm-history">
                <h2>Review history</h2>
                {state.decisions.length === 0 ? (
                  <p className="rm-muted">No submitted reviews.</p>
                ) : (
                  state.decisions.map((d, i) => (
                    <div className="rm-card" key={i}>
                      <div className="rm-row">
                        <strong>
                          {d.person} ·{" "}
                          {d.result === "approved"
                            ? "Approved"
                            : "Changes requested"}
                        </strong>
                        <span className="rm-muted">{d.time}</span>
                      </div>
                      <small>
                        {scopes.find((s) => s.id === d.scope)?.title}
                      </small>
                      <p>{d.body || "No additional comment."}</p>
                      <Vector label="Reviewed revision" revision={d.revision} />
                    </div>
                  ))
                )}
                <p className="rm-footnote">
                  A later push is shown as a source change. Approval validity
                  after a push is not decided in this prototype.
                </p>
              </section>
              <section className="rm-history">
                <h2>General feedback</h2>
                <p className="rm-muted">
                  Discussion is shared with the workspace timeline and does not
                  count as an approval.
                </p>
                {state.events
                  .filter((e) => e.feedback && !e.replyTo)
                  .map(eventCard)}
                {replyTo?.feedback && composer}
              </section>
            </>
          )}

          {["docs", "ontology", "source", "files"].includes(view) && (
            <ContentBrowser
              state={state}
              role={role}
              mode={mode}
              setMode={setMode}
              scope={scope}
              setScope={setScope}
              previous={previous}
              view={view}
              selectedId={selectedFile}
              onSelect={(id) => {
                setSelectedFile(id);
                setReplyTo(null);
                setComment("");
              }}
              onEdit={(id, text) =>
                update((s) => ({ ...s, edits: { ...s.edits, [id]: text } }))
              }
              discussion={
                <>
                  {state.events
                    .filter((e) => e.file === selectedFile && !e.replyTo)
                    .map(eventCard)}
                  {composer}
                </>
              }
              vector={
                view === "source" ? (
                  <Button
                    kind="secondary"
                    className="rm-history-trigger"
                    aria-haspopup="dialog"
                    onClick={() => setHistoryOpen(true)}
                  >
                    <History size={14} /> Commit History
                  </Button>
                ) : (
                  <Vector
                    revision={content.after}
                    label={content.afterLabel}
                    before={
                      content.effectiveMode === "all"
                        ? undefined
                        : content.before
                    }
                  />
                )
              }
            />
          )}
        </main>
      </div>

      {historyOpen && (
        <CommitHistory state={state} onClose={() => setHistoryOpen(false)} />
      )}
      {reading && (
        <Modal
          ariaLabel={
            reading === "task"
              ? "Workspace task · Issue #343"
              : reading === "revision"
                ? "Pushed revision"
                : "Implementation and decisions"
          }
          onClose={() => setReading(null)}
          size="lg"
          className="rm-modal"
        >
          <div className="rm-row">
            <h2>
              {reading === "task"
                ? "Workspace task · Issue #343"
                : reading === "revision"
                  ? "Pushed revision"
                  : "Implementation and decisions"}
            </h2>
            <button
              data-testid="review-mock-close-reading"
              aria-label={
                reading === "revision" ? "Close revision" : "Close description"
              }
              onClick={() => setReading(null)}
            >
              <X size={18} />
            </button>
          </div>
          {reading === "revision" ? (
            <>
              <p>
                Latest pushed state · {Object.keys(state.pushed.heads).length}{" "}
                repositories
              </p>
              {currentRecord ? (
                <CommitVectorTable
                  entries={[currentRecord]}
                  previous={
                    state.history[state.history.indexOf(currentRecord) + 1]
                  }
                  current={state.pushed}
                  base={state.base}
                />
              ) : (
                <p>Revision history unavailable.</p>
              )}
            </>
          ) : (
            /* Keyboard focus lets readers scroll long descriptions. */
            <div
              className="rm-full-description"
              role="region"
              aria-label="Full description"
              // eslint-disable-next-line jsx-a11y/no-noninteractive-tabindex
              tabIndex={0}
            >
              {reading === "task" ? taskDescription : descriptions[workspace]}
            </div>
          )}
        </Modal>
      )}
      {modal && (
        <Modal
          ariaLabel={
            modal === "request"
              ? "Request review"
              : modal === "decision"
                ? "Submit review"
                : modal === "feedback"
                  ? "Leave feedback"
                  : "Submit PR"
          }
          onClose={() => setModal(null)}
          size="md"
          className="rm-modal"
        >
          <div className="rm-row">
            <h2>
              {modal === "request"
                ? "Request review"
                : modal === "decision"
                  ? "Submit review"
                  : modal === "feedback"
                    ? "Leave feedback"
                    : "Submit PR"}
            </h2>
            <button
              data-testid="review-mock-close-dialog"
              aria-label="Close dialog"
              onClick={() => setModal(null)}
            >
              <X size={18} />
            </button>
          </div>
          {modal === "feedback" ? (
            <>
              <p>
                Share general feedback with the workspace owner. This appears in
                the timeline and does not submit an approval or complete a
                review request.
              </p>
              <label>
                Feedback
                <textarea
                  aria-label="Feedback"
                  rows={5}
                  value={feedbackBody}
                  onChange={(e) => setFeedbackBody(e.target.value)}
                />
              </label>
              <div className="rm-modal-footer">
                <Button kind="ghost" onClick={() => setModal(null)}>
                  Cancel
                </Button>
                <Button
                  kind="primary"
                  disabled={!feedbackBody.trim()}
                  onClick={() => {
                    update((s) =>
                      appendEvent(s, {
                        kind: "comment",
                        who: person,
                        body: feedbackBody.trim(),
                        feedback: true,
                      }),
                    );
                    setFeedbackBody("");
                    setModal(null);
                    setNotice("Feedback posted to the workspace timeline.");
                  }}
                >
                  Post feedback
                </Button>
              </div>
            </>
          ) : modal === "pr" ? (
            <>
              <p>
                Describe the implementation and decisions for reviewers. The
                task description stays on the workspace landing page.
              </p>
              <label>
                Implementation and decisions
                <textarea
                  aria-label="Implementation and decisions"
                  rows={5}
                  value={prBody}
                  onChange={(e) => setPrBody(e.target.value)}
                />
              </label>
              <Vector revision={state.pushed} />
              <div className="rm-modal-footer">
                <Button kind="ghost" onClick={() => setModal(null)}>
                  Cancel
                </Button>
                <Button
                  kind="primary"
                  disabled={!prBody.trim()}
                  onClick={() => {
                    if (role !== "owner" || state.pr) return;
                    update((s) =>
                      appendEvent(
                        { ...s, pr: true },
                        {
                          kind: "pr",
                          who: person,
                          body: "Submitted the PR for preview lifecycle cleanup.",
                        },
                      ),
                    );
                    setDescriptions((d) => ({
                      ...d,
                      [workspace]: prBody.trim(),
                    }));
                    setModal(null);
                    setNotice(
                      "PR submitted. Open Approvals to request configured reviews.",
                    );
                  }}
                >
                  Submit PR
                </Button>
              </div>
            </>
          ) : (
            <>
              <p>
                {modal === "request"
                  ? "Choose an organization-configured approval type and its eligible reviewers."
                  : "Choose one requested review type. Other review requests remain pending."}
              </p>
              <label>
                Approval type
                <select
                  aria-label="Approval type"
                  value={scope}
                  onChange={(e) => {
                    setScope(e.target.value);
                    setPeople([]);
                  }}
                >
                  {(modal === "request" ? scopes : pending).map((s) => (
                    <option key={s.id} value={s.id}>
                      {s.title} · {WORKBENCH_PHASES[s.phase].name}
                    </option>
                  ))}
                </select>
              </label>
              {modal === "request" ? (
                <fieldset>
                  <legend>Configured reviewers</legend>
                  {configuredReviewers(workspace, scope).map((name) => (
                    <label className="rm-choice" key={name}>
                      <input
                        type="checkbox"
                        checked={people.includes(name)}
                        onChange={() =>
                          setPeople((all) =>
                            all.includes(name)
                              ? all.filter((n) => n !== name)
                              : [...all, name],
                          )
                        }
                      />
                      <Avatar name={name} />
                      {name}
                    </label>
                  ))}
                  <small>
                    Each selected person receives a separate review request.
                  </small>
                </fieldset>
              ) : (
                <>
                  <div className="rm-inline-note">
                    This submission is for{" "}
                    <strong>{scopes.find((s) => s.id === scope)?.title}</strong>{" "}
                    only. It responds to your request for this type.
                  </div>
                  <fieldset>
                    <legend>Decision</legend>
                    <label className="rm-choice">
                      <input
                        type="radio"
                        name="decision"
                        value="approved"
                        checked={result === "approved"}
                        onChange={() => setResult("approved")}
                      />
                      <Check size={17} />
                      <span>
                        <strong>Approve</strong>
                        <small>
                          The reviewed changes meet this approval’s
                          requirements.
                        </small>
                      </span>
                    </label>
                    <label className="rm-choice">
                      <input
                        type="radio"
                        name="decision"
                        value="changes"
                        checked={result === "changes"}
                        onChange={() => setResult("changes")}
                      />
                      <MessageSquare size={17} />
                      <span>
                        <strong>Request changes</strong>
                        <small>
                          Explain what needs to change before approval.
                        </small>
                      </span>
                    </label>
                  </fieldset>
                  <label>
                    Review comment
                    {result === "changes" ? " (required)" : " (optional)"}
                    <textarea
                      aria-label="Review comment"
                      rows={3}
                      value={body}
                      onChange={(e) => setBody(e.target.value)}
                      placeholder="Share your reasoning…"
                    />
                  </label>
                </>
              )}
              <Vector
                revision={modal === "decision" ? target : state.pushed}
                label={
                  modal === "decision"
                    ? "Reviewing pushed revision"
                    : "Pushed revision"
                }
              />
              {modal === "decision" && (
                <details className="rm-test-event">
                  <summary>Demo: new push while reviewing</summary>
                  <Button kind="ghost" onClick={pushSample}>
                    Simulate docs push
                  </Button>
                </details>
              )}
              {modal === "decision" &&
                changedRepos(target, state.pushed).length > 0 && (
                  <div className="rm-inline-note" role="status">
                    A new push changed{" "}
                    {changedRepos(target, state.pushed).join(", ")}. Your draft
                    still reviews the captured revision.{" "}
                    <button
                      data-testid="review-mock-inspect-latest"
                      className="rm-link"
                      onClick={() => {
                        setTarget(structuredClone(state.pushed));
                        setModal(null);
                        setMode("last");
                        openView("docs", "requirements");
                      }}
                    >
                      Keep draft text and inspect latest changes
                    </button>
                    <small>
                      Your comment: {body || "(empty)"}. Reopen Review requests
                      after inspecting the latest revision.
                    </small>
                  </div>
                )}
              <div className="rm-modal-footer">
                <Button kind="ghost" onClick={() => setModal(null)}>
                  Cancel
                </Button>
                <Button
                  kind="primary"
                  disabled={
                    modal === "request"
                      ? !people.length
                      : !mayDecide(state, workspace, role, scope) ||
                        (result === "changes" && !body.trim())
                  }
                  onClick={() => {
                    if (modal === "request")
                      update((s) =>
                        requestReview(s, workspace, role, scope, people),
                      );
                    else
                      update((s) =>
                        submitDecision(s, workspace, role, {
                          scope,
                          result,
                          body: body.trim(),
                          revision: target,
                        }),
                      );
                    setModal(null);
                    setBody("");
                    setNotice(
                      modal === "request"
                        ? "Review requests recorded."
                        : "Review submitted for the captured pushed revision.",
                    );
                  }}
                >
                  {modal === "request"
                    ? "Send review request"
                    : "Submit review"}
                </Button>
              </div>
            </>
          )}
        </Modal>
      )}
    </div>
  );
}
