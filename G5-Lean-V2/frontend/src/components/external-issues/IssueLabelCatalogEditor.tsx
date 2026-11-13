// =============================================================================
// Label catalog editor — one issue domain's labels (g5.issue.labels.surfaces)
// =============================================================================
// Lists a domain's labels and, for members with write permission, creates,
// edits (name, colour, description) and deletes them. Reached from the label
// picker's "Manage labels" and from each domain in the organization's Issues
// settings; viewers read only. Creating a label happens only here: the
// picker never adds a name the catalog lacks.
//
// The editor is bound to the domain it was opened for: every write names
// that domain, whatever the creation target becomes meanwhile. A GitHub
// domain's labels change on GitHub as the person's connected account, under
// the same guards as their comments; on an untrusted domain each create or
// edit shows exactly what is sent before sending, and a delete is always
// confirmed (it removes the label from every issue too). A name another
// label already has is refused with that label shown; nothing is
// overwritten. When GitHub does not answer, the outcome is unknown: the
// person reloads the catalog before trying again. Catalog edits are
// last-writer-wins.
// =============================================================================

import { useId, useState } from "react";
import { LoaderCircle, Pencil, Plus, Trash2 } from "lucide-react";

import { Button, Modal, classNames } from "../ui/UI";
import {
  useCreateIssueLabel,
  useDeleteIssueLabel,
  useEditIssueLabel,
  useIssueLabelCatalog,
  type IssueLabelCatalog,
} from "../../hooks/useExternalIssues";
import { describeIssueRequestError } from "../../lib/externalIssueErrors";
import {
  MAX_LABEL_DESCRIPTION_CHARS,
  MAX_LABEL_NAME_CHARS,
  describeLabelError,
  existingLabelOf,
  labelDescriptionProblem,
  labelNameProblem,
  labelSwatchColor,
  normalizeLabelColor,
} from "../../lib/issueLabels";
import type {
  ExternalIssueLabelDTO,
  IssueProvider,
  LabelWriteDTO,
} from "../../lib/types";
import { IssueErrorNotice, IssueLabels } from "./ExternalIssueParts";

const FIELD_CLASSES =
  "w-full rounded-md border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-2.5 py-1.5 text-sm text-[var(--g5-text0)] placeholder:text-[var(--g5-text2)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)] disabled:opacity-60";

const PREVIEW_CLASSES =
  "whitespace-pre-wrap rounded-md border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-2.5 py-1.5 text-sm text-[var(--g5-text0)]";

/** The domain a picker or catalog editor is bound to. */
export interface IssueLabelDomain {
  domainId: string;
  /** `owner/name` or the native key; the catalog's own answer wins. */
  displayKey: string;
  provider: IssueProvider;
  /** An untrusted GitHub domain reviews each catalog write first. */
  contentTrusted: boolean;
}

/** GitHub did not answer: the catalog must be read again before retrying. */
export const LABEL_WRITE_UNKNOWN_MESSAGE =
  "GitHub did not confirm the change, so it may or may not have applied. Reload the labels to see the catalog before trying again.";

export function LabelSwatch({
  color,
  className,
}: {
  color: string | null | undefined;
  className?: string;
}) {
  const swatch = labelSwatchColor(color);
  return (
    <span
      aria-hidden="true"
      className={classNames(
        "inline-block h-3 w-3 shrink-0 rounded-full border border-[var(--g5-stroke)]",
        className,
      )}
      style={swatch ? { backgroundColor: swatch } : undefined}
    />
  );
}

interface LabelFieldValues {
  name: string;
  color: string;
  description: string;
}

function LabelFields({
  values,
  onChange,
  disabled,
}: {
  values: LabelFieldValues;
  onChange: (values: LabelFieldValues) => void;
  disabled: boolean;
}) {
  const nameId = useId();
  const colorId = useId();
  const descriptionId = useId();
  const nameProblem = values.name === "" ? null : labelNameProblem(values.name);
  const colorProblem =
    values.color.trim() !== "" && normalizeLabelColor(values.color) === null
      ? "A colour is six hex digits, such as d73a4a."
      : null;
  const descriptionProblem = labelDescriptionProblem(values.description);
  return (
    <div className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_8rem]">
      <div>
        <label
          htmlFor={nameId}
          className="mb-0.5 block text-xs font-medium text-[var(--g5-text1)]"
        >
          Name
        </label>
        <input
          id={nameId}
          type="text"
          data-testid="label-form-name"
          maxLength={MAX_LABEL_NAME_CHARS}
          value={values.name}
          disabled={disabled}
          aria-invalid={nameProblem !== null}
          onChange={(event) =>
            onChange({
              ...values,
              name: event.target.value.replace(/[\r\n]+/g, " "),
            })
          }
          className={FIELD_CLASSES}
        />
      </div>
      <div>
        <label
          htmlFor={colorId}
          className="mb-0.5 block text-xs font-medium text-[var(--g5-text1)]"
        >
          Colour
        </label>
        <div className="flex items-center gap-1.5">
          <LabelSwatch color={values.color} className="h-4 w-4" />
          <input
            id={colorId}
            type="text"
            data-testid="label-form-color"
            maxLength={7}
            placeholder="d73a4a"
            value={values.color}
            disabled={disabled}
            aria-invalid={colorProblem !== null}
            onChange={(event) =>
              onChange({ ...values, color: event.target.value })
            }
            className={classNames(FIELD_CLASSES, "font-[var(--g5-font-mono)]")}
          />
        </div>
      </div>
      <div className="sm:col-span-2">
        <label
          htmlFor={descriptionId}
          className="mb-0.5 block text-xs font-medium text-[var(--g5-text1)]"
        >
          Description{" "}
          <span className="font-normal text-[var(--g5-text2)]">(optional)</span>
        </label>
        <input
          id={descriptionId}
          type="text"
          data-testid="label-form-description"
          maxLength={MAX_LABEL_DESCRIPTION_CHARS}
          value={values.description}
          disabled={disabled}
          onChange={(event) =>
            onChange({
              ...values,
              description: event.target.value.replace(/[\r\n]+/g, " "),
            })
          }
          className={FIELD_CLASSES}
        />
      </div>
      {(nameProblem || colorProblem || descriptionProblem) && (
        <p
          role="status"
          data-testid="label-form-invalid"
          className="text-xs text-[var(--g5-blocked)] sm:col-span-2"
        >
          {nameProblem ?? colorProblem ?? descriptionProblem}
        </p>
      )}
    </div>
  );
}

function fieldsValid(values: LabelFieldValues): boolean {
  return (
    labelNameProblem(values.name) === null &&
    (values.color.trim() === "" ||
      normalizeLabelColor(values.color) !== null) &&
    labelDescriptionProblem(values.description) === null
  );
}

/** A refusal, with the label that already has the name when that is why. */
function LabelWriteError({
  error,
  fallback,
}: {
  error: Error;
  fallback: string;
}) {
  const existing = existingLabelOf(error);
  return (
    <div
      role="alert"
      data-testid="label-form-error"
      className="text-xs text-[var(--g5-blocked)]"
    >
      <p>{describeLabelError(error, fallback)}</p>
      {existing && (
        <div
          data-testid="label-form-existing"
          className="mt-1 flex flex-wrap items-center gap-2 text-[var(--g5-text1)]"
        >
          <span>Existing label:</span>
          <IssueLabels labels={[existing]} testId="label-form-existing-chip" />
          {existing.description && (
            <span className="text-[var(--g5-text2)]">
              {existing.description}
            </span>
          )}
        </div>
      )}
    </div>
  );
}

function UnknownOutcome({
  result,
  onReload,
}: {
  result: LabelWriteDTO;
  onReload: () => void;
}) {
  return (
    <div
      role="status"
      data-testid="label-form-unconfirmed"
      className="flex flex-wrap items-center gap-2 text-xs text-[var(--g5-warn)]"
    >
      <p className="min-w-0 flex-1">
        {LABEL_WRITE_UNKNOWN_MESSAGE}
        {result.message && (
          <span className="text-[var(--g5-text2)]"> ({result.message})</span>
        )}
      </p>
      <Button
        kind="secondary"
        data-testid="label-form-reload"
        onClick={onReload}
        className="px-2.5 py-1 text-xs"
      >
        Reload labels
      </Button>
    </div>
  );
}

/** What a create or edit sends, as an untrusted domain's review shows it. */
function LabelReview({
  heading,
  fields,
}: {
  heading: string;
  fields: { name?: string; color?: string; description?: string };
}) {
  return (
    <div data-testid="label-form-review" className="space-y-1.5">
      <p className="text-xs text-[var(--g5-text2)]">{heading}</p>
      {fields.name !== undefined && (
        <div>
          <p className="text-xs font-medium text-[var(--g5-text1)]">Name</p>
          <pre className={PREVIEW_CLASSES}>{fields.name}</pre>
        </div>
      )}
      {fields.color !== undefined && (
        <div>
          <p className="text-xs font-medium text-[var(--g5-text1)]">Colour</p>
          <pre className={PREVIEW_CLASSES}>#{fields.color}</pre>
        </div>
      )}
      {fields.description !== undefined && (
        <div>
          <p className="text-xs font-medium text-[var(--g5-text1)]">
            Description
          </p>
          <pre className={PREVIEW_CLASSES}>
            {fields.description || "(none)"}
          </pre>
        </div>
      )}
    </div>
  );
}

/**
 * A create or edit form: the fields, an untrusted domain's review step,
 * and the write's refusal or unknown outcome. `fields` is what is sent.
 */
function LabelWriteForm({
  label,
  values,
  onValues,
  fields,
  canSave,
  reviewHeading,
  pending,
  error,
  errorFallback,
  unknown,
  onReload,
  onSave,
  onCancel,
  saveText,
  testId,
}: {
  label: string;
  values: LabelFieldValues;
  onValues: (values: LabelFieldValues) => void;
  fields: { name?: string; color?: string; description?: string };
  canSave: boolean;
  /** Set on an untrusted GitHub domain: review exactly what is sent first. */
  reviewHeading: string | null;
  pending: boolean;
  error: Error | null;
  errorFallback: string;
  unknown: LabelWriteDTO | null;
  onReload: () => void;
  onSave: () => void;
  onCancel: () => void;
  saveText: string;
  testId: string;
}) {
  const [reviewing, setReviewing] = useState(false);
  return (
    <form
      data-testid={testId}
      aria-label={label}
      className="space-y-2 rounded-md border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] p-2.5"
      onSubmit={(event) => {
        event.preventDefault();
        // The editor opens over the issue page's label form or the create
        // dialog; React bubbles a submit through the portal, which would
        // submit that form too.
        event.stopPropagation();
        if (!canSave) return;
        if (reviewHeading !== null && !reviewing) setReviewing(true);
        else onSave();
      }}
    >
      {reviewing && reviewHeading !== null ? (
        <LabelReview heading={reviewHeading} fields={fields} />
      ) : (
        <LabelFields
          values={values}
          onChange={(next) => {
            setReviewing(false);
            onValues(next);
          }}
          disabled={pending}
        />
      )}
      {unknown && <UnknownOutcome result={unknown} onReload={onReload} />}
      {error && <LabelWriteError error={error} fallback={errorFallback} />}
      <div className="flex flex-wrap justify-end gap-2">
        <Button
          kind="ghost"
          data-testid="label-form-cancel"
          disabled={pending}
          onClick={onCancel}
          className="px-2.5 py-1 text-xs"
        >
          Cancel
        </Button>
        {reviewing && (
          <Button
            kind="secondary"
            data-testid="label-form-back"
            disabled={pending}
            onClick={() => setReviewing(false)}
            className="px-2.5 py-1 text-xs"
          >
            Edit
          </Button>
        )}
        <Button
          kind="primary"
          type="submit"
          data-testid={
            reviewHeading !== null && !reviewing
              ? "label-form-review-button"
              : "label-form-save"
          }
          disabled={!canSave || pending}
          className="px-2.5 py-1 text-xs"
        >
          {pending && <LoaderCircle className="h-3 w-3 animate-spin" />}
          {reviewHeading !== null && !reviewing ? "Review" : saveText}
        </Button>
      </div>
    </form>
  );
}

function CreateLabelForm({
  orgId,
  domain,
  displayKey,
  onDone,
  onReload,
}: {
  orgId: string;
  domain: IssueLabelDomain;
  displayKey: string;
  onDone: () => void;
  onReload: () => void;
}) {
  const createLabel = useCreateIssueLabel(orgId);
  const [values, setValues] = useState<LabelFieldValues>({
    name: "",
    color: "",
    description: "",
  });
  const [unknown, setUnknown] = useState<LabelWriteDTO | null>(null);
  const color = normalizeLabelColor(values.color);
  const fields = {
    name: values.name.trim(),
    ...(color ? { color } : {}),
    ...(values.description.trim()
      ? { description: values.description.trim() }
      : {}),
  };
  const save = async () => {
    setUnknown(null);
    try {
      const result = await createLabel.mutateAsync({
        domainId: domain.domainId,
        ...fields,
      });
      if (result.outcome === "unknown") {
        setUnknown(result);
        return;
      }
      onDone();
    } catch {
      // Explained in the form; the values are kept.
    }
  };
  return (
    <LabelWriteForm
      label="New label"
      testId="label-create-form"
      values={values}
      onValues={(next) => {
        setValues(next);
        setUnknown(null);
        createLabel.reset();
      }}
      fields={fields}
      canSave={fieldsValid(values)}
      reviewHeading={
        reviewsWrites(domain)
          ? `This label will be created in ${displayKey} on GitHub as your connected GitHub account.`
          : null
      }
      pending={createLabel.isLoading}
      error={createLabel.error}
      errorFallback="The label could not be created."
      unknown={unknown}
      onReload={onReload}
      onSave={() => void save()}
      onCancel={onDone}
      saveText="Create label"
    />
  );
}

function EditLabelForm({
  orgId,
  domain,
  displayKey,
  label,
  onDone,
  onReload,
}: {
  orgId: string;
  domain: IssueLabelDomain;
  displayKey: string;
  label: ExternalIssueLabelDTO & { id: string };
  onDone: () => void;
  onReload: () => void;
}) {
  const editLabel = useEditIssueLabel(orgId);
  const [values, setValues] = useState<LabelFieldValues>({
    name: label.name,
    color: label.color ?? "",
    description: label.description ?? "",
  });
  const [unknown, setUnknown] = useState<LabelWriteDTO | null>(null);
  // Only changed fields are sent; an emptied colour keeps the old one.
  const color = normalizeLabelColor(values.color);
  const fields = {
    ...(values.name.trim() !== label.name ? { name: values.name.trim() } : {}),
    ...(color && color !== label.color ? { color } : {}),
    ...(values.description.trim() !== (label.description ?? "")
      ? { description: values.description.trim() }
      : {}),
  };
  const changed = Object.keys(fields).length > 0;
  const save = async () => {
    setUnknown(null);
    try {
      const result = await editLabel.mutateAsync({
        domainId: domain.domainId,
        labelId: label.id,
        ...fields,
      });
      if (result.outcome === "unknown") {
        setUnknown(result);
        return;
      }
      onDone();
    } catch {
      // Explained in the form; the values are kept.
    }
  };
  return (
    <LabelWriteForm
      label={`Edit label ${label.name}`}
      testId="label-edit-form"
      values={values}
      onValues={(next) => {
        setValues(next);
        setUnknown(null);
        editLabel.reset();
      }}
      fields={fields}
      canSave={changed && fieldsValid(values)}
      reviewHeading={
        reviewsWrites(domain)
          ? `These changes will be saved to the label “${label.name}” in ${displayKey} on GitHub as your connected GitHub account.`
          : null
      }
      pending={editLabel.isLoading}
      error={editLabel.error}
      errorFallback="The label could not be saved."
      unknown={unknown}
      onReload={onReload}
      onSave={() => void save()}
      onCancel={onDone}
      saveText="Save"
    />
  );
}

/** Delete only after confirming: the label leaves every issue too. */
function DeleteLabelConfirm({
  orgId,
  domain,
  displayKey,
  label,
  onDone,
  onReload,
}: {
  orgId: string;
  domain: IssueLabelDomain;
  displayKey: string;
  label: ExternalIssueLabelDTO & { id: string };
  onDone: () => void;
  onReload: () => void;
}) {
  const deleteLabel = useDeleteIssueLabel(orgId);
  const [unknown, setUnknown] = useState<LabelWriteDTO | null>(null);
  const confirm = async () => {
    setUnknown(null);
    try {
      const result = await deleteLabel.mutateAsync({
        domainId: domain.domainId,
        labelId: label.id,
      });
      if (result.outcome === "unknown") {
        setUnknown(result);
        return;
      }
      onDone();
    } catch {
      // Explained below.
    }
  };
  return (
    <div
      data-testid="label-catalog-delete-confirm"
      className="space-y-2 rounded-md border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] p-2.5"
    >
      <p className="text-xs text-[var(--g5-text1)]">
        Delete the label “{label.name}” from {displayKey}? It is also removed
        from every issue that has it
        {domain.provider === "github" ? ", on GitHub as your account" : ""}.
        This cannot be undone.
      </p>
      {unknown && <UnknownOutcome result={unknown} onReload={onReload} />}
      {deleteLabel.error && (
        <LabelWriteError
          error={deleteLabel.error}
          fallback="The label could not be deleted."
        />
      )}
      <div className="flex flex-wrap justify-end gap-2">
        <Button
          kind="ghost"
          data-testid="label-catalog-delete-cancel"
          disabled={deleteLabel.isLoading}
          onClick={onDone}
          className="px-2.5 py-1 text-xs"
        >
          Cancel
        </Button>
        <Button
          kind="primary"
          data-testid="label-catalog-delete-confirm-button"
          disabled={deleteLabel.isLoading}
          onClick={() => void confirm()}
          className="px-2.5 py-1 text-xs"
        >
          {deleteLabel.isLoading && (
            <LoaderCircle className="h-3 w-3 animate-spin" />
          )}
          Delete label
        </Button>
      </div>
    </div>
  );
}

/** An untrusted GitHub domain reviews each catalog write before sending. */
function reviewsWrites(domain: IssueLabelDomain): boolean {
  return domain.provider === "github" && !domain.contentTrusted;
}

function hasId(
  label: ExternalIssueLabelDTO,
): label is ExternalIssueLabelDTO & { id: string } {
  return typeof label.id === "string" && label.id !== "";
}

function CatalogList({
  orgId,
  domain,
  catalog,
  canEdit,
  onReload,
}: {
  orgId: string;
  domain: IssueLabelDomain;
  catalog: IssueLabelCatalog;
  canEdit: boolean;
  onReload: () => void;
}) {
  // One row at a time is edited or confirmed, by label id.
  const [action, setAction] = useState<{
    kind: "edit" | "delete";
    labelId: string;
  } | null>(null);
  const displayKey = catalog.displayKey || domain.displayKey;
  if (catalog.labels.length === 0) {
    return (
      <p
        data-testid="label-catalog-empty"
        className="text-xs text-[var(--g5-text2)]"
      >
        {displayKey} has no labels yet.
      </p>
    );
  }
  return (
    <ul data-testid="label-catalog-list" className="space-y-1.5">
      {catalog.labels.map((label) => {
        const active =
          action && hasId(label) && action.labelId === label.id
            ? action.kind
            : null;
        return (
          <li
            key={label.id ?? label.name}
            data-testid={`label-catalog-row-${label.name}`}
            className="rounded-md px-1 py-1"
          >
            {active === "edit" && hasId(label) ? (
              <EditLabelForm
                orgId={orgId}
                domain={domain}
                displayKey={displayKey}
                label={label}
                onDone={() => setAction(null)}
                onReload={onReload}
              />
            ) : (
              <div className="flex items-start gap-2">
                <LabelSwatch color={label.color} className="mt-1" />
                <div className="min-w-0 flex-1">
                  <p className="text-sm text-[var(--g5-text0)]">{label.name}</p>
                  {label.description && (
                    <p className="text-xs text-[var(--g5-text2)]">
                      {label.description}
                    </p>
                  )}
                </div>
                {canEdit && hasId(label) && active === null && (
                  <>
                    <Button
                      kind="ghost"
                      data-testid={`label-catalog-edit-${label.name}`}
                      aria-label={`Edit label ${label.name}`}
                      onClick={() =>
                        setAction({ kind: "edit", labelId: label.id })
                      }
                      className="px-1.5 py-0.5 text-xs"
                    >
                      <Pencil aria-hidden="true" className="h-3 w-3" />
                      Edit
                    </Button>
                    <Button
                      kind="ghost"
                      data-testid={`label-catalog-delete-${label.name}`}
                      aria-label={`Delete label ${label.name}`}
                      onClick={() =>
                        setAction({ kind: "delete", labelId: label.id })
                      }
                      className="px-1.5 py-0.5 text-xs"
                    >
                      <Trash2 aria-hidden="true" className="h-3 w-3" />
                      Delete
                    </Button>
                  </>
                )}
              </div>
            )}
            {active === "delete" && hasId(label) && (
              <div className="mt-1.5">
                <DeleteLabelConfirm
                  orgId={orgId}
                  domain={domain}
                  displayKey={displayKey}
                  label={label}
                  onDone={() => setAction(null)}
                  onReload={onReload}
                />
              </div>
            )}
          </li>
        );
      })}
    </ul>
  );
}

/**
 * A domain's label catalog: listed for every member, changed by members
 * with write permission (`canEdit`).
 */
export function IssueLabelCatalogEditor({
  orgId,
  domain,
  canEdit,
}: {
  orgId: string;
  domain: IssueLabelDomain;
  canEdit: boolean;
}) {
  const catalog = useIssueLabelCatalog(orgId, domain.domainId);
  const [creating, setCreating] = useState(false);
  const displayKey =
    catalog.status === "ok" && catalog.data.displayKey
      ? catalog.data.displayKey
      : domain.displayKey;
  return (
    <div data-testid="label-catalog-editor" className="space-y-3">
      <p className="text-xs text-[var(--g5-text2)]">
        {canEdit
          ? domain.provider === "github"
            ? `Labels classify ${displayKey}'s issues. Changes are made on GitHub as your connected GitHub account.`
            : `Labels classify ${displayKey}'s issues.`
          : "Only members with write permission can change labels."}
      </p>
      {canEdit &&
        (creating ? (
          <CreateLabelForm
            orgId={orgId}
            domain={domain}
            displayKey={displayKey}
            onDone={() => setCreating(false)}
            onReload={catalog.refetch}
          />
        ) : (
          <Button
            kind="secondary"
            data-testid="label-catalog-new"
            onClick={() => setCreating(true)}
            className="px-2.5 py-1 text-xs"
          >
            <Plus aria-hidden="true" className="h-3.5 w-3.5" />
            New label
          </Button>
        ))}
      {catalog.status === "loading" && (
        <p role="status" className="text-xs text-[var(--g5-text2)]">
          Loading labels…
        </p>
      )}
      {catalog.status === "error" && (
        <IssueErrorNotice
          compact
          testId="label-catalog-error"
          presentation={describeIssueRequestError(catalog.error)}
          onRetry={catalog.refetch}
        />
      )}
      {catalog.status === "ok" && (
        <>
          <CatalogList
            orgId={orgId}
            domain={domain}
            catalog={catalog.data}
            canEdit={canEdit}
            onReload={catalog.refetch}
          />
          {catalog.data.truncated && (
            <p
              data-testid="label-catalog-truncated"
              className="text-xs text-[var(--g5-text2)]"
            >
              Only the first {catalog.data.labels.length} labels are listed.
            </p>
          )}
        </>
      )}
    </div>
  );
}

/** The catalog editor in a dialog above the picker's own surface. */
export function IssueLabelCatalogDialog({
  orgId,
  domain,
  canEdit,
  onClose,
}: {
  orgId: string;
  domain: IssueLabelDomain;
  canEdit: boolean;
  onClose: () => void;
}) {
  const headingId = useId();
  return (
    <Modal
      onClose={onClose}
      labelledBy={headingId}
      size="md"
      tier="top"
      testId="label-catalog-dialog"
      className="flex max-h-[85vh] flex-col p-0"
    >
      <div className="flex items-center justify-between border-b border-[var(--g5-stroke)] px-5 py-3">
        <h2
          id={headingId}
          className="text-base font-semibold text-[var(--g5-text0)]"
        >
          Labels · {domain.displayKey}
        </h2>
        <Button
          kind="ghost"
          data-testid="label-catalog-dialog-close"
          onClick={onClose}
          className="px-2.5 py-1 text-xs"
        >
          Done
        </Button>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto px-5 py-4">
        <IssueLabelCatalogEditor
          orgId={orgId}
          domain={domain}
          canEdit={canEdit}
        />
      </div>
    </Modal>
  );
}
