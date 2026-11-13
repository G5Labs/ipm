//! Delivery companion for a pristine workload template. Only host preparation
//! reads it; a VM start continues to use the existing locked reflink cache.
use super::{workload_root as root, workload_template as template};
use anyhow::{ensure, Context};
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use futures_util::StreamExt;
use g5d_foundation::oci::{parse_image_ref, upstream_client, Digest, Reference};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::Path,
};
use tokio::io::AsyncWriteExt;

const MIB: u64 = 1024 * 1024;
const MANIFEST_LIMIT: u64 = 16 * 1024;

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug)]
#[serde(deny_unknown_fields)]
struct Binding {
    version: u32,
    source_digest: String,
    template: template::Identity,
    raw_sha256: String,
}

fn source_digest(image: &str) -> anyhow::Result<String> {
    match parse_image_ref(image)?.reference {
        Reference::Digest(digest) => Ok(digest.as_str().into()),
        _ => anyhow::bail!("workload artifacts require an immutable source OCI digest"),
    }
}

/// The tag is a discovery key; registry manifest and layer digests bind the
/// downloaded bytes. The payload additionally binds source identity and size.
pub fn reference(image: &str, size_mib: u32) -> anyhow::Result<String> {
    let parsed = parse_image_ref(image)?;
    let digest = source_digest(image)?;
    Ok(format!(
        "{}/{}:workload-{}-{}-b2-s{}",
        parsed.registry,
        parsed.repository,
        &digest[7..],
        std::env::consts::ARCH,
        size_mib
    ))
}

fn output(path: &Path) -> anyhow::Result<File> {
    Ok(OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?)
}

/// Run by delivery after the ordinary preparation operation. No repository seed
/// or credentials enter this payload. Docker publishes these two regular files
/// as a single COPY layer in an otherwise empty OCI image.
pub fn export(image: &str, pristine: &Path, size_mib: u32, dest: &Path) -> anyhow::Result<()> {
    let (cache, identity) = template::location(pristine, size_mib)?;
    let _lock = template::lock(&cache, false)?;
    ensure!(
        template::complete(&cache, &identity),
        "matching pristine template is absent"
    );
    std::fs::create_dir(dest).context("artifact output must be a new directory")?;
    let mut source = File::open(cache.join("current/image.ext4"))?;
    let mut encoder = GzEncoder::new(output(&dest.join("image.ext4.gz"))?, Compression::fast());
    let mut hash = Sha256::new();
    let mut buffer = vec![0; MIB as usize];
    loop {
        let n = source.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
        encoder.write_all(&buffer[..n])?;
    }
    encoder.finish()?.sync_all()?;
    let binding = Binding {
        version: 1,
        source_digest: source_digest(image)?,
        template: identity,
        raw_sha256: format!("{:x}", hash.finalize()),
    };
    output(&dest.join("identity.json"))?.write_all(&serde_json::to_vec(&binding)?)?;
    output(&dest.join("reference.txt"))?.write_all(reference(image, size_mib)?.as_bytes())?;
    output(&dest.join("Dockerfile"))?
        .write_all(b"FROM scratch\nCOPY identity.json image.ext4.gz /\n")?;
    Ok(())
}

fn sparse_decode(reader: impl Read, dest: &Path, bytes: u64) -> anyhow::Result<String> {
    let mut decoder = GzDecoder::new(reader);
    let mut file = output(dest)?;
    let mut buffer = vec![0; 64 * 1024];
    let mut hash = Sha256::new();
    let mut total = 0u64;
    loop {
        let n = decoder.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        total = total
            .checked_add(n as u64)
            .context("artifact size overflow")?;
        ensure!(total <= bytes, "artifact expands beyond admitted capacity");
        hash.update(&buffer[..n]);
        if buffer[..n].iter().all(|b| *b == 0) {
            file.seek(SeekFrom::Current(n as i64))?;
        } else {
            file.write_all(&buffer[..n])?;
        }
    }
    ensure!(total == bytes, "artifact raw size mismatch");
    file.set_len(bytes)?;
    file.sync_all()?;
    Ok(format!("{:x}", hash.finalize()))
}

fn unpack(
    layer: &Path,
    stage: &Path,
    image: &str,
    identity: &template::Identity,
) -> anyhow::Result<()> {
    let capacity = u64::from(identity.size_mib) * MIB;
    // Limit the outer tar as well as the inner disk. Never unpack pathnames,
    // links, devices or arbitrary image metadata onto the host filesystem.
    let outer = GzDecoder::new(File::open(layer)?).take(capacity + 2 * MIB);
    let mut archive = tar::Archive::new(outer);
    let mut binding = None;
    let mut hash = None;
    for entry in archive.entries()? {
        let mut entry = entry?;
        ensure!(
            entry.header().entry_type().is_file(),
            "artifact entry must be a regular file"
        );
        let path = entry.path()?.into_owned();
        match path.to_str() {
            Some("identity.json") if binding.is_none() => {
                ensure!(
                    entry.size() <= MANIFEST_LIMIT,
                    "artifact identity is too large"
                );
                let mut bytes = Vec::new();
                entry.read_to_end(&mut bytes)?;
                binding = Some(serde_json::from_slice::<Binding>(&bytes)?);
            }
            Some("image.ext4.gz") if hash.is_none() => {
                ensure!(
                    entry.size() <= capacity + MIB,
                    "compressed artifact exceeds capacity bound"
                );
                hash = Some(sparse_decode(
                    &mut entry,
                    &stage.join("image.ext4"),
                    capacity,
                )?);
            }
            _ => anyhow::bail!("unexpected or duplicate workload artifact entry"),
        }
    }
    let expected = Binding {
        version: 1,
        source_digest: source_digest(image)?,
        template: identity.clone(),
        raw_sha256: hash.context("artifact disk is absent")?,
    };
    ensure!(
        binding.as_ref() == Some(&expected),
        "workload artifact identity or digest mismatch"
    );
    Ok(())
}

/// Missing/unsupported companions are a cache miss. Corrupt companions are
/// rejected and the caller may use the same trusted local preparation path.
/// This runs in the dedicated preparation process, never the async host daemon.
pub async fn import(
    image: &str,
    pristine: &Path,
    size_mib: u32,
    reserve_mib: u64,
) -> anyhow::Result<bool> {
    let companion = parse_image_ref(&reference(image, size_mib)?)?;
    let Some(client) = upstream_client(&companion.registry) else {
        return Ok(false);
    };
    import_from_client(image, pristine, size_mib, reserve_mib, &client, &companion).await
}

async fn import_from_client(
    image: &str,
    pristine: &Path,
    size_mib: u32,
    reserve_mib: u64,
    client: &g5d_foundation::oci::DistributionClient,
    companion: &g5d_foundation::oci::ImageRef,
) -> anyhow::Result<bool> {
    let (cache, identity) = template::location(pristine, size_mib)?;
    let _lock = template::lock(&cache, true)?;
    let _seed = template::lock_named(&cache, "seed-build.lock", true, false)?;
    if template::complete(&cache, &identity) {
        return Ok(true);
    }
    let raw = match client
        .fetch_manifest_raw(
            companion,
            &[
                "application/vnd.oci.image.manifest.v1+json",
                "application/vnd.docker.distribution.manifest.v2+json",
            ],
        )
        .await
    {
        Ok(raw) => raw,
        Err(g5d_foundation::oci::OciError::RegistryHttp { status: 404, .. }) => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    ensure!(
        raw.bytes.len() as u64 <= MANIFEST_LIMIT,
        "companion manifest too large"
    );
    let manifest: g5d_foundation::oci::registry::Manifest = serde_json::from_slice(&raw.bytes)?;
    ensure!(
        manifest.layers.len() == 1,
        "companion must have exactly one payload layer"
    );
    let layer = &manifest.layers[0];
    ensure!(
        matches!(
            layer.media_type.as_str(),
            "application/vnd.oci.image.layer.v1.tar+gzip"
                | "application/vnd.docker.image.rootfs.diff.tar.gzip"
        ),
        "unsupported companion compression"
    );
    let capacity = u64::from(size_mib) * MIB;
    ensure!(
        layer.size > 0 && layer.size <= capacity + 2 * MIB,
        "companion download exceeds capacity bound"
    );
    let digest = Digest::parse(&layer.digest)?;
    let _ = reserve_mib; // Compatibility flag: preparation reserves no slot capacity.
    template::remove_owned(&cache.join("building"))?;
    let stage = template::Staging(cache.join("building"));
    std::fs::create_dir(&stage.0)?;
    let compressed = stage.0.join("payload.gz");
    let mut file = tokio::fs::File::from_std(output(&compressed)?);
    let mut stream = client.open_blob(companion, &digest).await?;
    let mut hash = Sha256::new();
    let mut total = 0u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        total = total
            .checked_add(chunk.len() as u64)
            .context("download size overflow")?;
        ensure!(
            total <= layer.size,
            "companion stream exceeds its descriptor"
        );
        hash.update(&chunk);
        file.write_all(&chunk).await?;
    }
    file.flush().await?;
    drop(file);
    ensure!(
        total == layer.size && format!("sha256:{:x}", hash.finalize()) == layer.digest,
        "companion layer size or digest mismatch"
    );
    unpack(&compressed, &stage.0, image, &identity)?;
    std::fs::remove_file(compressed)?;
    let disk = stage.0.join("image.ext4");
    let check = root::helper("e2fsck")
        .args(["-f", "-n"])
        .arg(&disk)
        .output()?;
    ensure!(check.status.success(), "companion ext4 validation failed");
    let probe = stage.0.join("probe");
    ensure!(
        template::reflink(&disk, &probe)?,
        "host does not support private reflinks"
    );
    std::fs::remove_file(probe)?;
    std::fs::set_permissions(&disk, std::fs::Permissions::from_mode(0o400))?;
    let mut metadata = output(&stage.0.join("identity.json"))?;
    metadata.write_all(&serde_json::to_vec(&identity)?)?;
    metadata.sync_all()?;
    File::open(&stage.0)?.sync_all()?;
    template::remove_owned(&cache.join("current"))?;
    template::remove_owned(&cache.join("repo-current"))?;
    std::fs::rename(&stage.0, cache.join("current"))?;
    File::open(&cache)?.sync_all()?;
    tracing::info!(manifest_digest = %raw.digest.as_str(), compressed_bytes = total, size_mib,
        "imported verified deployment workload template");
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::MetadataExt;

    #[test]
    fn sparse_expansion_is_bounded_and_preserves_holes() {
        let dir = tempfile::tempdir().unwrap();
        let mut encoded = GzEncoder::new(Vec::new(), Compression::fast());
        encoded.write_all(&vec![0; 4 * MIB as usize]).unwrap();
        let bytes = encoded.finish().unwrap();
        let path = dir.path().join("disk");
        sparse_decode(&bytes[..], &path, 4 * MIB).unwrap();
        let stat = std::fs::metadata(path).unwrap();
        assert_eq!(stat.len(), 4 * MIB);
        assert!(stat.blocks() * 512 < MIB);
        assert!(sparse_decode(&bytes[..], &dir.path().join("oversize"), MIB).is_err());
        assert!(sparse_decode(&bytes[..], &dir.path().join("short"), 5 * MIB).is_err());
        assert!(sparse_decode(
            &bytes[..bytes.len() - 4],
            &dir.path().join("truncated"),
            4 * MIB
        )
        .is_err());
    }

    fn payload(binding: &Binding, body: &[u8]) -> Vec<u8> {
        let mut archive = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::fast()));
        for (name, bytes) in [
            ("identity.json", serde_json::to_vec(binding).unwrap()),
            ("image.ext4.gz", body.to_vec()),
        ] {
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o600);
            header.set_cksum();
            archive.append_data(&mut header, name, &bytes[..]).unwrap();
        }
        archive.into_inner().unwrap().finish().unwrap()
    }

    #[test]
    fn payload_binds_source_layout_capacity_and_raw_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let pristine = dir.path().join("sha256").join("a".repeat(64));
        std::fs::create_dir_all(pristine.join("rootfs")).unwrap();
        std::fs::write(pristine.join(".complete"), b"").unwrap();
        let (_, identity) = template::location(&pristine, 4).unwrap();
        let image = format!(
            "us-central1-docker.pkg.dev/p/g5/image@sha256:{}",
            "b".repeat(64)
        );
        let raw = vec![0; 4 * MIB as usize];
        let mut compressed = GzEncoder::new(Vec::new(), Compression::fast());
        compressed.write_all(&raw).unwrap();
        let compressed = compressed.finish().unwrap();
        let binding = Binding {
            version: 1,
            source_digest: source_digest(&image).unwrap(),
            template: identity.clone(),
            raw_sha256: format!("{:x}", Sha256::digest(&raw)),
        };
        let layer = dir.path().join("layer.gz");
        std::fs::write(&layer, payload(&binding, &compressed)).unwrap();
        let stage = dir.path().join("valid");
        std::fs::create_dir(&stage).unwrap();
        unpack(&layer, &stage, &image, &identity).unwrap();
        for (name, source, expected, bad_hash) in [
            (
                "source",
                image.replace(&"b".repeat(64), &"c".repeat(64)),
                identity.clone(),
                false,
            ),
            (
                "capacity",
                image.clone(),
                template::location(&pristine, 5).unwrap().1,
                false,
            ),
            (
                "layout",
                image.clone(),
                serde_json::from_value(serde_json::json!({
                "version":2,"oci_template":"a".repeat(64),"size_mib":4,
                "architecture":std::env::consts::ARCH,"bootstrap_version":999}))
                .unwrap(),
                false,
            ),
            ("hash", image.clone(), identity.clone(), true),
        ] {
            if bad_hash {
                let mut bad = Binding {
                    raw_sha256: "0".repeat(64),
                    ..serde_json::from_slice(&serde_json::to_vec(&binding).unwrap()).unwrap()
                };
                bad.version = 1;
                std::fs::write(&layer, payload(&bad, &compressed)).unwrap();
            }
            let stage = dir.path().join(name);
            std::fs::create_dir(&stage).unwrap();
            assert!(
                unpack(&layer, &stage, &source, &expected).is_err(),
                "{name}"
            );
        }
    }

    #[test]
    fn discovery_binds_source_and_capacity() {
        let image = format!(
            "us-central1-docker.pkg.dev/p/g5/image@sha256:{}",
            "a".repeat(64)
        );
        assert_ne!(
            reference(&image, 32).unwrap(),
            reference(&image, 64).unwrap()
        );
        assert!(reference("ubuntu:latest", 32).is_err());
        assert!(reference("ubuntu@sha256:../bad", 32).is_err());
    }

    #[tokio::test]
    #[ignore = "requires real Linux reflink filesystem, mkfs.ext4 and e2fsck"]
    async fn registry_roundtrip_validates_and_publishes_a_real_ext4_template() {
        use wiremock::{
            matchers::{method, path},
            Mock, MockServer, ResponseTemplate,
        };
        struct Anonymous;
        #[async_trait::async_trait]
        impl g5d_foundation::oci::auth::CredentialProvider for Anonymous {
            fn registry(&self) -> &str {
                "fixture"
            }
        }
        let server = MockServer::start().await;
        let dir = tempfile::tempdir().unwrap();
        let pristine = dir.path().join("sha256").join("a".repeat(64));
        std::fs::create_dir_all(pristine.join("rootfs/etc")).unwrap();
        std::fs::write(pristine.join(".complete"), b"").unwrap();
        std::fs::write(pristine.join("rootfs/sentinel"), b"delivery baseline").unwrap();
        template::prewarm(&pristine, 64, 0, Path::new("mkfs.ext4")).unwrap();
        let image = format!(
            "us-central1-docker.pkg.dev/fixture@sha256:{}",
            "b".repeat(64)
        );
        let exported = dir.path().join("exported");
        export(&image, &pristine, 64, &exported).unwrap();
        let binding =
            serde_json::from_slice(&std::fs::read(exported.join("identity.json")).unwrap())
                .unwrap();
        let blob = payload(
            &binding,
            &std::fs::read(exported.join("image.ext4.gz")).unwrap(),
        );
        let digest = format!("sha256:{:x}", Sha256::digest(&blob));
        let tag = parse_image_ref(&reference(&image, 64).unwrap())
            .unwrap()
            .reference
            .as_str()
            .to_owned();
        let manifest = serde_json::json!({"schemaVersion":2,
            "config":{"mediaType":"application/vnd.oci.image.config.v1+json","digest":format!("sha256:{}","0".repeat(64)),"size":2},
            "layers":[{"mediaType":"application/vnd.oci.image.layer.v1.tar+gzip","digest":digest,"size":blob.len()}]});
        Mock::given(method("GET"))
            .and(path(format!("/v2/fixture/manifests/{tag}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(&manifest))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("/v2/fixture/blobs/{digest}")))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(blob))
            .mount(&server)
            .await;
        let client = g5d_foundation::oci::DistributionClient::for_tests(
            reqwest::Client::new(),
            Box::new(Anonymous),
        );
        let mut endpoint = parse_image_ref(&reference(&image, 64).unwrap()).unwrap();
        endpoint.registry = server.address().to_string();
        let (cache, identity) = template::location(&pristine, 64).unwrap();
        template::remove_owned(&cache.join("current")).unwrap();
        assert!(
            import_from_client(&image, &pristine, 64, 0, &client, &endpoint)
                .await
                .unwrap()
        );
        assert!(template::complete(&cache, &identity));
        let disk = cache.join("current/image.ext4");
        let read = std::process::Command::new("debugfs")
            .args(["-R", "cat /sentinel"])
            .arg(disk)
            .output()
            .unwrap();
        assert_eq!(read.stdout, b"delivery baseline");
        // Warm import must need no registry request and no mkfs, preserving the
        // existing disk rather than introducing a second template owner.
        server.reset().await;
        assert!(
            import_from_client(&image, &pristine, 64, 0, &client, &endpoint)
                .await
                .unwrap()
        );
        assert!(server.received_requests().await.unwrap().is_empty());
    }
}
