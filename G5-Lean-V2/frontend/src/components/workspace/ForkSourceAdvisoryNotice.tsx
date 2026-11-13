// Non-blocking advisories about a fork source's remote-tracked repositories
// (g5.fork.source.advisory): held by a half-merged workspace PR, paused, or
// behind the provider. Forking is never refused for them.
import type { ForkSourceAdvisory } from "../../lib/types";
import { describePauseReason } from "./remoteTrackingStatus";

function describeAdvisory(advisory: ForkSourceAdvisory): string {
  switch (advisory.kind) {
    case "import_held":
      return `Auto-pull is held: workspace PR “${advisory.pullRequest?.title ?? "untitled"}” is merged on the provider for this repository but not for all of its repositories.`;
    case "paused":
      return `Tracking is paused: ${describePauseReason(advisory.pauseReason ?? "")}`;
    case "behind":
      return "The provider has commits this workspace has not imported yet.";
  }
}

export function ForkSourceAdvisoryNotice({
  advisories,
}: {
  advisories: ForkSourceAdvisory[];
}) {
  if (advisories.length === 0) {
    return null;
  }
  return (
    <div
      role="note"
      data-testid="fork-source-advisories"
      className="space-y-1 rounded-[var(--g5-radius-sm)] border border-[var(--g5-warn)]/40 bg-[var(--g5-warn)]/10 px-3 py-2 text-xs text-[var(--g5-text0)]"
    >
      <p className="font-medium">
        The fork starts from the current heads. Check these repositories:
      </p>
      <ul className="list-disc space-y-0.5 pl-4">
        {advisories.map((advisory) => (
          <li
            key={`${advisory.repoId}-${advisory.kind}`}
            data-testid="fork-source-advisory"
            data-kind={advisory.kind}
          >
            <span className="font-medium">{advisory.repoName}</span>:{" "}
            {describeAdvisory(advisory)}
          </li>
        ))}
      </ul>
    </div>
  );
}
