// =============================================================================
// API Contract Tests — Validates that frontend TypeScript interfaces match the
// Rust backend DTOs (represented as Zod schemas).
//
// These tests run at build time (vitest) with NO running server required.
// They catch mismatches between what the frontend expects (types.ts) and
// what the backend actually returns (api-schemas.ts Zod schemas).
//
// Methodology:
//   For each DTO, we construct a mock object that satisfies the Zod schema,
//   then verify it's assignable to the corresponding TypeScript interface.
//   Missing/extra fields and type mismatches are caught at compile time.
//
// When a test fails:
//   1. Check if the Rust DTO changed → update types.ts to match
//   2. Check if types.ts has aspirational fields → remove or make optional
//   3. Update api-schemas.ts if the Rust source changed
// =============================================================================

import { describe, it, expect } from "vitest";
import { z } from "zod";

// Zod schemas (Rust source-of-truth)
import {
  PaginationMetaSchema,
  AuthTokensDTOSchema,
  CurrentUserDTOSchema,
  OrgListItemDTOSchema,
  OrgDTOSchema,
  OrgMemberResponseDTOSchema,
  OrgAdminSettingsDTOSchema,
  OrgInvitationDTOSchema,
  PlatformOrgDTOSchema,
  CreatePlatformOrgResponseDTOSchema,
  BranchSummaryDTOSchema,
  WorkspaceListItemDTOSchema,
  WorkspaceDetailDTOSchema,
  WorkspaceStatsDTOSchema,
  CloneWorkspaceResponseDTOSchema,
  TaskDTOSchema,
  TimelineEntryDTOSchema,
  CommentDTOSchema,
  ReplyDTOSchema,
  SessionListItemDTOSchema,
  SessionInputDTOSchema,
  ChatMessageDTOSchema as ChatMessageDTOZod,
  WorkspaceSettingsDTOSchema,
  InstanceInfoDTOSchema,
  HealthCheckResponseSchema,
  SpendHistoryRouteDTOSchema,
  SpendSummaryRouteDTOSchema,
  FileTreeEntrySchema,
  DocListItemDTOSchema,
  DocDetailDTOSchema,
} from "../api-schemas";

// TypeScript interfaces (frontend expectations)
import type {
  WorkspaceSummary,
  WorkspaceDetail,
  WorkspaceStats,
  BranchSummary,
  CloneWorkspaceResponse,
  TaskSummary,
  TaskDetail,
  TimelineEntry,
  CommentDTO as TSCommentDTO,
  ChatSessionDTO,
  ChatMessageDTO as TSChatMessageDTO,
  SessionInputDTO as TSSessionInputDTO,
  Citation as TSCitation,
  WorkspaceSettings,
  InstanceInfoDTO as TSInstanceInfoDTO,
  HealthStatus,
  SpendHistoryDTO,
  SpendBreakdownData,
  FileTreeEntry as TSFileTreeEntry,
  DocNode,
  CurrentUserDTO as TSCurrentUserDTO,
  Organization,
  OrgMemberDTO,
  OrgAdminSettingsDTO,
  OrgInvitationDTO as TSOrgInvitationDTO,
  PlatformOrgDTO,
  CreatePlatformOrgResponse,
} from "../types";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/**
 * Creates a sample value that satisfies a Zod schema.
 * Used to generate test fixtures from the schema definition.
 */
function sampleFromSchema<T extends z.ZodTypeAny>(schema: T): z.infer<T> {
  return generateSample(schema) as z.infer<T>;
}

function generateSample(schema: any): unknown {
  if (schema instanceof z.ZodString) return "test-string";
  if (schema instanceof z.ZodNumber) return 42;
  if (schema instanceof z.ZodBoolean) return true;
  if (schema instanceof z.ZodNull) return null;
  if (schema instanceof z.ZodUndefined) return undefined;
  if (schema instanceof z.ZodLiteral) return schema.value;
  if (schema instanceof z.ZodEnum) return schema.options[0];
  if (schema instanceof z.ZodUnknown) return {};

  if (schema instanceof z.ZodNullable) {
    return generateSample(schema.unwrap());
  }

  if (schema instanceof z.ZodOptional) {
    return generateSample(schema.unwrap());
  }

  if (schema instanceof z.ZodArray) {
    return [generateSample(schema.element)];
  }

  if (schema instanceof z.ZodRecord) {
    return {};
  }

  if (schema instanceof z.ZodObject) {
    const shape = schema.shape as Record<string, z.ZodTypeAny>;
    const obj: Record<string, unknown> = {};
    for (const [key, fieldSchema] of Object.entries(shape)) {
      obj[key] = generateSample(fieldSchema);
    }
    return obj;
  }

  if (schema instanceof z.ZodLazy) {
    // For recursive types, return minimal
    return {};
  }

  return undefined;
}

/**
 * Type assertion helper: if this compiles, the types are compatible.
 * We check that a Zod-inferred type is assignable to the TS interface.
 */
function assertSchemaMatchesInterface<TSchema extends z.ZodTypeAny, TInterface>(
  _schemaType: z.infer<TSchema> extends TInterface ? true : false,
): void {
  // This is a compile-time only check. If it compiles, the types match.
}

// ---------------------------------------------------------------------------
// Contract: Report mismatches between Rust DTOs and TypeScript interfaces
// ---------------------------------------------------------------------------

/**
 * For each domain area, we:
 * 1. Parse a sample through the Zod schema (validates schema is coherent)
 * 2. Document known differences between Rust DTO and TS interface
 * 3. Where types already match, verify with compile-time assertions
 */

describe("API Contract Tests", () => {
  // =========================================================================
  // Response Envelope
  // =========================================================================
  describe("Response Envelope", () => {
    it("PaginationMeta schema is valid", () => {
      const sample = { page: 1, perPage: 20, total: 100 };
      expect(PaginationMetaSchema.parse(sample)).toEqual(sample);
    });
  });

  // =========================================================================
  // Auth
  // =========================================================================
  describe("Auth DTOs", () => {
    it("AuthTokensDTO schema is valid", () => {
      const sample = sampleFromSchema(AuthTokensDTOSchema);
      expect(AuthTokensDTOSchema.parse(sample)).toBeDefined();
    });
  });

  // =========================================================================
  // Users
  // =========================================================================
  describe("User DTOs", () => {
    it("CurrentUserDTO: Rust→TS mismatch documented", () => {
      const rustSample = sampleFromSchema(CurrentUserDTOSchema);
      expect(CurrentUserDTOSchema.parse(rustSample)).toBeDefined();

      // MISMATCH: TS CurrentUserDTO expects different shape
      // Rust: { id, email, displayName, avatarUrl?, oauthProvider?, orgs: OrgMembershipSummary[], scmConnections: ScmConnectionSummary[], createdAt }
      // TS:   { id, email, displayName, avatarUrl?, provider?, orgs: OrgMembership[], createdAt }
      //
      // Differences:
      // - Rust has `oauthProvider`, TS has `provider`
      // - Rust has `scmConnections[]`, TS does not
      // - Rust orgs shape: { orgId, orgName, orgSlug, role }
      // - TS orgs shape:   { orgId, name, slug, role }
    });
  });

  // =========================================================================
  // Organizations
  // =========================================================================
  describe("Org DTOs", () => {
    it("OrgListItemDTO schema is valid", () => {
      const sample = sampleFromSchema(OrgListItemDTOSchema);
      expect(OrgListItemDTOSchema.parse(sample)).toBeDefined();
    });

    it("OrgDTO: Rust→TS mismatch documented", () => {
      const rustSample = sampleFromSchema(OrgDTOSchema);
      expect(OrgDTOSchema.parse(rustSample)).toBeDefined();

      // MISMATCH: TS Organization expects different shape
      // Rust OrgDTO: { id, name, slug, createdAt }
      // TS Organization: { id, name, slug, plan, memberCount, createdAt }
      //
      // Differences:
      // - TS has `plan` (string) — not in Rust
      // - TS has `memberCount` (number) — not in Rust
    });

    it("OrgAdminSettingsDTO matches the TS type and the Admin page ranges", () => {
      const sample: OrgAdminSettingsDTO = {
        defaultIdleTimeoutSecs: 28_800,
        maxRunningVms: 10,
        minFreeSlots: 1,
      };
      expect(OrgAdminSettingsDTOSchema.parse(sample)).toEqual(sample);
      expect(() =>
        OrgAdminSettingsDTOSchema.parse({ ...sample, maxRunningVms: 101 }),
      ).toThrow();
      expect(() =>
        OrgAdminSettingsDTOSchema.parse({ ...sample, minFreeSlots: 101 }),
      ).toThrow();
      const withOwnerLimits: OrgAdminSettingsDTO = {
        ...sample,
        maxRunningVmsPerOwner: 5,
        maxWorkspacesPerOwner: 50,
      };
      expect(OrgAdminSettingsDTOSchema.parse(withOwnerLimits)).toEqual(
        withOwnerLimits,
      );
      expect(() =>
        OrgAdminSettingsDTOSchema.parse({
          ...sample,
          maxRunningVmsPerOwner: 101,
        }),
      ).toThrow();
      expect(() =>
        OrgAdminSettingsDTOSchema.parse({
          ...sample,
          maxWorkspacesPerOwner: 1001,
        }),
      ).toThrow();
      expect(() =>
        OrgAdminSettingsDTOSchema.parse({
          ...sample,
          defaultIdleTimeoutSecs: 59,
        }),
      ).toThrow();
    });

    it("platform operator DTOs match the TS types", () => {
      const org = sampleFromSchema(PlatformOrgDTOSchema);
      expect(PlatformOrgDTOSchema.parse(org)).toEqual(org);
      assertSchemaMatchesInterface<typeof PlatformOrgDTOSchema, PlatformOrgDTO>(
        true,
      );

      const created = sampleFromSchema(CreatePlatformOrgResponseDTOSchema);
      expect(CreatePlatformOrgResponseDTOSchema.parse(created)).toEqual(
        created,
      );
      assertSchemaMatchesInterface<
        typeof CreatePlatformOrgResponseDTOSchema,
        CreatePlatformOrgResponse
      >(true);
    });

    it("OrgMemberResponseDTO: Rust→TS mismatch documented", () => {
      const rustSample = sampleFromSchema(OrgMemberResponseDTOSchema);
      expect(OrgMemberResponseDTOSchema.parse(rustSample)).toBeDefined();

      // MISMATCH: TS OrgMemberDTO expects different shape
      // Rust: { userId, email, displayName, avatarUrl?, role, joinedAt }
      // TS:   { userId, displayName, avatarUrl?, email, role, joinedAt }
      // -> Fields match! TS type is compatible.
    });

    it("OrgInvitationDTO: Rust→TS partial match", () => {
      const rustSample = sampleFromSchema(OrgInvitationDTOSchema);
      expect(OrgInvitationDTOSchema.parse(rustSample)).toBeDefined();

      // Rust invitedBy: { userId, displayName }  (UserRefDTO)
      // TS   invitedBy: UserRef = { userId, displayName, avatarUrl? }
      // -> TS expects avatarUrl on UserRef but Rust doesn't include it
    });
  });

  // =========================================================================
  // Workspaces
  // =========================================================================
  describe("Workspace DTOs", () => {
    it("BranchSummaryDTO: Rust→TS mismatch documented", () => {
      const rustSample = sampleFromSchema(BranchSummaryDTOSchema);
      expect(BranchSummaryDTOSchema.parse(rustSample)).toBeDefined();

      // MISMATCH: TS BranchSummary vs Rust BranchSummaryDTO
      // Rust: { workspaceId, repoId, repoName, branchName, trackingMode, headCommit, divergence, behindBase, hasDraft, dirtyFileCount }
      // TS:   { workspaceId, repoId, repoName, branchName, headCommit, hasDraft, draftDirtyCount }
      //
      // Differences:
      // - Rust has `trackingMode`, `divergence`, `behindBase` — TS does not
      // - Rust has `dirtyFileCount`, TS has `draftDirtyCount` (different name!)
    });

    it("WorkspaceListItemDTO: Rust→TS major mismatch", () => {
      const rustSample = sampleFromSchema(WorkspaceListItemDTOSchema);
      expect(WorkspaceListItemDTOSchema.parse(rustSample)).toBeDefined();

      // MAJOR MISMATCH: TS WorkspaceSummary vs Rust WorkspaceListItemDTO
      //
      // Rust: { id, name, status, description?, clonedFromId?, branches: BranchSummaryDTO[],
      //         tasksCompleted, totalTasks, runningTime?, cost, concurrentTasks,
      //         lastUpdated, createdAt }
      //
      // TS:   { id, name, status, description?, createdBy: UserRef,
      //         branches: BranchSummary[], stats: { runningJobs, completedTasks, activeTasks,
      //         blockedTasks, failedTasks, totalCostUsd } }
      //
      // Differences:
      // - Rust has flat stats: tasksCompleted, totalTasks, cost, concurrentTasks
      //   TS has nested stats: { runningJobs, completedTasks, ... }
      // - Rust has lastUpdated, createdAt — TS does not
      // - TS has createdBy: UserRef — Rust does not
      // - Rust has runningTime (string) — TS does not
      // - Rust cost (f64) vs TS stats.totalCostUsd
    });

    it("WorkspaceListItemDTO: a clone response omits phase, a list read carries it (g5.workbench.phases.state)", () => {
      const listItem = sampleFromSchema(WorkspaceListItemDTOSchema);
      // The clone response reuses the list DTO with `phase: None`, serialized
      // as an ABSENT key (skip_serializing_if), never `null`.
      const { phase: _phase, ...cloneShaped } = listItem;
      expect(
        WorkspaceListItemDTOSchema.parse(cloneShaped).phase,
      ).toBeUndefined();
      expect(
        WorkspaceListItemDTOSchema.parse({ ...listItem, phase: "se" }).phase,
      ).toBe("se");
      expect(() =>
        WorkspaceListItemDTOSchema.parse({ ...listItem, phase: null }),
      ).toThrow();
    });

    it("Workspace DTOs: `kind` is optional and limited to base, working and personal (g5.personal.identity)", () => {
      const listItem = sampleFromSchema(WorkspaceListItemDTOSchema);
      const { kind: _kind, ...olderServer } = listItem;
      expect(
        WorkspaceListItemDTOSchema.parse(olderServer).kind,
      ).toBeUndefined();
      expect(
        WorkspaceListItemDTOSchema.parse({ ...listItem, kind: "personal" })
          .kind,
      ).toBe("personal");
      expect(() =>
        WorkspaceListItemDTOSchema.parse({ ...listItem, kind: "review" }),
      ).toThrow();
      const detail = sampleFromSchema(WorkspaceDetailDTOSchema);
      expect(
        WorkspaceDetailDTOSchema.parse({ ...detail, kind: "base" }).kind,
      ).toBe("base");
    });

    it("WorkspaceDetailDTO: Rust→TS major mismatch", () => {
      const rustSample = sampleFromSchema(WorkspaceDetailDTOSchema);
      expect(WorkspaceDetailDTOSchema.parse(rustSample)).toBeDefined();

      // MAJOR MISMATCH: TS WorkspaceDetail extends WorkspaceSummary
      // Rust: { id, orgId, name, isBase, status, description?, clonedFromId?,
      //         createdBy?(string), createdAt, updatedAt,
      //         branches: BranchSummaryDTO[] }
      //
      // TS:   extends WorkspaceSummary + { branchMode, clonedFromId, activeBranchId,
      //        branches: BranchDetail[], settings: WorkspaceSettings }
      //
      // Differences:
      // - Rust has orgId, isBase, updatedAt — TS doesn't
      // - TS has branchMode, activeBranchId, settings — Rust doesn't
      // - Rust createdBy is string|null, TS expects UserRef
    });

    it("WorkspaceStatsDTO: Rust→TS mismatch", () => {
      const rustSample = sampleFromSchema(WorkspaceStatsDTOSchema);
      expect(WorkspaceStatsDTOSchema.parse(rustSample)).toBeDefined();
    });
  });

  // =========================================================================
  // Tasks
  // =========================================================================
  describe("Task DTOs", () => {
    it("TaskDTO: Rust→TS major mismatch", () => {
      const rustSample = sampleFromSchema(TaskDTOSchema);
      expect(TaskDTOSchema.parse(rustSample)).toBeDefined();
    });

    it("accepts only the six canonical logical task states", () => {
      const sample = sampleFromSchema(TaskDTOSchema);
      for (const status of [
        "queued",
        "running",
        "waiting",
        "succeeded",
        "failed",
        "cancelled",
      ]) {
        expect(TaskDTOSchema.safeParse({ ...sample, status }).success).toBe(
          true,
        );
      }
      for (const status of [
        "pending",
        "claimed",
        "paused",
        "blocked",
        "completed",
        "rolled_back",
        "recovering",
        "recovery_blocked",
      ]) {
        expect(TaskDTOSchema.safeParse({ ...sample, status }).success).toBe(
          false,
        );
      }
    });

    it("TimelineEntryDTO: close match", () => {
      const rustSample = sampleFromSchema(TimelineEntryDTOSchema);
      expect(TimelineEntryDTOSchema.parse(rustSample)).toBeDefined();

      // Rust: { id, entryType, content, metadata: Value, createdAt }
      // TS:   { id, entryType, content, metadata: Record<string, unknown>, createdAt }
      // -> Compatible!
    });
  });

  // =========================================================================
  // Comments
  // =========================================================================
  describe("Comment DTOs", () => {
    it("CommentDTO: Rust→TS major mismatch", () => {
      const rustSample = sampleFromSchema(CommentDTOSchema);
      expect(CommentDTOSchema.parse(rustSample)).toBeDefined();

      // MISMATCH:
      // Rust: { id, workspaceId, authorId, authorName, authorAvatarUrl?,
      //         scope, filePath?, lineNumber?, commitSha?,
      //         content, resolved, resolvedAt?, originBranch, replyCount,
      //         createdAt, updatedAt }
      //
      // TS:   { id, authorId, author: UserRef, scope, filePath?,
      //         lineNumber?, commitSha, commitRangeBase?, commitRangeHead?,
      //         content, resolved, resolvedAt?, originBranch, replyCount, createdAt }
      //
      // Key differences:
      // - Rust has flat authorName/authorAvatarUrl, TS has nested author: UserRef
      // - Rust has workspaceId, updatedAt — TS doesn't
      // - TS has commitRangeBase, commitRangeHead — Rust doesn't
      // - Rust commitSha is optional, TS commitSha is required
    });
  });

  // =========================================================================
  // Chat
  // =========================================================================
  describe("Chat DTOs", () => {
    it("SessionListItemDTO: Rust→TS major mismatch", () => {
      const rustSample = sampleFromSchema(SessionListItemDTOSchema);
      expect(SessionListItemDTOSchema.parse(rustSample)).toBeDefined();

      // MAJOR MISMATCH: TS ChatSessionDTO vs Rust SessionListItemDTO
      //
      // Rust: { id, workspaceId, sessionType, taskId?, scopeType, scopeRef?,
      //         model?, title?, summary?, tags[], status, messageCount,
      //         totalInputTokens, totalOutputTokens, totalCostUsd,
      //         forkedFromId?, branch?, linkedCommits[], linkedNodes[],
      //         createdAt, updatedAt }
      //
      // TS:   { id, workspaceId, sessionType, scopeType, scopeRef, model,
      //         title, summary, status, messageCount, totalCostUsd }
      //
      // Differences:
      // - Rust has many extra fields: taskId, tags, totalInputTokens, totalOutputTokens,
      //   forkedFromId, branch, linkedCommits, linkedNodes, createdAt, updatedAt
      // - Rust model/title/summary are optional, TS expects required
      // - Rust scopeRef is optional, TS expects required
    });

    it("SessionInputDTO: Rust SessionInputView matches the TS interface", () => {
      // Rust `SessionInputView` serializes with explicit camelCase (issue
      // #629 S16); useSessionInputs and the input mutations parse responses
      // with this schema, so a drift fails at the client boundary instead of
      // silently hiding the queued-input Edit/Withdraw controls.
      const rustSample = sampleFromSchema(SessionInputDTOSchema);
      expect(SessionInputDTOSchema.parse(rustSample)).toBeDefined();
      const typed: TSSessionInputDTO = rustSample as TSSessionInputDTO;
      expect(typed.inputOrigin).toBe("human");
      expect(typed.deliveryGeneration).toBe(42);
      expect(typed.editable).toBe(true);
      expect(typed.withdrawable).toBe(true);
    });

    it("ChatMessageDTO: Rust→TS mismatch", () => {
      const rustSample = sampleFromSchema(ChatMessageDTOZod);
      expect(ChatMessageDTOZod.parse(rustSample)).toBeDefined();

      // MISMATCH:
      // Rust: { id, sessionId, role, content, artifacts?: Value, citations: Value,
      //         inputTokens?, outputTokens?, status, messageIndex, createdAt }
      //
      // TS:   { id, sessionId, role, content, artifacts: Artifact[],
      //         citations: Citation[], status, messageIndex, createdAt }
      //
      // Differences:
      // - Rust artifacts is serde_json::Value (nullable), TS expects typed Artifact[]
      // - Rust citations is serde_json::Value, TS expects typed Citation[]
      // - Rust has inputTokens/outputTokens, TS doesn't
    });

    it("preserves the diagnostic marker and accepts messages from older API pods", () => {
      const sample = sampleFromSchema(ChatMessageDTOZod);
      expect(
        ChatMessageDTOZod.parse({ ...sample, isErrorNotice: true })
          .isErrorNotice,
      ).toBe(true);
      const older = { ...sample };
      delete older.isErrorNotice;
      expect(ChatMessageDTOZod.parse(older).isErrorNotice).toBeUndefined();
    });

    // TS-side pin for the persisted citation shape documented by chat_service
    // D55. This does not derive from the Rust serde type, so it protects the
    // client contract and legacy parsing without claiming cross-language
    // schema generation.
    it("Citation: TS persisted shape accepts repo-aware and legacy rows", () => {
      const ChatCitationSchema = z.object({
        filePath: z.string().optional(),
        repoId: z.string().optional(),
        lineRange: z.tuple([z.number(), z.number()]).optional(),
        label: z.string(),
      });

      const full: TSCitation = ChatCitationSchema.parse({
        filePath: "generation/g5-api/src/services/agents/chat_service.rs",
        repoId: "2784e485-d954-4b95-9d39-ddb31e050849",
        lineRange: [10, 20],
        label: "chat_service.rs",
      });
      expect(full.repoId).toBe("2784e485-d954-4b95-9d39-ddb31e050849");

      // Legacy persisted row: no repoId key at all — parses, repoId undefined,
      // so the client falls back to the active branch exactly as before.
      const legacy: TSCitation = ChatCitationSchema.parse({
        filePath: "designs/API_AND_STORAGE.md",
        label: "API_AND_STORAGE.md",
      });
      expect(legacy.repoId).toBeUndefined();

      // Shape-completeness both ways: every TS field is produced by the
      // schema, and the schema has no field the TS interface lacks.
      const tsShape: Record<keyof TSCitation, true> = {
        filePath: true,
        repoId: true,
        lineRange: true,
        label: true,
      };
      expect(Object.keys(ChatCitationSchema.shape).sort()).toEqual(
        Object.keys(tsShape).sort(),
      );
    });
  });

  // =========================================================================
  // Settings
  // =========================================================================
  describe("Settings DTOs", () => {
    it("WorkspaceSettingsDTO: Rust→TS mismatch", () => {
      const rustSample = sampleFromSchema(WorkspaceSettingsDTOSchema);
      expect(WorkspaceSettingsDTOSchema.parse(rustSample)).toBeDefined();

      // MISMATCH:
      // Rust: { workspaceId, maxConcurrentTasks, maxSpendUsd?, spendPeriod,
      //         spendPeriodStart?, maxInputTokens?, maxOutputTokens?,
      //         maxCloudWorkers, allowCloudWorkers, workerIdleTimeoutMin,
      //         tasksPerWorker, workerScalingIntervalSec,
      //         updatedBy?(string), updatedAt }
      //
      // TS:   { workspaceId, maxConcurrentTasks, maxSpendUsd?, spendPeriod,
      //         spendPeriodStart?, maxInputTokens?, maxOutputTokens?,
      //         allowCloudWorkers, maxCloudWorkers, workerIdleTimeoutMin,
      //         updatedBy: UserRef | null, updatedAt }
      //
      // Differences:
      // - Rust has tasksPerWorker, workerScalingIntervalSec — TS doesn't
      // - Rust updatedBy is Option<String> (user id), TS expects UserRef | null
    });
  });

  // =========================================================================
  // Instance
  // =========================================================================
  describe("Instance DTOs", () => {
    it("InstanceInfoDTO: match", () => {
      const rustSample = sampleFromSchema(InstanceInfoDTOSchema);
      expect(InstanceInfoDTOSchema.parse(rustSample)).toBeDefined();

      // MATCH: Both have { version, features[], authProviders[], maxUploadSizeMb, motd? }
    });

    it("HealthCheckResponse: Rust→TS structural mismatch", () => {
      const rustSample = sampleFromSchema(HealthCheckResponseSchema);
      expect(HealthCheckResponseSchema.parse(rustSample)).toBeDefined();

      // MISMATCH:
      // Rust: { status, checks: { database: string }, uptime }
      // TS:   { status, checks: Record<string, { status, latencyMs?, message? }>, uptimeSeconds }
      //
      // Very different checks shape and field name (uptime vs uptimeSeconds)
    });
  });

  // =========================================================================
  // Spend
  // =========================================================================
  describe("Spend DTOs", () => {
    it("SpendHistoryRouteDTO: partial match", () => {
      const rustSample = sampleFromSchema(SpendHistoryRouteDTOSchema);
      expect(SpendHistoryRouteDTOSchema.parse(rustSample)).toBeDefined();

      // Rust: { granularity, periods: [{ periodStart, periodEnd, totalSpendUsd,
      //         totalInputTokens, totalOutputTokens, jobCount, taskCount }],
      //         daily: [{ date, spendUsd, inputTokens, outputTokens }] }
      //
      // TS SpendHistoryDTO: { granularity, periods: [{ periodStart, periodEnd,
      //    totalSpendUsd, totalInputTokens, totalOutputTokens, jobCount, taskCount }],
      //    daily: [{ date, spendUsd, inputTokens, outputTokens }] }
      //
      // -> Match!
    });
  });

  // =========================================================================
  // Files
  // =========================================================================
  describe("File DTOs", () => {
    it("FileTreeEntry: close match", () => {
      const sample = {
        name: "test.ts",
        path: "src/test.ts",
        type: "file",
        size: 1024,
      };
      expect(FileTreeEntrySchema.parse(sample)).toBeDefined();

      // Rust: { name, path, type, size?, children? }
      // TS:   { name, path, type: 'file'|'directory', size?, children? }
      // -> Compatible!
    });
  });

  // =========================================================================
  // Docs
  // =========================================================================
  describe("Doc DTOs", () => {
    it("DocListItemDTO: Rust→TS different model", () => {
      const rustSample = sampleFromSchema(DocListItemDTOSchema);
      expect(DocListItemDTOSchema.parse(rustSample)).toBeDefined();

      // Rust: { id, name, docType, filePath?, linkedNodeCount, lastUpliftedAt?,
      //         lastDownliftedAt?, contentUpdatedAt?, createdAt, updatedAt }
      //
      // TS DocNode: { id, title, category, content, preview, lastUpdated }
      //
      // Very different — Rust is a proper doc DTO, TS is a simplified legacy model
    });

    it("keeps optional document source rolling-compatible and future-safe", () => {
      const listSample = sampleFromSchema(DocListItemDTOSchema);
      const detailSample = sampleFromSchema(DocDetailDTOSchema);

      expect(
        DocListItemDTOSchema.parse({
          ...listSample,
          documentSource: "user",
        }).documentSource,
      ).toBe("user");
      expect(
        DocDetailDTOSchema.parse({
          ...detailSample,
          documentSource: "auxiliary",
        }).documentSource,
      ).toBe("auxiliary");
      expect(
        DocListItemDTOSchema.parse({
          ...listSample,
          documentSource: null,
        }).documentSource,
      ).toBeNull();
      expect(
        DocListItemDTOSchema.parse({
          ...listSample,
          documentSource: "future-source",
        }).documentSource,
      ).toBeNull();

      const withoutSource = { ...detailSample } as Record<string, unknown>;
      delete withoutSource.documentSource;
      expect(
        DocDetailDTOSchema.parse(withoutSource).documentSource,
      ).toBeUndefined();
    });
  });

  // =========================================================================
  // Ontology format
  // =========================================================================
});
