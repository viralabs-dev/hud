//! Bounded per-panel refresh workers; command feeds deny until explicit exact-byte trust.
use hud_custom::{Custom, Feed, Panel};
use hud_doc_store::{Approval, DocumentStore, FileDocumentStore, Proposal};
use hud_runner::{Cancellation, ConfiguredCommand, Confirmation};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver},
    thread::{self, JoinHandle},
    time::Duration,
};
const MAX_WORKERS: usize = 2;
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub text: String,
    pub error: Option<String>,
    pub refreshed_at: Option<Duration>,
}
struct Job {
    receiver: Receiver<io::Result<String>>,
    thread: JoinHandle<()>,
    cancel: Cancellation,
}
struct Entry {
    panel: Panel,
    next: Duration,
    snapshot: Snapshot,
    job: Option<Job>,
}
pub struct FeedController {
    config: PathBuf,
    digest: String,
    cwd: PathBuf,
    home: PathBuf,
    trust_dir: Option<PathBuf>,
    trusted: bool,
    pending: bool,
    entries: BTreeMap<String, Entry>,
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, message)
}
fn token(digest: &str) -> String {
    format!("hud-feed-trust v1\nsha256={digest}\n")
}
impl FeedController {
    /// Reads only explicit bounded owned configuration; no worker/provider is started here.
    pub fn new(
        config: &Path,
        cwd: &Path,
        home: &Path,
        trust_dir: Option<PathBuf>,
    ) -> io::Result<Self> {
        check_private_file(config)?;
        let bytes = hud_resources::read_owned_file(config, hud_custom::MAX_CONFIG)?;
        let custom = Custom::parse_source(&bytes, config)?;
        let digest = hash(&bytes);
        let trusted = trust_dir
            .as_ref()
            .is_some_and(|dir| read_trust(dir, &digest).unwrap_or(false));
        let active: std::collections::BTreeSet<String> = custom
            .columns
            .iter()
            .flat_map(|c| c.slots.iter().map(|s| s.id.as_str().to_owned()))
            .collect();
        let entries = custom
            .panels
            .into_iter()
            .filter(|(id, _)| active.contains(id))
            .map(|(id, panel)| {
                (
                    id,
                    Entry {
                        panel,
                        next: Duration::ZERO,
                        snapshot: Snapshot::default(),
                        job: None,
                    },
                )
            })
            .collect();
        Ok(Self {
            config: config.into(),
            digest,
            cwd: cwd.into(),
            home: home.into(),
            trust_dir,
            trusted,
            pending: false,
            entries,
        })
    }
    /// Selecting/saving a new custom never reuses old authority, even for identical bytes.
    pub fn new_untrusted(
        config: &Path,
        cwd: &Path,
        home: &Path,
        trust_dir: Option<PathBuf>,
    ) -> io::Result<Self> {
        let mut controller = Self::new(config, cwd, home, trust_dir)?;
        controller.trusted = false;
        Ok(controller)
    }
    pub fn digest(&self) -> &str {
        &self.digest
    }
    pub fn trusted(&self) -> bool {
        self.trusted
    }
    pub fn snapshots(&self) -> BTreeMap<String, Snapshot> {
        self.entries
            .iter()
            .map(|(id, e)| (id.clone(), e.snapshot.clone()))
            .collect()
    }
    pub fn running(&self) -> usize {
        self.entries.values().filter(|e| e.job.is_some()).count()
    }
    pub fn trust_preview(&mut self) -> String {
        self.pending = true;
        let commands = self
            .entries
            .iter()
            .filter_map(|(id, e)| {
                if let Feed::Command(argv) = &e.panel.feed {
                    Some(format!("{id}: {argv:?}"))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        hud_resources::sanitize(&format!("Trust command feeds for exact config SHA256 {}\n{}\nConfirm this digest explicitly; edits invalidate it.",self.digest,commands))
    }
    pub fn cancel_confirmation(&mut self) {
        self.pending = false;
    }
    /// Explicit gesture following trust_preview; token bytes belong only to this config digest.
    pub fn approve_trust(&mut self, digest: &str) -> io::Result<()> {
        let pending = std::mem::take(&mut self.pending);
        if !pending || digest != self.digest {
            return Err(invalid("missing or mismatched trust preview"));
        }
        let bytes = hud_resources::read_owned_file(&self.config, hud_custom::MAX_CONFIG)?;
        if hash(&bytes) != self.digest {
            self.trusted = false;
            self.cancel_all();
            return Err(invalid("configuration changed; reload and review"));
        }
        let dir = self
            .trust_dir
            .as_ref()
            .ok_or_else(|| invalid("explicit trust directory required"))?;
        check_private_directory(dir)?;
        let store = FileDocumentStore::new(dir)?;
        let preview = store.preview(Proposal {
            path: format!("{}.md", self.digest),
            content: token(&self.digest),
        })?;
        if preview.existing().is_some()
            && preview.existing() != Some(token(&self.digest).as_bytes())
        {
            return Err(invalid("trust token manually changed; refusing overwrite"));
        }
        if preview.existing().is_none() {
            store.apply(&preview, Approval::CreateNew)?;
        } else {
            check_private_file(&dir.join(format!("{}.md", self.digest)))?;
        }

        self.trusted = true;
        for entry in self.entries.values_mut() {
            entry.next = Duration::ZERO;
        }
        Ok(())
    }
    /// Nonblocking: drain completed jobs; coalesce one job per panel, at most two globally.
    /// `now` is elapsed monotonic time supplied by UI/test clock, never wall-clock.
    pub fn poll(&mut self, now: Duration) {
        let mut changed = false;
        for entry in self.entries.values_mut() {
            let result = entry
                .job
                .as_ref()
                .and_then(|job| match job.receiver.try_recv() {
                    Ok(r) => Some(r),
                    Err(mpsc::TryRecvError::Disconnected) => {
                        Some(Err(io::Error::other("feed worker disconnected")))
                    }
                    Err(mpsc::TryRecvError::Empty) => None,
                });
            if let Some(result) = result {
                if let Some(job) = entry.job.take() {
                    let _ = job.thread.join();
                }
                match result {
                    Ok(text) => {
                        entry.snapshot.text = text;
                        entry.snapshot.error = None;
                        entry.snapshot.refreshed_at = Some(now);
                    }
                    Err(error) => {
                        changed |= error.to_string().starts_with("config changed:")
                            || error.to_string().starts_with("command trust missing");
                        entry.snapshot.error = Some(output(&error.to_string(), 50));
                    }
                }
                entry.next = now + Duration::from_secs_f64(interval(&entry.panel));
            }
        }
        if changed {
            self.trusted = false;
            self.cancel_all();
            for entry in self.entries.values_mut() {
                entry.next = Duration::MAX;
            }
            return;
        }
        let mut available = MAX_WORKERS.saturating_sub(self.running());
        for entry in self.entries.values_mut() {
            if available == 0 {
                break;
            }
            if entry.job.is_some() || now < entry.next {
                continue;
            }
            if matches!(entry.panel.feed, Feed::Builtin) {
                continue;
            }
            if matches!(entry.panel.feed, Feed::Command(_)) && !self.trusted {
                entry.snapshot.error =
                    Some("command feed denied: exact config trust required".into());
                entry.next = now + Duration::from_secs_f64(interval(&entry.panel));
                continue;
            }
            let panel = entry.panel.clone();
            let config = self.config.clone();
            let digest = self.digest.clone();
            let cwd = self.cwd.clone();
            let home = self.home.clone();
            let trust = self.trust_dir.clone();
            let cancel = Cancellation::default();
            let worker_cancel = cancel.clone();
            let (tx, rx) = mpsc::sync_channel(1);
            let handle = thread::spawn(move || {
                let result = refresh(
                    &panel,
                    &config,
                    &digest,
                    &cwd,
                    &home,
                    trust.as_deref(),
                    &worker_cancel,
                );
                let _ = tx.send(result);
            });
            entry.job = Some(Job {
                receiver: rx,
                thread: handle,
                cancel,
            });
            available -= 1;
        }
    }
    pub fn cancel_all(&mut self) {
        for entry in self.entries.values() {
            if let Some(job) = &entry.job {
                job.cancel.cancel();
            }
        }
        for entry in self.entries.values_mut() {
            if let Some(job) = entry.job.take() {
                if matches!(entry.panel.feed, Feed::Command(_)) {
                    let deadline = std::time::Instant::now() + Duration::from_secs(2);
                    while !job.thread.is_finished() && std::time::Instant::now() < deadline {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                }
                if job.thread.is_finished() {
                    let _ = job.thread.join();
                }
                // Blocked read-only I/O cannot be interrupted; detach after cancellation.
            }
        }
    }
}
impl Drop for FeedController {
    fn drop(&mut self) {
        self.cancel_all();
    }
}
fn interval(panel: &Panel) -> f64 {
    panel.interval.clamp(5.0, 3600.0)
}
fn timeout(panel: &Panel) -> f64 {
    panel.timeout.clamp(1.0, 60.0)
}
fn output(text: &str, lines: usize) -> String {
    hud_resources::sanitize(text)
        .lines()
        .take(lines.min(2000))
        .collect::<Vec<_>>()
        .join("\n")
}
fn refresh(
    panel: &Panel,
    config: &Path,
    digest: &str,
    cwd: &Path,
    home: &Path,
    trust: Option<&Path>,
    cancel: &Cancellation,
) -> io::Result<String> {
    if cancel.is_cancelled() {
        return Err(io::ErrorKind::Interrupted.into());
    }
    check_private_file(config)?;
    let bytes = hud_resources::read_owned_file(config, hud_custom::MAX_CONFIG)?;
    if hash(&bytes) != digest {
        return Err(invalid("config changed: reload and review exact digest"));
    }
    match &panel.feed {
        Feed::Builtin => Ok(String::new()),
        Feed::Text(text) => Ok(output(text, panel.lines)),
        Feed::TextFile(path) | Feed::File(path) => {
            let cap = if matches!(panel.feed, Feed::TextFile(_)) {
                64 * 1024
            } else {
                hud_custom::MAX_CONTENT
            };
            let data = hud_resources::read_owned_file(path, cap)?;
            Ok(output(&String::from_utf8_lossy(&data), panel.lines))
        }
        Feed::Command(argv) => {
            let dir = trust.ok_or_else(|| invalid("command trust missing"))?;
            if !read_trust(dir, digest).unwrap_or(false) {
                return Err(invalid("command trust missing or modified"));
            }
            let cmd = ConfiguredCommand::new(
                argv,
                cwd,
                Duration::from_secs_f64(timeout(panel)),
                panel.lines,
            )?
            .requiring_confirmation();
            if cancel.is_cancelled() {
                return Err(io::ErrorKind::Interrupted.into());
            }
            let outcome =
                hud_runner::run_with_confirmation(&cmd, home, cancel, Confirmation::Approved)?;
            if outcome.cancelled {
                return Err(io::ErrorKind::Interrupted.into());
            }
            if outcome.timed_out {
                return Err(io::ErrorKind::TimedOut.into());
            }
            if outcome.code != Some(0) {
                return Err(io::Error::other(format!(
                    "feed exit {:?}: {}",
                    outcome.code,
                    outcome.stderr.join("\n")
                )));
            }
            Ok(output(&outcome.stdout.join("\n"), panel.lines))
        }
    }
}
fn read_trust(dir: &Path, digest: &str) -> io::Result<bool> {
    check_private_directory(dir)?;
    let path = dir.join(format!("{digest}.md"));
    let bytes = match hud_resources::read_owned_file(&path, 256) {
        Ok(b) => b,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e),
    };
    check_private_file(&path)?;
    Ok(bytes == token(digest).as_bytes())
}
#[cfg(unix)]
fn check_private(path: &Path, directory: bool) -> io::Result<()> {
    use rustix::fs::{self, FileType, Mode, OFlags};
    use std::path::Component;
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(invalid("parent traversal"));
    }
    let parts: Vec<_> = path
        .components()
        .filter_map(|c| {
            if let Component::Normal(n) = c {
                Some(n)
            } else {
                None
            }
        })
        .collect();
    let (last, parents) = parts
        .split_last()
        .ok_or_else(|| invalid("empty private path"))?;
    let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let mut fd = fs::open(
        if path.is_absolute() { "/" } else { "." },
        flags,
        Mode::empty(),
    )?;
    for name in parents {
        fd = fs::openat(&fd, *name, flags, Mode::empty())?;
        let st = fs::fstat(&fd)?;
        if st.st_mode & 0o022 != 0 && st.st_mode & 0o1000 == 0 {
            return Err(invalid("writable ancestor"));
        }
    }
    let fd = fs::openat(
        &fd,
        *last,
        if directory {
            flags
        } else {
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC
        },
        Mode::empty(),
    )?;
    let st = fs::fstat(&fd)?;
    if st.st_uid != rustix::process::getuid().as_raw()
        || st.st_mode & 0o077 != 0
        || FileType::from_raw_mode(st.st_mode)
            != if directory {
                FileType::Directory
            } else {
                FileType::RegularFile
            }
    {
        return Err(invalid("trust path must be private and owned"));
    }
    Ok(())
}
#[cfg(not(unix))]
fn check_private(_: &Path, _: bool) -> io::Result<()> {
    Err(io::ErrorKind::Unsupported.into())
}
fn check_private_directory(path: &Path) -> io::Result<()> {
    check_private(path, true)
}
fn check_private_file(path: &Path) -> io::Result<()> {
    check_private(path, false)
}
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        sync::atomic::{AtomicU64, Ordering},
        time::Instant,
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "hud-feed-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&p).unwrap();
            fs::set_permissions(&p, fs::Permissions::from_mode(0o700)).unwrap();
            Self(p)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn write(path: &Path, bytes: impl AsRef<[u8]>) {
        fs::write(path, bytes).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    fn config(t: &Temp, panel: &str) -> PathBuf {
        let p = t.0.join("custom.json");
        write(
            &p,
            format!(
                r#"{{"version":1,"name":"test","columns":[{{"slots":[{{"id":"sample"}}]}}],"panels":[{panel}]}}"#
            ),
        );
        p
    }
    fn settle(c: &mut FeedController, now: Duration) {
        let start = Instant::now();
        c.poll(now);
        while c.running() > 0 {
            assert!(start.elapsed() < Duration::from_secs(4));
            thread::sleep(Duration::from_millis(5));
            c.poll(now);
        }
    }
    #[test]
    fn digest_matches_known_vector() {
        assert_eq!(
            hash(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
    #[test]
    fn text_refresh_fake_clock_coalesces_and_sanitizes() {
        let t = Temp::new();
        let p = config(
            &t,
            r#"{"id":"sample","title":"Sample","type":"text","text":"a\nb\nc","lines":2,"interval":5.5,"timeout":1}"#,
        );
        let mut c = FeedController::new(&p, &t.0, &t.0, None).unwrap();
        settle(&mut c, Duration::ZERO);
        assert_eq!(c.snapshots()["sample"].text, "a\nb");
        assert_eq!(c.running(), 0);
        c.poll(Duration::from_secs(5));
        assert_eq!(c.running(), 0);
        c.poll(Duration::from_millis(5500));
        assert_eq!(c.running(), 1);
        c.poll(Duration::from_millis(5500));
        assert!(c.running() <= 1);
    }
    #[test]
    fn command_denied_until_exact_explicit_trust() {
        let t = Temp::new();
        let printf = t.0.join("fixture-printf");
        fs::copy("/usr/bin/printf", &printf).unwrap();
        fs::set_permissions(&printf, fs::Permissions::from_mode(0o700)).unwrap();
        let panel = format!(
            r#"{{"id":"sample","title":"Sample","type":"command","argv":["{}","%s","fixture"],"interval":5,"timeout":1}}"#,
            printf.display()
        );
        let p = config(&t, &panel);
        let mut c = FeedController::new(&p, &t.0, &t.0, Some(t.0.clone())).unwrap();
        settle(&mut c, Duration::ZERO);
        assert_eq!(c.running(), 0);
        assert!(c.snapshots()["sample"]
            .error
            .as_ref()
            .unwrap()
            .contains("denied"));
        let digest = c.digest().to_owned();
        assert!(c.approve_trust(&digest).is_err());
        c.trust_preview();
        assert!(c.approve_trust("wrong").is_err());
        c.trust_preview();
        c.approve_trust(&digest).unwrap();
        settle(&mut c, Duration::ZERO);
        assert_eq!(
            c.snapshots()["sample"].text,
            "fixture",
            "{:?}",
            c.snapshots()["sample"]
        );
        assert!(FeedController::new(&p, &t.0, &t.0, Some(t.0.clone()))
            .unwrap()
            .trusted());
    }
    #[test]
    fn deleted_trust_token_revokes_visible_state() {
        let t = Temp::new();
        let printf = t.0.join("printf-fixture");
        fs::copy("/usr/bin/printf", &printf).unwrap();
        fs::set_permissions(&printf, fs::Permissions::from_mode(0o700)).unwrap();
        let p = config(
            &t,
            &format!(
                r#"{{"id":"sample","title":"Sample","type":"command","argv":["{}","fixture"],"interval":5}}"#,
                printf.display()
            ),
        );
        let mut c = FeedController::new(&p, &t.0, &t.0, Some(t.0.clone())).unwrap();
        let digest = c.digest().to_owned();
        c.trust_preview();
        c.approve_trust(&digest).unwrap();
        settle(&mut c, Duration::ZERO);
        assert!(c.trusted());
        fs::remove_file(t.0.join(format!("{digest}.md"))).unwrap();
        settle(&mut c, Duration::from_secs(6));
        assert!(!c.trusted());
        assert_eq!(c.snapshots()["sample"].text, "fixture");
    }
    #[test]
    fn blocked_read_job_does_not_block_drop() {
        let t = Temp::new();
        let p = config(
            &t,
            r#"{"id":"sample","title":"Sample","type":"text","text":"fixture"}"#,
        );
        let mut c = FeedController::new(&p, &t.0, &t.0, None).unwrap();
        let (release, gate) = mpsc::channel();
        let (tx, rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            let _ = gate.recv();
            let _ = tx.send(Ok(String::new()));
        });
        c.entries.get_mut("sample").unwrap().job = Some(Job {
            receiver: rx,
            thread: handle,
            cancel: Cancellation::default(),
        });
        let started = Instant::now();
        drop(c);
        assert!(started.elapsed() < Duration::from_millis(200));
        release.send(()).unwrap();
    }
    #[test]
    fn changed_config_revokes_and_preserves_stale_snapshot() {
        let t = Temp::new();
        let p = config(
            &t,
            r#"{"id":"sample","title":"Sample","type":"text","text":"before","interval":5}"#,
        );
        let mut c = FeedController::new(&p, &t.0, &t.0, None).unwrap();
        settle(&mut c, Duration::ZERO);
        let mut changed = fs::read(&p).unwrap();
        changed.push(b' ');
        write(&p, changed);
        settle(&mut c, Duration::from_secs(5));
        assert_eq!(c.snapshots()["sample"].text, "before");
        assert!(c.snapshots()["sample"]
            .error
            .as_ref()
            .unwrap()
            .contains("config changed"));
        c.poll(Duration::from_secs(1000));
        assert_eq!(c.running(), 0);
    }
    #[test]
    fn stale_trust_preview_and_manually_modified_token_refused() {
        let t = Temp::new();
        let p = config(
            &t,
            r#"{"id":"sample","title":"Sample","type":"command","argv":["/usr/bin/printf","fixture"]}"#,
        );
        let mut c = FeedController::new(&p, &t.0, &t.0, Some(t.0.clone())).unwrap();
        let d = c.digest().to_owned();
        c.trust_preview();
        let mut bytes = fs::read(&p).unwrap();
        bytes.push(b' ');
        write(&p, bytes);
        assert!(c.approve_trust(&d).is_err());
        let mut c = FeedController::new(&p, &t.0, &t.0, Some(t.0.clone())).unwrap();
        let d = c.digest().to_owned();
        write(&t.0.join(format!("{d}.md")), "manual\n");
        c.trust_preview();
        assert!(c.approve_trust(&d).is_err());
        assert_eq!(
            fs::read_to_string(t.0.join(format!("{d}.md"))).unwrap(),
            "manual\n"
        );
    }
    #[test]
    fn file_failure_keeps_last_snapshot_and_no_more_than_two_workers() {
        let t = Temp::new();
        let file = t.0.join("feed.txt");
        write(&file, "stable\n");
        let panel = format!(
            r#"{{"id":"sample","title":"Sample","type":"file","path":"{}","interval":5}}"#,
            file.display()
        );
        let p = config(&t, &panel);
        let mut c = FeedController::new(&p, &t.0, &t.0, None).unwrap();
        settle(&mut c, Duration::ZERO);
        assert_eq!(c.snapshots()["sample"].text, "stable");
        fs::remove_file(&file).unwrap();
        settle(&mut c, Duration::from_secs(5));
        assert_eq!(c.snapshots()["sample"].text, "stable");
        assert!(c.snapshots()["sample"].error.is_some());
    }
    #[test]
    fn private_config_and_trust_required() {
        let t = Temp::new();
        let p = config(
            &t,
            r#"{"id":"sample","title":"Sample","type":"text","text":"x"}"#,
        );
        fs::set_permissions(&p, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(FeedController::new(&p, &t.0, &t.0, None).is_err());
        assert!(check_private_directory(Path::new("/tmp")).is_err());
    }
    #[test]
    fn registry_global_cap_and_cancel_drop_join() {
        let t = Temp::new();
        let p = t.0.join("custom.json");
        let panels = (0..12)
            .map(|i| format!(r#"{{"id":"p{i}","title":"P","type":"text","text":"fixture"}}"#))
            .collect::<Vec<_>>()
            .join(",");
        let cols = (0..2)
            .map(|c| {
                let slots = (c * 6..c * 6 + 6)
                    .map(|i| format!(r#"{{"id":"p{i}"}}"#))
                    .collect::<Vec<_>>()
                    .join(",");
                format!(r#"{{"slots":[{slots}]}}"#)
            })
            .collect::<Vec<_>>()
            .join(",");
        write(
            &p,
            format!(r#"{{"version":1,"name":"test","columns":[{cols}],"panels":[{panels}]}}"#),
        );
        let mut c = FeedController::new(&p, &t.0, &t.0, None).unwrap();
        c.poll(Duration::ZERO);
        assert_eq!(c.running(), 2);
        c.poll(Duration::ZERO);
        assert!(c.running() <= 2);
        c.cancel_all();
        assert_eq!(c.running(), 0);
        let sleep = t.0.join("fixture-sleep");
        fs::copy("/usr/bin/sleep", &sleep).unwrap();
        fs::set_permissions(&sleep, fs::Permissions::from_mode(0o700)).unwrap();
        let panel = format!(
            r#"{{"id":"sample","title":"Sleep","type":"command","argv":["{}","10"],"timeout":10}}"#,
            sleep.display()
        );
        let p = config(&t, &panel);
        let mut c = FeedController::new(&p, &t.0, &t.0, Some(t.0.clone())).unwrap();
        let d = c.digest().to_owned();
        c.trust_preview();
        c.approve_trust(&d).unwrap();
        c.poll(Duration::ZERO);
        thread::sleep(Duration::from_millis(30));
        let start = Instant::now();
        drop(c);
        assert!(start.elapsed() < Duration::from_secs(1));
    }
    #[test]
    fn command_timeout_does_not_replace_last_snapshot() {
        let t = Temp::new();
        let sleep = t.0.join("fixture-sleep");
        fs::copy("/usr/bin/sleep", &sleep).unwrap();
        fs::set_permissions(&sleep, fs::Permissions::from_mode(0o700)).unwrap();
        let panel = format!(
            r#"{{"id":"sample","title":"Sleep","type":"command","argv":["{}","10"],"timeout":1}}"#,
            sleep.display()
        );
        let p = config(&t, &panel);
        let mut c = FeedController::new(&p, &t.0, &t.0, Some(t.0.clone())).unwrap();
        let d = c.digest().to_owned();
        c.trust_preview();
        c.approve_trust(&d).unwrap();
        settle(&mut c, Duration::ZERO);
        assert!(c.snapshots()["sample"].error.is_some());
        assert!(c.snapshots()["sample"].text.is_empty());
        assert_eq!(c.running(), 0);
    }
}
