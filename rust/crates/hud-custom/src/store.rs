//! Custom JSON persistence with explicit snapshot comparison and atomic replacement.
//! Private directory must already exist; no configuration discovery or migration.
use crate::Custom;

#[derive(Clone, Debug)]
struct Proposal {
    pub path: String,
    pub content: String,
}
use std::{
    io,
    path::{Path, PathBuf},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Approval {
    CreateNew,
    OverwriteExisting,
}
#[derive(Clone, Debug)]
struct Preview {
    proposal: Proposal,
    existing: Option<Vec<u8>>,
    root: PathBuf,
    chain: Vec<(u64, u64)>,
    identity: Option<(u64, u64, i64, i64)>,
}

trait DocumentStore {
    fn preview(&self, proposal: Proposal) -> io::Result<Preview>;
    fn apply(&self, preview: &Preview, approval: Approval) -> io::Result<()>;
}
struct FileDocumentStore {
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
    const MAX: usize = crate::MAX_CONFIG;
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
            if !matches!(path, "custom.json" | "selection.json") {
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
                    && (st.st_uid != rustix::process::getuid().as_raw() || st.st_mode & 0o077 != 0)
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
                || st.st_mode & 0o077 != 0
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

#[derive(Clone, Debug)]
pub struct CustomSnapshot {
    pub custom: Option<Custom>,
    preview: Preview,
}
pub trait CustomStore {
    fn load(&self) -> io::Result<CustomSnapshot>;
    fn save(&self, custom: &Custom, expected: &CustomSnapshot) -> io::Result<CustomSnapshot>;
}
pub struct FileCustomStore {
    inner: FileDocumentStore,
}
impl FileCustomStore {
    /// Select an existing private directory; file is always custom.json.
    pub fn new(directory: impl AsRef<Path>) -> io::Result<Self> {
        Ok(Self {
            inner: FileDocumentStore::new(directory)?,
        })
    }
}
impl CustomStore for FileCustomStore {
    fn load(&self) -> io::Result<CustomSnapshot> {
        let preview = self.inner.preview(Proposal {
            path: "custom.json".into(),
            content: String::new(),
        })?;
        let custom = preview.existing.as_deref().map(Custom::parse).transpose()?;
        Ok(CustomSnapshot { custom, preview })
    }
    fn save(&self, custom: &Custom, expected: &CustomSnapshot) -> io::Result<CustomSnapshot> {
        let content =
            String::from_utf8(custom.to_json()?).map_err(|_| io::ErrorKind::InvalidData)?;
        let mut preview = expected.preview.clone();
        preview.proposal.content = content;
        let approval = if preview.existing.is_some() {
            Approval::OverwriteExisting
        } else {
            Approval::CreateNew
        };
        self.inner.apply(&preview, approval)?;
        self.load()
    }
}

#[derive(Clone, Debug)]
pub struct RememberedSelection {
    pub source: PathBuf,
    pub digest: String,
    pub trusted: bool,
}
#[derive(Clone, Debug)]
pub struct ResolvedSelection {
    pub selection: RememberedSelection,
    pub custom: Custom,
    pub changed: bool,
}
pub trait SelectionStore {
    fn load(&self) -> io::Result<Option<RememberedSelection>>;
    fn resolve(&self) -> io::Result<Option<ResolvedSelection>>;
    fn remember(&self, source: &Path, trusted: bool) -> io::Result<ResolvedSelection>;
    fn remember_reviewed(&self, source: &Path, digest: &str) -> io::Result<ResolvedSelection>;
}
pub struct FileSelectionStore {
    inner: FileDocumentStore,
}
impl FileSelectionStore {
    pub fn new(directory: impl AsRef<Path>) -> io::Result<Self> {
        Ok(Self {
            inner: FileDocumentStore::new(directory)?,
        })
    }
    /// Capture exact selection bytes for reviewed rollback; None represents default.
    pub fn checkpoint(&self) -> io::Result<Option<Vec<u8>>> {
        Ok(self
            .inner
            .preview(Proposal {
                path: "selection.json".into(),
                content: String::new(),
            })?
            .existing)
    }
    pub fn restore_checkpoint(
        &self,
        expected: &Option<Vec<u8>>,
        restored: &Option<Vec<u8>>,
    ) -> io::Result<()> {
        let content = match restored {
            Some(b) => String::from_utf8(b.clone()).map_err(|_| crate::bad())?,
            None => "null".into(),
        };
        let preview = self.inner.preview(Proposal {
            path: "selection.json".into(),
            content,
        })?;
        if &preview.existing != expected {
            return Err(io::Error::other(
                "Selection changed; reviewed rollback/default refused",
            ));
        }
        self.inner.apply(
            &preview,
            if preview.existing.is_some() {
                Approval::OverwriteExisting
            } else {
                Approval::CreateNew
            },
        )
    }
    pub fn load(&self) -> io::Result<Option<RememberedSelection>> {
        let preview = self.inner.preview(Proposal {
            path: "selection.json".into(),
            content: String::new(),
        })?;
        let Some(bytes) = preview.existing else {
            return Ok(None);
        };
        if bytes == b"null" {
            return Ok(None);
        }
        crate::reject_duplicate_keys(&bytes)?;
        let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| crate::bad())?;
        let m = crate::obj(&value, &["version", "source", "digest", "trusted"])?;
        if m.get("version").and_then(serde_json::Value::as_u64) != Some(1) {
            return Err(crate::bad());
        }
        let source = PathBuf::from(crate::string(m, "source")?);
        validate_source(&source)?;
        let digest = crate::string(m, "digest")?.to_owned();
        if digest.len() != 64
            || !digest
                .bytes()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        {
            return Err(crate::bad());
        }
        let trusted = m
            .get("trusted")
            .and_then(serde_json::Value::as_bool)
            .ok_or_else(crate::bad)?;
        Ok(Some(RememberedSelection {
            source,
            digest,
            trusted,
        }))
    }
    /// Caller explicitly selected/reviewed this source. No provider process is launched.
    /// Read/validate once, store exact-byte digest; source mutations revoke trust at resolve.
    pub fn remember(&self, source: &Path, trusted: bool) -> io::Result<ResolvedSelection> {
        if trusted {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "trusted selection requires reviewed exact digest",
            ));
        }
        self.remember_checked(source, None)
    }
    pub fn remember_reviewed(
        &self,
        source: &Path,
        reviewed_digest: &str,
    ) -> io::Result<ResolvedSelection> {
        self.remember_checked(source, Some(reviewed_digest))
    }
    fn remember_checked(
        &self,
        source: &Path,
        reviewed_digest: Option<&str>,
    ) -> io::Result<ResolvedSelection> {
        validate_source(source)?;
        let bytes = hud_resources::read_owned_file(source, crate::MAX_CONFIG)?;
        let custom = Custom::parse_source(&bytes, source)?;
        let digest = source_digest(&bytes);
        if reviewed_digest.is_some_and(|expected| expected != digest) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "source changed after review",
            ));
        }
        let trusted = reviewed_digest.is_some();
        let content=serde_json::to_string_pretty(&serde_json::json!({"version":1,"source":source.to_str().ok_or_else(crate::bad)?,"digest":digest,"trusted":trusted})).map_err(|_|crate::bad())?;
        let preview = self.inner.preview(Proposal {
            path: "selection.json".into(),
            content,
        })?;
        // Existing selection itself must be valid before overwrite, never clobber corruption.
        self.load()?;
        self.inner.apply(
            &preview,
            if preview.existing.is_some() {
                Approval::OverwriteExisting
            } else {
                Approval::CreateNew
            },
        )?;
        Ok(ResolvedSelection {
            selection: RememberedSelection {
                source: source.to_owned(),
                digest,
                trusted,
            },
            custom,
            changed: false,
        })
    }
    pub fn resolve(&self) -> io::Result<Option<ResolvedSelection>> {
        let Some(mut selection) = self.load()? else {
            return Ok(None);
        };
        let bytes = hud_resources::read_owned_file(&selection.source, crate::MAX_CONFIG)?;
        let digest = source_digest(&bytes);
        let changed = digest != selection.digest;
        if changed {
            selection.trusted = false;
            selection.digest = digest;
        }
        // Invalid changed configuration errors before any panel content/use.
        let custom = Custom::parse_source(&bytes, &selection.source)?;
        Ok(Some(ResolvedSelection {
            selection,
            custom,
            changed,
        }))
    }
}
impl SelectionStore for FileSelectionStore {
    fn load(&self) -> io::Result<Option<RememberedSelection>> {
        FileSelectionStore::load(self)
    }
    fn resolve(&self) -> io::Result<Option<ResolvedSelection>> {
        FileSelectionStore::resolve(self)
    }
    fn remember(&self, source: &Path, trusted: bool) -> io::Result<ResolvedSelection> {
        FileSelectionStore::remember(self, source, trusted)
    }
    fn remember_reviewed(&self, source: &Path, digest: &str) -> io::Result<ResolvedSelection> {
        FileSelectionStore::remember_reviewed(self, source, digest)
    }
}
fn validate_source(source: &Path) -> io::Result<()> {
    if !source.is_absolute()
        || source
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        || source.to_str().is_none()
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    Ok(())
}
pub fn source_digest(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use hud_domain::{DropTarget, PanelId};
    use std::{
        fs,
        os::unix::fs::{symlink, PermissionsExt},
        sync::atomic::{AtomicU64, Ordering},
    };
    static SEQ: AtomicU64 = AtomicU64::new(0);
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "hud-custom-persist-{}-{}",
                std::process::id(),
                SEQ.fetch_add(1, Ordering::Relaxed)
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
    fn custom() -> Custom {
        Custom::parse(br#"{"version":1,"name":"demo","columns":[{"width":40,"slots":[{"id":"note","height":7},{"id":"agenda","weight":2}]},{"slots":[{"id":"vault","height":"auto"},{"id":"output","weight":3}]}],"panels":[{"id":"note","title":"Notes","type":"text","text":"hello","lines":2}]}"#).unwrap()
    }
    fn write(p: &Path, data: &[u8]) {
        fs::write(p, data).unwrap();
        fs::set_permissions(p, fs::Permissions::from_mode(0o600)).unwrap();
    }
    #[test]
    fn roundtrip_unicode_and_command_never_executed() {
        let t = Temp::new();
        let s = FileCustomStore::new(&t.0).unwrap();
        let initial = s.load().unwrap();
        assert!(initial.custom.is_none());
        assert_eq!(fs::read_dir(&t.0).unwrap().count(), 0);
        let mut c = custom();
        c.panels.get_mut("note").unwrap().feed =
            crate::Feed::Text("ação 日本語\n[[Note\\|alias]]".into());
        let after = s.save(&c, &initial).unwrap();
        assert_eq!(after.custom, Some(c.clone()));
        assert_eq!(s.load().unwrap().custom, Some(c.clone()));
        c.panels.get_mut("note").unwrap().feed = crate::Feed::Command(vec![
            "touch".into(),
            t.0.join("forbidden").to_str().unwrap().into(),
        ]);
        let after = s.save(&c, &after).unwrap();
        assert_eq!(
            after
                .custom
                .as_ref()
                .unwrap()
                .content("note")
                .unwrap_err()
                .kind(),
            io::ErrorKind::PermissionDenied
        );
        assert!(!t.0.join("forbidden").exists());
        assert_eq!(
            fs::metadata(t.0.join("custom.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    #[test]
    fn reorder_preserves_geometry_feeds_and_titles() {
        let c = custom();
        let next = c
            .layout()
            .unwrap()
            .apply_drop(
                &PanelId::new("note").unwrap(),
                &DropTarget::Swap(PanelId::new("output").unwrap()),
            )
            .unwrap();
        let reordered = c.reordered(&next).unwrap();
        assert_eq!(reordered.columns[0].width, Some(40.));
        assert_eq!(
            reordered.columns[1].slots[1].height,
            crate::Height::Fixed(7)
        );
        assert_eq!(reordered.panels, c.panels);
        assert_eq!(
            Custom::parse(&reordered.to_json().unwrap()).unwrap(),
            reordered
        );
        let newcol = c
            .layout()
            .unwrap()
            .apply_drop(
                &PanelId::new("note").unwrap(),
                &DropTarget::Move {
                    column: 2,
                    index: 0,
                },
            )
            .unwrap();
        assert_eq!(c.reordered(&newcol).unwrap().columns.len(), 3);
    }
    #[test]
    fn stale_manual_corrupt_replacement_not_overwritten() {
        let t = Temp::new();
        let s = FileCustomStore::new(&t.0).unwrap();
        let snap = s.save(&custom(), &s.load().unwrap()).unwrap();
        let p = t.0.join("custom.json");
        write(&p, b"manual corrupted");
        assert!(s.save(&custom(), &snap).is_err());
        assert_eq!(fs::read(&p).unwrap(), b"manual corrupted");
        assert!(s.load().is_err());
        write(&p, &custom().to_json().unwrap());
        let snap = s.load().unwrap();
        fs::rename(&p, t.0.join("old.json")).unwrap();
        write(&p, &custom().to_json().unwrap());
        assert!(s.save(&custom(), &snap).is_err());
    }
    #[test]
    fn symlink_and_moved_directory_refused() {
        let t = Temp::new();
        let root = t.0.join("root");
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let s = FileCustomStore::new(&root).unwrap();
        let snap = s.load().unwrap();
        let victim = t.0.join("victim");
        write(&victim, b"untouched");
        symlink(&victim, root.join("custom.json")).unwrap();
        assert!(s.load().is_err());
        assert!(s.save(&custom(), &snap).is_err());
        assert_eq!(fs::read(&victim).unwrap(), b"untouched");
        fs::remove_file(root.join("custom.json")).unwrap();
        let snap = s.load().unwrap();
        fs::rename(&root, t.0.join("moved")).unwrap();
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(s.save(&custom(), &snap).is_err());
        assert!(!t.0.join("moved/custom.json").exists());
    }
    #[test]
    fn old_open_reader_atomic_and_stale_snapshot_rejected() {
        use std::io::Read;
        let t = Temp::new();
        let s = FileCustomStore::new(&t.0).unwrap();
        let c = custom();
        let snap = s.save(&c, &s.load().unwrap()).unwrap();
        let mut old = fs::File::open(t.0.join("custom.json")).unwrap();
        let mut next = c.clone();
        next.name = "next".into();
        let after = s.save(&next, &snap).unwrap();
        let mut bytes = Vec::new();
        old.read_to_end(&mut bytes).unwrap();
        assert_eq!(Custom::parse(&bytes).unwrap(), c);
        assert_eq!(after.custom, Some(next));
        assert!(s.save(&c, &snap).is_err());
    }
    #[test]
    fn bounds_permissions_and_missing_directory_no_mutation() {
        let t = Temp::new();
        let p = t.0.join("custom.json");
        write(&p, &vec![b'x'; crate::MAX_CONFIG + 1]);
        let s = FileCustomStore::new(&t.0).unwrap();
        assert!(s.load().is_err());
        write(&p, &custom().to_json().unwrap());
        fs::set_permissions(&p, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(s.load().is_err());
        fs::remove_file(&p).unwrap();
        fs::set_permissions(&t.0, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(s.load().is_err());
        let missing = t.0.join("missing");
        assert!(FileCustomStore::new(&missing).unwrap().load().is_err());
        assert!(!missing.exists());
    }
}
#[cfg(all(test, unix))]
mod selection_tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt};
    #[test]
    fn remembered_source_mutation_revokes_trust() {
        let root = std::env::temp_dir().join(format!("hud-selection-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let source = root.join("config.json");
        let content =
            br#"{"version":1,"name":"demo","columns":[{"slots":[{"id":"output"}]}],"panels":[]}"#;
        fs::write(&source, content).unwrap();
        fs::set_permissions(&source, fs::Permissions::from_mode(0o600)).unwrap();
        let store = FileSelectionStore::new(&root).unwrap();
        assert!(store.load().unwrap().is_none());
        store
            .remember_reviewed(&source, &source_digest(content))
            .unwrap();
        assert!(store.resolve().unwrap().unwrap().selection.trusted);
        let mut bytes = content.to_vec();
        bytes.push(b'\n');
        fs::write(&source, &bytes).unwrap();
        let resolved = store.resolve().unwrap().unwrap();
        assert!(resolved.changed);
        assert!(!resolved.selection.trusted);
        assert_eq!(resolved.custom.name, "demo");
        fs::write(&source, b"broken").unwrap();
        assert!(store.resolve().is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn sha256_known_answer() {
        assert_eq!(
            source_digest(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
