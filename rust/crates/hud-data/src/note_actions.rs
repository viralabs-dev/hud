//! Explicit note reads and separately approved task changes; never an implicit write.
use crate::clean_line;
use std::{
    io,
    path::{Component, Path, PathBuf},
};
const MAX_NOTE: usize = 2 * 1024 * 1024;
#[derive(Clone, Debug)]
pub struct NoteRead {
    rel: String,
    bytes: Vec<u8>,
    #[cfg(unix)]
    stamp: Stamp,
    #[cfg(unix)]
    chain_identity: Vec<(u64, u64)>,
}
impl NoteRead {
    pub fn path(&self) -> &str {
        &self.rel
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn display(&self) -> String {
        self.bytes
            .split(|b| *b == b'\n')
            .map(|line| clean_line(&String::from_utf8_lossy(line)))
            .collect::<Vec<_>>()
            .join("\n")
    }
}
#[derive(Clone, Debug)]
pub struct NoteChange {
    original: NoteRead,
    proposed: Vec<u8>,
}
impl NoteChange {
    pub fn original(&self) -> &NoteRead {
        &self.original
    }
    pub fn proposed(&self) -> &[u8] {
        &self.proposed
    }
}
pub struct NoteStore {
    root: PathBuf,
}
impl NoteStore {
    pub fn new(root: impl AsRef<Path>) -> io::Result<Self> {
        let root = root.as_ref();
        if root.as_os_str().is_empty()
            || root.components().any(|c| matches!(c, Component::ParentDir))
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let root = if root.is_absolute() {
            root.to_owned()
        } else {
            std::env::current_dir()?.join(root)
        };
        #[cfg(unix)]
        {
            let _ = crate::scanner::open_root(&root)?;
        }
        Ok(Self { root })
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn preview_read(&self, rel: &str) -> io::Result<NoteRead> {
        valid_rel(rel)?;
        #[cfg(unix)]
        {
            read_note(&self.root, rel)
        }
        #[cfg(not(unix))]
        {
            Err(io::ErrorKind::Unsupported.into())
        }
    }
    /// One-based physical line; only its checkbox byte changes. UTF-8, line
    /// endings, wikilinks, aliases and every unrelated byte remain untouched.
    pub fn preview_toggle(&self, rel: &str, line: usize, done: bool) -> io::Result<NoteChange> {
        let original = self.preview_read(rel)?;
        std::str::from_utf8(&original.bytes).map_err(|_| io::ErrorKind::InvalidData)?;
        if line == 0 {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let mut offset = 0;
        for (index, raw) in original.bytes.split_inclusive(|b| *b == b'\n').enumerate() {
            if index + 1 == line {
                let indent = raw.iter().take_while(|b| matches!(b, b' ' | b'\t')).count();
                let task = &raw[indent..];
                if task.len() < 6
                    || &task[..3] != b"- ["
                    || task[4] != b']'
                    || task[5] != b' '
                    || !matches!(task[3], b' ' | b'x' | b'X')
                {
                    return Err(io::ErrorKind::InvalidInput.into());
                }
                let mut proposed = original.bytes.clone();
                proposed[offset + indent + 3] = if done { b'x' } else { b' ' };
                return Ok(NoteChange { original, proposed });
            }
            offset += raw.len();
        }
        Err(io::ErrorKind::InvalidInput.into())
    }
    /// Explicit approval boundary. A read/preview cannot invoke this operation.
    pub fn apply_toggle(&self, change: &NoteChange) -> io::Result<()> {
        #[cfg(unix)]
        {
            apply(&self.root, change)
        }
        #[cfg(not(unix))]
        {
            let _ = change;
            Err(io::ErrorKind::Unsupported.into())
        }
    }
}
fn valid_rel(rel: &str) -> io::Result<()> {
    let p = Path::new(rel);
    let lower = rel.to_lowercase();
    if rel.is_empty()
        || rel.len() > 4096
        || p.components().count() > 64
        || p.is_absolute()
        || ![".md", ".markdown", ".txt"]
            .iter()
            .any(|ext| lower.ends_with(ext))
        || p.components()
            .any(|c| !matches!(c, Component::Normal(n) if !n.to_string_lossy().starts_with('.')))
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    Ok(())
}
#[cfg(unix)]
use rustix::fs::{self, AtFlags, FileType, Mode, OFlags};
#[cfg(unix)]
use std::{
    fs::File,
    io::{Read, Write},
    os::fd::OwnedFd,
    sync::atomic::{AtomicU64, Ordering},
};
#[cfg(unix)]
#[derive(Clone, Debug, Eq, PartialEq)]
struct Stamp {
    dev: u64,
    ino: u64,
    size: i64,
    modified: (i64, i64),
    changed: (i64, i64),
    mode: u32,
    uid: u32,
}
#[cfg(unix)]
#[allow(clippy::unnecessary_cast)] // Unix Stat types vary.
fn stamp(s: &fs::Stat) -> Stamp {
    Stamp {
        dev: s.st_dev as u64,
        ino: s.st_ino as u64,
        size: s.st_size,
        modified: (s.st_mtime, s.st_mtime_nsec as i64),
        changed: (s.st_ctime, s.st_ctime_nsec as i64),
        mode: s.st_mode,
        uid: s.st_uid,
    }
}
#[cfg(unix)]
type ChainIdentity = Vec<(u64, u64)>;
#[cfg(unix)]
#[allow(clippy::unnecessary_cast)] // Unix Stat types vary.
fn parent(root: &Path, rel: &str) -> io::Result<(OwnedFd, String, ChainIdentity)> {
    valid_rel(rel)?;
    let names: Vec<_> = Path::new(rel)
        .components()
        .filter_map(|c| {
            if let Component::Normal(n) = c {
                Some(n)
            } else {
                None
            }
        })
        .collect();
    let (name, dirs) = names.split_last().ok_or(io::ErrorKind::InvalidInput)?;
    let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let mut fd = fs::open("/", flags, Mode::empty())?;
    let st = fs::fstat(&fd)?;
    let mut chain = vec![(st.st_dev as u64, st.st_ino as u64)];
    for component in root.components() {
        if let Component::Normal(dir) = component {
            fd = fs::openat(&fd, dir, flags, Mode::empty())?;
            let st = fs::fstat(&fd)?;
            chain.push((st.st_dev as u64, st.st_ino as u64));
        }
    }
    for dir in dirs {
        fd = fs::openat(&fd, *dir, flags, Mode::empty())?;
        let st = fs::fstat(&fd)?;
        chain.push((st.st_dev as u64, st.st_ino as u64));
    }
    Ok((fd, name.to_string_lossy().into(), chain))
}
#[cfg(unix)]
#[allow(clippy::unnecessary_cast)] // Unix Stat types vary.
fn read_at(dir: &OwnedFd, rel: &str, name: &str, chain: &ChainIdentity) -> io::Result<NoteRead> {
    let before = fs::statat(dir, name, AtFlags::SYMLINK_NOFOLLOW)?;
    let fd = fs::openat(
        dir,
        name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )?;
    let st = fs::fstat(&fd)?;
    if FileType::from_raw_mode(st.st_mode) != FileType::RegularFile
        || st.st_uid != rustix::process::getuid().as_raw()
        || st.st_dev != before.st_dev
        || st.st_ino != before.st_ino
        || st.st_size < 0
        || st.st_size as u64 > MAX_NOTE as u64
    {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    let mut file = File::from(fd);
    let mut bytes = Vec::new();
    (&mut file)
        .take(MAX_NOTE as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_NOTE || stamp(&fs::fstat(&file)?) != stamp(&st) {
        return Err(io::ErrorKind::InvalidData.into());
    }
    Ok(NoteRead {
        rel: rel.into(),
        bytes,
        stamp: stamp(&st),
        chain_identity: chain.clone(),
    })
}
#[cfg(unix)]
fn read_note(root: &Path, rel: &str) -> io::Result<NoteRead> {
    let (dir, name, chain) = parent(root, rel)?;
    read_at(&dir, rel, &name, &chain)
}
#[cfg(all(test, unix))]
thread_local! { static BEFORE_COMMIT: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = std::cell::RefCell::new(None); }
#[cfg(unix)]
fn before_commit() {
    #[cfg(test)]
    BEFORE_COMMIT.with(|hook| {
        if let Some(action) = hook.borrow_mut().take() {
            action();
        }
    });
}
#[cfg(unix)]
fn apply(root: &Path, change: &NoteChange) -> io::Result<()> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let (dir, name, chain) = parent(root, change.original.path())?;
    let directory = fs::fstat(&dir)?;
    if directory.st_uid != rustix::process::getuid().as_raw()
        || directory.st_mode & 0o022 != 0
        || change.original.stamp.mode & 0o022 != 0
    {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    let current = read_at(&dir, change.original.path(), &name, &chain)?;
    if current.stamp != change.original.stamp
        || current.bytes != change.original.bytes
        || current.chain_identity != change.original.chain_identity
    {
        return Err(io::Error::other(
            "note changed since preview; approval discarded",
        ));
    }
    let tmp = format!(
        ".hud-note-{}-{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    let fd = fs::openat(
        &dir,
        tmp.as_str(),
        OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::RUSR | Mode::WUSR,
    )?;
    let result = (|| {
        let mut file = File::from(fd);
        file.write_all(&change.proposed)?;
        fs::fchmod(
            &file,
            Mode::from_bits_truncate(change.original.stamp.mode & 0o777),
        )?;
        file.sync_all()?;
        before_commit();
        // Reopen the selected root+relative chain: a pinned descriptor alone
        // would continue writing into a directory moved away from that path.
        let (selected_dir, selected_name, selected_chain) = parent(root, change.original.path())?;
        if selected_chain != chain
            || selected_chain != change.original.chain_identity
            || selected_name != name
        {
            return Err(io::Error::other(
                "selected note directory chain changed; approval discarded",
            ));
        }
        let current = read_at(
            &selected_dir,
            change.original.path(),
            &selected_name,
            &selected_chain,
        )?;
        if current.stamp != change.original.stamp
            || current.bytes != change.original.bytes
            || current.chain_identity != change.original.chain_identity
        {
            return Err(io::Error::other(
                "note changed before commit; approval discarded",
            ));
        }
        fs::renameat(&dir, tmp.as_str(), &dir, name.as_str())?;
        fs::fsync(&dir)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::unlinkat(&dir, tmp.as_str(), AtFlags::empty());
    }
    result
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{
        fs as host,
        os::unix::fs::{symlink, PermissionsExt},
    };
    static ID: AtomicU64 = AtomicU64::new(0);
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "hud-note-actions-{}-{}",
                std::process::id(),
                ID.fetch_add(1, Ordering::Relaxed)
            ));
            host::create_dir(&p).unwrap();
            host::set_permissions(&p, host::Permissions::from_mode(0o700)).unwrap();
            Self(p)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = host::remove_dir_all(&self.0);
        }
    }
    fn write(path: &Path, bytes: &[u8]) {
        host::write(path, bytes).unwrap();
        host::set_permissions(path, host::Permissions::from_mode(0o600)).unwrap();
    }
    #[test]
    fn preview_then_toggle_preserves_crlf_unicode_manual_prose_and_aliases() {
        let t = Temp::new();
        let path = t.0.join("ação.md");
        let original =
            "# Manual\r\n  - [ ] café 日本語 [[Link\\|alias]]\r\n- [X] other\r\n".as_bytes();
        write(&path, original);
        let store = NoteStore::new(&t.0).unwrap();
        let change = store.preview_toggle("ação.md", 2, true).unwrap();
        assert_eq!(host::read(&path).unwrap(), original);
        store.apply_toggle(&change).unwrap();
        assert_eq!(
            host::read(&path).unwrap(),
            "# Manual\r\n  - [x] café 日本語 [[Link\\|alias]]\r\n- [X] other\r\n".as_bytes()
        );
        assert!(
            store.apply_toggle(&change).is_err(),
            "consumed/stale preview cannot apply twice"
        );
        assert_eq!(
            host::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    #[test]
    fn manual_edit_or_replacement_after_preview_is_preserved() {
        let t = Temp::new();
        let path = t.0.join("a.md");
        write(&path, b"- [ ] before\n");
        let store = NoteStore::new(&t.0).unwrap();
        let change = store.preview_toggle("a.md", 1, true).unwrap();
        write(&path, b"- [ ] manual\n");
        assert!(store.apply_toggle(&change).is_err());
        assert_eq!(host::read(&path).unwrap(), b"- [ ] manual\n");
        let change = store.preview_toggle("a.md", 1, true).unwrap();
        host::rename(&path, t.0.join("old.md")).unwrap();
        write(&path, b"- [ ] manual\n");
        assert!(store.apply_toggle(&change).is_err());
        assert_eq!(
            host::read_dir(&t.0).unwrap().count(),
            2,
            "no abandoned temporaries"
        );
    }
    #[test]
    fn note_links_fifo_traversal_invalid_lines_and_nonprivate_write_parent_are_refused() {
        let t = Temp::new();
        write(&t.0.join("a.md"), b"not task\n- [ ] valid\n");
        let store = NoteStore::new(&t.0).unwrap();
        symlink("a.md", t.0.join("link.md")).unwrap();
        for rel in ["link.md", "../a.md", "/tmp/a.md", ".secret.md"] {
            assert!(store.preview_read(rel).is_err());
        }
        fs::mkfifoat(
            rustix::fs::CWD,
            t.0.join("fifo.md"),
            Mode::RUSR | Mode::WUSR,
        )
        .unwrap();
        assert!(store.preview_read("fifo.md").is_err());
        for line in [0, 1, 3] {
            assert!(store.preview_toggle("a.md", line, true).is_err());
        }
        let change = store.preview_toggle("a.md", 2, true).unwrap();
        host::set_permissions(&t.0, host::Permissions::from_mode(0o775)).unwrap();
        assert!(store.apply_toggle(&change).is_err());
        assert_eq!(
            host::read(t.0.join("a.md")).unwrap(),
            b"not task\n- [ ] valid\n"
        );
    }
    #[test]
    fn renamed_parent_with_same_note_hardlink_discards_approval() {
        let t = Temp::new();
        host::create_dir(t.0.join("sub")).unwrap();
        host::set_permissions(t.0.join("sub"), host::Permissions::from_mode(0o700)).unwrap();
        write(&t.0.join("sub/a.md"), b"- [ ] same\n");
        let store = NoteStore::new(&t.0).unwrap();
        let change = store.preview_toggle("sub/a.md", 1, true).unwrap();
        host::rename(t.0.join("sub"), t.0.join("old")).unwrap();
        host::create_dir(t.0.join("sub")).unwrap();
        host::set_permissions(t.0.join("sub"), host::Permissions::from_mode(0o700)).unwrap();
        host::hard_link(t.0.join("old/a.md"), t.0.join("sub/a.md")).unwrap();
        assert!(store.apply_toggle(&change).is_err());
        assert_eq!(host::read(t.0.join("old/a.md")).unwrap(), b"- [ ] same\n");
    }
    #[test]
    fn invalid_utf8_oversize_and_terminal_control_display() {
        let t = Temp::new();
        write(&t.0.join("a.md"), b"- [ ] \xff\n");
        let store = NoteStore::new(&t.0).unwrap();
        assert!(store.preview_toggle("a.md", 1, true).is_err());
        write(&t.0.join("a.md"), b"- [ ] \x1b]52;c;secret\x07 safe\n");
        assert!(!store
            .preview_read("a.md")
            .unwrap()
            .display()
            .contains("secret"));
        write(&t.0.join("a.md"), &vec![b'a'; MAX_NOTE + 1]);
        assert!(store.preview_read("a.md").is_err());
    }
    #[test]
    fn manual_edit_during_temp_write_is_preserved_and_temp_is_cleaned() {
        let t = Temp::new();
        let path = t.0.join("a.md");
        write(&path, b"- [ ] before\n");
        let store = NoteStore::new(&t.0).unwrap();
        let change = store.preview_toggle("a.md", 1, true).unwrap();
        let edited = path.clone();
        BEFORE_COMMIT.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(move || {
                write(&edited, b"- [ ] concurrent manual\n")
            }))
        });
        assert!(store.apply_toggle(&change).is_err());
        assert_eq!(host::read(&path).unwrap(), b"- [ ] concurrent manual\n");
        assert_eq!(host::read_dir(&t.0).unwrap().count(), 1);
    }
    #[test]
    fn directory_renamed_during_commit_never_changes_moved_or_replacement_note() {
        let t = Temp::new();
        host::create_dir(t.0.join("sub")).unwrap();
        host::set_permissions(t.0.join("sub"), host::Permissions::from_mode(0o700)).unwrap();
        write(&t.0.join("sub/a.md"), b"- [ ] original\n");
        let store = NoteStore::new(&t.0).unwrap();
        let change = store.preview_toggle("sub/a.md", 1, true).unwrap();
        let root = t.0.clone();
        BEFORE_COMMIT.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(move || {
                host::rename(root.join("sub"), root.join("old")).unwrap();
                host::create_dir(root.join("sub")).unwrap();
                host::set_permissions(root.join("sub"), host::Permissions::from_mode(0o700))
                    .unwrap();
                write(&root.join("sub/a.md"), b"- [ ] replacement\n");
            }))
        });
        assert!(store.apply_toggle(&change).is_err());
        assert_eq!(
            host::read(t.0.join("old/a.md")).unwrap(),
            b"- [ ] original\n"
        );
        assert_eq!(
            host::read(t.0.join("sub/a.md")).unwrap(),
            b"- [ ] replacement\n"
        );
        assert_eq!(host::read_dir(t.0.join("old")).unwrap().count(), 1);
    }
}
