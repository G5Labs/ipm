import {
  SourceControlHistory,
  HistoryCommit,
  committedDate,
} from "./SourceControlHistory";
import {
  SourceControlFileTree,
  type SourceControlFileEntry,
} from "./SourceControlFileTree";
import { GitBranch, ArrowRight } from "lucide-react";
import "./source-control.css";
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  useMemo,
  memo,
  useId,
  type ReactNode,
} from "react";
import { useNavigate, useLocation } from "react-router-dom";
import { api, ApiError } from "../../lib/api";
import { useAuth } from "../../hooks/useAuth";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useWSChannel } from "../../hooks/useWebSocket";
import {
  policyFreeGateKind,
  useWorkspaceLiveGate,
  useWorkspaceReadAuthority,
} from "../../lib/workspaceLiveGate";
import {
  selectAll,
  reconcileSelection,
  sourceChildren,
  sourceApplyInput,
  sourcePushedSelection,
  UNPUSHED,
  type SourceMenu,
  type SourceSelection,
  type SourceSnapshot,
  type SourceListRefresh,
  type SourceCommit,
  type SourcePatch,
} from "../../lib/sourceControlSelection";
import {
  CHANGE_FILE_KINDS,
  CHANGE_FILE_LABELS,
  changeFileSummary,
  type ChangeFileKind,
} from "../../lib/changeFiles";
import { ContextualHelp } from "../ui/ContextualHelp";
import { groupSourcePatches } from "../../lib/sourceControlFiles";
import {
  SourceControlDiffBody,
  SourceControlDiffViewport,
} from "./SourceControlDiffBody";
import type { DiffHunk } from "../../lib/types";

interface AppliedSnapshot extends SourceSnapshot {
  types: readonly ChangeFileKind[];
  comparisonKey?: string;
  pushedHeads?: Record<string, string>;
  selection?: SourceSelection;
  commits?: Record<string, SourceCommit>;
  comparison?: "all" | "pushed" | "unpushed" | "net" | "patches";
}
interface BrowserState {
  authority: string;
  menu: SourceMenu | null;
  selection: SourceSelection;
  types: readonly ChangeFileKind[];
  applied: AppliedSnapshot | null;
  updated: boolean | "stale" | "refresh" | "advanced";
  changedRepos?: string[];
  removed?: { repoId: string; name: string; sha: string }[];
}
interface BrowserContext extends BrowserState {
  workspaceId: string;
  ready: boolean;
  rememberHistory: (repoId: string, commits: SourceCommit[]) => void;
  load: () => Promise<void>;
  apply: (
    scope?: "pushed",
    heads?: Record<string, string>,
    types?: readonly ChangeFileKind[],
  ) => Promise<void>;
  applyFromMenu: () => Promise<void>;
  consumeMenuEntry: () => boolean;
  enter: (types?: readonly ChangeFileKind[]) => Promise<void>;
  entryPending: boolean;
  setSelection: (selection: SourceSelection) => void;
  setTypes: (types: readonly ChangeFileKind[]) => void;
  retry: () => Promise<void>;
  refresh: () => Promise<void>;
  busy: boolean;
  error: string;
  staleError: boolean;
  selectionRequest: number;
  openSelection: () => void;
  getView: () => { scroll: number; width: number; selected: string | null };
  rememberView: (
    view: Partial<{ scroll: number; width: number; selected: string | null }>,
  ) => void;
}
const Context = createContext<BrowserContext | null>(null);
const endpoint = (id: string) => `/api/v1/workspaces/${id}/source-control`;
export function SourceControlProvider({
  workspaceId,
  children,
}: {
  workspaceId: string;
  children: ReactNode;
}) {
  const { user } = useAuth();
  const gate = useWorkspaceLiveGate();
  const readAuthority = useWorkspaceReadAuthority();
  const client = useQueryClient();
  useWSChannel(`workspace:${workspaceId}`);
  const checkpoint = useQuery({
    queryKey: ["source-control-checkpoint", workspaceId],
    queryFn: async ({ signal }) => {
      const result = await api.get<{ generation: number }>(
        `${endpoint(workspaceId)}/checkpoint`,
        {
          signal,
        },
      );
      const observed = client.getQueryData<{ generation: number }>([
        "source-control-checkpoint",
        workspaceId,
      ]);
      return {
        generation: Math.max(result.generation, observed?.generation ?? 0),
      };
    },
    staleTime: Infinity,
    refetchOnWindowFocus: false,
    refetchOnReconnect: false,
  });
  const live = !!gate && policyFreeGateKind(gate) === "live";
  const authorityReady =
    !!user?.id && !!gate && policyFreeGateKind(gate) !== "unknown";
  const authority = JSON.stringify([
    workspaceId,
    user?.id,
    gate?.owner,
    gate?.runtimeKey,
    live,
  ]);
  const activeAuthority = useRef(authority);
  // Revoke before render. Old promises cannot restore private results even if
  // a stop/start happens again before their responses arrive.
  const epoch = useRef(0);
  if (activeAuthority.current !== authority) {
    activeAuthority.current = authority;
    epoch.current++;
  }
  const storageKey = `g5:source-control:${workspaceId}:${user?.id ?? ""}`;
  const savedKey = useRef(storageKey);
  const saved = useRef<{
    selection?: SourceSelection;
    types?: readonly ChangeFileKind[];
    comparison?: string;
  } | null>(null);
  if (saved.current === null || savedKey.current !== storageKey) {
    savedKey.current = storageKey;
    try {
      saved.current = JSON.parse(sessionStorage.getItem(storageKey) ?? "{}");
    } catch {
      saved.current = {};
    }
  }
  const blank = (): BrowserState => ({
    authority,
    menu: null,
    selection: {},
    types: CHANGE_FILE_KINDS,
    applied: null,
    updated: false,
  });
  const [stored, setStored] = useState<BrowserState>(blank);
  const state = stored.authority === authority ? stored : blank();
  const latest = useRef(state);
  latest.current = state;
  const [pendingRuns, setPendingRuns] = useState<
    readonly { epoch: number; isCurrent: () => boolean }[]
  >([]);
  const busy = pendingRuns.some(
    (request) => request.epoch === epoch.current && request.isCurrent(),
  );
  const [selectionRequest, setSelectionRequest] = useState(0);
  const [failure, setFailure] = useState<{
    authority: string;
    text: string;
    stale?: boolean;
  }>({ authority, text: "" });
  const error = failure.authority === authority ? failure.text : "";
  const staleError = failure.authority === authority && failure.stale === true;
  const retryAction = useRef<(() => Promise<void>) | null>(null);
  const wanted = useRef(false);
  const generation = checkpoint.data?.generation;
  const observed = useRef<number | undefined>(undefined);
  const checkVersion = useRef(0);
  const applyRequest = useRef<{
    version: number;
    heads?: Record<string, string>;
  }>({ version: 0 });
  const menuEntry = useRef<string | null>(null);
  const view = useRef({
    scroll: 0,
    width: 238,
    selected: null as string | null,
  });
  const getView = useCallback(() => view.current, []);
  const rememberView = useCallback((patch: Partial<typeof view.current>) => {
    Object.assign(view.current, patch);
  }, []);
  const pending = useRef<Promise<void> | null>(null);
  // Reload restoration also remains pending while workspace authority settles.
  const pendingEntry = useRef<{ types?: readonly ChangeFileKind[] } | null>(
    null,
  );
  const resetAuthority = useRef(authority);
  const withdrawn = useRef<string | null>(null);
  const withdrawPrivate = useCallback(() => {
    if (withdrawn.current === activeAuthority.current) return;
    withdrawn.current = activeAuthority.current;
    epoch.current++;
    setStored((previous) => ({
      ...previous,
      menu: null,
      selection: {},
      types: CHANGE_FILE_KINDS,
      applied: null,
      updated: false,
      removed: undefined,
    }));
    setPendingRuns((requests) =>
      requests.filter((request) => request.epoch === epoch.current),
    );
    setFailure({
      authority: activeAuthority.current,
      text: "Workspace access changed. Reopen the workspace to check current permissions.",
    });
    client.removeQueries({
      predicate: ({ queryKey }) =>
        queryKey[1] === workspaceId &&
        ["workspace-file", "file-tree", "doc", "docs"].includes(
          String(queryKey[0]),
        ),
    });
    // Withdraw the current authority evidence, rather than retaining a known
    // stale owner/VM while the existing authoritative queries reconcile.
    void client.resetQueries({ queryKey: ["workspace", workspaceId] });
    void client.resetQueries({ queryKey: ["workspace-vm", workspaceId] });
  }, [client, workspaceId]);
  useEffect(
    () =>
      client.getQueryCache().subscribe((event) => {
        if (activeAuthority.current !== authority || event.type !== "updated")
          return;
        const query = event.query,
          key = query.queryKey;
        if (
          key[1] !== workspaceId ||
          query.getObserversCount() === 0 ||
          !(query.state.error instanceof ApiError) ||
          ![401, 403].includes(query.state.error.status)
        )
          return;
        if (
          key[0] === "source-control-checkpoint" ||
          (["workspace-file", "file-tree", "doc", "docs"].includes(
            String(key[0]),
          ) &&
            (key[key.length - 1] === readAuthority ||
              key[key.length - 1] === gate?.runtimeKey))
        )
          withdrawPrivate();
      }),
    [
      authority,
      client,
      workspaceId,
      readAuthority,
      gate?.runtimeKey,
      withdrawPrivate,
    ],
  );
  const run = useCallback(
    async (
      work: (valid: () => boolean) => Promise<void>,
      isCurrent: () => boolean = () => true,
      retainFailure = false,
    ) => {
      const started = epoch.current;
      const valid = () => epoch.current === started && isCurrent();
      const request = { epoch: started, isCurrent };
      setPendingRuns((requests) => [...requests, request]);
      if (!retainFailure)
        setFailure({ authority: activeAuthority.current, text: "" });
      try {
        await work(valid);
      } catch (e) {
        if (valid() && e instanceof ApiError && [401, 403].includes(e.status)) {
          withdrawPrivate();
          return;
        }
        if (valid()) {
          const stale =
            e instanceof ApiError &&
            e.details.kind === "source_control_stale_list";
          setFailure({
            authority: activeAuthority.current,
            text: e instanceof Error ? e.message : String(e),
            stale,
          });
          if (stale)
            setStored((previous) => ({ ...previous, updated: "stale" }));
        }
      } finally {
        setPendingRuns((requests) =>
          requests.filter((pending) => pending !== request),
        );
      }
    },
    [withdrawPrivate],
  );
  const load = useCallback(async () => {
    wanted.current = true;
    if (!authorityReady || latest.current.menu || pendingEntry.current) return;
    if (pending.current) return pending.current;
    retryAction.current = load;
    const version = applyRequest.current.version;
    const operation = run(
      async (valid) => {
        const menu = await api.get<SourceMenu>(`${endpoint(workspaceId)}/menu`);
        if (!valid() || (menu.private && !live)) return;
        withdrawn.current = null;
        observed.current = menu.generation ?? 0;
        setStored((previous) => {
          const previousApplied =
            previous.authority === authority ? previous.applied : null;
          const applied = previousApplied?.pushedHeads
            ? bindPushedSelection(previousApplied, menu)
            : previousApplied;
          return {
            authority,
            menu,
            selection: applied?.pushedHeads
              ? applied.selection!
              : applyRequest.current.heads
                ? sourcePushedSelection(menu, applyRequest.current.heads)
                : selectAll(menu),
            types:
              previous.authority === authority
                ? previous.types
                : CHANGE_FILE_KINDS,
            applied,
            updated: false,
          };
        });
      },
      () => applyRequest.current.version === version,
    );
    pending.current = operation;
    await operation;
    if (pending.current === operation) pending.current = null;
  }, [authority, authorityReady, live, run, workspaceId]);
  const acceptRefreshedMenu = useCallback(
    (
      result: {
        menu: SourceMenu;
        missingCommits?: Record<string, string[]>;
      },
      manual = false,
      automatic = false,
    ) => {
      const menu = result.menu;
      setStored((previous) => {
        if (previous.authority !== authority) return previous;
        const selection = reconcileSelection(
          previous.menu!,
          menu,
          previous.selection,
        );
        const removed = [...(previous.removed ?? [])];
        for (const repo of menu.repos) {
          const missing = result.missingCommits?.[repo.repoId] ?? [];
          for (const sha of missing) {
            if (
              previous.selection[repo.repoId]?.checked.includes(sha) &&
              !removed.some(
                (item) => item.repoId === repo.repoId && item.sha === sha,
              )
            )
              removed.push({ repoId: repo.repoId, name: repo.name, sha });
          }
          selection[repo.repoId].checked = selection[
            repo.repoId
          ].checked.filter((sha) => !missing.includes(sha));
          repo.historyCommits = [
            ...new Map(
              [
                ...(
                  previous.menu!.repos.find((old) => old.repoId === repo.repoId)
                    ?.historyCommits ?? []
                ).filter((commit) => !missing.includes(commit.sha)),
                ...(repo.historyCommits ?? []),
              ].map((commit) => [commit.sha, commit]),
            ).values(),
          ];
        }
        return {
          ...previous,
          menu,
          selection,
          removed,
          changedRepos: automatic
            ? [
                ...new Set([
                  ...menu.repos
                    .filter((repo) => {
                      const old = previous.menu!.repos.find(
                        (r) => r.repoId === repo.repoId,
                      );
                      return (
                        !old ||
                        old.pushed !== repo.pushed ||
                        old.base !== repo.base
                      );
                    })
                    .map((repo) => repo.name),
                  ...previous
                    .menu!.repos.filter(
                      (old) =>
                        !menu.repos.some((repo) => repo.repoId === old.repoId),
                    )
                    .map((repo) => repo.name),
                ]),
              ]
            : undefined,
          updated: automatic ? "advanced" : manual ? "refresh" : true,
        };
      });
    },
    [authority],
  );
  const check = useCallback(
    async (manual = false) => {
      const current = latest.current;
      if (!current.menu) return;
      retryAction.current = () => check(manual);
      const version = ++checkVersion.current;
      const comparisonVersion = applyRequest.current.version;
      await run(
        async (valid) => {
          const result = await api.post<{
            changed: boolean;
            menu?: SourceMenu;
            missingCommits?: Record<string, string[]>;
          }>(`${endpoint(workspaceId)}/check`, {
            // A prior partial read is not a complete baseline. An explicit Save
            // must retry that projection even when its Git refs have not moved.
            fingerprint:
              manual || current.menu!.repos.some((repo) => repo.error)
                ? ""
                : current.menu!.fingerprint,
            ...(manual
              ? {
                  selected: Object.fromEntries(
                    Object.entries(current.selection).map(([repo, chosen]) => [
                      repo,
                      chosen.checked.filter((sha) => sha !== UNPUSHED),
                    ]),
                  ),
                }
              : {}),
          });
          if (!valid() || version !== checkVersion.current) return;
          if (!result.changed || !result.menu) {
            if (manual) {
              setFailure({ authority, text: "" });
              setStored((previous) => ({ ...previous, updated: "refresh" }));
            }
            return;
          }
          const menu = result.menu;
          if (current.menu!.private && !menu.private) {
            withdrawPrivate();
            return;
          }
          withdrawn.current = null;
          acceptRefreshedMenu({ ...result, menu }, manual);
          if (manual) setFailure({ authority, text: "" });
        },
        () =>
          version === checkVersion.current &&
          (!manual || comparisonVersion === applyRequest.current.version),
        manual,
      );
    },
    [authority, run, workspaceId, withdrawPrivate, acceptRefreshedMenu],
  );
  useEffect(() => {
    if (resetAuthority.current === authority) return;
    resetAuthority.current = authority;
    pending.current = null;
    setPendingRuns((requests) =>
      requests.filter((request) => request.epoch === epoch.current),
    );
    setFailure({ authority, text: "" });
    retryAction.current = null;
    view.current = { scroll: 0, width: 238, selected: null };
    if (wanted.current) void load();
  }, [authority, load]);
  useEffect(() => {
    if (generation === undefined || !state.menu) return;
    const previous = observed.current ?? state.menu.generation ?? 0;
    observed.current = Math.max(previous, generation);
    if (generation > previous) void check();
  }, [generation, state.menu, check]);
  const apply = async (
    scope?: "pushed",
    heads?: Record<string, string>,
    requestedTypes?: readonly ChangeFileKind[],
  ) => {
    const version = applyRequest.current.version + 1;
    applyRequest.current = {
      version,
      heads: scope === "pushed" ? heads : undefined,
    };
    const isCurrent = () => version === applyRequest.current.version;
    retryAction.current = () => apply(scope, heads, requestedTypes);
    if (scope === "pushed" && heads) {
      await run(async (valid) => {
        const applied = await api.post<SourceSnapshot | SourceListRefresh>(
          `${endpoint(workspaceId)}/pushed`,
          { heads },
        );
        if (!valid() || (applied.menu?.private && !live)) return;
        if ("refreshed" in applied) {
          pendingEntry.current = null;
          acceptRefreshedMenu(applied, false, true);
          return;
        }
        view.current.scroll = 0;
        pendingEntry.current = null;
        setStored((previous) => {
          const current = previous.authority === authority ? previous : blank();
          const represented = applied.menu ?? current.menu;
          const representedHeads = represented
            ? Object.fromEntries(
                represented.repos.map((repo) => [repo.repoId, repo.pushed]),
              )
            : heads;
          const unchangedSelection =
            current.menu &&
            selectionKey(current.selection) ===
              selectionKey(sourcePushedSelection(current.menu, heads));
          const snapshot: AppliedSnapshot = {
            ...applied,
            comparison: "pushed",
            types: requestedTypes ?? CHANGE_FILE_KINDS,
            pushedHeads: representedHeads,
          };
          return {
            ...current,
            menu: represented,
            selection:
              represented && applied.menu && unchangedSelection
                ? sourcePushedSelection(represented, representedHeads)
                : current.selection,
            applied: represented
              ? bindPushedSelection(snapshot, represented)
              : snapshot,
            updated: false,
          };
        });
      }, isCurrent);
      return;
    }
    await load();
    if (!isCurrent()) return;
    // Read the current ref after React has committed a newly loaded menu.
    const current = latest.current;
    if (!current.menu) return;
    let selection = current.selection;
    if (scope === "pushed") {
      selection = Object.fromEntries(
        current.menu.repos.map((r) => [
          r.repoId,
          { checked: r.commits.map((c) => c.sha), followAll: false },
        ]),
      );
      // Initialize the requested scope before the read, so edits during that
      // read stay pending instead of being replaced when its response arrives.
      setStored((previous) =>
        previous.authority === authority &&
        previous.menu?.fingerprint === current.menu?.fingerprint
          ? { ...previous, selection }
          : previous,
      );
    }
    const input = sourceApplyInput(current.menu, selection);
    const types = requestedTypes ?? (scope ? CHANGE_FILE_KINDS : current.types);
    const key = comparisonKey(current.menu, selection);
    if (
      !input.repos.length &&
      !scope &&
      current.updated !== "stale" &&
      canApplyTypes(current)
    ) {
      await run(async (valid) => {
        const checked = await api.post<{ changed: boolean; menu?: SourceMenu }>(
          `${endpoint(workspaceId)}/check`,
          { fingerprint: current.menu!.fingerprint },
        );
        if (!valid()) return;
        if (checked.changed && checked.menu) {
          acceptRefreshedMenu({ menu: checked.menu }, false, true);
          return;
        }
        pendingEntry.current = null;
        setStored((previous) => ({
          ...previous,
          applied: { ...current.applied!, types },
          removed: undefined,
          updated: false,
        }));
      }, isCurrent);
      return;
    }
    if (!input.repos.length) {
      if (
        scope &&
        !current.menu.repos.some((repo) => repo.error || repo.truncated)
      )
        setStored((previous) => ({
          ...previous,
          applied: {
            patches: [],
            fileCount: 0,
            private: false,
            comparison: "pushed",
            types: CHANGE_FILE_KINDS,
            selection,
            comparisonKey: key,
          },
          removed: undefined,
          updated: previous.menu?.fingerprint !== current.menu?.fingerprint,
        }));
      return;
    }
    await run(async (valid) => {
      const applied = await api.post<SourceSnapshot | SourceListRefresh>(
        `${endpoint(workspaceId)}/apply`,
        input,
      );
      if (!valid()) return;
      if ("refreshed" in applied) {
        if (applied.menu.private && !live) return;
        if (current.menu!.private && !applied.menu.private) {
          withdrawPrivate();
          return;
        }
        pendingEntry.current = null;
        acceptRefreshedMenu(applied, false, true);
        return;
      }
      if (applied.private && !live) return;
      pendingEntry.current = null;
      if (!canApplyTypes(current)) {
        view.current.scroll = 0;
        view.current.selected = null;
      }
      setStored((previous) => ({
        ...previous,
        menu:
          previous.menu && applied.selectedCommits
            ? {
                ...previous.menu,
                repos: previous.menu.repos.map((repo) => ({
                  ...repo,
                  historyCommits: [
                    ...new Map(
                      [
                        ...(repo.historyCommits ?? []),
                        ...Object.entries(applied.selectedCommits!)
                          .filter(([id]) => id.startsWith(`${repo.repoId}:`))
                          .map(([, commit]) => commit),
                      ].map((commit) => [commit.sha, commit]),
                    ).values(),
                  ],
                })),
              }
            : previous.menu,
        applied: {
          ...applied,
          types,
          comparisonKey: key,
          selection,
          comparison: selectionComparison(current.menu!, selection),
          commits: {
            ...Object.fromEntries(
              current.menu!.repos.flatMap((repo) =>
                [
                  ...repo.commits,
                  ...(repo.historyCommits ?? []),
                  ...(repo.comparison?.mergeBaseCommit
                    ? [repo.comparison.mergeBaseCommit]
                    : []),
                  ...(repo.comparison?.workspaceBase
                    ? [repo.comparison.workspaceBase]
                    : []),
                ].map((commit) => [`${repo.repoId}:${commit.sha}`, commit]),
              ),
            ),
            ...applied.selectedCommits,
          },
        },
        removed:
          previous.menu?.fingerprint === current.menu?.fingerprint
            ? undefined
            : previous.removed,
        updated: previous.menu?.fingerprint !== current.menu?.fingerprint,
      }));
    }, isCurrent);
  };
  useEffect(() => {
    if (!state.applied?.selection) return;
    const value = {
      selection: state.applied.selection,
      types: state.applied.types,
      comparison: state.applied.comparison,
    };
    saved.current = value;
    try {
      sessionStorage.setItem(storageKey, JSON.stringify(value));
    } catch {
      /* Storage may be disabled. */
    }
  }, [storageKey, state.applied]);
  const enter = async (types?: readonly ChangeFileKind[]) => {
    pendingEntry.current = types ? { types } : (pendingEntry.current ?? {});
    types ??= pendingEntry.current.types;
    if (!authorityReady) return;
    const returningView =
      !types && latest.current.applied ? { ...view.current } : null;
    const version = ++applyRequest.current.version;
    let captured: SourceMenu | null = null;
    await run(
      async (valid) => {
        const menu = await api.get<SourceMenu>(`${endpoint(workspaceId)}/menu`);
        if (!valid() || (menu.private && !live)) return;
        captured = menu;
        const selectedTypes =
          types ?? saved.current?.types ?? CHANGE_FILE_KINDS;
        const pushed = Object.fromEntries(
          menu.repos.map((repo) => [repo.repoId, repo.pushed]),
        );
        const selection =
          types || saved.current?.comparison === "pushed"
            ? sourcePushedSelection(menu, pushed)
            : (saved.current?.selection ??
              (latest.current.menu
                ? reconcileSelection(
                    latest.current.menu,
                    menu,
                    latest.current.selection,
                  )
                : selectAll(menu)));
        const next = {
          ...latest.current,
          authority,
          menu,
          selection,
          types: selectedTypes,
          updated: false,
        };
        latest.current = next;
        setStored(next);
      },
      () => applyRequest.current.version === version,
    );
    if (!captured || applyRequest.current.version !== version) return;
    const menu: SourceMenu = captured;
    if (!types && !saved.current?.selection && !latest.current.applied) {
      pendingEntry.current = null;
      return;
    }
    if (types || saved.current?.comparison === "pushed") {
      await apply(
        "pushed",
        Object.fromEntries(
          menu.repos.map((repo) => [repo.repoId, repo.pushed]),
        ),
        types ?? saved.current?.types ?? CHANGE_FILE_KINDS,
      );
    } else {
      await apply();
    }
    if (returningView) view.current = returningView;
  };
  return (
    <Context.Provider
      value={{
        ...state,
        workspaceId,
        ready: authorityReady,
        rememberHistory: (repoId, commits) =>
          setStored((previous) => ({
            ...previous,
            menu: previous.menu
              ? {
                  ...previous.menu,
                  repos: previous.menu.repos.map((repo) =>
                    repo.repoId !== repoId
                      ? repo
                      : {
                          ...repo,
                          historyCommits: [
                            ...new Map(
                              [...(repo.historyCommits ?? []), ...commits].map(
                                (c) => [c.sha, c],
                              ),
                            ).values(),
                          ],
                        },
                  ),
                }
              : null,
          })),
        load,
        retry: () => retryAction.current?.() ?? Promise.resolve(),
        refresh: () => (latest.current.menu ? check(true) : load()),
        apply,
        applyFromMenu: () => {
          menuEntry.current = authority;
          return apply();
        },
        consumeMenuEntry: () => {
          const ownsEntry = menuEntry.current === authority;
          menuEntry.current = null;
          return ownsEntry;
        },
        enter,
        entryPending: pendingEntry.current !== null,
        setSelection: (selection) =>
          setStored((previous) => ({ ...previous, selection })),
        setTypes: (types) => setStored((previous) => ({ ...previous, types })),
        busy,
        error,
        staleError,
        selectionRequest,
        openSelection: () => setSelectionRequest((request) => request + 1),
        getView,
        rememberView,
      }}
    >
      {children}
    </Context.Provider>
  );
}
function useBrowser() {
  return useContext(Context);
}
function SourceControlNotice({ reader = false }: { reader?: boolean }) {
  const browser = useBrowser();
  if (!browser) return null;
  return (
    <>
      {browser.error && (
        <p
          role="alert"
          className={reader ? undefined : "source-control-notice"}
          style={
            reader && browser.staleError
              ? { color: "var(--g5-warn)" }
              : undefined
          }
        >
          {browser.staleError ? (
            <>
              The workspace changed.{" "}
              <button
                data-testid={
                  reader
                    ? "source-control-reader-refresh"
                    : "source-control-refresh"
                }
                className="source-control-link"
                disabled={browser.busy}
                onClick={() => void browser.refresh()}
              >
                {browser.busy ? "Refreshing list…" : "Refresh list"}
              </button>
              , then click Show.
              {browser.applied &&
                " The diff below still shows the previous comparison."}
            </>
          ) : (
            <>
              {browser.error}
              {!reader && (
                <>
                  {" "}
                  <button
                    data-testid="source-control-retry"
                    className="source-control-link"
                    onClick={() => void browser.retry()}
                  >
                    Retry
                  </button>
                </>
              )}
            </>
          )}
        </p>
      )}
      {browser.updated === "advanced" && (
        <p
          role="status"
          className={reader ? undefined : "source-control-notice"}
        >
          {browser.changedRepos?.length
            ? browser.changedRepos.join(", ")
            : "The workspace history"}{" "}
          {browser.changedRepos && browser.changedRepos.length > 1
            ? "have"
            : "has"}{" "}
          changed since this list was loaded. The list has been updated. Review
          your choices and click Show.
        </p>
      )}
      {browser.removed?.map(({ repoId, name, sha }) => (
        <p
          key={`${repoId}:${sha}`}
          role="status"
          className={reader ? undefined : "source-control-notice"}
          style={{ color: "var(--g5-warn)" }}
        >
          Removed commit <code>{sha.slice(0, 7)}</code> in {name} from selection
          because it is no longer available. Click Show to apply the updated
          selection.
        </p>
      ))}
    </>
  );
}
export function useSourceControlMenuRequest() {
  return useBrowser()?.selectionRequest ?? 0;
}
function CheckBox({
  mixed,
  ...props
}: React.InputHTMLAttributes<HTMLInputElement> & { mixed?: boolean }) {
  const ref = useRef<HTMLInputElement>(null);
  useLayoutEffect(() => {
    if (ref.current) ref.current.indeterminate = !!mixed;
  }, [mixed]);
  return (
    <input
      {...props}
      type="checkbox"
      ref={ref}
      aria-checked={mixed ? "mixed" : props.checked}
      className="source-control-checkbox"
    />
  );
}
function selectionKey(selection: SourceSelection) {
  return JSON.stringify(
    Object.entries(selection)
      .filter(([, repo]) => repo.checked.length)
      .map(([id, repo]) => [id, [...repo.checked].sort()])
      .sort(([a], [b]) => String(a).localeCompare(String(b))),
  );
}
function comparisonKey(menu: SourceMenu, selection: SourceSelection) {
  return JSON.stringify([menu.fingerprint, selectionKey(selection)]);
}
function bindPushedSelection(
  snapshot: AppliedSnapshot,
  menu: SourceMenu,
): AppliedSnapshot {
  const selection = sourcePushedSelection(menu, snapshot.pushedHeads!);
  return {
    ...snapshot,
    selection,
    comparisonKey: comparisonKey(menu, selection),
  };
}
function canApplyTypes(browser: BrowserState) {
  return !!(
    browser.menu &&
    browser.applied &&
    browser.applied.comparisonKey ===
      comparisonKey(browser.menu, browser.selection) &&
    browser.types.join() !== browser.applied.types.join()
  );
}
function canApplyEmptyRange(browser: BrowserState) {
  return (
    !!browser.menu?.repos.some(
      (repo) =>
        !repo.unpushedOnly &&
        !repo.error &&
        repo.base === repo.pushed &&
        browser.selection[repo.repoId]?.followAll,
    ) &&
    (!browser.applied ||
      browser.applied.comparisonKey !==
        comparisonKey(browser.menu!, browser.selection))
  );
}
function selectionPending(browser: BrowserContext) {
  return !!(
    browser.applied &&
    ((browser.applied.selection &&
      selectionKey(browser.selection) !==
        selectionKey(browser.applied.selection)) ||
      browser.types.join() !== browser.applied.types.join())
  );
}
function selectionComparison(
  menu: SourceMenu,
  selection: SourceSelection,
): AppliedSnapshot["comparison"] {
  const modes = new Set(
    menu.repos
      .filter((repo) => selection[repo.repoId]?.checked.length)
      .map((repo) => {
        const checked = selection[repo.repoId].checked;
        const commits = checked.filter((id) => id !== UNPUSHED);
        const unpushed = checked.includes(UNPUSHED);
        if (repo.unpushedOnly && !commits.length) return "unpushed";
        const all =
          !repo.truncated &&
          commits.length === repo.commits.length &&
          repo.commits.every((commit) => commits.includes(commit.sha));
        return all
          ? unpushed
            ? "all"
            : "pushed"
          : !commits.length && unpushed
            ? "unpushed"
            : "patches";
      }),
  );
  return modes.size === 1
    ? [...modes][0]
    : modes.has("patches")
      ? "patches"
      : "net";
}
export function SourceControlMenu() {
  const browser = useBrowser();
  const typePrefix = useId();
  const navigate = useNavigate();
  const location = useLocation();
  const load = browser?.load;
  const menu = browser?.menu;
  useEffect(() => {
    if (!menu) void load?.();
  }, [load, menu]);
  if (!browser) return null;
  const { selection, setSelection } = browser;
  const any = Object.values(selection).some((s) => s.checked.length);
  const available =
    menu?.repos.flatMap((repo) =>
      repo.error ? [] : sourceChildren(repo).map((id) => [repo.repoId, id]),
    ) ?? [];
  const all =
    CHANGE_FILE_KINDS.every((kind) => browser.types.includes(kind)) &&
    available.every(([repo, id]) => selection[repo]?.checked.includes(id)) &&
    Object.entries(selection).every(([repo, chosen]) =>
      chosen.checked.every((id) =>
        available.some(([r, c]) => r === repo && c === id),
      ),
    );
  const selectedCommits = Object.values(selection).reduce(
    (total, repo) =>
      total + repo.checked.filter((id) => id !== UNPUSHED).length,
    0,
  );
  const selectedLive = Object.values(selection).some((repo) =>
    repo.checked.includes(UNPUSHED),
  );
  return (
    <div className="source-control-menu" data-testid="source-control-menu">
      <div className="source-control-tools">
        <label className="source-control-select-all">
          <CheckBox
            data-testid="source-control-all"
            aria-label="All changes"
            checked={all}
            mixed={!all && (any || browser.types.length > 0)}
            disabled={!menu || browser.busy}
            onChange={(e) => {
              if (!menu) return;
              const checked = e.target.checked;
              setSelection(
                checked
                  ? selectAll(menu)
                  : Object.fromEntries(
                      menu.repos.map((repo) => [
                        repo.repoId,
                        { checked: [], followAll: false },
                      ]),
                    ),
              );
              browser.setTypes(checked ? CHANGE_FILE_KINDS : []);
            }}
          />
          All changes
        </label>
        <button
          data-testid="source-control-show-changes"
          className="source-control-button source-control-primary"
          disabled={
            (!any && !canApplyTypes(browser) && !canApplyEmptyRange(browser)) ||
            browser.busy
          }
          onClick={() => {
            void browser.applyFromMenu();
            navigate(
              `${location.pathname.split("/").slice(0, 3).join("/")}?view=source-control`,
              { state: { sourceControlFromMenu: true } },
            );
          }}
        >
          Show
        </button>
        <div className="source-control-menu-help">
          <ContextualHelp label="About Source Control selection">
            <UnpushedHelp />
            <p>
              Checkbox edits stay pending until Show. Not yet pushed includes
              local work present when Show runs.
            </p>
          </ContextualHelp>
        </div>
      </div>
      <fieldset className="source-control-types">
        <legend>File types</legend>
        {CHANGE_FILE_KINDS.map((kind) => (
          <label key={kind} htmlFor={`${typePrefix}-${kind}`}>
            <CheckBox
              id={`${typePrefix}-${kind}`}
              aria-label={CHANGE_FILE_LABELS[kind]}
              checked={browser.types.includes(kind)}
              onChange={(event) =>
                browser.setTypes(
                  CHANGE_FILE_KINDS.filter((type) =>
                    type === kind
                      ? event.target.checked
                      : browser.types.includes(type),
                  ),
                )
              }
            />
            {CHANGE_FILE_LABELS[kind]}
          </label>
        ))}
      </fieldset>
      <div className="source-control-footer">
        <p
          className={
            browser.updated || selectionPending(browser)
              ? "source-control-pending"
              : "source-control-footer-summary"
          }
          role="status"
        >
          {browser.busy && browser.staleError
            ? "Refreshing checklist…"
            : browser.updated === true ||
                browser.updated === "refresh" ||
                browser.updated === "advanced"
              ? "Checklist updated · Click Show"
              : selectionPending(browser)
                ? "Selection changed · Click Show"
                : `${selectedCommits} commits${selectedLive ? " + not yet pushed" : ""} selected`}
        </p>
      </div>
      <div className="source-control-menu-scroll">
        {!menu && (
          <p role="status" className="source-control-notice">
            Loading repository changes…
          </p>
        )}
        <SourceControlNotice />
        {browser.updated === true && (
          <p role="status" className="source-control-notice">
            The checklist has been updated after Save changes.
          </p>
        )}
        {menu?.repos.map((repo) => {
          const children = sourceChildren(repo);
          const unpushedAvailable = children.includes(UNPUSHED);
          const chosen = selection[repo.repoId]?.checked ?? [];
          const all =
            children.length > 0 && children.every((id) => chosen.includes(id));
          const toggle = (id: string, checked: boolean) => {
            const next = checked
              ? [...new Set([...chosen, id])]
              : chosen.filter((c) => c !== id);
            setSelection({
              ...selection,
              [repo.repoId]: {
                checked: next,
                followAll:
                  children.length > 0 &&
                  children.every((c) => next.includes(c)) &&
                  next.every((c) => children.includes(c)),
              },
            });
          };
          return (
            <section
              key={repo.repoId}
              className="source-control-repo"
              aria-label={repo.name}
            >
              <label
                htmlFor={`source-repo-${repo.repoId}`}
                className="source-control-repo-heading"
              >
                <CheckBox
                  id={`source-repo-${repo.repoId}`}
                  aria-label={`All changes in ${repo.name}`}
                  checked={all}
                  mixed={!all && chosen.length > 0}
                  disabled={!!repo.error || !children.length}
                  onChange={(e) =>
                    setSelection({
                      ...selection,
                      [repo.repoId]: {
                        checked: e.target.checked ? children : [],
                        followAll: e.target.checked,
                      },
                    })
                  }
                />
                <GitBranch size={15} aria-hidden="true" />
                <strong>{repo.name}</strong>
                {!repo.unpushedOnly && (
                  <code>{repo.commits.length} commits</code>
                )}
              </label>
              {repo.error && (
                <p role="alert" className="source-control-notice">
                  {repo.error}
                </p>
              )}
              {repo.truncated && (
                <p role="note" className="source-control-notice">
                  Showing the first 250 commits. This is a partial history.
                </p>
              )}
              <label
                htmlFor={`source-unpushed-${repo.repoId}`}
                className="source-control-option"
                data-disabled={!unpushedAvailable || undefined}
              >
                <CheckBox
                  id={`source-unpushed-${repo.repoId}`}
                  aria-label={`Not yet pushed in ${repo.name}`}
                  checked={unpushedAvailable && chosen.includes(UNPUSHED)}
                  disabled={!unpushedAvailable}
                  aria-description={
                    !unpushedAvailable
                      ? repo.live
                        ? (repo.live.unavailableReason ??
                          "No not yet pushed changes.")
                        : "Available only to the workspace owner while the VM is running."
                      : undefined
                  }
                  onChange={(e) => toggle(UNPUSHED, e.target.checked)}
                />
                <span>
                  <strong className="source-control-subject">
                    Not yet pushed
                  </strong>
                  {repo.live?.unavailableReason && (
                    <span className="source-control-meta">
                      {repo.live.unavailableReason}
                    </span>
                  )}
                </span>
              </label>
              {repo.commits.map((commit) => (
                <HistoryCommit
                  key={commit.sha}
                  repo={repo}
                  commit={commit}
                  checked={chosen.includes(commit.sha)}
                  onToggle={toggle}
                />
              ))}
              <SourceControlHistory
                key={`${repo.pushed}:${repo.base}`}
                workspaceId={browser.workspaceId}
                repo={repo}
                chosen={chosen}
                onToggle={toggle}
                remember={(commits) =>
                  browser.rememberHistory(repo.repoId, commits)
                }
              />
              {chosen
                .filter(
                  (id) =>
                    !browser.busy &&
                    !children.includes(id) &&
                    !repo.historyCommits?.some((c) => c.sha === id) &&
                    repo.comparison?.mergeBaseCommit.sha !== id &&
                    repo.comparison?.workspaceBase?.sha !== id,
                )
                .map((id) => (
                  <label key={id} className="source-control-option">
                    <CheckBox checked onChange={() => toggle(id, false)} />
                    <span role="alert">
                      Selected revision {id.slice(0, 7)} is unavailable. Clear
                      it or retry.
                    </span>
                  </label>
                ))}
              {!children.length && !repo.error && (
                <p className="source-control-notice">No changes</p>
              )}
            </section>
          );
        })}
      </div>
    </div>
  );
}
export function SourceControlDiffWorkspace() {
  const browser = useBrowser();
  const location = useLocation();
  const navigate = useNavigate();
  const pane = useRef<HTMLDivElement>(null);
  const shell = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(browser?.getView().width ?? 238);
  const [maximumWidth, setMaximumWidth] = useState(360);
  const [selected, setSelected] = useState(browser?.getView().selected ?? null);
  const entered = useRef<string | null>(null);
  const handledRequest = useRef<string | null>(null);
  useLayoutEffect(() => {
    if (!browser?.ready) return;
    const request = location.state?.sourceControlSelection;
    const fromMenu = location.state?.sourceControlFromMenu === true;
    if ((request || fromMenu) && handledRequest.current !== location.key) {
      handledRequest.current = location.key;
      entered.current = browser.authority;
      const ownsMenuEntry = browser.consumeMenuEntry();
      if (request)
        void browser.enter(request === "code" ? ["code"] : CHANGE_FILE_KINDS);
      else if (!ownsMenuEntry) void browser.enter();
      const state = { ...location.state };
      delete state.sourceControlSelection;
      delete state.sourceControlFromMenu;
      navigate(`${location.pathname}${location.search}`, {
        replace: true,
        state,
      });
    } else if (entered.current !== browser.authority) {
      const firstEntry = entered.current === null;
      entered.current = browser.authority;
      if (firstEntry || browser.entryPending) void browser.enter();
      else void browser.load();
    }
  }, [
    browser,
    location.key,
    location.pathname,
    location.search,
    location.state,
    navigate,
  ]);
  const snapshot = browser?.applied;
  const mode = snapshot?.comparison ?? "patches";
  const selectedCount =
    snapshot?.selection &&
    Object.values(snapshot.selection).reduce(
      (count, repo) => count + repo.checked.length,
      0,
    );
  const title =
    mode === "all"
      ? "All changes"
      : mode === "pushed"
        ? "Pushed changes"
        : mode === "unpushed"
          ? "Not yet pushed"
          : selectedCount
            ? `${selectedCount} selected change${selectedCount === 1 ? "" : "s"}`
            : "Selected changes";
  const { groups: comparisonGroups, counts } = useMemo(
    () => groupSourcePatches(snapshot?.patches ?? []),
    [snapshot?.patches],
  );
  const types = snapshot?.types ?? CHANGE_FILE_KINDS;
  const { groups, files, additions, deletions } = useMemo(() => {
    const groups = new Map<string, SourcePatch[]>();
    // A memoized filename tree must not retain whole obsolete diff snapshots.
    const files = new Map<string, SourceControlFileEntry>();
    let additions = 0,
      deletions = 0;
    for (const [key, patches] of comparisonGroups) {
      if (!types.includes(patches[0].kind)) continue;
      groups.set(key, patches);
      files.set(key, {
        repoId: patches[0].repoId,
        repoName: patches[0].repoName,
        path: patches[0].fileKey ?? patches[0].file.path,
        change: patches[0].file.change,
        patchCount: patches.length,
      });
      for (const patch of patches) {
        additions += patch.file.diff.additions;
        deletions += patch.file.diff.deletions;
      }
    }
    return { groups, files, additions, deletions };
  }, [comparisonGroups, types]);
  const getView = browser?.getView;
  const rememberView = browser?.rememberView;
  if (selected && (!groups.has(selected) || getView?.().selected !== selected))
    setSelected(null);
  useLayoutEffect(() => {
    if (!getView) return;
    if (pane.current) {
      pane.current.scrollTop = getView().scroll;
      rememberView?.({ scroll: pane.current.scrollTop });
    }
    if (getView().selected && !groups.has(getView().selected!))
      rememberView?.({ selected: null });
  }, [snapshot, groups, getView, rememberView]);
  const selectFile = useCallback(
    (key: string) => {
      setSelected(key);
      rememberView?.({ selected: key });
      document
        .getElementById(`source-file-${encodeURIComponent(key)}`)
        ?.scrollIntoView({ block: "start" });
    },
    [rememberView],
  );
  if (!browser) return <p>Open Source Control to select repository changes.</p>;
  const resize = (value: number) => {
    const maximum = Math.max(
      150,
      Math.min(360, (shell.current?.clientWidth ?? 900) - 300),
    );
    setMaximumWidth(maximum);
    const next = Math.max(150, Math.min(value, maximum));
    setWidth(next);
    browser.rememberView({ width: next });
  };
  return (
    <div
      className="source-control-workspace flex h-full min-h-0 flex-col bg-[var(--g5-bg0)]"
      data-testid="source-control-diff-workspace"
    >
      <div className="source-control-heading">
        <div className="source-control-title-row">
          <h1>{snapshot ? title : "Changes"}</h1>
          {snapshot && (
            <span className="source-control-count">
              {groups.size} files changed
            </span>
          )}
          {snapshot && (
            <span className="source-control-totals">
              {mode === "patches" && (
                <span className="source-control-meta">Patch totals</span>
              )}
              <span className="source-control-additions">+{additions}</span>
              <span className="source-control-deletions">−{deletions}</span>
            </span>
          )}
          {snapshot && <FileTypeHelp />}
        </div>
        {!snapshot && (
          <p>Choose repository changes in Source Control, then click Show.</p>
        )}
        {browser.busy && !browser.staleError && (
          <span role="status"> Loading…</span>
        )}
        <SourceControlNotice reader />
      </div>
      {snapshot && (
        <div className="source-control-selection-banner" role="status">
          <span
            className={
              browser.updated || selectionPending(browser)
                ? "source-control-pending"
                : undefined
            }
          >
            {browser.updated
              ? browser.updated === true
                ? "Checklist updated. This diff shows the previously applied selection."
                : "Previous comparison · Click Show to apply the current checklist."
              : selectionPending(browser)
                ? "Selection changed. Showing the previous selection until you click Show."
                : mode === "patches"
                  ? "Each selected commit compares with its own parent."
                  : "Net comparison · One entry per changed file"}
          </span>
          <button
            data-testid="source-control-choose-changes"
            className="source-control-link"
            onClick={browser.openSelection}
          >
            Choose changes
          </button>
        </div>
      )}
      {snapshot && (
        <div className="flex min-h-0 flex-1" ref={shell}>
          <nav
            aria-label="Changed files"
            className="source-control-changed-tree shrink-0 overflow-auto text-xs"
            style={{ width }}
          >
            <div className="source-control-tree-heading source-control-type-summary">
              <p role="status" data-testid="source-control-type-summary">
                {changeFileSummary(counts, types)}
                {types.length > 0 ? " — " : ""}
                {groups.size} matching of {snapshot.fileCount} changed files
              </p>
            </div>
            <SourceControlFileTree
              files={files}
              selected={selected}
              onSelect={selectFile}
            />
          </nav>
          <div
            role="separator"
            aria-label="Resize changed files"
            aria-orientation="vertical"
            aria-valuemin={150}
            aria-valuemax={maximumWidth}
            aria-valuenow={width}
            tabIndex={0}
            className="source-control-splitter shrink-0 touch-none cursor-col-resize focus:bg-[var(--g5-info)]"
            onKeyDown={(e) => {
              if (["ArrowLeft", "ArrowRight", "Home", "End"].includes(e.key)) {
                e.preventDefault();
                resize(
                  e.key === "Home"
                    ? 150
                    : e.key === "End"
                      ? 10000
                      : width + (e.key === "ArrowRight" ? 10 : -10),
                );
              }
            }}
            onPointerDown={(e) => {
              e.currentTarget.setPointerCapture(e.pointerId);
            }}
            onPointerMove={(e) => {
              if (e.currentTarget.hasPointerCapture(e.pointerId))
                resize(
                  e.clientX -
                    (shell.current?.getBoundingClientRect().left ?? 0),
                );
            }}
            onPointerUp={(e) =>
              e.currentTarget.releasePointerCapture(e.pointerId)
            }
          />
          <div
            ref={pane}
            className="source-control-diff-stream min-w-0 flex-1 overflow-auto"
            data-testid="source-control-diff-stream"
            onScroll={(e) => {
              browser.rememberView({ scroll: e.currentTarget.scrollTop });
            }}
          >
            <SourceControlDiffViewport root={pane}>
              <SourceControlFileCards
                groups={groups}
                commits={snapshot.commits}
                mode={mode}
              />
            </SourceControlDiffViewport>
            {!groups.size && (
              <p className="text-sm">
                {comparisonGroups.size
                  ? "No changed files match the selected file types."
                  : snapshot.emptyOwnChanges
                    ? "No changes of its own"
                    : "No net file changes in the selected comparison."}
              </p>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

/** Pending menu/context updates never recompute the selected patch bodies. */
const SourceControlFileCards = memo(function SourceControlFileCards({
  groups,
  commits,
  mode,
}: {
  groups: Map<string, SourcePatch[]>;
  commits: AppliedSnapshot["commits"];
  mode: NonNullable<AppliedSnapshot["comparison"]>;
}) {
  const navigate = useNavigate();
  const currentNavigate = useRef(navigate);
  useLayoutEffect(() => {
    currentNavigate.current = navigate;
  }, [navigate]);
  const onViewFile = useCallback(
    (repoId: string, path: string) =>
      currentNavigate.current(
        `file/${path.split("/").map(encodeURIComponent).join("/")}?branch=${encodeURIComponent(repoId)}`,
        { relative: "path" },
      ),
    [],
  );
  return (
    <>
      {[...groups].map(([fileKey, patches]) => (
        <SourceControlFileCard
          key={fileKey}
          fileKey={fileKey}
          patches={patches}
          commits={commits}
          mode={mode}
          onViewFile={onViewFile}
        />
      ))}
    </>
  );
});

interface FileCardProps {
  fileKey: string;
  patches: SourcePatch[];
  commits: AppliedSnapshot["commits"];
  mode: NonNullable<AppliedSnapshot["comparison"]>;
  onViewFile: (repoId: string, path: string) => void;
}
const SourceControlFileCard = memo(
  function SourceControlFileCard({
    fileKey,
    patches,
    commits,
    mode,
    onViewFile,
  }: FileCardProps) {
    return (
      <article
        id={`source-file-${encodeURIComponent(fileKey)}`}
        className="source-control-diff-card"
      >
        <div className="source-control-file-heading">
          <div className="source-control-file-path">
            <span>{patches[0].fileKey ?? patches[0].file.path}</span>
            <p className="source-control-meta">
              {patches[0].repoName} ·{" "}
              {patches.length > 1
                ? `${patches.length} selected patches`
                : patches[0].file.change}
            </p>
          </div>
          <span className="source-control-totals">
            {patches[0].file.diff.status === "binary" ? (
              "Binary"
            ) : (
              <>
                <span className="source-control-additions">
                  +
                  {patches.reduce(
                    (total, patch) => total + patch.file.diff.additions,
                    0,
                  )}
                </span>
                <span className="source-control-deletions">
                  −
                  {patches.reduce(
                    (total, patch) => total + patch.file.diff.deletions,
                    0,
                  )}
                </span>
              </>
            )}
          </span>
          <details className="source-control-file-menu">
            <summary
              aria-label={`Actions for ${patches[0].fileKey ?? patches[0].file.path}`}
            >
              …
            </summary>
            <div className="source-control-file-actions">
              <button
                data-testid="source-control-view-file"
                onClick={() =>
                  onViewFile(
                    patches[0].repoId,
                    patches[0].fileKey ?? patches[0].file.path,
                  )
                }
              >
                View file
              </button>
            </div>
          </details>
        </div>
        <SourceControlFilePatches
          patches={patches}
          commits={commits}
          mode={mode}
        />
      </article>
    );
  },
  (before, after) => {
    if (
      before.fileKey !== after.fileKey ||
      before.mode !== after.mode ||
      before.onViewFile !== after.onViewFile
    )
      return false;
    if (
      before.patches !== after.patches &&
      JSON.stringify(before.patches) !== JSON.stringify(after.patches)
    )
      return false;
    if (after.mode === "patches" && before.commits !== after.commits)
      for (const patch of after.patches) {
        const key = `${patch.repoId}:${patch.after}`;
        if (
          before.commits?.[key] !== after.commits?.[key] &&
          JSON.stringify(before.commits?.[key]) !==
            JSON.stringify(after.commits?.[key])
        )
          return false;
      }
    return true;
  },
);

const SourceControlFilePatches = memo(function SourceControlFilePatches({
  patches,
  commits,
  mode,
}: {
  patches: SourcePatch[];
  commits: AppliedSnapshot["commits"];
  mode: NonNullable<AppliedSnapshot["comparison"]>;
}) {
  return (
    <>
      {patches.map((patch, i) => {
        const commit =
          patch.after && commits?.[`${patch.repoId}:${patch.after}`];
        const remerge =
          typeof patch.before === "object" &&
          patch.before !== null &&
          "kind" in patch.before &&
          patch.before.kind === "remerge";
        return (
          <section key={i} className="border-b border-[var(--g5-stroke)]">
            {mode === "patches" && (
              <div className="source-control-patch-label">
                {commit ? commit.summary : patch.label}
                {commit && (
                  <small>
                    {commit.sha.slice(0, 10)} · Committed{" "}
                    {committedDate(commit.committedAt)}
                    {commit.parents.length > 1
                      ? remerge
                        ? " · Merge, own changes"
                        : " · Merge, first-parent diff"
                      : ""}
                  </small>
                )}
              </div>
            )}
            <div className="source-control-diff-context">
              <span>
                {remerge
                  ? "Recreated merge"
                  : mode === "patches" && commit
                    ? "Parent"
                    : "Before"}
              </span>
              <code>
                {typeof patch.before === "string"
                  ? patch.before.slice(0, 10)
                  : remerge
                    ? "Git remerge"
                    : "Empty tree"}
              </code>
              <ArrowRight size={12} aria-hidden="true" />
              <span>{mode === "patches" && commit ? "Commit" : "After"}</span>
              <code>{patch.after?.slice(0, 10) ?? "Working files"}</code>
            </div>
            {patch.file.oldPath && patch.file.oldPath !== patch.file.path && (
              <p className="source-control-notice">
                Renamed from {patch.file.oldPath}
              </p>
            )}
            {patch.file.change === "deleted" && (
              <p className="source-control-notice">
                Deleted file: it may be unavailable in the current authorized
                source.
              </p>
            )}
            {patch.file.diff.status === "text" ? (
              <SourceControlDiffBody
                hunks={patch.file.diff.hunks as DiffHunk[]}
                additions={patch.file.diff.additions}
                deletions={patch.file.diff.deletions}
              />
            ) : (
              <p className="p-4 text-xs">
                {patch.file.diff.status === "binary"
                  ? "Binary file changed."
                  : "This file diff exceeds the display limit. View the file to inspect it."}
              </p>
            )}
          </section>
        );
      })}
    </>
  );
});

function UnpushedHelp() {
  return (
    <p>
      Not yet pushed is available only to the workspace owner while the VM is
      running. Show reads the current local work; a clean repository has no
      unpushed changes. When the VM is stopped, start it to use this row. Other
      people cannot view unpushed work.
    </p>
  );
}
function FileTypeHelp() {
  return (
    <ContextualHelp label="About changed file types">
      <UnpushedHelp />
      <p>
        Show applies the checklist. Complete selections show net changes; other
        selections keep each chosen commit's own parent comparison.
      </p>
      <p>
        <strong>Ontology</strong>: supported *.g5.md files under .g5/ontology in
        any repository.
      </p>
      <p>
        <strong>Docs</strong>: other files in docs-role repositories, including
        files outside Project Docs.
      </p>
      <p>
        <strong>Code</strong>: remaining files, including configuration, tests,
        assets and ordinary documentation.
      </p>
      <p>
        Only files matching the applied types appear in the file list and
        detailed diffs. Select more types and click Show to include their files.
      </p>
    </ContextualHelp>
  );
}
