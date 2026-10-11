//! Explicit review/approval boundary. Never creates directories or reads personal defaults.
pub use hud_resources::docs::Proposal;
use std::{
    io,
    path::{Path, PathBuf},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Approval {
    CreateNew,
    OverwriteExisting,
}
#[derive(Clone, Debug)]
pub struct Preview {
    proposal: Proposal,
    existing: Option<Vec<u8>>,
    root: PathBuf,
    chain: Vec<(u64, u64)>,
    identity: Option<(u64, u64, i64, i64)>,
}
impl Preview {
    pub fn proposal(&self) -> &Proposal {
        &self.proposal
    }
    pub fn same_snapshot(&self, other: &Self) -> bool {
        self.root == other.root
            && self.chain == other.chain
            && self.identity == other.identity
            && self.existing == other.existing
            && self.proposal.path == other.proposal.path
    }
    pub fn existing(&self) -> Option<&[u8]> {
        self.existing.as_deref()
    }
}
pub trait DocumentStore {
    fn preview(&self, proposal: Proposal) -> io::Result<Preview>;
    fn apply(&self, preview: &Preview, approval: Approval) -> io::Result<()>;
}
pub struct FileDocumentStore {
    root: PathBuf,
}
impl FileDocumentStore {
    pub fn new(root: impl AsRef<Path>) -> io::Result<Self> {
        let root = root.as_ref();
        if root.as_os_str().is_empty()
            || root
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let root = if root.is_absolute() {
            root.to_owned()
        } else {
            std::env::current_dir()?.join(root)
        };
        if root.parent().is_none() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        Ok(Self { root })
    }
}
#[cfg(unix)]
mod posix {
    use super::*;
    use rustix::fs::{self, AtFlags, FileType, Mode, OFlags};
    use std::{
        fs::File,
        io::{Read, Write},
        os::fd::OwnedFd,
        sync::atomic::{AtomicU64, Ordering},
    };
    static SEQ: AtomicU64 = AtomicU64::new(0);
    const MAX: usize = 256 * 1024;
    type Parent = (OwnedFd, String, Vec<(u64, u64)>);
    type Snapshot = (Option<Vec<u8>>, Option<(u64, u64, i64, i64)>);
    fn flags() -> OFlags {
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC
    }
    fn denied() -> io::Error {
        io::Error::new(
            io::ErrorKind::PermissionDenied,
            "unsafe document path or changed preview",
        )
    }
    impl FileDocumentStore {
        fn parent(&self, path: &str) -> io::Result<Parent> {
            if !hud_resources::docs::valid_note_path(path) {
                return Err(io::ErrorKind::InvalidInput.into());
            }
            let mut fd = fs::open("/", flags(), Mode::empty())?;
            let mut chain = Vec::new();
            let rootparts: Vec<_> = self
                .root
                .components()
                .filter_map(|c| {
                    if let std::path::Component::Normal(n) = c {
                        Some(n)
                    } else {
                        None
                    }
                })
                .collect();
            let parts: Vec<_> = path.split('/').collect();
            for (name, owned) in rootparts
                .iter()
                .enumerate()
                .map(|(i, n)| (*n, i + 1 == rootparts.len()))
                .chain(
                    parts[..parts.len() - 1]
                        .iter()
                        .map(|n| (std::ffi::OsStr::new(n), true)),
                )
            {
                fd = fs::openat(&fd, name, flags(), Mode::empty())?;
                let st = fs::fstat(&fd)?;
                if st.st_mode & 0o022 != 0 && st.st_mode & 0o1000 == 0 {
                    return Err(denied());
                }
                if owned
                    && (st.st_uid != rustix::process::getuid().as_raw() || st.st_mode & 0o022 != 0)
                {
                    return Err(denied());
                }
                chain.push((st.st_dev, st.st_ino));
            }
            let st = fs::fstat(&fd)?;
            if st.st_uid != rustix::process::getuid().as_raw() || st.st_mode & 0o022 != 0 {
                return Err(denied());
            }
            Ok((fd, parts.last().unwrap().to_string(), chain))
        }
        fn read(dir: &OwnedFd, name: &str) -> io::Result<Snapshot> {
            let fd = match fs::openat(
                dir,
                name,
                OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
                Mode::empty(),
            ) {
                Ok(fd) => fd,
                Err(rustix::io::Errno::NOENT) => return Ok((None, None)),
                Err(e) => return Err(e.into()),
            };
            let st = fs::fstat(&fd)?;
            if FileType::from_raw_mode(st.st_mode) != FileType::RegularFile
                || st.st_uid != rustix::process::getuid().as_raw()
                || st.st_mode & 0o022 != 0
                || st.st_size > MAX as i64
            {
                return Err(denied());
            }
            let identity = (st.st_dev, st.st_ino, st.st_mtime, st.st_ctime);
            let mut data = Vec::new();
            File::from(fd)
                .take((MAX + 1) as u64)
                .read_to_end(&mut data)?;
            if data.len() > MAX {
                return Err(io::ErrorKind::InvalidData.into());
            }
            Ok((Some(data), Some(identity)))
        }
    }
    impl DocumentStore for FileDocumentStore {
        fn preview(&self, proposal: Proposal) -> io::Result<Preview> {
            if proposal.content.len() > MAX {
                return Err(io::ErrorKind::InvalidInput.into());
            }
            let (dir, name, chain) = self.parent(&proposal.path)?;
            let (existing, identity) = Self::read(&dir, &name)?;
            Ok(Preview {
                proposal,
                existing,
                root: self.root.clone(),
                chain,
                identity,
            })
        }
        fn apply(&self, preview: &Preview, approval: Approval) -> io::Result<()> {
            if preview.root != self.root || preview.proposal.content.len() > MAX {
                return Err(denied());
            }
            if (preview.existing.is_some()) != (approval == Approval::OverwriteExisting) {
                return Err(denied());
            }
            let (dir, name, chain) = self.parent(&preview.proposal.path)?;
            if chain != preview.chain {
                return Err(denied());
            }
            let current = Self::read(&dir, &name)?;
            if current != (preview.existing.clone(), preview.identity) {
                return Err(denied());
            }
            let temporary = format!(
                ".hud-doc-{}-{}.tmp",
                std::process::id(),
                SEQ.fetch_add(1, Ordering::Relaxed)
            );
            let fd = fs::openat(
                &dir,
                temporary.as_str(),
                OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::from_raw_mode(0o600),
            )?;
            let result = (|| {
                let mut file = File::from(fd);
                file.write_all(preview.proposal.content.as_bytes())?;
                file.sync_all()?;
                if Self::read(&dir, &name)? != (preview.existing.clone(), preview.identity) {
                    return Err(denied());
                }
                // Reopen the complete chain immediately before commit, detecting moves since preview.
                if self.parent(&preview.proposal.path)?.2 != preview.chain {
                    return Err(denied());
                }
                if approval == Approval::CreateNew {
                    #[cfg(target_os = "linux")]
                    fs::renameat_with(
                        &dir,
                        temporary.as_str(),
                        &dir,
                        name.as_str(),
                        fs::RenameFlags::NOREPLACE,
                    )?;
                    #[cfg(not(target_os = "linux"))]
                    {
                        return Err(io::Error::new(
                            io::ErrorKind::Unsupported,
                            "exclusive document creation requires Linux",
                        ));
                    }
                } else {
                    fs::renameat(&dir, temporary.as_str(), &dir, name.as_str())?;
                }
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
impl DocumentStore for FileDocumentStore {
    fn preview(&self, _: Proposal) -> io::Result<Preview> {
        Err(io::ErrorKind::Unsupported.into())
    }
    fn apply(&self, _: &Preview, _: Approval) -> io::Result<()> {
        Err(io::ErrorKind::Unsupported.into())
    }
}
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{
        fs,
        os::unix::fs::{symlink, PermissionsExt},
        sync::atomic::{AtomicU64, Ordering},
    };
    fn write(path: impl AsRef<Path>, bytes: impl AsRef<[u8]>) -> io::Result<()> {
        fs::write(&path, bytes)?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
    }
    fn mkdir(path: impl AsRef<Path>) -> io::Result<()> {
        fs::create_dir(&path)?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
    }
    static SEQ: AtomicU64 = AtomicU64::new(0);
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "hud-doc-test-{}-{}",
                std::process::id(),
                SEQ.fetch_add(1, Ordering::Relaxed)
            ));
            mkdir(&p).unwrap();
            fs::set_permissions(&p, fs::Permissions::from_mode(0o700)).unwrap();
            Self(p)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn proposal(path: &str) -> Proposal {
        Proposal {
            path: path.into(),
            content: "ação 日本語\r\n[[Nota\\|alias]]\n\n".into(),
        }
    }
    #[test]
    fn preview_no_write_and_create_exact() {
        let t = Temp::new();
        let s = FileDocumentStore::new(&t.0).unwrap();
        let p = s.preview(proposal("Nota.md")).unwrap();
        assert_eq!(fs::read_dir(&t.0).unwrap().count(), 0);
        s.apply(&p, Approval::CreateNew).unwrap();
        assert_eq!(
            fs::read(t.0.join("Nota.md")).unwrap(),
            p.proposal().content.as_bytes()
        );
        assert_eq!(
            fs::metadata(t.0.join("Nota.md"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert!(s.apply(&p, Approval::CreateNew).is_err());
    }
    #[test]
    fn approval_kind_is_explicit() {
        let t = Temp::new();
        let s = FileDocumentStore::new(&t.0).unwrap();
        let p = s.preview(proposal("n.md")).unwrap();
        assert!(s.apply(&p, Approval::OverwriteExisting).is_err());
        write(t.0.join("n.md"), "manual\n").unwrap();
        let p = s.preview(proposal("n.md")).unwrap();
        assert!(s.apply(&p, Approval::CreateNew).is_err());
        s.apply(&p, Approval::OverwriteExisting).unwrap();
    }
    #[test]
    fn manual_edit_and_inode_replacement_refused() {
        let t = Temp::new();
        let s = FileDocumentStore::new(&t.0).unwrap();
        let n = t.0.join("n.md");
        write(&n, "original").unwrap();
        let p = s.preview(proposal("n.md")).unwrap();
        write(&n, "manual").unwrap();
        assert!(s.apply(&p, Approval::OverwriteExisting).is_err());
        assert_eq!(fs::read_to_string(&n).unwrap(), "manual");
        let p = s.preview(proposal("n.md")).unwrap();
        fs::rename(&n, t.0.join("old.md")).unwrap();
        write(&n, "manual").unwrap();
        assert!(s.apply(&p, Approval::OverwriteExisting).is_err());
    }
    #[test]
    fn symlink_before_or_after_preview_refused() {
        let t = Temp::new();
        let s = FileDocumentStore::new(&t.0).unwrap();
        write(t.0.join("victim.md"), "untouched").unwrap();
        let p = s.preview(proposal("n.md")).unwrap();
        symlink(t.0.join("victim.md"), t.0.join("n.md")).unwrap();
        assert!(s.apply(&p, Approval::CreateNew).is_err());
        assert!(s.preview(proposal("n.md")).is_err());
        assert_eq!(
            fs::read_to_string(t.0.join("victim.md")).unwrap(),
            "untouched"
        );
    }
    #[test]
    fn moved_directory_and_symlink_parent_refused() {
        let t = Temp::new();
        let sub = t.0.join("sub");
        mkdir(&sub).unwrap();
        let s = FileDocumentStore::new(&t.0).unwrap();
        let p = s.preview(proposal("sub/n.md")).unwrap();
        fs::rename(&sub, t.0.join("moved")).unwrap();
        mkdir(&sub).unwrap();
        assert!(s.apply(&p, Approval::CreateNew).is_err());
        assert!(!t.0.join("moved/n.md").exists());
        fs::remove_dir(&sub).unwrap();
        symlink(t.0.join("moved"), &sub).unwrap();
        assert!(s.preview(proposal("sub/n.md")).is_err());
    }
    #[test]
    fn unsafe_paths_and_missing_parent_have_no_mutation() {
        let t = Temp::new();
        let s = FileDocumentStore::new(&t.0).unwrap();
        for path in ["../n.md", "/n.md", "CON.md", "sub/n.md", "a.txt"] {
            assert!(s.preview(proposal(path)).is_err());
        }
        assert_eq!(fs::read_dir(&t.0).unwrap().count(), 0);
        let mut p = proposal("n.md");
        p.content = "x".repeat(256 * 1024 + 1);
        assert!(s.preview(p).is_err());
    }
    #[test]
    fn world_writable_and_nonregular_refused() {
        let t = Temp::new();
        let s = FileDocumentStore::new(&t.0).unwrap();
        let n = t.0.join("n.md");
        write(&n, "manual").unwrap();
        fs::set_permissions(&n, fs::Permissions::from_mode(0o666)).unwrap();
        assert!(s.preview(proposal("n.md")).is_err());
        fs::remove_file(&n).unwrap();
        mkdir(&n).unwrap();
        assert!(s.preview(proposal("n.md")).is_err());
    }
    #[test]
    fn changed_root_and_old_open_reader() {
        use std::io::Read;
        let t = Temp::new();
        let root = t.0.join("root");
        mkdir(&root).unwrap();
        let s = FileDocumentStore::new(&root).unwrap();
        write(root.join("n.md"), "original").unwrap();
        let p = s.preview(proposal("n.md")).unwrap();
        let mut old = fs::File::open(root.join("n.md")).unwrap();
        s.apply(&p, Approval::OverwriteExisting).unwrap();
        let mut text = String::new();
        old.read_to_string(&mut text).unwrap();
        assert_eq!(text, "original");
        let p = s.preview(proposal("n.md")).unwrap();
        fs::rename(&root, t.0.join("moved")).unwrap();
        mkdir(&root).unwrap();
        assert!(s.apply(&p, Approval::OverwriteExisting).is_err());
    }
}
