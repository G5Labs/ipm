import { describe, expect, it } from "vitest";
import {
  isAdminCenterTabItem,
  knownTabsOnly,
  requiresContentBranch,
  tabToPath,
  upsertTab,
} from "./WorkspacePage";
import type { TabItem } from "../lib/types";

describe("isAdminCenterTabItem", () => {
  it("is true for the admin center tabs", () => {
    const admin: TabItem[] = [
      { type: "workspace_settings", name: "Settings" },
      { type: "worker_hosts", name: "Workers" },
      { type: "org_members", name: "Members" },
      { type: "org_approvals", name: "Approvals" },
      { type: "org_github_app", name: "GitHub App" },
      { type: "org_admin", name: "Admin" },
    ];
    for (const tab of admin) {
      expect(isAdminCenterTabItem(tab)).toBe(true);
    }
  });

  it("is false for editor/job/content tabs (drawer stays available)", () => {
    const nonAdmin: TabItem[] = [
      { type: "file", filePath: "src/a.ts", name: "a.ts", branchId: "b1" },
      { type: "doc", docId: "d1", name: "Doc" },
      {
        type: "external_issue",
        repositoryId: "11",
        number: 7,
        name: "acme/api#7",
      },
    ];
    for (const tab of nonAdmin) {
      expect(isAdminCenterTabItem(tab)).toBe(false);
    }
  });

  it("is false for null/undefined (no tab selected)", () => {
    expect(isAdminCenterTabItem(null)).toBe(false);
    expect(isAdminCenterTabItem(undefined)).toBe(false);
  });
});

describe("requiresContentBranch", () => {
  const cases = {
    ontology_changes: false,
    doc: true,
    file: true,
    intent_log: true,
    guide_doc: false,
    pull_requests: false,
    workspace_changes: false,
    source_control_diff: false,
    source_control_change_graph: false,
    source_control_commit_diff: false,
    workspace_terminal: false,
    workspace_settings: false,
    worker_hosts: false,
    org_members: false,
    org_approvals: false,
    org_github_app: false,
    org_admin: false,
    workspace_approvals: false,
    workspace_review: false,
    workspace_details: false,
    workspace_activity: false,
    external_issue: false,
  } satisfies Record<TabItem["type"], boolean>;

  it.each(Object.entries(cases))(
    "%s requires branch selection: %s",
    (type, expected) => {
      expect(requiresContentBranch({ type } as TabItem)).toBe(expected);
    },
  );

  it("keeps an empty center panel idle", () => {
    expect(requiresContentBranch(null)).toBe(false);
    expect(requiresContentBranch(undefined)).toBe(false);
  });
});

describe("external issue tabs", () => {
  const issue = (repositoryId: string, number: number): TabItem => ({
    type: "external_issue",
    repositoryId,
    number,
    name: `acme/api#${number}`,
  });

  it("survive tab restoration; a retired Issues tab is dropped", () => {
    // Issues moved into Workspace Details; tabs persisted before that
    // still name the old singleton.
    const retired = {
      type: "workspace_issues",
      name: "Issues",
    } as unknown as TabItem;
    expect(knownTabsOnly([retired, issue("11", 7)])).toEqual([issue("11", 7)]);
  });

  it("key issue tabs by repository and number", () => {
    let tabs = upsertTab([], issue("11", 7));
    tabs = upsertTab(tabs, issue("11", 7));
    tabs = upsertTab(tabs, issue("11", 8));
    tabs = upsertTab(tabs, issue("12", 7));
    expect(tabs).toHaveLength(3);
  });

  it("own the `/issue/:repositoryId/:number` route", () => {
    expect(tabToPath(issue("11", 7))).toBe("/issue/11/7");
    expect(tabToPath(issue("repo/one", 7))).toBe("/issue/repo%2Fone/7");
  });
});
