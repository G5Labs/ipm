/** Small presentation rules shared by Claude and Codex conversation activity. */
export interface ActivityDisplay {
  description?: string;
  path?: string;
  paths?: string[];
  command?: string;
}

export function activityLabel(name: string, display: ActivityDisplay): string {
  if (display.description && display.description !== name) {
    return display.description;
  }
  if (display.path) {
    const filename = display.path.split(/[\\/]/).filter(Boolean).at(-1);
    const action = /^(read)$/i.test(name)
      ? "Read"
      : /^(write)$/i.test(name)
        ? "Write"
        : /^(edit|multiedit|file_change|apply_patch|patch)$/i.test(name)
          ? "Edit"
          : name;
    return `${action} ${filename ?? display.path}`;
  }
  return display.command?.trim() || name.replace(/_+/g, " ");
}

export function activitySummary(
  tools: { toolName: string; display: ActivityDisplay; status: string }[],
): string {
  if (tools.length === 1) {
    return activityLabel(tools[0].toolName, tools[0].display);
  }
  const commands = tools.filter((tool) =>
    /^(bash|shell|command_execution|exec_command)$/i.test(tool.toolName),
  ).length;
  const reads = tools.filter((tool) => /^read$/i.test(tool.toolName));
  const edits = tools.filter((tool) =>
    /^(write|edit|multiedit|file_change|apply_patch|patch)$/i.test(
      tool.toolName,
    ),
  );
  const other = tools.length - commands - reads.length - edits.length;
  const completed = tools.every((tool) =>
    ["done", "error"].includes(tool.status),
  );
  const parts: string[] = [];
  if (commands) {
    parts.push(
      `${completed ? "Ran " : ""}${commands} command${commands === 1 ? "" : "s"}`,
    );
  }
  if (reads.length) {
    const paths = new Set(reads.map((tool) => tool.display.path));
    const path = reads[0].display.path;
    parts.push(
      completed && paths.size === 1 && path
        ? `Read ${path.split(/[\\/]/).filter(Boolean).at(-1)}`
        : `${reads.length} file read${reads.length === 1 ? "" : "s"}`,
    );
  }
  if (edits.length) {
    const paths = new Set(edits.flatMap((tool) => tool.display.paths ?? []));
    const knownSuccessfulFiles = edits.every(
      (tool) => tool.status === "done" && tool.display.paths?.length,
    );
    parts.push(
      knownSuccessfulFiles
        ? `Updated ${paths.size} file${paths.size === 1 ? "" : "s"}`
        : `${edits.length} file operation${edits.length === 1 ? "" : "s"}`,
    );
  }
  if (other) parts.push(`${other} other action${other === 1 ? "" : "s"}`);
  return parts.join(", ");
}

/** Label for one kind of background work Claude reports. */
export function backgroundTaskKindLabel(kind: string): string {
  if (kind === "shell") return "Command";
  if (kind === "agent") return "Agent";
  return "Task";
}

/**
 * Elapsed time of a background wait: `42s`, `1m 05s`, then `2h 03m` once it
 * passes an hour. A start ahead of the local clock reads as `0s`.
 */
export function formatElapsedWait(milliseconds: number): string {
  const totalSeconds = Number.isFinite(milliseconds)
    ? Math.max(0, Math.floor(milliseconds / 1000))
    : 0;
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  if (hours > 0) return `${hours}h ${String(minutes).padStart(2, "0")}m`;
  if (minutes > 0) return `${minutes}m ${String(seconds).padStart(2, "0")}s`;
  return `${seconds}s`;
}
