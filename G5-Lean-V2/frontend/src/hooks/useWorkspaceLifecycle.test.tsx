import React from "react";
import {
  act,
  render,
  renderHook,
  screen,
  waitFor,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import {
  QueryClient,
  QueryClientProvider,
  onlineManager,
} from "@tanstack/react-query";
import { http, HttpResponse } from "msw";
import { afterEach, describe, expect, it } from "vitest";
import { WorkbenchWorkspaceControls } from "../components/workbench/WorkbenchWorkspaceControls";
import { createMockWorkspaceVm } from "../test/fixtures";
import { server } from "../test/msw/server";
import { _activeTestClients } from "../test/render";
import {
  useResumeWorkspaceVm,
  useStopWorkspaceVm,
  useTransferWorkspace,
} from "./useWorkspace";

type Action = "resume" | "stop" | "transfer";

function useActions(workspaceId: string) {
  return {
    resume: useResumeWorkspaceVm(workspaceId),
    stop: useStopWorkspaceVm(workspaceId),
    transfer: useTransferWorkspace(workspaceId),
  };
}

function submit(actions: ReturnType<typeof useActions>, action: Action) {
  return action === "transfer"
    ? actions.transfer.mutateAsync({
        toUserId: "recipient",
        idempotencyKey: "transfer-key",
      })
    : actions[action].mutateAsync(undefined);
}

function setup() {
  const client = new QueryClient({
    defaultOptions: {
      queries: { retry: false, staleTime: Infinity },
      mutations: { retry: false },
    },
  });
  _activeTestClients.push(client);
  const wrapper = ({ children }: { children: React.ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
  const calls: { workspaceId: string; action: string; body: unknown }[] = [];
  const replies = new Map<string, (response: Response) => void>();
  server.use(
    http.post("/api/v1/workspaces/:id/:action", async ({ params, request }) => {
      const body = await request.json();
      calls.push({
        workspaceId: String(params.id),
        action: String(params.action),
        body,
      });
      return new Promise<Response>((resolve) =>
        replies.set(String(params.id), resolve),
      );
    }),
  );
  const complete = (id: string, failed = false) =>
    replies.get(id)!(
      failed
        ? HttpResponse.json(
            { error: { code: "FORBIDDEN", message: `${id} refused` } },
            { status: 403 },
          )
        : HttpResponse.json({
            data: {
              outcome: "queued_for_capacity",
              status: "completed",
              vm: createMockWorkspaceVm({
                workspaceId: id,
                status: "queued_for_capacity",
              }),
            },
          }),
    );
  return { client, wrapper, calls, complete };
}

afterEach(() => onlineManager.setOnline(true));

describe("workspace lifecycle request isolation", () => {
  for (const action of ["resume", "stop", "transfer"] as const) {
    for (const first of ["a", "b"]) {
      it(`${action}: retains each workspace across rapid navigation, ${first} completes first`, async () => {
        const { wrapper, calls, complete } = setup();
        const { result, rerender } = renderHook(({ id }) => useActions(id), {
          wrapper,
          initialProps: { id: "a" },
        });
        let a!: Promise<unknown>;
        act(() => {
          a = submit(result.current, action).catch((error) => error);
        });
        await waitFor(() => expect(calls).toHaveLength(1));
        rerender({ id: "b" });
        expect(result.current[action].isLoading).toBe(false);
        expect(result.current[action].error).toBeNull();
        expect(result.current[action].data).toBeUndefined();
        let b!: Promise<unknown>;
        act(() => {
          b = submit(result.current, action);
        });
        await waitFor(() => expect(calls).toHaveLength(2));
        rerender({ id: "a" });
        expect(result.current[action].isLoading).toBe(true);
        rerender({ id: "b" });
        expect(result.current[action].isLoading).toBe(true);
        await act(async () => {
          complete(first, first === "a");
          await (first === "a" ? a : b);
        });
        await waitFor(() =>
          expect(result.current[action].isLoading).toBe(first !== "b"),
        );
        expect(result.current[action].error).toBeNull();
        if (first === "b")
          expect(result.current[action].data).toMatchObject({
            vm: { workspaceId: "b" },
          });
        const last = first === "a" ? "b" : "a";
        await act(async () => {
          complete(last, last === "a");
          await (last === "a" ? a : b);
        });
        await waitFor(() =>
          expect(result.current[action].data).toMatchObject({
            vm: { workspaceId: "b" },
          }),
        );
        expect(result.current[action].error).toBeNull();
        rerender({ id: "a" });
        expect(result.current[action].isLoading).toBe(false);
        expect(result.current[action].error?.message).toBe("a refused");
        expect(result.current[action].data).toBeUndefined();
        act(() => result.current[action].reset());
        await waitFor(() => expect(result.current[action].error).toBeNull());
        rerender({ id: "b" });
        expect(result.current[action].data).toMatchObject({
          vm: { workspaceId: "b" },
        });
        if (action === "transfer")
          expect(calls.map((call) => call.body)).toEqual([
            { toUserId: "recipient", idempotencyKey: "transfer-key" },
            { toUserId: "recipient", idempotencyKey: "transfer-key" },
          ]);
      });
    }

    it(`${action}: settles the originating cache after navigation, including failures`, async () => {
      const { client, wrapper, calls, complete } = setup();
      const keys = [
        "workspace-vm",
        "workspace",
        ...(action === "transfer"
          ? []
          : ["workspace-file", "workspace-changes", "chat-sessions"]),
      ];
      for (const id of ["a", "b"])
        for (const key of keys) client.setQueryData([key, id], { id });
      const privateKeys = [
        "workspace-file",
        "workspace-changes",
        "workspace-preview",
        "file-tree",
        "file-content",
      ];
      if (action === "transfer") {
        for (const id of ["a", "b"])
          for (const key of privateKeys)
            client.setQueryData([key, id, "repo"], { id });
      }
      const { result, rerender } = renderHook(({ id }) => useActions(id), {
        wrapper,
        initialProps: { id: "a" },
      });
      let pending!: Promise<unknown>;
      act(() => {
        pending = submit(result.current, action).catch((error) => error);
      });
      await waitFor(() => expect(calls).toHaveLength(1));
      rerender({ id: "b" });
      await act(async () => {
        complete("a", true);
        await pending;
      });
      for (const key of keys) {
        expect(client.getQueryState([key, "a"])?.isInvalidated, key).toBe(true);
        expect(client.getQueryState([key, "b"])?.isInvalidated, key).toBe(
          false,
        );
      }
      expect(result.current[action].error).toBeNull();
      expect(result.current[action].data).toBeUndefined();
      if (action === "transfer") {
        for (const key of privateKeys) {
          expect(client.getQueryData([key, "a", "repo"]), key).toBeUndefined();
          expect(client.getQueryData([key, "b", "repo"]), key).toEqual({
            id: "b",
          });
        }
      }
    });

    it(`${action}: binds an offline submission before navigation and prevents same-workspace races`, async () => {
      const { wrapper, calls, complete } = setup();
      onlineManager.setOnline(false);
      const { result, rerender } = renderHook(({ id }) => useActions(id), {
        wrapper,
        initialProps: { id: "a" },
      });
      let pending!: Promise<unknown>;
      act(() => {
        pending = submit(result.current, action);
      });
      // Admission reads the cache synchronously, even before the busy render.
      for (const sibling of ["resume", "stop", "transfer"] as const) {
        await expect(submit(result.current, sibling)).rejects.toThrow(
          "already in flight",
        );
      }
      rerender({ id: "b" });
      act(() => onlineManager.setOnline(true));
      await waitFor(() => expect(calls).toHaveLength(1));
      expect(calls[0]).toMatchObject({ workspaceId: "a", action });
      await act(async () => {
        complete("a");
        await pending;
      });
      expect(result.current[action].data).toBeUndefined();
      rerender({ id: "a" });
      expect(result.current[action].data).toMatchObject({
        vm: { workspaceId: "a" },
      });
    });
  }

  for (const action of ["resume", "stop"] as const) {
    it(`${action}: real keyed controls stay enabled in B and busy on returning to A`, async () => {
      const { wrapper, calls, complete } = setup();
      function Controls({ id }: { id: string }) {
        const actions = useActions(id);
        return (
          <WorkbenchWorkspaceControls
            key={id}
            vm={{
              status: "ok",
              data: createMockWorkspaceVm({
                workspaceId: id,
                status: action === "stop" ? "running" : "stopped",
              }),
            }}
            ownerUserId="owner"
            members={[]}
            canControl
            canTransfer
            isBase={false}
            busy={Object.values(actions).some((mutation) => mutation.isLoading)}
            error={
              Object.values(actions).find((mutation) => mutation.error)?.error
                ?.message
            }
            onResume={() => actions.resume.mutate(undefined)}
            onStop={() => actions.stop.mutate(undefined)}
            onTransfer={(toUserId) =>
              actions.transfer.mutate({ toUserId, idempotencyKey: "key" })
            }
          />
        );
      }
      const button = () =>
        screen.getByTestId(
          `workspace-vm-${action === "resume" ? "start" : "stop"}`,
        );
      const { rerender, unmount } = render(<Controls id="a" />, { wrapper });
      await userEvent.click(button());
      await waitFor(() => expect(calls).toHaveLength(1));
      rerender(<Controls id="b" />);
      expect(button()).toBeEnabled();
      await userEvent.click(button());
      await waitFor(() => expect(calls).toHaveLength(2));
      rerender(<Controls id="a" />);
      expect(button()).toBeDisabled();
      // A remounted page also finds the outstanding action, without VM polling.
      unmount();
      render(<Controls id="a" />, { wrapper });
      expect(button()).toBeDisabled();
      act(() => complete("b"));
      expect(button()).toBeDisabled();
      act(() => complete("a", true));
      await waitFor(() => expect(button()).toBeEnabled());
      expect(screen.getByText("a refused")).toBeVisible();
    });
  }
});
