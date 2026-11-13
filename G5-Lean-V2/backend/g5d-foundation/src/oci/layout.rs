//! Local OCI image-layout directory source (ontology
//! `software_architecture/g5d/oci/image_reference_and_entrypoint` D9,
//! `software_architecture/g5d/oci/extract_and_cache` D31).
//!
//! A directory produced by `docker buildx build --output type=oci,dest=<dir>`
//! carries the standard image-layout entries: the `oci-layout` marker file,
//! `index.json`, and content-addressed blobs under `blobs/<algorithm>/<hex>`.
//! Reading it directly gives locally built guest images a path into the
//! microVM host WITH their image config (Env included) — the bare-rootfs
//! directory input deliberately has no config, and losing `config.Env` is a
//! recorded failure mode (experience
//! `api_worker_host_private_runtime_image_and_oci_env`).
//!
//! The reader is a filesystem-backed [`RegistryClient`], so template caching,
//! whiteout application, digest verification, and per-VM GC are the SAME code
//! the remote registry path proves — never a parallel assembly pipeline.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde::Deserialize;
use sha2::{Digest as Sha256Digest, Sha256};

use super::error::OciError;
use super::registry::{
    current_arch, decode_image_config_document, Descriptor, Digest, ImageConfig, ImageRef,
    Manifest, RegistryClient, MEDIA_DOCKER_LIST, MEDIA_DOCKER_MANIFEST, MEDIA_OCI_INDEX,
    MEDIA_OCI_MANIFEST,
};

/// Marker file the OCI image-layout specification requires at the layout root.
const OCI_LAYOUT_MARKER: &str = "oci-layout";
const INDEX_FILE: &str = "index.json";

/// Routing predicate for a path-shaped image input: an existing directory
/// containing both required top-level layout entries. A plain pre-extracted
/// rootfs directory contains neither, so the historical bare-directory
/// semantics are unaffected (microvm_runtime D56).
pub fn is_oci_layout_dir(path: &Path) -> bool {
    path.is_dir() && path.join(OCI_LAYOUT_MARKER).is_file() && path.join(INDEX_FILE).is_file()
}

#[derive(Debug, Deserialize)]
struct LayoutIndex {
    manifests: Vec<LayoutIndexEntry>,
}

#[derive(Debug, Deserialize)]
struct LayoutIndexEntry {
    #[serde(rename = "mediaType")]
    media_type: String,
    digest: String,
    #[serde(default)]
    platform: Option<LayoutPlatform>,
}

#[derive(Debug, Deserialize)]
struct LayoutPlatform {
    architecture: String,
    os: String,
}

impl LayoutIndexEntry {
    fn is_manifest(&self) -> bool {
        self.media_type == MEDIA_OCI_MANIFEST || self.media_type == MEDIA_DOCKER_MANIFEST
    }

    fn is_index(&self) -> bool {
        self.media_type == MEDIA_OCI_INDEX || self.media_type == MEDIA_DOCKER_LIST
    }

    /// buildx appends provenance/SBOM attestation manifests with the
    /// placeholder `unknown/unknown` platform; they are not runnable images.
    fn is_attestation(&self) -> bool {
        self.platform
            .as_ref()
            .is_some_and(|p| p.os == "unknown" || p.architecture == "unknown")
    }

    fn matches_host(&self, want_arch: &str) -> bool {
        self.platform
            .as_ref()
            .is_some_and(|p| p.os == "linux" && p.architecture == want_arch)
    }
}

/// Filesystem-backed [`RegistryClient`] over one OCI image-layout directory.
/// The `ImageRef` arguments are log context only — content is addressed
/// purely by digest inside the layout.
pub(crate) struct OciLayoutClient {
    root: PathBuf,
}

impl OciLayoutClient {
    pub(crate) fn new(root: &Path) -> Result<Self, OciError> {
        if !is_oci_layout_dir(root) {
            return Err(OciError::ManifestDecode(format!(
                "'{}' is not an OCI image-layout directory (missing '{}' or '{}')",
                root.display(),
                OCI_LAYOUT_MARKER,
                INDEX_FILE,
            )));
        }
        Ok(Self {
            root: root.to_path_buf(),
        })
    }

    /// Content-addressed blob path. Routed through the typed [`Digest`] so a
    /// hostile digest string in a layout document can never traverse out of
    /// `blobs/` (same rule as the remote cache paths).
    fn blob_path(&self, digest: &str) -> Result<PathBuf, OciError> {
        let parsed = Digest::parse(digest)?;
        Ok(self.root.join("blobs").join("sha256").join(parsed.hex()))
    }

    fn read_blob_verified(&self, digest: &str) -> Result<Vec<u8>, OciError> {
        let path = self.blob_path(digest)?;
        let bytes = std::fs::read(&path).map_err(|e| OciError::CacheIo {
            path: path.display().to_string(),
            source: e,
        })?;
        let actual = format!("sha256:{}", hex::encode(Sha256::digest(&bytes)));
        let expected = Digest::parse(digest)?;
        if actual != expected.as_str() {
            return Err(OciError::DigestMismatch {
                expected: expected.as_str().to_string(),
                actual,
            });
        }
        Ok(bytes)
    }

    fn read_index(&self, path: &Path) -> Result<LayoutIndex, OciError> {
        let bytes = std::fs::read(path).map_err(|e| OciError::CacheIo {
            path: path.display().to_string(),
            source: e,
        })?;
        serde_json::from_slice(&bytes).map_err(|e| {
            OciError::ManifestDecode(format!(
                "layout index decode failed ({}): {e}",
                path.display()
            ))
        })
    }

    /// Select the single runnable manifest for this host from an index's
    /// entries: a platform-matching manifest wins; a nested index recurses
    /// one level (buildx multi-platform exports); a single platform-less
    /// manifest entry is accepted as-is (single-image exports omit platform).
    fn select_manifest_digest(
        &self,
        entries: &[LayoutIndexEntry],
        want_arch: &str,
        allow_recurse: bool,
    ) -> Result<String, OciError> {
        if let Some(entry) = entries
            .iter()
            .find(|e| e.is_manifest() && !e.is_attestation() && e.matches_host(want_arch))
        {
            return Ok(entry.digest.clone());
        }
        let manifests: Vec<&LayoutIndexEntry> = entries
            .iter()
            .filter(|e| e.is_manifest() && !e.is_attestation())
            .collect();
        if let [only] = manifests.as_slice() {
            if only.platform.is_none() {
                return Ok(only.digest.clone());
            }
        }
        if allow_recurse {
            for entry in entries.iter().filter(|e| e.is_index()) {
                let bytes = self.read_blob_verified(&entry.digest)?;
                let nested: LayoutIndex = serde_json::from_slice(&bytes).map_err(|e| {
                    OciError::ManifestDecode(format!("nested layout index decode failed: {e}"))
                })?;
                if let Ok(digest) = self.select_manifest_digest(&nested.manifests, want_arch, false)
                {
                    return Ok(digest);
                }
            }
        }
        Err(OciError::NoMatchingPlatform {
            arch: want_arch.to_string(),
        })
    }
}

#[async_trait]
impl RegistryClient for OciLayoutClient {
    /// The layout's immutable identity (`extract_and_cache` D32): the digest
    /// of the host-architecture runnable manifest selected from `index.json`
    /// — exactly the manifest `fetch_manifest` boots — so a fence keyed on it
    /// names the bytes the VM will run. The `ImageRef` is log context only.
    async fn resolve_manifest_digest(&self, _image: &ImageRef) -> Result<Digest, OciError> {
        let index = self.read_index(&self.root.join(INDEX_FILE))?;
        let digest = self.select_manifest_digest(&index.manifests, current_arch(), true)?;
        Digest::parse(&digest)
    }

    async fn fetch_manifest(&self, _image: &ImageRef) -> Result<Manifest, OciError> {
        let index = self.read_index(&self.root.join(INDEX_FILE))?;
        let digest = self.select_manifest_digest(&index.manifests, current_arch(), true)?;
        let bytes = self.read_blob_verified(&digest)?;
        serde_json::from_slice(&bytes)
            .map_err(|e| OciError::ManifestDecode(format!("layout manifest decode failed: {e}")))
    }

    async fn fetch_config(
        &self,
        _image: &ImageRef,
        descriptor: &Descriptor,
    ) -> Result<ImageConfig, OciError> {
        let bytes = self.read_blob_verified(&descriptor.digest)?;
        decode_image_config_document(&bytes)
    }

    async fn fetch_layer_blob(
        &self,
        _image: &ImageRef,
        descriptor: &Descriptor,
        dest_path: &Path,
    ) -> Result<(), OciError> {
        if descriptor.media_type.contains("zstd") {
            return Err(OciError::UnsupportedMediaType(
                descriptor.media_type.clone(),
            ));
        }
        // Layers can be large: hash while copying in bounded chunks instead
        // of loading the whole blob into memory, and keep the synchronous
        // filesystem work off the async executor (CodeRabbit stability
        // review). The bytes land in a same-directory temp file and rename
        // into place only after the digest matches, so `dest_path` never
        // holds unverified content; `create_new` keeps the trait's
        // exclusive-create contract.
        let src_path = self.blob_path(&descriptor.digest)?;
        let expected = Digest::parse(&descriptor.digest)?;
        let dest_path = dest_path.to_path_buf();
        tokio::task::spawn_blocking(move || -> Result<(), OciError> {
            let io_err = |path: &Path, e: std::io::Error| OciError::CacheIo {
                path: path.display().to_string(),
                source: e,
            };
            let mut src = std::fs::File::open(&src_path).map_err(|e| io_err(&src_path, e))?;
            // Per-ATTEMPT temp name (uuid, not pid: concurrent fetches of the
            // same dest inside one process share a pid and would collide on
            // `create_new`), and a drop guard so EVERY exit path —
            // read/write error, digest mismatch, publication failure, and
            // success — removes the temp file instead of leaking partials
            // that would wedge a retry.
            let tmp_path = dest_path.with_extension(format!("verify-tmp-{}", uuid::Uuid::now_v7()));
            struct TmpCleanup(std::path::PathBuf);
            impl Drop for TmpCleanup {
                fn drop(&mut self) {
                    let _ = std::fs::remove_file(&self.0);
                }
            }
            let _cleanup = TmpCleanup(tmp_path.clone());
            let mut tmp = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&tmp_path)
                .map_err(|e| io_err(&tmp_path, e))?;
            let mut hasher = Sha256::new();
            let mut buffer = vec![0_u8; 1024 * 1024];
            loop {
                let read =
                    std::io::Read::read(&mut src, &mut buffer).map_err(|e| io_err(&src_path, e))?;
                if read == 0 {
                    break;
                }
                hasher.update(&buffer[..read]);
                std::io::Write::write_all(&mut tmp, &buffer[..read])
                    .map_err(|e| io_err(&tmp_path, e))?;
            }
            drop(tmp);
            let actual = format!("sha256:{}", hex::encode(hasher.finalize()));
            if actual != expected.as_str() {
                return Err(OciError::DigestMismatch {
                    expected: expected.as_str().to_string(),
                    actual,
                });
            }
            // Atomic no-replace publication: `hard_link` fails with
            // `AlreadyExists` instead of replacing, and `dest_path` only ever
            // appears fully verified — unlike create_new-then-rename, which
            // exposes an empty claimed dest to concurrent readers and then
            // replaces it. The guard removes the temp link source.
            std::fs::hard_link(&tmp_path, &dest_path).map_err(|e| io_err(&dest_path, e))?;
            Ok(())
        })
        .await
        .map_err(|e| OciError::CacheIo {
            path: "layout layer copy task".to_string(),
            source: std::io::Error::other(e),
        })?
    }
}

#[cfg(test)]
mod tests {
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use serde_json::json;
    use sha2::{Digest as Sha256Digest, Sha256};
    use tar::{Builder, Header};

    use super::*;
    use crate::oci::cache::LayerCache;
    use crate::oci::extract::OciExtractor;

    fn build_gzipped_tar(entries: &[(&str, &[u8], u32)]) -> Vec<u8> {
        let mut gz = GzEncoder::new(Vec::new(), Compression::fast());
        {
            let mut tar = Builder::new(&mut gz);
            for (path, data, mode) in entries {
                let mut header = Header::new_gnu();
                header.set_size(data.len() as u64);
                header.set_mode(*mode);
                header.set_cksum();
                tar.append_data(&mut header, *path, &mut std::io::Cursor::new(*data))
                    .unwrap();
            }
            tar.finish().unwrap();
        }
        gz.finish().unwrap()
    }

    fn sha256_digest(bytes: &[u8]) -> String {
        format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
    }

    struct LayoutFixture {
        _dir: tempfile::TempDir,
        root: PathBuf,
        /// Hex (no `sha256:` prefix) of the gzip LAYER blob, so tests that
        /// corrupt the layer target exactly it — the manifest can be the
        /// largest blob in the store, and corrupting that instead makes a
        /// layer-copy cleanup assertion pass vacuously.
        layer_hex: String,
    }

    /// Write a complete, digest-consistent image layout: one gzip layer, a
    /// config carrying `env`, a manifest, and an index — optionally nesting
    /// the platform index one level, as buildx multi-platform exports do.
    fn write_layout(env: &[&str], nested: bool) -> LayoutFixture {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let blobs = root.join("blobs").join("sha256");
        std::fs::create_dir_all(&blobs).unwrap();
        let put = |bytes: &[u8]| -> String {
            let digest = sha256_digest(bytes);
            let hex = digest.strip_prefix("sha256:").unwrap();
            std::fs::write(blobs.join(hex), bytes).unwrap();
            digest
        };

        let layer = build_gzipped_tar(&[("etc/marker", b"layout-image", 0o644)]);
        let layer_digest = put(&layer);
        let config = serde_json::to_vec(&json!({ "config": { "Env": env } })).unwrap();
        let config_digest = put(&config);
        let manifest = serde_json::to_vec(&json!({
            "schemaVersion": 2,
            "mediaType": "application/vnd.oci.image.manifest.v1+json",
            "config": {
                "mediaType": "application/vnd.oci.image.config.v1+json",
                "digest": config_digest,
                "size": config.len(),
            },
            "layers": [{
                "mediaType": "application/vnd.oci.image.layer.v1.tar+gzip",
                "digest": layer_digest,
                "size": layer.len(),
            }],
        }))
        .unwrap();
        let manifest_digest = put(&manifest);

        let manifest_entry = json!({
            "mediaType": "application/vnd.oci.image.manifest.v1+json",
            "digest": manifest_digest,
            "size": manifest.len(),
            "platform": { "os": "linux", "architecture": current_arch() },
        });
        let attestation_entry = json!({
            "mediaType": "application/vnd.oci.image.manifest.v1+json",
            "digest": manifest_digest,
            "size": manifest.len(),
            "platform": { "os": "unknown", "architecture": "unknown" },
        });
        let top_manifests = if nested {
            let nested_index = serde_json::to_vec(&json!({
                "schemaVersion": 2,
                "manifests": [attestation_entry, manifest_entry],
            }))
            .unwrap();
            let nested_digest = put(&nested_index);
            json!([{
                "mediaType": "application/vnd.oci.image.index.v1+json",
                "digest": nested_digest,
                "size": nested_index.len(),
            }])
        } else {
            json!([manifest_entry])
        };
        std::fs::write(
            root.join(INDEX_FILE),
            serde_json::to_vec(&json!({ "schemaVersion": 2, "manifests": top_manifests })).unwrap(),
        )
        .unwrap();
        std::fs::write(
            root.join(OCI_LAYOUT_MARKER),
            br#"{"imageLayoutVersion":"1.0.0"}"#,
        )
        .unwrap();
        let layer_hex = layer_digest.strip_prefix("sha256:").unwrap().to_string();
        LayoutFixture {
            _dir: dir,
            root,
            layer_hex,
        }
    }

    fn extractor(cache_root: &Path) -> OciExtractor {
        OciExtractor::with_components(
            LayerCache::new(cache_root.to_path_buf()),
            Box::new(crate::oci::registry::DockerHubClient::new()),
        )
    }

    #[test]
    fn detection_requires_both_layout_entries() {
        let fixture = write_layout(&[], false);
        assert!(is_oci_layout_dir(&fixture.root));
        let bare = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(bare.path().join("etc")).unwrap();
        assert!(!is_oci_layout_dir(bare.path()));
    }

    #[tokio::test]
    async fn layout_manifest_digest_names_the_runnable_manifest_and_refuses_bare_trees() {
        // Flat and nested (buildx multi-platform) exports of the SAME image
        // resolve to the same manifest digest — the identity names the bytes
        // the VM boots, not the index shape (extract_and_cache D32).
        let flat = write_layout(&["FOO=bar"], false);
        let nested = write_layout(&["FOO=bar"], true);
        let state = tempfile::tempdir().unwrap();
        let ex = extractor(&state.path().join("cache"));
        let flat_digest = ex
            .layout_manifest_digest(&flat.root)
            .await
            .expect("flat layout digest");
        let nested_digest = ex
            .layout_manifest_digest(&nested.root)
            .await
            .expect("nested layout digest");
        assert_eq!(flat_digest, nested_digest);
        let manifest_bytes = std::fs::read(
            flat.root
                .join("blobs")
                .join("sha256")
                .join(flat_digest.hex()),
        )
        .expect("digest addresses a blob in the layout");
        assert_eq!(sha256_digest(&manifest_bytes), flat_digest.as_str());

        // A bare rootfs directory has no manifest: no identity is invented.
        let bare = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(bare.path().join("etc")).unwrap();
        let err = ex
            .layout_manifest_digest(bare.path())
            .await
            .expect_err("bare tree must be refused");
        assert!(
            err.to_string()
                .contains("not an OCI image-layout directory"),
            "unexpected error: {err}"
        );
    }

    #[tokio::test]
    async fn layout_materialization_preserves_env_and_reuses_template() {
        let fixture = write_layout(&["G5_BROWSER_TOOLS_AVAILABLE=true", "FOO=bar"], false);
        let state = tempfile::tempdir().unwrap();
        let ex = extractor(&state.path().join("cache"));
        let template_root = state.path().join("templates");

        let first = ex
            .materialize_rootfs_cow_from_layout(
                &fixture.root,
                &template_root,
                &state.path().join("vm-1"),
            )
            .await
            .expect("first layout materialization");
        assert_eq!(
            first.image_config.env,
            vec![
                "G5_BROWSER_TOOLS_AVAILABLE=true".to_string(),
                "FOO=bar".to_string()
            ],
            "image config env must survive layout delivery (D31)"
        );
        assert_eq!(
            std::fs::read(first.rootfs.join("etc/marker")).unwrap(),
            b"layout-image"
        );

        // Second materialization: template hit (template dir count unchanged),
        // distinct per-VM rootfs, same env.
        let templates_after_first = std::fs::read_dir(template_root.join("sha256"))
            .unwrap()
            .count();
        let second = ex
            .materialize_rootfs_cow_from_layout(
                &fixture.root,
                &template_root,
                &state.path().join("vm-2"),
            )
            .await
            .expect("second layout materialization");
        assert_eq!(second.image_config.env, first.image_config.env);
        assert_eq!(
            std::fs::read_dir(template_root.join("sha256"))
                .unwrap()
                .count(),
            templates_after_first,
            "same layout content must reuse one template"
        );
        assert_ne!(first.rootfs, second.rootfs);
    }

    #[tokio::test]
    async fn nested_index_resolves_host_platform_and_skips_attestations() {
        let fixture = write_layout(&["NESTED=yes"], true);
        let state = tempfile::tempdir().unwrap();
        let ex = extractor(&state.path().join("cache"));
        let out = ex
            .materialize_rootfs_cow_from_layout(
                &fixture.root,
                &state.path().join("templates"),
                &state.path().join("vm-1"),
            )
            .await
            .expect("nested-index layout materialization");
        assert_eq!(out.image_config.env, vec!["NESTED=yes".to_string()]);
    }

    #[tokio::test]
    async fn corrupted_blob_fails_with_digest_mismatch() {
        let fixture = write_layout(&[], false);
        // Corrupt the LAYER blob in place: keep the filename (digest), change
        // the bytes. Target the layer digest exactly — picking the largest
        // blob can select the manifest, which fails before `fetch_layer_blob`
        // ever creates its verify-tmp file and makes the cleanup assertion
        // below vacuous.
        let layer_file = fixture
            .root
            .join("blobs")
            .join("sha256")
            .join(&fixture.layer_hex);
        assert!(layer_file.is_file(), "fixture layer blob must exist");
        std::fs::write(&layer_file, b"corrupted bytes").unwrap();
        let state = tempfile::tempdir().unwrap();
        let ex = extractor(&state.path().join("cache"));
        let err = ex
            .materialize_rootfs_cow_from_layout(
                &fixture.root,
                &state.path().join("templates"),
                &state.path().join("vm-1"),
            )
            .await
            .expect_err("corrupted blob must fail loud");
        assert!(
            matches!(err, OciError::DigestMismatch { .. }),
            "expected DigestMismatch, got {err:?}"
        );
        // The failed fetch must not leak its verify-tmp file — a leftover
        // partial would otherwise sit beside the cache forever.
        let leftovers: Vec<_> = walkdir_files(state.path())
            .into_iter()
            .filter(|p| {
                p.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .contains("verify-tmp")
            })
            .collect();
        assert!(
            leftovers.is_empty(),
            "failed fetch leaked partial files: {leftovers:?}"
        );
    }

    /// Same-process concurrency: two fetches of the SAME dest each use their
    /// own per-attempt verify-tmp (uuid suffix — a pid suffix would collide on
    /// `create_new`), exactly one wins the atomic no-replace publication, the
    /// loser fails with AlreadyExists, and neither leaks a temp file.
    #[tokio::test]
    async fn concurrent_same_dest_layer_fetches_publish_exactly_once() {
        let fixture = write_layout(&[], false);
        let client = OciLayoutClient::new(&fixture.root).unwrap();
        let layer_bytes = std::fs::read(
            fixture
                .root
                .join("blobs")
                .join("sha256")
                .join(&fixture.layer_hex),
        )
        .unwrap();
        let descriptor = Descriptor {
            media_type: "application/vnd.oci.image.layer.v1.tar+gzip".to_string(),
            digest: format!("sha256:{}", fixture.layer_hex),
            size: layer_bytes.len() as u64,
        };
        let image = ImageRef {
            registry: "oci-layout".to_string(),
            repository: fixture.root.display().to_string(),
            reference: crate::oci::registry::Reference::Tag("local".to_string()),
        };
        let dest_dir = tempfile::tempdir().unwrap();
        let dest = dest_dir.path().join(&fixture.layer_hex);

        let (a, b) = tokio::join!(
            client.fetch_layer_blob(&image, &descriptor, &dest),
            client.fetch_layer_blob(&image, &descriptor, &dest),
        );
        let ok_count = [&a, &b].iter().filter(|r| r.is_ok()).count();
        assert_eq!(ok_count, 1, "exactly one publication must win: {a:?} {b:?}");
        let loser = if a.is_err() { a } else { b };
        assert!(
            loser
                .unwrap_err()
                .to_string()
                .to_lowercase()
                .contains("exists"),
            "loser must fail on the no-replace publication"
        );
        assert_eq!(std::fs::read(&dest).unwrap(), layer_bytes);
        let leftovers: Vec<_> = walkdir_files(dest_dir.path())
            .into_iter()
            .filter(|p| {
                p.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .contains("verify-tmp")
            })
            .collect();
        assert!(
            leftovers.is_empty(),
            "leaked verify-tmp files: {leftovers:?}"
        );
    }

    fn walkdir_files(root: &std::path::Path) -> Vec<std::path::PathBuf> {
        let mut files = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else {
                    files.push(path);
                }
            }
        }
        files
    }
}
