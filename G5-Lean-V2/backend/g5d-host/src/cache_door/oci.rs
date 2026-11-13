//! OCI Distribution pull-through facade. A guest's dockerd pulls
//! `g5-host:<port>/<upstream-host>/<repository>:<tag>`; the door fetches from
//! that upstream with the host's credentials, stores manifests and blobs by
//! digest, and serves them verbatim. Only allowlisted upstream repositories
//! are proxied (a host alone allows a public mirror, a host with a repository
//! prefix allows that prefix), references are validated before they reach a
//! URL, only reads exist, and every served blob was digest-verified on the
//! way in; dockerd verifies again on the way out.

use std::sync::Arc;

use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use futures_util::StreamExt;
use g5d_foundation::oci::{Digest, ImageRef, Reference};
use sha2::{Digest as _, Sha256};
use tokio::io::AsyncWriteExt;
use tokio_util::io::ReaderStream;

use super::{entry_body, held_body, CacheDoor, Entry, Stats};

/// Longest media type line a stored manifest may start with.
const MAX_MEDIA_TYPE_LINE: usize = 256;

const API_VERSION_HEADER: &str = "Docker-Distribution-API-Version";
/// A blob transfer (request plus body) must finish within this, or its fill
/// slot and temporary are released; a stalled upstream cannot hold the
/// door's slots forever.
const BLOB_TRANSFER_DEADLINE: std::time::Duration = std::time::Duration::from_secs(900);
/// A manifest fetch (authentication, request and body) must finish within
/// this, or its permit is released.
const MANIFEST_FETCH_DEADLINE: std::time::Duration = std::time::Duration::from_secs(60);
const CONTENT_DIGEST_HEADER: &str = "Docker-Content-Digest";
const DEFAULT_MANIFEST_ACCEPTS: &[&str] = &[
    "application/vnd.oci.image.index.v1+json",
    "application/vnd.docker.distribution.manifest.list.v2+json",
    "application/vnd.oci.image.manifest.v1+json",
    "application/vnd.docker.distribution.manifest.v2+json",
];

pub async fn api_version() -> Response {
    (
        StatusCode::OK,
        [(API_VERSION_HEADER, "registry/2.0")],
        Json(serde_json::json!({})),
    )
        .into_response()
}

fn registry_error(status: StatusCode, code: &str, message: String) -> Response {
    (
        status,
        [(API_VERSION_HEADER, "registry/2.0")],
        Json(serde_json::json!({"errors": [{"code": code, "message": message}]})),
    )
        .into_response()
}

/// Split `<upstream-host>/<repository>` and check the repository's shape
/// (lowercase path components, as the distribution spec restricts names).
fn parse_name(name: &str) -> Option<(String, String)> {
    let (host, repository) = name.split_once('/')?;
    if host.is_empty() || repository.is_empty() {
        return None;
    }
    let component_ok = |c: &str| {
        !c.is_empty()
            && c.bytes().all(|b| {
                b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'_' || b == b'-'
            })
            && !c.starts_with(['.', '_', '-'])
    };
    if !repository.split('/').all(component_ok) {
        return None;
    }
    Some((host.to_ascii_lowercase(), repository.to_string()))
}

/// A manifest reference as the distribution spec allows it: a digest or a
/// tag `[A-Za-z0-9_][A-Za-z0-9._-]{0,127}`. Anything else (a `/`, a `..`)
/// would become path structure in the upstream URL and must never get there.
fn parse_reference(reference: &str) -> Option<Reference> {
    if let Ok(digest) = Digest::parse(reference) {
        return Some(Reference::Digest(digest));
    }
    let mut bytes = reference.bytes();
    let first_ok = bytes
        .next()
        .is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_');
    let rest_ok = reference.len() <= 128
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.' || b == b'-');
    (first_ok && rest_ok).then(|| Reference::Tag(reference.to_string()))
}

pub async fn handle(
    State(door): State<Arc<CacheDoor>>,
    Path(rest): Path<String>,
    headers: HeaderMap,
) -> Response {
    if let Some((name, reference)) = rest.rsplit_once("/manifests/") {
        return manifest(&door, name, reference, &headers).await;
    }
    if let Some((name, digest)) = rest.rsplit_once("/blobs/") {
        return blob(&door, name, digest).await;
    }
    registry_error(
        StatusCode::NOT_FOUND,
        "UNSUPPORTED",
        "only manifests and blobs are served".into(),
    )
}

type Upstream = (Arc<g5d_foundation::oci::DistributionClient>, String, String);

/// The upstream client for `name`, refused (as a registry error) unless the
/// repository is allowlisted. Checked before the store as well as before the
/// upstream, so a guest cannot read even cached content of a repository the
/// door does not serve.
fn upstream_for(door: &CacheDoor, name: &str) -> Result<Upstream, Box<Response>> {
    let Some((host, repository)) = parse_name(name) else {
        return Err(Box::new(registry_error(
            StatusCode::NOT_FOUND,
            "NAME_INVALID",
            format!("invalid name {name}"),
        )));
    };
    if !door.allows(&host, &repository) {
        return Err(Box::new(registry_error(
            StatusCode::NOT_FOUND,
            "NAME_UNKNOWN",
            format!("{host}/{repository} is not served by this door"),
        )));
    }
    let Some(client) = door.upstream(&host) else {
        return Err(Box::new(registry_error(
            StatusCode::NOT_FOUND,
            "NAME_UNKNOWN",
            format!("upstream registry {host} is not served by this door"),
        )));
    };
    Ok((client, host, repository))
}

/// A stored manifest is one file: the media type line, then the raw bytes,
/// so the pair is accounted, touched and evicted together.
fn encode_manifest(media_type: &str, bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(media_type.len() + 1 + bytes.len());
    out.extend_from_slice(media_type.as_bytes());
    out.push(b'\n');
    out.extend_from_slice(bytes);
    out
}

#[cfg(test)]
fn decode_manifest(stored: Vec<u8>) -> Option<(String, Vec<u8>)> {
    let nl = stored.iter().position(|b| *b == b'\n')?;
    let media_type = std::str::from_utf8(&stored[..nl]).ok()?.to_string();
    if media_type.is_empty() || HeaderValue::from_str(&media_type).is_err() {
        return None;
    }
    Some((media_type, stored[nl + 1..].to_vec()))
}

/// Stream a stored manifest without buffering it: read its media type line,
/// then serve the rest of the file from disk under the entry's lease. `None`
/// means the stored file is not a manifest the door wrote (refetch).
async fn manifest_entry_body(entry: Entry) -> std::io::Result<Option<(Body, String, u64)>> {
    use tokio::io::{AsyncReadExt, AsyncSeekExt};
    let mut file = tokio::fs::File::open(&entry.path).await?;
    let mut head = vec![0u8; MAX_MEDIA_TYPE_LINE.min(entry.len as usize)];
    let mut filled = 0;
    while filled < head.len() {
        let n = file.read(&mut head[filled..]).await?;
        if n == 0 {
            break;
        }
        filled += n;
    }
    let Some(nl) = head[..filled].iter().position(|b| *b == b'\n') else {
        return Ok(None);
    };
    let media_type = match std::str::from_utf8(&head[..nl]) {
        Ok(m) if !m.is_empty() && HeaderValue::from_str(m).is_ok() => m.to_string(),
        _ => return Ok(None),
    };
    let start = nl as u64 + 1;
    file.seek(std::io::SeekFrom::Start(start)).await?;
    let body = held_body(ReaderStream::new(file), entry.lease);
    Ok(Some((body, media_type, entry.len - start)))
}

async fn manifest(door: &CacheDoor, name: &str, reference: &str, headers: &HeaderMap) -> Response {
    let (client, host, repository) = match upstream_for(door, name) {
        Ok(parts) => parts,
        Err(response) => return *response,
    };
    let Some(reference) = parse_reference(reference) else {
        return registry_error(
            StatusCode::NOT_FOUND,
            "MANIFEST_INVALID",
            format!("invalid reference {reference}"),
        );
    };
    if let Reference::Digest(digest) = &reference {
        let rel = format!("manifests/sha256/{}", digest.hex());
        if let Some(entry) = door.store.get(&rel).await {
            if let Ok(Some((body, media_type, len))) = manifest_entry_body(entry).await {
                Stats::bump(&door.stats.manifest_hits);
                return manifest_response(body, len, &media_type, digest);
            }
        }
    }
    let accepts: Vec<String> = headers
        .get_all(header::ACCEPT)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(','))
        .map(|v| v.split(';').next().unwrap_or("").trim().to_string())
        .filter(|v| !v.is_empty() && v != "*/*")
        .collect();
    let accepts: Vec<&str> = if accepts.is_empty() {
        DEFAULT_MANIFEST_ACCEPTS.to_vec()
    } else {
        accepts.iter().map(String::as_str).collect()
    };
    let image = ImageRef {
        registry: host,
        repository,
        reference,
    };
    Stats::bump(&door.stats.manifest_upstream);
    let Some(permit) = door.manifest_fetch_permit().await else {
        return registry_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "UNAVAILABLE",
            "cache door is shutting down".into(),
        );
    };
    let fetched = tokio::time::timeout(
        MANIFEST_FETCH_DEADLINE,
        client.fetch_manifest_raw(&image, &accepts),
    )
    .await
    .unwrap_or_else(|_| {
        Err(g5d_foundation::oci::OciError::RegistryHttp {
            url: format!("{}/{}", image.registry, image.repository),
            status: StatusCode::GATEWAY_TIMEOUT.as_u16(),
            body: format!(
                "manifest fetch exceeded {}s",
                MANIFEST_FETCH_DEADLINE.as_secs()
            ),
        })
    });
    let raw = match fetched {
        Ok(raw) => raw,
        Err(error) => {
            Stats::bump(&door.stats.errors);
            tracing::warn!(name, %error, "cache door manifest fetch failed");
            return registry_error(
                StatusCode::BAD_GATEWAY,
                "MANIFEST_UNKNOWN",
                error.to_string(),
            );
        }
    };
    let rel = format!("manifests/sha256/{}", raw.digest.hex());
    let _flight = door.store.lock(&rel).await;
    if door.store.get(&rel).await.is_none() {
        if let Err(error) = door
            .store
            .put_bytes(&rel, &encode_manifest(&raw.media_type, &raw.bytes))
            .await
        {
            Stats::bump(&door.stats.errors);
            tracing::warn!(%error, "cache door could not store manifest");
        }
    }
    // The buffered bytes travel with the admission permit, so the memory a
    // fetch holds is released only once the response has been sent.
    let len = raw.bytes.len() as u64;
    let body = held_body(
        futures_util::stream::once(std::future::ready(Ok(raw.bytes))),
        permit,
    );
    manifest_response(body, len, &raw.media_type, &raw.digest)
}

fn manifest_response(body: Body, len: u64, media_type: &str, digest: &Digest) -> Response {
    let mut response = (StatusCode::OK, body).into_response();
    let headers = response.headers_mut();
    headers.insert(API_VERSION_HEADER, HeaderValue::from_static("registry/2.0"));
    headers.insert(header::CONTENT_LENGTH, HeaderValue::from(len));
    if let Ok(value) = HeaderValue::from_str(media_type) {
        headers.insert(header::CONTENT_TYPE, value);
    }
    if let Ok(value) = HeaderValue::from_str(digest.as_str()) {
        headers.insert(CONTENT_DIGEST_HEADER, value);
    }
    response
}

async fn blob(door: &CacheDoor, name: &str, digest: &str) -> Response {
    let (client, host, repository) = match upstream_for(door, name) {
        Ok(parts) => parts,
        Err(response) => return *response,
    };
    let Ok(digest) = Digest::parse(digest) else {
        return registry_error(
            StatusCode::BAD_REQUEST,
            "DIGEST_INVALID",
            format!("invalid digest {digest}"),
        );
    };
    let rel = format!("blobs/sha256/{}", digest.hex());
    if let Some(entry) = door.store.get(&rel).await {
        Stats::bump(&door.stats.blob_hits);
        return stream_blob(door, entry, &digest).await;
    }
    let _flight = door.store.lock(&rel).await;
    if let Some(entry) = door.store.get(&rel).await {
        Stats::bump(&door.stats.blob_hits);
        return stream_blob(door, entry, &digest).await;
    }
    Stats::bump(&door.stats.blob_upstream);
    // Admission (a fill slot) comes before any upstream request, so a guest
    // cannot open more upstream transfers than the door will hold.
    let mut temp = match door.store.temp().await {
        Ok(temp) => temp,
        Err(error) => {
            Stats::bump(&door.stats.errors);
            return registry_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "BLOB_UPLOAD_UNKNOWN",
                error.to_string(),
            );
        }
    };
    let image = ImageRef {
        registry: host,
        repository,
        reference: Reference::Digest(digest.clone()),
    };
    let transfer = async {
        let mut stream = client
            .open_blob(&image, &digest)
            .await
            .map_err(std::io::Error::other)?;
        let mut file = temp.open().await?;
        let mut hasher = Sha256::new();
        let mut total = 0u64;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            temp.account(chunk.len() as u64).await?;
            hasher.update(&chunk);
            total += chunk.len() as u64;
            file.write_all(&chunk).await?;
        }
        file.flush().await?;
        let actual = format!("sha256:{}", hex::encode(hasher.finalize()));
        if actual != digest.as_str() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("upstream blob digest {actual} != {}", digest.as_str()),
            ));
        }
        Ok(total)
    };
    let written = match tokio::time::timeout(BLOB_TRANSFER_DEADLINE, transfer).await {
        Ok(written) => written,
        Err(_) => Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            format!(
                "upstream blob transfer exceeded {}s",
                BLOB_TRANSFER_DEADLINE.as_secs()
            ),
        )),
    };
    match written {
        Ok(total) => {
            Stats::add(&door.stats.blob_upstream_bytes, total);
            match door.store.commit(&rel, temp).await {
                Ok(entry) => stream_blob(door, entry, &digest).await,
                Err(error) => {
                    Stats::bump(&door.stats.errors);
                    registry_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "BLOB_UPLOAD_UNKNOWN",
                        error.to_string(),
                    )
                }
            }
        }
        Err(error) => {
            Stats::bump(&door.stats.errors);
            tracing::warn!(name, digest = digest.as_str(), %error, "cache door blob transfer failed");
            registry_error(StatusCode::BAD_GATEWAY, "BLOB_UNKNOWN", error.to_string())
        }
    }
}

async fn stream_blob(door: &CacheDoor, entry: Entry, digest: &Digest) -> Response {
    let (body, len) = match entry_body(entry).await {
        Ok(opened) => opened,
        Err(error) => {
            Stats::bump(&door.stats.errors);
            return registry_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "BLOB_UNKNOWN",
                error.to_string(),
            );
        }
    };
    Stats::add(&door.stats.blob_bytes_served, len);
    let mut response = (StatusCode::OK, body).into_response();
    let headers = response.headers_mut();
    headers.insert(API_VERSION_HEADER, HeaderValue::from_static("registry/2.0"));
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    headers.insert(header::CONTENT_LENGTH, HeaderValue::from(len));
    if let Ok(value) = HeaderValue::from_str(digest.as_str()) {
        headers.insert(CONTENT_DIGEST_HEADER, value);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::{decode_manifest, encode_manifest, parse_name, parse_reference};
    use g5d_foundation::oci::Reference;

    #[test]
    fn names_split_into_upstream_and_repository() {
        assert_eq!(
            parse_name("us-central1-docker.pkg.dev/g5-qa-1ccc/g5-tiers/g5-base").unwrap(),
            (
                "us-central1-docker.pkg.dev".into(),
                "g5-qa-1ccc/g5-tiers/g5-base".into()
            )
        );
        assert_eq!(
            parse_name("mirror.gcr.io/library/redis").unwrap(),
            ("mirror.gcr.io".into(), "library/redis".into())
        );
        assert!(parse_name("mirror.gcr.io").is_none());
        assert!(parse_name("mirror.gcr.io/").is_none());
        assert!(parse_name("mirror.gcr.io/Library/redis").is_none());
        assert!(parse_name("mirror.gcr.io/../etc").is_none());
    }

    #[test]
    fn references_are_digests_or_plain_tags() {
        assert!(matches!(parse_reference("26.04"), Some(Reference::Tag(t)) if t == "26.04"));
        assert!(matches!(parse_reference("_x-y.z"), Some(Reference::Tag(_))));
        let digest = format!("sha256:{}", "a".repeat(64));
        assert!(matches!(
            parse_reference(&digest),
            Some(Reference::Digest(_))
        ));
        for bad in [
            "",
            "../../other/blobs/sha256:abc",
            "a/b",
            ".hidden",
            "-dash",
            "sha256:abc",
            "tag with space",
        ] {
            assert!(parse_reference(bad).is_none(), "{bad:?}");
        }
        assert!(parse_reference(&"t".repeat(129)).is_none());
    }

    #[test]
    fn stored_manifests_carry_their_media_type() {
        let stored = encode_manifest("application/vnd.oci.image.index.v1+json", b"{}\n{");
        let (media_type, bytes) = decode_manifest(stored).unwrap();
        assert_eq!(media_type, "application/vnd.oci.image.index.v1+json");
        assert_eq!(bytes, b"{}\n{");
        assert!(decode_manifest(b"\n{}".to_vec()).is_none());
        assert!(decode_manifest(b"{}".to_vec()).is_none());
    }
}
