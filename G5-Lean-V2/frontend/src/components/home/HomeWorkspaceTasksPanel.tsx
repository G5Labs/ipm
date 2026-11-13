import {
  TaskStatusIndicator,
  taskStatusTitle,
} from "../shared/TaskStatusIndicator";
import { useInfiniteQuery } from "@tanstack/react-query";
import { useEffect, useMemo, useRef } from "react";
import { Link } from "react-router-dom";
import { workspaceTasksQueryOptions } from "../../hooks/useTasks";
import { taskRoutePath, workspacePagePath } from "../../lib/workspacePagePath";

export function useHomeWorkspaceTasks(workspaceId: string, enabled = true) {
  const options = workspaceTasksQueryOptions(workspaceId, 5);
  const tasksQuery = useInfiniteQuery({
    ...options,
    enabled: options.enabled && enabled,
  });
  const tasks = useMemo(
    () => [
      ...new Map(
        (tasksQuery.data?.pages.flatMap((page) => page.tasks) ?? []).map(
          (task) => [task.id, task],
        ),
      ).values(),
    ],
    [tasksQuery.data?.pages],
  );
  return { tasksQuery, tasks };
}

export function HomeWorkspaceTasksPanel({
  orgSlug,
  workspaceId,
  workspaceName,
  regionId,
  tasks: { tasksQuery, tasks },
  className,
}: {
  orgSlug: string;
  workspaceId: string;
  workspaceName: string;
  regionId: string;
  tasks: ReturnType<typeof useHomeWorkspaceTasks>;
  className?: string;
}) {
  const focusIndex = useRef<number | null>(null);
  const links = useRef(new Map<number, HTMLAnchorElement>());
  useEffect(() => {
    if (focusIndex.current !== null && tasks.length > focusIndex.current) {
      links.current.get(focusIndex.current)?.focus();
      focusIndex.current = null;
    }
  }, [tasks.length]);
  const loadMore = () => {
    focusIndex.current = tasks.length;
    void tasksQuery.fetchNextPage();
  };
  return (
    <div id={regionId} className={className}>
      {tasksQuery.isPending && <p role="status">Loading recent tasks…</p>}
      {tasksQuery.isError && tasks.length === 0 && (
        <p>
          Recent tasks unavailable.{" "}
          <button
            type="button"
            data-testid={`home-workspace-${workspaceId}-tasks-retry`}
            onClick={() => void tasksQuery.refetch()}
          >
            Retry
          </button>
        </p>
      )}
      {!tasksQuery.isPending && !tasksQuery.isError && tasks.length === 0 && (
        <p>No recent tasks.</p>
      )}
      <ul className="space-y-2 border-l border-[var(--g5-stroke)] pl-3">
        {tasks.map((task, index) => (
          <li key={task.id} data-testid="home-recent-task" className="text-xs">
            <Link
              ref={(element) => {
                if (element) links.current.set(index, element);
                else links.current.delete(index);
              }}
              to={
                workspacePagePath(orgSlug, workspaceName) +
                taskRoutePath(task.id)
              }
              className="font-medium hover:underline"
            >
              {task.name}
            </Link>{" "}
            <TaskStatusIndicator status={task.status} />
            <span className="ml-2 text-[var(--g5-text2)]">
              {taskStatusTitle(task.status)}
            </span>
            <time
              className="ml-2 text-[var(--g5-text2)]"
              dateTime={task.updatedAt}
            >
              {new Date(task.updatedAt).toLocaleString()}
            </time>
          </li>
        ))}
      </ul>
      {tasksQuery.isFetchNextPageError && (
        <p role="alert">
          More tasks could not be loaded.{" "}
          <button
            type="button"
            data-testid={`home-workspace-${workspaceId}-tasks-retry-more`}
            onClick={loadMore}
          >
            Retry
          </button>
        </p>
      )}
      {tasksQuery.hasNextPage && !tasksQuery.isFetchNextPageError && (
        <button
          type="button"
          data-testid={`home-workspace-${workspaceId}-tasks-show-more`}
          disabled={tasksQuery.isFetchingNextPage}
          onClick={loadMore}
          className="mt-2 text-xs text-[var(--g5-info)]"
        >
          {tasksQuery.isFetchingNextPage ? "Loading…" : "Show more tasks"}
        </button>
      )}
    </div>
  );
}
