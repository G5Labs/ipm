# Download credentials and integrity

A repository install and an Artifactory resource each look up a package before fetching its bytes. The two providers handle credentials differently. The remote registry’s credential boundary is an open maintainer decision.

The [ORAS archive boundary](../architecture/package-sources.g5.md#repo.oras.archive-boundary) covers OCI downloads and links to the proposed [signature verification rule](../architecture/package-sources.g5.md#repo.oras.signed-admission).

## Remote package registries

<a id="repo.remote.download"></a>
[D · repo.remote.download]

To install from a remote registry, IPM asks the registry for the package's URL and hash, then downloads that URL. It sends the repository's credentials on both requests, even when the URL names a different server.

- The download follows the URL exactly as the registry returns it. This was a deliberate 2022 change from a path relative to the repository. Sending credentials to another server was not discussed; see the [credential question](../../Verus_verification/open_questions.md#repo.remote.download.credentials).
- Artifactory downloads do the opposite and send no credentials on the byte request ([artifact authentication](#artifact.authentication)).

- Working assumption: keep following the returned URL, and record the forwarding as suspected defect C:D20. The proposed alternative sends credentials only when the URL's server and port match the repository; a third choice is to refuse such URLs. Either change can break registries that redirect downloads to storage needing the same credentials.

<a id="repo.remote.download.redirect"></a>
[D · repo.remote.download.redirect]

Registry requests follow HTTP redirects automatically and send the same credentials on each hop. A download URL on the repository's own server that redirects to another server therefore still delivers the Basic credentials and API key there.

- Checking only the URL the registry returns would not stop this. A fix has to decide credentials per hop, or stop following redirects on its own.

## Checking the downloaded file and reporting installation

<a id="repo.remote.download.integrity"></a>
[D · repo.remote.download.integrity]

When the registry gives a hash, the downloaded file's SHA-1 must equal it exactly. The computed SHA-1 is lowercase, so an uppercase hash from the registry is refused even when it is the same digest.

- A mismatch fails the install with "Download module, hash mismatch." No install token is kept and no install report is sent to the registry.
- An empty hash skips the check. Bytes that are not an archive then pass the download, extract to nothing and fail later with "File not found to obtain module object", after the registry has received an install report.
- A failed metadata request reports its HTTP status. A failed download reports only "tRequest statuscode not 200".

<a id="repo.remote.download.token"></a>
[D · repo.remote.download.token]

The registry may return an install token with the package. IPM keeps it only after the hash check passes, sends it once with the install report and then deletes it.

## Artifactory search and download

<a id="artifact.authentication"></a>
[D · artifact.authentication]

A token takes precedence over a complete username and password. Incomplete Basic credentials use anonymous search.

- As built, credentials are attached to the metadata request, but the byte-download request is a new anonymous request. A server that accepts the token for search and requires it for bytes therefore fails the download. A public byte endpoint succeeds.
- This is a confirmed mismatch in the credential handoff, recorded as C:D17. HTTP status must be 200; success in search does not establish permission to fetch the artifact.

## Metadata

### repo.remote.download

- design-verified

- uncertain
- agent
- user-facing-behavior, security
- Expands: [Repositories](repositories.g5.md#repo).
- Children: [redirects](#repo.remote.download.redirect), [integrity](#repo.remote.download.integrity), [install token](#repo.remote.download.token), [Should registry credentials be sent to a download URL on a different server?](../../Verus_verification/open_questions.md#repo.remote.download.credentials).

Direct decision: `Repo/Remote/PackageService.cls::GetModule`; request builder `Repo/Http/PackageService.cls::GetHttpRequest`. Caller: `Utils/Module.cls` remote branch of the module-reference loader (`tClient.GetModule`, then `LoadModuleFromArchive`, then `CollectAnalytics`). History: commit `d6cc4204` (2022-07-07). The parsed `path` variable and its comment are dead code. Verus [registry.rs](../../Verus_verification/consolidated/iter4/registry.rs) `{metadata_request,bytes_request,repaired_bytes_request,lemma_cross_origin_carries_credentials}`; controls `bytes-sent-to-configured-origin`, `bytes-drop-credentials-as-built` and `repair-still-forwards` rejected. Native R-cross: a two-origin provider recorded Basic and `apikey` headers on `127.0.0.1:18502` after metadata from `localhost:18501`. Redirect handling (`FollowRedirect=1`), TLS and production registries are outside this observation.

Open questions: [Should registry credentials be sent to a download URL on a different server?](../../Verus_verification/open_questions.md#repo.remote.download.credentials)

State machines: [registry · iter4](../../Verus_verification/models/iter4-registry/manifest.json)

### repo.remote.download.redirect

- design-verified

- agent
- user-facing-behavior, security
- Expands: [Remote download](#repo.remote.download).

Direct decision: `Repo/Http/PackageService.cls::GetHttpRequest` (`FollowRedirect = 1`, `apikey` header and `Username`/`Password` set once on the request). Caller: `Repo/Remote/PackageService.cls::GetModule`. Iteration 5 challenge of iteration 4's request boundary, which had left redirects open. Native X1: metadata named `localhost:18501/redirect/cross`, which answered 302 to `127.0.0.1:18502`; the provider recorded Basic and `apikey` on all three requests. Verus [redirect.rs](../../Verus_verification/consolidated/iter5/redirect.rs) `{as_built_sends,s14_sends,s14b_sends,lemma_s14_insufficient,lemma_s14b_never_leaks}`; controls `redirect-drops-credentials-as-built`, `s14-checks-every-hop`, `s14b-checks-first-hop` rejected. Redirects on metadata and listing requests share the same builder but were not exercised. CHANGELOG 0.10.11 `#1115` fixed the same redirect leak for ORAS token checks.

State machines: [redirect · iter5](../../Verus_verification/models/iter5-redirect/manifest.json)

### repo.remote.download.integrity

- agent
- user-facing-behavior, security
- Expands: [Remote download](#repo.remote.download).

Direct decision: `Repo/Remote/PackageService.cls::GetModule` lines 130-151. Readers: `Utils/Module.cls` (download error escapes before analytics; accepted bytes always treated as an archive), `General/Archive.cls::Extract` (C:D18). Verus `{integrity_ok,get_module,install,registry_neighbors}`; controls `hash-compared-case-insensitively`, `empty-hash-rejected`, `mismatch-sends-analytics`, `junk-refused-at-download` and `bytes-failure-reports-status` rejected. Native R-mismatch, R-upper, R-nohash, R-dl404, R-meta404, and full `install` I1 (installed), I2 (hash mismatch, no report), I3 (empty hash with non-archive bytes, manifest error, report sent). The leftover temporary file after a mismatch was not checked.

### repo.remote.download.token

- agent
- Expands: [Remote download](#repo.remote.download).

Writer: `Repo/Remote/PackageService.cls::GetModule` (`^IRIS.TempZPMRInst`). Reader: `::SendAnalyticsToRegistry`. Verus `token_stored`; control `token-stored-before-integrity` rejected. Native R-same/R-cross store, R-mismatch/R-upper do not, R-analytics sends `tok-same` once and the entry is gone afterwards.

### artifact.authentication

- agent
- Expands: [Artifact download](../features/artifact-download.g5.md#artifact).

Sources: `ArtifactoryTarball.cls::{DownloadArtifact,GetArtifactPath,RetrieveArtifact}`. Verus: `artifacts.rs::{auth,metadata_request,bytes_request,credentials_cross_endpoint}`. Native requests record fixture-token metadata followed by an unauthenticated GET and HTTP 401. See [C:D17](../../Verus_verification/bugs_issues.md).
