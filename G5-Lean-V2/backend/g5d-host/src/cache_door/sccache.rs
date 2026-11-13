//! sccache object facade in the layout sccache's WebDAV backend uses
//! (`<a>/<b>/<c>/<key>` under the endpoint). Reads fall through the host
//! store, the primed objects of the image templates the host itself prepared
//! (laid out as sccache's disk cache writes them, `<a>/<b>/<key>`), and the
//! organization's fleet prefix in object storage. Every mutating method is
//! refused: sccache's startup check then runs the backend read-only, keeps
//! compiling, and a guest's derived objects stay inside its own VM
//! (`g5.runtime.cache.trust`).

use std::sync::Arc;

use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;
use tokio_util::io::ReaderStream;

use super::{entry_body, safe_relative, CacheDoor, Stats, IMAGE_SCCACHE_DIR};

/// A fleet object read (request plus body) must finish within this, or its
/// fill slot and temporary are released.
const FLEET_TRANSFER_DEADLINE: std::time::Duration = std::time::Duration::from_secs(120);

/// `<hex>/<hex>/<hex>/<hex key>`, the only shape sccache's remote backends
/// write; returns the bare key.
fn sccache_key(path: &str) -> Option<&str> {
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() != 4 {
        return None;
    }
    let key = parts[3];
    let shape_ok = parts[..3]
        .iter()
        .all(|p| p.len() == 1 && p.bytes().all(|b| b.is_ascii_hexdigit()))
        && key.len() >= 16
        && key.bytes().all(|b| b.is_ascii_hexdigit())
        && key.starts_with(&format!("{}{}{}", parts[0], parts[1], parts[2]));
    shape_ok.then_some(key)
}

/// Where sccache's disk cache (the image prime) keeps `key`.
fn disk_layout(key: &str) -> String {
    format!("{}/{}/{}", &key[0..1], &key[1..2], key)
}

pub async fn get(State(door): State<Arc<CacheDoor>>, Path(path): Path<String>) -> Response {
    let path = path.to_ascii_lowercase();
    let Some(key) = sccache_key(&path).filter(|_| safe_relative(&path).is_some()) else {
        // Includes sccache's `.sccache_check` probe: absent is the right
        // answer for a facade that never stores a guest's write.
        return StatusCode::NOT_FOUND.into_response();
    };
    let rel = format!("sccache/{path}");
    if let Some(entry) = door.store.get(&rel).await {
        Stats::bump(&door.stats.sccache_store_hits);
        return stream_entry(&door, entry).await;
    }
    let seed_rel = format!("{IMAGE_SCCACHE_DIR}/{}", disk_layout(key));
    for rootfs in door.seed_roots().await {
        if let Some((file, len)) = super::open_seed(rootfs, seed_rel.clone()).await {
            Stats::bump(&door.stats.sccache_seed_hits);
            return file_response(Body::from_stream(ReaderStream::new(file)), len);
        }
    }
    let Some(fleet) = door.fleet() else {
        Stats::bump(&door.stats.sccache_misses);
        return StatusCode::NOT_FOUND.into_response();
    };
    let _flight = door.store.lock(&rel).await;
    if let Some(entry) = door.store.get(&rel).await {
        Stats::bump(&door.stats.sccache_store_hits);
        return stream_entry(&door, entry).await;
    }
    // Admission (a fill slot) comes before the fleet request, so a guest
    // cannot open more object-storage reads than the door will hold.
    let mut temp = match door.store.temp().await {
        Ok(temp) => temp,
        Err(error) => {
            Stats::bump(&door.stats.errors);
            tracing::warn!(%error, "cache door could not open a temporary");
            return StatusCode::NOT_FOUND.into_response();
        }
    };
    let transfer = async {
        let Some(resp) = fleet.get(&path).await.map_err(std::io::Error::other)? else {
            return Ok(false);
        };
        let mut file = temp.open().await?;
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(std::io::Error::other)?;
            temp.account(chunk.len() as u64).await?;
            file.write_all(&chunk).await?;
        }
        file.flush().await?;
        Ok::<bool, std::io::Error>(true)
    };
    let copied = match tokio::time::timeout(FLEET_TRANSFER_DEADLINE, transfer).await {
        Ok(copied) => copied,
        Err(_) => Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            format!(
                "fleet object transfer exceeded {}s",
                FLEET_TRANSFER_DEADLINE.as_secs()
            ),
        )),
    };
    match copied {
        Ok(true) => match door.store.commit(&rel, temp).await {
            Ok(entry) => {
                Stats::bump(&door.stats.sccache_fleet_hits);
                stream_entry(&door, entry).await
            }
            Err(error) => {
                Stats::bump(&door.stats.errors);
                tracing::warn!(%error, "cache door could not store fleet sccache object");
                StatusCode::NOT_FOUND.into_response()
            }
        },
        Ok(false) => {
            Stats::bump(&door.stats.sccache_misses);
            StatusCode::NOT_FOUND.into_response()
        }
        Err(error) => {
            Stats::bump(&door.stats.errors);
            tracing::warn!(%error, "cache door fleet sccache read failed");
            StatusCode::NOT_FOUND.into_response()
        }
    }
}

/// Any method that is not a read. sccache's startup check turns the refused
/// write into read-only mode and keeps compiling.
pub async fn refuse(State(door): State<Arc<CacheDoor>>) -> Response {
    Stats::bump(&door.stats.sccache_refused_writes);
    (
        StatusCode::METHOD_NOT_ALLOWED,
        "the cache door serves sccache objects read-only",
    )
        .into_response()
}

async fn stream_entry(door: &CacheDoor, entry: super::Entry) -> Response {
    match entry_body(entry).await {
        Ok((body, len)) => file_response(body, len),
        Err(error) => {
            Stats::bump(&door.stats.errors);
            tracing::warn!(%error, "cache door could not open a stored sccache object");
            StatusCode::NOT_FOUND.into_response()
        }
    }
}

fn file_response(body: Body, len: u64) -> Response {
    let mut response = (StatusCode::OK, body).into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    headers.insert(header::CONTENT_LENGTH, HeaderValue::from(len));
    response
}

#[cfg(test)]
mod tests {
    use super::{disk_layout, sccache_key};

    #[test]
    fn only_the_normalized_key_shape_is_served() {
        assert_eq!(
            sccache_key("a/b/c/abc0123456789abcdef0123"),
            Some("abc0123456789abcdef0123")
        );
        assert!(sccache_key("a/b/c/xyz0123456789abcdef0123").is_none());
        assert!(sccache_key("a/b/abc0123456789abcdef0123").is_none());
        assert!(sccache_key("ab/c/d/abcd0123456789abcdef0123").is_none());
        assert!(sccache_key(".sccache_check").is_none());
        assert!(sccache_key("a/b/c/../etc/passwd").is_none());
    }

    #[test]
    fn seeds_are_looked_up_in_the_disk_cache_layout() {
        assert_eq!(
            disk_layout("abc0123456789abcdef0123"),
            "a/b/abc0123456789abcdef0123"
        );
    }
}
