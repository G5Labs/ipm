// =============================================================================
// API Smoke Test — Validate live backend responses against Zod schemas
// =============================================================================
// Runs direct HTTP calls to the g5-api and validates every response shape
// against the canonical Zod schemas in api-schemas.ts.
//
// Prerequisites:
//   just e2e-up           # start Docker Compose test stack
//   just api-smoke        # run this test
//
// Env vars:
//   E2E_API_URL  — Backend base URL (default: http://localhost:7500)
// =============================================================================

import { describe, test, expect } from "vitest";
import { z } from "zod";
import { routes } from "../src/lib/routes";
import {
  envelopeSchema,
  // Auth
  AuthTokensDTOSchema,
  // Users
  CurrentUserDTOSchema,
  // Orgs
  OrgListItemDTOSchema,
  OrgDTOSchema,
  OrgMemberResponseDTOSchema,
  RepoDTOSchema,
  // Workspaces
  WorkspaceListItemDTOSchema,
  WorkspaceDetailDTOSchema,
  WorkspaceStatsDTOSchema,
  // Docs
  DocListItemDTOSchema,
  // Tasks
  WorkspaceTaskPageSchema,
  // Comments
  CommentDTOSchema,
  // Sessions
  SessionListItemDTOSchema,
  // Settings
  WorkspaceSettingsDTOSchema,
  SpendSummaryRouteDTOSchema,
  // Instance
  InstanceInfoDTOSchema,
  HealthCheckResponseSchema,
  ChatModelDTOSchema,
  // Files — (need a branch to test)
  FileTreeEntrySchema,
  // Permissions
  // Scopes
  ScopeCheckResponseSchema,
  // Ontology Format
} from "../src/lib/api-schemas";

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------
const API_BASE = process.env.E2E_API_URL ?? "http://localhost:7500";

// ---------------------------------------------------------------------------
// State accumulated across sequential tests
// ---------------------------------------------------------------------------
let token: string;
let userId: string;
let orgId: string;
let orgIds: string[] = [];
let workspaceId: string;
let repoId: string | undefined;
let sessionId: string;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Authenticated GET/POST/PUT/DELETE against the API */
async function api(
  path: string,
  options?: RequestInit & { skipAuth?: boolean },
): Promise<Response> {
  const { skipAuth, ...init } = options ?? {};
  const headers: Record<string, string> = {
    "Content-Type": "application/json",
    ...(init.headers as Record<string, string>),
  };
  if (!skipAuth && token) {
    headers["Authorization"] = `Bearer ${token}`;
  }
  return fetch(`${API_BASE}${path}`, { ...init, headers });
}

/** Assert status + validate body against an envelope(schema) */
async function expectEnvelope<T extends z.ZodTypeAny>(
  res: Response,
  schema: T,
  opts?: { status?: number },
) {
  const status = opts?.status ?? 200;
  const body = await res.json();
  expect(
    res.status,
    `Expected ${status}, got ${res.status}: ${JSON.stringify(body).slice(0, 300)}`,
  ).toBe(status);
  const result = envelopeSchema(schema).safeParse(body);
  if (!result.success) {
    const issues = result.error.issues.map(
      (i) => `  ${i.path.join(".")}: ${i.message}`,
    );
    expect.fail(`Envelope validation failed:\n${issues.join("\n")}`);
  }
  return result.data!;
}

/** Assert status + validate body against an envelope(z.array(schema)) */
async function expectEnvelopeList<T extends z.ZodTypeAny>(
  res: Response,
  schema: T,
  opts?: { status?: number },
) {
  return expectEnvelope(res, z.array(schema), opts);
}

/** Assert status + validate body directly (no envelope) */
async function expectShape<T extends z.ZodTypeAny>(
  res: Response,
  schema: T,
  opts?: { status?: number },
) {
  const status = opts?.status ?? 200;
  const body = await res.json();
  expect(
    res.status,
    `Expected ${status}, got ${res.status}: ${JSON.stringify(body).slice(0, 300)}`,
  ).toBe(status);
  const result = schema.safeParse(body);
  if (!result.success) {
    const issues = result.error.issues.map(
      (i) => `  ${i.path.join(".")}: ${i.message}`,
    );
    expect.fail(`Schema validation failed:\n${issues.join("\n")}`);
  }
  return result.data!;
}

// =============================================================================
// Tests — sequential: each group builds on IDs discovered by earlier groups
// =============================================================================

describe("API Smoke Tests", () => {
  // -------------------------------------------------------------------------
  // 0. Auth — obtain JWT via test-token endpoint
  // -------------------------------------------------------------------------
  describe("Auth", () => {
    test("POST /auth/test-token → AuthTokensDTO", async () => {
      const res = await api(routes.auth.testToken, {
        method: "POST",
        body: JSON.stringify({ email: "e2e-test@g5.dev" }),
        skipAuth: true,
      });

      // test-token may return bare DTO or envelope
      const body = await res.json();
      expect(res.status).toBe(200);

      const data = body.data ?? body;
      const result = AuthTokensDTOSchema.safeParse(data);
      if (!result.success) {
        const issues = result.error.issues.map(
          (i) => `  ${i.path.join(".")}: ${i.message}`,
        );
        expect.fail(`AuthTokensDTO validation failed:\n${issues.join("\n")}`);
      }

      token = result.data!.jwt;
      userId = result.data!.userId;
      expect(token).toBeTruthy();
      expect(userId).toBeTruthy();
    });
  });

  // -------------------------------------------------------------------------
  // 1. Instance — no workspace required
  // -------------------------------------------------------------------------
  describe("Instance", () => {
    test("GET /instance/info → InstanceInfoDTO", async () => {
      const res = await api(routes.instance.info);
      await expectEnvelope(res, InstanceInfoDTOSchema);
    });

    test("GET /instance/health → HealthCheckResponse", async () => {
      const res = await api(routes.instance.health);
      await expectEnvelope(res, HealthCheckResponseSchema);
    });

    test("GET /instance/chat/models → ChatModelDTO[]", async () => {
      const res = await api(routes.instance.chatModels);
      // May be envelope or bare array
      const body = await res.json();
      expect(res.status).toBe(200);
      const arr = body.data ?? body;
      const result = z.array(ChatModelDTOSchema).safeParse(arr);
      if (!result.success) {
        const issues = result.error.issues.map(
          (i) => `  ${i.path.join(".")}: ${i.message}`,
        );
        expect.fail(`ChatModelDTO[] validation failed:\n${issues.join("\n")}`);
      }
    });
  });

  // -------------------------------------------------------------------------
  // 2. Users
  // -------------------------------------------------------------------------
  describe("Users", () => {
    test("GET /users/me → CurrentUserDTO", async () => {
      const res = await api(routes.users.me);
      const envelope = await expectEnvelope(res, CurrentUserDTOSchema);
      // Ensure the user matches the one we authenticated as
      expect(envelope.data.id).toBe(userId);
      expect(envelope.data.email).toBe("e2e-test@g5.dev");
    });
  });

  // -------------------------------------------------------------------------
  // 3. Orgs — discover orgId for workspace tests
  // -------------------------------------------------------------------------
  describe("Orgs", () => {
    test("GET /orgs → OrgListItemDTO[]", async () => {
      const res = await api(routes.orgs.list);
      const envelope = await expectEnvelopeList(res, OrgListItemDTOSchema);
      if (envelope.data.length > 0) {
        orgIds = envelope.data.map((org) => org.orgId);
        orgId = envelope.data[0].orgId;
      }
    });

    test("GET /orgs/:id → OrgDTO (if org exists)", async () => {
      expect(
        orgId,
        "No org discovered from /orgs; cannot run org detail smoke check",
      ).toBeTruthy();
      const res = await api(routes.orgs.get(orgId));
      await expectEnvelope(res, OrgDTOSchema);
    });

    test("GET /orgs/:id/members → OrgMemberResponseDTO[]", async () => {
      expect(
        orgId,
        "No org discovered from /orgs; cannot run org members smoke check",
      ).toBeTruthy();
      const res = await api(routes.orgs.members(orgId));
      await expectEnvelopeList(res, OrgMemberResponseDTOSchema);
    });

    test("GET /orgs/:id/repos → RepoDTOSchema[]", async () => {
      expect(
        orgId,
        "No org discovered from /orgs; cannot run org repos smoke check",
      ).toBeTruthy();
      const res = await api(routes.orgs.repos(orgId));
      await expectEnvelopeList(res, RepoDTOSchema);
    });
  });

  // -------------------------------------------------------------------------
  // 4. Workspaces — discover workspaceId + repoId
  // -------------------------------------------------------------------------
  describe("Workspaces", () => {
    test("GET /orgs/:id/workspaces → WorkspaceListItemDTO[]", async () => {
      expect(
        orgId,
        "No org discovered from /orgs; cannot list workspaces",
      ).toBeTruthy();
      const res = await api(routes.workspaces.list(orgId));
      const envelope = await expectEnvelopeList(
        res,
        WorkspaceListItemDTOSchema,
      );

      const workspaceLists = [{ orgId, workspaces: envelope.data }];
      for (const candidateOrgId of orgIds.filter((id) => id !== orgId)) {
        const candidateRes = await api(routes.workspaces.list(candidateOrgId));
        const candidateEnvelope = await expectEnvelopeList(
          candidateRes,
          WorkspaceListItemDTOSchema,
        );
        workspaceLists.push({
          orgId: candidateOrgId,
          workspaces: candidateEnvelope.data,
        });
      }

      const fallback =
        workspaceLists.find((entry) => entry.workspaces.length > 0) ?? null;
      let selected: (typeof workspaceLists)[number] | null = null;
      let ws: (typeof envelope.data)[number] | undefined;
      for (const entry of workspaceLists) {
        const namedCandidates = entry.workspaces.filter(
          (workspace) => workspace.name === "e2e-test",
        );
        const candidates =
          namedCandidates.length > 0 ? namedCandidates : entry.workspaces;
        for (const candidate of candidates) {
          const docsRes = await api(routes.docs.list(candidate.id));
          if (docsRes.status === 200) {
            selected = entry;
            ws = candidate;
            break;
          }
        }
        if (ws && selected === entry) {
          break;
        }
      }
      if (!ws && fallback) {
        selected = fallback;
        ws = fallback.workspaces[0];
      }

      if (ws) {
        if (selected) {
          orgId = selected.orgId;
        }
        workspaceId = ws.id;
        // Try to extract a branch ID
        if (ws.branches?.length > 0) {
          repoId = ws.branches[0].repoId;
        }
      }
    });

    test("GET /workspaces/:id → WorkspaceDetailDTO", async () => {
      expect(
        workspaceId,
        "No workspace discovered from org workspace listing",
      ).toBeTruthy();
      const res = await api(routes.workspaces.get(workspaceId));
      const envelope = await expectEnvelope(res, WorkspaceDetailDTOSchema);
      // Also try to extract repoId if not yet available
      if (!repoId && envelope.data.branches?.length > 0) {
        repoId = envelope.data.branches[0].repoId;
      }
    });

    test("removed sharing routes and privacy patches are refused", async () => {
      expect(workspaceId).toBeTruthy();
      const detail = await api(routes.workspaces.get(workspaceId));
      const before = await detail.json();
      expect(detail.status).toBe(200);
      expect(before.data).not.toHaveProperty("visibility");
      for (const method of ["GET", "POST", "DELETE"]) {
        const suffix = method === "DELETE" ? `/${userId}` : "";
        const response = await api(
          `/api/v1/workspaces/${workspaceId}/members${suffix}`,
          {
            method,
            ...(method === "POST"
              ? { body: JSON.stringify({ userId, role: "viewer" }) }
              : {}),
          },
        );
        expect(response.status).toBe(404);
      }
      const rejected = await api(routes.workspaces.get(workspaceId), {
        method: "PATCH",
        body: JSON.stringify({
          visibility: "invite_only",
          description: "must not change",
        }),
      });
      expect(rejected.status).toBe(422);
      const after = await (
        await api(routes.workspaces.get(workspaceId))
      ).json();
      expect(after.data.description).toEqual(before.data.description);
    });

    test("GET /workspaces/:id/stats → WorkspaceStatsDTO", async () => {
      expect(
        workspaceId,
        "No workspace discovered from org workspace listing",
      ).toBeTruthy();
      const res = await api(routes.workspaces.stats(workspaceId));
      await expectEnvelope(res, WorkspaceStatsDTOSchema);
    });
  });

  // -------------------------------------------------------------------------
  // 6. Docs
  // -------------------------------------------------------------------------
  describe("Docs", () => {
    test("GET /workspaces/:id/docs → DocListItemDTO[]", async () => {
      expect(
        workspaceId,
        "No workspace discovered from org workspace listing",
      ).toBeTruthy();
      // Retry up to 3 times on transient socket errors — the server may
      // drop keep-alive connections under load after heavy Rust E2E phases.
      let res: Response | undefined;
      for (let attempt = 0; attempt < 3; attempt++) {
        try {
          res = await api(routes.docs.list(workspaceId));
          break;
        } catch (err: unknown) {
          if (attempt === 2) throw err;
          // Increasing back-off: 500 ms, 1 s
          await new Promise((r) => setTimeout(r, 500 * (attempt + 1)));
        }
      }
      expect(
        res,
        "Docs list request did not produce a response after retries",
      ).toBeDefined();
      await expectEnvelope(
        res!,
        z.object({
          docs: z.array(DocListItemDTOSchema),
          source: z.string().nullable(),
          repoOid: z.string().nullable(),
          snapshotAt: z.string().nullable(),
        }),
      );
    });
  });

  // -------------------------------------------------------------------------
  // 7. Tasks
  // -------------------------------------------------------------------------
  describe("Tasks", () => {
    test("GET /workspaces/:id/tasks → bounded task page", async () => {
      expect(
        workspaceId,
        "No workspace discovered from org workspace listing",
      ).toBeTruthy();
      const res = await api(`/api/v1/workspaces/${workspaceId}/tasks?limit=50`);
      const page = await expectEnvelope(res, WorkspaceTaskPageSchema);
      expect(page.data.tasks.length).toBeLessThanOrEqual(50);
      expect(page.data.hasMore).toBe(page.data.cursor !== null);
    });
  });

  // -------------------------------------------------------------------------
  // 8. Comments
  // -------------------------------------------------------------------------
  describe("Comments", () => {
    test("GET /workspaces/:id/comments → CommentDTO[]", async () => {
      expect(
        workspaceId,
        "No workspace discovered from org workspace listing",
      ).toBeTruthy();
      const res = await api(routes.comments.list(workspaceId));
      await expectEnvelopeList(res, CommentDTOSchema);
    });
  });

  // -------------------------------------------------------------------------
  // 9. Chat Sessions
  // -------------------------------------------------------------------------
  describe("Sessions", () => {
    test("GET /workspaces/:id/sessions → SessionListItemDTO[]", async () => {
      expect(
        workspaceId,
        "No workspace discovered from org workspace listing",
      ).toBeTruthy();
      const res = await api(routes.sessions.list(workspaceId));
      const envelope = await expectEnvelopeList(res, SessionListItemDTOSchema);
      if (envelope.data.length > 0) {
        sessionId = envelope.data[0].id;
        return;
      }

      const createRes = await api(routes.sessions.create(workspaceId), {
        method: "POST",
        body: JSON.stringify({ title: `api-smoke-${Date.now()}` }),
      });
      expect([200, 201]).toContain(createRes.status);
      const body = await createRes.json();
      sessionId = body.data?.id ?? body.data?.session?.id ?? body.id;
      expect(sessionId, "Session creation returned no ID").toBeTruthy();
    });
  });

  // -------------------------------------------------------------------------
  // 10. Retired execution routes
  // -------------------------------------------------------------------------
  describe("Retired execution routes", () => {
    test("drafts, per-task workers, and chat context links are absent", async () => {
      expect(
        workspaceId,
        "No workspace discovered from org workspace listing",
      ).toBeTruthy();
      expect(orgId, "No org discovered from auth bootstrap").toBeTruthy();
      expect(
        sessionId,
        "No session discovered from workspace session listing",
      ).toBeTruthy();
      for (const path of [
        `/api/v1/workspaces/${workspaceId}/drafts`,
        `/api/v1/workspaces/${workspaceId}/workers`,
        `/api/v1/orgs/${orgId}/workers`,
        `/api/v1/sessions/${sessionId}/context-links`,
      ]) {
        expect((await api(path)).status, path).toBe(404);
      }
    });
  });

  // -------------------------------------------------------------------------
  // 12. Settings & Spend
  // -------------------------------------------------------------------------
  describe("Settings", () => {
    test("GET /workspaces/:id/settings → WorkspaceSettingsDTO", async () => {
      expect(
        workspaceId,
        "No workspace discovered from org workspace listing",
      ).toBeTruthy();
      const res = await api(routes.settings.get(workspaceId));
      await expectEnvelope(res, WorkspaceSettingsDTOSchema);
    });

    test("GET /workspaces/:id/settings/spend → SpendSummaryRouteDTO", async () => {
      expect(
        workspaceId,
        "No workspace discovered from org workspace listing",
      ).toBeTruthy();
      const res = await api(routes.settings.spend(workspaceId));
      await expectEnvelope(res, SpendSummaryRouteDTOSchema);
    });
  });

  // -------------------------------------------------------------------------
  // 13. Workspace search removal (P8.9)
  // -------------------------------------------------------------------------
  describe("Workspace search removal", () => {
    test("GET /workspaces/:id/search → 404 (route removed)", async () => {
      expect(
        workspaceId,
        "No workspace discovered from org workspace listing",
      ).toBeTruthy();
      const res = await api(`/api/v1/workspaces/${workspaceId}/search?q=test`);
      expect(res.status).toBe(404);
    });

    test("GET /instance/search/status → 404 (route removed)", async () => {
      const res = await api("/api/v1/instance/search/status");
      expect(res.status).toBe(404);
    });

    test("GET /workspaces/:id/sessions/search stays available", async () => {
      expect(
        workspaceId,
        "No workspace discovered from org workspace listing",
      ).toBeTruthy();
      const res = await api(`${routes.sessions.search(workspaceId)}?q=test`);
      expect(res.status).toBe(200);
    });
  });

  // -------------------------------------------------------------------------
  // 14. Files (needs repoId)
  // -------------------------------------------------------------------------
  describe("Files", () => {
    test("GET /workspaces/:id/repos/:repoId/files → FileTreeEntry[]", async () => {
      // Phase-6b's lightweight API-smoke stack may only have unborn bootstrap
      // workspaces, whose branches are intentionally hidden from workspace
      // list/detail responses. In that case there is no valid branch-scoped
      // file-tree endpoint to validate.
      if (!repoId) return;
      const res = await api(routes.files.tree(workspaceId!, repoId));
      await expectEnvelope(
        res,
        z.object({
          entries: z.array(FileTreeEntrySchema),
          source: z.string(),
          repoOid: z.string(),
          snapshotAt: z.string(),
        }),
      );
    });
  });

  // -------------------------------------------------------------------------
  // 15. Permissions / Scopes
  // -------------------------------------------------------------------------
  describe("Scopes", () => {
    test("GET /auth/me/scopes → EffectiveScopesResponse", async () => {
      const res = await api(routes.auth.myScopes);
      const EffectiveScopesSchema = z.object({
        scopes: z.array(z.string()),
        orgDefaults: z.array(z.string()),
        userGrants: z.array(z.string()),
        workspaceOverrides: z.array(z.string()),
      });
      await expectEnvelope(res, EffectiveScopesSchema);
    });

    test("POST /auth/me/scopes/check → ScopeCheckResponse", async () => {
      const res = await api(routes.auth.checkScopes, {
        method: "POST",
        body: JSON.stringify({ scopes: ["workspace:read"] }),
      });
      await expectEnvelope(res, ScopeCheckResponseSchema);
    });
  });
});
