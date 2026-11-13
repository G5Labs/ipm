// Mock mode must advertise the dev stack's instance features: the `frontend`
// preview profile runs Vite with VITE_MOCK=true, and with the fixture's two
// features alone the gated sections (Guided, provider accounts) were missing
// (QA, 2026-10-13).
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { MOCK_INSTANCE_INFO } from "../data/mock";
import { GUIDED_JOURNEY_FEATURE } from "../lib/instanceFeatures";
import { EXTERNAL_ISSUES_ENABLED_SETTING } from "../lib/externalIssuesFlags";
import { VERIFICATION_ACTION_ENABLED_SETTING } from "../lib/verificationActionFlags";
import {
  DEV_DEFAULT_FEATURES,
  MOCK_DEMO_FEATURES,
  installFetchInterceptor,
  mockInstanceFeatures,
} from "./fetch-interceptor";
import { MOCK_GITHUB_LOGIN, searchMockIssues } from "./home-preview";

describe("mockInstanceFeatures", () => {
  it("adds the dev defaults and demo features to the fixture's features", () => {
    const features = mockInstanceFeatures(["chat", "codex_ui"], undefined);
    expect(features).toEqual([
      "chat",
      "codex_ui",
      ...DEV_DEFAULT_FEATURES,
      ...MOCK_DEMO_FEATURES,
    ]);
    expect(features).toContain("external_issues");
    expect(features).toContain(GUIDED_JOURNEY_FEATURE);
  });

  it("lets VITE_MOCK_FEATURES replace the defaults, down to none", () => {
    expect(
      mockInstanceFeatures(["chat"], "external_issues, github_app"),
    ).toEqual(["chat", "external_issues", "github_app"]);
    expect(mockInstanceFeatures(["chat"], "")).toEqual(["chat"]);
  });

  it("deduplicates and ignores a malformed fixture", () => {
    expect(mockInstanceFeatures(["guided_journey"], undefined)).toEqual([
      ...DEV_DEFAULT_FEATURES,
      ...MOCK_DEMO_FEATURES,
    ]);
    expect(mockInstanceFeatures(null, "")).toEqual([]);
  });
});

describe("installFetchInterceptor", () => {
  // MSW's server (test setup) wraps fetch before each test; capture that
  // wrapper, not the native one, so cleanup restores what MSW installed.
  // The override env is cleared so a developer's `.env.local` setting of
  // VITE_MOCK_FEATURES cannot change the default-feature assertion.
  let fetchBefore: typeof window.fetch;
  beforeEach(() => {
    fetchBefore = window.fetch;
    vi.stubEnv("VITE_MOCK_FEATURES", undefined);
    vi.spyOn(console, "debug").mockImplementation(() => {});
  });
  afterEach(() => {
    window.fetch = fetchBefore;
    vi.unstubAllEnvs();
    vi.restoreAllMocks();
  });

  it("answers instance info with the fixture plus the dev defaults", async () => {
    installFetchInterceptor();
    const response = await window.fetch(
      "http://localhost/api/v1/instance/info",
    );
    const body = (await response.json()) as {
      data: {
        version: string;
        features: string[];
      };
    };
    expect(body.data.version).toBe(MOCK_INSTANCE_INFO.version);
    expect(body.data.features).toEqual([
      ...MOCK_INSTANCE_INFO.features,
      ...[...DEV_DEFAULT_FEATURES, ...MOCK_DEMO_FEATURES].filter(
        (f) => !MOCK_INSTANCE_INFO.features.includes(f),
      ),
    ]);
  });

  it("answers the Home reads the fixture leaves empty", async () => {
    installFetchInterceptor();
    const get = async <T>(path: string) =>
      (
        (await (await window.fetch(`http://localhost${path}`)).json()) as {
          data: T;
        }
      ).data;

    const search = await get<{ results: unknown[]; domainCount: number }>(
      "/api/v1/orgs/org-g5labs/issues/search?q=&state=open",
    );
    expect(search.domainCount).toBe(2);
    expect(search.results.length).toBeGreaterThan(8);

    const connection = await get<{ connected: boolean; githubLogin: string }>(
      "/api/v1/github-app/user-connection",
    );
    expect(connection).toEqual({
      connected: true,
      githubLogin: MOCK_GITHUB_LOGIN,
    });

    const attention = await get<{ items: Array<{ kind: string }> }>(
      "/api/v1/orgs/org-g5labs/home/attention?limit=10&kind=approval&order=newest",
    );
    expect(attention.items.length).toBeGreaterThan(0);
    expect(attention.items.every((i) => i.kind === "approval")).toBe(true);

    const compute = await get<unknown[]>(
      "/api/v1/orgs/org-g5labs/compute/running",
    );
    expect(compute.length).toBeGreaterThan(0);
  });

  it("reads the org's external-issues switch as on until it is set", async () => {
    installFetchInterceptor();
    const response = await window.fetch(
      "http://localhost/api/v1/orgs/org-g5labs/config",
    );
    const body = (await response.json()) as {
      data: {
        settings: Record<string, unknown>;
        effectiveSettings: Record<string, unknown>;
      };
    };
    expect(body.data.settings[EXTERNAL_ISSUES_ENABLED_SETTING]).toBe(true);
    expect(body.data.effectiveSettings[EXTERNAL_ISSUES_ENABLED_SETTING]).toBe(
      true,
    );
    // The verification switch keeps its real default, off, until Admin
    // turns it on (ui g5.workbench.review-prompts.switch).
    expect(
      body.data.settings[VERIFICATION_ACTION_ENABLED_SETTING],
    ).toBeUndefined();
  });

  it("shows Base's re-verification and starts a run through the clone route", async () => {
    vi.spyOn(console, "warn").mockImplementation(() => {});
    installFetchInterceptor();
    const status = (await (
      await window.fetch(
        "http://localhost/api/v1/orgs/org-g5labs/home/base-reverification",
      )
    ).json()) as { data: { enabled: boolean; state?: string } };
    expect(status.data).toMatchObject({ enabled: true, state: "stale" });

    const post = (body: unknown) =>
      window.fetch("http://localhost/api/v1/workspaces/base/clone", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body),
      });
    // An ordinary fork is not the fixture's to answer: the handler passes.
    expect((await post({ name: "plain", idempotencyKey: "k1" })).status).toBe(
      404,
    );
    const started = await post({
      name: "base-reverification-2026-11-06",
      idempotencyKey: "k2",
      uniquifyName: true,
      reverification: { kind: "full" },
    });
    expect(started.status).toBe(201);
    const body = (await started.json()) as {
      data: { workspace: { name: string }; reverificationRun: unknown };
    };
    expect(body.data.workspace.name).toBe("base-reverification-2026-11-06");
    expect(body.data.reverificationRun).toMatchObject({
      kind: "full",
      state: "in_progress",
    });
  });

  it("honors a VITE_MOCK_FEATURES override, including an empty one", async () => {
    vi.stubEnv("VITE_MOCK_FEATURES", "");
    installFetchInterceptor();
    const response = await window.fetch(
      "http://localhost/api/v1/instance/info?probe=1",
    );
    const body = (await response.json()) as { data: { features: string[] } };
    expect(body.data.features).toEqual([...MOCK_INSTANCE_INFO.features]);
  });

  it("leaves other mocked routes untouched", async () => {
    installFetchInterceptor();
    const response = await window.fetch(
      "http://localhost/api/v1/instance/health",
    );
    const body = (await response.json()) as { data: Record<string, unknown> };
    expect(body.data).not.toHaveProperty("features");
  });
});

describe("searchMockIssues", () => {
  const row = (title: string, labels: string[], assignees: string[] = []) =>
    ({
      title,
      labels: labels.map((name) => ({ name, color: null })),
      assignees: assignees.map((login) => ({ login, avatarUrl: null })),
    }) as unknown as Parameters<typeof searchMockIssues>[0][number];
  const rows = [
    row("Task queue stalls", ["bug"], [MOCK_GITHUB_LOGIN]),
    row("Document the preview", ["docs", "good first issue"]),
    row("Queue metrics", ["enhancement"], ["someone"]),
  ];

  it("matches every free term in the title, case-insensitively", () => {
    expect(searchMockIssues(rows, "QUEUE", null)).toHaveLength(2);
    expect(searchMockIssues(rows, "queue stalls", null)).toEqual([rows[0]]);
    expect(searchMockIssues(rows, "", null)).toEqual(rows);
  });

  it("filters by label qualifiers, quoted or not, and ignores others", () => {
    expect(searchMockIssues(rows, "label:docs", null)).toEqual([rows[1]]);
    expect(
      searchMockIssues(rows, 'label:"good first issue" is:open', null),
    ).toEqual([rows[1]]);
  });

  it("narrows by assignee: me is the connected login, none is unassigned", () => {
    expect(searchMockIssues(rows, "", "me")).toEqual([rows[0]]);
    expect(searchMockIssues(rows, "", "none")).toEqual([rows[1]]);
  });
});
