//! Door tests against fake upstreams: a wiremock registry behind the OCI
//! facade, a wiremock object store plus metadata server behind the sccache
//! facade, and a template directory seeding primed objects.

use std::{collections::HashMap, sync::Arc};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use g5d_foundation::oci::auth::CredentialProvider;
use g5d_foundation::oci::DistributionClient;
use sha2::{Digest as _, Sha256};
use tower::ServiceExt;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::{CacheDoor, CacheDoorConfig, FleetSccache, GcsReader, Store};

#[derive(Debug)]
struct Anonymous(String);

impl CredentialProvider for Anonymous {
    fn registry(&self) -> &str {
        &self.0
    }
}

fn sha(bytes: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
}

async fn door_with(
    registry: Option<&MockServer>,
    fleet: Option<(&MockServer, &MockServer)>,
    seed_templates: Vec<std::path::PathBuf>,
    max_bytes: u64,
) -> (Arc<CacheDoor>, tempfile::TempDir) {
    door_with_allow(registry, fleet, seed_templates, max_bytes, None).await
}

/// `allow_prefix` restricts the registry's allowlist entry to a repository
/// prefix (`host/prefix`); `None` allows every repository, as for a mirror.
async fn door_with_allow(
    registry: Option<&MockServer>,
    fleet: Option<(&MockServer, &MockServer)>,
    seed_templates: Vec<std::path::PathBuf>,
    max_bytes: u64,
    allow_prefix: Option<&str>,
) -> (Arc<CacheDoor>, tempfile::TempDir) {
    let store_dir = tempfile::tempdir().unwrap();
    let mut upstreams = HashMap::new();
    let mut hosts = Vec::new();
    if let Some(registry) = registry {
        let host = registry.address().to_string();
        hosts.push(match allow_prefix {
            Some(prefix) => format!("{host}/{prefix}"),
            None => host.clone(),
        });
        upstreams.insert(
            host.clone(),
            Arc::new(DistributionClient::for_tests(
                reqwest::Client::new(),
                Box::new(Anonymous(host)),
            )),
        );
    }
    let fleet_reader = fleet.map(|(storage, metadata)| {
        Arc::new(GcsReader::with_endpoints(
            "fleet-bucket",
            "guest-sccache/orgs/g5",
            &storage.uri(),
            &format!("{}/token", metadata.uri()),
        ))
    });
    let config = CacheDoorConfig {
        bind: ([127, 0, 0, 1], 0).into(),
        store_dir: store_dir.path().join("door"),
        max_bytes,
        min_free_bytes: 0,
        upstreams: hosts,
        fleet: fleet.map(|_| FleetSccache {
            bucket: "fleet-bucket".into(),
            prefix: "guest-sccache/orgs/g5".into(),
        }),
        seed_templates,
    };
    let door = CacheDoor::with_components(config, upstreams, fleet_reader)
        .await
        .unwrap();
    (door, store_dir)
}

async fn get(door: &Arc<CacheDoor>, uri: &str) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    let response = door
        .router()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap()
        .to_vec();
    (status, headers, bytes)
}

#[tokio::test]
async fn oci_pull_through_serves_manifests_and_blobs_by_digest_from_the_store() {
    let registry = MockServer::start().await;
    let layer = b"layer-bytes-of-a-tier".to_vec();
    let layer_digest = sha(&layer);
    let manifest = serde_json::json!({
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.manifest.v1+json",
        "config": {"mediaType": "application/vnd.oci.image.config.v1+json", "digest": sha(b"{}"), "size": 2},
        "layers": [{"mediaType": "application/vnd.oci.image.layer.v1.tar+gzip", "digest": layer_digest, "size": layer.len()}]
    })
    .to_string();
    let manifest_digest = sha(manifest.as_bytes());
    Mock::given(method("GET"))
        .and(path("/v2/g5-qa/g5-ci/g5-base/manifests/abc123"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/vnd.oci.image.manifest.v1+json")
                .set_body_bytes(manifest.clone()),
        )
        .expect(1)
        .mount(&registry)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/v2/g5-qa/g5-ci/g5-base/blobs/{layer_digest}"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(layer.clone()))
        .expect(1)
        .mount(&registry)
        .await;
    let (door, _dir) = door_with(Some(&registry), None, Vec::new(), 1 << 30).await;
    let host = registry.address().to_string();

    let (status, headers, _) = get(&door, "/v2/").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["Docker-Distribution-API-Version"], "registry/2.0");

    // Tag pull goes upstream and lands the manifest by digest.
    let (status, headers, body) = get(
        &door,
        &format!("/v2/{host}/g5-qa/g5-ci/g5-base/manifests/abc123"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    assert_eq!(headers["Docker-Content-Digest"], manifest_digest);
    assert_eq!(
        headers["Content-Type"],
        "application/vnd.oci.image.manifest.v1+json"
    );
    assert_eq!(body, manifest.as_bytes());

    // By-digest manifest comes from the store (the upstream mock expects 1 call total).
    let (status, _, body) = get(
        &door,
        &format!("/v2/{host}/g5-qa/g5-ci/g5-base/manifests/{manifest_digest}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, manifest.as_bytes());

    // Blob: first from upstream (digest-verified), second from the store.
    for _ in 0..2 {
        let (status, headers, body) = get(
            &door,
            &format!("/v2/{host}/g5-qa/g5-ci/g5-base/blobs/{layer_digest}"),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(headers["Docker-Content-Digest"], layer_digest);
        assert_eq!(headers["Content-Length"], layer.len().to_string());
        assert_eq!(body, layer);
    }
    let (_, _, stats) = get(&door, "/stats").await;
    let stats: serde_json::Value = serde_json::from_slice(&stats).unwrap();
    assert_eq!(stats["oci"]["blob_upstream"], 1);
    assert_eq!(stats["oci"]["blob_hits"], 1);
    assert_eq!(stats["oci"]["manifest_hits"], 1);
    assert_eq!(stats["oci"]["manifest_upstream"], 1);
}

#[tokio::test]
async fn oci_pull_through_refuses_unknown_upstreams_and_corrupt_blobs() {
    let registry = MockServer::start().await;
    let wanted = sha(b"real");
    Mock::given(method("GET"))
        .and(path(format!("/v2/lib/x/blobs/{wanted}")))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"forged".to_vec()))
        .mount(&registry)
        .await;
    Mock::given(method("GET"))
        .and(path("/v2/lib/x/manifests/huge"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/vnd.oci.image.manifest.v1+json")
                .set_body_bytes(vec![b'{'; (4 << 20) + 1]),
        )
        .mount(&registry)
        .await;
    let (door, dir) = door_with(Some(&registry), None, Vec::new(), 1 << 30).await;
    let host = registry.address().to_string();

    // An upstream manifest past the client's bound is never buffered whole.
    let (status, _, body) = get(&door, &format!("/v2/{host}/lib/x/manifests/huge")).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert!(String::from_utf8_lossy(&body).contains("larger than"));

    let (status, _, body) = get(&door, "/v2/evil.example/lib/x/manifests/latest").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(String::from_utf8_lossy(&body).contains("NAME_UNKNOWN"));

    // A prefixed entry serves only repositories at or under its prefix, so
    // the host identity's wider read access never reaches a guest.
    let (scoped, _scoped_dir) =
        door_with_allow(Some(&registry), None, Vec::new(), 1 << 30, Some("lib")).await;
    for name in ["library/x", "lib-other/x", "x/lib"] {
        let (status, _, body) = get(&scoped, &format!("/v2/{host}/{name}/manifests/latest")).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{name}");
        assert!(
            String::from_utf8_lossy(&body).contains("NAME_UNKNOWN"),
            "{name}"
        );
    }
    let (status, _, _) = get(&scoped, &format!("/v2/{host}/lib/x/blobs/{wanted}")).await;
    assert_eq!(
        status,
        StatusCode::BAD_GATEWAY,
        "the allowed prefix reaches the upstream (whose blob is forged)"
    );

    // A reference is a digest or a plain tag; one carrying path structure
    // (percent-encoded, as dockerd would never send but a guest could) is
    // refused before it can become a URL that escapes the repository.
    let traversal = format!("..%2F..%2Fother%2Fblobs%2F{wanted}");
    let (status, _, body) = get(&scoped, &format!("/v2/{host}/lib/x/manifests/{traversal}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(String::from_utf8_lossy(&body).contains("MANIFEST_INVALID"));
    assert!(
        registry
            .received_requests()
            .await
            .unwrap_or_default()
            .iter()
            .all(|r| !r.url.path().contains("other")),
        "the upstream never saw the traversal"
    );

    let (status, _, body) = get(&door, &format!("/v2/{host}/lib/x/blobs/{wanted}")).await;
    assert_eq!(
        status,
        StatusCode::BAD_GATEWAY,
        "{}",
        String::from_utf8_lossy(&body)
    );
    assert!(
        !dir.path().join("door/blobs").exists(),
        "a corrupt blob must not be stored"
    );
    let leftovers: Vec<_> = std::fs::read_dir(dir.path().join("door"))
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with(".tmp-"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "temporary files are removed on failure"
    );
}

#[tokio::test]
async fn sccache_facade_layers_store_seed_and_fleet_and_refuses_writes() {
    let storage = MockServer::start().await;
    let metadata = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/token"))
        .and(header("Metadata-Flavor", "Google"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "host-token", "expires_in": 3600, "token_type": "Bearer"
        })))
        .expect(1)
        .mount(&metadata)
        .await;
    let fleet_key = "f/e/e/fee0000000000000000000000000000f";
    Mock::given(method("GET"))
        .and(path(format!(
            "/storage/v1/b/fleet-bucket/o/guest-sccache%2Forgs%2Fg5%2F{}",
            fleet_key.replace('/', "%2F")
        )))
        .and(query_param("alt", "media"))
        .and(header("authorization", "Bearer host-token"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"fleet-object".to_vec()))
        .expect(1)
        .mount(&storage)
        .await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&storage)
        .await;

    let templates = tempfile::tempdir().unwrap();
    let seed_key = "a/b/c/abc0000000000000000000000000000a";
    let trusted = templates.path().join("sha256/deadbeef");
    let seed_dir = trusted.join("rootfs").join(super::IMAGE_SCCACHE_DIR);
    std::fs::create_dir_all(&seed_dir).unwrap();
    std::fs::write(trusted.join(".complete"), b"ok").unwrap();
    // A template still being assembled (no completion marker) and one the
    // host did not prepare itself (not listed) never seed, whatever they
    // hold.
    for (dir, key) in [
        (
            templates.path().join("sha256/.tmp-assembling"),
            "b/b/bbb0000000000000000000000000000b",
        ),
        (
            templates.path().join("sha256/custom-image"),
            "c/c/ccc0000000000000000000000000000c",
        ),
    ] {
        let objects = dir.join("rootfs").join(super::IMAGE_SCCACHE_DIR);
        std::fs::create_dir_all(objects.join(&key[..3])).unwrap();
        std::fs::write(objects.join(key), b"untrusted-object").unwrap();
        std::fs::write(dir.join(".complete"), b"ok").unwrap();
    }
    // The image prime is sccache's disk cache, laid out `a/b/<key>`.
    std::fs::create_dir_all(seed_dir.join("a/b")).unwrap();
    std::fs::write(
        seed_dir.join("a/b/abc0000000000000000000000000000a"),
        b"primed-object",
    )
    .unwrap();
    // A rootfs is guest-authored content: a symlink that leaves the template
    // must never be followed, whether it is the object or a directory on the
    // way to it.
    let outside = templates.path().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("host.env"), b"host-secret").unwrap();
    std::fs::write(
        outside.join("dee0000000000000000000000000000d"),
        b"host-secret",
    )
    .unwrap();
    std::os::unix::fs::symlink(
        outside.join("host.env"),
        seed_dir.join("a/b/abc0000000000000000000000000000b"),
    )
    .unwrap();
    std::fs::create_dir_all(seed_dir.join("d")).unwrap();
    std::os::unix::fs::symlink(&outside, seed_dir.join("d/e")).unwrap();

    let (door, _dir) = door_with(
        None,
        Some((&storage, &metadata)),
        vec![
            trusted.clone(),
            templates.path().join("sha256/.tmp-assembling"),
        ],
        1 << 30,
    )
    .await;

    // sccache's startup probe: absent, never stored.
    let (status, _, _) = get(&door, "/sccache/.sccache_check").await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Seed (the image prime) serves without touching the store or the fleet.
    let (status, _, body) = get(&door, &format!("/sccache/{seed_key}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, b"primed-object");
    for linked in [
        "a/b/c/abc0000000000000000000000000000b",
        "d/e/e/dee0000000000000000000000000000d",
        "b/b/b/bbb0000000000000000000000000000b",
        "c/c/c/ccc0000000000000000000000000000c",
    ] {
        let (status, _, body) = get(&door, &format!("/sccache/{linked}")).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{linked}");
        assert!(!body.windows(11).any(|w| w == b"host-secret"), "{linked}");
        assert!(
            !body.windows(16).any(|w| w == b"untrusted-object"),
            "{linked}"
        );
    }

    // Fleet object: fetched once with the host token, then served from the store.
    for _ in 0..2 {
        let (status, _, body) = get(&door, &format!("/sccache/{fleet_key}")).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, b"fleet-object");
    }

    // A miss everywhere is a plain 404; an invalid key never reaches storage.
    let (status, _, _) = get(&door, "/sccache/0/0/0/000000000000000000000000000000").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, _) = get(&door, "/sccache/a/b/c/../../../etc/passwd").await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Writes are refused for every mutating method.
    for m in ["PUT", "POST", "DELETE", "MKCOL"] {
        let response = door
            .router()
            .oneshot(
                Request::builder()
                    .method(m)
                    .uri(format!("/sccache/{fleet_key}"))
                    .body(Body::from("payload"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED, "{m}");
    }

    let (_, _, stats) = get(&door, "/stats").await;
    let stats: serde_json::Value = serde_json::from_slice(&stats).unwrap();
    assert_eq!(stats["sccache"]["seed_hits"], 1);
    assert_eq!(stats["sccache"]["fleet_hits"], 1);
    assert_eq!(stats["sccache"]["store_hits"], 1);
    assert_eq!(
        stats["sccache"]["misses"], 5,
        "symlinked and untrusted seeds fall through to misses"
    );
    assert_eq!(stats["sccache"]["refused_writes"], 4);
    assert_eq!(stats["sccache"]["seed_dirs"], 1);
}

#[tokio::test]
async fn store_evicts_least_recently_used_entries_past_its_bound() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open_with(dir.path(), 100, 0).await.unwrap();
    store
        .put_bytes("blobs/sha256/aaa", &[1u8; 40])
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    store
        .put_bytes("blobs/sha256/bbb", &[2u8; 40])
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    // Touch the oldest so it becomes the most recently used.
    assert!(store.get("blobs/sha256/aaa").await.is_some());
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    store
        .put_bytes("blobs/sha256/ccc", &[3u8; 40])
        .await
        .unwrap();
    assert!(store.used_bytes() <= 100);
    assert!(
        store.get("blobs/sha256/aaa").await.is_some(),
        "recently read entry survives"
    );
    assert!(
        store.get("blobs/sha256/bbb").await.is_none(),
        "least recently used entry is evicted"
    );
    assert!(store.get("blobs/sha256/ccc").await.is_some());
    assert!(store.get("../escape").await.is_none());

    // Reopening re-scans the bytes on disk and drops interrupted temporaries.
    std::fs::write(dir.path().join(".tmp-leftover"), b"x").unwrap();
    let reopened = Store::open_with(dir.path(), 100, 0).await.unwrap();
    assert_eq!(reopened.used_bytes(), 80);
    assert!(!dir.path().join(".tmp-leftover").exists());
}

#[tokio::test]
async fn store_accounts_replacements_reservations_and_flights() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open_with(dir.path(), 100, 0).await.unwrap();
    store
        .put_bytes("blobs/sha256/aaa", &[1u8; 40])
        .await
        .unwrap();
    store
        .put_bytes("blobs/sha256/aaa", &[1u8; 40])
        .await
        .unwrap();
    assert_eq!(store.used_bytes(), 40, "a replacement is accounted once");
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    store
        .put_bytes("blobs/sha256/bbb", &[2u8; 40])
        .await
        .unwrap();

    // A transfer reserves against the bound before it writes, evicting the
    // least recently used committed entry to make room; dropping it
    // uncommitted releases the reservation and removes the file.
    let mut temp = store.temp().await.unwrap();
    temp.account(40).await.unwrap();
    assert_eq!(
        store.inflight_bytes(),
        40,
        "reserved exactly what a small store allows"
    );
    assert!(store.get("blobs/sha256/aaa").await.is_none());
    assert!(store.get("blobs/sha256/bbb").await.is_some());
    std::fs::write(temp.path(), [3u8; 40]).unwrap();
    let path = temp.path().to_path_buf();
    drop(temp);
    assert_eq!(store.inflight_bytes(), 0);
    assert!(!path.exists(), "an uncommitted temporary is removed");

    // No object may take more than half the store.
    let mut big = store.temp().await.unwrap();
    assert!(big.account(51).await.is_err());
    drop(big);
    assert_eq!(store.inflight_bytes(), 0);

    // A key's single-flight entry leaves the map with its last participant:
    // a holder, or a waiter cancelled before it ever acquired the lock.
    let flight = store.lock("blobs/sha256/ccc").await;
    assert_eq!(store.flight_count(), 1);
    let waiter = {
        let store = store.clone();
        tokio::spawn(async move {
            tokio::time::timeout(
                std::time::Duration::from_millis(20),
                store.lock("blobs/sha256/ccc"),
            )
            .await
            .is_err()
        })
    };
    assert!(
        waiter.await.unwrap(),
        "the waiter was cancelled while waiting"
    );
    assert_eq!(store.flight_count(), 1, "the holder still participates");
    drop(flight);
    assert_eq!(store.flight_count(), 0, "nobody holds or waits any more");

    // A leased entry (a reader still streaming it) is neither evicted nor
    // dropped from the accounting, however old it is; the store then refuses
    // new fills rather than overrunning its bound.
    let reader = store.get("blobs/sha256/bbb").await.unwrap();
    assert_eq!(store.lease_count(), 1);
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    store
        .put_bytes("blobs/sha256/ddd", &[4u8; 40])
        .await
        .unwrap();
    assert!(
        store.get("blobs/sha256/bbb").await.is_some(),
        "leased entry survives"
    );
    assert_eq!(store.used_bytes(), 80);
    let other_reader = store.get("blobs/sha256/ddd").await.unwrap();
    assert_eq!(store.lease_count(), 2);
    let mut blocked = store.temp().await.unwrap();
    assert!(
        blocked.account(40).await.is_err(),
        "only leased entries remain, so there is no room"
    );
    assert_eq!(store.used_bytes(), 80, "nothing leased was dropped");
    drop(blocked);
    drop(reader);
    drop(other_reader);
    assert_eq!(store.lease_count(), 0);
}

#[tokio::test]
async fn store_releases_a_reservation_whose_request_was_cancelled() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open_with(dir.path(), 100, 0).await.unwrap();
    store
        .put_bytes("blobs/sha256/aaa", &[1u8; 40])
        .await
        .unwrap();
    store
        .put_bytes("blobs/sha256/bbb", &[2u8; 40])
        .await
        .unwrap();
    // Making room evicts on a blocking thread; a zero timeout cancels the
    // account future at its first pending point, after the reservation was
    // taken. Whether or not it completed, the temporary owns the
    // reservation and dropping it releases it.
    let mut temp = store.temp().await.unwrap();
    let _ = tokio::time::timeout(std::time::Duration::ZERO, temp.account(40)).await;
    assert_eq!(store.inflight_bytes(), 40);
    drop(temp);
    assert_eq!(store.inflight_bytes(), 0);
    assert!(store.used_bytes() <= 100);
}

#[tokio::test]
async fn store_publishes_a_commit_whose_request_was_cancelled() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open_with(dir.path(), 100, 0).await.unwrap();
    let mut temp = store.temp().await.unwrap();
    temp.account(40).await.unwrap();
    std::fs::write(temp.path(), [7u8; 40]).unwrap();
    // The publication runs to completion on a blocking thread even when the
    // request is cancelled at the commit's first pending point, so the file
    // is either counted and present or absent and uncounted, never present
    // and uncounted.
    let _ = tokio::time::timeout(
        std::time::Duration::ZERO,
        store.commit("blobs/sha256/cancelled", temp),
    )
    .await;
    for _ in 0..50 {
        if store.inflight_bytes() == 0 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(store.inflight_bytes(), 0);
    let present = store.get("blobs/sha256/cancelled").await.is_some();
    assert_eq!(store.used_bytes(), if present { 40 } else { 0 });
    assert_eq!(store.charged_bytes(), store.used_bytes());
    let leftovers = std::fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with(".tmp-"))
        .count();
    assert_eq!(leftovers, 0);
}

#[tokio::test]
async fn store_admission_holds_at_the_bound_under_concurrent_fills() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open_with(dir.path(), 100, 0).await.unwrap();
    // Twenty 10-byte fills against a 100-byte bound: whatever interleaving
    // the runtime produces, the committed total never exceeds the bound and
    // the admission counter settles on exactly the committed bytes.
    let fills = (0..20).map(|i| {
        let store = store.clone();
        async move {
            // Every returned entry stays readable: it is leased before the
            // publication releases the lock, so no eviction can take it.
            store
                .put_bytes(&format!("blobs/sha256/{i:03}"), &[i as u8; 10])
                .await
                .and_then(|entry| std::fs::metadata(&entry.path).map(|_| ()))
        }
    });
    let results = futures_util::future::join_all(fills).await;
    assert!(results.iter().any(|r| r.is_ok()));
    assert!(store.used_bytes() <= 100);
    assert_eq!(store.inflight_bytes(), 0);
    assert_eq!(store.charged_bytes(), store.used_bytes());
    let reopened = Store::open_with(dir.path(), 100, 0).await.unwrap();
    assert_eq!(reopened.used_bytes(), store.used_bytes());
}

#[tokio::test]
async fn store_leaves_no_temporary_behind_when_requests_are_cancelled() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open_with(dir.path(), 1000, 0).await.unwrap();
    // Cancel whole fills at their first pending point, repeatedly: the
    // write and the rename belong to the publication task, so a cancelled
    // request leaves either a counted entry or nothing, never a stray
    // temporary (which eviction would ignore and the disk would keep).
    for i in 0..30 {
        let _ = tokio::time::timeout(
            std::time::Duration::ZERO,
            store.put_bytes(&format!("blobs/sha256/c{i:02}"), &[9u8; 10]),
        )
        .await;
    }
    for _ in 0..100 {
        if store.inflight_bytes() == 0 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(store.inflight_bytes(), 0);
    let leftovers = std::fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with(".tmp-"))
        .count();
    assert_eq!(leftovers, 0, "no temporary survives a cancelled request");
    let reopened = Store::open_with(dir.path(), 1000, 0).await.unwrap();
    assert_eq!(store.used_bytes(), reopened.used_bytes());
    assert_eq!(store.charged_bytes(), store.used_bytes());
}

#[tokio::test]
async fn store_invariants_hold_under_cancellations_and_evictions() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open_with(dir.path(), 100, 0).await.unwrap();
    // Fills that overflow the bound (forcing eviction walks) interleaved
    // with fills cancelled at arbitrary points. Whatever the interleaving,
    // afterwards the accounting matches the disk, nothing is reserved, and
    // no temporary is left.
    let tasks = (0..60).map(|i| {
        let store = store.clone();
        async move {
            let key = format!("blobs/sha256/m{i:02}");
            let bytes = [i as u8; 10];
            let fill = store.put_bytes(&key, &bytes);
            if i % 3 == 0 {
                let _ = tokio::time::timeout(
                    std::time::Duration::from_micros((i as u64 % 7) * 50),
                    fill,
                )
                .await;
            } else {
                let _ = fill.await;
            }
        }
    });
    futures_util::future::join_all(tasks).await;
    for _ in 0..100 {
        if store.inflight_bytes() == 0 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(store.inflight_bytes(), 0);
    let reopened = Store::open_with(dir.path(), 100, 0).await.unwrap();
    assert_eq!(store.used_bytes(), reopened.used_bytes());
    assert_eq!(store.charged_bytes(), store.used_bytes());
    assert!(store.used_bytes() <= 100);
    let leftovers = std::fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with(".tmp-"))
        .count();
    assert_eq!(leftovers, 0);
}

#[tokio::test]
async fn store_refuses_fills_under_the_free_space_floor() {
    let dir = tempfile::tempdir().unwrap();
    // A floor no filesystem can satisfy stands in for a disk something else
    // has filled: the door's own share is untouched, yet it stops filling.
    let store = Store::open_with(dir.path(), 1 << 30, u64::MAX)
        .await
        .unwrap();
    let mut temp = store.temp().await.unwrap();
    let error = temp.account(10).await.unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::StorageFull);
    assert_eq!(store.inflight_bytes(), 0);
    assert_eq!(store.charged_bytes(), 0);
}

#[tokio::test]
async fn store_keeps_its_accounting_under_concurrent_fills() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open_with(dir.path(), 1000, 0).await.unwrap();
    let fills = (0..12).map(|i| {
        let store = store.clone();
        async move {
            // Every returned entry stays readable: it is leased before the
            // publication releases the lock, so no eviction can take it.
            store
                .put_bytes(&format!("blobs/sha256/{i:03}"), &[i as u8; 10])
                .await
                .and_then(|entry| std::fs::metadata(&entry.path).map(|_| ()))
        }
    });
    for result in futures_util::future::join_all(fills).await {
        result.unwrap();
    }
    assert_eq!(store.used_bytes(), 120);
    assert_eq!(store.inflight_bytes(), 0);
    // The accounting matches the disk exactly after a reopen.
    let reopened = Store::open_with(dir.path(), 1000, 0).await.unwrap();
    assert_eq!(reopened.used_bytes(), 120);
}

#[test]
fn config_from_env_is_off_without_a_port() {
    let _guard = ENV_LOCK.lock().unwrap();
    for key in [
        super::ENV_PORT,
        super::ENV_DIR,
        super::ENV_MAX_BYTES,
        super::ENV_MIN_FREE_BYTES,
        super::ENV_UPSTREAMS,
        super::ENV_SCCACHE_BUCKET,
        super::ENV_SCCACHE_PREFIX,
    ] {
        std::env::remove_var(key);
    }
    assert!(CacheDoorConfig::from_env(Vec::new()).unwrap().is_none());
    std::env::set_var(super::ENV_PORT, "0");
    assert!(CacheDoorConfig::from_env(Vec::new()).unwrap().is_none());
    std::env::set_var(super::ENV_PORT, "7111");
    std::env::set_var(
        super::ENV_UPSTREAMS,
        "US-central1-docker.pkg.dev/g5-qa-1ccc/g5-tiers/, mirror.gcr.io",
    );
    std::env::set_var(super::ENV_SCCACHE_BUCKET, "g5-qa-1ccc-deploy-artifacts");
    std::env::set_var(super::ENV_SCCACHE_PREFIX, "/guest-sccache/orgs/g5/");
    let config = CacheDoorConfig::from_env(vec!["/tmp/templates/sha256/k".into()])
        .unwrap()
        .unwrap();
    assert_eq!(config.bind.port(), 7111);
    assert!(config.bind.ip().is_loopback());
    assert_eq!(config.max_bytes, super::DEFAULT_MAX_BYTES);
    assert_eq!(config.min_free_bytes, super::store::DEFAULT_MIN_FREE_BYTES);
    assert_eq!(
        config.upstreams,
        vec![
            "mirror.gcr.io",
            "registry-1.docker.io",
            "us-central1-docker.pkg.dev/g5-qa-1ccc/g5-tiers"
        ]
    );
    let fleet = config.fleet.unwrap();
    assert_eq!(fleet.bucket, "g5-qa-1ccc-deploy-artifacts");
    assert_eq!(fleet.prefix, "guest-sccache/orgs/g5");
    std::env::set_var(super::ENV_PORT, "port");
    assert!(CacheDoorConfig::from_env(Vec::new()).is_err());
    for key in [
        super::ENV_PORT,
        super::ENV_UPSTREAMS,
        super::ENV_SCCACHE_BUCKET,
        super::ENV_SCCACHE_PREFIX,
    ] {
        std::env::remove_var(key);
    }
}

static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
