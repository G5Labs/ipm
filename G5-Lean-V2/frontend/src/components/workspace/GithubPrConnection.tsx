import {
  githubConnectionPath,
  githubPrConnectionRequired,
} from "../../lib/githubConnection";

export function GithubPrConnection({
  errors,
}: {
  errors: (string | null | undefined)[];
}) {
  if (!errors.some(githubPrConnectionRequired)) return null;
  return (
    <a
      className="mt-2 inline-block text-xs text-[var(--g5-info)] hover:underline"
      href={githubConnectionPath(
        `${window.location.pathname}${window.location.search}`,
      )}
    >
      Manage GitHub connection in Account settings
    </a>
  );
}
