//! Explicit Rust-only state directory. No Python configuration is touched.
use hud_domain::Layout;
use std::{
    io,
    path::{Path, PathBuf},
};
pub trait LayoutStore {
    fn load(&self) -> io::Result<Option<Layout>>;
    fn save(&self, layout: &Layout) -> io::Result<()>;
}
pub struct FileLayoutStore {
    directory: PathBuf,
    max_columns: usize,
}
impl FileLayoutStore {
    pub fn new(directory: impl AsRef<Path>, max_columns: usize) -> io::Result<Self> {
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
            max_columns,
        })
    }
}
#[cfg(unix)]
mod unix {
    use super::*;
    use rustix::fs::{self, AtFlags, FileType, Mode, OFlags};
    use std::{
        fs::File,
        io::{Read, Write},
        os::fd::OwnedFd,
        path::Component,
        sync::atomic::{AtomicU64, Ordering},
    };
    const NAME: &str = "layout.state";
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
        {
            return Err(invalid(
                "state must be owned by current user, private, and of expected type",
            ));
        }
        Ok(())
    }
    impl FileLayoutStore {
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
    impl LayoutStore for FileLayoutStore {
        fn load(&self) -> io::Result<Option<Layout>> {
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
            if fs::fstat(&fd)?.st_size > hud_layout_codec::MAX_BYTES as i64 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "layout exceeds size cap",
                ));
            }
            let mut data = Vec::new();
            File::from(fd)
                .take(hud_layout_codec::MAX_BYTES as u64 + 1)
                .read_to_end(&mut data)?;
            if data.len() > hud_layout_codec::MAX_BYTES {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "layout grew beyond size cap",
                ));
            }
            let text = std::str::from_utf8(&data)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            hud_layout_codec::decode(text, self.max_columns)
                .map(Some)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
        }
        fn save(&self, layout: &Layout) -> io::Result<()> {
            let text = hud_layout_codec::encode(layout)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
            // Verify configured limits before any filesystem mutation.
            hud_layout_codec::decode(&text, self.max_columns)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
            let dir = self.directory(true)?;
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
                ".layout-{}-{}.tmp",
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
}
#[cfg(not(unix))]
impl LayoutStore for FileLayoutStore {
    fn load(&self) -> io::Result<Option<Layout>> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "secure state storage is currently POSIX-only",
        ))
    }
    fn save(&self, _: &Layout) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "secure state storage is currently POSIX-only",
        ))
    }
}
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use hud_domain::PanelId;
    use std::{
        fs,
        os::unix::fs::{symlink, PermissionsExt},
        sync::atomic::{AtomicU64, Ordering},
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "hud-store-test-{}-{}",
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
    fn layout() -> Layout {
        Layout::new(
            vec![
                vec![PanelId::new("system").unwrap()],
                vec![PanelId::new("output").unwrap()],
            ],
            4,
        )
        .unwrap()
    }
    #[test]
    fn save_reload_private_and_missing_load_no_mutation() {
        let t = Temp::new();
        let p = t.0.join("state");
        let s = FileLayoutStore::new(&p, 4).unwrap();
        assert!(s.load().unwrap().is_none());
        assert!(!p.exists());
        s.save(&layout()).unwrap();
        assert_eq!(s.load().unwrap(), Some(layout()));
        assert_eq!(
            fs::metadata(&p).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(p.join("layout.state"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(fs::read_dir(&p).unwrap().count(), 1);
    }
    #[test]
    fn symlink_parent_and_state_directory_refused() {
        let t = Temp::new();
        let real = t.0.join("real");
        fs::create_dir(&real).unwrap();
        fs::set_permissions(&real, fs::Permissions::from_mode(0o700)).unwrap();
        let link = t.0.join("link");
        symlink(&real, &link).unwrap();
        let s = FileLayoutStore::new(link.join("state"), 4).unwrap();
        assert!(s.save(&layout()).is_err());
        assert!(!real.join("state").exists());
        let s = FileLayoutStore::new(&link, 4).unwrap();
        assert!(s.load().is_err());
    }
    #[test]
    fn final_symlink_refused_without_target_mutation() {
        let t = Temp::new();
        let s = FileLayoutStore::new(&t.0, 4).unwrap();
        let victim = t.0.join("victim");
        fs::write(&victim, "unchanged").unwrap();
        symlink(&victim, t.0.join("layout.state")).unwrap();
        assert!(s.load().is_err());
        assert!(s.save(&layout()).is_err());
        assert_eq!(fs::read_to_string(victim).unwrap(), "unchanged");
    }
    #[test]
    fn corrupt_oversized_and_public_files_refused() {
        let t = Temp::new();
        let s = FileLayoutStore::new(&t.0, 4).unwrap();
        let p = t.0.join("layout.state");
        fs::write(&p, "broken").unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(s.load().unwrap_err().kind(), io::ErrorKind::InvalidData);
        fs::write(&p, vec![b'x'; hud_layout_codec::MAX_BYTES + 1]).unwrap();
        assert_eq!(s.load().unwrap_err().kind(), io::ErrorKind::InvalidData);
        fs::set_permissions(&p, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(s.load().is_err());
        assert!(s.save(&layout()).is_err());
    }
    #[test]
    fn invalid_limit_does_not_create_state_and_parent_traversal_rejected() {
        let t = Temp::new();
        let p = t.0.join("state");
        assert!(FileLayoutStore::new(&p, 1)
            .unwrap()
            .save(&layout())
            .is_err());
        assert!(!p.exists());
        assert!(FileLayoutStore::new(t.0.join("../escape"), 4).is_err());
    }
    #[test]
    fn replacement_is_atomic_to_old_open_reader() {
        let t = Temp::new();
        let s = FileLayoutStore::new(&t.0, 4).unwrap();
        s.save(&layout()).unwrap();
        let old = fs::File::open(t.0.join("layout.state")).unwrap();
        let new = Layout::new(vec![vec![PanelId::new("changed").unwrap()]], 4).unwrap();
        s.save(&new).unwrap();
        use std::io::Read;
        let mut oldtext = String::new();
        let mut old = old;
        old.read_to_string(&mut oldtext).unwrap();
        assert_eq!(hud_layout_codec::decode(&oldtext, 4).unwrap(), layout());
        assert_eq!(s.load().unwrap(), Some(new));
    }
}
