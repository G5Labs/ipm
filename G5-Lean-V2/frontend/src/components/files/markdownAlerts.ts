/** Shared labels and styles; each syntax keeps its established icon treatment. */
export const MARKDOWN_ALERTS: Record<
  string,
  { title: string; icon: string; style: string }
> = {
  note: { title: "Note", icon: "ℹ️", style: "note" },
  tip: { title: "Tip", icon: "💡", style: "tip" },
  important: { title: "Important", icon: "❗", style: "important" },
  warning: { title: "Warning", icon: "⚠️", style: "warning" },
  caution: { title: "Caution", icon: "🚨", style: "caution" },
  danger: { title: "Danger", icon: "🚨", style: "caution" },
};
