// =============================================================================
// Suggested first message for work on an issue
// =============================================================================
// Names the issue and points the agent at `g5d issue show`, which reads the
// issue's full text inside G5; the issue text itself is not copied into the
// message.
// =============================================================================

export function startMessageFromIssue(issue: {
  displayKey: string;
  title: string;
}): string {
  const title = issue.title.replace(/\s+/g, " ").trim();
  return `Work on ${issue.displayKey}: ${title}\n\nRead the issue with \`g5d issue show\` before you start.`;
}
