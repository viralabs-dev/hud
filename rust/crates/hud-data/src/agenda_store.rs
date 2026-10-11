//! Explicit private agenda directory. No Python config or active Vault is accessed.
use super::Agenda;
use std::{
    io,
    path::{Path, PathBuf},
};
#[derive(Clone, Debug)]
pub enum AgendaMutation {
    Add(super::Item),
    Done(usize),
    Remove(usize),
}
impl AgendaMutation {
    fn next(&self, previous: &Agenda) -> io::Result<Agenda> {
        let mut next = previous.clone();
        let valid = match self {
            Self::Add(item) => {
                next.add(item.clone());
                true
            }
            Self::Done(index) => next.mark_done(*index),
            Self::Remove(index) => next.remove(*index).is_some(),
        };
        if valid {
            Ok(next)
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid agenda identity",
            ))
        }
    }
}
pub trait LocalNotesStore {
    fn append(&self, stamp: &str, text: &str) -> io::Result<()>;
    fn recent(&self, count: usize) -> io::Result<String>;
}
pub struct FileLocalNotesStore {
    state: FileAgendaStore,
}
impl FileLocalNotesStore {
    pub fn new(directory: impl AsRef<Path>) -> io::Result<Self> {
        Ok(Self {
            state: FileAgendaStore::new(directory, MAX_AGENDA_ITEMS)?,
        })
    }
}
/// Explicit Rust-only remembered folder; never reads Python personal state.
pub trait FolderSelectionStore {
    fn load(&self) -> io::Result<Option<PathBuf>>;
    fn save(&self, previous: Option<&Path>, next: Option<&Path>) -> io::Result<()>;
}
pub struct FileFolderSelectionStore {
    state: FileAgendaStore,
}
impl FileFolderSelectionStore {
    pub fn new(directory: impl AsRef<Path>) -> io::Result<Self> {
        Ok(Self {
            state: FileAgendaStore::new(directory, MAX_AGENDA_ITEMS)?,
        })
    }
}
pub trait AgendaStore {
    fn load(&self) -> io::Result<Option<Agenda>>;
    fn save(&self, agenda: &Agenda) -> io::Result<()>;
    fn apply(&self, previous: &Agenda, mutation: &AgendaMutation) -> io::Result<Agenda> {
        let loaded = self.load()?.unwrap_or_default();
        if loaded.items != previous.items {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "agenda changed on disk; refresh before selecting",
            ));
        }
        let next = mutation.next(previous)?;
        self.save(&next)?;
        Ok(next)
    }
}
pub struct FileAgendaStore {
    directory: PathBuf,
    max_items: usize,
}
impl FileAgendaStore {
    pub fn new(directory: impl AsRef<Path>, max_items: usize) -> io::Result<Self> {
        if max_items == 0 || max_items > MAX_AGENDA_ITEMS {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid agenda item cap",
            ));
        }
        let directory = directory.as_ref();
        if directory.as_os_str().is_empty()
            || directory
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "empty state directory or parent traversal",
            ));
        }
        let directory = if directory.is_absolute() {
            directory.to_owned()
        } else {
            std::env::current_dir()?.join(directory)
        };
        if directory.parent().is_none() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "state directory cannot be root",
            ));
        }
        Ok(Self {
            directory,
            max_items,
        })
    }
}
#[cfg(unix)]
mod unix {
    use super::*;
    use rustix::fs::{self, AtFlags, FileType, Mode, OFlags};
    use std::{
        fs::File,
        io::{Read, Seek, SeekFrom, Write},
        os::{fd::OwnedFd, unix::fs::MetadataExt},
        path::Component,
        sync::atomic::{AtomicU64, Ordering},
    };
    const NAME: &str = "agenda.md";
    #[cfg(test)]
    thread_local! { pub(super) static FAIL_COMMIT:std::cell::Cell<bool>=const {std::cell::Cell::new(false)}; }
    #[cfg(test)]
    thread_local! { pub(super) static BEFORE_COMMIT:std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const { std::cell::RefCell::new(None) }; }
    fn before_commit() -> io::Result<()> {
        #[cfg(test)]
        if let Some(hook) = BEFORE_COMMIT.with(|hook| hook.borrow_mut().take()) {
            hook();
        }
        #[cfg(test)]
        if FAIL_COMMIT.with(|flag| flag.replace(false)) {
            return Err(io::Error::other("fixture failure before rename"));
        }
        Ok(())
    }
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    fn invalid(message: &str) -> io::Error {
        io::Error::new(io::ErrorKind::PermissionDenied, message)
    }
    fn flags() -> OFlags {
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC
    }
    fn private(fd: &OwnedFd, directory: bool) -> io::Result<()> {
        let st = fs::fstat(fd)?;
        let expected = if directory {
            FileType::Directory
        } else {
            FileType::RegularFile
        };
        if FileType::from_raw_mode(st.st_mode) != expected
            || st.st_uid != rustix::process::getuid().as_raw()
            || st.st_mode & 0o077 != 0
            || (!directory && st.st_nlink != 1)
        {
            return Err(invalid(
                "state must be owned by current user, private, and of expected type",
            ));
        }
        Ok(())
    }
    impl FileAgendaStore {
        // Each name is opened relative to a pinned parent descriptor; no
        // canonicalize-then-open window and no followed symlink component.
        fn directory(&self, create: bool) -> io::Result<OwnedFd> {
            let mut fd = fs::open("/", flags(), Mode::empty())?;
            let names: Vec<_> = self
                .directory
                .components()
                .filter_map(|c| match c {
                    Component::Normal(n) => Some(n),
                    _ => None,
                })
                .collect();
            for (i, name) in names.iter().enumerate() {
                let last = i + 1 == names.len();
                let next = match fs::openat(&fd, *name, flags(), Mode::empty()) {
                    Ok(next) => next,
                    Err(rustix::io::Errno::NOENT) if create && last => {
                        match fs::mkdirat(&fd, *name, Mode::from_raw_mode(0o700)) {
                            Ok(()) | Err(rustix::io::Errno::EXIST) => (),
                            Err(e) => return Err(e.into()),
                        }
                        fs::openat(&fd, *name, flags(), Mode::empty())?
                    }
                    Err(e) => return Err(e.into()),
                };
                if last {
                    private(&next, true)?;
                } else {
                    let st = fs::fstat(&next)?;
                    // Sticky shared /tmp is allowed; non-sticky writable
                    // ancestors could let another user replace this chain.
                    if st.st_mode & 0o022 != 0 && st.st_mode & 0o1000 == 0 {
                        return Err(invalid("writable ancestor"));
                    }
                }
                fd = next;
            }
            Ok(fd)
        }
    }
    impl AgendaStore for FileAgendaStore {
        fn load(&self) -> io::Result<Option<Agenda>> {
            let dir = match self.directory(false) {
                Ok(d) => d,
                Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
                Err(e) => return Err(e),
            };
            let fd = match fs::openat(
                &dir,
                NAME,
                OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
                Mode::empty(),
            ) {
                Ok(fd) => fd,
                Err(rustix::io::Errno::NOENT) => return Ok(None),
                Err(e) => return Err(e.into()),
            };
            private(&fd, false)?;
            if fs::fstat(&fd)?.st_size > MAX_AGENDA_BYTES as i64 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "agenda exceeds size cap",
                ));
            }
            let mut data = Vec::new();
            File::from(fd)
                .take(MAX_AGENDA_BYTES as u64 + 1)
                .read_to_end(&mut data)?;
            if data.len() > MAX_AGENDA_BYTES {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "agenda grew beyond size cap",
                ));
            }
            let text = std::str::from_utf8(&data)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            decode(text, self.max_items).map(Some)
        }
        fn save(&self, agenda: &Agenda) -> io::Result<()> {
            self.commit(agenda, None)
        }
        fn apply(&self, previous: &Agenda, mutation: &AgendaMutation) -> io::Result<Agenda> {
            let next = mutation.next(previous)?;
            self.commit(&next, Some((previous, mutation)))?;
            Ok(next)
        }
    }
    fn lock(dir: &OwnedFd) -> io::Result<OwnedFd> {
        let fd = fs::openat(
            dir,
            ".hud-data.lock",
            OFlags::RDWR | OFlags::CREATE | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
            Mode::from_raw_mode(0o600),
        )?;
        private(&fd, false)?;
        if fs::fstat(&fd)?.st_nlink != 1 {
            return Err(invalid("state lock has multiple links"));
        }
        fs::flock(&fd, fs::FlockOperation::NonBlockingLockExclusive)?;
        Ok(fd)
    }
    type Fingerprint = (u64, u64, u64, i64, i64, i64, i64, u32, u32, u64);
    fn fingerprint(metadata: &std::fs::Metadata) -> Fingerprint {
        (
            metadata.dev(),
            metadata.ino(),
            metadata.len(),
            metadata.mtime(),
            metadata.mtime_nsec(),
            metadata.ctime(),
            metadata.ctime_nsec(),
            metadata.mode(),
            metadata.uid(),
            metadata.nlink(),
        )
    }
    #[derive(Eq, PartialEq)]
    struct RawSnapshot {
        text: String,
        fingerprint: Fingerprint,
    }
    fn read_snapshot(dir: &OwnedFd, name: &str) -> io::Result<Option<RawSnapshot>> {
        let fd = match fs::openat(
            dir,
            name,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
            Mode::empty(),
        ) {
            Ok(fd) => fd,
            Err(rustix::io::Errno::NOENT) => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        private(&fd, false)?;
        let mut file = File::from(fd);
        let before = fingerprint(&file.metadata()?);
        if before.2 > MAX_AGENDA_BYTES as u64 {
            return Err(invalid("state exceeds byte cap"));
        }
        let mut data = Vec::new();
        (&mut file)
            .take(MAX_AGENDA_BYTES as u64 + 1)
            .read_to_end(&mut data)?;
        if data.len() > MAX_AGENDA_BYTES || fingerprint(&file.metadata()?) != before {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "state changed during read or exceeded cap",
            ));
        }
        let text =
            String::from_utf8(data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        Ok(Some(RawSnapshot {
            text,
            fingerprint: before,
        }))
    }
    fn read_raw(dir: &OwnedFd, name: &str) -> io::Result<Option<String>> {
        Ok(read_snapshot(dir, name)?.map(|snapshot| snapshot.text))
    }
    fn same_directory(state: &FileAgendaStore, pinned: &OwnedFd) -> io::Result<()> {
        let current = state.directory(false)?;
        let first = fs::fstat(pinned)?;
        let next = fs::fstat(&current)?;
        if first.st_dev != next.st_dev || first.st_ino != next.st_ino {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "state directory changed before commit",
            ));
        }
        Ok(())
    }
    impl FileAgendaStore {
        fn commit(
            &self,
            agenda: &Agenda,
            edit: Option<(&Agenda, &AgendaMutation)>,
        ) -> io::Result<()> {
            let encoded = encode(agenda, self.max_items)?;
            let dir = self.directory(true)?;
            let _lock = lock(&dir)?;
            let original = read_snapshot(&dir, NAME)?;
            let text = if let Some(snapshot) = &original {
                let raw = &snapshot.text;
                let previous = decode(raw, self.max_items)?;
                let mutation = if let Some((expected, mutation)) = edit {
                    if previous.items != expected.items {
                        return Err(io::Error::new(
                            io::ErrorKind::WouldBlock,
                            "agenda changed on disk; refresh before selecting",
                        ));
                    }
                    Some(mutation.clone())
                } else {
                    infer_mutation(&previous, agenda)?
                };
                match mutation {
                    Some(mutation) => patch_agenda(raw, &previous, &mutation)?,
                    None => raw.clone(),
                }
            } else {
                if edit.is_some_and(|(expected, _)| !expected.items.is_empty()) {
                    return Err(io::Error::new(
                        io::ErrorKind::WouldBlock,
                        "agenda disappeared; refresh before selecting",
                    ));
                }
                let _ = encoded; // validation above; initial identity order must match application.
                format!(
                    "# Agenda (HUD)\n\n{}\n",
                    agenda
                        .items
                        .iter()
                        .map(super::super::Item::to_line)
                        .collect::<Vec<_>>()
                        .join("\n")
                )
            };
            if text.len() > MAX_AGENDA_BYTES || decode(&text, self.max_items)?.items != agenda.items
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "patched agenda exceeds limits or changes identity",
                ));
            }
            match fs::statat(&dir, NAME, AtFlags::SYMLINK_NOFOLLOW) {
                Ok(st)
                    if FileType::from_raw_mode(st.st_mode) != FileType::RegularFile
                        || st.st_uid != rustix::process::getuid().as_raw()
                        || st.st_mode & 0o077 != 0 =>
                {
                    return Err(invalid("existing state is not a private regular file"));
                }
                Ok(_) | Err(rustix::io::Errno::NOENT) => (),
                Err(e) => return Err(e.into()),
            }
            let temporary = format!(
                ".agenda-{}-{}.tmp",
                std::process::id(),
                SEQUENCE.fetch_add(1, Ordering::Relaxed)
            );
            let fd = fs::openat(
                &dir,
                temporary.as_str(),
                OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::from_raw_mode(0o600),
            )?;
            let result = (|| {
                let mut file = File::from(fd);
                file.write_all(text.as_bytes())?;
                file.sync_all()?;
                before_commit()?;
                same_directory(self, &dir)?;
                if read_snapshot(&dir, NAME)? != original {
                    return Err(io::Error::new(
                        io::ErrorKind::WouldBlock,
                        "agenda changed before commit; manual edits preserved",
                    ));
                }
                fs::renameat(&dir, temporary.as_str(), &dir, NAME)?;
                fs::fsync(&dir)?;
                Ok(())
            })();
            if result.is_err() {
                let _ = fs::unlinkat(&dir, temporary.as_str(), AtFlags::empty());
            }
            result
        }
    }
    fn folder_value(snapshot: Option<&RawSnapshot>) -> io::Result<Option<PathBuf>> {
        let Some(snapshot) = snapshot else {
            return Ok(None);
        };
        if snapshot.text.len() > 4096 {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let value: Option<String> =
            serde_json::from_str(&snapshot.text).map_err(|_| io::ErrorKind::InvalidData)?;
        match value {
            None => Ok(None),
            Some(value)
                if Path::new(&value).is_absolute()
                    && !value.chars().any(char::is_control)
                    && !Path::new(&value)
                        .components()
                        .any(|c| matches!(c, Component::ParentDir)) =>
            {
                Ok(Some(value.into()))
            }
            _ => Err(io::ErrorKind::InvalidData.into()),
        }
    }
    impl FolderSelectionStore for FileFolderSelectionStore {
        fn load(&self) -> io::Result<Option<PathBuf>> {
            let directory = match self.state.directory(false) {
                Ok(dir) => dir,
                Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
                Err(e) => return Err(e),
            };
            folder_value(read_snapshot(&directory, "pasta.state")?.as_ref())
        }
        fn save(&self, previous: Option<&Path>, next: Option<&Path>) -> io::Result<()> {
            if next.is_some_and(|path| {
                !path.is_absolute()
                    || path.components().any(|c| matches!(c, Component::ParentDir))
                    || path
                        .to_str()
                        .is_none_or(|s| s.chars().any(char::is_control))
            }) {
                return Err(io::ErrorKind::InvalidInput.into());
            }
            let next = next
                .map(|path| path.to_str().ok_or(io::ErrorKind::InvalidInput))
                .transpose()?;
            let text = serde_json::to_string(&next).map_err(io::Error::other)?;
            if text.len() > 4096 {
                return Err(io::ErrorKind::InvalidInput.into());
            }
            let dir = self.state.directory(true)?;
            let _lock = lock(&dir)?;
            let original = read_snapshot(&dir, "pasta.state")?;
            if folder_value(original.as_ref())?.as_deref() != previous {
                return Err(io::ErrorKind::WouldBlock.into());
            }
            let temporary = format!(
                ".hud-folder-{}-{}.tmp",
                std::process::id(),
                SEQUENCE.fetch_add(1, Ordering::Relaxed)
            );
            let fd = fs::openat(
                &dir,
                temporary.as_str(),
                OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::from_raw_mode(0o600),
            )?;
            let result = (|| {
                let mut file = File::from(fd);
                file.write_all(text.as_bytes())?;
                file.sync_all()?;
                before_commit()?;
                same_directory(&self.state, &dir)?;
                if read_snapshot(&dir, "pasta.state")? != original {
                    return Err(io::ErrorKind::WouldBlock.into());
                }
                fs::renameat(&dir, temporary.as_str(), &dir, "pasta.state")?;
                fs::fsync(&dir)?;
                Ok(())
            })();
            if result.is_err() {
                let _ = fs::unlinkat(&dir, temporary.as_str(), AtFlags::empty());
            }
            result
        }
    }
    impl LocalNotesStore for FileLocalNotesStore {
        fn append(&self, stamp: &str, text: &str) -> io::Result<()> {
            if stamp.len() != 16
                || !stamp.is_ascii()
                || super::super::Date::parse(&stamp[..10]).is_none()
                || &stamp[10..11] != " "
                || super::super::parse_time(&stamp[11..])
                    .ok()
                    .flatten()
                    .as_deref()
                    != Some(&stamp[11..])
                || text.trim().is_empty()
                || text.len() > 4096
                || super::super::clean_line(text) != text
                || text.contains(['\n', '\r'])
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "invalid note text or timestamp",
                ));
            }
            let dir = self.state.directory(true)?;
            let _lock = lock(&dir)?;
            let fd = fs::openat(
                &dir,
                "notas.md",
                OFlags::RDWR
                    | OFlags::APPEND
                    | OFlags::CREATE
                    | OFlags::NOFOLLOW
                    | OFlags::CLOEXEC
                    | OFlags::NONBLOCK,
                Mode::from_raw_mode(0o600),
            )?;
            private(&fd, false)?;
            let stat = fs::fstat(&fd)?;
            if stat.st_nlink != 1 || stat.st_size > MAX_AGENDA_BYTES as i64 {
                return Err(invalid("notes must be bounded and have one link"));
            }
            let mut file = File::from(fd);
            let mut original = Vec::new();
            (&mut file)
                .take(MAX_AGENDA_BYTES as u64 + 1)
                .read_to_end(&mut original)?;
            if original.len() > MAX_AGENDA_BYTES {
                return Err(invalid("notes grew beyond cap"));
            }
            let raw = std::str::from_utf8(&original)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            let eol = if raw.contains("\r\n") { "\r\n" } else { "\n" };
            let separator = if !original.is_empty() && !original.ends_with(b"\n") {
                eol
            } else {
                ""
            };
            let addition = format!("{separator}- {stamp} {text}{eol}");
            if original.len() + addition.len() > MAX_AGENDA_BYTES {
                return Err(invalid("notes byte cap exceeded"));
            }
            let observed = fingerprint(&file.metadata()?);
            before_commit()?;
            same_directory(&self.state, &dir)?;
            let current =
                read_snapshot(&dir, "notas.md")?.ok_or_else(|| invalid("notes disappeared"))?;
            if current.fingerprint != observed || current.text.as_bytes() != original {
                return Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "notes changed before append; nothing appended",
                ));
            }
            file.seek(SeekFrom::End(0))?;
            file.write_all(addition.as_bytes())?;
            file.sync_all()?;
            fs::fsync(&dir)?;
            Ok(())
        }
        fn recent(&self, count: usize) -> io::Result<String> {
            if count == 0 || count > 1000 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "note count must be 1..1000",
                ));
            }
            let dir = match self.state.directory(false) {
                Ok(dir) => dir,
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    return Ok("nenhuma nota ainda".into())
                }
                Err(e) => return Err(e),
            };
            let raw = read_raw(&dir, "notas.md")?.unwrap_or_default();
            let lines: Vec<_> = raw.lines().collect();
            if lines.is_empty() {
                return Ok("nenhuma nota ainda".into());
            }
            Ok(lines[lines.len().saturating_sub(count)..]
                .iter()
                .map(|line| super::super::clean_line(line))
                .collect::<Vec<_>>()
                .join("\n"))
        }
    }
}
#[cfg(not(unix))]
impl AgendaStore for FileAgendaStore {
    fn load(&self) -> io::Result<Option<Agenda>> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "secure state storage is currently POSIX-only",
        ))
    }
    fn save(&self, _: &Agenda) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "secure state storage is currently POSIX-only",
        ))
    }
}

#[cfg(not(unix))]
impl LocalNotesStore for FileLocalNotesStore {
    fn append(&self, _: &str, _: &str) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "secure notes storage is POSIX-only",
        ))
    }
    fn recent(&self, _: usize) -> io::Result<String> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "secure notes storage is POSIX-only",
        ))
    }
}
#[cfg(not(unix))]
impl FolderSelectionStore for FileFolderSelectionStore {
    fn load(&self) -> io::Result<Option<PathBuf>> {
        Err(io::ErrorKind::Unsupported.into())
    }
    fn save(&self, _: Option<&Path>, _: Option<&Path>) -> io::Result<()> {
        Err(io::ErrorKind::Unsupported.into())
    }
}
#[cfg(unix)]
fn infer_mutation(previous: &Agenda, next: &Agenda) -> io::Result<Option<AgendaMutation>> {
    if previous.items == next.items {
        return Ok(None);
    }
    if next.items.len() == previous.items.len() + 1
        && next.items[..previous.items.len()] == previous.items
    {
        return Ok(Some(AgendaMutation::Add(
            next.items.last().unwrap().clone(),
        )));
    }
    let candidates: Vec<_> = (0..previous.items.len())
        .flat_map(|index| [AgendaMutation::Done(index), AgendaMutation::Remove(index)])
        .filter(|change| {
            change
                .next(previous)
                .is_ok_and(|candidate| candidate.items == next.items)
        })
        .collect();
    if candidates.len() == 1 {
        return Ok(candidates.into_iter().next());
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        "ambiguous agenda edit; use revision-scoped identity",
    ))
}
#[cfg(unix)]
fn patch_agenda(raw: &str, previous: &Agenda, mutation: &AgendaMutation) -> io::Result<String> {
    let mut result = String::new();
    let mut task_index = 0;
    for line in raw.split_inclusive('\n') {
        let items = super::parse_agenda(line);
        if items.is_empty() {
            result.push_str(line);
            continue;
        }
        if items.len() != 1 || previous.items.get(task_index) != items.first() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "agenda line identity mismatch",
            ));
        }
        match mutation {
            AgendaMutation::Remove(index) if *index == task_index => {}
            AgendaMutation::Done(index) if *index == task_index => {
                result.push_str(&line.replacen("- [ ]", "- [x]", 1));
            }
            _ => result.push_str(line),
        }
        task_index += 1;
    }
    if task_index != previous.items.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "agenda revision mismatch",
        ));
    }
    if let AgendaMutation::Add(item) = mutation {
        let eol = if raw.contains("\r\n") { "\r\n" } else { "\n" };
        if !result.is_empty() && !result.ends_with('\n') {
            result.push_str(eol);
        }
        result.push_str(&item.to_line());
        result.push_str(eol);
    }
    Ok(result)
}
pub const MAX_AGENDA_BYTES: usize = 256 * 1024;
pub const MAX_AGENDA_ITEMS: usize = 2048;
fn encode(agenda: &Agenda, maximum: usize) -> io::Result<String> {
    if agenda.items.len() > maximum {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "agenda item limit exceeded",
        ));
    }
    for item in &agenda.items {
        if !item.source.is_empty()
            || item.text.trim().is_empty()
            || item.text.chars().count() > 300
            || super::clean_line(&item.text) != item.text
            || item.text.contains(['\n', '\r'])
            || item
                .time
                .as_ref()
                .is_some_and(|time| super::parse_time(time).ok().flatten().as_ref() != Some(time))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid local agenda item",
            ));
        }
    }
    let text = agenda.to_markdown();
    if text.len() > MAX_AGENDA_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "agenda byte cap exceeded",
        ));
    }
    Ok(text)
}
fn decode(text: &str, maximum: usize) -> io::Result<Agenda> {
    if text.len() > MAX_AGENDA_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "agenda byte cap exceeded",
        ));
    }
    // Preserve Python's comment/prose tolerance, but never silently discard malformed task lines.
    for line in text
        .lines()
        .filter(|line| line.trim_start().starts_with("- ["))
    {
        if super::parse_agenda(line).len() != 1 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "malformed agenda task line",
            ));
        }
    }
    let agenda = Agenda::load_text(text);
    encode(&agenda, maximum).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    Ok(agenda)
}
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{
        fs,
        io::Read,
        os::unix::fs::{symlink, PermissionsExt},
        sync::atomic::{AtomicU64, Ordering},
    };

    #[test]
    fn folder_selection_private_cas_and_manual_edit_refusal() {
        let temp = Temp::new();
        let store = FileFolderSelectionStore::new(&temp.0).unwrap();
        assert_eq!(store.load().unwrap(), None);
        store
            .save(None, Some(Path::new("/tmp/fixture-one")))
            .unwrap();
        assert_eq!(
            store.load().unwrap(),
            Some(PathBuf::from("/tmp/fixture-one"))
        );
        assert!(store
            .save(None, Some(Path::new("/tmp/fixture-two")))
            .is_err());
        assert!(store
            .save(
                Some(Path::new("/tmp/fixture-one")),
                Some(Path::new("relative"))
            )
            .is_err());
        let file = temp.0.join("pasta.state");
        let changed = file.clone();
        unix::BEFORE_COMMIT.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(move || {
                fs::write(changed, b"\"/tmp/manual\"").unwrap();
            }));
        });
        assert!(store
            .save(
                Some(Path::new("/tmp/fixture-one")),
                Some(Path::new("/tmp/fixture-two"))
            )
            .is_err());
        assert_eq!(store.load().unwrap(), Some(PathBuf::from("/tmp/manual")));
        store.save(Some(Path::new("/tmp/manual")), None).unwrap();
        assert_eq!(store.load().unwrap(), None);
        assert_eq!(
            fs::metadata(&file).unwrap().permissions().mode() & 0o777,
            0o600
        );
        fs::remove_file(&file).unwrap();
        symlink("/tmp/never-followed", &file).unwrap();
        assert!(store.load().is_err());
    }
    static NEXT: AtomicU64 = AtomicU64::new(0);
    #[test]
    fn p0_agenda_mutation_preserves_frontmatter_prose_and_crlf() {
        let temp = Temp::new();
        let path = temp.0.join("agenda.md");
        let raw = "---\r\ntitle: Café 日本語\r\n---\r\nTexto livre\r\n- [ ] 2026-10-10 ação\r\n# rodapé\r\n";
        fs::write(&path, raw).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let store = FileAgendaStore::new(&temp.0, MAX_AGENDA_ITEMS).unwrap();
        let mut agenda = store.load().unwrap().unwrap();
        agenda.mark_done(0);
        store.save(&agenda).unwrap();
        assert_eq!(
            fs::read_to_string(path).unwrap(),
            raw.replace("- [ ]", "- [x]")
        );
    }
    #[test]
    fn p0_notes_append_preserves_raw_frontmatter_tasks_unicode_and_missing_eol() {
        let temp = Temp::new();
        let path = temp.0.join("notas.md");
        let raw =
            "---\r\ntitle: Notas café\r\n---\r\nprosa 日本語\r\n- [ ] tarefa manual\r\nsem newline";
        fs::write(&path, raw).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let store = FileLocalNotesStore::new(&temp.0).unwrap();
        store
            .append("2026-10-10 14:30", "quero ação 🐱 日本語")
            .unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            format!("{raw}\r\n- 2026-10-10 14:30 quero ação 🐱 日本語\r\n")
        );
        assert_eq!(
            store.recent(1).unwrap(),
            "- 2026-10-10 14:30 quero ação 🐱 日本語"
        );
        let before = fs::read(&path).unwrap();
        assert!(store.append("2026-10-10 25:00", "x").is_err());
        assert!(store.append("2026-10-10 14:30", "bad\ntext").is_err());
        assert!(store.recent(0).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
    }
    #[test]
    fn p0_notes_links_and_replacement_before_append_are_refused() {
        let temp = Temp::new();
        let target = temp.0.join("untouched.md");
        fs::write(&target, "fixture").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
        symlink(&target, temp.0.join("notas.md")).unwrap();
        let store = FileLocalNotesStore::new(&temp.0).unwrap();
        assert!(store.append("2026-10-10 14:30", "ação").is_err());
        assert_eq!(fs::read_to_string(&target).unwrap(), "fixture");
        fs::remove_file(temp.0.join("notas.md")).unwrap();
        fs::write(temp.0.join("notas.md"), "original\n").unwrap();
        fs::set_permissions(temp.0.join("notas.md"), fs::Permissions::from_mode(0o600)).unwrap();
        let root = temp.0.clone();
        unix::BEFORE_COMMIT.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(move || {
                fs::rename(root.join("notas.md"), root.join("old.md")).unwrap();
                fs::write(root.join("notas.md"), "manual\n").unwrap();
                fs::set_permissions(root.join("notas.md"), fs::Permissions::from_mode(0o600))
                    .unwrap();
            }))
        });
        assert!(store.append("2026-10-10 14:30", "ação").is_err());
        assert_eq!(
            fs::read_to_string(temp.0.join("old.md")).unwrap(),
            "original\n"
        );
        assert_eq!(
            fs::read_to_string(temp.0.join("notas.md")).unwrap(),
            "manual\n"
        );
    }
    #[test]
    fn p0_agenda_duplicate_occurrence_and_stale_disk_preserve_text() {
        let temp = Temp::new();
        let path = temp.0.join("agenda.md");
        let raw="---\ntitle: ação\n---\nprosa\n- [ ] 2026-10-10 duplicada\n\n- [ ] 2026-10-10 duplicada\nrodapé\n";
        fs::write(&path, raw).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let store = FileAgendaStore::new(&temp.0, MAX_AGENDA_ITEMS).unwrap();
        let previous = store.load().unwrap().unwrap();
        store.apply(&previous, &AgendaMutation::Remove(1)).unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "---\ntitle: ação\n---\nprosa\n- [ ] 2026-10-10 duplicada\n\nrodapé\n"
        );
        let before = fs::read(&path).unwrap();
        assert!(store.apply(&previous, &AgendaMutation::Done(0)).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
    }
    #[test]
    fn p0_agenda_manual_edit_during_tempwrite_is_never_overwritten() {
        let temp = Temp::new();
        let path = temp.0.join("agenda.md");
        fs::write(&path, "prosa\n- [ ] 2026-10-10 ação\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let store = FileAgendaStore::new(&temp.0, MAX_AGENDA_ITEMS).unwrap();
        let previous = store.load().unwrap().unwrap();
        let target = path.clone();
        unix::BEFORE_COMMIT.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(move || {
                fs::write(target, "edição manual\n- [ ] 2026-10-10 ação\n").unwrap()
            }))
        });
        assert!(store.apply(&previous, &AgendaMutation::Done(0)).is_err());
        assert_eq!(
            fs::read_to_string(path).unwrap(),
            "edição manual\n- [ ] 2026-10-10 ação\n"
        );
        assert!(!fs::read_dir(&temp.0).unwrap().any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp")));
    }
    #[test]
    fn p0_agenda_directory_move_and_final_byte_limit_are_refused() {
        let temp = Temp::new();
        let dir = temp.0.join("data");
        fs::create_dir(&dir).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
        let store = FileAgendaStore::new(&dir, MAX_AGENDA_ITEMS).unwrap();
        let original = Agenda::load_text("- [ ] 2026-10-10 ação\n");
        store.save(&original).unwrap();
        let root = temp.0.clone();
        unix::BEFORE_COMMIT.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(move || {
                fs::rename(root.join("data"), root.join("moved")).unwrap();
                fs::create_dir(root.join("data")).unwrap();
                fs::set_permissions(root.join("data"), fs::Permissions::from_mode(0o700)).unwrap();
            }))
        });
        assert!(store.apply(&original, &AgendaMutation::Done(0)).is_err());
        assert!(!dir.join("agenda.md").exists());
        assert!(!fs::read_to_string(temp.0.join("moved/agenda.md"))
            .unwrap()
            .contains("[x]"));
        let raw = format!(
            "# {}\n- [ ] 2026-10-10 ação\n",
            "a".repeat(MAX_AGENDA_BYTES - 50)
        );
        let path = dir.join("agenda.md");
        fs::write(&path, &raw).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let previous = store.load().unwrap().unwrap();
        let item = super::super::parse_entry(
            "2026-10-11 texto muito longo para caber neste arquivo",
            super::super::Date::parse("2026-10-10").unwrap(),
        )
        .unwrap();
        assert!(store.apply(&previous, &AgendaMutation::Add(item)).is_err());
        assert_eq!(fs::read_to_string(path).unwrap(), raw);
    }
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "hud-agenda-store-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
            Self(path)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn agenda() -> Agenda {
        Agenda {
            items: vec![super::super::parse_entry(
                "hoje 08h café 日本語",
                super::super::Date::new(2026, 10, 10).unwrap(),
            )
            .unwrap()],
        }
    }
    #[test]
    fn missing_load_roundtrip_permissions() {
        let temp = Temp::new();
        let dir = temp.0.join("data");
        let store = FileAgendaStore::new(&dir, MAX_AGENDA_ITEMS).unwrap();
        assert!(store.load().unwrap().is_none());
        assert!(!dir.exists());
        store.save(&agenda()).unwrap();
        assert_eq!(store.load().unwrap().unwrap().items, agenda().items);
        assert_eq!(
            fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(dir.join("agenda.md"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(fs::read_dir(dir).unwrap().count(), 2); // agenda + private coordination lock
    }
    #[test]
    fn symlink_chain_and_final_file_refused() {
        let temp = Temp::new();
        let real = temp.0.join("real");
        fs::create_dir(&real).unwrap();
        fs::set_permissions(&real, fs::Permissions::from_mode(0o700)).unwrap();
        symlink(&real, temp.0.join("link")).unwrap();
        let parent = FileAgendaStore::new(temp.0.join("link/data"), MAX_AGENDA_ITEMS).unwrap();
        assert!(parent.save(&agenda()).is_err());
        assert!(!real.join("data").exists());
        let store = FileAgendaStore::new(&real, MAX_AGENDA_ITEMS).unwrap();
        let victim = temp.0.join("victim");
        fs::write(&victim, "unchanged").unwrap();
        symlink(&victim, real.join("agenda.md")).unwrap();
        assert!(store.load().is_err());
        assert!(store.save(&agenda()).is_err());
        assert_eq!(fs::read_to_string(victim).unwrap(), "unchanged");
    }
    #[test]
    fn invalid_utf8_task_oversize_and_public_refused() {
        let temp = Temp::new();
        let store = FileAgendaStore::new(&temp.0, MAX_AGENDA_ITEMS).unwrap();
        let file = temp.0.join("agenda.md");
        for bytes in [
            vec![0xff],
            b"- [ ] invalid date".to_vec(),
            vec![b'a'; MAX_AGENDA_BYTES + 1],
        ] {
            fs::write(&file, bytes).unwrap();
            fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
            assert_eq!(store.load().unwrap_err().kind(), io::ErrorKind::InvalidData);
        }
        fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(store.load().is_err());
        assert!(store.save(&agenda()).is_err());
    }
    #[test]
    fn invalid_input_does_not_create_or_replace() {
        let temp = Temp::new();
        let dir = temp.0.join("data");
        let store = FileAgendaStore::new(&dir, 1).unwrap();
        let mut invalid = agenda();
        invalid.items[0].text = "unsafe\x1b[31m".into();
        assert!(store.save(&invalid).is_err());
        assert!(!dir.exists());
        store.save(&agenda()).unwrap();
        let before = fs::read(dir.join("agenda.md")).unwrap();
        invalid = agenda();
        invalid.items.push(invalid.items[0].clone());
        assert!(store.save(&invalid).is_err());
        assert_eq!(fs::read(dir.join("agenda.md")).unwrap(), before);
        assert!(FileAgendaStore::new(temp.0.join("../escape"), 1).is_err());
        assert!(FileAgendaStore::new(&temp.0, 0).is_err());
    }
    #[test]
    fn failed_commit_keeps_previous_file_and_cleans_temporary() {
        let temp = Temp::new();
        let store = FileAgendaStore::new(&temp.0, MAX_AGENDA_ITEMS).unwrap();
        store.save(&agenda()).unwrap();
        let mut changed = agenda();
        changed.items[0].done = true;
        unix::FAIL_COMMIT.with(|flag| flag.set(true));
        assert!(store.save(&changed).is_err());
        assert_eq!(store.load().unwrap().unwrap().items, agenda().items);
        assert_eq!(fs::read_dir(&temp.0).unwrap().count(), 2); // agenda + private coordination lock
        store.save(&changed).unwrap();
        assert_eq!(store.load().unwrap().unwrap().items, changed.items);
    }
    #[test]
    fn fifo_and_nonprivate_directory_are_refused() {
        let temp = Temp::new();
        let store = FileAgendaStore::new(&temp.0, MAX_AGENDA_ITEMS).unwrap();
        rustix::fs::mkfifoat(
            rustix::fs::CWD,
            temp.0.join("agenda.md"),
            rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
        )
        .unwrap();
        assert!(store.load().is_err());
        assert!(store.save(&agenda()).is_err());
        fs::remove_file(temp.0.join("agenda.md")).unwrap();
        fs::set_permissions(&temp.0, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(store.load().is_err());
        assert!(store.save(&agenda()).is_err());
        assert_eq!(fs::read_dir(&temp.0).unwrap().count(), 1); // lock created before refusing FIFO
    }
    #[test]
    fn writable_nonsticky_ancestor_is_refused() {
        let temp = Temp::new();
        fs::set_permissions(&temp.0, fs::Permissions::from_mode(0o777)).unwrap();
        let store = FileAgendaStore::new(temp.0.join("data"), MAX_AGENDA_ITEMS).unwrap();
        assert!(store.load().is_err());
        assert!(store.save(&agenda()).is_err());
        assert!(!temp.0.join("data").exists());
    }
    #[test]
    fn atomic_replace_preserves_open_reader() {
        let temp = Temp::new();
        let store = FileAgendaStore::new(&temp.0, MAX_AGENDA_ITEMS).unwrap();
        store.save(&agenda()).unwrap();
        let mut old = fs::File::open(temp.0.join("agenda.md")).unwrap();
        let mut changed = agenda();
        changed.items[0].done = true;
        store.save(&changed).unwrap();
        let mut oldtext = String::new();
        old.read_to_string(&mut oldtext).unwrap();
        assert_eq!(Agenda::load_text(&oldtext).items, agenda().items);
        assert_eq!(store.load().unwrap().unwrap().items, changed.items);
    }
}
