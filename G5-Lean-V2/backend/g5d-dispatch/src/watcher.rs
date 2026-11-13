//! Native change observation. Linux registrations use pinned directory FDs and
//! nonrecursive watches: notify 6's recursive inotify traversal follows symlinks.
//! Event delivery rechecks the confined parent. Uncertain events disclose only
//! the subscribed root and request a rescan.

use crate::files::{FileError, FileService, RepositoryRoot};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs::File;
#[cfg(target_os = "linux")]
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

const QUEUE_CAPACITY: usize = 256;
const DEGRADED_RETRY_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Clone)]
struct Subscription {
    repo_id: String,
    path: String,
}

enum Command {
    Subscribe(Subscription, mpsc::SyncSender<Result<(), FileError>>),
    Event(notify::Result<Event>),
    #[cfg(test)]
    Inspect(mpsc::SyncSender<(usize, Vec<PathBuf>, bool)>),
    /// The registered directories by repository-relative path; native paths
    /// are descriptor links on Linux and say nothing about the directory.
    #[cfg(test)]
    Registered(mpsc::SyncSender<Vec<String>>),
}

struct Registration {
    repo_id: String,
    relative: String,
    native: PathBuf,
    held: File,
}

pub struct NativeWatcher {
    commands: mpsc::SyncSender<Command>,
    alive: Arc<AtomicBool>,
    subscriptions: Arc<Mutex<Vec<Subscription>>>,
}

pub struct NativeEvents {
    receiver: tokio::sync::mpsc::Receiver<Value>,
    overflow: Arc<AtomicBool>,
    subscriptions: Arc<Mutex<Vec<Subscription>>>,
    pending: VecDeque<Value>,
}

impl NativeWatcher {
    pub fn new(
        repositories: Vec<RepositoryRoot>,
        files: Arc<FileService>,
    ) -> anyhow::Result<(Self, NativeEvents)> {
        let (commands, incoming) = mpsc::sync_channel(QUEUE_CAPACITY);
        let (outgoing, receiver) = tokio::sync::mpsc::channel(QUEUE_CAPACITY);
        let alive = Arc::new(AtomicBool::new(true));
        let overflow = Arc::new(AtomicBool::new(false));
        let subscriptions = Arc::new(Mutex::new(Vec::new()));
        let thread_alive = alive.clone();
        let thread_overflow = overflow.clone();
        let thread_subscriptions = subscriptions.clone();
        let callbacks = commands.clone();
        let (ready, initialized) = mpsc::sync_channel(1);
        let worker = move || {
            let topology_lost = Arc::new(AtomicBool::new(false));
            let callback_topology_lost = topology_lost.clone();
            let callback_overflow = thread_overflow.clone();
            let watcher = notify::recommended_watcher(move |event| {
                if callbacks.try_send(Command::Event(event)).is_err() {
                    callback_overflow.store(true, Ordering::Release);
                    callback_topology_lost.store(true, Ordering::Release);
                }
            });
            let mut watcher = match watcher {
                Ok(watcher) => {
                    let _ = ready.send(Ok(()));
                    watcher
                }
                Err(error) => {
                    let _ = ready.send(Err(error.to_string()));
                    return;
                }
            };
            let roots: BTreeMap<_, _> = repositories
                .into_iter()
                .map(|repo| (repo.repo_id, repo.root))
                .collect();
            let mut registrations = BTreeMap::new();
            let mut topology = RefreshSchedule::default();
            #[cfg(test)]
            let mut refresh_count = 0;
            while thread_alive.load(Ordering::Acquire) {
                let now = Instant::now();
                if topology_lost.swap(false, Ordering::AcqRel) {
                    topology.mark(now);
                }
                if topology.take_due(now) {
                    let current = thread_subscriptions.lock().unwrap().clone();
                    #[cfg(test)]
                    {
                        refresh_count += 1;
                    }
                    let restored =
                        refresh(&mut watcher, &mut registrations, &roots, &files, &current).is_ok();
                    topology.finished(restored, Instant::now());
                    // Rescan after registration closes the observation gap for
                    // files created inside a new directory during coalescing.
                    for watch in &current {
                        emit(&outgoing, &thread_overflow, rescan(watch));
                    }
                }
                let command = match incoming.recv_timeout(topology.wait_duration(Instant::now())) {
                    Ok(command) => command,
                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(_) => break,
                };
                match command {
                    Command::Subscribe(subscription, response) => {
                        let result = (|| {
                            files.validate_watch(&subscription.repo_id, &subscription.path)?;
                            let mut current = thread_subscriptions.lock().unwrap();
                            if current.iter().any(|item: &Subscription| {
                                item.repo_id == subscription.repo_id
                                    && beneath(&subscription.path, &item.path)
                            }) {
                                return Ok(());
                            }
                            let previous = current.clone();
                            current.retain(|item| {
                                item.repo_id != subscription.repo_id
                                    || !beneath(&item.path, &subscription.path)
                            });
                            while current.len() >= 32 {
                                current.remove(0);
                            }
                            current.push(subscription.clone());
                            #[cfg(test)]
                            {
                                refresh_count += 1;
                            }
                            if let Err(error) =
                                refresh(&mut watcher, &mut registrations, &roots, &files, &current)
                            {
                                *current = previous;
                                let restored = refresh(
                                    &mut watcher,
                                    &mut registrations,
                                    &roots,
                                    &files,
                                    &current,
                                )
                                .is_ok();
                                topology.finished(restored, Instant::now());
                                return Err(error);
                            }
                            topology.finished(true, Instant::now());
                            Ok(())
                        })();
                        let _ = response.send(result);
                    }
                    Command::Event(Ok(event)) => {
                        if matches!(event.kind, EventKind::Access(_)) {
                            continue;
                        }
                        let current = thread_subscriptions.lock().unwrap().clone();
                        if event_changes_directories(&event, &registrations, &files, &current) {
                            topology.mark(Instant::now());
                        }
                        send_event_notifications(
                            &event,
                            &registrations,
                            &files,
                            &current,
                            &outgoing,
                            &thread_overflow,
                        );
                    }
                    Command::Event(Err(_)) => {
                        topology.mark(Instant::now());
                        for watch in thread_subscriptions.lock().unwrap().iter() {
                            emit(&outgoing, &thread_overflow, rescan(watch));
                        }
                    }
                    #[cfg(test)]
                    Command::Inspect(response) => {
                        let _ = response.send((
                            refresh_count,
                            registrations
                                .values()
                                .map(|registration| registration.native.clone())
                                .collect(),
                            topology.degraded,
                        ));
                    }
                    #[cfg(test)]
                    Command::Registered(response) => {
                        let _ = response.send(
                            registrations
                                .values()
                                .map(|registration| registration.relative.clone())
                                .collect(),
                        );
                    }
                }
            }
        };
        std::thread::Builder::new()
            .name("g5d-file-watch".into())
            .spawn(worker)?;
        initialized
            .recv()
            .map_err(|_| anyhow::anyhow!("file watcher failed to initialize"))?
            .map_err(anyhow::Error::msg)?;
        Ok((
            Self {
                commands,
                alive,
                subscriptions: subscriptions.clone(),
            },
            NativeEvents {
                receiver,
                overflow,
                subscriptions,
                pending: VecDeque::new(),
            },
        ))
    }

    pub fn watch(&mut self, repo_id: &str, path: &str) -> Result<(), FileError> {
        let (response, result) = mpsc::sync_channel(1);
        self.commands
            .send(Command::Subscribe(
                Subscription {
                    repo_id: repo_id.to_owned(),
                    path: path.to_owned(),
                },
                response,
            ))
            .map_err(|_| error("IO_ERROR", "native watcher stopped"))?;
        result
            .recv()
            .map_err(|_| error("IO_ERROR", "native watcher stopped"))?
    }

    /// Return one coarse notification for every active subscription. A
    /// downstream relay uses these after its own bounded broadcast queue
    /// reports lag, so clients rescan instead of silently accepting a gap.
    pub fn rescan_events(&self) -> Vec<Value> {
        self.subscriptions
            .lock()
            .unwrap()
            .iter()
            .map(rescan)
            .collect()
    }
}

impl Drop for NativeWatcher {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::Release);
    }
}

impl NativeEvents {
    pub async fn next(&mut self) -> Option<Value> {
        loop {
            if self.overflow.swap(false, Ordering::AcqRel) {
                self.pending
                    .extend(self.subscriptions.lock().unwrap().iter().map(rescan));
            }
            if let Some(value) = self.pending.pop_front() {
                return Some(value);
            }
            tokio::select! {
                event=self.receiver.recv()=>return event,
                _=tokio::time::sleep(Duration::from_millis(100))=>{}
            }
        }
    }
}

#[derive(Default)]
struct RefreshSchedule {
    due: Option<Instant>,
    degraded: bool,
}

impl RefreshSchedule {
    fn mark(&mut self, now: Instant) {
        if self.degraded {
            return;
        }
        // Fixed windows coalesce a burst without letting continuous activity
        // postpone registration of newly created directories indefinitely.
        self.due.get_or_insert(now + Duration::from_millis(25));
    }
    fn finished(&mut self, restored: bool, now: Instant) {
        self.degraded = !restored;
        self.due = if restored {
            None
        } else {
            let retry = now + DEGRADED_RETRY_INTERVAL;
            Some(self.due.map_or(retry, |pending| pending.min(retry)))
        };
    }
    fn take_due(&mut self, now: Instant) -> bool {
        if self.due.is_some_and(|due| now >= due) {
            self.due = None;
            true
        } else {
            false
        }
    }
    fn wait_duration(&self, now: Instant) -> Duration {
        self.due.map_or(Duration::from_millis(100), |due| {
            due.saturating_duration_since(now)
                .min(Duration::from_millis(100))
        })
    }
}

fn event_relative_path(path: &std::path::Path, registration: &Registration) -> Option<String> {
    let suffix = path.strip_prefix(&registration.native).ok()?.to_str()?;
    Some(if suffix.is_empty() {
        registration.relative.clone()
    } else if registration.relative.is_empty() {
        suffix.to_owned()
    } else {
        format!("{}/{suffix}", registration.relative)
    })
}

fn send_event_notifications(
    event: &Event,
    registrations: &BTreeMap<(String, String), Registration>,
    files: &FileService,
    subscriptions: &[Subscription],
    sender: &tokio::sync::mpsc::Sender<Value>,
    overflow: &AtomicBool,
) {
    if event.need_rescan() {
        for watch in subscriptions {
            emit(sender, overflow, rescan(watch));
        }
        return;
    }
    let mut delivered = BTreeSet::new();
    for path in &event.paths {
        for registration in registrations.values() {
            let Some(relative) = event_relative_path(path, registration) else {
                continue;
            };
            if !files.is_visible_path(&registration.repo_id, &relative) {
                continue;
            }
            for watch in subscriptions.iter().filter(|watch| {
                watch.repo_id == registration.repo_id
                    && (beneath(&relative, &watch.path) || beneath(&watch.path, &relative))
            }) {
                // A native file removal has a verified parent even though its
                // leaf no longer opens. Stale directory descriptors still
                // disclose only the subscribed root. Canonical paths can match several native
                // registrations on macOS, but each notification emits once per
                // repository, subscription, emitted path and notification kind.
                let concrete = beneath(&relative, &watch.path)
                    && files.watch_parent_matches(
                        &registration.repo_id,
                        &registration.relative,
                        &registration.held,
                    )
                    && (matches!(
                        event.kind,
                        EventKind::Remove(notify::event::RemoveKind::File)
                    ) || files
                        .validate_watch(&registration.repo_id, &relative)
                        .is_ok());
                let emitted_path = if concrete { &relative } else { &watch.path };
                let key = (
                    watch.repo_id.clone(),
                    watch.path.clone(),
                    emitted_path.clone(),
                    concrete,
                );
                if delivered.contains(&key) {
                    continue;
                }
                if delivered.len() >= QUEUE_CAPACITY {
                    overflow.store(true, Ordering::Release);
                    return;
                }
                delivered.insert(key);
                let notification = if concrete {
                    json!({"repo_id":watch.repo_id,"path":relative})
                } else {
                    rescan(watch)
                };
                emit(sender, overflow, notification);
            }
        }
    }
}

fn event_changes_directories(
    event: &Event,
    registrations: &BTreeMap<(String, String), Registration>,
    files: &FileService,
    subscriptions: &[Subscription],
) -> bool {
    if event.need_rescan() {
        return true;
    }
    // The watched directory set is pruned by the checkout's `.gitignore`
    // files, so any change to one of them (a rule added, dropped, the file
    // created, removed or renamed) changes which directories to watch.
    if !matches!(event.kind, EventKind::Access(_))
        && event.paths.iter().any(|path| {
            path.file_name()
                .is_some_and(|name| name == crate::file_tree::GITIGNORE)
        })
    {
        return true;
    }
    match event.kind {
        EventKind::Create(notify::event::CreateKind::File)
        | EventKind::Remove(notify::event::RemoveKind::File)
        | EventKind::Access(_) => return false,
        EventKind::Modify(notify::event::ModifyKind::Name(_) | notify::event::ModifyKind::Any)
        | EventKind::Create(_)
        | EventKind::Remove(_)
        | EventKind::Any => {}
        _ => return false,
    }
    if event.paths.is_empty() {
        return true;
    }
    for path in &event.paths {
        for registration in registrations.values() {
            let Some(relative) = event_relative_path(path, registration) else {
                continue;
            };
            if !files.is_visible_path(&registration.repo_id, &relative)
                || !subscriptions.iter().any(|watch| {
                    watch.repo_id == registration.repo_id
                        && (beneath(&relative, &watch.path) || beneath(&watch.path, &relative))
                })
            {
                continue;
            }
            // A directory that was deleted/renamed remains in the registration
            // map until reconciliation. A new directory is checked through the
            // same no-symlink descriptor traversal as ordinary file operations.
            if registrations.contains_key(&(registration.repo_id.clone(), relative.clone()))
                || files.watch_path_is_directory(&registration.repo_id, &relative)
            {
                return true;
            }
        }
    }
    false
}

fn refresh(
    watcher: &mut RecommendedWatcher,
    registrations: &mut BTreeMap<(String, String), Registration>,
    roots: &BTreeMap<String, PathBuf>,
    files: &FileService,
    subscriptions: &[Subscription],
) -> Result<(), FileError> {
    let mut directories = BTreeMap::new();
    for subscription in subscriptions {
        let found = match files.watch_directories(&subscription.repo_id, &subscription.path) {
            Ok(found) => found,
            Err(error) if error.code == "FILE_MISSING" => Vec::new(),
            Err(error) => return Err(error),
        };
        for (path, file) in found {
            directories.insert((subscription.repo_id.clone(), path), file);
        }
        if directories.len() > 10_000 {
            return Err(error(
                "WATCH_LIMIT_EXCEEDED",
                "maximum 10000 native directory watches",
            ));
        }
    }
    let stale: Vec<_> = registrations
        .iter()
        .filter_map(|(key, registered)| {
            let keep = directories.get(key).is_some_and(|file| {
                match (file.metadata(), registered.held.metadata()) {
                    (Ok(left), Ok(right)) => left.dev() == right.dev() && left.ino() == right.ino(),
                    _ => false,
                }
            });
            (!keep).then_some(key.clone())
        })
        .collect();
    for key in stale {
        if let Some(old) = registrations.remove(&key) {
            let _ = watcher.unwatch(&old.native);
        }
    }
    for ((repo_id, relative), held) in directories {
        if registrations.contains_key(&(repo_id.clone(), relative.clone())) {
            continue;
        }
        #[cfg(target_os = "linux")]
        let native = PathBuf::from(format!("/proc/self/fd/{}", held.as_raw_fd()));
        #[cfg(target_os = "macos")]
        let native = roots
            .get(&repo_id)
            .ok_or_else(|| error("PATH_FORBIDDEN", "unknown repository"))?
            .join(&relative)
            .canonicalize()
            .map_err(|_| error("IO_ERROR", "watch path is unavailable"))?;
        #[cfg(target_os = "linux")]
        let _ = roots;
        // macOS FSEvents may canonicalize its registered path; verify that path
        // still refers to the descriptor we obtained through confined traversal.
        let same = std::fs::metadata(&native)
            .and_then(|actual| {
                held.metadata().map(|expected| {
                    actual.dev() == expected.dev() && actual.ino() == expected.ino()
                })
            })
            .unwrap_or(false);
        if !same {
            return Err(error(
                "PATH_FORBIDDEN",
                "watch directory changed during registration",
            ));
        }
        watcher
            .watch(&native, RecursiveMode::NonRecursive)
            .map_err(|_| error("IO_ERROR", "native watch registration failed"))?;
        registrations.insert(
            (repo_id.clone(), relative.clone()),
            Registration {
                repo_id,
                relative,
                native,
                held,
            },
        );
    }
    Ok(())
}

fn beneath(path: &str, root: &str) -> bool {
    root.is_empty()
        || path == root
        || path
            .strip_prefix(root)
            .is_some_and(|suffix| suffix.starts_with('/'))
}
fn rescan(subscription: &Subscription) -> Value {
    json!({"repo_id":subscription.repo_id,"path":subscription.path,"rescan_required":true})
}
fn emit(sender: &tokio::sync::mpsc::Sender<Value>, overflow: &AtomicBool, value: Value) {
    if sender.try_send(value).is_err() {
        overflow.store(true, Ordering::Release);
    }
}
fn error(code: &'static str, message: &str) -> FileError {
    FileError {
        code,
        message: message.to_owned(),
        data: Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::ObjectFormat;
    use std::os::unix::fs::{symlink, PermissionsExt};

    fn fixture(root: &std::path::Path) -> (NativeWatcher, NativeEvents) {
        let repositories = vec![RepositoryRoot {
            repo_id: "repo".into(),
            root: root.to_owned(),
            object_format: ObjectFormat::Sha1,
        }];
        let files =
            Arc::new(FileService::new(repositories.clone(), vec![".g5/cache".into()]).unwrap());
        NativeWatcher::new(repositories, files).unwrap()
    }

    async fn observe(events: &mut NativeEvents, path: &str) -> Value {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let event = events.next().await.unwrap();
                if event["path"] == path {
                    return event;
                }
            }
        })
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn native_write_then_restore_and_permission_changes_are_observed() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("file"), "before").unwrap();
        let (mut watcher, mut events) = fixture(root.path());
        watcher.watch("repo", "file").unwrap();
        std::fs::write(root.path().join("file"), "temporary").unwrap();
        std::fs::write(root.path().join("file"), "before").unwrap();
        let observed = observe(&mut events, "file").await;
        assert_eq!(observed["repo_id"], "repo");
        assert_eq!(
            std::fs::read_to_string(root.path().join("file")).unwrap(),
            "before"
        );
        drop(watcher);
        let (mut watcher, mut events) = fixture(root.path());
        watcher.watch("repo", "file").unwrap();
        std::fs::set_permissions(
            root.path().join("file"),
            std::fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        observe(&mut events, "file").await;
    }

    #[tokio::test]
    async fn excluded_and_symlink_events_never_disclose_hidden_names() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join(".g5/cache")).unwrap();
        std::fs::create_dir_all(root.path().join(".git")).unwrap();
        std::fs::create_dir(root.path().join("inside")).unwrap();
        symlink(outside.path(), root.path().join("link")).unwrap();
        let (mut watcher, mut events) = fixture(root.path());
        watcher.watch("repo", "").unwrap();
        assert_eq!(
            watcher.watch("repo", "link").unwrap_err().code,
            "PATH_FORBIDDEN"
        );
        std::fs::write(root.path().join(".git/hidden-git-name"), "private").unwrap();
        std::fs::write(root.path().join(".g5/cache/hidden-runtime-name"), "private").unwrap();
        std::fs::write(outside.path().join("hidden-link-name"), "private").unwrap();
        std::fs::rename(root.path().join("inside"), outside.path().join("moved")).unwrap();
        std::fs::write(
            outside.path().join("moved/hidden-stale-descriptor-name"),
            "private",
        )
        .unwrap();
        let deadline = tokio::time::Instant::now() + Duration::from_millis(700);
        while let Ok(Some(event)) = tokio::time::timeout_at(deadline, events.next()).await {
            assert!(
                !event["path"].as_str().unwrap().contains("hidden"),
                "{event}"
            );
        }
    }

    #[tokio::test]
    async fn bounded_delivery_overflow_requires_rescan() {
        let (sender, receiver) = tokio::sync::mpsc::channel(1);
        let overflow = Arc::new(AtomicBool::new(false));
        emit(&sender, &overflow, json!({"path":"a"}));
        emit(&sender, &overflow, json!({"path":"b"}));
        let subscriptions = Arc::new(Mutex::new(vec![Subscription {
            repo_id: "repo".into(),
            path: "tracked".into(),
        }]));
        let mut events = NativeEvents {
            receiver,
            overflow,
            subscriptions,
            pending: VecDeque::new(),
        };
        assert_eq!(
            events.next().await.unwrap(),
            json!({"repo_id":"repo","path":"tracked","rescan_required":true})
        );
    }

    #[test]
    fn downstream_lag_rescans_every_subscription() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("one"), "one").unwrap();
        std::fs::write(root.path().join("two"), "two").unwrap();
        let (mut watcher, _events) = fixture(root.path());
        watcher.watch("repo", "one").unwrap();
        watcher.watch("repo", "two").unwrap();
        assert_eq!(
            watcher.rescan_events(),
            vec![
                json!({"repo_id":"repo","path":"one","rescan_required":true}),
                json!({"repo_id":"repo","path":"two","rescan_required":true}),
            ]
        );
    }

    #[test]
    fn broad_subscription_covers_descendants_without_consuming_slots() {
        let root = tempfile::tempdir().unwrap();
        for index in 0..40 {
            std::fs::write(root.path().join(format!("file-{index}")), "content").unwrap();
        }
        let (mut watcher, _events) = fixture(root.path());
        watcher.watch("repo", "").unwrap();
        for index in 0..40 {
            watcher.watch("repo", &format!("file-{index}")).unwrap();
        }
        assert_eq!(
            watcher.rescan_events(),
            vec![json!({"repo_id":"repo","path":"","rescan_required":true})]
        );
    }

    #[test]
    fn narrow_subscriptions_rotate_at_the_capacity_limit() {
        let root = tempfile::tempdir().unwrap();
        for index in 0..40 {
            std::fs::write(root.path().join(format!("file-{index}")), "content").unwrap();
        }
        let (mut watcher, _events) = fixture(root.path());
        for index in 0..40 {
            watcher.watch("repo", &format!("file-{index}")).unwrap();
        }
        let rescans = watcher.rescan_events();
        assert_eq!(rescans.len(), 32);
        assert!(!rescans.iter().any(|event| event["path"] == "file-0"));
        assert!(rescans.iter().any(|event| event["path"] == "file-39"));
    }
    #[test]
    fn a_gitignore_change_rebuilds_the_watched_directories() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join(".gitignore"), "generated/\n").unwrap();
        std::fs::create_dir(root.path().join("generated")).unwrap();
        std::fs::create_dir(root.path().join("src")).unwrap();
        let (mut watcher, _events) = fixture(root.path());
        watcher.watch("repo", "").unwrap();
        let inspect = || {
            let (reply, result) = mpsc::sync_channel(1);
            watcher.commands.send(Command::Inspect(reply)).unwrap();
            result.recv().unwrap()
        };
        let registered = || {
            let (reply, result) = mpsc::sync_channel(1);
            watcher.commands.send(Command::Registered(reply)).unwrap();
            let mut paths: Vec<String> = result.recv().unwrap();
            paths.sort();
            paths
        };
        let (before, paths, _) = inspect();
        let native = &paths[0];
        assert_eq!(
            registered(),
            ["", "src"],
            "the ignored directory is not watched"
        );
        // Dropping the rule is an ordinary file modification, but it changes
        // which directories to watch.
        std::fs::write(root.path().join(".gitignore"), "").unwrap();
        watcher
            .commands
            .send(Command::Event(Ok(Event::new(EventKind::Modify(
                notify::event::ModifyKind::Data(notify::event::DataChange::Content),
            ))
            .add_path(native.join(".gitignore")))))
            .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            let (count, _, _) = inspect();
            let paths = registered();
            if count > before && paths == ["", "generated", "src"] {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the .gitignore change must re-register directories: {paths:?}"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn file_only_native_event_burst_does_not_walk_the_directory_tree_again() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("file"), "unchanged").unwrap();
        let (mut watcher, _events) = fixture(root.path());
        watcher.watch("repo", "").unwrap();
        let inspect = || {
            let (reply, result) = mpsc::sync_channel(1);
            watcher.commands.send(Command::Inspect(reply)).unwrap();
            result.recv().unwrap()
        };
        let (before, paths, _) = inspect();
        let native = &paths[0];
        // Drive the real native-event worker with deterministic backend events,
        // then a queue barrier. No wall-clock performance threshold is involved.
        for _ in 0..100 {
            for event in [
                Event::new(EventKind::Create(notify::event::CreateKind::File))
                    .add_path(native.join("file")),
                Event::new(EventKind::Remove(notify::event::RemoveKind::File))
                    .add_path(native.join("gone")),
                Event::new(EventKind::Modify(notify::event::ModifyKind::Name(
                    notify::event::RenameMode::Both,
                )))
                .add_path(native.join("gone"))
                .add_path(native.join("file")),
            ] {
                watcher.commands.send(Command::Event(Ok(event))).unwrap();
            }
        }
        assert_eq!(
            inspect().0,
            before,
            "ordinary file events must not rebuild directory registrations"
        );
    }

    #[test]
    fn topology_refresh_coalesces_bursts_without_starving_new_directories() {
        let now = Instant::now();
        let mut schedule = RefreshSchedule::default();
        for offset in 0..100 {
            schedule.mark(now + Duration::from_micros(offset));
        }
        assert!(!schedule.take_due(now + Duration::from_millis(24)));
        assert!(schedule.take_due(now + Duration::from_millis(25)));
        assert!(!schedule.take_due(now + Duration::from_millis(25)));
        schedule.mark(now + Duration::from_millis(26));
        schedule.mark(now + Duration::from_millis(50));
        assert!(schedule.take_due(now + Duration::from_millis(51)));
    }
    fn inspect_watcher(watcher: &NativeWatcher) -> (usize, Vec<PathBuf>, bool) {
        let (reply, result) = mpsc::sync_channel(1);
        watcher.commands.send(Command::Inspect(reply)).unwrap();
        result.recv().unwrap()
    }

    async fn await_degraded(watcher: &NativeWatcher, expected: bool) -> usize {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let state = inspect_watcher(watcher);
                if state.2 == expected {
                    return state.0;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn excessive_native_depth_keeps_rescanning_until_watch_coverage_recovers() {
        let root = tempfile::tempdir().unwrap();
        let (mut watcher, mut events) = fixture(root.path());
        watcher.watch("repo", "").unwrap();
        let mut deepest = root.path().to_owned();
        for depth in 0..65 {
            deepest.push(format!("d{depth}"));
            std::fs::create_dir(&deepest).unwrap();
        }
        // Feed the real root path through the callback handoff. macOS may
        // not start emitting immediately after registration when several
        // test watchers initialize in parallel.
        watcher
            .commands
            .send(Command::Event(Ok(Event::new(EventKind::Create(
                notify::event::CreateKind::Folder,
            ))
            .add_path(root.path().canonicalize().unwrap().join("d0")))))
            .unwrap();
        // This waits for a real confined traversal failure, not a simulated
        // oversized registration counter. The old root watch remains live.
        let first_failure_count = await_degraded(&watcher, true).await;
        while events.receiver.try_recv().is_ok() {}
        events.pending.clear();
        events.overflow.store(false, Ordering::Release);
        std::fs::write(deepest.join("unregistered-native-file"), "new content").unwrap();
        tokio::time::timeout(Duration::from_secs(4), async {
            loop {
                let event = events.next().await.unwrap();
                let state = inspect_watcher(&watcher);
                if event["path"] == ""
                    && event["rescan_required"] == true
                    && state.2
                    && state.0 > first_failure_count
                {
                    break;
                }
            }
        })
        .await
        .expect("degraded coverage must generate a fresh periodic rescan");
        std::fs::remove_dir_all(root.path().join("d0")).unwrap();
        await_degraded(&watcher, false).await;
        std::fs::write(root.path().join("after-recovery"), "visible").unwrap();
        observe(&mut events, "after-recovery").await;
        assert!(!inspect_watcher(&watcher).2);
    }

    #[test]
    fn degraded_refresh_retries_are_bounded_and_stop_after_recovery() {
        let now = Instant::now();
        let mut schedule = RefreshSchedule::default();
        schedule.finished(false, now);
        assert!(schedule.degraded);
        for offset in 0..100 {
            schedule.mark(now + Duration::from_millis(offset));
        }
        assert!(!schedule.take_due(now + DEGRADED_RETRY_INTERVAL - Duration::from_millis(1)));
        assert!(schedule.take_due(now + DEGRADED_RETRY_INTERVAL));
        schedule.finished(false, now + DEGRADED_RETRY_INTERVAL);
        assert!(!schedule.take_due(now + DEGRADED_RETRY_INTERVAL + Duration::from_millis(25)));
        assert!(schedule.take_due(now + DEGRADED_RETRY_INTERVAL * 2));
        schedule.finished(true, now + DEGRADED_RETRY_INTERVAL * 2);
        assert!(!schedule.degraded);
        assert!(!schedule.take_due(now + Duration::from_secs(100)));
    }

    #[test]
    fn canonical_ancestor_registrations_emit_once_per_subscription_and_native_notification() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("one/two")).unwrap();
        std::fs::write(root.path().join("one/two/file"), "content").unwrap();
        let canonical = root.path().canonicalize().unwrap();
        let files = FileService::new(
            vec![RepositoryRoot {
                repo_id: "repo".into(),
                root: root.path().to_owned(),
                object_format: ObjectFormat::Sha1,
            }],
            Vec::new(),
        )
        .unwrap();
        let registrations = files
            .watch_directories("repo", "")
            .unwrap()
            .into_iter()
            .map(|(relative, held)| {
                (
                    ("repo".into(), relative.clone()),
                    Registration {
                        repo_id: "repo".into(),
                        native: canonical.join(&relative),
                        relative,
                        held,
                    },
                )
            })
            .collect();
        let subscriptions = vec![
            Subscription {
                repo_id: "repo".into(),
                path: String::new(),
            },
            Subscription {
                repo_id: "repo".into(),
                path: "one".into(),
            },
        ];
        let (sender, mut receiver) = tokio::sync::mpsc::channel(16);
        let overflow = AtomicBool::new(false);
        let path = canonical.join("one/two/file");
        let event = Event::new(EventKind::Modify(notify::event::ModifyKind::Data(
            notify::event::DataChange::Any,
        )))
        .add_path(path.clone())
        .add_path(path);
        send_event_notifications(
            &event,
            &registrations,
            &files,
            &subscriptions,
            &sender,
            &overflow,
        );
        let mut observed = Vec::new();
        while let Ok(event) = receiver.try_recv() {
            observed.push(event);
        }
        assert_eq!(
            observed.len(),
            2,
            "each subscription gets one event despite repeated paths and matching ancestors"
        );
        assert!(observed
            .iter()
            .all(|event| event == &json!({"repo_id":"repo","path":"one/two/file"})));
        assert!(!overflow.load(Ordering::Acquire));

        std::fs::remove_file(root.path().join("one/two/file")).unwrap();
        let removed = Event::new(EventKind::Remove(notify::event::RemoveKind::File))
            .add_path(canonical.join("one/two/file"));
        send_event_notifications(
            &removed,
            &registrations,
            &files,
            &subscriptions,
            &sender,
            &overflow,
        );
        let mut deleted = Vec::new();
        while let Ok(event) = receiver.try_recv() {
            deleted.push(event);
        }
        assert_eq!(deleted.len(), 2);
        assert!(deleted
            .iter()
            .all(|event| { event == &json!({"repo_id":"repo","path":"one/two/file"}) }));
    }
}
