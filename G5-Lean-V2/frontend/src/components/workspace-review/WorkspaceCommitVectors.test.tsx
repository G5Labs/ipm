import { expect, it } from "vitest";
import { screen, within } from "@testing-library/react";
import { renderApp } from "../../test/render";
import type {
  ReviewOverview,
  VectorRevision,
} from "../../hooks/useWorkspaceReviewFlow";
import { WorkspaceCommitVectors } from "./WorkspaceCommitVectors";

const head = {
  repoId: "code",
  workspaceId: "workspace",
  branchName: "feature",
  headCommit: "a".repeat(40),
};
const overview: ReviewOverview = {
  workspace: {
    ownerUserId: "owner",
    targetWorkspaceId: "parent",
    phase: "se",
    readOnly: false,
  },
  pr: null,
  heads: [head],
  baseHeads: null,
  parentHeads: null,
  pushedAt: {},
  repos: [{ id: "code", name: "Code", role: "code", rootPath: "code" }],
  requests: [],
  decisions: [],
};
const row: VectorRevision = {
  id: "recorded-vector",
  workspaceId: "workspace",
  observedAt: "2026-10-12T10:00:00Z",
  heads: [head],
  pushedAt: { code: "2026-10-11T09:00:00Z" },
};

it("marks the recorded current vector and preserves its member's actual push time", () => {
  renderApp(<WorkspaceCommitVectors rows={[row]} overview={overview} />);
  expect(screen.getByText("Current pushed state")).toBeVisible();
  expect(screen.getByLabelText("Pushed at")).toHaveAttribute(
    "datetime",
    row.pushedAt.code,
  );
  expect(screen.getByLabelText(`Commit ${head.headCommit}`)).toBeVisible();
});

it("keeps removed repository members in historical vectors and distinguishes missing members", () => {
  const old = {
    ...row,
    id: "older",
    heads: [{ ...head, repoId: "removed-docs", headCommit: "b".repeat(40) }],
    pushedAt: {},
  };
  renderApp(<WorkspaceCommitVectors rows={[row, old]} overview={overview} />);
  const rows = within(
    screen.getByRole("table", { name: "Workspace commit vectors" }),
  ).getAllByRole("row");
  expect(
    within(rows[0]).getByRole("columnheader", { name: "removed-docs" }),
  ).toBeVisible();
  expect(
    within(rows[1]).getByText("Not a member at this revision"),
  ).toBeVisible();
  expect(
    within(rows[2]).getByLabelText(`Commit ${"b".repeat(40)}`),
  ).toBeVisible();
  expect(within(rows[2]).getAllByText("Push time unknown")).toHaveLength(2);
});
