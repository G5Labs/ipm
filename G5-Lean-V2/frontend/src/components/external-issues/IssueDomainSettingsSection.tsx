// =============================================================================
// Issue settings — issue domain section (organization admins)
// =============================================================================
// Lists the organization's issue domains (GitHub repositories and the native
// G5 tracker) with their recorded availability, toggles whether a domain's
// issue content is trusted, removes a domain (existing workspace links are
// unaffected), adds a GitHub repository from the binding's covered
// repositories, and adds the native tracker ("Use G5 issues") with its issue
// key (g5.issue.native.domain). GitHub trust defaults off: untrusted content
// renders as restricted Markdown; native content defaults to trusted, since
// only members and their agents write it. A native domain exports its issues
// as JSON and cannot be removed while it has issues. Each domain's Labels
// opens its label catalog editor (g5.issue.labels.surfaces). While the
// organization has issues off, the section only lists domains so they can
// be removed.
// =============================================================================

import { useId, useMemo, useState } from "react";
import {
  CircleDot,
  Download,
  LoaderCircle,
  Plus,
  Tags,
  Trash2,
} from "lucide-react";

import { ApiError, api } from "../../lib/api";
import { routes } from "../../lib/routes";
import {
  NATIVE_ISSUE_KEY_MAX,
  isValidNativeIssueKey,
  suggestNativeIssueKey,
} from "../../lib/nativeIssues";
import { Button, Chip } from "../ui/UI";
import {
  useCreateIssueDomain,
  useDeleteIssueDomain,
  useIssueDomains,
  useUpdateIssueDomain,
} from "../../hooks/useExternalIssues";
import {
  describeDomainUnavailable,
  describeIssueRequestError,
  issueErrorCodeOf,
  type IssueErrorPresentation,
} from "../../lib/externalIssueErrors";
import type { GithubAppBindingDTO, IssueDomainDTO } from "../../lib/types";
import { IssueErrorNotice } from "./ExternalIssueParts";
import { IssueLabelCatalogEditor } from "./IssueLabelCatalogEditor";

export interface IssueDomainSettingsSectionProps {
  orgId: string;
  /** The organization's slug; the native issue key's suggestion comes from it. */
  orgSlug?: string;
  /** Latest recorded binding; candidates come from its covered repositories. */
  binding: GithubAppBindingDTO | null;
  /**
   * GitHub repositories can be added (the deployment has the GitHub App).
   * Without it only the native tracker is offered.
   */
  githubRepositories?: boolean;
  /**
   * Issues are off for the organization: domains can be removed, not added
   * or changed. The section is absent when there is none.
   */
  removeOnly?: boolean;
}

function describeDomainMutationError(error: unknown): IssueErrorPresentation {
  if (error instanceof ApiError && error.status === 409) {
    switch (error.details.code) {
      case "domain_has_issues":
        return {
          title:
            "This issue tracker still has issues, so it cannot be removed. Its issues would become unreachable; export them first if you need a copy.",
          detail: null,
        };
      case "domain_exists":
        return {
          title: "This organization already uses G5 issues.",
          detail: null,
        };
    }
    if (!issueErrorCodeOf(error)) {
      return {
        title: "This repository is already an issue domain.",
        detail: null,
      };
    }
  }
  return describeIssueRequestError(error);
}

/**
 * Download the native tracker's JSON export (issues, comments and events)
 * as a file. The route is administrator-only.
 */
async function downloadNativeExport(orgId: string, domain: IssueDomainDTO) {
  const response = await api.getRawResponse(
    routes.orgs.issueDomainExport(orgId, domain.id),
  );
  // Refusals (403 included) throw ApiError before this point.
  if (!response.ok) {
    throw new Error(`The export could not be downloaded (${response.status}).`);
  }
  const url = URL.createObjectURL(
    new Blob([await response.text()], { type: "application/json" }),
  );
  try {
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = `${domain.displayKey.toLowerCase()}-issues.json`;
    document.body.appendChild(anchor);
    anchor.click();
    anchor.remove();
  } finally {
    // Let the click start the download before the URL goes away.
    window.setTimeout(() => URL.revokeObjectURL(url), 0);
  }
}

function DomainRow({
  orgId,
  domain,
  onToggleTrust,
  onToggleComments,
  onRemove,
  busy,
  removeOnly,
}: {
  orgId: string;
  domain: IssueDomainDTO;
  onToggleTrust: (domain: IssueDomainDTO, trusted: boolean) => void;
  onToggleComments: (domain: IssueDomainDTO, enabled: boolean) => void;
  onRemove: (domain: IssueDomainDTO) => Promise<boolean>;
  busy: boolean;
  removeOnly: boolean;
}) {
  const trustId = useId();
  const commentsId = useId();
  const [confirming, setConfirming] = useState(false);
  const [labelsOpen, setLabelsOpen] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [exportError, setExportError] = useState<string | null>(null);
  const native = domain.provider === "g5";
  const exportIssues = async () => {
    setExporting(true);
    setExportError(null);
    try {
      await downloadNativeExport(orgId, domain);
    } catch (error) {
      setExportError(
        error instanceof ApiError && error.status === 403
          ? "Only organization administrators can export issues."
          : error instanceof Error
            ? error.message
            : "The export failed.",
      );
    } finally {
      setExporting(false);
    }
  };
  return (
    <li
      data-testid={`issue-domain-${domain.id}`}
      className="rounded-lg border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-3.5 py-3"
    >
      <div className="flex flex-wrap items-center gap-2">
        <span className="min-w-0 truncate text-sm font-medium text-[var(--g5-text0)]">
          {domain.displayKey}
        </span>
        <span
          data-testid="issue-domain-kind"
          className="text-xs text-[var(--g5-text2)]"
        >
          {native ? "G5 issues" : "GitHub repository"}
        </span>
        {domain.isDefaultCreateTarget && (
          <span className="rounded-full border border-[var(--g5-stroke)] px-1.5 text-[10px] text-[var(--g5-text2)]">
            Default
          </span>
        )}
        <Chip
          data-testid="issue-domain-availability"
          state={domain.available ? "ok" : "blocked"}
          text={domain.available ? "Available" : "Unavailable"}
        />
        <span className="flex-1" />
        {!removeOnly && (
          <>
            <label
              htmlFor={trustId}
              className="inline-flex cursor-pointer items-center gap-1.5 text-xs text-[var(--g5-text1)]"
            >
              <input
                id={trustId}
                type="checkbox"
                role="switch"
                data-testid="issue-domain-trusted-toggle"
                checked={domain.contentTrusted}
                disabled={busy}
                onChange={(event) =>
                  onToggleTrust(domain, event.target.checked)
                }
                className="h-4 w-4 accent-[var(--g5-info)]"
              />
              Trusted content
            </label>
            {/* Native comments never leave G5: no switch to open them. */}
            {!native && (
              <label
                htmlFor={commentsId}
                title="People comment, and create issues in the default repository, under their own GitHub account. G5 never posts to public repositories."
                className="inline-flex cursor-pointer items-center gap-1.5 text-xs text-[var(--g5-text1)]"
              >
                <input
                  id={commentsId}
                  type="checkbox"
                  role="switch"
                  data-testid="issue-domain-comments-toggle"
                  checked={domain.commentsEnabled}
                  disabled={busy}
                  onChange={(event) =>
                    onToggleComments(domain, event.target.checked)
                  }
                  className="h-4 w-4 accent-[var(--g5-info)]"
                />
                Allow G5 comments and issue creation
              </label>
            )}
          </>
        )}
        {!removeOnly && (
          <Button
            kind="ghost"
            onClick={() => setLabelsOpen((open) => !open)}
            aria-expanded={labelsOpen}
            data-testid="issue-domain-labels"
            aria-label={`Labels of ${domain.displayKey}`}
            className="px-2 py-1 text-xs"
          >
            <Tags aria-hidden="true" className="h-3.5 w-3.5" />
            Labels
          </Button>
        )}
        {native && (
          <Button
            kind="ghost"
            onClick={() => void exportIssues()}
            disabled={exporting}
            data-testid="issue-domain-export"
            aria-label={`Export issues of ${domain.displayKey}`}
            className="px-2 py-1 text-xs"
          >
            {exporting ? (
              <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <Download aria-hidden="true" className="h-3.5 w-3.5" />
            )}
            Export issues
          </Button>
        )}
        {!confirming && (
          <Button
            kind="ghost"
            onClick={() => setConfirming(true)}
            disabled={busy}
            data-testid="issue-domain-remove"
            aria-label={`Remove issue domain ${domain.displayKey}`}
            className="px-2 py-1 text-xs"
          >
            <Trash2 aria-hidden="true" className="h-3.5 w-3.5" />
            Remove
          </Button>
        )}
      </div>
      {exportError && (
        <p
          role="alert"
          data-testid="issue-domain-export-error"
          className="mt-1.5 text-xs text-[var(--g5-blocked)]"
        >
          {exportError}
        </p>
      )}
      {!domain.available && (
        <p
          data-testid="issue-domain-unavailable-reason"
          className="mt-1.5 text-xs text-[var(--g5-blocked)]"
        >
          {describeDomainUnavailable(domain.unavailableReason)}
        </p>
      )}
      {labelsOpen && !removeOnly && (
        <div
          data-testid="issue-domain-labels-panel"
          className="mt-3 border-t border-[var(--g5-stroke)] pt-3"
        >
          <IssueLabelCatalogEditor
            orgId={orgId}
            domain={{
              domainId: domain.id,
              displayKey: domain.displayKey,
              provider: domain.provider,
              contentTrusted: domain.contentTrusted,
            }}
            canEdit
          />
        </div>
      )}
      {confirming && (
        <div
          data-testid="issue-domain-remove-confirm"
          className="mt-2.5 flex flex-wrap items-center gap-2"
        >
          <p className="min-w-0 flex-1 text-xs text-[var(--g5-text1)]">
            {native
              ? `Stop using G5 issues (${domain.displayKey})? This works only while it has no issues.`
              : `Stop using ${domain.displayKey} as an issue domain? Existing workspace links are kept.`}
          </p>
          <Button
            kind="secondary"
            onClick={() => setConfirming(false)}
            data-testid="issue-domain-remove-cancel"
            className="px-3 py-1 text-xs"
          >
            Cancel
          </Button>
          <Button
            kind="primary"
            disabled={busy}
            onClick={() => {
              void onRemove(domain).then((ok) => {
                if (ok) setConfirming(false);
              });
            }}
            data-testid="issue-domain-remove-confirm-button"
            className="px-3 py-1 text-xs"
          >
            Confirm remove
          </Button>
        </div>
      )}
    </li>
  );
}

export function IssueDomainSettingsSection({
  orgId,
  orgSlug = "",
  binding,
  githubRepositories = true,
  removeOnly = false,
}: IssueDomainSettingsSectionProps) {
  const repoSelectId = useId();
  const trustCheckboxId = useId();
  const nativeKeyId = useId();
  const nativeTrustId = useId();
  const domains = useIssueDomains(orgId);
  const createDomain = useCreateIssueDomain(orgId);
  const updateDomain = useUpdateIssueDomain(orgId);
  const deleteDomain = useDeleteIssueDomain(orgId);
  const [selectedRepo, setSelectedRepo] = useState("");
  const [trustNew, setTrustNew] = useState(false);
  // "Use G5 issues": the key defaults from the slug; content defaults trusted.
  const [nativeKey, setNativeKey] = useState(() =>
    suggestNativeIssueKey(orgSlug),
  );
  const [nativeTrusted, setNativeTrusted] = useState(true);

  const domainList = useMemo(
    () => (domains.status === "ok" ? domains.data.domains : []),
    [domains],
  );
  const candidates = useMemo(() => {
    if (binding?.disposition !== "active") return [];
    const configured = new Set(
      domainList.map((domain) => domain.externalContainerId),
    );
    return binding.coveredRepositories.filter(
      (repository) => !configured.has(String(repository.githubRepoId)),
    );
  }, [binding, domainList]);
  const effectiveSelection = candidates.some(
    (repository) => String(repository.githubRepoId) === selectedRepo,
  )
    ? selectedRepo
    : "";

  const mutationError =
    createDomain.error ?? updateDomain.error ?? deleteDomain.error;
  const busy =
    createDomain.isLoading || updateDomain.isLoading || deleteDomain.isLoading;

  const resetErrors = () => {
    createDomain.reset();
    updateDomain.reset();
    deleteDomain.reset();
  };

  const addDomain = async () => {
    if (!effectiveSelection) return;
    resetErrors();
    try {
      await createDomain.mutateAsync({
        githubRepoId: effectiveSelection,
        contentTrusted: trustNew,
      });
      setSelectedRepo("");
      setTrustNew(false);
    } catch {
      // The error notice explains the failure.
    }
  };

  const hasNativeDomain = domainList.some((domain) => domain.provider === "g5");
  const nativeKeyValid = isValidNativeIssueKey(nativeKey);
  const addNativeDomain = async () => {
    if (!nativeKeyValid) return;
    resetErrors();
    try {
      await createDomain.mutateAsync({
        provider: "g5",
        key: nativeKey,
        contentTrusted: nativeTrusted,
      });
    } catch {
      // The error notice explains the failure.
    }
  };

  const toggleTrust = (domain: IssueDomainDTO, contentTrusted: boolean) => {
    resetErrors();
    updateDomain.mutate({ domainId: domain.id, contentTrusted });
  };

  const toggleComments = (domain: IssueDomainDTO, commentsEnabled: boolean) => {
    resetErrors();
    updateDomain.mutate({ domainId: domain.id, commentsEnabled });
  };

  const removeDomain = async (domain: IssueDomainDTO): Promise<boolean> => {
    resetErrors();
    try {
      await deleteDomain.mutateAsync(domain.id);
      return true;
    } catch {
      return false;
    }
  };

  // Off for the organization, the section appears only once there are
  // domains left to remove.
  if (removeOnly && domainList.length === 0) return null;

  return (
    <section
      data-testid="issue-domain-section"
      aria-labelledby="issue-domain-section-title"
      className="mt-5 rounded-xl border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] p-5"
    >
      <div className="flex items-start gap-2">
        <CircleDot
          aria-hidden="true"
          className="mt-0.5 h-4 w-4 text-[var(--g5-text2)]"
        />
        <div>
          <h2
            id="issue-domain-section-title"
            className="text-[15px] font-semibold text-[var(--g5-text0)]"
          >
            Issue domain
          </h2>
          {removeOnly ? (
            <p
              data-testid="issue-domain-remove-only"
              className="mt-1 max-w-2xl text-xs leading-relaxed text-[var(--g5-text2)]"
            >
              Issues are off for this organization. You can remove issue
              domains; turn issues on to add or change them.
            </p>
          ) : (
            <p className="mt-1 max-w-2xl text-xs leading-relaxed text-[var(--g5-text2)]">
              Where organization members search, read and link issues to
              workspaces: G5 issues, stored in G5, and GitHub repositories.
              Issue content that is not trusted renders as restricted Markdown.
            </p>
          )}
        </div>
      </div>

      {mutationError && (
        <div className="mt-4">
          <IssueErrorNotice
            testId="issue-domain-error"
            presentation={describeDomainMutationError(mutationError)}
          />
        </div>
      )}

      {domains.status === "loading" && (
        <p
          role="status"
          className="mt-4 text-xs text-[var(--g5-text2)]"
          data-testid="issue-domain-loading"
        >
          Loading issue domains…
        </p>
      )}
      {domains.status === "error" && (
        <div className="mt-4">
          <IssueErrorNotice
            testId="issue-domain-load-error"
            presentation={{
              title: "Failed to load issue domains.",
              detail: null,
            }}
          />
        </div>
      )}
      {domains.status === "ok" && domainList.length === 0 && (
        <p
          data-testid="issue-domain-empty"
          className="mt-4 text-xs text-[var(--g5-text2)]"
        >
          No issue domain is configured yet.
        </p>
      )}
      {domainList.length > 0 && (
        <ul data-testid="issue-domain-list" className="mt-4 space-y-2">
          {domainList.map((domain) => (
            <DomainRow
              key={domain.id}
              orgId={orgId}
              domain={domain}
              onToggleTrust={toggleTrust}
              onToggleComments={toggleComments}
              onRemove={removeDomain}
              busy={busy}
              removeOnly={removeOnly}
            />
          ))}
        </ul>
      )}

      {domains.status === "ok" && !removeOnly && !hasNativeDomain && (
        <form
          data-testid="issue-domain-native-form"
          aria-label="Use G5 issues"
          className="mt-5 flex flex-wrap items-end gap-3 border-t border-[var(--g5-stroke)] pt-4"
          onSubmit={(event) => {
            event.preventDefault();
            void addNativeDomain();
          }}
        >
          <div className="min-w-[220px] flex-1">
            <p className="text-xs font-medium text-[var(--g5-text1)]">
              G5 issues
            </p>
            <p className="mt-0.5 text-xs text-[var(--g5-text2)]">
              Track issues in G5 itself, beside any GitHub repositories. The key
              prefixes issue numbers ({nativeKeyValid ? nativeKey : "KEY"}
              -12) and cannot be changed later.
            </p>
          </div>
          <div>
            <label
              htmlFor={nativeKeyId}
              className="text-xs font-medium text-[var(--g5-text1)]"
            >
              Issue key
            </label>
            <input
              id={nativeKeyId}
              data-testid="issue-domain-native-key"
              value={nativeKey}
              maxLength={NATIVE_ISSUE_KEY_MAX}
              aria-invalid={!nativeKeyValid}
              onChange={(event) =>
                setNativeKey(
                  event.target.value.toUpperCase().replace(/[^A-Z0-9]/g, ""),
                )
              }
              className="mt-1 h-9 w-32 rounded border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-2 font-mono text-sm text-[var(--g5-text0)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]"
            />
          </div>
          <label
            htmlFor={nativeTrustId}
            className="inline-flex h-9 cursor-pointer items-center gap-1.5 text-xs text-[var(--g5-text1)]"
          >
            <input
              id={nativeTrustId}
              type="checkbox"
              data-testid="issue-domain-native-trusted"
              checked={nativeTrusted}
              onChange={(event) => setNativeTrusted(event.target.checked)}
              className="h-4 w-4 accent-[var(--g5-info)]"
            />
            Trust G5 issue content
          </label>
          <Button
            kind="primary"
            type="submit"
            disabled={busy || !nativeKeyValid}
            data-testid="issue-domain-native-add"
          >
            {createDomain.isLoading ? (
              <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <Plus aria-hidden="true" className="h-3.5 w-3.5" />
            )}
            Use G5 issues
          </Button>
          {!nativeKeyValid && (
            <p
              role="status"
              data-testid="issue-domain-native-key-invalid"
              className="w-full text-xs text-[var(--g5-blocked)]"
            >
              The key is 2–10 letters and digits and starts with a letter.
            </p>
          )}
        </form>
      )}

      {domains.status === "ok" && !removeOnly && githubRepositories && (
        <div className="mt-5 border-t border-[var(--g5-stroke)] pt-4">
          {binding?.disposition !== "active" ? (
            <p
              data-testid="issue-domain-needs-binding"
              className="text-xs text-[var(--g5-text2)]"
            >
              Connect the GitHub App and choose covered repositories to add a
              GitHub repository.
            </p>
          ) : candidates.length === 0 ? (
            <p
              data-testid="issue-domain-no-candidates"
              className="text-xs text-[var(--g5-text2)]"
            >
              {binding.coveredRepositories.length === 0
                ? "The GitHub App covers no repositories. Choose repositories above to add an issue domain."
                : "Every covered repository is already an issue domain."}
            </p>
          ) : (
            <form
              data-testid="issue-domain-add-form"
              aria-label="Add issue domain"
              className="flex flex-wrap items-end gap-3"
              onSubmit={(event) => {
                event.preventDefault();
                void addDomain();
              }}
            >
              <div className="min-w-[220px] flex-1">
                <label
                  htmlFor={repoSelectId}
                  className="text-xs font-medium text-[var(--g5-text1)]"
                >
                  Covered repository
                </label>
                <select
                  id={repoSelectId}
                  data-testid="issue-domain-repository"
                  value={effectiveSelection}
                  onChange={(event) => setSelectedRepo(event.target.value)}
                  className="mt-1 h-9 w-full rounded border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-2 text-sm text-[var(--g5-text0)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]"
                >
                  <option value="">Choose a repository…</option>
                  {candidates.map((repository) => (
                    <option
                      key={repository.githubRepoId}
                      value={String(repository.githubRepoId)}
                    >
                      {repository.fullName}
                    </option>
                  ))}
                </select>
              </div>
              <label
                htmlFor={trustCheckboxId}
                className="inline-flex h-9 cursor-pointer items-center gap-1.5 text-xs text-[var(--g5-text1)]"
              >
                <input
                  id={trustCheckboxId}
                  type="checkbox"
                  data-testid="issue-domain-new-trusted"
                  checked={trustNew}
                  onChange={(event) => setTrustNew(event.target.checked)}
                  className="h-4 w-4 accent-[var(--g5-info)]"
                />
                Trust issue content
              </label>
              <Button
                kind="primary"
                type="submit"
                disabled={busy || !effectiveSelection}
                data-testid="issue-domain-add"
              >
                {createDomain.isLoading ? (
                  <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
                ) : (
                  <Plus aria-hidden="true" className="h-3.5 w-3.5" />
                )}
                Add issue domain
              </Button>
            </form>
          )}
        </div>
      )}
    </section>
  );
}
