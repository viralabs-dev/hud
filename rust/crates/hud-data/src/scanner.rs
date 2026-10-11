use super::*;
use rustix::fs::{self, AtFlags, Dir, FileType, Mode, OFlags};
use std::{fs::File, io::Read, os::fd::OwnedFd, path::Component};
#[cfg(test)]
thread_local! { static BEFORE_OPEN: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = std::cell::RefCell::new(None); }
fn before_open() {
    #[cfg(test)]
    BEFORE_OPEN.with(|hook| {
        if let Some(hook) = hook.borrow_mut().take() {
            hook();
        }
    });
}
fn flags() -> OFlags {
    OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC
}
pub(super) fn open_root(root: &Path) -> io::Result<OwnedFd> {
    if root.as_os_str().is_empty() || root.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "empty root or parent traversal",
        ));
    }
    let root = if root.is_absolute() {
        root.to_owned()
    } else {
        std::env::current_dir()?.join(root)
    };
    let mut fd = fs::open("/", flags(), Mode::empty())?;
    for component in root.components() {
        if let Component::Normal(name) = component {
            fd = fs::openat(&fd, name, flags(), Mode::empty())?;
        }
    }
    Ok(fd)
}
#[derive(Clone, Debug, Eq, PartialEq)]
struct Stamp {
    dev: u64,
    ino: u64,
    uid: u32,
    size: i64,
    mtime: i64,
    mtime_ns: i64,
    ctime: i64,
    ctime_ns: i64,
}
#[allow(clippy::unnecessary_cast)] // Stat types differ between Unix targets.
fn stamp(st: &fs::Stat) -> Stamp {
    Stamp {
        dev: st.st_dev as u64,
        ino: st.st_ino as u64,
        uid: st.st_uid,
        size: st.st_size,
        mtime: st.st_mtime,
        mtime_ns: st.st_mtime_nsec as i64,
        ctime: st.st_ctime,
        ctime_ns: st.st_ctime_nsec as i64,
    }
}
#[derive(Clone)]
struct CacheEntry {
    stamp: Stamp,
    text: std::sync::Arc<str>,
    raw_bytes: usize,
    board: Option<Board>,
    tasks: Vec<Item>,
}
#[derive(Default)]
pub struct ScanCache {
    root: Option<std::path::PathBuf>,
    entries: BTreeMap<String, CacheEntry>,
}
impl ScanCache {
    pub fn clear(&mut self) {
        self.entries.clear();
        self.root = None;
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
struct Scan<'a> {
    snap: Snapshot,
    limits: Limits,
    bytes: usize,
    entries: usize,
    term: Option<&'a str>,
    cancelled: &'a std::sync::atomic::AtomicBool,
    old_cache: &'a BTreeMap<String, CacheEntry>,
    next_cache: BTreeMap<String, CacheEntry>,
    cache_bytes: usize,
    calendar: CalendarMoment,
    recent: Vec<(i64, i64, String)>,
}
impl Scan<'_> {
    fn check_cancelled(&self) -> io::Result<()> {
        if self.cancelled.load(std::sync::atomic::Ordering::Relaxed) {
            Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "Vault scan cancelled",
            ))
        } else {
            Ok(())
        }
    }
    fn walk(&mut self, fd: OwnedFd, base: &str, depth: usize) -> io::Result<()> {
        self.check_cancelled()?;
        if depth > self.limits.depth {
            self.snap.capped = true;
            return Ok(());
        }
        let mut names = Vec::new();
        for entry in Dir::read_from(&fd)? {
            self.check_cancelled()?;
            let entry = entry?;
            self.entries += 1;
            if self.entries > self.limits.files.saturating_mul(4) {
                self.snap.capped = true;
                break;
            }
            let Ok(name) = entry.file_name().to_str() else {
                self.snap.rejected += 1;
                continue;
            };
            if !name.starts_with('.') {
                names.push(name.to_owned());
            }
        }
        names.sort();
        for name in names {
            self.check_cancelled()?;
            if self.snap.notes >= self.limits.files || self.bytes >= self.limits.total_bytes {
                self.snap.capped = true;
                break;
            }
            let rel = if base.is_empty() {
                name.clone()
            } else {
                format!("{base}/{name}")
            };
            let before = match fs::statat(&fd, name.as_str(), AtFlags::SYMLINK_NOFOLLOW) {
                Ok(st) => st,
                Err(_) => {
                    self.snap.rejected += 1;
                    continue;
                }
            };
            let kind = FileType::from_raw_mode(before.st_mode);
            if kind == FileType::Directory {
                if [
                    "node_modules",
                    "__pycache__",
                    "venv",
                    "target",
                    "dist",
                    "build",
                    "vendor",
                ]
                .contains(&name.as_str())
                {
                    continue;
                }
                match fs::openat(&fd, name.as_str(), flags(), Mode::empty()) {
                    Ok(child) => {
                        let st = fs::fstat(&child)?;
                        if st.st_dev == before.st_dev && st.st_ino == before.st_ino {
                            self.walk(child, &rel, depth + 1)?
                        } else {
                            self.snap.rejected += 1;
                        }
                    }
                    Err(_) => self.snap.rejected += 1,
                }
                continue;
            }
            let lower = name.to_lowercase();
            if ![".md", ".markdown", ".txt"]
                .iter()
                .any(|ext| lower.ends_with(ext))
            {
                continue;
            }
            if kind != FileType::RegularFile {
                self.snap.rejected += 1;
                continue;
            }
            self.snap.notes += 1;
            if rel.contains("sync-conflict") {
                self.snap.today += usize::from(before.st_mtime >= self.calendar.midnight_unix);
                self.snap.conflicts.push(rel);
                continue;
            }
            self.check_cancelled()?;
            before_open();
            let opened = match fs::openat(
                &fd,
                name.as_str(),
                OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
                Mode::empty(),
            ) {
                Ok(fd) => fd,
                Err(_) => {
                    self.snap.rejected += 1;
                    continue;
                }
            };
            let st = fs::fstat(&opened)?;
            if FileType::from_raw_mode(st.st_mode) != FileType::RegularFile
                || st.st_uid != rustix::process::getuid().as_raw()
                || st.st_dev != before.st_dev
                || st.st_ino != before.st_ino
                || st.st_size < 0
                || st.st_size as u64 > self.limits.note_bytes as u64
            {
                self.snap.rejected += 1;
                continue;
            }
            let available = self.limits.total_bytes.saturating_sub(self.bytes);
            let maximum = self.limits.note_bytes.min(available);
            let current_stamp = stamp(&st);
            let cached = self
                .old_cache
                .get(&rel)
                .filter(|entry| entry.stamp == current_stamp && entry.raw_bytes <= maximum);
            let entry = if let Some(entry) = cached {
                if stamp(&fs::fstat(&opened)?) != current_stamp {
                    self.snap.rejected += 1;
                    continue;
                }
                self.snap.cache_hits += 1;
                entry.clone()
            } else {
                let mut raw = Vec::new();
                self.snap.reads += 1;
                let mut file = File::from(opened);
                (&mut file)
                    .take(maximum.saturating_add(1) as u64)
                    .read_to_end(&mut raw)?;
                if raw.len() > maximum || stamp(&fs::fstat(&file)?) != current_stamp {
                    self.snap.rejected += 1;
                    if maximum < self.limits.note_bytes {
                        self.snap.capped = true;
                    }
                    continue;
                }
                let text: std::sync::Arc<str> = String::from_utf8_lossy(&raw)
                    .replace("\r\n", "\n")
                    .replace('\r', "\n")
                    .into();
                CacheEntry {
                    stamp: current_stamp,
                    raw_bytes: raw.len(),
                    board: parse_board(&text, &rel),
                    tasks: parse_tasks(&text, &rel),
                    text,
                }
            };
            self.bytes += entry.raw_bytes;
            let text = &entry.text;
            self.snap.today += usize::from(st.st_mtime >= self.calendar.midnight_unix);
            self.recent
                .push((st.st_mtime, st.st_mtime_nsec as i64, rel.clone()));
            if let Some(board) = &entry.board {
                self.snap.boards.push(board.clone());
            }
            self.snap.tasks.extend(entry.tasks.iter().cloned());
            if self.cache_bytes.saturating_add(text.len()) <= self.limits.total_bytes {
                self.cache_bytes += text.len();
                self.next_cache.insert(rel.clone(), entry.clone());
            }
            if let Some(term) = self.term {
                if self.snap.hits.len() < self.limits.hits && rel.to_lowercase().contains(term) {
                    self.snap.hits.push(Hit {
                        rel: rel.clone(),
                        line: 0,
                        text: String::new(),
                    })
                }
                if self.snap.hits.len() < self.limits.hits {
                    if let Some((n, line)) = text
                        .lines()
                        .enumerate()
                        .find(|(_, line)| line.to_lowercase().contains(term))
                    {
                        self.snap.hits.push(Hit {
                            rel,
                            line: n + 1,
                            text: clean_line(line.trim()),
                        });
                    }
                }
            }
        }
        Ok(())
    }
}
pub fn scan(root: &Path, term: Option<&str>, limits: Limits) -> io::Result<Snapshot> {
    scan_cancellable(
        root,
        term,
        limits,
        &std::sync::atomic::AtomicBool::new(false),
    )
}
pub fn scan_cancellable(
    root: &Path,
    term: Option<&str>,
    limits: Limits,
    cancelled: &std::sync::atomic::AtomicBool,
) -> io::Result<Snapshot> {
    // Preserve historical cancellation-before-root behavior, including clock failure.
    if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "Vault scan cancelled",
        ));
    }
    scan_cached(
        root,
        term,
        limits,
        cancelled,
        &mut ScanCache::default(),
        SystemLocalClock.moment()?,
    )
}
pub fn scan_cached(
    root: &Path,
    term: Option<&str>,
    limits: Limits,
    cancelled: &std::sync::atomic::AtomicBool,
    cache: &mut ScanCache,
    calendar: CalendarMoment,
) -> io::Result<Snapshot> {
    if limits.note_bytes == 0 || limits.files == 0 || limits.depth == 0 || limits.total_bytes == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "scan limits must be positive",
        ));
    }
    if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "Vault scan cancelled",
        ));
    }
    let root = if root.is_absolute() {
        root.to_path_buf()
    } else {
        std::env::current_dir()?.join(root)
    };
    let fd = open_root(&root)?;
    if cache.root.as_ref() != Some(&root) {
        cache.clear();
    }
    let obsidian = fs::openat(&fd, ".obsidian", flags(), Mode::empty()).is_ok();
    let term = term.map(str::to_lowercase);
    let mut state = Scan {
        snap: Snapshot::default(),
        limits,
        bytes: 0,
        entries: 0,
        term: term.as_deref(),
        cancelled,
        old_cache: &cache.entries,
        next_cache: BTreeMap::new(),
        cache_bytes: 0,
        calendar,
        recent: Vec::new(),
    };
    state.walk(fd, "", 0)?;
    state.check_cancelled()?;
    state.snap.obsidian = obsidian;
    state.recent.sort_by(|a, b| b.cmp(a));
    state.snap.recent = state
        .recent
        .iter()
        .take(12)
        .map(|(t, _, p)| (p.clone(), *t))
        .collect();
    state.snap.conflicts.sort();
    state.snap.boards.sort_by_key(|b| {
        (
            std::cmp::Reverse(*b.counts.get("doing").unwrap_or(&0)),
            std::cmp::Reverse(*b.counts.get("blocked").unwrap_or(&0)),
            std::cmp::Reverse(*b.counts.get("todo").unwrap_or(&0)),
            b.name.to_lowercase(),
        )
    });
    let Scan {
        snap, next_cache, ..
    } = state;
    cache.entries = next_cache;
    cache.root = Some(root);
    Ok(snap)
}

#[cfg(test)]
mod adversarial {
    use super::*;
    use std::{
        fs as host,
        os::unix::fs::symlink,
        sync::atomic::{AtomicUsize, Ordering},
    };
    static ID: AtomicUsize = AtomicUsize::new(0);
    fn fixture() -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "hud-data-race-{}-{}",
            std::process::id(),
            ID.fetch_add(1, Ordering::Relaxed)
        ));
        host::create_dir(&root).unwrap();
        root
    }
    #[test]
    fn swapped_regular_inode_is_rejected() {
        let root = fixture();
        host::write(root.join("a.md"), "old").unwrap();
        let moved = root.clone();
        BEFORE_OPEN.with(|h| {
            *h.borrow_mut() = Some(Box::new(move || {
                host::rename(moved.join("a.md"), moved.join("old.txt")).unwrap();
                host::write(moved.join("a.md"), "- [ ] unexpected 📅 2026-10-11").unwrap();
            }))
        });
        let snap = scan(&root, None, Limits::default()).unwrap();
        assert_eq!(snap.rejected, 1);
        assert!(snap.tasks.is_empty());
        host::remove_dir_all(root).unwrap();
    }
    #[test]
    fn swapped_symlink_is_rejected() {
        let root = fixture();
        host::write(root.join("a.md"), "old").unwrap();
        let moved = root.clone();
        BEFORE_OPEN.with(|h| {
            *h.borrow_mut() = Some(Box::new(move || {
                host::remove_file(moved.join("a.md")).unwrap();
                symlink("/etc/passwd", moved.join("a.md")).unwrap();
            }))
        });
        let snap = scan(&root, None, Limits::default()).unwrap();
        assert_eq!(snap.rejected, 1);
        assert!(snap.tasks.is_empty());
        host::remove_dir_all(root).unwrap();
    }
    #[test]
    fn fifo_and_total_budget_are_bounded() {
        let root = fixture();
        host::write(root.join("a.md"), "- [ ] too large 📅 2026-10-11").unwrap();
        fs::mkfifoat(
            rustix::fs::CWD,
            root.join("pipe.md"),
            Mode::RUSR | Mode::WUSR,
        )
        .unwrap();
        let snap = scan(
            &root,
            None,
            Limits {
                total_bytes: 4,
                ..Limits::default()
            },
        )
        .unwrap();
        assert!(snap.tasks.is_empty());
        assert!(snap.capped);
        assert_eq!(snap.rejected, 2);
        host::remove_dir_all(root).unwrap();
    }
    #[test]
    fn depth_limit_prevents_deeper_reads() {
        let root = fixture();
        host::create_dir_all(root.join("one/two")).unwrap();
        host::write(root.join("one/two/a.md"), "- [ ] hidden 📅 2026-10-11").unwrap();
        let snap = scan(
            &root,
            None,
            Limits {
                depth: 1,
                ..Limits::default()
            },
        )
        .unwrap();
        assert!(snap.capped);
        assert!(snap.tasks.is_empty());
        host::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod cached_tests {
    use super::*;
    use std::{
        fs as host,
        sync::atomic::{AtomicBool, AtomicU64, Ordering},
        time::{Duration, SystemTime},
    };
    static ID: AtomicU64 = AtomicU64::new(0);
    struct Temp(std::path::PathBuf);
    impl Temp {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "hud-cache-{}-{}",
                std::process::id(),
                ID.fetch_add(1, Ordering::Relaxed)
            ));
            host::create_dir(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = host::remove_dir_all(&self.0);
        }
    }
    fn moment() -> CalendarMoment {
        CalendarMoment {
            today: Date::new(2026, 3, 8).unwrap(),
            midnight_unix: 100000,
            now_unix: 180000,
        }
    }
    fn read(t: &Temp, cache: &mut ScanCache, term: Option<&str>, limits: Limits) -> Snapshot {
        scan_cached(&t.0, term, limits, &AtomicBool::new(false), cache, moment()).unwrap()
    }
    #[test]
    fn unchanged_cache_hit_changed_same_size_preserved_mtime_and_removed_notes() {
        let t = Temp::new();
        let p = t.0.join("a.md");
        host::write(&p, "- [ ] café 📅 2026-03-08").unwrap();
        let mut cache = ScanCache::default();
        let first = read(&t, &mut cache, None, Limits::default());
        assert_eq!(first.reads, 1);
        let second = read(&t, &mut cache, Some("CAFÉ"), Limits::default());
        assert_eq!(second.reads, 0);
        assert_eq!(second.cache_hits, 1);
        assert_eq!(second.hits.len(), 1);
        let modified = host::metadata(&p).unwrap().modified().unwrap();
        std::thread::sleep(Duration::from_millis(1));
        host::write(&p, "- [ ] chá! 📅 2026-03-08").unwrap();
        host::File::options()
            .write(true)
            .open(&p)
            .unwrap()
            .set_times(host::FileTimes::new().set_modified(modified))
            .unwrap();
        let third = read(&t, &mut cache, None, Limits::default());
        assert_eq!(third.reads, 1);
        assert_eq!(third.tasks[0].text, "chá!");
        host::remove_file(p).unwrap();
        let removed = read(&t, &mut cache, None, Limits::default());
        assert_eq!(removed.notes, 0);
        assert!(cache.is_empty());
    }
    #[test]
    fn cache_does_not_bypass_new_limits_links_or_cancellation() {
        let t = Temp::new();
        host::write(t.0.join("a.md"), "- [ ] cached 📅 2026-03-08").unwrap();
        let mut cache = ScanCache::default();
        read(&t, &mut cache, None, Limits::default());
        let err = scan_cached(
            &t.0,
            None,
            Limits::default(),
            &AtomicBool::new(true),
            &mut cache,
            moment(),
        )
        .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::Interrupted);
        assert_eq!(cache.len(), 1);
        let capped = read(
            &t,
            &mut cache,
            None,
            Limits {
                note_bytes: 2,
                ..Limits::default()
            },
        );
        assert!(capped.tasks.is_empty());
        assert_eq!(capped.cache_hits, 0);
        assert!(cache.is_empty());
        host::remove_file(t.0.join("a.md")).unwrap();
        std::os::unix::fs::symlink("/etc/passwd", t.0.join("a.md")).unwrap();
        let snap = read(&t, &mut cache, None, Limits::default());
        assert!(snap.tasks.is_empty());
        assert!(snap.rejected > 0);
    }
    #[test]
    fn recent_today_obsidian_and_filters_use_injected_local_midnight() {
        let t = Temp::new();
        host::create_dir(t.0.join(".obsidian")).unwrap();
        for (name, stamp) in [
            ("before.md", 99999),
            ("midnight.md", 100000),
            ("after.md", 100001),
        ] {
            let p = t.0.join(name);
            host::write(&p, "- [ ] due 📅 2026-03-08").unwrap();
            host::File::options()
                .write(true)
                .open(&p)
                .unwrap()
                .set_times(
                    host::FileTimes::new()
                        .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(stamp)),
                )
                .unwrap();
        }
        let snap = read(&t, &mut ScanCache::default(), None, Limits::default());
        assert!(snap.obsidian);
        assert_eq!(snap.today, 2);
        assert_eq!(snap.recent[0].0, "after.md");
        assert_eq!(snap.tasks_on(moment().today).len(), 3);
        assert_eq!(snap.recent_matching("MIDNIGHT").len(), 1);
        let next = CalendarMoment {
            today: moment().today.add_days(1).unwrap(),
            midnight_unix: 182800,
            now_unix: 182801,
        };
        let snap = scan_cached(
            &t.0,
            None,
            Limits::default(),
            &AtomicBool::new(false),
            &mut ScanCache::default(),
            next,
        )
        .unwrap();
        assert_eq!(snap.today, 0);
    }
    #[test]
    fn cache_root_change_and_inode_replacement_force_reads() {
        let a = Temp::new();
        let b = Temp::new();
        host::write(a.0.join("a.md"), "one").unwrap();
        host::write(b.0.join("a.md"), "two").unwrap();
        let mut cache = ScanCache::default();
        read(&a, &mut cache, None, Limits::default());
        assert_eq!(
            read(&b, &mut cache, Some("two"), Limits::default()).reads,
            1
        );
        host::rename(b.0.join("a.md"), b.0.join("old.md")).unwrap();
        host::write(b.0.join("a.md"), "new").unwrap();
        let snap = read(&b, &mut cache, Some("new"), Limits::default());
        assert_eq!(snap.reads, 2);
        assert_eq!(snap.hits[0].text, "new");
    }
}
