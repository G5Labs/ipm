import { Navigate } from "react-router-dom";
import { useGithubConnection } from "../../hooks/useGithubConnection";
import {
  githubConnectionPath,
  githubConnectionReturnPath,
} from "../../lib/githubConnection";
import { Button } from "../ui/UI";

export function GithubConnectionCard({
  returnTo,
  afterLogin = false,
  connectionFailed = false,
  redirect = (url) => window.location.assign(url),
}: {
  returnTo: string;
  afterLogin?: boolean;
  connectionFailed?: boolean;
  redirect?: (url: string) => void;
}) {
  const { status, connect, disconnect } = useGithubConnection();
  const destination = githubConnectionReturnPath(returnTo);
  if (afterLogin && status.data?.connected && !connectionFailed) {
    return <Navigate to={destination} replace />;
  }
  const startConnection = async () => {
    try {
      const callback =
        githubConnectionPath(destination) + (afterLogin ? "&afterLogin=1" : "");
      const { redirectUrl } = await connect.mutateAsync(callback);
      redirect(redirectUrl);
    } catch {
      // Display the error without disrupting the user's G5 session.
    }
  };
  const error = connect.error ?? disconnect.error ?? status.error;
  return (
    <section
      className="mb-4 rounded-[var(--g5-radius-md)] border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] p-4"
      aria-label="GitHub connection"
    >
      <h2 className="text-sm font-semibold text-[var(--g5-text0)]">
        GitHub connection
      </h2>
      <p className="mt-2 text-sm text-[var(--g5-text2)]">
        Create pull requests and post issue comments under your GitHub account.
        This connection is shared across G5 workspaces.
      </p>
      {status.isPending ? (
        <p role="status" className="mt-2 text-sm">
          Checking GitHub connection…
        </p>
      ) : (
        <>
          <p className="mt-2 text-sm">
            {status.isError
              ? "GitHub connection status is unavailable."
              : status.data?.connected
                ? `Connected as ${status.data.githubLogin}`
                : "GitHub is not connected."}
          </p>
          <div className="mt-3 flex flex-wrap gap-2">
            <Button
              kind="primary"
              disabled={connect.isPending || disconnect.isPending}
              onClick={() => void startConnection()}
            >
              {connect.isPending
                ? "Connecting…"
                : status.data?.connected
                  ? "Reconnect GitHub"
                  : "Connect GitHub"}
            </Button>
            {status.data?.connected && (
              <Button
                kind="secondary"
                disabled={connect.isPending || disconnect.isPending}
                onClick={() => disconnect.mutate()}
              >
                {disconnect.isPending ? "Disconnecting…" : "Disconnect GitHub"}
              </Button>
            )}
          </div>
        </>
      )}
      {connectionFailed && (
        <p role="alert" className="mt-2 text-sm text-[var(--g5-blocked)]">
          GitHub could not be connected. You can try again or continue using G5.
        </p>
      )}
      {error && (
        <p role="alert" className="mt-2 text-sm text-[var(--g5-blocked)]">
          {error.message}
        </p>
      )}
      {afterLogin && (
        <a
          className="mt-3 inline-block text-sm text-[var(--g5-info)] hover:underline"
          href={destination}
        >
          Continue without connecting
        </a>
      )}
    </section>
  );
}
