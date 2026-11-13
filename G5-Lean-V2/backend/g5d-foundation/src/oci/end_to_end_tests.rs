//! End-to-end OCI extract tests against a mocked DockerHub registry.
//!
//! Exercises the full auth → manifest → layer-blob → extract → assemble
//! pipeline without touching the real DockerHub, so the tests stay
//! deterministic and runnable offline.

use std::path::PathBuf;
use std::sync::Arc;

use flate2::write::GzEncoder;
use flate2::Compression;
use sha2::{Digest, Sha256};
use tar::{Builder, Header};
use tempfile::TempDir;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::cache::LayerCache;
use super::extract::OciExtractor;
use super::registry::{DockerHubClient, ImageRef, Reference, RegistryClient};

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

struct MockedRegistry {
    server: MockServer,
    host: String, // host:port without scheme
    layer_bytes: Vec<u8>,
    layer_digest: String,
    config_digest: String,
    manifest_digest: String,
}

async fn setup_mocked_registry(
    repository: &str,
    reference_tag: &str,
    layer_entries: &[(&str, &[u8], u32)],
) -> MockedRegistry {
    setup_mocked_registry_layers(
        repository,
        reference_tag,
        &[build_gzipped_tar(layer_entries)],
    )
    .await
}

async fn setup_mocked_registry_layers(
    repository: &str,
    reference_tag: &str,
    layers: &[Vec<u8>],
) -> MockedRegistry {
    let server = MockServer::start().await;
    let host_with_scheme = server.uri();
    let host = host_with_scheme
        .strip_prefix("http://")
        .expect("MockServer URI begins with http://")
        .to_string();

    let layer_bytes = layers[0].clone();
    let layer_digest = sha256_digest(&layer_bytes);

    let config_bytes = serde_json::to_vec(&serde_json::json!({
        "config": {
            "Env": [
                "JAVA_HOME=/opt/java/openjdk",
                "MAVEN_HOME=/usr/share/maven",
                "PATH=/usr/share/maven/bin:/opt/java/openjdk/bin:/usr/bin"
            ]
        }
    }))
    .unwrap();
    let config_digest = sha256_digest(&config_bytes);

    let manifest_json = serde_json::json!({
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.manifest.v1+json",
        "config": {
            "mediaType": "application/vnd.oci.image.config.v1+json",
            "digest": config_digest,
            "size": config_bytes.len(),
        },
        "layers": layers.iter().map(|bytes| serde_json::json!({
            "mediaType": "application/vnd.oci.image.layer.v1.tar+gzip",
            "digest": sha256_digest(bytes),
            "size": bytes.len(),
        })).collect::<Vec<_>>()
    });
    let manifest_digest = sha256_digest(&serde_json::to_vec(&manifest_json).unwrap());

    Mock::given(method("GET"))
        .and(path(format!("/v2/{}/manifests/{}", repository, reference_tag)))
        .and(|request: &wiremock::Request| !request.headers.contains_key("Authorization"))
        .respond_with(
            ResponseTemplate::new(401).insert_header(
                "Www-Authenticate",
                format!(
                    r#"Bearer realm="{}/token",service="registry.docker.io",scope="repository:{}:pull""#,
                    host_with_scheme, repository
                )
                .as_str(),
            ),
        )
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "token": "test-bearer-token",
            "expires_in": 300,
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path(format!(
            "/v2/{}/manifests/{}",
            repository, reference_tag
        )))
        .and(header("Authorization", "Bearer test-bearer-token"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/vnd.oci.image.manifest.v1+json")
                .set_body_json(manifest_json.clone()),
        )
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path(format!(
            "/v2/{repository}/manifests/{manifest_digest}"
        )))
        .and(header("Authorization", "Bearer test-bearer-token"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/vnd.oci.image.manifest.v1+json")
                .set_body_json(manifest_json),
        )
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/v2/{}/blobs/{}", repository, config_digest)))
        .and(header("Authorization", "Bearer test-bearer-token"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/vnd.oci.image.config.v1+json")
                .set_body_bytes(config_bytes.clone()),
        )
        .mount(&server)
        .await;

    for bytes in layers {
        Mock::given(method("GET"))
            .and(path(format!(
                "/v2/{repository}/blobs/{}",
                sha256_digest(bytes)
            )))
            .and(header("Authorization", "Bearer test-bearer-token"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header(
                        "Content-Type",
                        "application/vnd.oci.image.layer.v1.tar+gzip",
                    )
                    .set_body_bytes(bytes.clone()),
            )
            .mount(&server)
            .await;
    }

    MockedRegistry {
        server,
        host,
        layer_bytes,
        layer_digest,
        config_digest,
        manifest_digest,
    }
}

fn test_extractor(cache_root: PathBuf) -> OciExtractor {
    let http = reqwest::Client::builder()
        .user_agent("g5d-oci-test/0.1.0")
        .build()
        .unwrap();
    OciExtractor::with_components(
        LayerCache::new(cache_root),
        Box::new(DockerHubClient::new_for_tests(http)),
    )
}

// Exercise the public parsed-reference entry points against a local registry.
struct LocalRegistry {
    host: String,
    client: DockerHubClient,
    pause_first_layer: Option<Arc<tokio::sync::Notify>>,
    layer_started: std::sync::atomic::AtomicBool,
}

impl LocalRegistry {
    fn image(&self, image: &ImageRef) -> ImageRef {
        ImageRef {
            registry: self.host.clone(),
            ..image.clone()
        }
    }
}

#[async_trait::async_trait]
impl RegistryClient for LocalRegistry {
    async fn fetch_manifest(
        &self,
        image: &ImageRef,
    ) -> Result<super::registry::Manifest, super::OciError> {
        self.client.fetch_manifest(&self.image(image)).await
    }

    async fn fetch_config(
        &self,
        image: &ImageRef,
        descriptor: &super::registry::Descriptor,
    ) -> Result<super::ImageConfig, super::OciError> {
        self.client
            .fetch_config(&self.image(image), descriptor)
            .await
    }

    async fn fetch_layer_blob(
        &self,
        image: &ImageRef,
        descriptor: &super::registry::Descriptor,
        dest: &std::path::Path,
    ) -> Result<(), super::OciError> {
        if let Some(notify) = &self.pause_first_layer {
            if !self
                .layer_started
                .swap(true, std::sync::atomic::Ordering::SeqCst)
            {
                tokio::fs::write(dest, b"partially downloaded blob")
                    .await
                    .map_err(|source| super::OciError::CacheIo {
                        path: dest.display().to_string(),
                        source,
                    })?;
                notify.notify_one();
                std::future::pending::<()>().await;
            }
        }
        self.client
            .fetch_layer_blob(&self.image(image), descriptor, dest)
            .await
    }
}

fn preparation_extractor(cache_root: PathBuf, registry: &MockedRegistry) -> OciExtractor {
    OciExtractor::with_components(
        LayerCache::new(cache_root),
        Box::new(LocalRegistry {
            host: registry.host.clone(),
            client: DockerHubClient::new_for_tests(reqwest::Client::new()),
            pause_first_layer: None,
            layer_started: std::sync::atomic::AtomicBool::new(false),
        }),
    )
}

/// Full registry → extraction → cached-layer merge → template/clone boundary.
/// All outside paths are disposable test fixtures, never live host locations.
#[cfg(unix)]
#[tokio::test]
async fn malicious_cross_layer_symlink_is_contained_in_direct_and_template_assembly() {
    for operation in ["write", "named-whiteout", "opaque-whiteout"] {
        let outside = TempDir::new().unwrap();
        std::fs::write(outside.path().join("victim"), b"host data").unwrap();
        let mut gz = GzEncoder::new(Vec::new(), Compression::fast());
        {
            let mut tar = Builder::new(&mut gz);
            let mut header = Header::new_gnu();
            header.set_size(0);
            header.set_mode(0o777);
            header.set_entry_type(tar::EntryType::Symlink);
            header.set_link_name(outside.path()).unwrap();
            header.set_cksum();
            tar.append_data(&mut header, "x", std::io::empty()).unwrap();
            tar.finish().unwrap();
        }
        let lower = gz.finish().unwrap();
        let entry = match operation {
            "write" => "x/victim",
            "named-whiteout" => "x/.wh.victim",
            _ => "x/.wh..wh..opq",
        };
        let upper = build_gzipped_tar(&[(entry, b"image data", 0o644), ("x/new", b"new", 0o644)]);
        let mocked = setup_mocked_registry_layers("library/alpine", "3.18", &[lower, upper]).await;
        let state = TempDir::new().unwrap();
        let extractor = preparation_extractor(state.path().join("cache"), &mocked);
        let direct = state.path().join("direct");
        extractor
            .materialize_rootfs("alpine:3.18", &direct)
            .await
            .unwrap();
        assert_eq!(
            std::fs::read(outside.path().join("victim")).unwrap(),
            b"host data"
        );
        assert!(!outside.path().join("new").exists());
        assert!(std::fs::symlink_metadata(direct.join("x"))
            .unwrap()
            .is_dir());
        assert_eq!(std::fs::read(direct.join("x/new")).unwrap(), b"new");
        let templates = state.path().join("templates");
        // Concurrent boot and preparation share the build lock and cached
        // layers; a subsequent boot exercises the completed template hit.
        let first_vm = state.path().join("vm1");
        let (prepared, booted) = tokio::join!(
            extractor.prepare_rootfs_template("alpine:3.18", &templates),
            extractor.materialize_rootfs_cow("alpine:3.18", &templates, &first_vm),
        );
        prepared.unwrap();
        booted.unwrap();
        let second_vm = state.path().join("vm2");
        extractor
            .materialize_rootfs_cow("alpine:3.18", &templates, &second_vm)
            .await
            .unwrap();
        for vm in [&first_vm, &second_vm] {
            assert!(std::fs::symlink_metadata(vm.join("x")).unwrap().is_dir());
            assert_eq!(std::fs::read(vm.join("x/new")).unwrap(), b"new");
            if operation == "write" {
                assert_eq!(std::fs::read(vm.join("x/victim")).unwrap(), b"image data");
            } else {
                assert!(!vm.join("x/victim").exists());
            }
        }
        assert_eq!(
            std::fs::read(outside.path().join("victim")).unwrap(),
            b"host data"
        );
        assert!(!outside.path().join("new").exists());
    }
}

#[tokio::test]
async fn invalid_whiteout_never_publishes_a_template_and_retry_stays_contained() {
    let mocked = setup_mocked_registry("library/alpine", "3.18", &[(".wh...", b"", 0o644)]).await;
    let state = TempDir::new().unwrap();
    let templates = state.path().join("templates");
    let extractor = preparation_extractor(state.path().join("cache"), &mocked);
    std::fs::write(state.path().join("keep"), b"host data").unwrap();
    for _ in 0..2 {
        let error = extractor
            .prepare_rootfs_template("alpine:3.18", &templates)
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains("invalid OCI whiteout"),
            "{error}"
        );
        assert_eq!(
            std::fs::read(state.path().join("keep")).unwrap(),
            b"host data"
        );
        assert_eq!(
            std::fs::read_dir(templates.join("sha256")).unwrap().count(),
            0,
            "failed merge must remove its partial template and lock"
        );
    }
    let valid =
        setup_mocked_registry("library/alpine", "3.18", &[("etc/valid", b"ok", 0o644)]).await;
    let extractor = preparation_extractor(state.path().join("cache"), &valid);
    let vm = state.path().join("vm");
    extractor
        .materialize_rootfs_cow("alpine:3.18", &templates, &vm)
        .await
        .unwrap();
    assert_eq!(std::fs::read(vm.join("etc/valid")).unwrap(), b"ok");
}

#[cfg(unix)]
#[tokio::test]
async fn cache_marker_never_writes_through_image_links() {
    use std::os::unix::fs::MetadataExt;
    for kind in ["symlink", "dangling", "hardlink", "directory"] {
        let outside = TempDir::new().unwrap();
        let victim = outside.path().join("victim");
        if kind != "dangling" {
            std::fs::write(&victim, b"host data").unwrap();
        }
        let mut gz = GzEncoder::new(Vec::new(), Compression::fast());
        {
            let mut tar = Builder::new(&mut gz);
            let mut header = Header::new_gnu();
            header.set_size(10);
            header.set_mode(0o644);
            header.set_cksum();
            tar.append_data(&mut header, "payload", &b"image data"[..])
                .unwrap();
            let mut header = Header::new_gnu();
            header.set_size(0);
            header.set_mode(0o755);
            header.set_entry_type(match kind {
                "hardlink" => tar::EntryType::Link,
                "directory" => tar::EntryType::Directory,
                _ => tar::EntryType::Symlink,
            });
            if kind == "hardlink" {
                header.set_link_name("payload").unwrap();
            } else if kind != "directory" {
                header.set_link_name(&victim).unwrap();
            }
            header.set_cksum();
            tar.append_data(&mut header, ".complete", std::io::empty())
                .unwrap();
            tar.finish().unwrap();
        }
        let mocked =
            setup_mocked_registry_layers("library/alpine", "3.18", &[gz.finish().unwrap()]).await;
        let state = TempDir::new().unwrap();
        let cache = state.path().join("cache");
        let extractor = preparation_extractor(cache.clone(), &mocked);
        for attempt in 0..2 {
            let target = state.path().join(format!("rootfs-{attempt}"));
            extractor
                .materialize_rootfs("alpine:3.18", &target)
                .await
                .unwrap();
            if kind == "dangling" {
                assert!(!victim.exists(), "must not create an outside marker target");
            } else {
                assert_eq!(std::fs::read(&victim).unwrap(), b"host data", "{kind}");
            }
            assert_eq!(
                std::fs::read(target.join("payload")).unwrap(),
                b"image data",
                "{kind}"
            );
            assert!(!target.join(".complete").exists());
            let marker = LayerCache::new(cache.clone())
                .layer_dir(&mocked.layer_digest)
                .join(".complete");
            let meta = std::fs::symlink_metadata(marker).unwrap();
            assert!(meta.is_file());
            assert_eq!(meta.nlink(), 1);
        }
        assert_eq!(
            count_blob_requests(&mocked, "library/alpine", &mocked.layer_digest).await,
            1
        );
    }
}

#[cfg(unix)]
#[tokio::test]
async fn cache_marker_recovers_old_link_markers_under_the_layer_lock() {
    use std::os::unix::fs::{symlink, MetadataExt};
    for kind in ["symlink", "hardlink"] {
        let mocked =
            setup_mocked_registry("library/alpine", "3.18", &[("payload", b"fresh", 0o644)]).await;
        let state = TempDir::new().unwrap();
        let victim = state.path().join("host-file");
        std::fs::write(&victim, b"host data").unwrap();
        let cache = state.path().join("cache");
        let cached_layer = LayerCache::new(cache.clone()).layer_dir(&mocked.layer_digest);
        std::fs::create_dir_all(&cached_layer).unwrap();
        std::fs::write(cached_layer.join("payload"), b"stale").unwrap();
        let marker = cached_layer.join(".complete");
        if kind == "symlink" {
            symlink(&victim, &marker).unwrap();
        } else {
            std::fs::hard_link(cached_layer.join("payload"), &marker).unwrap();
        }
        let extractor = preparation_extractor(cache, &mocked);
        let first = state.path().join("first");
        let second = state.path().join("second");
        let (a, b) = tokio::join!(
            extractor.materialize_rootfs("alpine:3.18", &first),
            extractor.materialize_rootfs("alpine:3.18", &second),
        );
        a.unwrap();
        b.unwrap();
        for target in [first, second] {
            assert_eq!(std::fs::read(target.join("payload")).unwrap(), b"fresh");
        }
        assert_eq!(std::fs::read(&victim).unwrap(), b"host data");
        let meta = std::fs::symlink_metadata(&marker).unwrap();
        assert!(meta.is_file());
        assert_eq!(meta.nlink(), 1);
        assert_eq!(
            count_blob_requests(&mocked, "library/alpine", &mocked.layer_digest).await,
            1
        );
    }
}

#[tokio::test]
async fn pre_containment_template_is_rebuilt_before_boot() {
    let mocked =
        setup_mocked_registry("library/alpine", "3.18", &[("etc/valid", b"fresh", 0o644)]).await;
    let state = TempDir::new().unwrap();
    let templates = state.path().join("templates");
    // Seed the precise previous format, including its completion sentinel.
    let mut identity = Sha256::new();
    identity.update(b"g5d-rootfs-template-v1\0");
    identity.update(if super::template::daemon_is_root() {
        "uid=root"
    } else {
        "uid=nonroot"
    });
    identity.update([0]);
    identity.update(&mocked.config_digest);
    identity.update([0]);
    identity.update(&mocked.layer_digest);
    let old_template = super::template::template_dir(&templates, &hex::encode(identity.finalize()));
    std::fs::create_dir_all(old_template.join("rootfs/etc")).unwrap();
    std::fs::write(old_template.join("rootfs/etc/valid"), b"stale").unwrap();
    super::template::write_image_config(&old_template, &super::ImageConfig::default()).unwrap();
    std::fs::write(old_template.join(".complete"), b"ok").unwrap();
    let extractor = preparation_extractor(state.path().join("cache"), &mocked);
    let vm = state.path().join("vm");
    extractor
        .materialize_rootfs_cow("alpine:3.18", &templates, &vm)
        .await
        .unwrap();
    assert_eq!(std::fs::read(vm.join("etc/valid")).unwrap(), b"fresh");
    assert_eq!(
        std::fs::read(old_template.join("rootfs/etc/valid")).unwrap(),
        b"stale"
    );
}

/// A gzipped layer whose entries may be directories (`path/` with
/// `EntryType::Directory`) or files, each with an explicit mode: the shape
/// BuildKit records for a `chmod -R` layer (directory entries with the new
/// mode, re-emitted files).
fn build_gzipped_tar_with_dirs(entries: &[(&str, Option<&[u8]>, u32)]) -> Vec<u8> {
    let mut gz = GzEncoder::new(Vec::new(), Compression::fast());
    {
        let mut tar = Builder::new(&mut gz);
        for (path, data, mode) in entries {
            let mut header = Header::new_gnu();
            header.set_mode(*mode);
            match data {
                Some(data) => {
                    header.set_size(data.len() as u64);
                    header.set_cksum();
                    tar.append_data(&mut header, *path, &mut std::io::Cursor::new(*data))
                        .unwrap();
                }
                None => {
                    header.set_size(0);
                    header.set_entry_type(tar::EntryType::Directory);
                    header.set_cksum();
                    tar.append_data(&mut header, *path, std::io::empty())
                        .unwrap();
                }
            }
        }
        tar.finish().unwrap();
    }
    gz.finish().unwrap()
}

/// `g5.runtime.boot.image.rootfs`: a second layer that only changes the mode
/// of directories the first layer created (the g5dev image's
/// `chmod -R a+rwX` over its baked cargo registry) reaches both the direct
/// and the template-assembled rootfs, so a non-root guest can write where
/// the image says it can.
#[cfg(unix)]
#[tokio::test]
async fn later_layer_directory_mode_reaches_direct_and_template_rootfs() {
    use std::os::unix::fs::PermissionsExt;
    let lower = build_gzipped_tar_with_dirs(&[
        ("usr/", None, 0o755),
        ("usr/local/", None, 0o755),
        ("usr/local/cargo/", None, 0o755),
        ("usr/local/cargo/registry/", None, 0o755),
        ("usr/local/cargo/registry/cache/", None, 0o755),
        (
            "usr/local/cargo/registry/cache/dep.crate",
            Some(b"baked"),
            0o644,
        ),
        ("etc/", None, 0o755),
        ("etc/alpine-release", Some(b"3.18.0\n"), 0o644),
        ("tmp/", None, 0o1777),
    ]);
    // `tmp/marker` has no `tmp/` header: the extraction synthesizes the
    // parent, which must not overwrite the lower layer's 1777.
    let upper = build_gzipped_tar_with_dirs(&[
        ("tmp/marker", Some(b"added"), 0o644),
        ("usr/", None, 0o755),
        ("usr/local/", None, 0o755),
        ("usr/local/cargo/", None, 0o1777),
        ("usr/local/cargo/registry/", None, 0o777),
        ("usr/local/cargo/registry/cache/", None, 0o777),
        (
            "usr/local/cargo/registry/cache/dep.crate",
            Some(b"baked"),
            0o666,
        ),
    ]);
    let mocked = setup_mocked_registry_layers("library/alpine", "3.18", &[lower, upper]).await;
    let state = TempDir::new().unwrap();
    let extractor = preparation_extractor(state.path().join("cache"), &mocked);
    let mode = |root: &std::path::Path, rel: &str| {
        std::fs::symlink_metadata(root.join(rel))
            .unwrap()
            .permissions()
            .mode()
            & 0o7777
    };
    let assert_image_modes = |root: &std::path::Path| {
        assert_eq!(
            mode(root, "usr/local/cargo/registry/cache"),
            0o777,
            "{}",
            root.display()
        );
        assert_eq!(
            mode(root, "usr/local/cargo/registry"),
            0o777,
            "{}",
            root.display()
        );
        assert_eq!(mode(root, "usr/local/cargo"), 0o1777, "{}", root.display());
        assert_eq!(mode(root, "usr/local"), 0o755, "{}", root.display());
        assert_eq!(
            mode(root, "usr/local/cargo/registry/cache/dep.crate"),
            0o666
        );
        assert_eq!(
            std::fs::read(root.join("usr/local/cargo/registry/cache/dep.crate")).unwrap(),
            b"baked"
        );
        assert_eq!(
            mode(root, "tmp"),
            0o1777,
            "a synthesized parent keeps the lower layer's mode: {}",
            root.display()
        );
        assert_eq!(std::fs::read(root.join("tmp/marker")).unwrap(), b"added");
        assert!(!root.join(".explicit-dirs").exists());
    };

    let direct = state.path().join("direct");
    extractor
        .materialize_rootfs("alpine:3.18", &direct)
        .await
        .unwrap();
    assert_image_modes(&direct);

    let templates = state.path().join("templates");
    let vm = state.path().join("vm");
    extractor
        .materialize_rootfs_cow("alpine:3.18", &templates, &vm)
        .await
        .unwrap();
    assert_image_modes(&vm);
    assert_is_clean_rootfs_clone(&vm);
}

/// `g5.runtime.boot.image.rootfs`: a template assembled under format v2, the
/// one deployed hosts hold, dropped later layers' directory modes; the v3 key
/// never matches it, so the next start rebuilds from the cached layers and
/// the stale template is left alone.
#[tokio::test]
async fn template_assembled_before_directory_mode_fidelity_is_rebuilt() {
    let mocked =
        setup_mocked_registry("library/alpine", "3.18", &[("etc/valid", b"fresh", 0o644)]).await;
    let state = TempDir::new().unwrap();
    let templates = state.path().join("templates");
    let mut identity = Sha256::new();
    identity.update(b"g5d-rootfs-template-v2\0");
    identity.update(if super::template::daemon_is_root() {
        "uid=root"
    } else {
        "uid=nonroot"
    });
    identity.update([0]);
    identity.update(&mocked.config_digest);
    identity.update([0]);
    identity.update(&mocked.layer_digest);
    let old_template = super::template::template_dir(&templates, &hex::encode(identity.finalize()));
    std::fs::create_dir_all(old_template.join("rootfs/etc")).unwrap();
    std::fs::write(old_template.join("rootfs/etc/valid"), b"stale").unwrap();
    super::template::write_image_config(&old_template, &super::ImageConfig::default()).unwrap();
    std::fs::write(old_template.join(".complete"), b"ok").unwrap();
    let extractor = preparation_extractor(state.path().join("cache"), &mocked);
    let vm = state.path().join("vm");
    extractor
        .materialize_rootfs_cow("alpine:3.18", &templates, &vm)
        .await
        .unwrap();
    assert_eq!(std::fs::read(vm.join("etc/valid")).unwrap(), b"fresh");
    assert_eq!(
        std::fs::read(old_template.join("rootfs/etc/valid")).unwrap(),
        b"stale"
    );
}

#[tokio::test]
async fn host_preparation_builds_template_without_vm_and_boot_reuses_it() {
    let mocked = setup_mocked_registry(
        "library/alpine",
        "3.18",
        &[("etc/alpine-release", b"3.18.0\n", 0o644)],
    )
    .await;
    let cache = TempDir::new().unwrap();
    let state = TempDir::new().unwrap();
    let templates = state.path().join("templates");
    let extractor = preparation_extractor(cache.path().to_path_buf(), &mocked);
    let prepared = extractor
        .prepare_rootfs_template("alpine:3.18", &templates)
        .await
        .unwrap();
    assert_eq!(
        std::fs::read_dir(state.path()).unwrap().count(),
        1,
        "preparation must not allocate a VM clone"
    );

    // A restarted host reuses the on-disk template, not in-memory state.
    let restarted = preparation_extractor(cache.path().to_path_buf(), &mocked);
    restarted
        .prepare_rootfs_template("alpine:3.18", &templates)
        .await
        .unwrap();
    let vm1 = state.path().join("vm-1");
    let pinned_image = format!("alpine@{}", mocked.manifest_digest);
    let first = restarted
        .materialize_rootfs_cow(&pinned_image, &templates, &vm1)
        .await
        .unwrap();
    assert_eq!(first.template_dir.as_ref(), Some(&prepared));
    assert!(first
        .image_config
        .env
        .iter()
        .any(|entry| entry == "JAVA_HOME=/opt/java/openjdk"));
    assert_is_clean_rootfs_clone(&vm1);
    std::fs::write(vm1.join("etc/alpine-release"), "private VM edit").unwrap();
    let vm2 = state.path().join("vm-2");
    restarted
        .materialize_rootfs_cow("alpine:3.18", &templates, &vm2)
        .await
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(vm2.join("etc/alpine-release")).unwrap(),
        "3.18.0\n"
    );
    assert_eq!(
        count_blob_requests(&mocked, "library/alpine", &mocked.layer_digest).await,
        1
    );
    assert_eq!(
        count_blob_requests(&mocked, "library/alpine", &mocked.config_digest).await,
        1
    );
}

#[tokio::test]
async fn cancelled_host_preparation_cleans_partial_download_and_can_retry() {
    let mocked = setup_mocked_registry(
        "library/alpine",
        "3.18",
        &[("etc/alpine-release", b"3.18.0\n", 0o644)],
    )
    .await;
    let cache = TempDir::new().unwrap();
    let state = TempDir::new().unwrap();
    let templates = state.path().join("templates");
    let requested = Arc::new(tokio::sync::Notify::new());
    let extractor = Arc::new(OciExtractor::with_components(
        LayerCache::new(cache.path().to_path_buf()),
        Box::new(LocalRegistry {
            host: mocked.host.clone(),
            client: DockerHubClient::new_for_tests(reqwest::Client::new()),
            pause_first_layer: Some(requested.clone()),
            layer_started: std::sync::atomic::AtomicBool::new(false),
        }),
    ));
    let preparing = tokio::spawn({
        let extractor = extractor.clone();
        let templates = templates.clone();
        async move {
            extractor
                .prepare_rootfs_template("alpine:3.18", &templates)
                .await
        }
    });
    tokio::time::timeout(std::time::Duration::from_secs(5), requested.notified())
        .await
        .unwrap();
    assert!(
        std::fs::read_dir(cache.path().join("sha256"))
            .unwrap()
            .count()
            > 0
    );
    preparing.abort();
    assert!(preparing.await.unwrap_err().is_cancelled());
    for dir in [templates.join("sha256"), cache.path().join("sha256")] {
        assert_eq!(
            std::fs::read_dir(&dir).unwrap().count(),
            0,
            "partial data and locks remain in {}",
            dir.display()
        );
    }
    // Reuse the same cache object: a leaked in-process owner or disk lock
    // would hang this retry instead of completing within ten seconds.
    tokio::time::timeout(
        std::time::Duration::from_secs(10),
        extractor.prepare_rootfs_template("alpine:3.18", &templates),
    )
    .await
    .unwrap()
    .unwrap();
    let vm = state.path().join("vm");
    extractor
        .materialize_rootfs_cow("alpine:3.18", &templates, &vm)
        .await
        .unwrap();
    assert_is_clean_rootfs_clone(&vm);
}

#[tokio::test]
async fn host_preparation_and_vm_start_share_the_template_build_lock() {
    let mocked = setup_mocked_registry(
        "library/alpine",
        "3.18",
        &[("etc/alpine-release", b"3.18.0\n", 0o644)],
    )
    .await;
    let cache = TempDir::new().unwrap();
    let state = TempDir::new().unwrap();
    let templates = state.path().join("templates");
    let extractor = preparation_extractor(cache.path().to_path_buf(), &mocked);
    // Warm authentication so this test isolates template locking.
    extractor
        .client_for_test()
        .fetch_manifest(&super::parse_image_ref("alpine:3.18").unwrap())
        .await
        .unwrap();
    let vm = state.path().join("vm");
    let (prepared, booted) = tokio::join!(
        extractor.prepare_rootfs_template("alpine:3.18", &templates),
        extractor.materialize_rootfs_cow("alpine:3.18", &templates, &vm),
    );
    prepared.unwrap();
    booted.unwrap();
    assert_is_clean_rootfs_clone(&vm);
    assert_eq!(
        count_blob_requests(&mocked, "library/alpine", &mocked.config_digest).await,
        1
    );
    assert_eq!(
        count_blob_requests(&mocked, "library/alpine", &mocked.layer_digest).await,
        1
    );
}

#[tokio::test]
async fn failed_host_preparation_can_retry_without_publishing_partial_template() {
    let mocked = setup_mocked_registry(
        "library/alpine",
        "3.18",
        &[("etc/alpine-release", b"3.18.0\n", 0o644)],
    )
    .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/v2/library/alpine/blobs/{}",
            mocked.layer_digest
        )))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"corrupt layer"))
        .with_priority(1)
        .up_to_n_times(1)
        .mount(&mocked.server)
        .await;
    let cache = TempDir::new().unwrap();
    let state = TempDir::new().unwrap();
    let templates = state.path().join("templates");
    let extractor = preparation_extractor(cache.path().to_path_buf(), &mocked);
    assert!(extractor
        .prepare_rootfs_template("alpine:3.18", &templates)
        .await
        .is_err());
    assert_eq!(
        std::fs::read_dir(templates.join("sha256")).unwrap().count(),
        0,
        "failed preparation must remove its unpublished template and lock"
    );
    extractor
        .prepare_rootfs_template("alpine:3.18", &templates)
        .await
        .unwrap();
    let vm = state.path().join("vm");
    extractor
        .materialize_rootfs_cow("alpine:3.18", &templates, &vm)
        .await
        .unwrap();
    assert_is_clean_rootfs_clone(&vm);
    assert_eq!(
        count_blob_requests(&mocked, "library/alpine", &mocked.layer_digest).await,
        2
    );
}

#[tokio::test]
async fn google_mirror_429_falls_back_once_to_canonical_dockerhub() {
    let mirror = MockServer::start().await;
    let canonical = MockServer::start().await;
    let mirror_host = mirror
        .uri()
        .strip_prefix("http://")
        .expect("mock mirror URI uses http")
        .to_string();
    let canonical_host = canonical
        .uri()
        .strip_prefix("http://")
        .expect("mock canonical URI uses http")
        .to_string();
    let manifest_path = "/v2/library/buildpack-deps/manifests/noble";

    Mock::given(method("GET"))
        .and(path(manifest_path))
        .respond_with(ResponseTemplate::new(429).set_body_json(serde_json::json!({
            "errors": [{"code": "TOOMANYREQUESTS", "message": "cache unavailable"}]
        })))
        .mount(&mirror)
        .await;

    Mock::given(method("GET"))
        .and(path(manifest_path))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/vnd.oci.image.manifest.v1+json")
                .set_body_json(serde_json::json!({
                    "schemaVersion": 2,
                    "mediaType": "application/vnd.oci.image.manifest.v1+json",
                    "config": {
                        "mediaType": "application/vnd.oci.image.config.v1+json",
                        "digest": format!("sha256:{}", "0".repeat(64)),
                        "size": 2
                    },
                    "layers": []
                })),
        )
        .mount(&canonical)
        .await;

    let http = reqwest::Client::builder()
        .user_agent("g5d-oci-mirror-test/0.1.0")
        .build()
        .unwrap();
    let client =
        DockerHubClient::new_for_tests_with_mirror(http, mirror_host.clone(), canonical_host);
    let image = ImageRef {
        registry: mirror_host,
        repository: "library/buildpack-deps".to_string(),
        reference: Reference::Tag("noble".to_string()),
    };

    let manifest = client
        .fetch_manifest(&image)
        .await
        .expect("mirror availability failure must fall back to canonical Docker Hub");
    assert!(manifest.layers.is_empty());
    assert_eq!(mirror.received_requests().await.unwrap().len(), 1);
    assert_eq!(canonical.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn full_pull_flow_writes_layer_contents() {
    let mocked = setup_mocked_registry(
        "library/alpine",
        "3.18",
        &[("etc/alpine-release", b"3.18.0\n", 0o644)],
    )
    .await;

    let cache_dir = TempDir::new().unwrap();
    let target_dir = TempDir::new().unwrap();

    let extractor = test_extractor(cache_dir.path().to_path_buf());
    let image = ImageRef {
        registry: mocked.host.clone(),
        repository: "library/alpine".to_string(),
        reference: Reference::Tag("3.18".to_string()),
    };

    let manifest = extractor
        .client_for_test()
        .fetch_manifest(&image)
        .await
        .expect("manifest fetched");
    assert_eq!(manifest.layers.len(), 1);
    assert_eq!(manifest.layers[0].digest, mocked.layer_digest);
    assert_eq!(manifest.config.digest, mocked.config_digest);

    let image_config = extractor
        .client_for_test()
        .fetch_config(&image, &manifest.config)
        .await
        .expect("image config fetched");
    assert_eq!(
        image_config.env,
        vec![
            "JAVA_HOME=/opt/java/openjdk",
            "MAVEN_HOME=/usr/share/maven",
            "PATH=/usr/share/maven/bin:/opt/java/openjdk/bin:/usr/bin",
        ]
    );

    let layer_dir = extractor
        .cache_for_test()
        .ensure_layer(extractor.client_for_test(), &image, &manifest.layers[0])
        .await
        .expect("layer extracted into cache");

    // The cache layer dir must contain a `.complete` marker (defense
    // in depth against an empty/partially-populated dir being mistaken
    // for a cache hit).
    assert!(
        layer_dir.join(".complete").is_file(),
        "expected .complete marker in cached layer at {:?}",
        layer_dir
    );

    crate::oci::extract::apply_layer(&layer_dir, target_dir.path()).expect("apply layer");

    let release = std::fs::read_to_string(target_dir.path().join("etc/alpine-release")).unwrap();
    assert_eq!(release, "3.18.0\n");
    assert!(!mocked.layer_bytes.is_empty());
    // The `.complete` marker must not leak into the assembled rootfs.
    assert!(!target_dir.path().join(".complete").exists());
}

#[tokio::test]
async fn cache_hit_skips_second_blob_fetch() {
    let mocked = setup_mocked_registry(
        "library/alpine",
        "3.18",
        &[("etc/alpine-release", b"3.18.0\n", 0o644)],
    )
    .await;

    let cache_dir = TempDir::new().unwrap();

    let extractor = test_extractor(cache_dir.path().to_path_buf());
    let image = ImageRef {
        registry: mocked.host.clone(),
        repository: "library/alpine".to_string(),
        reference: Reference::Tag("3.18".to_string()),
    };
    let manifest = extractor
        .client_for_test()
        .fetch_manifest(&image)
        .await
        .unwrap();

    let first = extractor
        .cache_for_test()
        .ensure_layer(extractor.client_for_test(), &image, &manifest.layers[0])
        .await
        .unwrap();
    let second = extractor
        .cache_for_test()
        .ensure_layer(extractor.client_for_test(), &image, &manifest.layers[0])
        .await
        .unwrap();
    assert_eq!(first, second);

    let blob_requests = mocked
        .server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|r| {
            r.url
                .path()
                .starts_with(&format!("/v2/library/alpine/blobs/{}", mocked.layer_digest))
        })
        .count();
    assert_eq!(blob_requests, 1, "blob must be fetched exactly once");
}

#[tokio::test]
async fn digest_mismatch_on_layer_body_errors_cleanly() {
    let server = MockServer::start().await;
    let host = server.uri().strip_prefix("http://").unwrap().to_string();

    let real_bytes = build_gzipped_tar(&[("etc/passwd", b"root:x:0:0::/root:/bin/sh", 0o644)]);
    let lying_digest = "sha256:0000000000000000000000000000000000000000000000000000000000000000";

    let manifest_json = serde_json::json!({
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.manifest.v1+json",
        "config": {
            "mediaType": "application/vnd.oci.image.config.v1+json",
            "digest": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
            "size": 2,
        },
        "layers": [
            {
                "mediaType": "application/vnd.oci.image.layer.v1.tar+gzip",
                "digest": lying_digest,
                "size": real_bytes.len(),
            }
        ]
    });

    Mock::given(method("GET"))
        .and(path("/v2/library/x/manifests/1"))
        .respond_with(ResponseTemplate::new(401).insert_header(
            "Www-Authenticate",
            format!(
                r#"Bearer realm="{}/token",service="registry.docker.io",scope="repository:library/x:pull""#,
                server.uri()
            )
            .as_str(),
        ))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "token": "tok",
            "expires_in": 300,
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v2/library/x/manifests/1"))
        .and(header("Authorization", "Bearer tok"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/vnd.oci.image.manifest.v1+json")
                .set_body_json(manifest_json),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/v2/library/x/blobs/{}", lying_digest)))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header(
                    "Content-Type",
                    "application/vnd.oci.image.layer.v1.tar+gzip",
                )
                .set_body_bytes(real_bytes.clone()),
        )
        .mount(&server)
        .await;

    let cache_dir = TempDir::new().unwrap();
    let extractor = test_extractor(cache_dir.path().to_path_buf());
    let image = ImageRef {
        registry: host.clone(),
        repository: "library/x".to_string(),
        reference: Reference::Tag("1".to_string()),
    };
    let manifest = extractor
        .client_for_test()
        .fetch_manifest(&image)
        .await
        .unwrap();
    let err = extractor
        .cache_for_test()
        .ensure_layer(extractor.client_for_test(), &image, &manifest.layers[0])
        .await
        .expect_err("digest mismatch must error");
    let msg = format!("{}", err);
    assert!(
        msg.contains("digest mismatch") || msg.contains("DigestMismatch"),
        "msg was: {}",
        msg
    );
    let cache_layer_dir = extractor.cache_for_test().layer_dir(lying_digest);
    assert!(
        !cache_layer_dir.join(".complete").is_file(),
        "polluted cache at {:?}",
        cache_layer_dir
    );
}

#[tokio::test]
async fn digest_mismatch_on_config_body_errors_cleanly() {
    let server = MockServer::start().await;
    let host = server.uri().strip_prefix("http://").unwrap().to_string();

    let config_bytes = serde_json::to_vec(&serde_json::json!({
        "config": {
            "Env": ["PATH=/usr/share/maven/bin:/usr/bin"]
        }
    }))
    .unwrap();
    let lying_digest = "sha256:0000000000000000000000000000000000000000000000000000000000000000";

    let manifest_json = serde_json::json!({
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.manifest.v1+json",
        "config": {
            "mediaType": "application/vnd.oci.image.config.v1+json",
            "digest": lying_digest,
            "size": config_bytes.len(),
        },
        "layers": []
    });

    Mock::given(method("GET"))
        .and(path("/v2/library/x/manifests/1"))
        .respond_with(ResponseTemplate::new(401).insert_header(
            "Www-Authenticate",
            format!(
                r#"Bearer realm="{}/token",service="registry.docker.io",scope="repository:library/x:pull""#,
                server.uri()
            )
            .as_str(),
        ))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "token": "tok",
            "expires_in": 300,
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v2/library/x/manifests/1"))
        .and(header("Authorization", "Bearer tok"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/vnd.oci.image.manifest.v1+json")
                .set_body_json(manifest_json),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/v2/library/x/blobs/{}", lying_digest)))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/vnd.oci.image.config.v1+json")
                .set_body_bytes(config_bytes.clone()),
        )
        .mount(&server)
        .await;

    let cache_dir = TempDir::new().unwrap();
    let extractor = test_extractor(cache_dir.path().to_path_buf());
    let image = ImageRef {
        registry: host,
        repository: "library/x".to_string(),
        reference: Reference::Tag("1".to_string()),
    };
    let manifest = extractor
        .client_for_test()
        .fetch_manifest(&image)
        .await
        .unwrap();
    let err = extractor
        .client_for_test()
        .fetch_config(&image, &manifest.config)
        .await
        .expect_err("config digest mismatch must error");
    let msg = format!("{}", err);
    assert!(
        msg.contains("digest mismatch") || msg.contains("DigestMismatch"),
        "msg was: {}",
        msg
    );
}

/// Concurrent same-digest pulls must coalesce: the blob endpoint is hit
/// exactly once even when N tokio tasks call `ensure_layer` simultaneously.
#[tokio::test]
async fn concurrent_pulls_dedup_to_single_fetch() {
    let mocked = setup_mocked_registry(
        "library/alpine",
        "3.18",
        &[("etc/alpine-release", b"3.18.0\n", 0o644)],
    )
    .await;

    let cache_dir = TempDir::new().unwrap();
    let extractor = Arc::new(test_extractor(cache_dir.path().to_path_buf()));
    let image = ImageRef {
        registry: mocked.host.clone(),
        repository: "library/alpine".to_string(),
        reference: Reference::Tag("3.18".to_string()),
    };
    let manifest = extractor
        .client_for_test()
        .fetch_manifest(&image)
        .await
        .unwrap();
    let descriptor = manifest.layers[0].clone();

    let parallelism = 8;
    let mut handles = Vec::with_capacity(parallelism);
    for _ in 0..parallelism {
        let extractor = extractor.clone();
        let image = image.clone();
        let descriptor = descriptor.clone();
        handles.push(tokio::spawn(async move {
            extractor
                .cache_for_test()
                .ensure_layer(extractor.client_for_test(), &image, &descriptor)
                .await
        }));
    }

    let mut results = Vec::with_capacity(parallelism);
    for h in handles {
        results.push(h.await.expect("join").expect("ensure_layer success"));
    }
    // All callers must converge on the same final layer dir.
    let first = results[0].clone();
    assert!(first.join(".complete").is_file(), "cache must be complete");
    for r in &results {
        assert_eq!(r, &first);
    }

    let blob_requests = mocked
        .server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|r| {
            r.url
                .path()
                .starts_with(&format!("/v2/library/alpine/blobs/{}", mocked.layer_digest))
        })
        .count();
    assert_eq!(
        blob_requests, 1,
        "concurrent same-digest pulls must coalesce to exactly one network fetch"
    );
}

// ---------------------------------------------------------------------------
// Assembled-rootfs template cache (ontology g5/g5d/oci D29/D30)
// ---------------------------------------------------------------------------

async fn count_blob_requests(mocked: &MockedRegistry, repository: &str, digest: &str) -> usize {
    mocked
        .server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|r| {
            r.url
                .path()
                .starts_with(&format!("/v2/{}/blobs/{}", repository, digest))
        })
        .count()
}

fn assert_is_clean_rootfs_clone(target: &std::path::Path) {
    let release = std::fs::read_to_string(target.join("etc/alpine-release")).unwrap();
    assert_eq!(release, "3.18.0\n");
    // Template-internal markers must never leak into a guest rootfs (D29
    // layout: markers are siblings of rootfs/, not inside it).
    assert!(!target.join(".complete").exists());
    assert!(!target.join(".explicit-dirs").exists());
    assert!(!target.join("image-config.json").exists());
    assert!(!target.join("rootfs").exists());
}

/// D30: a second boot of the same image reuses the assembled template — the
/// layer blob AND the config blob are each fetched exactly once, both clones
/// carry the full content + image config, and no template markers leak.
#[tokio::test]
async fn template_cow_second_materialization_reuses_template() {
    let mocked = setup_mocked_registry(
        "library/alpine",
        "3.18",
        &[("etc/alpine-release", b"3.18.0\n", 0o644)],
    )
    .await;

    let cache_dir = TempDir::new().unwrap();
    let state_dir = TempDir::new().unwrap();
    let template_root = state_dir.path().join("templates");

    let extractor = test_extractor(cache_dir.path().to_path_buf());
    let image = ImageRef {
        registry: mocked.host.clone(),
        repository: "library/alpine".to_string(),
        reference: Reference::Tag("3.18".to_string()),
    };

    let expected_env = vec![
        "JAVA_HOME=/opt/java/openjdk".to_string(),
        "MAVEN_HOME=/usr/share/maven".to_string(),
        "PATH=/usr/share/maven/bin:/opt/java/openjdk/bin:/usr/bin".to_string(),
    ];

    let vm1 = state_dir.path().join("vm-1");
    let first = extractor
        .materialize_rootfs_cow_for(&image, "library/alpine:3.18", &template_root, &vm1)
        .await
        .expect("cold materialization builds template + clones");
    assert_eq!(first.rootfs, vm1);
    assert_eq!(first.image_config.env, expected_env);
    assert_is_clean_rootfs_clone(&vm1);

    let vm2 = state_dir.path().join("vm-2");
    let second = extractor
        .materialize_rootfs_cow_for(&image, "library/alpine:3.18", &template_root, &vm2)
        .await
        .expect("warm materialization clones from template");
    assert_eq!(second.image_config.env, expected_env);
    assert_is_clean_rootfs_clone(&vm2);

    // The clones are independent trees: mutating one must not affect the
    // other (per-VM rootfs isolation — vm-exec writes per-VM state into it).
    std::fs::write(vm1.join("etc/alpine-release"), b"mutated\n").unwrap();
    assert_eq!(
        std::fs::read_to_string(vm2.join("etc/alpine-release")).unwrap(),
        "3.18.0\n"
    );

    assert_eq!(
        count_blob_requests(&mocked, "library/alpine", &mocked.layer_digest).await,
        1,
        "layer blob must be fetched exactly once across two boots"
    );
    assert_eq!(
        count_blob_requests(&mocked, "library/alpine", &mocked.config_digest).await,
        1,
        "config blob must be fetched exactly once (warm boot reads image-config.json)"
    );
}

/// D30: N concurrent cold materializations of the same image build the
/// template exactly once and every caller gets a complete, independent clone.
#[tokio::test]
async fn template_cow_concurrent_materializations_build_once() {
    let mocked = setup_mocked_registry(
        "library/alpine",
        "3.18",
        &[("etc/alpine-release", b"3.18.0\n", 0o644)],
    )
    .await;

    let cache_dir = TempDir::new().unwrap();
    let state_dir = TempDir::new().unwrap();
    let template_root = state_dir.path().join("templates");

    let extractor = Arc::new(test_extractor(cache_dir.path().to_path_buf()));
    let image = ImageRef {
        registry: mocked.host.clone(),
        repository: "library/alpine".to_string(),
        reference: Reference::Tag("3.18".to_string()),
    };

    // Warm the bearer-token cache first: this test is about template-build
    // coalescing, not
    // concurrent cold-token minting (covered by the singleflight tests).
    extractor
        .client_for_test()
        .fetch_manifest(&image)
        .await
        .unwrap();

    let parallelism = 4;
    let mut handles = Vec::with_capacity(parallelism);
    for i in 0..parallelism {
        let extractor = extractor.clone();
        let image = image.clone();
        let template_root = template_root.clone();
        let target = state_dir.path().join(format!("vm-{}", i));
        handles.push(tokio::spawn(async move {
            extractor
                .materialize_rootfs_cow_for(&image, "library/alpine:3.18", &template_root, &target)
                .await
                .map(|m| m.rootfs)
        }));
    }

    for h in handles {
        let rootfs = h
            .await
            .expect("join")
            .expect("cow materialization succeeds");
        assert_is_clean_rootfs_clone(&rootfs);
    }

    assert_eq!(
        count_blob_requests(&mocked, "library/alpine", &mocked.layer_digest).await,
        1,
        "concurrent cold boots must assemble the template exactly once"
    );
    assert_eq!(
        count_blob_requests(&mocked, "library/alpine", &mocked.config_digest).await,
        1,
        "concurrent cold boots must fetch the config exactly once"
    );
}

/// D30: images with different layer content produce distinct template keys
/// and independent templates under the same template root.
#[tokio::test]
async fn template_cow_distinct_images_use_distinct_templates() {
    let mocked_a = setup_mocked_registry(
        "library/alpine",
        "3.18",
        &[("etc/alpine-release", b"3.18.0\n", 0o644)],
    )
    .await;
    let mocked_b = setup_mocked_registry(
        "library/alpine",
        "3.18",
        &[
            ("etc/alpine-release", b"3.18.0\n", 0o644),
            ("etc/motd", b"different image\n", 0o644),
        ],
    )
    .await;
    assert_ne!(mocked_a.layer_digest, mocked_b.layer_digest);

    let cache_dir = TempDir::new().unwrap();
    let state_dir = TempDir::new().unwrap();
    let template_root = state_dir.path().join("templates");
    let extractor = test_extractor(cache_dir.path().to_path_buf());

    let image_a = ImageRef {
        registry: mocked_a.host.clone(),
        repository: "library/alpine".to_string(),
        reference: Reference::Tag("3.18".to_string()),
    };
    let image_b = ImageRef {
        registry: mocked_b.host.clone(),
        repository: "library/alpine".to_string(),
        reference: Reference::Tag("3.18".to_string()),
    };

    let vm_a = state_dir.path().join("vm-a");
    let vm_b = state_dir.path().join("vm-b");
    let materialized_a = extractor
        .materialize_rootfs_cow_for(&image_a, "image-a", &template_root, &vm_a)
        .await
        .unwrap();
    let materialized_b = extractor
        .materialize_rootfs_cow_for(&image_b, "image-b", &template_root, &vm_b)
        .await
        .unwrap();

    assert_ne!(materialized_a.template_dir, materialized_b.template_dir);
    assert!(materialized_a.template_dir.is_some());
    assert!(materialized_b.template_dir.is_some());
    assert!(!vm_a.join("etc/motd").exists());
    assert_eq!(
        std::fs::read_to_string(vm_b.join("etc/motd")).unwrap(),
        "different image\n"
    );

    let templates: Vec<_> = std::fs::read_dir(template_root.join("sha256"))
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir() && !e.file_name().to_string_lossy().starts_with('.'))
        .collect();
    assert_eq!(
        templates.len(),
        2,
        "distinct image content must key distinct templates"
    );
}

/// D29: the clone step refuses a pre-existing target (the per-VM dir is
/// created by the clone; a collision means a caller bug).
#[tokio::test]
async fn template_cow_rejects_existing_target() {
    let mocked = setup_mocked_registry(
        "library/alpine",
        "3.18",
        &[("etc/alpine-release", b"3.18.0\n", 0o644)],
    )
    .await;

    let cache_dir = TempDir::new().unwrap();
    let state_dir = TempDir::new().unwrap();
    let template_root = state_dir.path().join("templates");
    let extractor = test_extractor(cache_dir.path().to_path_buf());
    let image = ImageRef {
        registry: mocked.host.clone(),
        repository: "library/alpine".to_string(),
        reference: Reference::Tag("3.18".to_string()),
    };

    let occupied = state_dir.path().join("vm-occupied");
    std::fs::create_dir_all(&occupied).unwrap();
    let err = extractor
        .materialize_rootfs_cow_for(&image, "library/alpine:3.18", &template_root, &occupied)
        .await
        .expect_err("existing target must be rejected");
    assert!(
        format!("{}", err).contains("already exists"),
        "err: {}",
        err
    );
}
