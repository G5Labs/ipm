import { useLocation } from "react-router-dom";
import { useInstanceInfo } from "../hooks/useSettings";
import { isGithubAppEnabled } from "../lib/githubAppFlags";
import { githubConnectionReturnPath } from "../lib/githubConnection";
import { GithubConnectionCard } from "../components/settings/GithubConnectionCard";

export function GithubConnectionPage() {
  const location = useLocation();
  const params = new URLSearchParams(location.search);
  const returnTo = githubConnectionReturnPath(params.get("returnTo"));
  const instance = useInstanceInfo();
  const enabled = instance.status === "ok" && isGithubAppEnabled(instance.data);
  return (
    <main className="mx-auto max-w-lg px-6 py-12">
      <h1 className="mb-6 text-xl font-semibold">Account settings</h1>
      {enabled ? (
        <GithubConnectionCard
          returnTo={returnTo}
          afterLogin={params.get("afterLogin") === "1"}
          connectionFailed={params.get("githubConnection") === "failed"}
        />
      ) : (
        <p className="mb-4">
          {instance.status === "loading"
            ? "Loading…"
            : "GitHub App connections are unavailable on this deployment."}
        </p>
      )}
      <a
        href={returnTo}
        className="text-sm text-[var(--g5-info)] hover:underline"
      >
        Continue to G5
      </a>
    </main>
  );
}
