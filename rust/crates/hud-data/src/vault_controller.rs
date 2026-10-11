//! Nonblocking, single-flight read-only Vault refresh/search adapter.
//! All timing is monotonic and injected by the application via `poll`.
use crate::{clean_line, Limits, Snapshot};
use std::{
    io,
    path::{Component, Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

pub const REFRESH_INTERVAL: Duration = Duration::from_secs(5);
pub const MAX_QUERY_CHARS: usize = 256;
/// Source seam for deterministic tests. Production reads only the explicit root.
pub trait SnapshotSource: Send + Sync + 'static {
    fn read(
        &self,
        root: &Path,
        query: Option<&str>,
        limits: Limits,
        cancelled: &AtomicBool,
    ) -> io::Result<Snapshot>;
}
struct FileSource {
    #[cfg(unix)]
    cache: std::sync::Mutex<crate::ScanCache>,
    clock: Arc<dyn crate::CalendarClock>,
}
impl SnapshotSource for FileSource {
    fn read(
        &self,
        root: &Path,
        query: Option<&str>,
        limits: Limits,
        cancelled: &AtomicBool,
    ) -> io::Result<Snapshot> {
        #[cfg(unix)]
        {
            crate::scan_cached(
                root,
                query,
                limits,
                cancelled,
                &mut *self
                    .cache
                    .lock()
                    .map_err(|_| io::Error::other("Vault cache lock poisoned"))?,
                self.clock.moment()?,
            )
        }
        #[cfg(not(unix))]
        {
            crate::scan_cancellable(root, query, limits, cancelled)
        }
    }
}
struct Completion {
    generation: u64,
    query: Option<String>,
    result: io::Result<Snapshot>,
}
struct Worker {
    cancel: Arc<AtomicBool>,
    receiver: mpsc::Receiver<Completion>,
    handle: JoinHandle<()>,
}
/// Owns at most one scanner thread and one coalesced pending request. A cancelled
/// request stops at scanner checkpoints; generation checks discard its result.
/// `Drop` never joins a potentially slow filesystem read. Its sole worker is
/// cancelled and detached; no further worker can be started by that controller.
pub struct VaultController {
    root: PathBuf,
    limits: Limits,
    source: Arc<dyn SnapshotSource>,
    worker: Option<Worker>,
    pending: bool,
    generation: u64,
    query: Option<String>,
    completed_query: Option<String>,
    snapshot: Arc<Snapshot>,
    error: Option<String>,
    last_started: Option<Duration>,
    last_success: Option<Duration>,
    version: u64,
    refresh_interval: Duration,
}
impl VaultController {
    pub fn new(root: PathBuf, limits: Limits) -> io::Result<Self> {
        Self::with_clock(root, limits, Arc::new(crate::SystemLocalClock))
    }
    pub fn with_clock(
        root: PathBuf,
        limits: Limits,
        clock: Arc<dyn crate::CalendarClock>,
    ) -> io::Result<Self> {
        Self::with_source(
            root,
            limits,
            Arc::new(FileSource {
                #[cfg(unix)]
                cache: std::sync::Mutex::new(crate::ScanCache::default()),
                clock,
            }),
        )
    }
    pub fn with_source(
        root: PathBuf,
        limits: Limits,
        source: Arc<dyn SnapshotSource>,
    ) -> io::Result<Self> {
        if root.as_os_str().is_empty()
            || root.components().any(|c| matches!(c, Component::ParentDir))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "explicit Vault root required; parent traversal rejected",
            ));
        }
        if limits.note_bytes == 0
            || limits.files == 0
            || limits.depth == 0
            || limits.total_bytes == 0
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "scan limits must be positive",
            ));
        }
        // Resolve relative roots once; a later working-directory change must not
        // silently select a different Vault.
        let root = if root.is_absolute() {
            root
        } else {
            std::env::current_dir()?.join(root)
        };
        Ok(Self {
            root,
            limits,
            source,
            worker: None,
            pending: true,
            generation: 1,
            query: None,
            completed_query: None,
            snapshot: Arc::new(Snapshot::default()),
            error: None,
            last_started: None,
            last_success: None,
            version: 0,
            refresh_interval: REFRESH_INTERVAL,
        })
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }
    pub fn snapshot_shared(&self) -> Arc<Snapshot> {
        Arc::clone(&self.snapshot)
    }
    pub fn query(&self) -> Option<&str> {
        self.query.as_deref()
    }
    /// Query associated with the last successful snapshot. Until it matches
    /// `query`, the UI must label old hits as previous results, not current hits.
    pub fn completed_query(&self) -> Option<&str> {
        self.completed_query.as_deref()
    }
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
    pub fn is_busy(&self) -> bool {
        self.pending || self.worker.is_some()
    }
    pub fn version(&self) -> u64 {
        self.version
    }
    pub fn last_success(&self) -> Option<Duration> {
        self.last_success
    }
    pub fn refresh(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.pending = true;
        if let Some(worker) = &self.worker {
            worker.cancel.store(true, Ordering::Relaxed);
        }
    }
    /// Literal case-insensitive search, bounded and sanitized for terminal
    /// display. Empty/whitespace input clears search. Rapid edits coalesce.
    pub fn search(&mut self, query: &str) {
        let query: String = clean_line(query)
            .trim()
            .chars()
            .take(MAX_QUERY_CHARS)
            .collect();
        let query = (!query.is_empty()).then_some(query);
        if query != self.query {
            self.query = query;
            self.refresh();
        }
    }
    /// Invalidate an in-flight request without scheduling another scan. Useful
    /// when leaving a view; a future `refresh` resumes it. Last success survives.
    pub fn cancel(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.pending = false;
        if let Some(worker) = &self.worker {
            worker.cancel.store(true, Ordering::Relaxed);
        }
        self.last_started = None;
    }
    /// Call once per event-loop tick. Never waits for filesystem work. Returns
    /// true only if an accepted snapshot/error changed; periodic refresh uses 5s
    /// from the previous completion (or start if the source is still busy).
    pub fn refresh_interval(&self) -> Duration {
        self.refresh_interval
    }
    pub fn set_refresh_interval(&mut self, seconds: f64) -> io::Result<()> {
        if !seconds.is_finite() || !(2.0..=300.0).contains(&seconds) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        self.refresh_interval = Duration::from_secs_f64(seconds);
        Ok(())
    }
    pub fn poll(&mut self, now: Duration) -> bool {
        let mut changed = false;
        if self
            .worker
            .as_ref()
            .is_some_and(|worker| worker.handle.is_finished())
        {
            let worker = self.worker.take().expect("finished worker");
            let completion = worker.receiver.try_recv();
            let _ = worker.handle.join(); // is_finished established above
            if let Ok(completion) = completion {
                if completion.generation == self.generation {
                    self.last_started = Some(now);
                    match completion.result {
                        Ok(snapshot) => {
                            self.snapshot = Arc::new(snapshot);
                            self.completed_query = completion.query;
                            self.error = None;
                            self.last_success = Some(now);
                        }
                        Err(error) => {
                            self.error =
                                Some(clean_line(&error.to_string()).chars().take(512).collect());
                        }
                    }
                    self.version = self.version.wrapping_add(1);
                    changed = true;
                }
            }
        }
        if self.worker.is_none()
            && !self.pending
            && self
                .last_started
                .is_some_and(|last| now.saturating_sub(last) >= self.refresh_interval)
        {
            self.pending = true;
        }
        if self.worker.is_none() && self.pending {
            let root = self.root.clone();
            let query = self.query.clone();
            let generation = self.generation;
            let limits = self.limits;
            let source = Arc::clone(&self.source);
            let cancel = Arc::new(AtomicBool::new(false));
            let token = Arc::clone(&cancel);
            let (sender, receiver) = mpsc::sync_channel(1);
            match thread::Builder::new()
                .name("hud-vault".into())
                .spawn(move || {
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        source.read(&root, query.as_deref(), limits, &token)
                    }))
                    .unwrap_or_else(|_| Err(io::Error::other("Vault source panicked")));
                    let _ = sender.send(Completion {
                        generation,
                        query,
                        result,
                    });
                }) {
                Ok(handle) => {
                    self.worker = Some(Worker {
                        cancel,
                        receiver,
                        handle,
                    });
                    self.pending = false;
                    self.last_started = Some(now);
                }
                Err(error) => {
                    self.pending = false;
                    self.last_started = Some(now);
                    self.error = Some(clean_line(&error.to_string()));
                    self.version = self.version.wrapping_add(1);
                    changed = true;
                }
            }
        }
        changed
    }
}
impl Drop for VaultController {
    fn drop(&mut self) {
        if let Some(worker) = &self.worker {
            worker.cancel.store(true, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    struct Request {
        query: Option<String>,
        reply: mpsc::Sender<io::Result<Snapshot>>,
        token: Arc<AtomicBool>,
    }
    struct Gate {
        calls: mpsc::Sender<Request>,
    }
    impl SnapshotSource for Gate {
        fn read(
            &self,
            _root: &Path,
            query: Option<&str>,
            _limits: Limits,
            cancelled: &AtomicBool,
        ) -> io::Result<Snapshot> {
            let (reply, receiver) = mpsc::channel();
            let token = Arc::new(AtomicBool::new(cancelled.load(Ordering::Relaxed)));
            self.calls
                .send(Request {
                    query: query.map(str::to_owned),
                    reply,
                    token: Arc::clone(&token),
                })
                .unwrap();
            // Deliberately noncooperative source: proves bounded single-flight
            // even if a filesystem call has not yet reached a cancellation point.
            let result = receiver.recv().unwrap();
            token.store(cancelled.load(Ordering::Relaxed), Ordering::Relaxed);
            result
        }
    }
    fn controller() -> (VaultController, mpsc::Receiver<Request>) {
        let (sender, receiver) = mpsc::channel();
        let controller = VaultController::with_source(
            PathBuf::from("/fixture-only"),
            Limits::default(),
            Arc::new(Gate { calls: sender }),
        )
        .unwrap();
        (controller, receiver)
    }
    fn snapshot(notes: usize) -> Snapshot {
        Snapshot {
            notes,
            ..Snapshot::default()
        }
    }
    fn settle(controller: &mut VaultController, now: u64) {
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while controller.is_busy() {
            controller.poll(Duration::from_secs(now));
            assert!(
                std::time::Instant::now() < deadline,
                "worker did not settle"
            );
            thread::yield_now();
        }
    }
    #[test]
    fn periodic_refresh_sees_added_deleted_and_changed_fixture_without_writes() {
        use std::fs;
        let root = std::env::temp_dir().join(format!("hud-controller-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir(&root).unwrap();
        fs::write(root.join("a.md"), "- [ ] café 📅 2026-10-11").unwrap();
        let mut c = VaultController::new(root.clone(), Limits::default()).unwrap();
        settle(&mut c, 0);
        assert_eq!(c.snapshot().tasks[0].text, "café");
        let version = c.version();
        fs::remove_file(root.join("a.md")).unwrap();
        fs::write(root.join("novo.md"), "- [ ] 日本語 📅 2026-10-12").unwrap();
        c.poll(Duration::from_secs(4));
        assert!(!c.is_busy());
        assert_eq!(c.version(), version);
        c.poll(Duration::from_secs(5));
        settle(&mut c, 5);
        assert_eq!(c.snapshot().notes, 1);
        assert_eq!(c.snapshot().tasks[0].text, "日本語");
        c.search("日本語");
        settle(&mut c, 5);
        assert_eq!(c.snapshot().hits[0].text, "- [ ] 日本語 📅 2026-10-12");
        assert_eq!(
            fs::read_to_string(root.join("novo.md")).unwrap(),
            "- [ ] 日本語 📅 2026-10-12"
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn slow_query_is_nonblocking_and_rapid_search_coalesces_without_stale_publish() {
        let (mut c, requests) = controller();
        c.search("first");
        c.poll(Duration::ZERO);
        let first = requests.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(first.query.as_deref(), Some("first"));
        for n in 0..1000 {
            c.search(&format!("query {n}"));
            c.poll(Duration::from_secs(1));
        }
        assert!(matches!(
            requests.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));
        first.reply.send(Ok(snapshot(99))).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        let last = loop {
            c.poll(Duration::from_secs(1));
            if let Ok(request) = requests.try_recv() {
                break request;
            }
            assert!(std::time::Instant::now() < deadline);
            thread::yield_now();
        };
        assert!(first.token.load(Ordering::Relaxed));
        assert_eq!(c.snapshot().notes, 0);
        assert_eq!(c.version(), 0);
        assert_eq!(last.query.as_deref(), Some("query 999"));
        last.reply.send(Ok(snapshot(7))).unwrap();
        settle(&mut c, 1);
        assert_eq!(c.snapshot().notes, 7);
        assert_eq!(c.completed_query(), Some("query 999"));
        assert_eq!(c.version(), 1);
    }
    #[test]
    fn failure_retains_success_and_recovers_on_next_refresh() {
        let (mut c, requests) = controller();
        c.poll(Duration::ZERO);
        requests
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .reply
            .send(Ok(snapshot(12)))
            .unwrap();
        settle(&mut c, 0);
        let previous = c.snapshot_shared();
        c.search("novo");
        c.poll(Duration::from_secs(1));
        requests
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .reply
            .send(Err(io::Error::other("bad\x1b]52;c;secret\x07\u{202e}")))
            .unwrap();
        settle(&mut c, 1);
        assert!(Arc::ptr_eq(&previous, &c.snapshot_shared()));
        assert_eq!(c.error(), Some("bad"));
        assert_eq!(c.completed_query(), None);
        assert_eq!(c.query(), Some("novo"));
        assert_eq!(c.last_success(), Some(Duration::ZERO));
        c.poll(Duration::from_secs(6));
        requests
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .reply
            .send(Ok(snapshot(13)))
            .unwrap();
        settle(&mut c, 6);
        assert_eq!(c.error(), None);
        assert_eq!(c.completed_query(), Some("novo"));
        assert_eq!(c.snapshot().notes, 13);
    }
    #[test]
    fn cancel_discards_result_and_does_not_restart_until_requested() {
        let (mut c, requests) = controller();
        c.poll(Duration::ZERO);
        let request = requests.recv_timeout(Duration::from_secs(2)).unwrap();
        c.cancel();
        request.reply.send(Ok(snapshot(10))).unwrap();
        settle(&mut c, 100);
        c.poll(Duration::from_secs(1000));
        assert!(!c.is_busy());
        assert_eq!(c.version(), 0);
        assert_eq!(c.snapshot().notes, 0);
        c.refresh();
        c.poll(Duration::from_secs(1001));
        requests
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .reply
            .send(Ok(snapshot(2)))
            .unwrap();
        settle(&mut c, 1001);
        assert_eq!(c.snapshot().notes, 2);
    }
    struct PanicSource(Mutex<bool>);
    impl SnapshotSource for PanicSource {
        fn read(
            &self,
            _root: &Path,
            _query: Option<&str>,
            _limits: Limits,
            _cancelled: &AtomicBool,
        ) -> io::Result<Snapshot> {
            let panic = {
                let mut first = self.0.lock().unwrap();
                let old = *first;
                *first = false;
                old
            };
            if panic {
                panic!("fixture panic");
            }
            Ok(snapshot(3))
        }
    }
    #[test]
    fn source_panic_becomes_error_and_worker_can_recover() {
        let mut c = VaultController::with_source(
            PathBuf::from("/fixture"),
            Limits::default(),
            Arc::new(PanicSource(Mutex::new(true))),
        )
        .unwrap();
        settle(&mut c, 0);
        assert_eq!(c.error(), Some("Vault source panicked"));
        c.refresh();
        settle(&mut c, 1);
        assert_eq!(c.error(), None);
        assert_eq!(c.snapshot().notes, 3);
    }
    #[test]
    fn roots_queries_and_backwards_clock_are_safe_and_bounded() {
        assert!(VaultController::new(PathBuf::new(), Limits::default()).is_err());
        assert!(VaultController::new(PathBuf::from("/tmp/../other"), Limits::default()).is_err());
        let (mut c, requests) = controller();
        c.search(&format!("\x1b[31m{}\u{202e}", "日本語".repeat(100)));
        assert_eq!(c.query().unwrap().chars().count(), MAX_QUERY_CHARS);
        assert!(!c.query().unwrap().contains('\x1b'));
        c.poll(Duration::from_secs(10));
        requests
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .reply
            .send(Ok(snapshot(1)))
            .unwrap();
        settle(&mut c, 10);
        c.poll(Duration::ZERO);
        assert!(!c.is_busy());
        c.search(" \t\n");
        c.poll(Duration::from_secs(11));
        let request = requests.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(request.query, None);
        request.reply.send(Ok(snapshot(2))).unwrap();
        settle(&mut c, 11);
    }
    #[cfg(unix)]
    #[test]
    fn cancelled_scanner_does_not_open_root() {
        let err = crate::scan_cancellable(
            Path::new("/fixture-does-not-exist"),
            None,
            Limits::default(),
            &AtomicBool::new(true),
        )
        .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::Interrupted);
    }
}
