// Tab-local transcript preferences. These are disposable view state, never
// approval delivery evidence. Storage may be disabled or full.
const prefix = "g5:chat-view:v1:";
const capacity = 50;

export function readChatViewState<T>(kind: string, key: string): T | undefined {
  try {
    const records = JSON.parse(sessionStorage.getItem(prefix + kind) ?? "[]");
    if (!Array.isArray(records)) return undefined;
    return records.find(
      (record) => Array.isArray(record) && record[0] === key,
    )?.[1] as T | undefined;
  } catch {
    return undefined;
  }
}

export function writeChatViewState(kind: string, key: string, value: unknown) {
  try {
    const parsed = JSON.parse(sessionStorage.getItem(prefix + kind) ?? "[]");
    const records = (Array.isArray(parsed) ? parsed : []).filter(
      (record) => Array.isArray(record) && record[0] !== key,
    );
    const empty =
      value === undefined ||
      (value !== null &&
        typeof value === "object" &&
        !Array.isArray(value) &&
        Object.keys(value).length === 0);
    if (!empty) records.push([key, value]);
    else if (records.length === (Array.isArray(parsed) ? parsed.length : 0))
      return;
    sessionStorage.setItem(
      prefix + kind,
      JSON.stringify(records.slice(-capacity)),
    );
  } catch {
    // In-memory component/draft state still works without browser storage.
  }
}
