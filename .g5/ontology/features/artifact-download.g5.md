# Downloading build artifacts

`ArtifactoryTarball` lets a module author fetch a build artifact from JFrog Artifactory and copy its contents into an installation folder or a package.

## Definitions

- **Artifact**: the archive returned by the configured Artifactory server.
- **Local artifact folder**: the resource's folder under the module root, where IPM extracts the archive.
- **AQL**: Artifactory's query language, used by the authenticated search.
- **Overlay**: copying files over equal names while retaining unrelated files in the destination.

```
module resource + environment
  -> choose authenticated AQL or restricted anonymous search
  -> choose artifact path -> fetch bytes -> save temporary file
  -> extract into local artifact folder
       -> Activate: overlay InstallDirectory
       -> export: overlay caller's destination for this resource
```

## Selecting an artifact

<a id="artifact"></a>
[I · artifact]

An `ArtifactoryTarball` resource downloads an archive and copies its contents to `InstallDirectory` during Activate. During package export, it copies those contents to the destination supplied for that resource.

- Working assumption: preserve observed reuse and disclose that it gives no freshness guarantee. Alternatives are immutable digest-addressed files, explicit refresh, or validating the cached bytes against the selected artifact.
- Working assumption: disclose the warning boundary without promising protection. Alternatives are a common refusal rule, an explicit operator override, or retaining warning-only behavior.

<a id="artifact.filter"></a>
[D · artifact.filter]

Without credentials, the filter must give a name or at least one `@property`. Unsupported keys or operators fail instead of silently broadening the search.

- Allowed keys are `repo`, `name`, `path`, `type` and `@property`. `type`, when supplied, must be `file`.
- A condition is a plain value or an object with exactly one `$eq`. Only `name` and `path` also allow `$match`; arrays, nested operator values and multiple operators are refused.
- `*` matches any run and `?` one character. Other punctuation is literal. The native and formal contrasts cover ordinary ASCII text without newlines; ICU character behavior outside that scope is open.

<a id="artifact.selection"></a>
[D · artifact.selection]

Anonymous search keeps only results matching the repository, name, path and every required property, then takes the first result with the greatest `lastUpdated` text. An equal timestamp keeps the earlier result; no match is an error.

- Each required property must occur among that result's property values. Missing or scalar property data does not satisfy it.
- A root-level artifact uses `.` for path matching. Its returned download path omits that dot folder.
- With credentials, IPM asks AQL to sort by decreasing update time and takes result zero. It does not sort or recheck that response locally. Correct filtering and order on this path depend on the server; reversing a disposable server's response reverses IPM's choice.

## Fetching and admitting the archive

<a id="artifact.failure"></a>
[D · artifact.failure]

A missing `ARTI_SERVER` is an error outside developer mode and a warning with no download in developer mode. Developer mode also warns and skips anonymous search or fetch failures, but does not suppress authenticated failures or an extraction exception.

- Skipping does not create an artifact folder. Activate returns success without copying if the folder is still absent. Export still tries its copy, so the same skip does not by itself establish export success.
- Extraction and copies may leave files when a later step fails. A database rollback does not compensate those effects. No general filesystem atomicity is claimed.

<a id="artifact.archive-admission"></a>
[I · artifact.archive-admission]

An archive read error should fail extraction and the consuming download. Success must not hide a reported stream error.

- Known gap C:D18: on the tested IRIS provider, a non-gzip byte sequence produces an empty read with an error status. The tar reader ignores that status and treats the empty header as completion; the archive wrapper then returns success. Both ordinary and developer-mode anonymous downloads can succeed with an empty artifact folder.
- A gzip stream containing a nonempty header with a bad checksum is refused. A valid archive extracts its file. These neighbors distinguish the defect from a policy that rejects every empty archive.
- The formal contract covers the first-header decision under the observed gzip/header provider. It does not verify gzip decoding, checksum arithmetic, PAX records, path traversal or every extraction write.

## Reusing or copying local content

<a id="artifact.local-content"></a>
[D · artifact.local-content]

Activate reuses an existing local artifact folder without checking its contents or contacting the server. Export calls the download helper even when that folder exists.

- The byte helper fetches the response, but does not overwrite a temporary file with the same returned filename. Its previous bytes can therefore be reused even after a newer response arrives. File existence is not a freshness check.
- Successful copies overlay the destination: equal names are overwritten and unrelated target files remain. Other exported resources are outside this resource's destination and remain untouched.
- The caller builds that export destination from the module or dependency location and the resource's relative path. The processor does not independently choose the package root.

## Related rules on other pages

<a id="artifact.authentication"></a>
- Artifactory search and download — [artifact.authentication](../concepts/download-security.g5.md#artifact.authentication).

## Metadata

### artifact

- uncertain
- agent
- user-facing-behavior
- Expands: [Resource processors](resources.g5.md#module.processor).
- Children: [filter](#artifact.filter), [selection](#artifact.selection), [authentication](../concepts/download-security.g5.md#artifact.authentication), [local content](#artifact.local-content), [failures](#artifact.failure), [archive admission](#artifact.archive-admission), [When should an existing artifact folder or same-named temporary file be refreshed?](../../Verus_verification/open_questions.md#artifact.refresh-policy), [Should artifact copies refuse protected installation folders as FileCopy does?](../../Verus_verification/open_questions.md#artifact.target-policy).

Sources: `ResourceProcessor/ArtifactoryTarball.cls`; `Lifecycle/Base.cls::ExportSingleModule`. All child source spans, source hashes, Verus contracts and exact native contrasts are in the generated [iteration 3 index](../../uplift/consolidated/iter3/INDEX.json). These are scoped hand translations and native correspondence evidence; no ObjectScript implementation proof is claimed.

Open questions: [When should an existing artifact folder or same-named temporary file be refreshed?](../../Verus_verification/open_questions.md#artifact.refresh-policy), [Should artifact copies refuse protected installation folders as FileCopy does?](../../Verus_verification/open_questions.md#artifact.target-policy)

### artifact.filter

- agent
- Expands: [Artifact download](#artifact).

Sources: `ArtifactoryTarball.cls::ParseAnonymousFilter`, `::ParseCondition`, `::WildcardMatch`; native upstream unit fixtures and iteration 3 probes. Verus: `artifacts.rs::{condition,parse_fields,wildcard,filter_boundaries,wildcard_neighbors}`.

### artifact.selection

- agent
- Expands: [Artifact download](#artifact).

Sources: `ArtifactoryTarball.cls::{SelectNewestArtifact,MatchesCondition,HasProperties,GetArtifactPath,GetArtifactPathAnonymous}`. Verus: `artifacts.rs::{accepted,newest,selection_neighbors}`. Native AQL reversal and anonymous selection receipts do not prove a production provider ordering contract.

### artifact.failure

- agent
- Expands: [Artifact download](#artifact).

Sources: `ArtifactoryTarball.cls::DownloadArtifact`; Verus `artifacts.rs::download`. Native branch receipts cover missing server, authenticated refusal, successful extraction and malformed archive acceptance. Generic anonymous transport refusal and thrown extraction failures remain source/formal contrasts.

### artifact.archive-admission

- agent
- Expands: [Artifact download](#artifact).

Sources: `General/Archive.cls::Extract`; `Utils/FileBinaryTar.cls::{ExtractFile,ExtractStream,ParseHeader}`. Verus: `artifacts.rs::{header_action,artifact_first_header,failed_read_is_not_rejected}`; explicit gzip/header port in [TRUST](../../Verus_verification/consolidated/iter3/TRUST.md). Native invalid gzip, bad checksum and valid archive are distinct cases. [C:D18](../../Verus_verification/bugs_issues.md) retains the intended failure rule.

### artifact.local-content

- agent
- Expands: [Artifact download](#artifact).

Sources: `ArtifactoryTarball.cls::{OnAfterPhase,OnExportItem,RetrieveArtifact}`; `Lifecycle/Base.cls::ExportSingleModule`; native `%Library.File.CopyDir` signature and documentation. Verus: `artifacts.rs::{save_download,activate,copy_export,cache_and_failure_neighbors}`.
