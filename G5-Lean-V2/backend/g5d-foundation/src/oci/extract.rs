//! Layer extraction + rootfs assembly.
//!
//! `extract_gzipped_tar` decompresses a single layer blob into a directory.
//! `OciExtractor::materialize_rootfs` orchestrates the end-to-end flow:
//! parse image ref → fetch manifest → ensure each layer is cached → apply
//! layers in order with OCI whiteout semantics into the caller's target
//! rootfs directory.
//!
//! ## Path-traversal defense in depth
//!
//! `tar::Archive::unpack` already rejects absolute paths and `..` components
//! per the crate docs. We add an explicit second-layer guard inside the
//! per-entry extraction loop so a regression in the `tar` crate, or a
//! malicious entry with a tricky symlink target, cannot escape the layer
//! root. Every resolved path is verified to live under the destination
//! root before any I/O hits the disk.

use std::collections::{BTreeSet, HashSet};
use std::path::{Component, Path, PathBuf};

use flate2::read::GzDecoder;
use tar::{Archive, EntryType};

use super::cache::LayerCache;
use super::error::OciError;
use super::registry::{
    parse_image_ref, AcrClient, Digest, DockerHubClient, EcrClient, GarClient, GhcrClient,
    ImageConfig, ImageRef, Manifest, Reference, Registry, RegistryClient,
};
use super::template;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterializedRootfs {
    pub rootfs: PathBuf,
    pub image_config: ImageConfig,
    /// Exact resolved pristine template, paired with this materialization.
    /// Never infer it by resolving a mutable image tag a second time.
    pub template_dir: Option<PathBuf>,
}

/// Sidecar at a cached layer's root naming the directories the layer tar
/// recorded explicitly, as NUL-separated relative paths. Assembly replays a
/// recorded directory's mode onto an existing lower directory
/// (`g5.runtime.boot.image.rootfs`); a parent the extraction had to
/// synthesize for a nested entry carries no image metadata and must not
/// overwrite a lower layer's mode (a layer adding `tmp/file` must leave
/// `/tmp` at 1777). Layers cached without the sidecar are extracted again.
pub(crate) const EXPLICIT_DIRS_MARKER: &str = ".explicit-dirs";

#[cfg(unix)]
fn path_bytes(path: &Path) -> &[u8] {
    use std::os::unix::ffi::OsStrExt;
    path.as_os_str().as_bytes()
}

#[cfg(unix)]
fn path_from_bytes(bytes: &[u8]) -> PathBuf {
    use std::os::unix::ffi::OsStrExt;
    PathBuf::from(std::ffi::OsStr::from_bytes(bytes))
}

#[cfg(not(unix))]
fn path_bytes(path: &Path) -> &[u8] {
    path.to_str().unwrap_or_default().as_bytes()
}

#[cfg(not(unix))]
fn path_from_bytes(bytes: &[u8]) -> PathBuf {
    PathBuf::from(String::from_utf8_lossy(bytes).into_owned())
}

/// The sidecar's bytes for the directories `extract_gzipped_tar` reported.
pub(crate) fn encode_explicit_dirs(dirs: &BTreeSet<PathBuf>) -> Vec<u8> {
    let mut out = Vec::new();
    for dir in dirs {
        out.extend_from_slice(path_bytes(dir));
        out.push(0);
    }
    out
}

/// The layer's recorded directories, or `None` when the layer carries no
/// readable sidecar; assembly then treats every directory as synthesized.
fn read_explicit_dirs(layer: &Path) -> std::io::Result<Option<HashSet<PathBuf>>> {
    let path = layer.join(EXPLICIT_DIRS_MARKER);
    match std::fs::symlink_metadata(&path) {
        Ok(meta) if meta.is_file() => {}
        Ok(_) => return Ok(None),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    }
    let bytes = std::fs::read(&path)?;
    Ok(Some(
        bytes
            .split(|byte| *byte == 0)
            .filter(|entry| !entry.is_empty())
            .map(path_from_bytes)
            .collect(),
    ))
}

/// Extract a gzipped tar blob into `dest`. The OCI image-layer spec covers
/// gzip (`application/vnd.oci.image.layer.v1.tar+gzip`) and zstd
/// (`application/vnd.oci.image.layer.v1.tar+zstd`). This build supports the
/// gzip variant only; zstd layers are rejected so the registry-side error
/// is "we can't read this" rather than a silent half-extract.
///
/// The extractor explicitly validates each entry's path is contained within
/// `dest` before letting the `tar` crate write anything. This is defense in
/// depth on top of `tar::Archive::unpack`'s built-in sanitization.
///
/// Returns the directories the tar recorded explicitly (normalized relative
/// paths, each once however many headers named it: assembly needs only set
/// membership, and a layer may repeat a directory header without bound),
/// for the layer's [`EXPLICIT_DIRS_MARKER`] sidecar.
pub fn extract_gzipped_tar(
    blob: &Path,
    dest: &Path,
    media_type: &str,
) -> std::io::Result<BTreeSet<PathBuf>> {
    if media_type.contains("zstd") {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!(
                "zstd-compressed layers not supported in this build: {}",
                media_type
            ),
        ));
    }
    let file = std::fs::File::open(blob)?;
    let decoder = GzDecoder::new(file);
    let mut archive = Archive::new(decoder);
    archive.set_preserve_permissions(true);
    archive.set_overwrite(true);
    // Do not preserve ownerships — the daemon is typically non-root, and
    // even when root we want files owned by the daemon UID so the host
    // filesystem audit doesn't show unknown UIDs from random base images.
    archive.set_unpack_xattrs(false);

    // Per-entry extraction with explicit path-traversal guard. Each entry's
    // path is normalized; we refuse anything that is absolute, includes a
    // parent-dir component, or — after resolution against `dest` — does not
    // start with `dest`. Symlink and hardlink targets are validated to stay
    // inside `dest` as well, since `tar::Archive::unpack` does NOT enforce
    // that for link bodies by default.
    let mut explicit_dirs = BTreeSet::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path_in_tar = entry.path()?.into_owned();
        validate_relative_path(&path_in_tar)?;
        let target = dest.join(&path_in_tar);
        ensure_under(dest, &target)?;
        if entry.header().entry_type() == EntryType::Directory {
            // Normalize like the assembled tree's `read_dir` paths: a GNU
            // tar's leading `./` is not part of the directory's name.
            explicit_dirs.insert(
                path_in_tar
                    .components()
                    .filter(|component| !matches!(component, Component::CurDir))
                    .collect::<PathBuf>(),
            );
        }

        if entry.header().entry_type() == EntryType::Symlink
            || entry.header().entry_type() == EntryType::Link
        {
            if let Some(link) = entry.link_name()? {
                validate_link_target(dest, &target, &link)?;
            }
        }

        entry.unpack_in(dest)?;

        // Ownership-coupled setuid handling (ontology g5/g5d/oci, layer
        // assembly Phase 2): ownership is never preserved (files land owned
        // by the daemon uid), so when the daemon is NOT root a preserved
        // setuid/setgid bit points at the daemon's own uid — never what the
        // image meant, and actively harmful: guest-root exec of such a binary
        // DROPS to the daemon's euid. Strip the bits non-root; a root daemon
        // (the Linux bare worker) preserves them with their root ownership.
        #[cfg(unix)]
        if matches!(
            entry.header().entry_type(),
            EntryType::Regular | EntryType::Directory
        ) {
            // Directories too: a setgid dir would force the daemon's gid onto
            // guest-created children, and apply_files re-propagates cached
            // modes verbatim (codex implementation review, Low #3).
            strip_setid_bits_if_nonroot(&target);
        }
    }
    Ok(explicit_dirs)
}

/// Strip setuid/setgid mode bits from `path` when the current process is not
/// root. No-op (preserving image semantics) when running as root. Errors are
/// ignored — the entry may legitimately be replaced by a later layer.
#[cfg(unix)]
fn strip_setid_bits_if_nonroot(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    // SAFETY: geteuid has no failure modes and touches no memory.
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    if let Ok(meta) = std::fs::symlink_metadata(path) {
        let mode = meta.permissions().mode();
        if mode & 0o6000 != 0 {
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode & 0o1777));
        }
    }
}

fn validate_relative_path(path: &Path) -> std::io::Result<()> {
    let mut saw_normal = false;
    for component in path.components() {
        match component {
            Component::Normal(_) => saw_normal = true,
            Component::CurDir => {}
            Component::ParentDir => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "rejected tar entry with parent-dir component: {}",
                        path.display()
                    ),
                ));
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("rejected absolute tar entry path: {}", path.display()),
                ));
            }
        }
    }
    if !saw_normal {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "rejected empty tar entry path",
        ));
    }
    Ok(())
}

fn ensure_under(root: &Path, candidate: &Path) -> std::io::Result<()> {
    // Resolve `..` and `.` components in `candidate` against `root` without
    // requiring the path to exist (canonicalize would fail before extraction).
    // After every component, the resolved buffer MUST still be a path under
    // `root`. If it ever escapes, refuse.
    let mut resolved = root.to_path_buf();
    let prefix_components: Vec<Component> = root.components().collect();
    let cand_components: Vec<Component> = candidate.components().collect();
    let rel_components = if cand_components.len() >= prefix_components.len()
        && cand_components[..prefix_components.len()] == prefix_components[..]
    {
        &cand_components[prefix_components.len()..]
    } else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "tar entry path '{}' is not rooted in destination '{}'",
                candidate.display(),
                root.display()
            ),
        ));
    };
    for c in rel_components {
        match c {
            Component::Normal(part) => resolved.push(part),
            Component::CurDir => {}
            Component::ParentDir => {
                let popped = resolved.pop();
                if !popped || !resolved.starts_with(root) {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!(
                            "tar entry path '{}' escapes destination '{}'",
                            candidate.display(),
                            root.display()
                        ),
                    ));
                }
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("tar entry path '{}' is absolute", candidate.display()),
                ));
            }
        }
    }
    Ok(())
}

fn validate_link_target(dest: &Path, link_path: &Path, link_body: &Path) -> std::io::Result<()> {
    // A link target is one of:
    //   * ROOTFS-ABSOLUTE (e.g. `/usr/bin/mawk`, `/bin/busybox`): inside an
    //     extracted rootfs this denotes the GUEST's `/usr/bin/mawk`, i.e.
    //     `dest/usr/bin/mawk`. Real base images (ubuntu, alpine, node, …) use
    //     these pervasively (`/etc/alternatives/*`, `lib -> /usr/lib`, busybox
    //     applets), so they MUST be accepted — rejecting them outright (the
    //     previous behavior) made the extractor unable to materialize any real
    //     base image. We rebuild the path from its Normal components onto
    //     `dest` (not by stripping a literal separator) so `..`/`.` survive and
    //     are caught below.
    //   * RELATIVE (e.g. `../bin/busybox`): resolved against the link's parent.
    //
    // `ensure_under` then rejects anything that escapes `dest` (e.g. a target of
    // `/../../etc/shadow`). This is defense-in-depth: the AUTHORITATIVE
    // write-through guard is `tar`'s own `validate_inside_dst` canonicalize
    // check inside `Entry::unpack_in`, which follows symlinks at write time and
    // refuses any parent dir that resolves outside `dest` (tar 0.4.46).
    let composed = if link_body.is_absolute() {
        let mut p = dest.to_path_buf();
        for component in link_body.components() {
            match component {
                Component::Normal(part) => p.push(part),
                Component::CurDir => {}
                Component::ParentDir => p.push(".."),
                // Drop the leading root/prefix: an absolute target is rebased on
                // `dest` (the rootfs root), not the host root.
                Component::RootDir | Component::Prefix(_) => {}
            }
        }
        p
    } else {
        link_path.parent().unwrap_or(dest).join(link_body)
    };
    ensure_under(dest, &composed)
}

/// Routes a parsed image to the registry client that holds the right
/// credentials (D18). Production owns one client per supported registry; the
/// test/injection path wraps a single client used for every host so the
/// existing wiremock end-to-end tests are unaffected.
enum RegistryRouter {
    Multi {
        dockerhub: Box<dyn RegistryClient>,
        ghcr: Box<dyn RegistryClient>,
        gar: Box<dyn RegistryClient>,
        ecr: Box<dyn RegistryClient>,
        acr: Box<dyn RegistryClient>,
    },
    Single(Box<dyn RegistryClient>),
}

impl RegistryRouter {
    fn production() -> Self {
        RegistryRouter::Multi {
            dockerhub: Box::new(DockerHubClient::new()),
            ghcr: Box::new(GhcrClient::new()),
            gar: Box::new(GarClient::new()),
            ecr: Box::new(EcrClient::new()),
            acr: Box::new(AcrClient::new()),
        }
    }

    /// Select the client for a normalized `ImageRef.registry` host.
    fn client_for(&self, registry_host: &str) -> &dyn RegistryClient {
        match self {
            RegistryRouter::Single(c) => c.as_ref(),
            RegistryRouter::Multi {
                dockerhub,
                ghcr,
                gar,
                ecr,
                acr,
            } => match Registry::for_host(registry_host) {
                Registry::Ghcr => ghcr.as_ref(),
                Registry::Gar => gar.as_ref(),
                Registry::Ecr => ecr.as_ref(),
                Registry::Acr => acr.as_ref(),
                Registry::DockerHub => dockerhub.as_ref(),
            },
        }
    }

    /// A representative client for test accessors that don't route by host.
    #[cfg(test)]
    fn any(&self) -> &dyn RegistryClient {
        match self {
            RegistryRouter::Single(c) => c.as_ref(),
            RegistryRouter::Multi { dockerhub, .. } => dockerhub.as_ref(),
        }
    }
}

pub struct OciExtractor {
    router: RegistryRouter,
    cache: LayerCache,
}

impl OciExtractor {
    pub fn new(cache_root: PathBuf) -> Self {
        Self {
            router: RegistryRouter::production(),
            cache: LayerCache::new(cache_root),
        }
    }

    pub fn from_env() -> Self {
        Self {
            router: RegistryRouter::production(),
            cache: LayerCache::from_env(),
        }
    }

    pub fn with_components(cache: LayerCache, client: Box<dyn RegistryClient>) -> Self {
        Self {
            router: RegistryRouter::Single(client),
            cache,
        }
    }

    /// Test-only accessor for the underlying registry client so wiremock
    /// tests can drive manifest/layer fetches against a mock host.
    #[cfg(test)]
    pub(crate) fn client_for_test(&self) -> &dyn RegistryClient {
        self.router.any()
    }

    /// Test-only accessor for the underlying layer cache so wiremock tests
    /// can call `ensure_layer` directly with a constructed `ImageRef`.
    #[cfg(test)]
    pub(crate) fn cache_for_test(&self) -> &LayerCache {
        &self.cache
    }

    /// Pin `image_ref` to its digest form (extract_and_cache D32): a
    /// reference that already carries a digest is returned unchanged; a tag
    /// is resolved through the registry client that owns its registry
    /// (registry_auth_core D23) and rewritten in place as `name@sha256:…`.
    /// A tag that cannot be resolved is an error — never a fallback to the
    /// mutable name.
    pub async fn pin_image_ref(&self, image_ref: &str) -> Result<String, OciError> {
        let image = parse_image_ref(image_ref)?;
        if matches!(image.reference, Reference::Digest(_)) {
            return Ok(image_ref.to_string());
        }
        let client = self.router.client_for(&image.registry);
        let digest = client.resolve_manifest_digest(&image).await?;
        Ok(rewrite_tag_as_digest(image_ref, digest.as_str()))
    }

    /// Immutable identity of a local OCI image-layout directory (D31/D32):
    /// the digest of the host-architecture runnable manifest the boot path
    /// would select from `index.json`, read through the same
    /// [`super::layout::OciLayoutClient`]. The directory stays the boot
    /// reference; the caller composes `oci-layout@<digest>` for fences. A
    /// directory that is not an OCI layout (a bare rootfs tree) is an error —
    /// nothing immutable names such a tree.
    pub async fn layout_manifest_digest(&self, layout_dir: &Path) -> Result<Digest, OciError> {
        let client = super::layout::OciLayoutClient::new(layout_dir)?;
        let image = ImageRef {
            registry: "oci-layout".to_string(),
            repository: layout_dir.display().to_string(),
            reference: Reference::Tag("local".to_string()),
        };
        client.resolve_manifest_digest(&image).await
    }

    pub async fn materialize_rootfs(
        &self,
        image_ref: &str,
        target_dir: &Path,
    ) -> Result<PathBuf, OciError> {
        Ok(self
            .materialize_rootfs_with_config(image_ref, target_dir)
            .await?
            .rootfs)
    }

    pub async fn materialize_rootfs_with_config(
        &self,
        image_ref: &str,
        target_dir: &Path,
    ) -> Result<MaterializedRootfs, OciError> {
        let image = parse_image_ref(image_ref)?;
        tracing::info!(
            image = image_ref,
            target = %target_dir.display(),
            "Materializing OCI rootfs"
        );
        ensure_empty_target(target_dir).await?;

        // Route to the client holding credentials for this registry (D18).
        let client = self.router.client_for(&image.registry);

        let manifest = client.fetch_manifest(&image).await?;
        tracing::debug!(
            image = image_ref,
            registry = %image.registry,
            layer_count = manifest.layers.len(),
            "Fetched OCI manifest"
        );

        let image_config = client.fetch_config(&image, &manifest.config).await?;
        self.assemble_layers(client, &image, image_ref, &manifest, target_dir)
            .await?;

        tracing::info!(
            image = image_ref,
            target = %target_dir.display(),
            layers = manifest.layers.len(),
            "OCI rootfs materialized"
        );
        Ok(MaterializedRootfs {
            rootfs: target_dir.to_path_buf(),
            image_config,
            template_dir: None,
        })
    }

    /// Ensure every layer of `manifest` is cached, then apply them in order
    /// (whiteouts + files, D6) into `target_dir`. Shared by the per-call
    /// direct assembly path and the template build path (D29).
    async fn assemble_layers(
        &self,
        client: &dyn RegistryClient,
        image: &ImageRef,
        image_ref: &str,
        manifest: &Manifest,
        target_dir: &Path,
    ) -> Result<(), OciError> {
        let mut layer_dirs: Vec<PathBuf> = Vec::with_capacity(manifest.layers.len());
        for descriptor in &manifest.layers {
            let dir = self.cache.ensure_layer(client, image, descriptor).await?;
            layer_dirs.push(dir);
        }

        for (idx, layer_dir) in layer_dirs.iter().enumerate() {
            tracing::debug!(
                image = image_ref,
                layer_idx = idx,
                layer_dir = %layer_dir.display(),
                "Applying OCI layer"
            );
            apply_layer(layer_dir, target_dir).map_err(|e| OciError::LayerExtract {
                digest: format!("layer[{}]={}", idx, layer_dir.display()),
                source: e,
            })?;
        }
        Ok(())
    }

    /// Template-backed materialization (ontology `g5/g5d/oci` D29): assemble
    /// the rootfs at most once per image content identity under
    /// `template_root`, then COW-clone the assembled tree into `target_dir`.
    ///
    /// `template_root` must live on the same filesystem as `target_dir`
    /// (reflinks cannot cross filesystems); the isolation caller passes
    /// `<G5D_ROOTFS_STATE_DIR>/templates`. `target_dir` must not exist — the
    /// clone creates it — and, like the direct path, ownership of its
    /// lifecycle stays with the caller. The returned image config is read
    /// from the template's persisted `image-config.json`, so a warm boot
    /// costs one manifest round-trip plus a clone: no config-blob fetch and
    /// no layer I/O.
    pub async fn materialize_rootfs_cow(
        &self,
        image_ref: &str,
        template_root: &Path,
        target_dir: &Path,
    ) -> Result<MaterializedRootfs, OciError> {
        let image = parse_image_ref(image_ref)?;
        self.materialize_rootfs_cow_for(&image, image_ref, template_root, target_dir)
            .await
    }

    /// Download, verify and assemble the shared image template without
    /// creating a VM rootfs. Host bootstrap uses this before registration;
    /// subsequent VM starts use the same cache key and build lock. Returns
    /// the template directory (`<template_root>/sha256/<key>`).
    pub async fn prepare_rootfs_template(
        &self,
        image_ref: &str,
        template_root: &Path,
    ) -> Result<PathBuf, OciError> {
        let image = parse_image_ref(image_ref)?;
        let client = self.router.client_for(&image.registry);
        let materialized = self
            .prepare_rootfs_template_with_client(client, &image, image_ref, template_root)
            .await?;
        Ok(materialized
            .rootfs
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or(materialized.rootfs))
    }

    /// Prepare a local OCI layout without cloning its whole directory per VM.
    pub async fn prepare_rootfs_template_from_layout(
        &self,
        layout_dir: &Path,
        template_root: &Path,
    ) -> Result<PathBuf, OciError> {
        let client = super::layout::OciLayoutClient::new(layout_dir)?;
        let image = ImageRef {
            registry: "oci-layout".to_string(),
            repository: layout_dir.display().to_string(),
            reference: Reference::Tag("local".to_string()),
        };
        let prepared = self
            .prepare_rootfs_template_with_client(
                &client,
                &image,
                &format!("oci-layout:{}", layout_dir.display()),
                template_root,
            )
            .await?;
        Ok(prepared
            .rootfs
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or(prepared.rootfs))
    }

    /// Parsed-ref core of [`materialize_rootfs_cow`]; `image_ref` is used
    /// for log context only. `pub(crate)` so the wiremock end-to-end tests
    /// can drive it against a mock `host:port` registry that
    /// `parse_image_ref` (correctly) rejects.
    pub(crate) async fn materialize_rootfs_cow_for(
        &self,
        image: &ImageRef,
        image_ref: &str,
        template_root: &Path,
        target_dir: &Path,
    ) -> Result<MaterializedRootfs, OciError> {
        let client = self.router.client_for(&image.registry);
        self.materialize_rootfs_cow_with_client(client, image, image_ref, template_root, target_dir)
            .await
    }

    /// Materialize a rootfs from a local OCI image-layout directory (ontology
    /// `g5/g5d/oci` D31): the standard `docker buildx --output type=oci`
    /// export. Reuses the full template pipeline through the filesystem-backed
    /// [`super::layout::OciLayoutClient`], so the image config (Env included)
    /// survives delivery exactly as for registry images. The synthetic image
    /// identity is log context only — template keys derive from resolved
    /// digests, so a layout image and the same image pulled from a registry
    /// share one template identity.
    pub async fn materialize_rootfs_cow_from_layout(
        &self,
        layout_dir: &Path,
        template_root: &Path,
        target_dir: &Path,
    ) -> Result<MaterializedRootfs, OciError> {
        let client = super::layout::OciLayoutClient::new(layout_dir)?;
        let image = ImageRef {
            registry: "oci-layout".to_string(),
            repository: layout_dir.display().to_string(),
            reference: Reference::Tag("local".to_string()),
        };
        let image_ref = format!("oci-layout:{}", layout_dir.display());
        self.materialize_rootfs_cow_with_client(
            &client,
            &image,
            &image_ref,
            template_root,
            target_dir,
        )
        .await
    }

    /// Client-parameterized core of [`materialize_rootfs_cow`]: one template
    /// pipeline shared by the registry router path and the local layout path.
    async fn materialize_rootfs_cow_with_client(
        &self,
        client: &dyn RegistryClient,
        image: &ImageRef,
        image_ref: &str,
        template_root: &Path,
        target_dir: &Path,
    ) -> Result<MaterializedRootfs, OciError> {
        let prepared = self
            .prepare_rootfs_template_with_client(client, image, image_ref, template_root)
            .await?;

        if let Some(parent) = target_dir.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| OciError::CacheIo {
                    path: parent.display().to_string(),
                    source: e,
                })?;
        }
        if tokio::fs::symlink_metadata(target_dir).await.is_ok() {
            return Err(OciError::CacheIo {
                path: target_dir.display().to_string(),
                source: std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    "target rootfs directory already exists; the clone step creates it",
                ),
            });
        }
        template::clone_rootfs(&prepared.rootfs, target_dir).await?;

        tracing::info!(
            image = image_ref,
            template = %prepared.rootfs.display(),
            target = %target_dir.display(),
            "OCI rootfs cloned from template"
        );
        Ok(MaterializedRootfs {
            rootfs: target_dir.to_path_buf(),
            image_config: prepared.image_config,
            template_dir: prepared.template_dir,
        })
    }

    // The returned root is shared and must never be booted or mutated.
    async fn prepare_rootfs_template_with_client(
        &self,
        client: &dyn RegistryClient,
        image: &ImageRef,
        image_ref: &str,
        template_root: &Path,
    ) -> Result<MaterializedRootfs, OciError> {
        // Tag references resolve through the manifest on every boot (tag
        // freshness); the template key derives from resolved content only.
        let manifest = client.fetch_manifest(image).await?;
        let layer_digests: Vec<String> = manifest.layers.iter().map(|d| d.digest.clone()).collect();
        let key = template::template_key(
            template::daemon_is_root(),
            &manifest.config.digest,
            &layer_digests,
        );
        let template_dir = template::template_dir(template_root, &key);

        if template::is_complete(&template_dir) {
            tracing::info!(
                image = image_ref,
                template = %template_dir.display(),
                "Rootfs template cache hit"
            );
        } else {
            self.build_template(client, image, image_ref, &manifest, &template_dir, &key)
                .await?;
        }

        Ok(MaterializedRootfs {
            rootfs: template_dir.join(template::ROOTFS_DIR),
            image_config: template::read_image_config(&template_dir)?,
            template_dir: Some(template_dir),
        })
    }

    /// Build the digest-keyed template under a cross-process lock (D29):
    /// assemble into a `.tmp-<uuid>` sibling, persist the image config,
    /// write the `.complete` sentinel, and atomically rename into place.
    async fn build_template(
        &self,
        client: &dyn RegistryClient,
        image: &ImageRef,
        image_ref: &str,
        manifest: &Manifest,
        template_dir: &Path,
        key: &str,
    ) -> Result<(), OciError> {
        let lock = match template::TemplateBuildLock::acquire(template_dir, key).await? {
            Some(lock) => lock,
            // A sibling finished the template while we waited on the lock.
            None => return Ok(()),
        };
        // Re-check after winning: a sibling may have completed between our
        // last poll and the mkdir win.
        if template::is_complete(template_dir) {
            lock.release().await;
            return Ok(());
        }
        let result = self
            .build_template_locked(client, image, image_ref, manifest, template_dir)
            .await;
        lock.release().await;
        result
    }

    async fn build_template_locked(
        &self,
        client: &dyn RegistryClient,
        image: &ImageRef,
        image_ref: &str,
        manifest: &Manifest,
        template_dir: &Path,
    ) -> Result<(), OciError> {
        let parent = template_dir
            .parent()
            .expect("template dir always has a sha256/ parent");
        let tmp = parent.join(format!(".tmp-{}", uuid::Uuid::now_v7()));
        let _cleanup =
            super::temporary::TemporaryDirectory::create(tmp.clone()).map_err(|source| {
                OciError::CacheIo {
                    path: tmp.display().to_string(),
                    source,
                }
            })?;
        let tmp_rootfs = tmp.join(template::ROOTFS_DIR);

        tracing::info!(
            image = image_ref,
            template = %template_dir.display(),
            layers = manifest.layers.len(),
            "Building rootfs template (one-time assembly for this image digest)"
        );

        let build: Result<(), OciError> = async {
            std::fs::create_dir(&tmp_rootfs).map_err(|e| OciError::CacheIo {
                path: tmp_rootfs.display().to_string(),
                source: e,
            })?;
            // Fetch + digest-verify the config before assembling so a
            // tampered config fails before any multi-GB work (mirrors the
            // direct path's ordering, D6 step 2).
            let image_config = client.fetch_config(image, &manifest.config).await?;
            self.assemble_layers(client, image, image_ref, manifest, &tmp_rootfs)
                .await?;
            template::write_image_config(&tmp, &image_config)?;
            let marker = tmp.join(template::COMPLETE_MARKER);
            tokio::fs::write(&marker, b"ok")
                .await
                .map_err(|e| OciError::CacheIo {
                    path: marker.display().to_string(),
                    source: e,
                })?;
            Ok(())
        }
        .await;

        if let Err(e) = build {
            let _ = tokio::fs::remove_dir_all(&tmp).await;
            return Err(e);
        }

        match tokio::fs::rename(&tmp, template_dir).await {
            Ok(()) => {
                tracing::info!(
                    image = image_ref,
                    template = %template_dir.display(),
                    "Rootfs template built"
                );
                Ok(())
            }
            Err(e) => {
                // Another process won the race; content-addressed identity
                // makes their template equivalent to ours.
                let _ = tokio::fs::remove_dir_all(&tmp).await;
                if template::is_complete(template_dir) {
                    Ok(())
                } else {
                    Err(OciError::CacheIo {
                        path: template_dir.display().to_string(),
                        source: e,
                    })
                }
            }
        }
    }
}

async fn ensure_empty_target(path: &Path) -> Result<(), OciError> {
    match tokio::fs::read_dir(path).await {
        Ok(mut entries) => {
            if entries
                .next_entry()
                .await
                .map_err(|e| OciError::CacheIo {
                    path: path.display().to_string(),
                    source: e,
                })?
                .is_some()
            {
                return Err(OciError::CacheIo {
                    path: path.display().to_string(),
                    source: std::io::Error::new(
                        std::io::ErrorKind::AlreadyExists,
                        "target rootfs directory is not empty",
                    ),
                });
            }
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => tokio::fs::create_dir_all(path)
            .await
            .map_err(|e| OciError::CacheIo {
                path: path.display().to_string(),
                source: e,
            }),
        Err(e) => Err(OciError::CacheIo {
            path: path.display().to_string(),
            source: e,
        }),
    }
}

/// Apply a single layer's extracted directory contents on top of `target`
/// with OCI whiteout semantics. Whiteouts in the layer apply first, then
/// regular files; per OCI spec a single layer that contains both deletes a
/// path and recreates a different file of the same name must apply the
/// delete before the write.
///
/// The cache layout's `.complete` sentinel and `.explicit-dirs` sidecar at
/// the layer root are skipped during assembly — they are cache-internal,
/// not part of the OCI rootfs.
pub(crate) fn apply_layer(layer: &Path, target: &Path) -> std::io::Result<()> {
    require_merge_directory(target)?;
    let explicit_dirs = read_explicit_dirs(layer)?;
    apply_whiteouts(layer, layer, target)?;
    apply_files(layer, layer, target, explicit_dirs.as_ref())?;
    Ok(())
}

fn is_cache_marker(rel: &Path) -> bool {
    rel == Path::new(".complete") || rel == Path::new(EXPLICIT_DIRS_MARKER)
}

/// Assembly owns an unpublished rootfs exclusively; cached layers are immutable
/// and no guest uses this directory until assembly finishes. Check every target
/// ancestor with lstat, never canonicalize image symlinks as host paths. These
/// checks protect against image entries, not concurrent mutation by a host process.
fn require_merge_directory(path: &Path) -> std::io::Result<()> {
    if !std::fs::symlink_metadata(path)?.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "OCI merge ancestor is not a real directory: {}",
                path.display()
            ),
        ));
    }
    Ok(())
}

fn merge_target_path(root: &Path, relative: &Path) -> std::io::Result<PathBuf> {
    validate_relative_path(relative)?;
    let target = root.join(relative);
    ensure_under(root, &target)?;
    require_merge_directory(root)?;
    let mut parent = root.to_path_buf();
    if let Some(relative_parent) = relative.parent() {
        for component in relative_parent.components() {
            parent.push(component);
            require_merge_directory(&parent)?;
        }
    }
    Ok(target)
}

fn merge_metadata(path: &Path) -> std::io::Result<Option<std::fs::Metadata>> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) => Ok(Some(meta)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// The caller has checked the parent chain. A symlink (including a dangling
/// one) is an entry to unlink, never a directory to traverse. Propagate failures
/// so a failed unlink cannot turn the subsequent write into a write-through.
fn remove_merge_entry(path: &Path) -> std::io::Result<()> {
    if let Some(meta) = merge_metadata(path)? {
        if meta.is_dir() {
            std::fs::remove_dir_all(path)?;
        } else {
            std::fs::remove_file(path)?;
        }
    }
    Ok(())
}

fn apply_whiteouts(root: &Path, current: &Path, target_root: &Path) -> std::io::Result<()> {
    for entry in std::fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        let rel = path
            .strip_prefix(root)
            .map_err(|_| std::io::Error::other("strip_prefix failed during whiteout pass"))?;
        // Skip the cache markers — they are not part of the rootfs.
        if is_cache_marker(rel) {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_str().unwrap_or("");
        let target_path = merge_target_path(target_root, rel)?;
        if name == ".wh..wh..opq" {
            // merge_target_path checked every component of this parent. The
            // whiteout pass only descends into real lower-layer directories.
            let target_parent = target_path.parent().expect("checked relative path");
            for child in std::fs::read_dir(target_parent)? {
                let child = child?;
                let relative_child = rel.parent().unwrap().join(child.file_name());
                let child_path = merge_target_path(target_root, &relative_child)?;
                remove_merge_entry(&child_path)?;
            }
        } else if let Some(stripped) = name.strip_prefix(".wh.") {
            // A marker called .wh., .wh.. or .wh... must not delete the
            // containing directory or its parent, even when lexically beneath
            // target_root. Whiteouts identify exactly one ordinary filename.
            if !matches!(
                Path::new(stripped).components().next(),
                Some(Component::Normal(_))
            ) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("invalid OCI whiteout name: {name}"),
                ));
            }
            let relative_target = rel.parent().unwrap().join(stripped);
            let target_path = merge_target_path(target_root, &relative_target)?;
            remove_merge_entry(&target_path)?;
        } else if entry.file_type()?.is_dir() {
            // A missing/file/symlink lower entry has no children to white out.
            // The file pass will replace it with the upper layer's real dir.
            if merge_metadata(&target_path)?.is_some_and(|meta| meta.is_dir()) {
                apply_whiteouts(root, &path, target_root)?;
            }
        }
    }
    Ok(())
}

fn apply_files(
    root: &Path,
    current: &Path,
    target_root: &Path,
    explicit_dirs: Option<&HashSet<PathBuf>>,
) -> std::io::Result<()> {
    for entry in std::fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        if entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.starts_with(".wh."))
        {
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .map_err(|_| std::io::Error::other("strip_prefix failed during file pass"))?;
        if is_cache_marker(rel) {
            continue;
        }
        let target_path = merge_target_path(target_root, rel)?;
        let file_type = entry.file_type()?;

        if file_type.is_symlink() {
            remove_merge_entry(&target_path)?;
            let link = std::fs::read_link(&path)?;
            #[cfg(unix)]
            std::os::unix::fs::symlink(&link, &target_path)?;
            #[cfg(not(unix))]
            {
                let _ = link;
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Unsupported,
                    "OCI symlink assembly requires Unix",
                ));
            }
        } else if file_type.is_dir() {
            let exists = merge_metadata(&target_path)?.is_some_and(|meta| meta.is_dir());
            if !exists {
                remove_merge_entry(&target_path)?;
                std::fs::create_dir(&target_path)?;
            }
            apply_files(root, &path, target_root, explicit_dirs)?;
            // Populate the directory before applying potentially read-only
            // image modes; replacing a symlink now creates a real directory.
            // A directory the layer recorded takes its mode whether or not it
            // already existed (`g5.runtime.boot.image.rootfs`): a chmod-only
            // layer is directory entries with the new mode, and dropping them
            // left the g5dev image's baked cargo registry root-only in every
            // guest. A parent the extraction synthesized for a nested entry
            // has no recorded mode and leaves an existing directory alone.
            let recorded = explicit_dirs.is_some_and(|dirs| dirs.contains(rel));
            if !exists || recorded {
                std::fs::set_permissions(&target_path, entry.metadata()?.permissions())?;
                #[cfg(unix)]
                strip_setid_bits_if_nonroot(&target_path);
            }
        } else if file_type.is_file() {
            remove_merge_entry(&target_path)?;
            // Exclusive creation also prevents overwriting an existing hardlink.
            let mut output = std::fs::File::options()
                .write(true)
                .create_new(true)
                .open(&target_path)?;
            std::io::copy(&mut std::fs::File::open(&path)?, &mut output)?;
            output.set_permissions(entry.metadata()?.permissions())?;
            #[cfg(unix)]
            strip_setid_bits_if_nonroot(&target_path);
        } else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("unsupported OCI layer entry: {}", path.display()),
            ));
        }
    }
    Ok(())
}

/// String surgery for D32: replace the trailing `:tag` of the LAST path
/// component (or append to a bare name) with `@<digest>`, leaving the
/// registry host — which may itself carry a `:port` — untouched, so the
/// pinned form routes exactly like the caller's unpinned form.
fn rewrite_tag_as_digest(image_ref: &str, digest: &str) -> String {
    let (head, last) = match image_ref.rfind('/') {
        Some(idx) => (&image_ref[..=idx], &image_ref[idx + 1..]),
        None => ("", image_ref),
    };
    let name = match last.rfind(':') {
        Some(idx) => &last[..idx],
        None => last,
    };
    format!("{head}{name}@{digest}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use tar::{Builder, Header};
    use tempfile::TempDir;

    #[test]
    fn rewrite_tag_as_digest_keeps_registry_ports_and_paths() {
        let d = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        assert_eq!(
            rewrite_tag_as_digest("ghcr.io/g5labs/g5d-g5dev:main", d),
            format!("ghcr.io/g5labs/g5d-g5dev@{d}")
        );
        assert_eq!(
            rewrite_tag_as_digest("localhost:5000/team/img:v1", d),
            format!("localhost:5000/team/img@{d}")
        );
        assert_eq!(rewrite_tag_as_digest("ubuntu", d), format!("ubuntu@{d}"));
        assert_eq!(
            rewrite_tag_as_digest("registry.example.com:443/img", d),
            format!("registry.example.com:443/img@{d}")
        );
    }

    #[tokio::test]
    async fn pin_image_ref_returns_digest_refs_unchanged_without_network() {
        // A digest reference never touches the registry: the extractor holds
        // a client that would fail any network call.
        let tmp = TempDir::new().unwrap();
        struct Refusing;
        #[async_trait::async_trait]
        impl RegistryClient for Refusing {
            async fn fetch_manifest(&self, _: &ImageRef) -> Result<Manifest, OciError> {
                panic!("network call on a digest reference")
            }
            async fn fetch_config(
                &self,
                _: &ImageRef,
                _: &crate::oci::registry::Descriptor,
            ) -> Result<ImageConfig, OciError> {
                panic!("network call on a digest reference")
            }
            async fn fetch_layer_blob(
                &self,
                _: &ImageRef,
                _: &crate::oci::registry::Descriptor,
                _: &Path,
            ) -> Result<(), OciError> {
                panic!("network call on a digest reference")
            }
        }
        let extractor = OciExtractor::with_components(
            LayerCache::new(tmp.path().to_path_buf()),
            Box::new(Refusing),
        );
        let pinned = "ghcr.io/g5labs/g5d-g5dev@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        assert_eq!(extractor.pin_image_ref(pinned).await.unwrap(), pinned);
        // The provided default refuses tags fail-loud on a non-registry client.
        let err = extractor
            .pin_image_ref("ghcr.io/g5labs/g5d-g5dev:main")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("manifest digest"), "{err}");
    }

    /// Build a single-entry gzipped tar archive whose header's path field is
    /// written byte-for-byte from `path`, bypassing `tar::Builder::append_data`'s
    /// own safety check (which itself refuses absolute paths and `..`). This
    /// lets us simulate a tar payload arriving from a malicious registry.
    fn build_gzipped_tar_raw_path(path_bytes: &[u8], data: &[u8], mode: u32) -> Vec<u8> {
        assert!(
            path_bytes.len() < 100,
            "raw path test helper supports short paths only"
        );
        let mut header = Header::new_ustar();
        header.set_size(data.len() as u64);
        header.set_mode(mode);
        header.set_entry_type(EntryType::Regular);
        // Write path directly to the name field, bypassing the safety guard
        // in `set_path` / `append_data` which the production `tar::Builder`
        // would otherwise apply.
        {
            let name = &mut header.as_old_mut().name;
            for b in name.iter_mut() {
                *b = 0;
            }
            name[..path_bytes.len()].copy_from_slice(path_bytes);
        }
        header.set_cksum();

        let mut gz = GzEncoder::new(Vec::new(), Compression::fast());
        {
            let mut tar = Builder::new(&mut gz);
            tar.append(&header, data).unwrap();
            tar.finish().unwrap();
        }
        gz.finish().unwrap()
    }

    fn build_tar_with_link(link_name: &str, link_target: &str, is_symlink: bool) -> Vec<u8> {
        // `tar::Header::set_link_name` accepts arbitrary bytes (including
        // absolute paths and `..` traversal), so this helper does not need
        // the raw-bytes workaround the path-traversal tests use.
        let mut gz = GzEncoder::new(Vec::new(), Compression::fast());
        {
            let mut tar = Builder::new(&mut gz);
            let mut header = Header::new_gnu();
            header.set_size(0);
            header.set_mode(0o777);
            header.set_entry_type(if is_symlink {
                EntryType::Symlink
            } else {
                EntryType::Link
            });
            header.set_link_name(link_target).unwrap();
            header.set_cksum();
            tar.append_data(&mut header, link_name, &mut std::io::Cursor::new(&[][..]))
                .unwrap();
            tar.finish().unwrap();
        }
        gz.finish().unwrap()
    }

    /// Ownership-coupled setuid handling (ontology g5/g5d/oci Phase 2): a
    /// non-root extractor must strip setuid/setgid bits — ownership is never
    /// preserved, so the bit would point at the daemon's own uid and make
    /// guest-root exec DROP privileges (the ubuntu:24.04 `/usr/bin/mount`
    /// dev-flip RCA). A root extractor preserves image semantics unchanged.
    #[cfg(unix)]
    #[test]
    fn extract_strips_setid_bits_when_nonroot() {
        use std::os::unix::fs::PermissionsExt;
        let payload = build_gzipped_tar_raw_path(b"bin-mount", b"#!/bin/sh\n", 0o4755);
        let blob_dir = TempDir::new().unwrap();
        let blob = blob_dir.path().join("layer.tar.gz");
        std::fs::write(&blob, payload).unwrap();

        let dest = TempDir::new().unwrap();
        extract_gzipped_tar(
            &blob,
            dest.path(),
            "application/vnd.oci.image.layer.v1.tar+gzip",
        )
        .unwrap();

        let mode = std::fs::symlink_metadata(dest.path().join("bin-mount"))
            .unwrap()
            .permissions()
            .mode();
        // SAFETY: geteuid has no failure modes and touches no memory.
        if unsafe { libc::geteuid() } == 0 {
            assert_eq!(mode & 0o6000, 0o4000, "root extractor preserves setuid");
        } else {
            assert_eq!(
                mode & 0o6000,
                0,
                "non-root extractor must strip setuid/setgid (got mode {mode:o})"
            );
            assert_eq!(mode & 0o777, 0o755, "non-setid permission bits preserved");
        }
    }

    /// Stale-cache self-heal (codex post-push P2): a layer dir cached BEFORE
    /// the unpack-time strip landed still carries setid modes; assembly must
    /// not re-propagate them into the rootfs under a non-root daemon.
    #[cfg(unix)]
    #[test]
    fn apply_layer_strips_setid_from_stale_cached_modes_when_nonroot() {
        use std::os::unix::fs::PermissionsExt;
        let layer = TempDir::new().unwrap();
        std::fs::create_dir_all(layer.path().join("sgid-dir")).unwrap();
        std::fs::set_permissions(
            layer.path().join("sgid-dir"),
            std::fs::Permissions::from_mode(0o2755),
        )
        .unwrap();
        std::fs::write(layer.path().join("suid-bin"), b"x").unwrap();
        std::fs::set_permissions(
            layer.path().join("suid-bin"),
            std::fs::Permissions::from_mode(0o4755),
        )
        .unwrap();

        let target = TempDir::new().unwrap();
        apply_layer(layer.path(), target.path()).unwrap();

        for name in ["sgid-dir", "suid-bin"] {
            let mode = std::fs::symlink_metadata(target.path().join(name))
                .unwrap()
                .permissions()
                .mode();
            // SAFETY: geteuid has no failure modes and touches no memory.
            if unsafe { libc::geteuid() } == 0 {
                assert_ne!(mode & 0o6000, 0, "root assembly preserves setid on {name}");
            } else {
                assert_eq!(
                    mode & 0o6000,
                    0,
                    "non-root assembly must strip setid on {name} (mode {mode:o})"
                );
            }
        }
    }

    /// Write a hand-built layer's sidecar: the directories its tar recorded.
    fn record_explicit_dirs(layer: &Path, dirs: &[&str]) {
        let dirs: BTreeSet<PathBuf> = dirs.iter().map(PathBuf::from).collect();
        std::fs::write(
            layer.join(EXPLICIT_DIRS_MARKER),
            encode_explicit_dirs(&dirs),
        )
        .unwrap();
    }

    /// Only directory entries are recorded, each once; a nested file's
    /// synthesized parent is not.
    #[test]
    fn extract_reports_only_the_directories_the_tar_recorded() {
        let mut gz = GzEncoder::new(Vec::new(), Compression::default());
        {
            let mut tar = tar::Builder::new(&mut gz);
            let mut dir = Header::new_gnu();
            dir.set_size(0);
            dir.set_mode(0o1777);
            dir.set_entry_type(EntryType::Directory);
            dir.set_cksum();
            tar.append_data(&mut dir, "recorded/", std::io::empty())
                .unwrap();
            let mut prefixed = Header::new_gnu();
            prefixed.set_size(0);
            prefixed.set_mode(0o777);
            prefixed.set_entry_type(EntryType::Directory);
            prefixed.set_cksum();
            tar.append_data(&mut prefixed, "./dot/prefixed/", std::io::empty())
                .unwrap();
            // A layer may name the same directory any number of times (a
            // repeated header overwrites the same on-disk directory); the
            // record keeps set membership, not the repetitions.
            for _ in 0..3 {
                let mut again = Header::new_gnu();
                again.set_size(0);
                again.set_mode(0o1777);
                again.set_entry_type(EntryType::Directory);
                again.set_cksum();
                tar.append_data(&mut again, "recorded/", std::io::empty())
                    .unwrap();
            }
            let mut file = Header::new_gnu();
            file.set_size(1);
            file.set_mode(0o644);
            file.set_entry_type(EntryType::Regular);
            file.set_cksum();
            tar.append_data(&mut file, "synthesized/child", &b"x"[..])
                .unwrap();
            tar.finish().unwrap();
        }
        let blob_dir = TempDir::new().unwrap();
        let blob = blob_dir.path().join("layer.tar.gz");
        std::fs::write(&blob, gz.finish().unwrap()).unwrap();
        let dest = TempDir::new().unwrap();
        let dirs = extract_gzipped_tar(
            &blob,
            dest.path(),
            "application/vnd.oci.image.layer.v1.tar+gzip",
        )
        .unwrap();
        assert_eq!(
            dirs,
            BTreeSet::from([PathBuf::from("dot/prefixed"), PathBuf::from("recorded")]),
            "a leading `./` is not part of the recorded name; a repeated header is one entry"
        );
        assert_eq!(
            encode_explicit_dirs(&dirs),
            b"dot/prefixed\0recorded\0".to_vec(),
            "the sidecar carries each recorded directory once"
        );
        assert!(dest.path().join("synthesized/child").is_file());
        let decoded = read_explicit_dirs(dest.path()).unwrap();
        assert!(decoded.is_none(), "no sidecar until the cache writes one");
        std::fs::write(
            dest.path().join(EXPLICIT_DIRS_MARKER),
            encode_explicit_dirs(&dirs),
        )
        .unwrap();
        let decoded = read_explicit_dirs(dest.path()).unwrap().unwrap();
        assert!(decoded.contains(Path::new("recorded")));
        assert!(decoded.contains(Path::new("dot/prefixed")));
        assert!(!decoded.contains(Path::new("synthesized")));
    }

    /// A nested entry whose parent the extraction synthesized (no directory
    /// header in the tar) carries no mode for that parent: a lower layer's
    /// `/tmp` 1777 survives a layer that adds `tmp/file`, with or without a
    /// sidecar (Codex review of PR #547).
    #[cfg(unix)]
    #[test]
    fn apply_layer_keeps_a_lower_layers_mode_for_a_synthesized_parent_directory() {
        use std::os::unix::fs::PermissionsExt;
        for sidecar in [true, false] {
            let target = TempDir::new().unwrap();
            let lower = TempDir::new().unwrap();
            std::fs::create_dir(lower.path().join("tmp")).unwrap();
            std::fs::set_permissions(
                lower.path().join("tmp"),
                std::fs::Permissions::from_mode(0o1777),
            )
            .unwrap();
            record_explicit_dirs(lower.path(), &["tmp"]);
            apply_layer(lower.path(), target.path()).unwrap();

            let upper = TempDir::new().unwrap();
            std::fs::create_dir(upper.path().join("tmp")).unwrap();
            std::fs::set_permissions(
                upper.path().join("tmp"),
                std::fs::Permissions::from_mode(0o755),
            )
            .unwrap();
            std::fs::write(upper.path().join("tmp/file"), b"added").unwrap();
            if sidecar {
                record_explicit_dirs(upper.path(), &[]);
            }
            apply_layer(upper.path(), target.path()).unwrap();

            let mode = std::fs::symlink_metadata(target.path().join("tmp"))
                .unwrap()
                .permissions()
                .mode()
                & 0o7777;
            assert_eq!(
                mode, 0o1777,
                "sidecar={sidecar}: synthesized parent kept the lower mode"
            );
            assert_eq!(
                std::fs::read(target.path().join("tmp/file")).unwrap(),
                b"added"
            );
            assert!(!target.path().join(EXPLICIT_DIRS_MARKER).exists());
        }
    }

    /// A later layer that only changes an existing directory's mode is a
    /// directory entry with the new mode and no new children
    /// (`g5.runtime.boot.image.rootfs`); assembly must apply it, sticky bit
    /// included, exactly as it applies a re-emitted file's mode.
    #[cfg(unix)]
    #[test]
    fn apply_layer_applies_a_later_layers_directory_mode_to_an_existing_directory() {
        use std::os::unix::fs::PermissionsExt;
        let target = TempDir::new().unwrap();
        let lower = TempDir::new().unwrap();
        std::fs::create_dir_all(lower.path().join("usr/local/cargo/registry/cache")).unwrap();
        std::fs::write(
            lower
                .path()
                .join("usr/local/cargo/registry/cache/dep.crate"),
            b"baked",
        )
        .unwrap();
        for dir in [
            "usr/local/cargo",
            "usr/local/cargo/registry",
            "usr/local/cargo/registry/cache",
        ] {
            std::fs::set_permissions(
                lower.path().join(dir),
                std::fs::Permissions::from_mode(0o755),
            )
            .unwrap();
        }
        apply_layer(lower.path(), target.path()).unwrap();
        let mode = |rel: &str| {
            std::fs::symlink_metadata(target.path().join(rel))
                .unwrap()
                .permissions()
                .mode()
                & 0o7777
        };
        assert_eq!(mode("usr/local/cargo/registry/cache"), 0o755);

        let upper = TempDir::new().unwrap();
        std::fs::create_dir_all(upper.path().join("usr/local/cargo/registry/cache")).unwrap();
        std::fs::write(
            upper
                .path()
                .join("usr/local/cargo/registry/cache/dep.crate"),
            b"baked",
        )
        .unwrap();
        std::fs::set_permissions(
            upper
                .path()
                .join("usr/local/cargo/registry/cache/dep.crate"),
            std::fs::Permissions::from_mode(0o666),
        )
        .unwrap();
        std::fs::set_permissions(
            upper.path().join("usr/local/cargo/registry/cache"),
            std::fs::Permissions::from_mode(0o777),
        )
        .unwrap();
        std::fs::set_permissions(
            upper.path().join("usr/local/cargo/registry"),
            std::fs::Permissions::from_mode(0o777),
        )
        .unwrap();
        std::fs::set_permissions(
            upper.path().join("usr/local/cargo"),
            std::fs::Permissions::from_mode(0o1777),
        )
        .unwrap();
        // Untouched ancestors travel with their unchanged mode.
        std::fs::set_permissions(
            upper.path().join("usr/local"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        std::fs::set_permissions(
            upper.path().join("usr"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        record_explicit_dirs(
            upper.path(),
            &[
                "usr",
                "usr/local",
                "usr/local/cargo",
                "usr/local/cargo/registry",
                "usr/local/cargo/registry/cache",
            ],
        );
        apply_layer(upper.path(), target.path()).unwrap();

        assert_eq!(mode("usr/local/cargo/registry/cache/dep.crate"), 0o666);
        assert_eq!(
            mode("usr/local/cargo/registry/cache"),
            0o777,
            "an existing directory takes the later layer's mode"
        );
        assert_eq!(mode("usr/local/cargo/registry"), 0o777);
        assert_eq!(
            mode("usr/local/cargo"),
            0o1777,
            "the sticky bit is not a setid bit and survives every daemon identity"
        );
        assert_eq!(mode("usr/local"), 0o755);
        assert_eq!(
            std::fs::read(
                target
                    .path()
                    .join("usr/local/cargo/registry/cache/dep.crate")
            )
            .unwrap(),
            b"baked"
        );
    }

    /// The non-root setid strip applies to an existing directory's re-emitted
    /// mode exactly as it applies to a new one.
    #[cfg(unix)]
    #[test]
    fn apply_layer_strips_setid_from_a_later_layers_existing_directory_mode_when_nonroot() {
        use std::os::unix::fs::PermissionsExt;
        let target = TempDir::new().unwrap();
        std::fs::create_dir_all(target.path().join("shared")).unwrap();
        std::fs::set_permissions(
            target.path().join("shared"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        let layer = TempDir::new().unwrap();
        std::fs::create_dir_all(layer.path().join("shared")).unwrap();
        std::fs::set_permissions(
            layer.path().join("shared"),
            std::fs::Permissions::from_mode(0o2775),
        )
        .unwrap();
        record_explicit_dirs(layer.path(), &["shared"]);
        apply_layer(layer.path(), target.path()).unwrap();
        let mode = std::fs::symlink_metadata(target.path().join("shared"))
            .unwrap()
            .permissions()
            .mode()
            & 0o7777;
        // SAFETY: geteuid has no failure modes and touches no memory.
        if unsafe { libc::geteuid() } == 0 {
            assert_eq!(
                mode, 0o2775,
                "root assembly keeps the image's setgid directory"
            );
        } else {
            assert_eq!(
                mode, 0o775,
                "non-root assembly strips setgid but keeps the new mode"
            );
        }
    }

    #[test]
    fn apply_layer_copies_files_from_empty_target() {
        let layer = TempDir::new().unwrap();
        std::fs::create_dir_all(layer.path().join("etc")).unwrap();
        std::fs::write(layer.path().join("etc/hello"), b"world").unwrap();

        let target = TempDir::new().unwrap();
        apply_layer(layer.path(), target.path()).unwrap();

        let contents = std::fs::read_to_string(target.path().join("etc/hello")).unwrap();
        assert_eq!(contents, "world");
    }

    #[test]
    fn apply_layer_overwrites_existing_file() {
        let target = TempDir::new().unwrap();
        std::fs::create_dir_all(target.path().join("etc")).unwrap();
        std::fs::write(target.path().join("etc/hello"), b"old").unwrap();

        let layer = TempDir::new().unwrap();
        std::fs::create_dir_all(layer.path().join("etc")).unwrap();
        std::fs::write(layer.path().join("etc/hello"), b"new").unwrap();

        apply_layer(layer.path(), target.path()).unwrap();
        let contents = std::fs::read_to_string(target.path().join("etc/hello")).unwrap();
        assert_eq!(contents, "new");
    }

    #[test]
    fn whiteout_deletes_named_file() {
        let target = TempDir::new().unwrap();
        std::fs::create_dir_all(target.path().join("etc")).unwrap();
        std::fs::write(target.path().join("etc/hello"), b"world").unwrap();

        let layer = TempDir::new().unwrap();
        std::fs::create_dir_all(layer.path().join("etc")).unwrap();
        std::fs::write(layer.path().join("etc/.wh.hello"), b"").unwrap();

        apply_layer(layer.path(), target.path()).unwrap();
        assert!(!target.path().join("etc/hello").exists());
    }

    #[test]
    fn opaque_whiteout_clears_directory() {
        let target = TempDir::new().unwrap();
        std::fs::create_dir_all(target.path().join("opt")).unwrap();
        std::fs::write(target.path().join("opt/keep"), b"a").unwrap();
        std::fs::write(target.path().join("opt/also"), b"b").unwrap();

        let layer = TempDir::new().unwrap();
        std::fs::create_dir_all(layer.path().join("opt")).unwrap();
        std::fs::write(layer.path().join("opt/.wh..wh..opq"), b"").unwrap();
        std::fs::write(layer.path().join("opt/new"), b"c").unwrap();

        apply_layer(layer.path(), target.path()).unwrap();
        assert!(!target.path().join("opt/keep").exists());
        assert!(!target.path().join("opt/also").exists());
        let contents = std::fs::read_to_string(target.path().join("opt/new")).unwrap();
        assert_eq!(contents, "c");
    }

    #[test]
    #[cfg(unix)]
    fn symlink_in_layer_is_preserved() {
        use std::os::unix::fs::symlink;
        let layer = TempDir::new().unwrap();
        std::fs::create_dir_all(layer.path().join("usr/bin")).unwrap();
        std::fs::write(layer.path().join("usr/bin/sh"), b"shebang").unwrap();
        symlink("sh", layer.path().join("usr/bin/bash")).unwrap();

        let target = TempDir::new().unwrap();
        apply_layer(layer.path(), target.path()).unwrap();

        let meta = target
            .path()
            .join("usr/bin/bash")
            .symlink_metadata()
            .unwrap();
        assert!(meta.file_type().is_symlink());
        let link = std::fs::read_link(target.path().join("usr/bin/bash")).unwrap();
        assert_eq!(link, std::path::PathBuf::from("sh"));
    }

    #[test]
    fn whiteout_on_missing_path_is_noop() {
        let layer = TempDir::new().unwrap();
        std::fs::write(layer.path().join(".wh.does-not-exist"), b"").unwrap();

        let target = TempDir::new().unwrap();
        apply_layer(layer.path(), target.path()).unwrap();
        assert!(std::fs::read_dir(target.path()).unwrap().next().is_none());
    }

    #[test]
    fn complete_marker_skipped_during_apply() {
        // The cache writes a `.complete` sentinel inside each cached layer
        // dir. apply_layer must NOT copy it into the rootfs.
        let layer = TempDir::new().unwrap();
        std::fs::write(layer.path().join(".complete"), b"ok").unwrap();
        std::fs::create_dir_all(layer.path().join("etc")).unwrap();
        std::fs::write(layer.path().join("etc/keep"), b"data").unwrap();

        let target = TempDir::new().unwrap();
        apply_layer(layer.path(), target.path()).unwrap();
        assert!(!target.path().join(".complete").exists());
        assert!(target.path().join("etc/keep").exists());
    }

    /// SEC-C03: the lower layer can plant a link at any depth. Neither
    /// file writes nor either kind of whiteout may use it as a directory.
    #[cfg(unix)]
    #[test]
    fn layer_merge_never_traverses_lower_layer_symlinks() {
        use std::os::unix::fs::symlink;
        for link_kind in ["absolute", "relative", "dangling"] {
            for operation in ["write", "named-whiteout", "opaque-whiteout"] {
                for nested in [false, true] {
                    let sandbox = TempDir::new().unwrap();
                    let outside = sandbox.path().join("outside");
                    std::fs::create_dir(&outside).unwrap();
                    std::fs::write(outside.join("victim"), b"host data").unwrap();
                    let lower = sandbox.path().join("lower");
                    let upper = sandbox.path().join("upper");
                    let target = sandbox.path().join("target");
                    for dir in [&lower, &upper, &target] {
                        std::fs::create_dir(dir).unwrap();
                    }
                    let relative = if nested { "parent/x" } else { "x" };
                    if nested {
                        std::fs::create_dir(lower.join("parent")).unwrap();
                    }
                    let link = match link_kind {
                        "relative" => PathBuf::from(if nested {
                            "../../outside"
                        } else {
                            "../outside"
                        }),
                        "dangling" => outside.join("not-created"),
                        _ => outside.clone(),
                    };
                    symlink(&link, lower.join(relative)).unwrap();
                    std::fs::create_dir_all(upper.join(relative)).unwrap();
                    match operation {
                        "write" => {
                            std::fs::write(upper.join(relative).join("victim"), b"image data")
                        }
                        "named-whiteout" => {
                            std::fs::write(upper.join(relative).join(".wh.victim"), b"")
                        }
                        _ => std::fs::write(upper.join(relative).join(".wh..wh..opq"), b""),
                    }
                    .unwrap();
                    apply_layer(&lower, &target).unwrap();
                    // Reapplying a layer also covers retries after partial assembly.
                    for _ in 0..2 {
                        apply_layer(&upper, &target).unwrap();
                        assert_eq!(
                            std::fs::read(outside.join("victim")).unwrap(),
                            b"host data",
                            "{link_kind} {operation} nested={nested}"
                        );
                        assert!(!outside.join("not-created").exists());
                        assert!(std::fs::symlink_metadata(target.join(relative))
                            .unwrap()
                            .is_dir());
                        if operation == "write" {
                            assert_eq!(
                                std::fs::read(target.join(relative).join("victim")).unwrap(),
                                b"image data"
                            );
                        }
                    }
                }
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn layer_merge_replaces_all_entry_types_without_following_links() {
        use std::os::unix::fs::symlink;
        for old in ["file", "directory", "symlink", "hardlink"] {
            for new in ["file", "directory", "symlink"] {
                let outside = TempDir::new().unwrap();
                let victim = outside.path().join("victim");
                std::fs::write(&victim, b"host data").unwrap();
                let target = TempDir::new().unwrap();
                let layer = TempDir::new().unwrap();
                let old_path = target.path().join("entry");
                match old {
                    "file" => std::fs::write(&old_path, b"old").unwrap(),
                    "directory" => {
                        std::fs::create_dir(&old_path).unwrap();
                        symlink(outside.path(), old_path.join("nested-link")).unwrap();
                    }
                    "hardlink" => std::fs::hard_link(&victim, &old_path).unwrap(),
                    _ => symlink(&victim, &old_path).unwrap(),
                }
                let new_path = layer.path().join("entry");
                match new {
                    "file" => std::fs::write(&new_path, b"new").unwrap(),
                    "directory" => {
                        std::fs::create_dir(&new_path).unwrap();
                        std::fs::write(new_path.join("child"), b"new").unwrap();
                    }
                    _ => symlink("/usr/bin/sh", &new_path).unwrap(),
                }
                apply_layer(layer.path(), target.path()).unwrap();
                assert_eq!(std::fs::read(&victim).unwrap(), b"host data");
                match new {
                    "file" => assert_eq!(std::fs::read(&old_path).unwrap(), b"new"),
                    "directory" => {
                        assert_eq!(std::fs::read(old_path.join("child")).unwrap(), b"new")
                    }
                    _ => assert_eq!(
                        std::fs::read_link(&old_path).unwrap(),
                        Path::new("/usr/bin/sh")
                    ),
                }
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn whiteouts_unlink_symlinks_without_touching_their_targets() {
        use std::os::unix::fs::symlink;
        for marker in [".wh.link", ".wh..wh..opq"] {
            let outside = TempDir::new().unwrap();
            std::fs::write(outside.path().join("victim"), b"host data").unwrap();
            let target = TempDir::new().unwrap();
            symlink(outside.path(), target.path().join("link")).unwrap();
            symlink(
                outside.path().join("missing"),
                target.path().join("dangling"),
            )
            .unwrap();
            let layer = TempDir::new().unwrap();
            std::fs::write(layer.path().join(marker), b"").unwrap();
            std::fs::write(layer.path().join(".wh.dangling"), b"").unwrap();
            apply_layer(layer.path(), target.path()).unwrap();
            assert!(std::fs::symlink_metadata(target.path().join("link")).is_err());
            assert!(std::fs::symlink_metadata(target.path().join("dangling")).is_err());
            assert_eq!(
                std::fs::read(outside.path().join("victim")).unwrap(),
                b"host data"
            );
        }
    }

    #[test]
    fn layer_merge_rejects_whiteouts_without_a_normal_target_name() {
        for marker in [".wh.", ".wh..", ".wh..."] {
            let sandbox = TempDir::new().unwrap();
            let target = sandbox.path().join("target");
            let layer = sandbox.path().join("layer");
            std::fs::create_dir(&target).unwrap();
            std::fs::create_dir(&layer).unwrap();
            std::fs::write(target.join("keep"), b"rootfs data").unwrap();
            std::fs::write(sandbox.path().join("keep"), b"host data").unwrap();
            std::fs::write(layer.join(marker), b"").unwrap();
            let result = apply_layer(&layer, &target);
            assert!(result.is_err(), "must reject {marker}");
            assert_eq!(std::fs::read(target.join("keep")).unwrap(), b"rootfs data");
            assert_eq!(
                std::fs::read(sandbox.path().join("keep")).unwrap(),
                b"host data"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn layer_merge_rejects_symlink_root() {
        let outside = TempDir::new().unwrap();
        let sandbox = TempDir::new().unwrap();
        let target = sandbox.path().join("rootfs");
        std::os::unix::fs::symlink(outside.path(), &target).unwrap();
        let layer = TempDir::new().unwrap();
        std::fs::write(layer.path().join("new"), b"image data").unwrap();
        assert!(apply_layer(layer.path(), &target).is_err());
        assert!(!outside.path().join("new").exists());
    }

    #[test]
    fn zstd_layer_rejected() {
        let blob = TempDir::new().unwrap();
        let blob_path = blob.path().join("not-real");
        std::fs::write(&blob_path, b"placeholder").unwrap();
        let dest = TempDir::new().unwrap();
        let err = extract_gzipped_tar(
            &blob_path,
            dest.path(),
            "application/vnd.oci.image.layer.v1.tar+zstd",
        )
        .unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    }

    #[test]
    fn extract_rejects_absolute_path_entry() {
        let tmp = TempDir::new().unwrap();
        let blob_path = tmp.path().join("blob.tar.gz");
        // A malicious registry could ship a tar entry whose header path
        // starts with `/`. `tar::Builder::append_data` refuses to write
        // such a path; the only realistic way to construct this payload
        // is to populate the header name field directly with raw bytes,
        // which is exactly what an attacker would do.
        let bytes = build_gzipped_tar_raw_path(b"/etc/shadow", b"pwned", 0o644);
        std::fs::write(&blob_path, &bytes).unwrap();

        let dest = TempDir::new().unwrap();
        let err = extract_gzipped_tar(
            &blob_path,
            dest.path(),
            "application/vnd.oci.image.layer.v1.tar+gzip",
        )
        .expect_err("absolute paths must be rejected");
        let msg = err.to_string();
        assert!(
            msg.contains("absolute") || msg.contains("escape") || msg.contains("rejected"),
            "expected absolute-path rejection, got: {msg}"
        );
        // The host file must NOT exist under the dest.
        assert!(!dest.path().join("etc/shadow").exists());
    }

    #[test]
    fn extract_rejects_parent_dir_entry() {
        let tmp = TempDir::new().unwrap();
        let blob_path = tmp.path().join("blob.tar.gz");
        let bytes = build_gzipped_tar_raw_path(b"../escape", b"pwned", 0o644);
        std::fs::write(&blob_path, &bytes).unwrap();

        let dest = TempDir::new().unwrap();
        let err = extract_gzipped_tar(
            &blob_path,
            dest.path(),
            "application/vnd.oci.image.layer.v1.tar+gzip",
        )
        .expect_err("parent-dir traversal must be rejected");
        let msg = err.to_string();
        assert!(
            msg.contains("parent") || msg.contains("escape") || msg.contains("rejected"),
            "expected parent-dir rejection, got: {msg}"
        );
        assert!(!dest.path().parent().unwrap().join("escape").exists());
    }

    #[test]
    fn extract_rejects_symlink_escaping_dest() {
        let tmp = TempDir::new().unwrap();
        let blob_path = tmp.path().join("blob.tar.gz");
        // A symlink whose target escapes `dest` via `..` must be rejected.
        // (Absolute IN-ROOTFS targets like `/bin/busybox` are now accepted —
        // see extract_accepts_rootfs_absolute_symlink — but a target that
        // resolves outside the rootfs root is still refused.)
        let bytes = build_tar_with_link("etc/passwd", "../../../../etc/passwd", true);
        std::fs::write(&blob_path, &bytes).unwrap();

        let dest = TempDir::new().unwrap();
        let err = extract_gzipped_tar(
            &blob_path,
            dest.path(),
            "application/vnd.oci.image.layer.v1.tar+gzip",
        )
        .expect_err("symlink target escaping dest must be rejected");
        let msg = err.to_string();
        assert!(
            msg.contains("escape") || msg.contains("link") || msg.contains("rejected"),
            "expected symlink-escape rejection, got: {msg}"
        );
    }

    #[test]
    fn extract_accepts_rootfs_absolute_symlink() {
        // Real base images ship absolute symlinks pointing within the rootfs
        // (e.g. busybox `bin/arch -> /bin/busybox`, ubuntu
        // `/etc/alternatives/* -> /usr/bin/*`). These are ROOTFS-absolute and
        // must extract cleanly, preserved verbatim (they resolve against the
        // guest root at runtime). Rejecting them broke extraction of every real
        // image.
        let tmp = TempDir::new().unwrap();
        let blob_path = tmp.path().join("blob.tar.gz");
        let bytes = build_tar_with_link("bin/arch", "/bin/busybox", true);
        std::fs::write(&blob_path, &bytes).unwrap();

        let dest = TempDir::new().unwrap();
        extract_gzipped_tar(
            &blob_path,
            dest.path(),
            "application/vnd.oci.image.layer.v1.tar+gzip",
        )
        .expect("rootfs-absolute symlink target must extract successfully");

        let link = dest.path().join("bin/arch");
        let meta = std::fs::symlink_metadata(&link).expect("symlink should exist");
        assert!(meta.file_type().is_symlink(), "bin/arch must be a symlink");
        assert_eq!(
            std::fs::read_link(&link).unwrap(),
            std::path::PathBuf::from("/bin/busybox"),
            "absolute symlink target must be preserved verbatim"
        );
    }

    #[test]
    fn extract_rejects_hardlink_pointing_outside_dest() {
        let tmp = TempDir::new().unwrap();
        let blob_path = tmp.path().join("blob.tar.gz");
        let bytes = build_tar_with_link("etc/passwd", "../../../../etc/passwd", false);
        std::fs::write(&blob_path, &bytes).unwrap();

        let dest = TempDir::new().unwrap();
        let err = extract_gzipped_tar(
            &blob_path,
            dest.path(),
            "application/vnd.oci.image.layer.v1.tar+gzip",
        )
        .expect_err("hardlink to outside dest must be rejected");
        let msg = err.to_string();
        assert!(
            msg.contains("escape") || msg.contains("link") || msg.contains("rejected"),
            "expected hardlink-escape rejection, got: {msg}"
        );
    }
}
