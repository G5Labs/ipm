//! Host cache door (ontology `g5.runtime.cache.door`, `g5.runtime.cache.fleet`).
//!
//! One read-through cache on the host loopback that every VM on the host
//! reaches through the `g5-host` alias passt maps onto host loopback. Two
//! facades:
//!
//! - `/v2/<upstream-host>/<repository>/...`: an OCI Distribution pull-through
//!   for allowlisted upstream registries. Manifests and blobs are stored by
//!   digest in the door's own content-addressed store and served verbatim; the
//!   consumer (dockerd) verifies every digest itself, so a cache entry can
//!   only fail, never substitute (`g5.runtime.cache.trust`).
//! - `/sccache/<key>`: an sccache object facade in the layout sccache's
//!   WebDAV backend expects. Reads fall through the host store, the primed
//!   objects of the image templates the host itself prepared (its
//!   organization's platform image, never a workspace-selected custom image,
//!   since seeds are derived objects served to every VM) and the
//!   organization's fleet prefix in object storage; every mutating method is
//!   refused, so a guest's derived objects never reach another VM.
//!
//! The store is bounded and evicted least recently used; it is disposable
//! and dies with the host. Nothing here is a save of workspace state.

mod gcs;
mod nofollow;
mod oci;
mod sccache;
mod store;

use std::{
    collections::HashMap,
    net::SocketAddr,
    path::{Path, PathBuf},
    pin::Pin,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    task::{Context, Poll},
};

use axum::{
    extract::State,
    response::IntoResponse,
    routing::{any, get},
    Json, Router,
};
use g5_contracts::error::G5Error;
use g5_contracts::types::worker_host::WORKER_HOST_CACHE_DOOR_GB;
use g5d_foundation::oci::{
    template_is_complete, upstream_client, DistributionClient, TEMPLATE_ROOTFS_DIR,
};
use tokio::sync::watch;

pub use gcs::GcsReader;
pub use store::{Entry, Lease, Store};

/// Port to serve on (host loopback). Unset or `0` leaves the door off.
pub const ENV_PORT: &str = "G5D_CACHE_DOOR_PORT";
/// Store directory. Default `/var/g5d-data/cache/door`.
pub const ENV_DIR: &str = "G5D_CACHE_DOOR_DIR";
/// Store bound in bytes. Default: the share the host boot disk sizing
/// reserves for the door (`WORKER_HOST_CACHE_DOOR_GB`); bring-up passes the
/// same constant so the two cannot drift.
pub const ENV_MAX_BYTES: &str = "G5D_CACHE_DOOR_MAX_BYTES";
/// Free space the store's filesystem must keep (bytes). Default 4 GiB.
pub const ENV_MIN_FREE_BYTES: &str = "G5D_CACHE_DOOR_MIN_FREE_BYTES";
/// Comma-separated upstream entries the OCI facade may proxy: `host` serves
/// every repository of a public mirror, `host/prefix` only repositories at
/// or under that prefix (the host identity may read far more).
pub const ENV_UPSTREAMS: &str = "G5D_CACHE_REGISTRY_UPSTREAMS";
/// Object-storage bucket holding the organization's fleet sccache prefix.
pub const ENV_SCCACHE_BUCKET: &str = "G5D_CACHE_SCCACHE_BUCKET";
/// Prefix inside that bucket (already organization-scoped by the API).
pub const ENV_SCCACHE_PREFIX: &str = "G5D_CACHE_SCCACHE_PREFIX";

pub const DEFAULT_DIR: &str = "/var/g5d-data/cache/door";
pub const DEFAULT_MAX_BYTES: u64 = (WORKER_HOST_CACHE_DOOR_GB as u64) << 30;
pub const DEFAULT_UPSTREAMS: &[&str] = &["mirror.gcr.io", "registry-1.docker.io"];
/// Where a guest image's primed sccache objects live inside its rootfs
/// (`Dockerfile.g5d-g5dev` `SCCACHE_DIR`).
pub const IMAGE_SCCACHE_DIR: &str = "opt/g5-build-cache/sccache";

#[derive(Debug, Clone)]
pub struct FleetSccache {
    pub bucket: String,
    pub prefix: String,
}

#[derive(Debug, Clone)]
pub struct CacheDoorConfig {
    pub bind: SocketAddr,
    pub store_dir: PathBuf,
    pub max_bytes: u64,
    /// Free space the store's filesystem must keep after a fill.
    pub min_free_bytes: u64,
    /// Allowlist entries, `host` or `host/repository-prefix`.
    pub upstreams: Vec<String>,
    pub fleet: Option<FleetSccache>,
    /// Template directories (`<templates>/sha256/<key>`) whose primed sccache
    /// objects seed the facade: only images the host itself prepared as its
    /// organization's platform image. A workspace-selected custom image is
    /// never a seed, because seeds are derived objects served to every VM
    /// (`g5.runtime.cache.trust`).
    pub seed_templates: Vec<PathBuf>,
}

impl CacheDoorConfig {
    /// Read the door's configuration from the host environment. `None` means
    /// the door is off (no port), which is every host the bring-up did not
    /// configure.
    pub fn from_env(seed_templates: Vec<PathBuf>) -> Result<Option<Self>, G5Error> {
        let Some(port) = std::env::var(ENV_PORT)
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
        else {
            return Ok(None);
        };
        let port: u16 = port
            .parse()
            .map_err(|_| G5Error::Config(format!("{ENV_PORT} must be a port number: {port}")))?;
        if port == 0 {
            return Ok(None);
        }
        let store_dir = std::env::var(ENV_DIR)
            .ok()
            .filter(|v| !v.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(DEFAULT_DIR));
        let max_bytes = match std::env::var(ENV_MAX_BYTES) {
            Ok(v) if !v.trim().is_empty() => v
                .trim()
                .parse::<u64>()
                .map_err(|_| G5Error::Config(format!("{ENV_MAX_BYTES} must be bytes: {v}")))?,
            _ => DEFAULT_MAX_BYTES,
        };
        let min_free_bytes = match std::env::var(ENV_MIN_FREE_BYTES) {
            Ok(v) if !v.trim().is_empty() => v
                .trim()
                .parse::<u64>()
                .map_err(|_| G5Error::Config(format!("{ENV_MIN_FREE_BYTES} must be bytes: {v}")))?,
            _ => store::DEFAULT_MIN_FREE_BYTES,
        };
        let mut upstreams: Vec<String> = DEFAULT_UPSTREAMS.iter().map(|s| s.to_string()).collect();
        if let Ok(list) = std::env::var(ENV_UPSTREAMS) {
            for entry in list.split(',') {
                let entry = entry.trim().trim_matches('/');
                if entry.is_empty() {
                    continue;
                }
                let (host, prefix) = split_entry(entry);
                let entry = match prefix {
                    Some(prefix) => format!("{}/{prefix}", host.to_ascii_lowercase()),
                    None => host.to_ascii_lowercase(),
                };
                if !upstreams.contains(&entry) {
                    upstreams.push(entry);
                }
            }
        }
        let fleet = std::env::var(ENV_SCCACHE_BUCKET)
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .map(|bucket| FleetSccache {
                bucket,
                prefix: std::env::var(ENV_SCCACHE_PREFIX)
                    .ok()
                    .map(|v| v.trim().trim_matches('/').to_string())
                    .filter(|v| !v.is_empty())
                    .unwrap_or_else(|| "guest-sccache".to_string()),
            });
        Ok(Some(Self {
            bind: SocketAddr::from(([127, 0, 0, 1], port)),
            store_dir,
            max_bytes,
            min_free_bytes,
            upstreams,
            fleet,
            seed_templates,
        }))
    }
}

/// Counters the measurement gate reads (`/stats`).
#[derive(Debug, Default)]
pub struct Stats {
    pub manifest_hits: AtomicU64,
    pub manifest_upstream: AtomicU64,
    pub blob_hits: AtomicU64,
    pub blob_upstream: AtomicU64,
    pub blob_upstream_bytes: AtomicU64,
    pub blob_bytes_served: AtomicU64,
    pub sccache_store_hits: AtomicU64,
    pub sccache_seed_hits: AtomicU64,
    pub sccache_fleet_hits: AtomicU64,
    pub sccache_misses: AtomicU64,
    pub sccache_refused_writes: AtomicU64,
    pub errors: AtomicU64,
}

impl Stats {
    fn bump(counter: &AtomicU64) {
        counter.fetch_add(1, Ordering::Relaxed);
    }
    fn add(counter: &AtomicU64, n: u64) {
        counter.fetch_add(n, Ordering::Relaxed);
    }
}

pub struct CacheDoor {
    pub config: CacheDoorConfig,
    pub store: Arc<Store>,
    upstreams: HashMap<String, Arc<DistributionClient>>,
    /// `(host, repository prefix)` pairs the OCI facade serves.
    allow: Vec<(String, Option<String>)>,
    /// Upstream manifest fetches in flight: each buffers up to the client's
    /// manifest bound, so admission keeps a burst of guest requests from
    /// holding unbounded memory.
    manifest_fetches: Arc<tokio::sync::Semaphore>,
    fleet: Option<Arc<GcsReader>>,
    pub stats: Arc<Stats>,
}

impl CacheDoor {
    /// Build the door from its configuration with production upstream
    /// clients and, when a fleet bucket is configured, a reader that uses the
    /// host's own GCP identity.
    pub async fn from_config(config: CacheDoorConfig) -> Result<Arc<Self>, G5Error> {
        let mut upstreams = HashMap::new();
        for entry in &config.upstreams {
            let (host, _) = split_entry(entry);
            if upstreams.contains_key(host) {
                continue;
            }
            match upstream_client(host) {
                Some(client) => {
                    upstreams.insert(host.to_string(), Arc::new(client));
                }
                None => {
                    return Err(G5Error::Config(format!(
                        "{ENV_UPSTREAMS}: no credential provider for upstream registry {host}"
                    )))
                }
            }
        }
        let fleet = config
            .fleet
            .as_ref()
            .map(|fleet| Arc::new(GcsReader::new(&fleet.bucket, &fleet.prefix)));
        Self::with_components(config, upstreams, fleet).await
    }

    /// Build the door from explicit components (tests inject wiremock-backed
    /// upstreams and a fleet reader pointed at a fake object store).
    pub async fn with_components(
        config: CacheDoorConfig,
        upstreams: HashMap<String, Arc<DistributionClient>>,
        fleet: Option<Arc<GcsReader>>,
    ) -> Result<Arc<Self>, G5Error> {
        let store = Store::open_with(&config.store_dir, config.max_bytes, config.min_free_bytes)
            .await
            .map_err(|error| G5Error::Internal {
                message: format!(
                    "cache door store {} unusable: {error}",
                    config.store_dir.display()
                ),
            })?;
        let allow = config
            .upstreams
            .iter()
            .map(|entry| {
                let (host, prefix) = split_entry(entry);
                (host.to_string(), prefix.map(str::to_string))
            })
            .collect();
        Ok(Arc::new(Self {
            config,
            store,
            upstreams,
            allow,
            manifest_fetches: Arc::new(tokio::sync::Semaphore::new(store::MAX_CONCURRENT_FILLS)),
            fleet,
            stats: Arc::new(Stats::default()),
        }))
    }

    pub(crate) fn upstream(&self, host: &str) -> Option<Arc<DistributionClient>> {
        self.upstreams.get(host).cloned()
    }

    /// Whether the OCI facade serves `repository` on `host`: an entry without
    /// a prefix allows every repository (a public mirror), one with a prefix
    /// allows only repositories at or under it.
    pub(crate) fn allows(&self, host: &str, repository: &str) -> bool {
        self.allow.iter().any(|(h, prefix)| {
            h == host
                && match prefix {
                    None => true,
                    Some(p) => {
                        repository == p
                            || repository
                                .strip_prefix(p.as_str())
                                .is_some_and(|rest| rest.starts_with('/'))
                    }
                }
        })
    }

    pub(crate) fn fleet(&self) -> Option<Arc<GcsReader>> {
        self.fleet.clone()
    }

    /// Wait for an upstream manifest fetch slot. The permit is owned so the
    /// response body can hold it until the buffered manifest is sent.
    pub(crate) async fn manifest_fetch_permit(&self) -> Option<tokio::sync::OwnedSemaphorePermit> {
        self.manifest_fetches.clone().acquire_owned().await.ok()
    }

    /// The rootfs of every trusted template that is completely assembled
    /// (a template mid-assembly is a `.tmp-*` sibling without the completion
    /// marker and is still being written). What lies under a rootfs is image
    /// content: readers open it through [`open_seed`]'s symlink-refusing
    /// walk, and nothing is ever written into a template.
    pub(crate) async fn seed_roots(&self) -> Vec<PathBuf> {
        let mut roots = Vec::new();
        for template in &self.config.seed_templates {
            let name = template.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.is_empty() || name.starts_with('.') {
                continue;
            }
            let probe = template.clone();
            let complete = tokio::task::spawn_blocking(move || template_is_complete(&probe))
                .await
                .unwrap_or(false);
            if complete {
                roots.push(template.join(TEMPLATE_ROOTFS_DIR));
            }
        }
        roots
    }

    /// Trusted templates whose rootfs carries a primed sccache directory.
    pub(crate) async fn seed_dir_count(&self) -> usize {
        let mut count = 0;
        for rootfs in self.seed_roots().await {
            let present = tokio::task::spawn_blocking(move || {
                nofollow::open_nofollow(&rootfs, IMAGE_SCCACHE_DIR, true).is_ok()
            })
            .await
            .unwrap_or(false);
            if present {
                count += 1;
            }
        }
        count
    }

    pub fn router(self: &Arc<Self>) -> Router {
        Router::new()
            .route("/v2/", get(oci::api_version))
            .route("/v2/*rest", get(oci::handle))
            .route("/sccache/*key", get(sccache::get).fallback(sccache::refuse))
            .route("/stats", get(stats))
            .fallback(any(not_found))
            .with_state(self.clone())
    }
}

async fn stats(State(door): State<Arc<CacheDoor>>) -> impl IntoResponse {
    let s = &door.stats;
    let seed_dirs = door.seed_dir_count().await;
    let load = |c: &AtomicU64| c.load(Ordering::Relaxed);
    Json(serde_json::json!({
        "oci": {
            "manifest_hits": load(&s.manifest_hits),
            "manifest_upstream": load(&s.manifest_upstream),
            "blob_hits": load(&s.blob_hits),
            "blob_upstream": load(&s.blob_upstream),
            "blob_upstream_bytes": load(&s.blob_upstream_bytes),
            "blob_bytes_served": load(&s.blob_bytes_served),
        },
        "sccache": {
            "store_hits": load(&s.sccache_store_hits),
            "seed_hits": load(&s.sccache_seed_hits),
            "fleet_hits": load(&s.sccache_fleet_hits),
            "misses": load(&s.sccache_misses),
            "refused_writes": load(&s.sccache_refused_writes),
            "seed_dirs": seed_dirs,
            "fleet": door.config.fleet.as_ref().map(|f| format!("gs://{}/{}", f.bucket, f.prefix)),
        },
        "store": {
            "dir": door.config.store_dir.display().to_string(),
            "used_bytes": door.store.used_bytes(),
            "inflight_bytes": door.store.inflight_bytes(),
            "leased_entries": door.store.lease_count(),
            "max_bytes": door.config.max_bytes,
            "min_free_bytes": door.config.min_free_bytes,
        },
        "errors": load(&s.errors),
    }))
}

async fn not_found() -> impl IntoResponse {
    axum::http::StatusCode::NOT_FOUND
}

/// Serve the door until `shutdown` flips. Binding the loopback port is the
/// only fallible setup; a door that cannot bind reports and the host keeps
/// running without it, because the cache is never required for correctness.
pub async fn serve(
    door: Arc<CacheDoor>,
    mut shutdown: watch::Receiver<bool>,
) -> Result<(), G5Error> {
    let listener = tokio::net::TcpListener::bind(door.config.bind)
        .await
        .map_err(|error| G5Error::Internal {
            message: format!("cache door cannot bind {}: {error}", door.config.bind),
        })?;
    tracing::info!(
        bind = %door.config.bind,
        store = %door.config.store_dir.display(),
        max_bytes = door.config.max_bytes,
        upstreams = ?door.config.upstreams,
        fleet = door.config.fleet.as_ref().map(|f| format!("gs://{}/{}", f.bucket, f.prefix)),
        "cache door listening"
    );
    axum::serve(listener, door.router())
        .with_graceful_shutdown(async move {
            while !*shutdown.borrow() {
                if shutdown.changed().await.is_err() {
                    break;
                }
            }
        })
        .await
        .map_err(|error| G5Error::Internal {
            message: format!("cache door server failed: {error}"),
        })
}

/// Reject anything that would let a request escape the store root: only the
/// characters content addresses and sccache keys are made of, no `..`.
pub(crate) fn safe_relative(rel: &str) -> Option<&str> {
    if rel.is_empty()
        || rel.starts_with('/')
        || rel.ends_with('/')
        || rel.contains("//")
        || rel
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        || !rel
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'/' || b == b'.' || b == b'_' || b == b'-')
    {
        return None;
    }
    Some(rel)
}

pub(crate) fn path_under(root: &Path, rel: &str) -> Option<PathBuf> {
    safe_relative(rel).map(|rel| root.join(rel))
}

/// Split an allowlist entry into its host and optional repository prefix.
fn split_entry(entry: &str) -> (&str, Option<&str>) {
    match entry.split_once('/') {
        Some((host, prefix)) if !prefix.is_empty() => (host, Some(prefix)),
        Some((host, _)) => (host, None),
        None => (entry, None),
    }
}

/// Open a seed object beneath a trusted template's rootfs through the
/// descriptor-based, symlink-refusing walk, off the runtime thread.
pub(crate) async fn open_seed(rootfs: PathBuf, rel: String) -> Option<(tokio::fs::File, u64)> {
    tokio::task::spawn_blocking(move || nofollow::open_nofollow(&rootfs, &rel, false).ok())
        .await
        .ok()
        .flatten()
        .map(|(file, meta)| (tokio::fs::File::from_std(file), meta.len()))
}

/// Stream a store entry. The body owns the entry's read lease until it
/// ends, so eviction leaves the file alone and accounted while a (possibly
/// slow) reader is still on it.
pub(crate) async fn entry_body(entry: Entry) -> std::io::Result<(axum::body::Body, u64)> {
    let file = tokio::fs::File::open(&entry.path).await?;
    Ok((
        held_body(tokio_util::io::ReaderStream::new(file), entry.lease),
        entry.len,
    ))
}

/// A body that keeps `hold` (a lease, an admission permit) alive until the
/// stream is dropped, which happens after the last byte is sent or when the
/// client goes away.
pub(crate) fn held_body<S, H>(inner: S, hold: H) -> axum::body::Body
where
    S: futures_util::Stream<Item = std::io::Result<bytes::Bytes>> + Send + Unpin + 'static,
    H: Send + Unpin + 'static,
{
    axum::body::Body::from_stream(Held { inner, _hold: hold })
}

struct Held<S, H> {
    inner: S,
    _hold: H,
}

impl<S, H> futures_util::Stream for Held<S, H>
where
    S: futures_util::Stream<Item = std::io::Result<bytes::Bytes>> + Unpin,
    H: Unpin,
{
    type Item = std::io::Result<bytes::Bytes>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.inner).poll_next(cx)
    }
}

// The facade tests drive wiremock upstreams through
// `DistributionClient::for_tests`, which the foundation crate exposes under
// its `test-helpers` feature; this crate's dev-dependency turns it on.
#[cfg(test)]
mod tests;
