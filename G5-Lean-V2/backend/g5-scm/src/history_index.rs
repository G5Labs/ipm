//! Rebuildable ordering only; callers still authorize every read and read objects.
use g5_contracts::G5Error;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

const MAX_INDEXES: usize = 16;
const MAX_COMMIT_IDS: usize = 2_000_000;
const IDLE_TTL: Duration = Duration::from_secs(600);

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct HistoryKey {
    pub repository: PathBuf,
    pub scope: String,
}
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct CommitOrder {
    ids: Box<[git2::Oid]>,
    by_id: Box<[usize]>,
}
impl CommitOrder {
    fn new(ids: Vec<git2::Oid>) -> Self {
        // Oversized cuts are not retained; avoid an extra full lookup index.
        let mut by_id: Vec<usize> = if ids.len() <= MAX_COMMIT_IDS {
            (0..ids.len()).collect()
        } else {
            vec![]
        };
        by_id.sort_unstable_by_key(|position| ids[*position]);
        Self {
            ids: ids.into_boxed_slice(),
            by_id: by_id.into_boxed_slice(),
        }
    }
    pub(super) fn position(&self, id: git2::Oid) -> Option<usize> {
        if self.by_id.is_empty() {
            return self.ids.iter().position(|candidate| *candidate == id);
        }
        self.by_id
            .binary_search_by_key(&id, |position| self.ids[*position])
            .ok()
            .map(|position| self.by_id[position])
    }
    pub(super) fn ids(&self) -> &[git2::Oid] {
        &self.ids
    }
}
pub(crate) type HistoryIndex = Arc<CommitOrder>;
type Slot = Arc<Mutex<Option<HistoryIndex>>>;
struct CachedIndex {
    key: HistoryKey,
    accessed: Instant,
    slot: Slot,
    ids: usize,
}
#[derive(Default)]
struct HistoryCache(VecDeque<CachedIndex>);
impl HistoryCache {
    fn slot(&mut self, key: HistoryKey) -> Slot {
        self.0.retain(|entry| entry.accessed.elapsed() < IDLE_TTL);
        let entry = self
            .0
            .iter()
            .position(|entry| entry.key == key)
            .and_then(|position| self.0.remove(position));
        let mut entry = entry.unwrap_or_else(|| CachedIndex {
            key,
            accessed: Instant::now(),
            slot: Arc::new(Mutex::new(None)),
            ids: 0,
        });
        entry.accessed = Instant::now();
        let slot = entry.slot.clone();
        self.0.push_back(entry);
        while self.0.len() > MAX_INDEXES {
            self.0.pop_front();
        }
        slot
    }

    fn finish(&mut self, slot: &Slot, count: usize) {
        if count > MAX_COMMIT_IDS {
            self.0.retain(|entry| !Arc::ptr_eq(&entry.slot, slot));
            return;
        }
        // Account under the global lock without ever taking a build-slot lock.
        if let Some(entry) = self
            .0
            .iter_mut()
            .find(|entry| Arc::ptr_eq(&entry.slot, slot))
        {
            entry.ids = count;
        }
        let mut ids: usize = self.0.iter().map(|entry| entry.ids).sum();
        while ids > MAX_COMMIT_IDS {
            let Some(entry) = self.0.pop_front() else {
                break;
            };
            ids -= entry.ids;
        }
    }
}

fn cached_index(
    cache: &Mutex<HistoryCache>,
    key: HistoryKey,
    build: impl FnOnce() -> Result<Vec<git2::Oid>, G5Error>,
) -> Result<HistoryIndex, G5Error> {
    let slot = cache.lock().unwrap_or_else(|e| e.into_inner()).slot(key);
    let index = {
        let mut value = slot.lock().unwrap_or_else(|e| e.into_inner());
        if value.is_none() {
            *value = Some(Arc::new(CommitOrder::new(build()?)));
        }
        value.as_ref().unwrap().clone()
    };
    cache
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .finish(&slot, index.ids.len());
    Ok(index)
}

pub(crate) fn history_index(
    key: HistoryKey,
    build: impl FnOnce() -> Result<Vec<git2::Oid>, G5Error>,
) -> Result<HistoryIndex, G5Error> {
    static CACHE: OnceLock<Mutex<HistoryCache>> = OnceLock::new();
    cached_index(CACHE.get_or_init(Default::default), key, build)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn key(scope: &str) -> HistoryKey {
        HistoryKey {
            repository: PathBuf::from("repo"),
            scope: scope.into(),
        }
    }
    #[test]
    fn indexes_are_shared_per_scope_and_rebuilt_after_eviction() {
        let cache = Mutex::new(HistoryCache::default());
        let first = cached_index(&cache, key("first"), || Ok(vec![git2::Oid::ZERO_SHA1])).unwrap();
        let same = cached_index(&cache, key("first"), || panic!("repeated graph walk")).unwrap();
        assert!(Arc::ptr_eq(&first, &same));
        for index in 0..MAX_INDEXES {
            cached_index(&cache, key(&index.to_string()), || Ok(vec![])).unwrap();
        }
        let rebuilt = cached_index(&cache, key("first"), || Ok(first.ids.to_vec())).unwrap();
        assert_eq!(first, rebuilt);
        assert!(!Arc::ptr_eq(&first, &rebuilt));
        assert_eq!(cache.lock().unwrap().0.len(), MAX_INDEXES);
    }
    #[test]
    fn oversized_indexes_are_readable_but_not_retained() {
        let cache = Mutex::new(HistoryCache::default());
        let small = cached_index(&cache, key("small"), || Ok(vec![git2::Oid::ZERO_SHA1])).unwrap();
        let index = cached_index(&cache, key("large"), || {
            Ok(vec![git2::Oid::ZERO_SHA1; MAX_COMMIT_IDS + 1])
        })
        .unwrap();
        assert_eq!(index.ids.len(), MAX_COMMIT_IDS + 1);
        assert_eq!(cache.lock().unwrap().0.len(), 1);
        let retained = cached_index(&cache, key("small"), || {
            panic!("oversized read evicted another cut")
        })
        .unwrap();
        assert!(Arc::ptr_eq(&small, &retained));
    }
    #[test]
    fn idle_indexes_expire_and_repository_identity_is_part_of_the_key() {
        let cache = Mutex::new(HistoryCache::default());
        let first = cached_index(&cache, key("cut"), || Ok(vec![git2::Oid::ZERO_SHA1])).unwrap();
        let mut another = key("cut");
        another.repository = PathBuf::from("other-repo");
        let other = cached_index(&cache, another, || Ok(vec![])).unwrap();
        assert!(other.ids.is_empty());
        cache.lock().unwrap().0[0].accessed -= IDLE_TTL;
        let rebuilt = cached_index(&cache, key("cut"), || Ok(first.ids.to_vec())).unwrap();
        assert_eq!(first, rebuilt);
        assert!(!Arc::ptr_eq(&first, &rebuilt));
    }
    #[test]
    fn cold_requests_share_the_build_and_failed_builds_can_retry() {
        let cache = Mutex::new(HistoryCache::default());
        assert!(cached_index(&cache, key("retry"), || Err(G5Error::StaleHistory)).is_err());
        let builds = std::sync::atomic::AtomicUsize::new(0);
        std::thread::scope(|threads| {
            for _ in 0..8 {
                let cache = &cache;
                let builds = &builds;
                threads.spawn(move || {
                    let value = cached_index(cache, key("retry"), || {
                        builds.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        Ok(vec![git2::Oid::ZERO_SHA1])
                    })
                    .unwrap();
                    assert_eq!(value.ids.len(), 1);
                });
            }
        });
        assert_eq!(builds.load(std::sync::atomic::Ordering::Relaxed), 1);
    }
}
